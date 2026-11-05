//! Job templates: starter documents that hold a job setup, layers, notes,
//! document variables and, when asked for, the design and its toolpaths.
//!
//! A template is an ordinary `.carve` file in the templates folder, which
//! sits beside the clipart folder under the user's data directory. Files
//! at the top of the folder are the user's own templates; each subfolder is
//! a category. The standard sheet templates (see [`standard`]) are written
//! into a folder of their own and listed under "Standard".
//!
//! Opening a template makes a new untitled document from it: it has no file
//! path, so the first save asks where to put it.
//!
//! Commands: `templates.list`, `templates.save` and `templates.new`; see
//! `docs/KERNEL_API.md`.

pub mod standard;

use std::path::{Path as FsPath, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::doc::{Document, Units};
use crate::error::CoreError;
use crate::io::native;
use crate::kernel::{Kernel, KernelReply};

/// Environment variable naming a folder to keep the user's templates in
/// instead of the data directory, for development hosts and tests.
pub const DIR_ENV: &str = "CARVE_TEMPLATES_DIR";

/// Longest template or category name, in characters.
pub const MAX_NAME: usize = 120;

/// Where the templates live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateDirs {
    /// The user's templates folder.
    pub user: PathBuf,
    /// Where the standard templates are written.
    pub standard: PathBuf,
}

impl TemplateDirs {
    /// `CarveStudio/templates` under the platform data directory (or the
    /// folder [`DIR_ENV`] names) and `CarveStudio/templates-standard-<version>`
    /// under the cache directory. Unit tests get folders under the temporary
    /// directory instead, so they never touch the user's files.
    pub fn user_default() -> Self {
        let standard_folder = format!("templates-standard-{}", standard::STANDARD_VERSION);
        if cfg!(test) {
            let base = std::env::temp_dir().join(format!("carve-templates-{}", std::process::id()));
            return TemplateDirs {
                user: base.join("templates"),
                standard: base.join(standard_folder),
            };
        }
        let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
        let user = std::env::var_os(DIR_ENV)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("CarveStudio")
                    .join("templates")
            });
        TemplateDirs {
            user,
            standard: cache.join("CarveStudio").join(standard_folder),
        }
    }
}

/// Whether `name` is a templates command.
pub fn handles(name: &str) -> bool {
    matches!(name, "templates.list" | "templates.save" | "templates.new")
}

/// Runs a templates command against the user's templates folder.
///
/// # Errors
/// The errors of [`dispatch_in`].
pub fn dispatch(kernel: &mut Kernel, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    dispatch_in(&TemplateDirs::user_default(), kernel, name, args)
}

/// Runs a templates command with the templates in `dirs`.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] for a bad name, a template that already
/// exists or a file that is not a document, and [`CoreError::Io`] for files
/// that cannot be read or written.
pub fn dispatch_in(
    dirs: &TemplateDirs,
    kernel: &mut Kernel,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "templates.list" => KernelReply::ok(&list(dirs)),
        "templates.save" => {
            let args: SaveArgs = parse_args(name, args)?;
            let path = save(dirs, kernel.document(), &args)?;
            KernelReply::ok(&json!({ "path": path.to_string_lossy() }))
        }
        "templates.new" => {
            let args: NewArgs = parse_args(name, args)?;
            let template = native::open(&args.path, 0)?;
            // Clears the display cache and the sculpt baselines, as opening
            // any document does.
            kernel.apply("doc.new", "")?;
            let revision = kernel.document().revision() + 1;
            let mut document = Document::from_file(template.to_file(), revision)?;
            kernel.prefs().values().apply_to_document(&mut document);
            *kernel.document_mut() = document;
            KernelReply::ok(&json!({
                "revision": revision,
                "name": template_name(&args.path),
            }))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

fn parse_args<T: DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        args
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

fn yes() -> bool {
    true
}

/// Arguments of `templates.save`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveArgs {
    /// Template name, also its file name.
    pub name: String,
    /// Subfolder to file it under; the top of the folder when absent.
    #[serde(default)]
    pub category: Option<String>,
    /// Whether the vectors and relief components go into the template.
    #[serde(default = "yes")]
    pub design: bool,
    /// Whether the toolpaths go in too; only with the design.
    #[serde(default = "yes")]
    pub toolpaths: bool,
    /// Whether a template of the same name may be replaced.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewArgs {
    path: PathBuf,
}

/// One template as `templates.list` describes it. Sizes are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// Name shown in the list.
    pub name: String,
    /// Subfolder, `Standard` for the standard sheets, `None` at the top.
    pub category: Option<String>,
    /// The file.
    pub path: String,
    /// True for the standard sheet templates.
    pub standard: bool,
    /// The job's display units.
    pub units: Units,
    /// Job width.
    pub width: f64,
    /// Job height.
    pub height: f64,
    /// Material thickness.
    pub thickness: f64,
    /// Number of layers.
    pub layers: usize,
    /// Number of top level vectors.
    pub vectors: usize,
    /// Number of toolpaths.
    pub toolpaths: usize,
}

/// A file that could not be listed and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWarning {
    /// The file.
    pub path: String,
    /// What went wrong.
    pub message: String,
}

/// What `templates.list` answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateList {
    /// The user's templates folder.
    pub user_dir: String,
    /// The standard templates first, then the user's by category and name.
    pub templates: Vec<TemplateInfo>,
    /// Files that are not documents.
    pub warnings: Vec<ListWarning>,
}

/// Lists the standard templates, writing them first if needed, then the
/// user's own. Files that do not open are reported as warnings.
pub fn list(dirs: &TemplateDirs) -> TemplateList {
    let mut found: Vec<(PathBuf, Option<String>, bool)> = Vec::new();
    let mut warnings = Vec::new();
    match standard::ensure(&dirs.standard) {
        Ok(paths) => found.extend(
            paths
                .into_iter()
                .map(|path| (path, Some(standard::STANDARD_CATEGORY.to_string()), true)),
        ),
        Err(error) => warnings.push(warning(&dirs.standard, &error)),
    }
    // The folder is made on first use so there is somewhere to save into;
    // failing to make it only means it lists nothing.
    let _ = std::fs::create_dir_all(&dirs.user);
    found.extend(
        documents_in(&dirs.user)
            .into_iter()
            .map(|path| (path, None, false)),
    );
    for (category, folder) in categories_in(&dirs.user) {
        found.extend(
            documents_in(&folder)
                .into_iter()
                .map(|path| (path, Some(category.clone()), false)),
        );
    }
    let mut templates = Vec::with_capacity(found.len());
    for (path, category, standard) in found {
        match describe(&path, category, standard) {
            Ok(info) => templates.push(info),
            Err(error) => warnings.push(warning(&path, &error)),
        }
    }
    TemplateList {
        user_dir: dirs.user.to_string_lossy().into_owned(),
        templates,
        warnings,
    }
}

fn warning(path: &FsPath, error: &CoreError) -> ListWarning {
    ListWarning {
        path: path.to_string_lossy().into_owned(),
        message: error.to_string(),
    }
}

fn describe(
    path: &FsPath,
    category: Option<String>,
    standard: bool,
) -> Result<TemplateInfo, CoreError> {
    let doc = native::open(path, 0)?;
    let job = doc.job_setup();
    Ok(TemplateInfo {
        name: template_name(path),
        category,
        path: path.to_string_lossy().into_owned(),
        standard,
        units: job.units,
        width: job.width,
        height: job.height,
        thickness: job.thickness,
        layers: doc.layers().len(),
        vectors: doc.order().len(),
        toolpaths: doc.toolpaths().len(),
    })
}

/// A template's name: its file name without the extension.
fn template_name(path: &FsPath) -> String {
    path.file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
}

fn visible(path: &FsPath) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| !name.starts_with('.'))
}

/// The `.carve` files directly in `dir`, sorted by name ignoring case.
fn documents_in(dir: &FsPath) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && visible(path)
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case(native::EXTENSION))
        })
        .collect();
    paths.sort_by_key(|path| template_name(path).to_lowercase());
    paths
}

/// The visible subfolders of `dir` as (name, folder), sorted ignoring case.
fn categories_in(dir: &FsPath) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && visible(path))
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some((name, path))
        })
        .collect();
    folders.sort_by_key(|(name, _)| name.to_lowercase());
    folders
}

/// Checks a template or category name: not empty, not too long, and
/// usable as a file name on every platform.
///
/// # Errors
/// [`CoreError::Validation`] naming what is wrong.
pub fn check_name(what: &str, name: &str) -> Result<(), CoreError> {
    if name.is_empty() {
        return Err(CoreError::validation(format!("the {what} needs a name")));
    }
    if name.chars().count() > MAX_NAME {
        return Err(CoreError::validation(format!(
            "a {what} name is at most {MAX_NAME} characters"
        )));
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err(CoreError::validation(format!(
            "a {what} name cannot start or end with a full stop"
        )));
    }
    if let Some(bad) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err(CoreError::validation(format!(
            "a {what} name cannot contain {bad:?}"
        )));
    }
    Ok(())
}

/// The file a template of this name and category is saved to.
///
/// # Errors
/// The errors of [`check_name`].
pub fn template_path(
    dirs: &TemplateDirs,
    name: &str,
    category: Option<&str>,
) -> Result<PathBuf, CoreError> {
    check_name("template", name)?;
    let mut folder = dirs.user.clone();
    if let Some(category) = category {
        check_name("category", category)?;
        folder.push(category);
    }
    Ok(folder.join(format!("{name}.{}", native::EXTENSION)))
}

/// The document a template is made of: `doc` without its file path,
/// selection or undo history, and without the design or toolpaths when
/// they are left out.
///
/// # Errors
/// [`CoreError::Validation`] if the document cannot be rebuilt, which only
/// a corrupt document causes.
pub fn template_document(
    doc: &Document,
    design: bool,
    toolpaths: bool,
) -> Result<Document, CoreError> {
    let mut file = doc.to_file();
    if !design {
        file.entities.clear();
        file.order.clear();
        file.components.clear();
        if let Some(bottom) = file.bottom.as_mut() {
            bottom.entities.clear();
            bottom.order.clear();
            bottom.components.clear();
        }
    }
    if !(design && toolpaths) {
        file.toolpaths.clear();
    }
    Document::from_file(file, 0)
}

/// Saves `doc` as a template and answers the file written.
///
/// # Errors
/// [`CoreError::Validation`] for a bad name or a template that exists when
/// `overwrite` is off, and the errors of [`native::save_copy`].
pub fn save(dirs: &TemplateDirs, doc: &Document, args: &SaveArgs) -> Result<PathBuf, CoreError> {
    let name = args.name.trim();
    let category = args
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let path = template_path(dirs, name, category)?;
    if path.exists() && !args.overwrite {
        return Err(CoreError::validation(format!(
            "a template called {name:?} already exists"
        )));
    }
    let template = template_document(doc, args.design, args.toolpaths)?;
    native::save_copy(&template, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::commands::CamState;
    use crate::doc::{Entity, EntityKind, JobSetup, XyOrigin};
    use crate::geom::{rect, Point};
    use crate::test_support::TempDir;
    use serde_json::Value;

    fn temp_kernel(label: &str) -> (TempDir, TemplateDirs, Kernel) {
        let temp = TempDir::new(label);
        let cam = CamState::new(
            crate::cam::tooldb::ToolStore::at(temp.path().join("tools.json")),
            crate::post::PostLibrary::at(temp.path().join("posts")),
        );
        let dirs = TemplateDirs {
            user: temp.path().join("templates"),
            standard: temp.path().join("standard"),
        };
        (temp, dirs, Kernel::with_cam(cam))
    }

    fn call(dirs: &TemplateDirs, kernel: &mut Kernel, name: &str, args: Value) -> Value {
        match dispatch_in(dirs, kernel, name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("reply is JSON");
                value["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("{name} answered bytes"),
        }
    }

    fn kernel_call(kernel: &mut Kernel, name: &str, args: Value) -> Value {
        match kernel.apply(name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("reply is JSON");
                value["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("{name} answered bytes"),
        }
    }

    /// A kernel whose document has a custom job, two layers, a variable,
    /// notes and a rectangle.
    fn set_up(kernel: &mut Kernel) {
        let job = JobSetup {
            units: Units::Inch,
            width: 609.6,
            height: 457.2,
            thickness: 12.7,
            xy_origin: XyOrigin::Centre,
            ..JobSetup::default()
        };
        kernel_call(kernel, "job.set", serde_json::to_value(job).expect("job"));
        kernel_call(kernel, "layers.add", json!({ "name": "Cut Out" }));
        kernel_call(
            kernel,
            "doc.variables.set",
            json!({ "variables": { "depth": 6.0 } }),
        );
        kernel_call(kernel, "doc.notes.set", json!({ "notes": "Sign blank" }));
        let doc = kernel.document_mut();
        doc.begin("Create");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(-50.0, -20.0), Point::new(50.0, 20.0)).expect("rect"),
            },
        });
        doc.commit();
    }

    #[test]
    fn names_that_are_not_file_names_are_refused() {
        assert!(check_name("template", "Sign 600 x 400").is_ok());
        for bad in ["", ".hidden", "a/b", "a\\b", "a:b", "end.", "tab\there"] {
            assert!(check_name("template", bad).is_err(), "{bad:?}");
        }
        assert!(check_name("template", &"x".repeat(MAX_NAME + 1)).is_err());
    }

    #[test]
    fn a_template_round_trips_through_save_and_new() {
        let (_temp, dirs, mut kernel) = temp_kernel("templates-round-trip");
        set_up(&mut kernel);
        let saved = call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Sign", "category": "Signs" }),
        );
        let path = saved["path"].as_str().expect("path").to_string();
        assert!(path.ends_with("Signs/Sign.carve"), "{path}");
        let original = template_document(kernel.document(), true, true).expect("doc");

        kernel_call(&mut kernel, "doc.new", json!({}));
        assert_eq!(kernel.document().order().len(), 0);
        let before = kernel.document().revision();
        let reply = call(&dirs, &mut kernel, "templates.new", json!({ "path": path }));
        assert_eq!(reply["name"], "Sign");
        let revision = reply["revision"].as_u64().expect("revision");
        assert!(revision > before);
        let doc = kernel.document();
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.file_path(), None, "a template opens untitled");
        assert!(!doc.is_dirty());
        assert_eq!(doc.job_setup().units, Units::Inch);
        assert_eq!(doc.job_setup().xy_origin, XyOrigin::Centre);
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(doc.variables().get("depth"), Some(&6.0));
        assert_eq!(doc.notes(), "Sign blank");
        assert_eq!(doc.order().len(), 1);
        assert_eq!(doc, &original);

        let info = kernel_call(&mut kernel, "doc.info", json!({}));
        assert!(info["path"].is_null());
    }

    #[test]
    fn the_design_can_be_left_out() {
        let (_temp, dirs, mut kernel) = temp_kernel("templates-no-design");
        set_up(&mut kernel);
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Blank", "design": false }),
        );
        let listed = list(&dirs);
        let blank = listed
            .templates
            .iter()
            .find(|t| t.name == "Blank")
            .expect("listed");
        assert_eq!(blank.category, None);
        assert!(!blank.standard);
        assert_eq!(blank.vectors, 0);
        assert_eq!(blank.layers, 2);
        assert_eq!(blank.units, Units::Inch);
        assert!((blank.width - 609.6).abs() < 1e-9);
        assert!((blank.thickness - 12.7).abs() < 1e-9);
    }

    #[test]
    fn saving_over_a_template_needs_overwrite() {
        let (_temp, dirs, mut kernel) = temp_kernel("templates-overwrite");
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Job" }),
        );
        let again = dispatch_in(
            &dirs,
            &mut kernel,
            "templates.save",
            &json!({ "name": "Job" }).to_string(),
        );
        assert!(again.is_err());
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Job", "overwrite": true }),
        );
        let bad = dispatch_in(
            &dirs,
            &mut kernel,
            "templates.save",
            &json!({ "name": "../escape" }).to_string(),
        );
        assert!(bad.is_err());
    }

    #[test]
    fn the_list_has_the_standard_sheets_first_then_categories() {
        let (_temp, dirs, mut kernel) = temp_kernel("templates-list");
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "zeta" }),
        );
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Alpha" }),
        );
        call(
            &dirs,
            &mut kernel,
            "templates.save",
            json!({ "name": "Door", "category": "Cabinets" }),
        );
        std::fs::write(dirs.user.join("broken.carve"), b"not a zip").expect("write");
        std::fs::write(dirs.user.join("notes.txt"), b"ignored").expect("write");

        let listed = call(&dirs, &mut kernel, "templates.list", json!({}));
        let names: Vec<&str> = listed["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        let standard = standard::STANDARD_SHEETS.len();
        assert_eq!(names.len(), standard + 3);
        assert_eq!(names[0], standard::STANDARD_SHEETS[0].name);
        assert_eq!(&names[standard..], ["Alpha", "zeta", "Door"]);
        assert_eq!(listed["templates"][0]["category"], "Standard");
        assert_eq!(listed["templates"][0]["standard"], true);
        assert_eq!(listed["templates"][standard + 2]["category"], "Cabinets");
        assert_eq!(listed["warnings"].as_array().expect("warnings").len(), 1);
        assert_eq!(
            listed["userDir"],
            dirs.user.to_string_lossy().into_owned().as_str()
        );
    }

    #[test]
    fn a_standard_template_opens_as_a_new_job() {
        let (_temp, dirs, mut kernel) = temp_kernel("templates-standard-new");
        let listed = list(&dirs);
        let ply = &listed.templates[0];
        call(
            &dirs,
            &mut kernel,
            "templates.new",
            json!({ "path": ply.path }),
        );
        let job = kernel.document().job_setup();
        assert!((job.width - 2440.0).abs() < 1e-9);
        assert!((job.height - 1220.0).abs() < 1e-9);
        assert!((job.thickness - 18.0).abs() < 1e-9);
        assert_eq!(kernel.document().file_path(), None);
    }

    #[test]
    fn a_missing_template_leaves_the_document_alone() {
        let (temp, dirs, mut kernel) = temp_kernel("templates-missing");
        set_up(&mut kernel);
        let revision = kernel.document().revision();
        let missing = temp.path().join("nope.carve");
        let result = dispatch_in(
            &dirs,
            &mut kernel,
            "templates.new",
            &json!({ "path": missing }).to_string(),
        );
        assert!(result.is_err());
        assert_eq!(kernel.document().revision(), revision);
        assert_eq!(kernel.document().order().len(), 1);
    }

    #[test]
    fn the_kernel_routes_template_commands() {
        assert!(handles("templates.list"));
        assert!(handles("templates.save"));
        assert!(handles("templates.new"));
        assert!(!handles("templates.delete"));
        let (_temp, _dirs, mut kernel) = temp_kernel("templates-route");
        let listed = kernel_call(&mut kernel, "templates.list", json!({}));
        assert!(listed["templates"].as_array().is_some());
    }
}
