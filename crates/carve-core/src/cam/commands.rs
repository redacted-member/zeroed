//! The `tools.*`, `posts.*` and `cam.save` commands.
//!
//! The tool database and the post library are user settings rather than parts
//! of the document, so they carry their own revision counter instead of the
//! document one. Every mutating command answers with `dbRevision`, which the
//! tool database panel watches the way the rest of the app watches the
//! document revision.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::error::CoreError;
use crate::geom::BBox;
use crate::kernel::KernelReply;
use crate::post::interp::post_process_pieces;
use crate::post::rotary::Wrap;
use crate::post::{PostJob, PostLibrary};

use super::naming;
use super::save::{self, SaveOutput};
use super::tiling::{self, TileSpec};
use super::tool::{CuttingData, FeedUnits, Tool};
use super::tooldb::{MergeMode, ToolDatabase, ToolStore, ROOT_GROUP};
use super::toolpath::Toolpath;
use super::trial_cut::TrialCut;

/// Every command name this module answers to.
pub const COMMANDS: [&str; 39] = [
    "tools.list",
    "tools.get",
    "tools.save",
    "tools.delete",
    "tools.move",
    "tools.copy",
    "tools.import",
    "tools.addDefaults",
    "tools.export",
    "tools.groups.list",
    "tools.groups.add",
    "tools.groups.rename",
    "tools.groups.move",
    "tools.groups.delete",
    "tools.materials.list",
    "tools.materials.add",
    "tools.materials.rename",
    "tools.materials.delete",
    "tools.machines.list",
    "tools.machines.add",
    "tools.machines.rename",
    "tools.machines.delete",
    "tools.cuttingData.set",
    "tools.cuttingData.delete",
    "tools.place",
    "tools.materials.activate",
    "tools.materials.duplicate",
    "tools.nameFormats.list",
    "tools.nameFormats.set",
    "tools.previewName",
    "posts.list",
    "posts.get",
    "posts.import",
    "posts.text",
    "posts.changelog",
    "posts.remove",
    "posts.customize",
    "posts.folder",
    "cam.save",
];

/// True when `name` is one of the commands this module answers to.
pub fn handles(name: &str) -> bool {
    COMMANDS.contains(&name)
}

/// The tool database, the post library and the calculated toolpaths, which
/// the kernel owns alongside the document.
#[derive(Debug)]
pub struct CamState {
    store: ToolStore,
    posts: PostLibrary,
    database: Option<ToolDatabase>,
    db_revision: u64,
    toolpaths: Vec<Toolpath>,
}

impl Default for CamState {
    fn default() -> Self {
        CamState::new(ToolStore::user_default(), PostLibrary::user_default())
    }
}

impl CamState {
    /// State that reads and writes the given tool file and posts directory.
    /// Nothing is read until the first command asks for it.
    pub fn new(store: ToolStore, posts: PostLibrary) -> Self {
        CamState {
            store,
            posts,
            database: None,
            db_revision: 0,
            toolpaths: Vec::new(),
        }
    }

    /// Where the tool database file lives.
    pub fn store(&self) -> &ToolStore {
        &self.store
    }

    /// The post library.
    pub fn posts(&self) -> &PostLibrary {
        &self.posts
    }

    /// The calculated toolpaths, in the order they were added.
    pub fn toolpaths(&self) -> &[Toolpath] {
        &self.toolpaths
    }

    /// Adds a calculated toolpath, replacing one that has the same id.
    pub fn set_toolpath(&mut self, toolpath: Toolpath) {
        match self.toolpaths.iter_mut().find(|t| t.id == toolpath.id) {
            Some(found) => *found = toolpath,
            None => self.toolpaths.push(toolpath),
        }
    }

    /// Drops every calculated toolpath.
    pub fn clear_toolpaths(&mut self) {
        self.toolpaths.clear();
    }

    /// How many times the tool database has been written.
    pub fn db_revision(&self) -> u64 {
        self.db_revision
    }

    /// The tool database, reading it from disk and seeding the shipped one on
    /// the first call.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be read or written, and
    /// [`CoreError::Validation`] when it is not a database.
    pub fn database(&mut self) -> Result<&mut ToolDatabase, CoreError> {
        if self.database.is_none() {
            self.database = Some(self.store.load_or_seed()?);
        }
        self.database
            .as_mut()
            .ok_or_else(|| CoreError::validation("the tool database could not be opened"))
    }

    /// Writes the database back to its file and answers the new revision.
    fn save_database(&mut self) -> Result<u64, CoreError> {
        let database = self
            .database
            .as_ref()
            .ok_or_else(|| CoreError::validation("the tool database is not open"))?;
        self.store.save(database)?;
        self.db_revision += 1;
        Ok(self.db_revision)
    }

    fn toolpath(&self, id: u64) -> Result<&Toolpath, CoreError> {
        self.toolpaths
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| CoreError::validation(format!("no toolpath with id {id}")))
    }
}

/// Runs one of this module's commands.
///
/// # Errors
/// Returns [`CoreError::UnknownCommand`] for a name this module does not
/// answer to, and whatever the command itself reports.
pub fn dispatch(state: &mut CamState, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    let args = Args::parse(name, args)?;
    match name {
        "tools.list" => tools_list(state, &args),
        "tools.get" => tools_get(state, &args),
        "tools.save" => {
            let tool: Tool = args.field("tool")?;
            let group = args.u64_or("group", ROOT_GROUP);
            let id = state.database()?.save_tool(tool, group)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "id": id, "dbRevision": revision }))
        }
        "tools.delete" => {
            let ids = args.ids()?;
            let database = state.database()?;
            let deleted: Vec<u64> = ids
                .into_iter()
                .filter(|id| database.delete_tool(*id))
                .collect();
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "deleted": deleted, "dbRevision": revision }))
        }
        "tools.move" => {
            let id = args.u64("id")?;
            let group = args.u64_or("group", ROOT_GROUP);
            state.database()?.move_tool(id, group)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.copy" => {
            let id = args.u64("id")?;
            let group = args.opt_u64("group")?;
            let new_id = state.database()?.copy_tool(id, group)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "id": new_id, "dbRevision": revision }))
        }
        "tools.import" => {
            let path = args.path("path")?;
            let group = args.u64_or("group", ROOT_GROUP);
            let text = std::fs::read_to_string(&path)
                .map_err(|error| CoreError::io(path.display(), error))?;
            let other = ToolDatabase::from_json(&text)?;
            let mode = args.string_or("mode", "import");
            let reply = match mode.as_str() {
                "import" => {
                    let ids = state.database()?.import(&other, group)?;
                    json!({ "toolIds": ids, "updated": [] })
                }
                "mergeOverwrite" | "mergeKeep" => {
                    let mode = if mode == "mergeOverwrite" {
                        MergeMode::Overwrite
                    } else {
                        MergeMode::KeepExisting
                    };
                    let report = state.database()?.merge(&other, mode)?;
                    json!({ "toolIds": report.added, "updated": report.updated })
                }
                other => {
                    return Err(CoreError::invalid_args(
                        "tools.import",
                        format!("unknown import mode {other}"),
                    ))
                }
            };
            let revision = state.save_database()?;
            let mut reply = reply;
            reply["dbRevision"] = json!(revision);
            KernelReply::ok(&reply)
        }
        "tools.addDefaults" => {
            let defaults = ToolDatabase::shipped_default()?;
            let ids = state.database()?.add_missing_defaults(&defaults)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "toolIds": ids, "dbRevision": revision }))
        }
        "tools.export" => {
            let path = args.path("path")?;
            let group = args.u64_or("group", ROOT_GROUP);
            let tool_ids: Vec<u64> = args
                .get("toolIds")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            let database = state.database()?;
            let exported = if tool_ids.is_empty() {
                database.export_group(group)?
            } else {
                database.export_tools(&tool_ids)?
            };
            let text = exported.to_json()?;
            write_file(&path, &text)?;
            KernelReply::ok(&json!({ "path": path.display().to_string() }))
        }
        "tools.groups.list" => {
            let database = state.database()?;
            let ids: Vec<u64> = database.groups().map(|group| group.id).collect();
            let groups: Vec<Value> = ids.iter().map(|id| group_json(database, *id)).collect();
            KernelReply::ok(&groups)
        }
        "tools.groups.add" => {
            let name = args.string("name")?;
            let parent = args.opt_u64("parent")?;
            let id = state.database()?.add_group(&name, parent)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "id": id, "dbRevision": revision }))
        }
        "tools.groups.rename" => {
            let id = args.u64("id")?;
            let name = args.string("name")?;
            state.database()?.rename_group(id, &name)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.groups.move" => {
            let id = args.u64("id")?;
            let parent = args.opt_u64("parent")?;
            state.database()?.move_group(id, parent)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.groups.delete" => {
            let id = args.u64("id")?;
            let move_to = args.opt_u64("moveTo")?;
            state.database()?.delete_group(id, move_to)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.materials.list" => {
            let materials: Vec<String> = state.database()?.materials().cloned().collect();
            KernelReply::ok(&materials)
        }
        "tools.materials.add" => {
            let name = args.string("name")?;
            let added = state.database()?.add_material(&name)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "added": added, "dbRevision": revision }))
        }
        "tools.materials.rename" => {
            let from = args.string("from")?;
            let to = args.string("to")?;
            state.database()?.rename_material(&from, &to)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.materials.delete" => {
            let name = args.string("name")?;
            let removed = state.database()?.remove_material(&name);
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "removed": removed, "dbRevision": revision }))
        }
        "tools.machines.list" => {
            let machines: Vec<String> = state.database()?.machines().cloned().collect();
            KernelReply::ok(&machines)
        }
        "tools.machines.add" => {
            let name = args.string("name")?;
            let added = state.database()?.add_machine(&name)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "added": added, "dbRevision": revision }))
        }
        "tools.machines.rename" => {
            let from = args.string("from")?;
            let to = args.string("to")?;
            state.database()?.rename_machine(&from, &to)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.machines.delete" => {
            let name = args.string("name")?;
            let removed = state.database()?.remove_machine(&name);
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "removed": removed, "dbRevision": revision }))
        }
        "tools.cuttingData.set" => cutting_data_set(state, &args),
        "tools.place" => {
            let id = args.u64("id")?;
            let before = args.opt_u64("before")?;
            let database = state.database()?;
            if args.string_or("kind", "tool") == "group" {
                database.place_group(id, args.opt_u64("parent")?, before)?;
            } else {
                database.place_tool(id, args.u64_or("group", ROOT_GROUP), before)?;
            }
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.materials.activate" => {
            let name = args.string_or("name", "");
            state.database()?.set_active_material(&name)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "dbRevision": revision }))
        }
        "tools.materials.duplicate" => {
            let from = args.string("from")?;
            let to = args.string("to")?;
            let copy = args.bool_or("copyCuttingData", false);
            let copied = state.database()?.duplicate_material(&from, &to, copy)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "copied": copied, "dbRevision": revision }))
        }
        "tools.nameFormats.list" => {
            let formats = state.database()?.name_formats();
            KernelReply::ok(&json!({ "formats": formats, "variables": naming::VARIABLES }))
        }
        "tools.nameFormats.set" => {
            let kind = args.string("kind")?;
            let format = args.string_or("format", "");
            let update = args.bool_or("updateExisting", false);
            let renamed = state.database()?.set_name_format(&kind, &format, update)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "renamed": renamed, "dbRevision": revision }))
        }
        "tools.previewName" => {
            let tool: Tool = args.field("tool")?;
            let machine = args.string_or("machine", "");
            let database = state.database()?;
            let material = args.string_or("material", database.active_material());
            let resolved = database.resolve_tool(&tool, &machine, &material);
            KernelReply::ok(&json!({ "name": resolved.name }))
        }
        "tools.cuttingData.delete" => {
            let id = args.u64("id")?;
            let machine = args.string_or("machine", "");
            let material = args.string_or("material", "");
            let database = state.database()?;
            let tool = database
                .tool(id)
                .cloned()
                .ok_or_else(|| CoreError::validation(format!("no tool with id {id}")))?;
            let group = database.group_of(id).unwrap_or(ROOT_GROUP);
            let mut tool = tool;
            let removed = tool.remove_cutting_data(&machine, &material);
            database.save_tool(tool, group)?;
            let revision = state.save_database()?;
            KernelReply::ok(&json!({ "removed": removed, "dbRevision": revision }))
        }
        "posts.list" => {
            let summaries = state.posts.summaries()?;
            KernelReply::ok(&summaries)
        }
        "posts.get" => {
            let id = args.string("id")?;
            let post = state.posts.get(&id)?;
            KernelReply::ok(&post)
        }
        "posts.import" => {
            let path = args.path("path")?;
            let id = state.posts.import(&path)?;
            KernelReply::ok(&json!({ "id": id }))
        }
        "posts.text" => {
            let id = args.string("id")?;
            let (text, path) = state.posts.text(&id)?;
            KernelReply::ok(&json!({ "id": id, "text": text, "path": path }))
        }
        "posts.changelog" => {
            let id = args.string("id")?;
            let (text, _) = state.posts.text(&id)?;
            let entries = crate::post::changelog::change_log(&text);
            KernelReply::ok(&json!({ "id": id, "entries": entries }))
        }
        "posts.remove" => {
            let id = args.string("id")?;
            state.posts.remove(&id)?;
            KernelReply::ok(&json!({ "removed": id }))
        }
        "posts.customize" => {
            let id = args.string("id")?;
            let new_id = state.posts.customize(&id)?;
            KernelReply::ok(&json!({ "id": new_id }))
        }
        "posts.folder" => KernelReply::ok(&json!({
            "path": state.posts.user_dir().display().to_string(),
        })),
        "cam.save" => cam_save(state, &args),
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The whole database in tree order: groups depth first, each followed by
/// its tools in list order. Every tool also carries `displayName` and
/// `resolved`, the tool as a toolpath cuts with it on the machine and
/// material asked for (the active material when none is given).
fn tools_list(state: &mut CamState, args: &Args) -> Result<KernelReply, CoreError> {
    let revision = state.db_revision();
    let machine = args.string_or("machine", "");
    let database = state.database()?;
    let material = args.string_or("material", database.active_material());
    let mut group_ids = Vec::new();
    let mut stack: Vec<u64> = database
        .child_groups(ROOT_GROUP)
        .map(|g| g.id)
        .collect::<Vec<_>>();
    stack.reverse();
    while let Some(id) = stack.pop() {
        if group_ids.contains(&id) {
            continue;
        }
        group_ids.push(id);
        let mut children: Vec<u64> = database.child_groups(id).map(|g| g.id).collect();
        children.reverse();
        stack.extend(children);
    }
    let groups: Vec<Value> = group_ids
        .iter()
        .map(|id| group_json(database, *id))
        .collect();
    let mut tools: Vec<Value> = Vec::with_capacity(database.tool_count());
    for group in std::iter::once(ROOT_GROUP).chain(group_ids.iter().copied()) {
        for tool in database.tools_in(group) {
            let mut value = tool_json(tool, group);
            let resolved = database.resolve_tool(tool, &machine, &material);
            if let Some(object) = value.as_object_mut() {
                object.insert("displayName".to_string(), json!(resolved.name));
                object.insert("chipLoad".to_string(), json!(resolved.chip_load()));
                object.insert("resolved".to_string(), serde_json::to_value(&resolved)?);
            }
            tools.push(value);
        }
    }
    let materials: Vec<String> = database.materials().cloned().collect();
    let machines: Vec<String> = database.machines().cloned().collect();
    KernelReply::ok(&json!({
        "groups": groups,
        "tools": tools,
        "materials": materials,
        "machines": machines,
        "activeMaterial": database.active_material(),
        "machine": machine,
        "material": material,
        "dbRevision": revision,
    }))
}

fn tools_get(state: &mut CamState, args: &Args) -> Result<KernelReply, CoreError> {
    let id = args.u64("id")?;
    let machine = args.string_or("machine", "");
    let database = state.database()?;
    let material = args.string_or("material", database.active_material());
    let group = database.group_of(id).unwrap_or(ROOT_GROUP);
    let resolved = database.resolve(id, &machine, &material)?;
    KernelReply::ok(&tool_json(&resolved, group))
}

fn cutting_data_set(state: &mut CamState, args: &Args) -> Result<KernelReply, CoreError> {
    let id = args.u64("id")?;
    let database = state.database()?;
    let mut tool = database
        .tool(id)
        .cloned()
        .ok_or_else(|| CoreError::validation(format!("no tool with id {id}")))?;
    let group = database.group_of(id).unwrap_or(ROOT_GROUP);
    let feed_units: FeedUnits = match args.get("feedUnits") {
        Some(found) => serde_json::from_value(found.clone())
            .map_err(|error| CoreError::invalid_args("tools.cuttingData.set", error))?,
        None => tool.feed_units,
    };
    let data = CuttingData {
        machine: args.string_or("machine", ""),
        material: args.string_or("material", ""),
        feed_units,
        feed_rate: args.f64_or("feedRate", tool.feed_rate),
        plunge_rate: args.f64_or("plungeRate", tool.plunge_rate),
        spindle_speed: args.f64_or("spindleSpeed", tool.spindle_speed),
        pass_depth: args.f64_or("passDepth", tool.pass_depth),
        stepover: args.f64_or("stepover", tool.stepover),
        number: args
            .get("number")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
    };
    tool.set_cutting_data(data);
    database.save_tool(tool, group)?;
    let revision = state.save_database()?;
    KernelReply::ok(&json!({ "dbRevision": revision }))
}

fn cam_save(state: &mut CamState, args: &Args) -> Result<KernelReply, CoreError> {
    if args.bool_or("outputDirectToMachine", false) {
        return Err(CoreError::unsupported(
            "sending a toolpath straight to the machine is not in this build, save a file instead",
        ));
    }
    let post = state.posts.get(&args.string("post")?)?;
    // An air or scrap cut is posted in place of the real program, its file
    // and header labelled so it is never mistaken for it.
    let trial: Option<TrialCut> = match args.get("trialCut").filter(|t| !t.is_null()) {
        Some(value) => {
            let trial: TrialCut = serde_json::from_value(value.clone())
                .map_err(|e| CoreError::invalid_args("cam.save", e))?;
            trial.validate()?;
            Some(trial)
        }
        None => None,
    };
    let path = match trial {
        Some(trial) => trial_path(&args.path("path")?, trial.suffix()),
        None => args.path("path")?,
    };
    // `output` picks the Save Toolpaths option; older callers send the two
    // flags instead.
    let output: SaveOutput = match args.get("output").filter(|o| !o.is_null()) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|e| CoreError::invalid_args("cam.save", e))?,
        None if args.bool_or("oneFilePerToolpath", false) => SaveOutput::MultipleFiles,
        None => SaveOutput::OneFile,
    };
    let all_visible = args.bool_or("outputAllVisible", false);
    let ids: Vec<u64> = if all_visible {
        state
            .toolpaths()
            .iter()
            .filter(|t| t.visible)
            .map(|t| t.id)
            .collect()
    } else {
        match args.get("toolpathIds").and_then(Value::as_array) {
            Some(list) => list.iter().filter_map(Value::as_u64).collect(),
            None => Vec::new(),
        }
    };
    if ids.is_empty() {
        return Err(CoreError::validation("no toolpaths were chosen to save"));
    }
    let toolpaths: Vec<Toolpath> = ids
        .iter()
        .map(|id| state.toolpath(*id).cloned())
        .collect::<Result<Vec<_>, _>>()?;
    let toolpaths = match trial {
        Some(trial) => trial.apply(&toolpaths, args.f64_or("materialTop", 0.0)),
        None => toolpaths,
    };
    let files = save::plan(
        &toolpaths,
        output,
        args.bool_or("groupWherePossible", false),
    )?;
    for file in &files {
        // Each side is output in its own job space, so one file holds one side.
        let side = file.toolpaths.first().map(|t| t.side);
        if file.toolpaths.iter().any(|t| Some(t.side) != side) {
            return Err(CoreError::validation(
                "the top and bottom sides are cut separately: save each side's toolpaths to its own file",
            ));
        }
        // Each sheet is its own board, placed at the same origin: one file
        // holding two sheets would cut both boards' parts into one.
        let sheet = file.toolpaths.first().map(|t| t.sheet);
        if file.toolpaths.iter().any(|t| Some(t.sheet) != sheet) {
            return Err(CoreError::validation(
                "each sheet is cut from its own material: save each sheet's toolpaths to its own file",
            ));
        }
        save::check_tool_changes(&post, &file.toolpaths)?;
    }
    // A rotary job carries its blank; the kernel adds it from the document.
    let wrap = match args.get("rotary").filter(|r| !r.is_null()) {
        Some(value) => {
            let rotary: crate::doc::RotarySetup = serde_json::from_value(value.clone())
                .map_err(|e| CoreError::invalid_args("cam.save", e))?;
            // Machine Configuration says how this machine's axis is wired.
            let wiring = args
                .get("rotaryWiring")
                .filter(|w| !w.is_null())
                .map(|w| serde_json::from_value(w.clone()))
                .transpose()
                .map_err(|e| CoreError::invalid_args("cam.save", e))?
                .unwrap_or_default();
            Some(Wrap::for_job(&post, &rotary)?.with_wiring(wiring))
        }
        None => None,
    };
    let first = toolpaths.first();
    let job = PostJob {
        file_name: args.string_or("fileName", &file_name_of(&path)),
        path_name: path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        date: args.string_or("date", ""),
        time: args.string_or("time", ""),
        notes: args.string_or("notes", ""),
        home: first.map(|t| t.home).unwrap_or_default(),
        safe_z: first.map_or(5.0, |t| t.safe_z),
        m6_applies_length: args.bool_or("m6AppliesLength", true),
        label: trial.map(|t| t.label().to_string()).unwrap_or_default(),
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("toolpath")
        .to_string();
    let directory = path.parent().map(PathBuf::from).unwrap_or_default();
    let target_for = |suffix: &str| -> PathBuf {
        if suffix.is_empty() {
            with_extension(&path, &post.file_extension)
        } else {
            directory.join(format!("{stem}{suffix}.{}", post.file_extension))
        }
    };
    // A post with TAPE_SPLITTING may write a program as several files,
    // named from the target's stem as the post asks.
    let write = |target: &PathBuf, toolpaths: &[Toolpath]| -> Result<Vec<String>, CoreError> {
        let job = PostJob {
            file_name: file_name_of(target),
            ..job.clone()
        };
        let pieces = post_process_pieces(&post, &job, toolpaths, wrap)?;
        let stem = target
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("toolpath")
            .to_string();
        let folder = target.parent().map(PathBuf::from).unwrap_or_default();
        let mut paths = Vec::with_capacity(pieces.len());
        for (index, text) in pieces.iter().enumerate() {
            let named = match (&post.tape_splitting, pieces.len()) {
                (Some(split), n) if n > 1 => split
                    .file_name(index, &stem, &post.file_extension)
                    .map(|name| folder.join(name)),
                _ => None,
            };
            let piece_target = named.unwrap_or_else(|| target.clone());
            write_file(&piece_target, text)?;
            paths.push(piece_target.display().to_string());
        }
        Ok(paths)
    };

    let tiling = match args.get("tiling").filter(|t| !t.is_null()) {
        Some(spec) => {
            let spec: TileSpec = serde_json::from_value(spec.clone())
                .map_err(|e| CoreError::invalid_args("cam.save", e))?;
            let bounds = args
                .get("jobBounds")
                .and_then(Value::as_array)
                .map(|b| b.iter().filter_map(Value::as_f64).collect::<Vec<f64>>())
                .filter(|b| b.len() == 4)
                .ok_or_else(|| CoreError::validation("tiling needs the job size"))?;
            let job_box = BBox::new(
                crate::geom::Point::new(bounds[0], bounds[1]),
                crate::geom::Point::new(bounds[2], bounds[3]),
            );
            Some(tiling::tiles(job_box, &spec)?)
        }
        None => None,
    };

    let mut written = Vec::new();
    let mut tiles_written = Vec::new();
    for file in &files {
        let suffix = file
            .suffix
            .as_ref()
            .map(|s| format!("_{s}"))
            .unwrap_or_default();
        let Some(tiles) = &tiling else {
            let whole: Vec<Toolpath> = file.toolpaths.iter().map(|t| (*t).clone()).collect();
            written.extend(write(&target_for(&suffix), &whole)?);
            continue;
        };
        for tile in tiles {
            let clipped: Vec<Toolpath> = file
                .toolpaths
                .iter()
                .filter_map(|t| tiling::clip(t, tile))
                .collect();
            if clipped.is_empty() {
                continue;
            }
            let target = target_for(&format!(
                "{suffix}_tile_{}_{}",
                tile.row + 1,
                tile.column + 1
            ));
            let paths = write(&target, &clipped)?;
            tiles_written.push(json!({
                "row": tile.row,
                "column": tile.column,
                "shift": [tile.shift.0, tile.shift.1],
                "path": paths.first().cloned().unwrap_or_default(),
            }));
            written.extend(paths);
        }
    }
    if tiling.is_some() {
        if written.is_empty() {
            return Err(CoreError::validation("no tile has anything to cut"));
        }
        return KernelReply::ok(&json!({ "paths": written, "tiles": tiles_written }));
    }
    KernelReply::ok(&json!({ "paths": written }))
}

/// `path` with `suffix` added to its file name before the extension,
/// unless the name already ends with it.
fn trial_path(path: &std::path::Path, suffix: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("toolpath");
    if stem.ends_with(suffix) {
        return path.to_path_buf();
    }
    let name = match path.extension().and_then(|e| e.to_str()) {
        Some(extension) => format!("{stem}{suffix}.{extension}"),
        None => format!("{stem}{suffix}"),
    };
    path.with_file_name(name)
}

/// JSON for one tool, with the group it sits in and the two values the tool
/// list shows that are not fields of their own.
fn tool_json(tool: &Tool, group: u64) -> Value {
    let mut value = serde_json::to_value(tool).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("group".to_string(), json!(group));
        object.insert(
            "toolTypeName".to_string(),
            json!(tool.tool_type.type_name()),
        );
        object.insert("diameter".to_string(), json!(tool.diameter()));
    }
    value
}

/// JSON for one group, with the path of names down to it.
fn group_json(database: &ToolDatabase, id: u64) -> Value {
    let path = database.group_path(id);
    match database.group(id) {
        Some(group) => json!({
            "id": group.id,
            "name": group.name,
            "parent": group.parent,
            "order": group.order,
            "path": path,
        }),
        None => Value::Null,
    }
}

fn write_file(path: &std::path::Path, text: &str) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CoreError::io(parent.display(), error))?;
        }
    }
    std::fs::write(path, text).map_err(|error| CoreError::io(path.display(), error))
}

fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn with_extension(path: &std::path::Path, extension: &str) -> PathBuf {
    match path.extension() {
        Some(_) => path.to_path_buf(),
        None => path.with_extension(extension),
    }
}

/// A thin reader over the command's JSON arguments.
struct Args {
    command: String,
    value: Value,
}

impl Args {
    fn parse(command: &str, args: &str) -> Result<Args, CoreError> {
        let text = if args.trim().is_empty() { "null" } else { args };
        let value: Value =
            serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))?;
        Ok(Args {
            command: command.to_string(),
            value,
        })
    }

    fn missing(&self, field: &str) -> CoreError {
        CoreError::invalid_args(&self.command, format!("{field} is missing"))
    }

    fn get(&self, field: &str) -> Option<&Value> {
        self.value.get(field).filter(|found| !found.is_null())
    }

    fn u64(&self, field: &str) -> Result<u64, CoreError> {
        self.get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| self.missing(field))
    }

    fn u64_or(&self, field: &str, fallback: u64) -> u64 {
        self.get(field).and_then(Value::as_u64).unwrap_or(fallback)
    }

    fn opt_u64(&self, field: &str) -> Result<Option<u64>, CoreError> {
        match self.get(field) {
            Some(found) => Ok(Some(found.as_u64().ok_or_else(|| self.missing(field))?)),
            None => Ok(None),
        }
    }

    fn f64_or(&self, field: &str, fallback: f64) -> f64 {
        self.get(field).and_then(Value::as_f64).unwrap_or(fallback)
    }

    fn string(&self, field: &str) -> Result<String, CoreError> {
        self.get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| self.missing(field))
    }

    fn string_or(&self, field: &str, fallback: &str) -> String {
        self.get(field)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    }

    fn bool_or(&self, field: &str, fallback: bool) -> bool {
        self.get(field).and_then(Value::as_bool).unwrap_or(fallback)
    }

    fn path(&self, field: &str) -> Result<PathBuf, CoreError> {
        Ok(PathBuf::from(self.string(field)?))
    }

    fn ids(&self) -> Result<Vec<u64>, CoreError> {
        if let Some(list) = self.get("ids").and_then(Value::as_array) {
            return Ok(list.iter().filter_map(Value::as_u64).collect());
        }
        Ok(vec![self.u64("id")?])
    }

    fn field<T: serde::de::DeserializeOwned>(&self, field: &str) -> Result<T, CoreError> {
        let found = self.get(field).ok_or_else(|| self.missing(field))?;
        serde_json::from_value(found.clone())
            .map_err(|error| CoreError::invalid_args(&self.command, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::cam::toolpath::{Move, Point3, StrategyKind};
    use crate::test_support::TempDir;

    struct Fixture {
        temp: TempDir,
        state: CamState,
    }

    impl Fixture {
        fn new(label: &str) -> Fixture {
            let temp = TempDir::new(label);
            let store = ToolStore::at(temp.path().join("tools.json"));
            let posts = PostLibrary::at(temp.path().join("posts"));
            Fixture {
                temp,
                state: CamState::new(store, posts),
            }
        }

        fn call(&mut self, name: &str, args: &str) -> Value {
            let reply = dispatch(&mut self.state, name, args)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            match reply {
                KernelReply::Json(text) => {
                    let value: Value = serde_json::from_str(&text).expect("reply is JSON");
                    assert_eq!(value["ok"], Value::Bool(true), "{value}");
                    value["result"].clone()
                }
                KernelReply::Bytes(_) => panic!("{name} answered with bytes"),
            }
        }

        fn fails(&mut self, name: &str, args: &str) -> CoreError {
            dispatch(&mut self.state, name, args).expect_err("command fails")
        }
    }

    fn toolpath(id: u64, name: &str) -> Toolpath {
        let tool = Tool {
            name: "End Mill 6 mm".to_string(),
            number: 1,
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        let mut tp = Toolpath::new(id, name, tool, StrategyKind::Profile);
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -2.0)),
            Move::feed(Point3::new(30.0, 0.0, -2.0)),
            Move::rapid(Point3::new(30.0, 0.0, 5.0)),
        ];
        tp.recompute_bounds();
        tp
    }

    #[test]
    fn every_listed_command_is_answered() {
        let mut fixture = Fixture::new("cmd-names");
        for name in COMMANDS {
            let error = dispatch(&mut fixture.state, name, "{}").err();
            assert!(
                !matches!(error, Some(CoreError::UnknownCommand(_))),
                "{name} is listed but not answered"
            );
        }
        assert!(handles("cam.save"));
        assert!(!handles("doc.new"));
    }

    #[test]
    fn tools_list_seeds_the_default_database() {
        let mut fixture = Fixture::new("cmd-list");
        let result = fixture.call("tools.list", "{}");
        assert_eq!(result["tools"].as_array().map(Vec::len), Some(20));
        assert!(result["groups"].as_array().is_some_and(|g| g.len() >= 10));
        assert_eq!(result["dbRevision"], 0);
        let first = &result["tools"][0];
        assert!(first["group"].as_u64().is_some());
        assert!(first["toolTypeName"].as_str().is_some());
        assert!(first["diameter"].as_f64().is_some());
        assert!(fixture.state.store().path().exists());
    }

    #[test]
    fn a_tool_is_saved_fetched_and_deleted() {
        let mut fixture = Fixture::new("cmd-tool");
        let tool = Tool {
            name: "Mine".to_string(),
            tool_type: ToolGeometry::BallNose { diameter: 4.0 },
            ..Tool::default()
        };
        let args = json!({ "tool": tool }).to_string();
        let saved = fixture.call("tools.save", &args);
        let id = saved["id"].as_u64().expect("an id");
        assert_eq!(saved["dbRevision"], 1);
        let fetched = fixture.call("tools.get", &json!({ "id": id }).to_string());
        assert_eq!(fetched["name"], "Mine");
        assert_eq!(fetched["toolTypeName"], "ballNose");
        let deleted = fixture.call("tools.delete", &json!({ "id": id }).to_string());
        assert_eq!(deleted["deleted"], json!([id]));
        assert!(fixture
            .fails("tools.get", &json!({ "id": id }).to_string())
            .to_string()
            .contains("no tool"));
    }

    #[test]
    fn groups_are_added_renamed_moved_and_deleted() {
        let mut fixture = Fixture::new("cmd-groups");
        let added = fixture.call("tools.groups.add", &json!({ "name": "Mine" }).to_string());
        let id = added["id"].as_u64().expect("an id");
        let child = fixture.call(
            "tools.groups.add",
            &json!({ "name": "Small", "parent": id }).to_string(),
        );
        let child_id = child["id"].as_u64().expect("an id");
        fixture.call(
            "tools.groups.rename",
            &json!({ "id": child_id, "name": "Tiny" }).to_string(),
        );
        let groups = fixture.call("tools.groups.list", "{}");
        let found = groups
            .as_array()
            .expect("a list")
            .iter()
            .find(|g| g["id"] == json!(child_id))
            .expect("the child is listed");
        assert_eq!(found["path"], json!(["Mine", "Tiny"]));
        fixture.call("tools.groups.move", &json!({ "id": child_id }).to_string());
        fixture.call(
            "tools.groups.delete",
            &json!({ "id": id, "moveTo": 0 }).to_string(),
        );
        assert!(fixture
            .fails(
                "tools.groups.rename",
                &json!({ "id": id, "name": "x" }).to_string()
            )
            .to_string()
            .contains("no tool group"));
    }

    #[test]
    fn a_tool_moves_and_copies_between_groups() {
        let mut fixture = Fixture::new("cmd-move");
        let list = fixture.call("tools.list", "{}");
        let id = list["tools"][0]["id"].as_u64().expect("an id");
        let group = fixture.call(
            "tools.groups.add",
            &json!({ "name": "Favourites" }).to_string(),
        )["id"]
            .as_u64()
            .expect("an id");
        fixture.call(
            "tools.move",
            &json!({ "id": id, "group": group }).to_string(),
        );
        let moved = fixture.call("tools.get", &json!({ "id": id }).to_string());
        assert_eq!(moved["group"], json!(group));
        let copy = fixture.call("tools.copy", &json!({ "id": id }).to_string());
        let copy_id = copy["id"].as_u64().expect("an id");
        let copied = fixture.call("tools.get", &json!({ "id": copy_id }).to_string());
        assert!(copied["name"].as_str().is_some_and(|n| n.ends_with("copy")));
    }

    #[test]
    fn materials_and_machines_are_managed_through_commands() {
        let mut fixture = Fixture::new("cmd-materials");
        fixture.call(
            "tools.materials.add",
            &json!({ "name": "Walnut" }).to_string(),
        );
        let materials = fixture.call("tools.materials.list", "{}");
        assert!(materials
            .as_array()
            .expect("a list")
            .contains(&json!("Walnut")));
        fixture.call(
            "tools.materials.rename",
            &json!({ "from": "Walnut", "to": "Black Walnut" }).to_string(),
        );
        let removed = fixture.call(
            "tools.materials.delete",
            &json!({ "name": "Black Walnut" }).to_string(),
        );
        assert_eq!(removed["removed"], Value::Bool(true));
        fixture.call(
            "tools.machines.add",
            &json!({ "name": "Big Router" }).to_string(),
        );
        let machines = fixture.call("tools.machines.list", "{}");
        assert!(machines
            .as_array()
            .expect("a list")
            .contains(&json!("Big Router")));
        fixture.call(
            "tools.machines.rename",
            &json!({ "from": "Big Router", "to": "Router" }).to_string(),
        );
        fixture.call(
            "tools.machines.delete",
            &json!({ "name": "Router" }).to_string(),
        );
    }

    #[test]
    fn cutting_data_is_set_read_back_and_removed() {
        let mut fixture = Fixture::new("cmd-cutting");
        let list = fixture.call("tools.list", "{}");
        let id = list["tools"][0]["id"].as_u64().expect("an id");
        fixture.call("tools.materials.add", &json!({ "name": "Oak" }).to_string());
        fixture.call(
            "tools.machines.add",
            &json!({ "name": "Router" }).to_string(),
        );
        fixture.call(
            "tools.cuttingData.set",
            &json!({
                "id": id, "machine": "Router", "material": "Oak",
                "feedUnits": "mmPerMin", "feedRate": 1234.0, "plungeRate": 321.0,
                "spindleSpeed": 15000.0, "passDepth": 2.0, "stepover": 1.5
            })
            .to_string(),
        );
        let scoped = fixture.call(
            "tools.get",
            &json!({ "id": id, "machine": "Router", "material": "Oak" }).to_string(),
        );
        assert_eq!(scoped["feedRate"], json!(1234.0));
        let plain = fixture.call("tools.get", &json!({ "id": id }).to_string());
        assert_ne!(plain["feedRate"], json!(1234.0));
        let removed = fixture.call(
            "tools.cuttingData.delete",
            &json!({ "id": id, "machine": "Router", "material": "Oak" }).to_string(),
        );
        assert_eq!(removed["removed"], Value::Bool(true));
    }

    #[test]
    fn the_database_is_exported_and_imported() {
        let mut fixture = Fixture::new("cmd-port");
        let path = fixture.temp.path().join("out").join("tools.json");
        let exported = fixture.call(
            "tools.export",
            &json!({ "path": path.display().to_string() }).to_string(),
        );
        assert!(path.exists(), "{exported}");
        let before = fixture.call("tools.list", "{}")["tools"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default();
        let imported = fixture.call(
            "tools.import",
            &json!({ "path": path.display().to_string() }).to_string(),
        );
        assert_eq!(imported["toolIds"].as_array().map(Vec::len), Some(before));
        let after = fixture.call("tools.list", "{}")["tools"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default();
        assert_eq!(after, before * 2);
    }

    #[test]
    fn a_merge_import_and_a_single_tool_export() {
        let mut fixture = Fixture::new("cmd-merge");
        let list = fixture.call("tools.list", "{}");
        let first = list["tools"][0]["id"].as_u64().expect("a tool");
        let path = fixture.temp.path().join("one.json");
        fixture.call(
            "tools.export",
            &json!({ "path": path.display().to_string(), "toolIds": [first] }).to_string(),
        );
        let merged = fixture.call(
            "tools.import",
            &json!({ "path": path.display().to_string(), "mode": "mergeKeep" }).to_string(),
        );
        // The exported tool sits at the top level there, so it is new here.
        assert_eq!(merged["toolIds"].as_array().map(Vec::len), Some(1));
        let again = fixture.call(
            "tools.import",
            &json!({ "path": path.display().to_string(), "mode": "mergeOverwrite" }).to_string(),
        );
        assert_eq!(again["updated"].as_array().map(Vec::len), Some(1));
        assert_eq!(again["toolIds"].as_array().map(Vec::len), Some(0));
        let error = fixture.fails(
            "tools.import",
            &json!({ "path": path.display().to_string(), "mode": "sideways" }).to_string(),
        );
        assert!(matches!(error, CoreError::InvalidArgs { .. }), "{error}");
    }

    #[test]
    fn the_list_resolves_tools_for_the_active_material() {
        let mut fixture = Fixture::new("cmd-resolve");
        let list = fixture.call("tools.list", "{}");
        let id = list["tools"][0]["id"].as_u64().expect("a tool");
        fixture.call("tools.materials.add", &json!({ "name": "Oak" }).to_string());
        fixture.call(
            "tools.cuttingData.set",
            &json!({ "id": id, "machine": "", "material": "Oak", "feedRate": 777.0, "number": 9 })
                .to_string(),
        );
        fixture.call(
            "tools.materials.activate",
            &json!({ "name": "Oak" }).to_string(),
        );
        let list = fixture.call("tools.list", "{}");
        assert_eq!(list["activeMaterial"], "Oak");
        let tool = list["tools"]
            .as_array()
            .and_then(|all| all.iter().find(|t| t["id"] == json!(id)))
            .cloned()
            .expect("the tool");
        assert_eq!(tool["resolved"]["feedRate"], json!(777.0));
        assert_eq!(tool["resolved"]["number"], json!(9));
        assert_ne!(tool["feedRate"], json!(777.0));
        let got = fixture.call("tools.get", &json!({ "id": id }).to_string());
        assert_eq!(got["feedRate"], json!(777.0));
        fixture.call(
            "tools.materials.duplicate",
            &json!({ "from": "Oak", "to": "Ash", "copyCuttingData": true }).to_string(),
        );
        let got = fixture.call(
            "tools.get",
            &json!({ "id": id, "material": "Ash" }).to_string(),
        );
        assert_eq!(got["feedRate"], json!(777.0));
        assert!(fixture
            .fails(
                "tools.materials.activate",
                &json!({ "name": "Gone" }).to_string()
            )
            .to_string()
            .contains("Gone"));
    }

    #[test]
    fn name_formats_and_previews_come_from_the_kernel() {
        let mut fixture = Fixture::new("cmd-names-format");
        let formats = fixture.call("tools.nameFormats.list", "{}");
        assert!(formats["formats"]["vBit"].as_str().is_some());
        assert!(formats["variables"]
            .as_array()
            .is_some_and(|v| v.iter().any(|n| n == "Diameter")));
        let tool = Tool {
            name: "{Tool Type|U} {Diameter}".to_string(),
            ..Tool::default()
        };
        let preview = fixture.call("tools.previewName", &json!({ "tool": tool }).to_string());
        assert_eq!(preview["name"], "END MILL 6");
        let set = fixture.call(
            "tools.nameFormats.set",
            &json!({ "kind": "drill", "format": "Drill {Diameter}", "updateExisting": true })
                .to_string(),
        );
        assert_eq!(set["renamed"].as_array().map(Vec::len), Some(2));
        let list = fixture.call("tools.list", "{}");
        assert!(list["tools"]
            .as_array()
            .is_some_and(|all| all.iter().any(|t| t["displayName"] == "Drill 3")));
    }

    #[test]
    fn tools_and_groups_are_placed_in_order() {
        let mut fixture = Fixture::new("cmd-place");
        let list = fixture.call("tools.list", "{}");
        let groups = list["groups"].as_array().cloned().unwrap_or_default();
        let top: Vec<u64> = groups
            .iter()
            .filter(|g| g["parent"].is_null())
            .filter_map(|g| g["id"].as_u64())
            .collect();
        assert!(top.len() >= 2);
        fixture.call(
            "tools.place",
            &json!({ "kind": "group", "id": top[1], "parent": null, "before": top[0] }).to_string(),
        );
        let list = fixture.call("tools.list", "{}");
        let first_top = list["groups"]
            .as_array()
            .and_then(|all| all.iter().find(|g| g["parent"].is_null()))
            .and_then(|g| g["id"].as_u64());
        assert_eq!(first_top, Some(top[1]));
        let tool = list["tools"][0]["id"].as_u64().expect("tool");
        fixture.call(
            "tools.place",
            &json!({ "id": tool, "group": 0 }).to_string(),
        );
        let list = fixture.call("tools.list", "{}");
        // Tools at the top level come first in the list.
        assert_eq!(list["tools"][0]["id"], json!(tool));
        assert_eq!(list["tools"][0]["group"], json!(0));
    }

    #[test]
    fn posts_are_viewed_customized_and_removed() {
        let mut fixture = Fixture::new("cmd-post-manage");
        let text = fixture.call("posts.text", &json!({ "id": "mach3_mm" }).to_string());
        assert!(text["text"]
            .as_str()
            .is_some_and(|t| t.contains("POST_NAME")));
        let log = fixture.call("posts.changelog", &json!({ "id": "mach3_mm" }).to_string());
        assert_eq!(log["entries"][0]["who"], "CarveStudio");
        assert_eq!(log["entries"][0]["what"], "First version.");
        let copy = fixture.call("posts.customize", &json!({ "id": "mach3_mm" }).to_string());
        let id = copy["id"].as_str().expect("id").to_string();
        let listed = fixture.call("posts.list", "{}");
        let entry = listed
            .as_array()
            .and_then(|all| all.iter().find(|p| p["id"] == json!(id)))
            .cloned()
            .expect("the copy is listed");
        assert!(entry["sourcePath"].as_str().is_some_and(|p| !p.is_empty()));
        fixture.call("posts.remove", &json!({ "id": id }).to_string());
        assert!(fixture
            .fails("posts.remove", &json!({ "id": "mach3_mm" }).to_string())
            .to_string()
            .contains("cannot be removed"));
        let folder = fixture.call("posts.folder", "{}");
        assert!(folder["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("posts")));
    }

    #[test]
    fn posts_are_listed_fetched_and_imported() {
        let mut fixture = Fixture::new("cmd-posts");
        let posts = fixture.call("posts.list", "{}");
        let list = posts.as_array().expect("a list");
        assert_eq!(list.len(), crate::post::SHIPPED_POSTS.len());
        assert!(list.iter().any(|p| p["id"] == json!("grbl_mm")));
        let one = fixture.call("posts.get", &json!({ "id": "grbl_mm" }).to_string());
        assert_eq!(one["fileExtension"], "gcode");
        assert!(one["blocks"].is_object());
        let source = fixture.temp.path().join("mine.pp");
        std::fs::write(
            &source,
            "POST_NAME = \"Mine\"\nFILE_EXTENSION = nc\nVAR X_POSITION = [X|A|X|1.3]\n\
             begin RAPID_MOVE\n+ \"G0[X]\"\nend\nbegin FEED_MOVE\n+ \"G1[X]\"\nend\n",
        )
        .expect("fixture is written");
        let imported = fixture.call(
            "posts.import",
            &json!({ "path": source.display().to_string() }).to_string(),
        );
        assert_eq!(imported["id"], "mine");
        assert_eq!(
            fixture.call("posts.list", "{}").as_array().map(Vec::len),
            Some(crate::post::SHIPPED_POSTS.len() + 1)
        );
    }

    #[test]
    fn cam_save_writes_one_file_per_tile_with_the_cuts_moved_to_the_origin() {
        let mut fixture = Fixture::new("cmd-save-tiles");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        let path = fixture.temp.path().join("long");
        let result = fixture.call(
            "cam.save",
            &json!({
                "toolpathIds": [1], "post": "grbl_mm",
                "path": path.display().to_string(),
                "tiling": { "tileWidth": 20.0 },
                "jobBounds": [0.0, 0.0, 100.0, 100.0],
            })
            .to_string(),
        );
        let tiles = result["tiles"].as_array().expect("tiles");
        assert_eq!(tiles.len(), 2, "{result}");
        for tile in tiles {
            let written = std::path::PathBuf::from(tile["path"].as_str().expect("path"));
            assert!(written.exists(), "{written:?}");
            assert!(written
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("long_tile_1_")));
        }
    }

    #[test]
    fn cam_save_splits_a_long_program_into_the_files_the_post_names() {
        let mut fixture = Fixture::new("cmd-save-tape");
        for id in 1..=6 {
            fixture
                .state
                .set_toolpath(toolpath(id, &format!("profile {id}")));
        }
        let posts = fixture.temp.path().join("posts");
        std::fs::create_dir_all(&posts).expect("posts folder");
        let text = include_str!("../../assets/posts/grbl_mm.pp").replacen(
            "RAPID_RATE = 3000",
            "RAPID_RATE = 3000\nTAPE_SPLITTING = 30 10 \"%s_%d.gcode\" 1 YES",
            1,
        );
        std::fs::write(posts.join("tape.pp"), text).expect("post written");
        let path = fixture.temp.path().join("long");
        let result = fixture.call(
            "cam.save",
            &json!({
                "toolpathIds": [1, 2, 3, 4, 5, 6], "post": "tape",
                "path": path.display().to_string()
            })
            .to_string(),
        );
        let paths: Vec<&str> = result["paths"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(paths.len() > 1, "{result}");
        for (index, written) in paths.iter().enumerate() {
            assert!(
                written.ends_with(&format!("long_{}.gcode", index + 1)),
                "{written}"
            );
            let text = std::fs::read_to_string(written).expect("reads back");
            assert!(
                text.contains("M30"),
                "every piece ends with the footer: {text}"
            );
        }
        assert!(!fixture.temp.path().join("long.gcode").exists());
    }

    /// The lowest Z word of the feed moves (G1) in a posted program.
    fn lowest_feed_z(text: &str) -> f64 {
        text.lines()
            .map(|l| l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit()))
            .filter(|l| l.starts_with("G1"))
            .filter_map(|l| l.split_once('Z'))
            .map(|(_, rest)| {
                let number: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                number.parse::<f64>().expect("a height")
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn cam_save_posts_an_air_cut_and_a_scrap_cut_labelled_as_such() {
        let mut fixture = Fixture::new("cmd-save-trial");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        let save =
            |fixture: &mut Fixture, name: &str, trial: Value, top: f64| -> (String, String) {
                let path = fixture.temp.path().join(name);
                let result = fixture.call(
                    "cam.save",
                    &json!({
                        "toolpathIds": [1], "post": "linuxcnc_mm",
                        "path": path.display().to_string(),
                        "trialCut": trial, "materialTop": top,
                    })
                    .to_string(),
                );
                let written = result["paths"][0].as_str().expect("a path").to_string();
                let text = std::fs::read_to_string(&written).expect("reads back");
                (written, text)
            };
        // Air: the cut 2 deep goes 25 above the surface, at 27 over the
        // 2 deep line; the rapids keep their 7 clearance over it.
        let (written, text) = save(
            &mut fixture,
            "job.ngc",
            json!({"kind": "air", "height": 25.0}),
            0.0,
        );
        assert!(written.ends_with("job_AIR_CUT.ngc"), "{written}");
        assert!(
            text.lines().any(|l| l == "(AIR CUT: job_AIR_CUT.ngc)"),
            "{text}"
        );
        assert_eq!(lowest_feed_z(&text), 25.0, "{text}");
        assert!(text.contains("Z32.0000"), "{text}");
        // A name already labelled is not labelled twice; the surface of
        // 18 mm material with Z zero on the bed is at 18.
        let (written, text) = save(
            &mut fixture,
            "job_AIR_CUT.ngc",
            json!({"kind": "air", "height": 25.4}),
            18.0,
        );
        assert!(written.ends_with("/job_AIR_CUT.ngc"), "{written}");
        assert_eq!(lowest_feed_z(&text), 43.4, "{text}");
        // Scrap: 0.5 deep at the most.
        let (written, text) = save(
            &mut fixture,
            "job",
            json!({"kind": "scrap", "depth": 0.5}),
            0.0,
        );
        assert!(written.ends_with("job_SCRAP_CUT.ngc"), "{written}");
        assert!(
            text.lines().any(|l| l == "(SCRAP CUT: job_SCRAP_CUT.ngc)"),
            "{text}"
        );
        assert_eq!(lowest_feed_z(&text), -0.5, "{text}");
        // The real file says neither.
        let path = fixture.temp.path().join("real");
        let result = fixture.call(
            "cam.save",
            &json!({ "toolpathIds": [1], "post": "linuxcnc_mm", "path": path.display().to_string() })
                .to_string(),
        );
        let text = std::fs::read_to_string(result["paths"][0].as_str().expect("a path"))
            .expect("reads back");
        assert!(text.lines().any(|l| l == "(real.ngc)"), "{text}");
        assert_eq!(lowest_feed_z(&text), -2.0, "{text}");
        // No height, no trial.
        let error = dispatch(
            &mut fixture.state,
            "cam.save",
            &json!({
                "toolpathIds": [1], "post": "linuxcnc_mm",
                "path": fixture.temp.path().join("bad").display().to_string(),
                "trialCut": {"kind": "air", "height": 0.0},
            })
            .to_string(),
        );
        assert!(error.is_err());
    }

    #[test]
    fn cam_save_passes_the_tool_length_choice_to_the_post() {
        let mut fixture = Fixture::new("cmd-save-length");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        for (applies, expected) in [(true, false), (false, true)] {
            let path = fixture.temp.path().join(format!("job{applies}"));
            let result = fixture.call(
                "cam.save",
                &json!({
                    "toolpathIds": [1], "post": "uccnc_mm",
                    "path": path.display().to_string(), "m6AppliesLength": applies,
                })
                .to_string(),
            );
            let text = std::fs::read_to_string(result["paths"][0].as_str().expect("a path"))
                .expect("reads back");
            let has = text
                .lines()
                .any(|l| l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit()) == "G43H1");
            assert_eq!(has, expected, "{text}");
        }
    }

    #[test]
    fn cam_save_writes_one_file_for_the_chosen_toolpaths() {
        let mut fixture = Fixture::new("cmd-save");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        fixture.state.set_toolpath(toolpath(2, "profile 2"));
        let path = fixture.temp.path().join("job");
        let result = fixture.call(
            "cam.save",
            &json!({
                "toolpathIds": [1, 2], "post": "grbl_mm",
                "path": path.display().to_string()
            })
            .to_string(),
        );
        let paths = result["paths"].as_array().expect("a list");
        assert_eq!(paths.len(), 1);
        let written = fixture.temp.path().join("job.gcode");
        assert!(written.exists());
        let text = std::fs::read_to_string(&written).expect("the file reads back");
        assert!(text.contains("(profile 1)"), "{text}");
        assert!(text.contains("(profile 2)"), "{text}");
        assert!(text.contains("M30"), "{text}");
    }

    #[test]
    fn cam_save_wraps_a_rotary_job_through_a_rotary_post() {
        let mut fixture = Fixture::new("cmd-save-rotary");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        let path = fixture.temp.path().join("wrapped");
        let rotary = json!({ "diameter": 40.0, "axis": "x" });
        fixture.call(
            "cam.save",
            &json!({
                "toolpathIds": [1], "post": "grbl_rotary_a_mm",
                "path": path.display().to_string(), "rotary": rotary,
            })
            .to_string(),
        );
        let text = std::fs::read_to_string(fixture.temp.path().join("wrapped.gcode"))
            .expect("the file reads back");
        assert!(text.contains('A'), "{text}");
        let error = fixture.fails(
            "cam.save",
            &json!({
                "toolpathIds": [1], "post": "grbl_mm",
                "path": path.display().to_string(), "rotary": rotary,
            })
            .to_string(),
        );
        assert!(error.to_string().contains("rotary"), "{error}");
    }

    #[test]
    fn cam_save_can_write_one_file_for_each_toolpath() {
        let mut fixture = Fixture::new("cmd-save-each");
        fixture.state.set_toolpath(toolpath(1, "profile 1"));
        fixture.state.set_toolpath(toolpath(2, "pocket top"));
        let path = fixture.temp.path().join("job.gcode");
        let result = fixture.call(
            "cam.save",
            &json!({
                "toolpathIds": [1, 2], "post": "grbl_mm", "oneFilePerToolpath": true,
                "path": path.display().to_string()
            })
            .to_string(),
        );
        assert_eq!(result["paths"].as_array().map(Vec::len), Some(2));
        assert!(fixture.temp.path().join("job_profile_1.gcode").exists());
        assert!(fixture.temp.path().join("job_pocket_top.gcode").exists());
    }

    #[test]
    fn cam_save_groups_same_tool_toolpaths_and_checks_tool_changes() {
        let mut fixture = Fixture::new("cmd-save-group");
        fixture.state.set_toolpath(toolpath(1, "rough"));
        fixture.state.set_toolpath(toolpath(2, "finish"));
        let mut vbit = toolpath(3, "letters");
        vbit.tool.name = "V Bit 60".to_string();
        vbit.tool.tool_type = ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 60.0,
        };
        vbit.tool_number = 2;
        fixture.state.set_toolpath(vbit.clone());
        let path = fixture.temp.path().join("sign.gcode");
        let grouped = fixture.call(
            "cam.save",
            &json!({ "toolpathIds": [1, 2, 3], "post": "grbl_mm",
                     "output": "multipleFiles", "groupWherePossible": true,
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert_eq!(grouped["paths"].as_array().map(Vec::len), Some(2));
        let both = std::fs::read_to_string(fixture.temp.path().join("sign_1-2.gcode"))
            .expect("the grouped file");
        assert!(
            both.contains("(rough)") && both.contains("(finish)"),
            "{both}"
        );
        assert!(fixture.temp.path().join("sign_letters.gcode").exists());

        let one = fixture.call(
            "cam.save",
            &json!({ "toolpathIds": [1, 3], "post": "grbl_mm", "output": "oneFile",
                     "path": path.display().to_string() })
            .to_string(),
        );
        let text = std::fs::read_to_string(one["paths"][0].as_str().expect("path")).expect("reads");
        assert!(text.contains("Fit V Bit 60"), "a tool change: {text}");
        let no_change = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1, 3], "post": "laser_grbl", "output": "oneFile",
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert!(
            no_change.to_string().contains("cannot change tools"),
            "{no_change}"
        );
        vbit.tool_number = 1;
        fixture.state.set_toolpath(vbit);
        let clash = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1, 3], "post": "grbl_mm", "output": "oneFile",
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert!(clash.to_string().contains("tool number 1"), "{clash}");
        let selected = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1, 2], "post": "grbl_mm", "output": "selected",
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert!(selected.to_string().contains("select one"), "{selected}");
    }

    #[test]
    fn cam_save_can_take_every_visible_toolpath() {
        let mut fixture = Fixture::new("cmd-save-visible");
        fixture.state.set_toolpath(toolpath(1, "one"));
        let mut hidden = toolpath(2, "two");
        hidden.visible = false;
        fixture.state.set_toolpath(hidden);
        let path = fixture.temp.path().join("job.gcode");
        fixture.call(
            "cam.save",
            &json!({
                "post": "grbl_mm", "outputAllVisible": true,
                "path": path.display().to_string()
            })
            .to_string(),
        );
        let text = std::fs::read_to_string(&path).expect("the file reads back");
        assert!(text.contains("(one)"), "{text}");
        assert!(!text.contains("(two)"), "{text}");
    }

    #[test]
    fn cam_save_refuses_what_it_cannot_do() {
        let mut fixture = Fixture::new("cmd-save-bad");
        fixture.state.set_toolpath(toolpath(1, "one"));
        let path = fixture.temp.path().join("job.gcode").display().to_string();
        let direct = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1], "post": "grbl_mm", "path": path,
                     "outputDirectToMachine": true })
            .to_string(),
        );
        assert!(matches!(direct, CoreError::Unsupported(_)), "{direct}");
        let none = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [], "post": "grbl_mm", "path": path }).to_string(),
        );
        assert!(none.to_string().contains("no toolpaths"), "{none}");
        let missing = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [9], "post": "grbl_mm", "path": path }).to_string(),
        );
        assert!(
            missing.to_string().contains("no toolpath with id 9"),
            "{missing}"
        );
        let unknown = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1], "post": "nope", "path": path }).to_string(),
        );
        assert!(
            unknown.to_string().contains("no post processor"),
            "{unknown}"
        );
    }

    #[test]
    fn cam_save_keeps_the_two_sides_apart() {
        let mut fixture = Fixture::new("cmd-save-sides");
        fixture.state.set_toolpath(toolpath(1, "top"));
        let mut bottom = toolpath(2, "bottom");
        bottom.side = crate::doc::Side::Bottom;
        fixture.state.set_toolpath(bottom);
        let path = fixture.temp.path().join("job.gcode");
        let mixed = fixture.fails(
            "cam.save",
            &json!({ "toolpathIds": [1, 2], "post": "grbl_mm",
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert!(mixed.to_string().contains("own file"), "{mixed}");
        let each = fixture.call(
            "cam.save",
            &json!({ "toolpathIds": [1, 2], "post": "grbl_mm", "oneFilePerToolpath": true,
                     "path": path.display().to_string() })
            .to_string(),
        );
        assert_eq!(each["paths"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn missing_arguments_are_reported() {
        let mut fixture = Fixture::new("cmd-args");
        let error = fixture.fails("tools.get", "{}");
        assert!(matches!(error, CoreError::InvalidArgs { .. }), "{error}");
        let broken = fixture.fails("tools.get", "{\"id\":}");
        assert!(matches!(broken, CoreError::InvalidArgs { .. }), "{broken}");
    }

    #[test]
    fn a_toolpath_is_replaced_by_id_rather_than_added_twice() {
        let mut state = CamState::new(
            ToolStore::at(PathBuf::from("unused.json")),
            PostLibrary::at(PathBuf::from("unused")),
        );
        state.set_toolpath(toolpath(1, "one"));
        state.set_toolpath(toolpath(1, "one again"));
        assert_eq!(state.toolpaths().len(), 1);
        assert_eq!(state.toolpaths()[0].name, "one again");
        state.clear_toolpaths();
        assert!(state.toolpaths().is_empty());
    }

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(save::safe_file_name("profile 1"), "profile_1");
        assert_eq!(save::safe_file_name("a/b:c"), "a_b_c");
        assert_eq!(save::safe_file_name("///"), "toolpath");
    }
}
