//! Gadgets: small Lua 5.4 scripts that add commands to the app.
//!
//! A gadget is a folder `<name>/` holding `<name>.lua` and an optional
//! `gadget.json` manifest (`{"title", "description"}`). The script declares a
//! global `gadget` table with its title and a dialog, and a global function
//! `run(values)` that the app calls with the values the user typed:
//!
//! ```lua
//! gadget = {
//!   title = "Hole",
//!   dialog = { fields = {
//!     { id = "diameter", label = "Diameter", kind = "length", default = 10 },
//!   } },
//! }
//! function run(values)
//!   local job = carve.job()
//!   carve.circle(job.centreX, job.centreY, values.diameter / 2)
//! end
//! ```
//!
//! Field kinds are `number`, `length` (millimetres; the form shows job
//! units), `choice` (with `options`), `check`, `text` and `tool` (a tool
//! database id, 0 for none chosen; `carve.tool(id)` fetches the tool).
//! A field may name a `group` (neighbouring fields with one group share a
//! section of the form) and `enabledBy`, a check field that must be ticked
//! for it to be used. A gadget that works on the selection sets
//! `selection` to the message the form shows while nothing is selected.
//!
//! Scripts reach the kernel through `carve.call(command, args)`, which runs
//! any kernel command and answers its result as a table, and through the
//! helpers of `prelude.lua` built on it. A whole run is one undo step; a run
//! that raises an error puts the document back and reports the Lua message.
//!
//! Gadgets run sandboxed: no `io`, `os`, `package`, `require`, `load`,
//! `dofile` or `debug`, a memory cap and an instruction budget, so an endless
//! loop stops with an error instead of hanging the app. Commands that touch
//! files, the undo stack or other gadgets are refused.
//!
//! Toolpaths a run calculates remember the gadget and its values, saved
//! with the document: `gadgets.run` with `toolpathId` runs the gadget again
//! to edit that toolpath, which takes the new toolpath's place in the list
//! (one undo step) instead of adding one, and the vectors the earlier run
//! drew are taken back first unless they were changed since. Each gadget's
//! last values are kept in the preferences and `gadgets.describe` answers
//! them as `lastValues`, for the dialog to start from.
//!
//! Three gadgets ship inside the kernel. User gadgets live in the `gadgets`
//! folder next to the tool database (the user data directory), and one with
//! the same name as a shipped gadget takes its place.

mod runtime;

use std::cell::Cell;
use std::path::{Path as FsPath, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::CoreError;
use crate::kernel::{Kernel, KernelReply};

pub use runtime::Limits;

/// Every command name this module answers to.
pub const COMMANDS: [&str; 3] = ["gadgets.list", "gadgets.describe", "gadgets.run"];

/// True when `name` is one of the commands this module answers to.
pub fn handles(name: &str) -> bool {
    COMMANDS.contains(&name)
}

/// A gadget compiled into the kernel.
struct Bundled {
    name: &'static str,
    script: &'static str,
    manifest: &'static str,
}

const BUNDLED: [Bundled; 3] = [
    Bundled {
        name: "gear-maker",
        script: include_str!("bundled/gear-maker/gear-maker.lua"),
        manifest: include_str!("bundled/gear-maker/gadget.json"),
    },
    Bundled {
        name: "keyhole",
        script: include_str!("bundled/keyhole/keyhole.lua"),
        manifest: include_str!("bundled/keyhole/gadget.json"),
    },
    Bundled {
        name: "rounding",
        script: include_str!("bundled/rounding/rounding.lua"),
        manifest: include_str!("bundled/rounding/gadget.json"),
    },
];

thread_local! {
    /// True while a gadget runs on this thread, so that a gadget can never
    /// start another one.
    static RUNNING: Cell<bool> = const { Cell::new(false) };
}

/// Clears [`RUNNING`] when the run ends, however it ends.
struct RunningGuard;

impl RunningGuard {
    fn enter() -> Result<RunningGuard, CoreError> {
        if RUNNING.with(Cell::get) {
            return Err(CoreError::validation("a gadget cannot run another gadget"));
        }
        RUNNING.with(|running| running.set(true));
        Ok(RunningGuard)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.with(|running| running.set(false));
    }
}

/// Where a gadget was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Shipped inside the kernel.
    Bundled,
    /// Found in the user's gadgets folder.
    User,
}

/// The optional `gadget.json` next to a script.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    title: Option<String>,
    description: Option<String>,
}

/// A gadget's script and where it came from.
#[derive(Debug, Clone)]
struct Source {
    name: String,
    origin: Origin,
    script: String,
    manifest: Manifest,
}

/// Kind of one dialog field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    /// A plain number.
    Number,
    /// A length in millimetres, shown in job units by the form.
    Length,
    /// One of `options`.
    Choice,
    /// A check box.
    Check,
    /// Free text.
    Text,
    /// A tool from the tool database, by id; 0 when none is chosen.
    Tool,
}

/// One entry of a choice field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChoiceOption {
    /// Value handed to the script.
    pub value: String,
    /// Text the form shows.
    pub label: String,
}

/// One field of a gadget dialog, as the form renders it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Key of the value in the table `run` receives.
    pub id: String,
    /// Label shown next to the control.
    pub label: String,
    /// Control kind.
    pub kind: FieldKind,
    /// Value used when the form sends none: a number for `number` and
    /// `length` (millimetres), a bool for `check`, a string otherwise.
    pub default: Value,
    /// Entries of a `choice` field.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ChoiceOption>,
    /// Smallest legal number, in the field's unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Largest legal number, in the field's unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Help text shown on hover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// Heading of the section the field sits in; neighbouring fields with
    /// the same group share one section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Id of a check field that must be ticked for this field to be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by: Option<String>,
}

/// What `gadgets.describe` answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    /// Folder name, which is what `gadgets.run` takes.
    pub name: String,
    /// Title for the menu and the form.
    pub title: String,
    /// One or two sentences for the form.
    pub description: String,
    /// Where the gadget was found.
    pub origin: Origin,
    /// Dialog fields in order.
    pub fields: Vec<Field>,
    /// When set, the gadget works on the selection and the form shows this
    /// message instead of running while nothing is selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_selection: Option<String>,
    /// The values of the gadget's last successful run, from the
    /// preferences, when it has run before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_values: Option<Value>,
}

/// The `gadget` table as a script writes it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawGadget {
    title: Option<String>,
    description: Option<String>,
    dialog: RawDialog,
    selection: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawDialog {
    fields: Vec<RawField>,
}

#[derive(Debug, Deserialize)]
struct RawField {
    id: String,
    #[serde(default)]
    label: Option<String>,
    kind: FieldKind,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    options: Vec<Value>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    tooltip: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default, rename = "enabledBy")]
    enabled_by: Option<String>,
}

/// Runs a gadget command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] for arguments that do not parse and
/// [`CoreError::Validation`] for an unknown gadget, a bad dialog, a value out
/// of range or a script error (with the Lua message).
pub fn dispatch(kernel: &mut Kernel, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "gadgets.list" => KernelReply::ok(&json!({ "gadgets": list(kernel) })),
        "gadgets.describe" => {
            let args: NameArgs = parse_args(name, args)?;
            let source = find(kernel, &args.name)?;
            let mut description = describe(&source)?;
            description.last_values = kernel
                .prefs
                .values()
                .gadgets
                .last_values
                .get(&description.name)
                .cloned();
            KernelReply::ok(&description)
        }
        "gadgets.run" => {
            let args: RunArgs = parse_args(name, args)?;
            let done = run_editing(
                kernel,
                &args.name,
                &args.values,
                Limits::default(),
                args.toolpath_id,
            )?;
            KernelReply::ok(&json!({
                "messages": done.messages,
                "revision": done.revision,
                "toolpathIds": done.toolpaths,
            }))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct NameArgs {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RunArgs {
    name: String,
    #[serde(default)]
    values: Map<String, Value>,
    /// The toolpath the gadget made before, to edit it in place.
    #[serde(default, rename = "toolpathId")]
    toolpath_id: Option<u64>,
}

fn parse_args<T: serde::de::DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let text = if args.trim().is_empty() { "{}" } else { args };
    serde_json::from_str(text).map_err(|error| CoreError::InvalidArgs {
        command: command.to_string(),
        message: error.to_string(),
    })
}

/// The folder user gadgets live in: `gadgets` next to the tool database,
/// which is the CarveStudio folder of the user data directory.
pub fn user_dir(kernel: &Kernel) -> PathBuf {
    kernel
        .cam()
        .store()
        .path()
        .parent()
        .map(FsPath::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
        .join("gadgets")
}

/// True for a name that is safe to use as a folder name.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
}

/// Reads one user gadget folder, or `None` when it holds no script.
fn read_user(dir: &FsPath, name: &str) -> Option<Source> {
    let script = std::fs::read_to_string(dir.join(format!("{name}.lua"))).ok()?;
    let manifest = std::fs::read_to_string(dir.join("gadget.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    Some(Source {
        name: name.to_string(),
        origin: Origin::User,
        script,
        manifest,
    })
}

/// Every gadget, shipped ones first, each group sorted by name. A user
/// gadget with a shipped gadget's name takes its place.
fn sources(kernel: &Kernel) -> Vec<Source> {
    let mut user: Vec<Source> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(user_dir(kernel)) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_dir() && valid_name(name) {
                if let Some(source) = read_user(&path, name) {
                    user.push(source);
                }
            }
        }
    }
    user.sort_by(|a, b| a.name.cmp(&b.name));
    let mut all: Vec<Source> = BUNDLED
        .iter()
        .filter(|b| !user.iter().any(|u| u.name == b.name))
        .map(|b| Source {
            name: b.name.to_string(),
            origin: Origin::Bundled,
            script: b.script.to_string(),
            manifest: serde_json::from_str(b.manifest).unwrap_or_default(),
        })
        .collect();
    all.extend(user);
    all
}

fn find(kernel: &Kernel, name: &str) -> Result<Source, CoreError> {
    if !valid_name(name) {
        return Err(CoreError::validation(format!("no gadget called {name}")));
    }
    sources(kernel)
        .into_iter()
        .find(|source| source.name == name)
        .ok_or_else(|| CoreError::validation(format!("no gadget called {name}")))
}

/// The list entries: name, title, description, origin and, for a gadget
/// whose script does not load, the error.
fn list(kernel: &Kernel) -> Vec<Value> {
    sources(kernel)
        .iter()
        .map(|source| match describe(source) {
            Ok(description) => json!({
                "name": description.name,
                "title": description.title,
                "description": description.description,
                "origin": description.origin,
            }),
            Err(error) => json!({
                "name": source.name,
                "title": source.manifest.title.clone().unwrap_or_else(|| source.name.clone()),
                "description": source.manifest.description.clone().unwrap_or_default(),
                "origin": source.origin,
                "error": error_text(&error),
            }),
        })
        .collect()
}

/// The message of an error without the category prefix.
fn error_text(error: &CoreError) -> String {
    match error {
        CoreError::Validation(message) => message.clone(),
        other => other.to_string(),
    }
}

/// Loads the script and reads its dialog.
fn describe(source: &Source) -> Result<Description, CoreError> {
    let raw = runtime::read_gadget_table(&source.name, &source.script, Limits::default())
        .map_err(CoreError::validation)?;
    let raw: RawGadget = match raw {
        Value::Null => RawGadget::default(),
        value => serde_json::from_value(value).map_err(|e| {
            CoreError::validation(format!("{}: bad gadget table: {e}", source.name))
        })?,
    };
    let fields = raw
        .dialog
        .fields
        .into_iter()
        .map(|field| normalise_field(&source.name, field))
        .collect::<Result<Vec<Field>, CoreError>>()?;
    let mut ids: Vec<&str> = fields.iter().map(|f| f.id.as_str()).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CoreError::validation(format!(
            "{}: two dialog fields share an id",
            source.name
        )));
    }
    for field in &fields {
        if let Some(check) = &field.enabled_by {
            if !fields
                .iter()
                .any(|f| &f.id == check && f.kind == FieldKind::Check)
            {
                return Err(CoreError::validation(format!(
                    "{}: field {} is enabled by {check}, which is not a check field",
                    source.name, field.id
                )));
            }
        }
    }
    Ok(Description {
        name: source.name.clone(),
        title: source
            .manifest
            .title
            .clone()
            .or(raw.title)
            .unwrap_or_else(|| source.name.clone()),
        description: source
            .manifest
            .description
            .clone()
            .or(raw.description)
            .unwrap_or_default(),
        origin: source.origin,
        fields,
        needs_selection: raw.selection,
        last_values: None,
    })
}

/// Checks one raw field and fills in the defaults its kind implies.
fn normalise_field(gadget: &str, raw: RawField) -> Result<Field, CoreError> {
    if raw.id.is_empty() {
        return Err(CoreError::validation(format!(
            "{gadget}: a dialog field has no id"
        )));
    }
    let bad =
        |what: &str| CoreError::validation(format!("{gadget}: field {} {what}", raw.id.as_str()));
    let options: Vec<ChoiceOption> = raw
        .options
        .iter()
        .map(|option| match option {
            Value::String(text) => Ok(ChoiceOption {
                value: text.clone(),
                label: text.clone(),
            }),
            Value::Object(map) => {
                let value = map
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad("has an option without a value"))?;
                let label = map.get("label").and_then(Value::as_str).unwrap_or(value);
                Ok(ChoiceOption {
                    value: value.to_string(),
                    label: label.to_string(),
                })
            }
            _ => Err(bad("has an option that is not text")),
        })
        .collect::<Result<_, _>>()?;
    let default = match raw.kind {
        FieldKind::Number | FieldKind::Length => {
            let value = match &raw.default {
                None => raw.min.unwrap_or(0.0),
                Some(v) => v.as_f64().ok_or_else(|| bad("needs a number default"))?,
            };
            json!(value)
        }
        FieldKind::Check => match &raw.default {
            None => json!(false),
            Some(Value::Bool(b)) => json!(b),
            Some(_) => return Err(bad("needs a true or false default")),
        },
        FieldKind::Tool => json!(0),
        FieldKind::Text => match &raw.default {
            None => json!(""),
            Some(Value::String(s)) => json!(s),
            Some(_) => return Err(bad("needs a text default")),
        },
        FieldKind::Choice => {
            let first = options
                .first()
                .ok_or_else(|| bad("is a choice without options"))?;
            match &raw.default {
                None => json!(first.value),
                Some(Value::String(s)) if options.iter().any(|o| &o.value == s) => json!(s),
                Some(_) => return Err(bad("has a default that is not one of its options")),
            }
        }
    };
    Ok(Field {
        label: raw.label.clone().unwrap_or_else(|| raw.id.clone()),
        id: raw.id,
        kind: raw.kind,
        default,
        options,
        min: raw.min,
        max: raw.max,
        tooltip: raw.tooltip,
        group: raw.group,
        enabled_by: raw.enabled_by,
    })
}

/// The values `run` receives: each field's value from `given`, or its
/// default, checked against its kind and range.
fn coerce_values(fields: &[Field], given: &Map<String, Value>) -> Result<Value, CoreError> {
    let mut out = Map::new();
    for field in fields {
        let value = match given.get(&field.id) {
            None | Some(Value::Null) => field.default.clone(),
            Some(value) => value.clone(),
        };
        let checked = match field.kind {
            FieldKind::Number | FieldKind::Length => {
                let number = value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                    CoreError::validation(format!("{} must be a number", field.label))
                })?;
                if let Some(min) = field.min.filter(|min| number < *min) {
                    return Err(CoreError::validation(format!(
                        "{} must be at least {min}",
                        field.label
                    )));
                }
                if let Some(max) = field.max.filter(|max| number > *max) {
                    return Err(CoreError::validation(format!(
                        "{} must be at most {max}",
                        field.label
                    )));
                }
                json!(number)
            }
            FieldKind::Check => json!(value.as_bool().ok_or_else(|| {
                CoreError::validation(format!("{} must be true or false", field.label))
            })?),
            FieldKind::Tool => json!(value.as_u64().ok_or_else(|| {
                CoreError::validation(format!("{} must be a tool", field.label))
            })?),
            FieldKind::Text => json!(value.as_str().ok_or_else(|| {
                CoreError::validation(format!("{} must be text", field.label))
            })?),
            FieldKind::Choice => {
                let text = value.as_str().unwrap_or_default();
                if !field.options.iter().any(|o| o.value == text) {
                    return Err(CoreError::validation(format!(
                        "{} must be one of its options",
                        field.label
                    )));
                }
                json!(text)
            }
        };
        out.insert(field.id.clone(), checked);
    }
    Ok(Value::Object(out))
}

/// Runs a gadget as one undo step labelled with its title. Answers the
/// messages it wrote and the document revision afterwards.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown gadget, a value the dialog
/// refuses, a gadget started from inside another, or a script error, in
/// which case the document is back the way it was.
pub fn run(
    kernel: &mut Kernel,
    name: &str,
    values: &Map<String, Value>,
    limits: Limits,
) -> Result<(Vec<String>, u64), CoreError> {
    let done = run_editing(kernel, name, values, limits, None)?;
    Ok((done.messages, done.revision))
}

/// What a gadget run did.
#[derive(Debug, Clone, PartialEq)]
pub struct RunDone {
    /// Messages the script wrote.
    pub messages: Vec<String>,
    /// The document revision afterwards.
    pub revision: u64,
    /// The toolpaths the run made or, when it edited one, that toolpath
    /// first, in list order.
    pub toolpaths: Vec<u64>,
}

/// Runs a gadget as one undo step labelled with its title. With `editing`,
/// the first toolpath the run calculates replaces that toolpath (which a
/// run of the same gadget made) in place, keeping its id and list position,
/// and the vectors the earlier run drew go first unless changed since.
/// Every toolpath the run makes records the gadget and its values, and the
/// values are remembered in the preferences for the next run.
///
/// # Errors
/// As [`run`], and [`CoreError::Validation`] when `editing` names a
/// toolpath that is gone or that this gadget did not make.
pub fn run_editing(
    kernel: &mut Kernel,
    name: &str,
    values: &Map<String, Value>,
    limits: Limits,
    editing: Option<u64>,
) -> Result<RunDone, CoreError> {
    let _guard = RunningGuard::enter()?;
    let source = find(kernel, name)?;
    let description = describe(&source)?;
    let values = coerce_values(&description.fields, values)?;
    let earlier = match editing {
        Some(id) => {
            let toolpath = kernel.document().toolpath(id)?;
            match &toolpath.gadget {
                Some(gadget) if gadget.name == source.name => Some(gadget.drawn.clone()),
                _ => {
                    return Err(CoreError::validation(format!(
                        "{} was not made by the {} gadget",
                        toolpath.name, description.title
                    )))
                }
            }
        }
        None => None,
    };

    let document = kernel.document_mut();
    let depth = document.journal().undo_depth();
    let limit = document.journal().limit();
    // Every command the script runs pushes its own step; none may fall off
    // the bottom before they are merged.
    document.set_undo_limit(usize::MAX);
    if let Some(drawn) = &earlier {
        take_back(document, drawn);
    }
    let toolpaths_before: Vec<u64> = document.toolpath_order().to_vec();
    let entities_before = document.entity_ids();

    let outcome = runtime::run(kernel, &source.name, &source.script, &values, limits);

    let document = kernel.document_mut();
    let result = match outcome {
        Ok(messages) => {
            match record_run(
                document,
                &source.name,
                &values,
                editing,
                &toolpaths_before,
                &entities_before,
            ) {
                Ok(toolpaths) => {
                    document.squash_steps_since(depth, &description.title);
                    Ok(RunDone {
                        messages,
                        revision: document.revision(),
                        toolpaths,
                    })
                }
                Err(error) => {
                    document.revert_steps_since(depth);
                    Err(error)
                }
            }
        }
        Err(message) => {
            document.revert_steps_since(depth);
            Err(CoreError::validation(message))
        }
    };
    document.set_undo_limit(limit);
    if result.is_ok() {
        // Remembering is a convenience: a preferences file that cannot be
        // written does not fail a run that worked.
        let _ = kernel.prefs.remember_gadget(&source.name, &values);
    }
    result
}

/// Deletes the vectors an earlier run drew that are still as it drew them.
fn take_back(document: &mut crate::doc::Document, drawn: &[crate::cam::toolpath::SourceRef]) {
    let unchanged: Vec<u64> = drawn
        .iter()
        .filter(|d| {
            document
                .entity(d.id)
                .is_ok_and(|entity| entity.revision == d.revision)
        })
        .map(|d| d.id)
        .collect();
    if unchanged.is_empty() {
        return;
    }
    document.begin("Take Back Gadget Vectors");
    document.delete_entities(&unchanged);
    document.commit();
}

/// Stamps the toolpaths a run made with the gadget, its values and the
/// vectors it drew, and puts the first in place of `editing`. Answers the
/// toolpaths made, the edited one first.
fn record_run(
    document: &mut crate::doc::Document,
    name: &str,
    values: &Value,
    editing: Option<u64>,
    toolpaths_before: &[u64],
    entities_before: &std::collections::BTreeSet<u64>,
) -> Result<Vec<u64>, CoreError> {
    let made: Vec<u64> = document
        .toolpath_order()
        .iter()
        .copied()
        .filter(|id| !toolpaths_before.contains(id))
        .collect();
    let drawn: Vec<crate::cam::toolpath::SourceRef> = document
        .entity_ids()
        .difference(entities_before)
        .filter_map(|id| {
            document
                .entity(*id)
                .ok()
                .filter(|entity| entity.parent.is_none())
                .map(|entity| crate::cam::toolpath::SourceRef {
                    id: *id,
                    revision: entity.revision,
                })
        })
        .collect();
    if made.is_empty() {
        return Ok(Vec::new());
    }
    let stamp = crate::cam::toolpath::GadgetSource {
        name: name.to_string(),
        values: values.clone(),
        drawn,
    };
    document.begin("Record Gadget");
    let mut answered = Vec::with_capacity(made.len());
    let mut rest = made.as_slice();
    if let (Some(id), Some((first, others))) = (editing, made.split_first()) {
        let mut replacement = document.toolpath(*first)?.clone();
        replacement.gadget = Some(stamp.clone());
        replacement.visible = document.toolpath(id)?.visible;
        document.delete_toolpaths(&[*first])?;
        document.replace_toolpath(id, replacement)?;
        answered.push(id);
        rest = others;
    }
    for id in rest {
        document.update_toolpath(*id, |t| t.gadget = Some(stamp.clone()))?;
        answered.push(*id);
    }
    document.commit();
    Ok(answered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::commands::CamState;
    use crate::test_support::TempDir;

    fn temp_kernel(label: &str) -> (TempDir, Kernel) {
        let temp = TempDir::new(label);
        let cam = CamState::new(
            crate::cam::tooldb::ToolStore::at(temp.path().join("tools.json")),
            crate::post::PostLibrary::at(temp.path().join("posts")),
        );
        (temp, Kernel::with_cam(cam))
    }

    fn call(kernel: &mut Kernel, name: &str, args: Value) -> Result<Value, CoreError> {
        match kernel.apply(name, &args.to_string())? {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("reply is JSON");
                Ok(value["result"].clone())
            }
            KernelReply::Bytes(_) => panic!("{name} answered bytes"),
        }
    }

    fn install(temp: &TempDir, name: &str, script: &str) {
        let dir = temp.path().join("gadgets").join(name);
        std::fs::create_dir_all(&dir).expect("gadget folder");
        std::fs::write(dir.join(format!("{name}.lua")), script).expect("script written");
    }

    fn entity_count(kernel: &Kernel) -> usize {
        kernel.document().order().len()
    }

    fn small_limits() -> Limits {
        Limits {
            instructions: 2_000_000,
            ..Limits::default()
        }
    }

    #[test]
    fn the_shipped_gadgets_are_listed_and_described() {
        let (_temp, mut kernel) = temp_kernel("gadgets-list");
        let listed = call(&mut kernel, "gadgets.list", json!({})).expect("list");
        let names: Vec<&str> = listed["gadgets"]
            .as_array()
            .expect("an array")
            .iter()
            .filter_map(|g| g["name"].as_str())
            .collect();
        assert_eq!(names, vec!["gear-maker", "keyhole", "rounding"]);
        assert!(listed["gadgets"][0].get("error").is_none());
        let gear = call(
            &mut kernel,
            "gadgets.describe",
            json!({ "name": "gear-maker" }),
        )
        .expect("describe");
        assert_eq!(gear["origin"], "bundled");
        let fields = gear["fields"].as_array().expect("fields");
        assert!(fields
            .iter()
            .any(|f| f["id"] == "teeth" && f["kind"] == "number"));
        assert!(fields.iter().any(|f| f["kind"] == "length"));
    }

    #[test]
    fn the_gear_maker_draws_a_gear_in_one_undo_step() {
        let (_temp, mut kernel) = temp_kernel("gadgets-gear");
        let before = kernel.document().journal().undo_depth();
        let reply = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "gear-maker", "values": { "teeth": 24, "module": 2.0, "bore": 8.0 } }),
        )
        .expect("the gear runs");
        assert!(reply["revision"].as_u64().is_some());
        assert!(!reply["messages"].as_array().expect("messages").is_empty());
        // The outline and the bore.
        assert_eq!(entity_count(&kernel), 2);
        let listed = call(&mut kernel, "entities.list", json!({})).expect("list");
        let widths: Vec<f64> = listed
            .as_array()
            .expect("entities")
            .iter()
            .map(|e| e["bbox"][2].as_f64().unwrap_or(0.0) - e["bbox"][0].as_f64().unwrap_or(0.0))
            .collect();
        // Outside diameter is (teeth + 2) x module.
        assert!(widths.iter().any(|w| (w - 52.0).abs() < 0.5), "{widths:?}");
        assert!(widths.iter().any(|w| (w - 8.0).abs() < 1e-6), "{widths:?}");
        assert_eq!(kernel.document().journal().undo_depth(), before + 1);
        assert_eq!(kernel.document().journal().undo_label(), Some("Gear Maker"));
        call(&mut kernel, "undo", json!({})).expect("undo");
        assert_eq!(entity_count(&kernel), 0);
        // The layer the gadget made goes with it.
        let layers = call(&mut kernel, "layers.list", json!({})).expect("layers");
        assert!(!layers
            .as_array()
            .expect("layers")
            .iter()
            .any(|l| l["name"] == "Gear"));
    }

    fn first_end_mill(kernel: &mut Kernel) -> u64 {
        let tools = call(kernel, "tools.list", json!({})).expect("tools");
        tools["tools"]
            .as_array()
            .and_then(|list| list.iter().find(|t| t["toolType"]["kind"] == "endMill"))
            .and_then(|t| t["id"].as_u64())
            .expect("an end mill")
    }

    /// A circle of radius 5 round (x, y), as a new entity id.
    fn add_circle(kernel: &mut Kernel, x: f64, y: f64) -> u64 {
        let entity = json!({
            "kind": "path", "layer": 0, "sheet": 0,
            "path": { "subpaths": [{
                "start": { "x": x + 5.0, "y": y },
                "segments": [
                    { "kind": "arc", "to": { "x": x - 5.0, "y": y }, "bulge": 1.0 },
                    { "kind": "arc", "to": { "x": x + 5.0, "y": y }, "bulge": 1.0 },
                ],
                "closed": true,
            }] },
        });
        let reply = call(kernel, "entities.create", json!({ "entity": entity })).expect("circle");
        reply["id"].as_u64().expect("an id")
    }

    #[test]
    fn the_keyhole_gadget_describes_the_reference_dialog() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-describe");
        let reply = call(
            &mut kernel,
            "gadgets.describe",
            json!({ "name": "keyhole" }),
        )
        .expect("describe");
        assert_eq!(reply["title"], "Keyhole Toolpath");
        assert!(reply["needsSelection"].as_str().is_some());
        let fields = reply["fields"].as_array().expect("fields");
        let ids: Vec<&str> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(
            ids,
            [
                "direction",
                "depth",
                "length",
                "preview",
                "hole",
                "slot",
                "layer",
                "tool",
                "name"
            ]
        );
        assert_eq!(fields[0]["options"].as_array().map(Vec::len), Some(4));
        assert_eq!(fields[4]["enabledBy"], "preview");
        assert_eq!(fields[1]["group"], "Slot Dimensions");
        assert_eq!(fields[8]["default"], "Keyhole Toolpath");
    }

    #[test]
    fn editing_a_keyhole_toolpath_reruns_the_gadget_in_place() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-edit");
        let mill = first_end_mill(&mut kernel);
        let circle = add_circle(&mut kernel, 50.0, 50.0);
        call(&mut kernel, "selection.set", json!({ "ids": [circle] })).expect("select");
        let first = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill, "depth": 6.0 } }),
        )
        .expect("first run");
        let id = first["toolpathIds"][0].as_u64().expect("toolpath made");
        let listed = call(&mut kernel, "cam.list", json!({})).expect("list");
        assert_eq!(listed[0]["gadget"], "keyhole");
        let got = call(&mut kernel, "cam.get", json!({ "id": id })).expect("get");
        assert_eq!(got["gadget"]["name"], "keyhole");
        assert_eq!(got["gadget"]["values"]["depth"], 6.0);
        // The circle and one preview outline.
        assert_eq!(entity_count(&kernel), 2);

        // The dialog starts from the values last used.
        let described = call(
            &mut kernel,
            "gadgets.describe",
            json!({ "name": "keyhole" }),
        )
        .expect("describe");
        assert_eq!(described["lastValues"]["depth"], 6.0);
        assert_eq!(described["lastValues"]["tool"], mill);

        let undo_before = kernel.document().journal().undo_depth();
        let edited = call(
            &mut kernel,
            "gadgets.run",
            json!({
                "name": "keyhole",
                "toolpathId": id,
                "values": { "tool": mill, "depth": 8.0, "name": "Hanger" },
            }),
        )
        .expect("edit run");
        assert_eq!(edited["toolpathIds"], json!([id]));
        let listed = call(&mut kernel, "cam.list", json!({})).expect("list");
        let list = listed.as_array().expect("toolpaths");
        assert_eq!(list.len(), 1, "updated, not added: {listed}");
        assert_eq!(list[0]["id"], id);
        assert_eq!(list[0]["name"], "Hanger");
        let got = call(&mut kernel, "cam.get", json!({ "id": id })).expect("get");
        assert_eq!(got["gadget"]["values"]["depth"], 8.0);
        let bottom = got["bounds"]["min"]["z"].as_f64().expect("bounds");
        assert!((bottom + 8.0).abs() < 1e-6, "slot depth {bottom}");
        // The old preview went and a new one was drawn.
        assert_eq!(entity_count(&kernel), 2);
        assert_eq!(kernel.document().journal().undo_depth(), undo_before + 1);

        call(&mut kernel, "undo", json!({})).expect("undo");
        let got = call(&mut kernel, "cam.get", json!({ "id": id })).expect("get");
        assert_eq!(got["gadget"]["values"]["depth"], 6.0);
        assert_eq!(entity_count(&kernel), 2);
    }

    #[test]
    fn an_edit_of_a_toolpath_the_gadget_did_not_make_is_refused() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-edit-refused");
        let mill = first_end_mill(&mut kernel);
        let circle = add_circle(&mut kernel, 50.0, 50.0);
        call(&mut kernel, "selection.set", json!({ "ids": [circle] })).expect("select");
        let gone = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "toolpathId": 99, "values": { "tool": mill } }),
        )
        .expect_err("no such toolpath");
        assert!(gone.to_string().contains("99"), "{gone}");
        let made = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill } }),
        )
        .expect("run");
        let id = made["toolpathIds"][0].clone();
        let other = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "gear-maker", "toolpathId": id, "values": {} }),
        )
        .expect_err("not a gear toolpath");
        assert!(other.to_string().contains("was not made by"), "{other}");
    }

    #[test]
    fn the_keyhole_gadget_needs_a_selection_and_a_tool() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-refused");
        let mill = first_end_mill(&mut kernel);
        let none = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill } }),
        )
        .expect_err("nothing selected");
        assert!(none.to_string().contains("circular vectors"), "{none}");
        let id = add_circle(&mut kernel, 50.0, 50.0);
        call(&mut kernel, "selection.set", json!({ "ids": [id] })).expect("select");
        let no_tool = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": {} }),
        )
        .expect_err("no tool");
        assert!(no_tool.to_string().contains("choose a tool"), "{no_tool}");
        assert_eq!(entity_count(&kernel), 1);
    }

    #[test]
    fn the_keyhole_gadget_cuts_a_slot_at_each_centre_and_draws_previews() {
        let (temp, mut kernel) = temp_kernel("gadgets-keyhole");
        let mill = first_end_mill(&mut kernel);
        let a = add_circle(&mut kernel, 30.0, 40.0);
        let b = add_circle(&mut kernel, 120.0, 40.0);
        call(&mut kernel, "selection.set", json!({ "ids": [a, b] })).expect("select");
        call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill, "direction": "leftToRight",
                     "depth": 6.0, "length": 20.0, "hole": 10.0, "slot": 5.0 } }),
        )
        .expect("the keyhole runs");
        // One preview outline per entry point, on its own layer.
        assert_eq!(entity_count(&kernel), 4);
        let layers = call(&mut kernel, "layers.list", json!({})).expect("layers");
        let preview = layers
            .as_array()
            .expect("layers")
            .iter()
            .find(|l| l["name"] == "Keyhole Preview")
            .and_then(|l| l["id"].as_u64())
            .expect("the preview layer");
        let listed = call(&mut kernel, "entities.list", json!({})).expect("list");
        let outlines: Vec<&Value> = listed
            .as_array()
            .expect("entities")
            .iter()
            .filter(|e| e["layer"].as_u64() == Some(preview))
            .collect();
        assert_eq!(outlines.len(), 2);
        // The first runs from the left of the hole (radius 5) to the round
        // slot end (20 along plus half the slot width).
        let bbox = &outlines[0]["bbox"];
        assert!(
            (bbox[0].as_f64().unwrap_or(0.0) - 25.0).abs() < 1e-6,
            "{bbox}"
        );
        assert!(
            (bbox[2].as_f64().unwrap_or(0.0) - 52.5).abs() < 1e-6,
            "{bbox}"
        );
        let list = call(&mut kernel, "cam.list", json!({})).expect("cam.list");
        assert_eq!(list[0]["name"], "Keyhole Toolpath");
        assert_eq!(list[0]["kind"], "keyhole");
        let id = list[0]["id"].as_u64().expect("toolpath id");

        // Through a shipped post: plunge at each centre, out 20 mm and back.
        let out = temp.path().join("keyhole.gcode");
        call(
            &mut kernel,
            "cam.save",
            json!({ "toolpathIds": [id], "post": "grbl_mm", "path": out.to_string_lossy() }),
        )
        .expect("saved");
        let gcode = std::fs::read_to_string(&out).expect("gcode");
        let job = call(&mut kernel, "job.get", json!({})).expect("job");
        let top = if job["zZero"] == "bottom" {
            job["thickness"].as_f64().unwrap_or(0.0)
        } else {
            0.0
        };
        let plunge = format!("G1Z{:.3}", top - 6.0);
        // Modal output: each slot plunges, runs 20 mm along +X, comes back
        // and lifts out at the entry point.
        let expected = [
            "G0X30.000Y40.000",
            plunge.as_str(),
            "G1X50.000",
            "G1X30.000",
            "G0Z",
            "G0X120.000",
            plunge.as_str(),
            "G1X140.000",
            "G1X120.000",
            "G0Z",
        ];
        let lines: Vec<&str> = gcode.lines().collect();
        let mut from = 0;
        for want in expected {
            let found = lines[from..]
                .iter()
                .position(|line| line.starts_with(want))
                .unwrap_or_else(|| panic!("{want} missing after line {from} of\n{gcode}"));
            from += found + 1;
        }

        call(&mut kernel, "undo", json!({})).expect("undo");
        assert_eq!(entity_count(&kernel), 2);
        let toolpaths = call(&mut kernel, "cam.list", json!({})).expect("cam.list");
        assert!(toolpaths.as_array().expect("toolpaths").is_empty());
    }

    #[test]
    fn the_keyhole_preview_skips_an_empty_path_in_a_group() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-empty");
        let mill = first_end_mill(&mut kernel);
        let a = add_circle(&mut kernel, 30.0, 40.0);
        let empty = json!({ "kind": "path", "layer": 0, "sheet": 0, "path": { "subpaths": [] } });
        let empty = call(&mut kernel, "entities.create", json!({ "entity": empty }))
            .expect("empty path")["id"]
            .as_u64()
            .expect("an id");
        let group = call(&mut kernel, "entities.group", json!({ "ids": [a, empty] }))
            .expect("group")["id"]
            .as_u64()
            .expect("an id");
        call(&mut kernel, "selection.set", json!({ "ids": [group] })).expect("select");
        call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill, "direction": "bottomToTop",
                     "depth": 6.0, "length": 20.0, "hole": 10.0, "slot": 5.0 } }),
        )
        .expect("the keyhole runs");
        let layers = call(&mut kernel, "layers.list", json!({})).expect("layers");
        let preview = layers
            .as_array()
            .expect("layers")
            .iter()
            .find(|l| l["name"] == "Keyhole Preview")
            .and_then(|l| l["id"].as_u64())
            .expect("the preview layer");
        let listed = call(&mut kernel, "entities.list", json!({})).expect("list");
        let outlines: Vec<&Value> = listed
            .as_array()
            .expect("entities")
            .iter()
            .filter(|e| e["layer"].as_u64() == Some(preview))
            .collect();
        // One outline, round the circle's entry point, none at 0, 0.
        assert_eq!(outlines.len(), 1, "{outlines:?}");
        let left = outlines[0]["bbox"][0].as_f64().unwrap_or(0.0);
        assert!((left - 25.0).abs() < 1e-6, "{outlines:?}");
    }

    #[test]
    fn the_keyhole_preview_draws_an_outline_for_each_vector_in_a_group() {
        let (_temp, mut kernel) = temp_kernel("gadgets-keyhole-group");
        let mill = first_end_mill(&mut kernel);
        let a = add_circle(&mut kernel, 30.0, 40.0);
        let b = add_circle(&mut kernel, 120.0, 40.0);
        let c = add_circle(&mut kernel, 200.0, 90.0);
        let inner = call(&mut kernel, "entities.group", json!({ "ids": [b, c] }))
            .expect("inner group")["id"]
            .as_u64()
            .expect("an id");
        let group = call(&mut kernel, "entities.group", json!({ "ids": [a, inner] }))
            .expect("group")["id"]
            .as_u64()
            .expect("an id");
        call(&mut kernel, "selection.set", json!({ "ids": [group] })).expect("select");
        call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "keyhole", "values": { "tool": mill, "direction": "bottomToTop",
                     "depth": 6.0, "length": 20.0, "hole": 10.0, "slot": 5.0 } }),
        )
        .expect("the keyhole runs");
        let layers = call(&mut kernel, "layers.list", json!({})).expect("layers");
        let preview = layers
            .as_array()
            .expect("layers")
            .iter()
            .find(|l| l["name"] == "Keyhole Preview")
            .and_then(|l| l["id"].as_u64())
            .expect("the preview layer");
        let listed = call(&mut kernel, "entities.list", json!({})).expect("list");
        let mut centres: Vec<(f64, f64)> = listed
            .as_array()
            .expect("entities")
            .iter()
            .filter(|e| e["layer"].as_u64() == Some(preview))
            .map(|e| {
                let b = &e["bbox"];
                let x = (b[0].as_f64().unwrap_or(0.0) + b[2].as_f64().unwrap_or(0.0)) / 2.0;
                // The hole's bottom is 5 below the entry point.
                (x, b[1].as_f64().unwrap_or(0.0) + 5.0)
            })
            .collect();
        centres.sort_by(|p, q| p.0.total_cmp(&q.0));
        // One outline at each vector's centre, as the toolpath cuts a slot
        // at each: not one at the middle of the group.
        let want = [(30.0, 40.0), (120.0, 40.0), (200.0, 90.0)];
        assert_eq!(centres.len(), want.len(), "{centres:?}");
        for ((x, y), (wx, wy)) in centres.iter().zip(want) {
            assert!(
                (x - wx).abs() < 1e-6 && (y - wy).abs() < 1e-6,
                "{centres:?}"
            );
        }
        let calculated = call(&mut kernel, "cam.list", json!({})).expect("cam.list");
        assert_eq!(calculated.as_array().map(Vec::len), Some(1));
        // The toolpath's entry points are the same centres.
        let paths: Vec<crate::geom::Path> = kernel
            .document()
            .paths_of(group)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        let mut entries: Vec<(f64, f64)> = crate::cam::keyhole::entry_points(&paths)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect();
        entries.sort_by(|p, q| p.0.total_cmp(&q.0));
        for ((x, y), (ex, ey)) in centres.iter().zip(&entries) {
            assert!(
                (x - ex).abs() < 1e-6 && (y - ey).abs() < 1e-6,
                "{entries:?}"
            );
        }
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn the_rounding_gadget_turns_a_rotary_blank_with_the_chosen_tool() {
        let (_temp, mut kernel) = temp_kernel("gadgets-rounding");
        let fields = call(
            &mut kernel,
            "gadgets.describe",
            json!({ "name": "rounding" }),
        )
        .expect("describe");
        assert!(fields["fields"]
            .as_array()
            .expect("fields")
            .iter()
            .any(|f| f["kind"] == "tool" && f["default"] == 0));
        let mill = first_end_mill(&mut kernel);
        let flat = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "rounding", "values": { "tool": mill } }),
        )
        .expect_err("a flat job");
        assert!(flat.to_string().contains("rotary"), "{flat}");
        let mut job = call(&mut kernel, "job.get", json!({})).expect("job");
        job["kind"] = json!("rotary");
        job["rotary"] = json!({ "diameter": 50.0, "axis": "x", "length": 120.0 });
        call(&mut kernel, "job.set", job).expect("rotary job");
        let none = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "rounding", "values": {} }),
        )
        .expect_err("no tool");
        assert!(none.to_string().contains("choose a tool"), "{none}");
        call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "rounding", "values": { "tool": mill, "finish": 44.0,
                     "method": "raster", "name": "Turn Down" } }),
        )
        .expect("rounds");
        let list = call(&mut kernel, "cam.list", json!({})).expect("cam.list");
        assert_eq!(list[0]["name"], "Turn Down");
        assert!(list[0]["moves"].as_u64().unwrap_or(0) > 10);
    }

    #[test]
    fn a_value_out_of_range_is_refused() {
        let (_temp, mut kernel) = temp_kernel("gadgets-range");
        let error = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "gear-maker", "values": { "teeth": 2 } }),
        )
        .expect_err("too few teeth");
        assert!(error.to_string().contains("at least"), "{error}");
    }

    #[test]
    fn the_sandbox_has_no_files_processes_or_modules() {
        let (temp, mut kernel) = temp_kernel("gadgets-sandbox");
        install(
            &temp,
            "probe",
            r#"
            function run(values)
              carve.message(tostring(os) .. " " .. tostring(io) .. " " .. tostring(require)
                .. " " .. tostring(load) .. " " .. tostring(dofile) .. " " .. tostring(debug)
                .. " " .. tostring(package) .. " " .. tostring(string.dump))
            end
            "#,
        );
        let (messages, _) =
            run(&mut kernel, "probe", &Map::new(), small_limits()).expect("probe runs");
        assert_eq!(
            messages,
            vec!["nil nil nil nil nil nil nil nil".to_string()]
        );

        install(
            &temp,
            "shell",
            "function run(values) os.execute('touch /tmp/never') end",
        );
        let error = run(&mut kernel, "shell", &Map::new(), small_limits()).expect_err("no os");
        assert!(error.to_string().contains("os"), "{error}");
    }

    #[test]
    fn an_endless_loop_is_stopped() {
        let (temp, mut kernel) = temp_kernel("gadgets-loop");
        install(&temp, "spin", "function run(values) while true do end end");
        let error = run(&mut kernel, "spin", &Map::new(), small_limits()).expect_err("stopped");
        assert!(error.to_string().contains("too long"), "{error}");
        install(&temp, "spin-top", "while true do end");
        let listed = call(&mut kernel, "gadgets.list", json!({})).expect("list still works");
        let entry = listed["gadgets"]
            .as_array()
            .expect("gadgets")
            .iter()
            .find(|g| g["name"] == "spin-top")
            .cloned()
            .expect("listed");
        assert!(entry["error"].as_str().is_some());
    }

    #[test]
    fn an_error_rolls_the_whole_run_back() {
        let (temp, mut kernel) = temp_kernel("gadgets-rollback");
        install(
            &temp,
            "half",
            r#"
            function run(values)
              carve.layer("Half")
              carve.rectangle(0, 0, 10, 10)
              carve.circle(20, 20, 5)
              error("stopped half way")
            end
            "#,
        );
        let revision = kernel.document().revision();
        let depth = kernel.document().journal().undo_depth();
        let error = run(&mut kernel, "half", &Map::new(), small_limits()).expect_err("fails");
        assert!(error.to_string().contains("stopped half way"), "{error}");
        assert!(!error.to_string().contains("traceback"), "{error}");
        assert!(!error.to_string().contains(".lua:"), "{error}");
        assert_eq!(entity_count(&kernel), 0);
        assert_eq!(kernel.document().journal().undo_depth(), depth);
        assert!(kernel.document().revision() > revision);
        let layers = call(&mut kernel, "layers.list", json!({})).expect("layers");
        assert!(!layers
            .as_array()
            .expect("layers")
            .iter()
            .any(|l| l["name"] == "Half"));
    }

    #[test]
    fn a_gadget_cannot_run_a_gadget_or_touch_files_or_undo() {
        let (temp, mut kernel) = temp_kernel("gadgets-reentry");
        for (name, command) in [
            ("again", "gadgets.run"),
            ("saver", "doc.save"),
            ("undoer", "undo"),
            ("opener", "doc.open"),
            ("templater", "templates.new"),
        ] {
            install(
                &temp,
                name,
                &format!("function run(v) carve.call('{command}', {{ name = 'again', path = '/tmp/x' }}) end"),
            );
            let error = run(&mut kernel, name, &Map::new(), small_limits()).expect_err(name);
            assert!(error.to_string().contains("not available"), "{error}");
        }
        // The guard is released after each run.
        assert!(!RUNNING.with(Cell::get));
    }

    #[test]
    fn carve_call_answers_tables_and_kernel_errors_can_be_caught() {
        let (temp, mut kernel) = temp_kernel("gadgets-call");
        install(
            &temp,
            "caller",
            r#"
            function run(values)
              local info = carve.call("doc.info", {})
              carve.message(info.job.units .. " " .. tostring(info.activeLayer))
              local ok, err = pcall(carve.call, "entities.get", { id = 999 })
              carve.message(tostring(ok))
              local id = carve.polyline({ {0, 0}, {10, 0}, {10, 5} }, false)
              carve.select({ id })
              carve.message(tostring(#carve.selection()))
              print("done", 1)
            end
            "#,
        );
        let (messages, _) = run(&mut kernel, "caller", &Map::new(), small_limits()).expect("runs");
        assert_eq!(messages[0], "mm 1");
        assert_eq!(messages[1], "false");
        assert_eq!(messages[2], "1");
        assert_eq!(messages[3], "done\t1");
    }

    #[test]
    fn a_user_gadget_with_a_manifest_and_dialog_is_described() {
        let (temp, mut kernel) = temp_kernel("gadgets-user");
        install(
            &temp,
            "hole",
            r#"
            gadget = {
              title = "Hole",
              dialog = { fields = {
                { id = "d", label = "Diameter", kind = "length", default = 10, min = 1 },
                { id = "shape", kind = "choice", options = { "round", { value = "square", label = "Square" } } },
                { id = "both", kind = "check", default = true },
                { id = "name", kind = "text", default = "H" },
              } },
            }
            function run(values)
              carve.message(values.d .. " " .. values.shape .. " " .. tostring(values.both) .. " " .. values.name)
            end
            "#,
        );
        std::fs::write(
            temp.path().join("gadgets/hole/gadget.json"),
            r#"{"description": "Cuts a hole."}"#,
        )
        .expect("manifest");
        let described =
            call(&mut kernel, "gadgets.describe", json!({ "name": "hole" })).expect("describe");
        assert_eq!(described["title"], "Hole");
        assert_eq!(described["description"], "Cuts a hole.");
        assert_eq!(described["origin"], "user");
        assert_eq!(described["fields"][1]["default"], "round");
        assert_eq!(described["fields"][1]["options"][1]["label"], "Square");
        let reply = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "hole", "values": { "shape": "square" } }),
        )
        .expect("runs");
        assert_eq!(reply["messages"][0], "10.0 square true H");
        let error = call(
            &mut kernel,
            "gadgets.run",
            json!({ "name": "hole", "values": { "shape": "oval" } }),
        )
        .expect_err("not an option");
        assert!(error.to_string().contains("options"));
        let error = call(&mut kernel, "gadgets.describe", json!({ "name": "../x" }))
            .expect_err("no path names");
        assert!(error.to_string().contains("no gadget"));
    }
}
