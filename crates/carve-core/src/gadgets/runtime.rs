//! The sandboxed Lua state a gadget runs in.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{
    Error as LuaError, Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt, StdLib, Table,
    Value as LuaValue, VmState,
};
use serde_json::Value;

use crate::kernel::{Kernel, KernelReply};

/// The library every gadget sees, built on `carve.call`.
const PRELUDE: &str = include_str!("prelude.lua");

/// How often the instruction hook fires.
const HOOK_EVERY: u32 = 10_000;

/// Resource caps for one gadget run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Lua VM instructions a run may execute before it is stopped. Kernel
    /// commands the script calls do not count.
    pub instructions: u64,
    /// Bytes the Lua heap may grow to.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            instructions: 200_000_000,
            memory: 256 * 1024 * 1024,
        }
    }
}

/// Commands a gadget may not run: ones that replace the document, move the
/// undo stack, read or write files, change settings or start a gadget.
fn refused(command: &str) -> bool {
    const EXACT: [&str; 14] = [
        "doc.new",
        "doc.open",
        "doc.save",
        "undo",
        "redo",
        "undo.beginGroup",
        "undo.endGroup",
        "cam.save",
        "prefs.set",
        "prefs.reset",
        "bitmaps.load",
        "model.importMesh",
        "model.exportStl",
        "cam.loadTemplate",
    ];
    const READ_ONLY: [&str; 7] = [
        "tools.list",
        "tools.get",
        "tools.groups.list",
        "tools.materials.list",
        "tools.machines.list",
        "posts.list",
        "posts.get",
    ];
    if EXACT.contains(&command) {
        return true;
    }
    if command.starts_with("gadgets.")
        || command.starts_with("io.")
        || command.starts_with("templates.")
    {
        return true;
    }
    if command.starts_with("tools.") || command.starts_with("posts.") {
        return !READ_ONLY.contains(&command);
    }
    command.contains("saveTemplate")
}

/// A fresh state with only the safe libraries, the caps and the prelude.
fn new_state(limits: Limits) -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )?;
    lua.set_memory_limit(limits.memory)?;
    let globals = lua.globals();
    for name in ["dofile", "loadfile", "load", "require", "collectgarbage"] {
        globals.raw_set(name, LuaValue::Nil)?;
    }
    let string: Table = globals.get("string")?;
    string.raw_set("dump", LuaValue::Nil)?;

    let budget = (limits.instructions / u64::from(HOOK_EVERY)).max(1);
    let ticks = Rc::new(Cell::new(0_u64));
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_EVERY),
        move |_, _| {
            let next = ticks.get() + 1;
            ticks.set(next);
            if next > budget {
                Err(LuaError::runtime("the gadget ran too long and was stopped"))
            } else {
                Ok(VmState::Continue)
            }
        },
    )?;

    let carve = lua.create_table()?;
    let array_mt = lua.array_metatable();
    carve.raw_set(
        "array",
        lua.create_function(move |_, table: Table| {
            table.set_metatable(Some(array_mt.clone()))?;
            Ok(table)
        })?,
    )?;
    globals.raw_set("carve", carve)?;
    lua.load(PRELUDE).set_name("@prelude.lua").exec()?;
    Ok(lua)
}

/// Runs the script's top level, which declares `gadget` and `run`.
fn load_script(lua: &Lua, name: &str, script: &str) -> mlua::Result<()> {
    lua.load(script).set_name(format!("@{name}.lua")).exec()
}

/// The readable part of a Lua error: the message without the traceback.
pub(super) fn message_of(error: &LuaError) -> String {
    let full = full_message(error);
    let message = match full.find("stack traceback:") {
        Some(at) => full[..at].trim_end(),
        None => full.as_str(),
    };
    without_location(message).to_string()
}

/// A message without the `name.lua:12: ` a script's `error` puts in front,
/// which says nothing to the person running the gadget.
fn without_location(message: &str) -> &str {
    let Some((head, rest)) = message.split_once(".lua:") else {
        return message;
    };
    if head.is_empty() || head.contains(char::is_whitespace) {
        return message;
    }
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    match rest[digits..].strip_prefix(": ") {
        Some(text) if digits > 0 => text,
        _ => message,
    }
}

fn full_message(error: &LuaError) -> String {
    match error {
        LuaError::RuntimeError(message) => message.clone(),
        LuaError::SyntaxError { message, .. } => message.clone(),
        LuaError::CallbackError { cause, .. } => message_of(cause),
        LuaError::WithContext { cause, .. } => message_of(cause),
        LuaError::MemoryError(_) => "the gadget ran out of memory".to_string(),
        other => other.to_string(),
    }
}

/// Loads the script with `carve.call` switched off and answers its `gadget`
/// table as JSON (`Null` when the script declares none).
pub(super) fn read_gadget_table(name: &str, script: &str, limits: Limits) -> Result<Value, String> {
    let read = || -> mlua::Result<Value> {
        let lua = new_state(limits)?;
        let carve: Table = lua.globals().get("carve")?;
        carve.raw_set(
            "call",
            lua.create_function(|_, _: mlua::MultiValue| -> mlua::Result<()> {
                Err(LuaError::runtime(
                    "carve.call is only available inside run(values)",
                ))
            })?,
        )?;
        carve.raw_set("message", lua.create_function(|_, _: String| Ok(()))?)?;
        load_script(&lua, name, script)?;
        match lua.globals().get::<LuaValue>("gadget")? {
            LuaValue::Nil => Ok(Value::Null),
            LuaValue::Table(table) => lua.from_value(LuaValue::Table(table)),
            _ => Err(LuaError::runtime("gadget must be a table")),
        }
    };
    read().map_err(|error| message_of(&error))
}

/// JSON whole numbers as integers, so that ids and counts a script works
/// out with float arithmetic still read as integers in the kernel.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(f) if number.is_f64() && f.fract() == 0.0 && f.abs() < 9.0e15 => {
                // Exact: the value is whole and well inside i64.
                Value::from(f as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, integral(v))).collect())
        }
        other => other,
    }
}

/// Runs one kernel command for `carve.call` and answers its result.
fn call_kernel(
    kernel: &mut Kernel,
    lua: &Lua,
    command: &str,
    args: LuaValue,
) -> mlua::Result<LuaValue> {
    if refused(command) {
        return Err(LuaError::runtime(format!(
            "{command} is not available to gadgets"
        )));
    }
    let args = match args {
        LuaValue::Nil => "{}".to_string(),
        value => integral(lua.from_value::<Value>(value)?).to_string(),
    };
    let reply = kernel
        .apply(command, &args)
        .map_err(|error| LuaError::runtime(format!("{command}: {error}")))?;
    match reply {
        KernelReply::Json(text) => {
            let envelope: Value = serde_json::from_str(&text).map_err(LuaError::external)?;
            let options = mlua::serde::ser::Options::new()
                .serialize_none_to_null(false)
                .serialize_unit_to_null(false);
            lua.to_value_with(&envelope["result"], options)
        }
        KernelReply::Bytes(_) => Err(LuaError::runtime(format!(
            "{command} answers with binary data, which gadgets cannot read"
        ))),
    }
}

/// Runs `run(values)` of the script against the kernel. Answers the messages
/// the script wrote, or the Lua error message.
pub(super) fn run(
    kernel: &mut Kernel,
    name: &str,
    script: &str,
    values: &Value,
    limits: Limits,
) -> Result<Vec<String>, String> {
    let lua = new_state(limits).map_err(|error| message_of(&error))?;
    let messages: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let outcome = lua.scope(|scope| {
        let carve: Table = lua.globals().get("carve")?;
        carve.raw_set(
            "call",
            scope.create_function_mut(|lua, (command, args): (String, LuaValue)| {
                call_kernel(kernel, lua, &command, args)
            })?,
        )?;
        carve.raw_set(
            "message",
            scope.create_function(|_, text: String| {
                messages.borrow_mut().push(text);
                Ok(())
            })?,
        )?;
        load_script(&lua, name, script)?;
        let run: Function = match lua.globals().get::<LuaValue>("run")? {
            LuaValue::Function(function) => function,
            _ => {
                return Err(LuaError::runtime(format!(
                    "{name} has no run(values) function"
                )))
            }
        };
        let options = mlua::serde::ser::Options::new()
            .serialize_none_to_null(false)
            .serialize_unit_to_null(false);
        run.call::<()>(lua.to_value_with(values, options)?)
    });
    match outcome {
        Ok(()) => Ok(messages.into_inner()),
        Err(error) => Err(message_of(&error)),
    }
}
