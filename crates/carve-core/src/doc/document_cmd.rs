//! Document, job and undo commands: `doc.*`, `job.*`, `undo`, `redo` and
//! `undo.info`. These replace or rewind the whole document, so they run on
//! the kernel and reset the caches that hang off the document.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::layers_cmd::{layer_list, sheet_list};
use crate::doc::{Document, JobSetup};
use crate::error::CoreError;
use crate::io;
use crate::kernel::{edit, parse_args, revision_reply, Kernel, KernelReply, PathArgs};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "doc.new"
            | "doc.open"
            | "doc.restore"
            | "doc.save"
            | "doc.saveCopy"
            | "doc.info"
            | "doc.notes.get"
            | "doc.variables.get"
            | "doc.variables.set"
            | "doc.variables.export"
            | "doc.variables.import"
            | "doc.notes.set"
            | "doc.guides.get"
            | "doc.guides.set"
            | "job.get"
            | "job.set"
            | "job.setSide"
            | "job.swapSides"
            | "undo"
            | "redo"
            | "undo.info"
            | "undo.beginGroup"
            | "undo.endGroup"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(kernel: &mut Kernel, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "doc.new" => {
            let next = kernel.document.revision() + 1;
            kernel.document = Document::with_revision(next);
            kernel
                .prefs
                .values()
                .apply_to_new_document(&mut kernel.document);
            kernel.display.clear();
            kernel.sculpt_baselines.clear();
            kernel.clipboard.left_job();
            revision_reply(&kernel.document)
        }
        "doc.open" => {
            let args: PathArgs = parse_args(name, args)?;
            open_document(kernel, &args.path)?;
            revision_reply(&kernel.document)
        }
        "doc.restore" => {
            let args: RestoreArgs = parse_args(name, args)?;
            open_document(kernel, &args.path)?;
            kernel.document.mark_restored(args.as_path);
            revision_reply(&kernel.document)
        }
        "doc.save" => {
            let args: PathArgs = parse_args(name, args)?;
            io::native::save(&kernel.document, &args.path)?;
            kernel.document.mark_saved(args.path);
            revision_reply(&kernel.document)
        }
        "doc.saveCopy" => {
            let args: SaveCopyArgs = parse_args(name, args)?;
            let path = args.path.unwrap_or_else(|| {
                let fallback = dirs::config_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("CarveStudio")
                    .join("Backups");
                io::native::backup_path(kernel.document.file_path(), &fallback)
            });
            io::native::save_copy(&kernel.document, &path)?;
            KernelReply::ok(&json!({ "path": path.display().to_string() }))
        }
        "doc.info" => KernelReply::ok(&info(&kernel.document)),
        "doc.notes.get" => KernelReply::ok(&json!({ "notes": kernel.document.notes() })),
        "doc.variables.get" => KernelReply::ok(&json!({
            "variables": kernel.document.variables(),
            "formulas": kernel.document.variable_formulas(),
        })),
        "doc.variables.set" => {
            let args: VariablesArgs = parse_args(name, args)?;
            edit(&mut kernel.document, "Document Variables", |doc| {
                doc.set_variable_definitions(args.variables)?;
                Ok(json!({}))
            })
        }
        "doc.variables.export" => {
            let args: PathArgs = parse_args(name, args)?;
            let text = variables_text(&kernel.document.variable_definitions());
            std::fs::write(&args.path, text)
                .map_err(|error| CoreError::io(args.path.display(), error))?;
            KernelReply::ok(&json!({ "count": kernel.document.variables().len() }))
        }
        "doc.variables.import" => {
            let args: PathArgs = parse_args(name, args)?;
            let text = std::fs::read_to_string(&args.path)
                .map_err(|error| CoreError::io(args.path.display(), error))?;
            let read = parse_variables(&text)?;
            let count = read.len();
            // Variables of the same name are replaced; the rest stay.
            let mut next = kernel.document.variable_definitions();
            next.extend(read);
            edit(&mut kernel.document, "Import Document Variables", |doc| {
                doc.set_variable_definitions(next)?;
                Ok(json!({ "count": count }))
            })
        }
        "doc.guides.get" => KernelReply::ok(&json!({ "guides": kernel.document.guides() })),
        "doc.guides.set" => {
            let args: GuidesArgs = parse_args(name, args)?;
            if args.guides.iter().any(|g| !g.position.is_finite()) {
                return Err(CoreError::validation("a guide needs a position"));
            }
            kernel.document.set_guides(args.guides);
            KernelReply::ok(&json!({ "dirty": kernel.document.is_dirty() }))
        }
        "doc.notes.set" => {
            let args: NotesArgs = parse_args(name, args)?;
            edit(&mut kernel.document, "Notes", |doc| {
                doc.set_notes(args.notes);
                Ok(json!({}))
            })
        }
        "job.get" => KernelReply::ok(kernel.document.job_setup()),
        "job.set" => {
            let job: JobSetup = parse_args(name, args)?;
            edit(&mut kernel.document, "Job Setup", |doc| {
                doc.set_job(job)?;
                Ok(serde_json::to_value(doc.job_setup())?)
            })
        }
        "job.setSide" => {
            // A view change: no undo step, and the job is not changed.
            let args: SideArgs = parse_args(name, args)?;
            kernel.document.show_side(args.side)?;
            KernelReply::ok(&json!({
                "revision": kernel.document.revision(),
                "side": kernel.document.active_side(),
            }))
        }
        "job.swapSides" => edit(&mut kernel.document, "Swap Sides", |doc| {
            doc.swap_sides()?;
            Ok(json!({ "side": doc.active_side() }))
        }),
        "undo" => {
            kernel.document.end_undo_group();
            let label = kernel.document.undo();
            kernel.display.prune(&kernel.document);
            KernelReply::ok(&json!({
                "revision": kernel.document.revision(),
                "label": label,
            }))
        }
        "redo" => {
            kernel.document.end_undo_group();
            let label = kernel.document.redo();
            kernel.display.prune(&kernel.document);
            KernelReply::ok(&json!({
                "revision": kernel.document.revision(),
                "label": label,
            }))
        }
        "undo.beginGroup" => {
            let args: GroupArgs = parse_args(name, args)?;
            kernel.document.begin_undo_group(args.label);
            KernelReply::ok(&json!({}))
        }
        "undo.endGroup" => {
            let merged = kernel.document.end_undo_group();
            KernelReply::ok(&json!({ "merged": merged }))
        }
        "undo.info" => {
            let journal = kernel.document.journal();
            KernelReply::ok(&json!({
                "canUndo": journal.can_undo(),
                "canRedo": journal.can_redo(),
                "undoLabel": journal.undo_label(),
                "redoLabel": journal.redo_label(),
            }))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

fn info(doc: &Document) -> Value {
    json!({
        "revision": doc.revision(),
        "path": doc.file_path().map(|p| p.display().to_string()),
        "dirty": doc.is_dirty(),
        "job": doc.job_setup(),
        "activeSide": doc.active_side(),
        "layers": layer_list(doc),
        "sheets": sheet_list(doc),
        "activeLayer": doc.active_layer(),
        "activeSheet": doc.active_sheet(),
        "notes": doc.notes(),
        "counts": {
            "entities": doc.order().len(),
            "components": 0,
            "toolpaths": 0,
        },
    })
}

/// The document variables as a text file: one `name = value` a line, the
/// formula in place of the value for a variable defined by one.
pub fn variables_text(
    variables: &std::collections::BTreeMap<String, crate::doc::variables::Definition>,
) -> String {
    use crate::doc::variables::Definition;
    variables
        .iter()
        .map(|(name, definition)| match definition {
            Definition::Number(value) => format!("{name} = {value}\n"),
            Definition::Formula(formula) => format!("{name} = {formula}\n"),
        })
        .collect()
}

/// Reads variables written by [`variables_text`]: `name = value` a line
/// (also `name,value` or `name<tab>value`); a value that is not a number
/// is a formula, checked when the variables are set. Blank lines and lines
/// starting with `#` are skipped.
///
/// # Errors
/// [`CoreError::Validation`] naming the first line that does not read.
pub fn parse_variables(
    text: &str,
) -> Result<std::collections::BTreeMap<String, crate::doc::variables::Definition>, CoreError> {
    use crate::doc::variables::Definition;
    let mut out = std::collections::BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || CoreError::validation(format!("line {} is not name = value", index + 1));
        let (name, value) = line
            .split_once('=')
            .or_else(|| line.split_once(','))
            .or_else(|| line.split_once('\t'))
            .ok_or_else(bad)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(bad());
        }
        let definition = match value.parse::<f64>() {
            Ok(number) => Definition::Number(number),
            Err(_) => Definition::Formula(value.to_string()),
        };
        out.insert(name.trim().to_string(), definition);
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct VariablesArgs {
    variables: std::collections::BTreeMap<String, crate::doc::variables::Definition>,
}

/// Replaces the document with the job in the `.carve` file at `path`.
fn open_document(kernel: &mut Kernel, path: &std::path::Path) -> Result<(), CoreError> {
    let next = kernel.document.revision() + 1;
    kernel.document = io::native::open(path, next)?;
    kernel
        .prefs
        .values()
        .apply_to_document(&mut kernel.document);
    kernel.display.clear();
    kernel.sculpt_baselines.clear();
    kernel.clipboard.left_job();
    Ok(())
}

/// `{path, as?}`: an autosave backup, and the job file it stands for.
#[derive(Debug, Deserialize)]
struct RestoreArgs {
    path: PathBuf,
    #[serde(default, rename = "as")]
    as_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct SaveCopyArgs {
    #[serde(default)]
    path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuidesArgs {
    guides: Vec<crate::doc::GuideLine>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotesArgs {
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SideArgs {
    side: crate::doc::Side,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupArgs {
    #[serde(default)]
    label: Option<String>,
}
