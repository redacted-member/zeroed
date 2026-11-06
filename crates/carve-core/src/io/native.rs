//! The native `.carve` document format.
//!
//! A `.carve` file is a zip archive holding
//! - `document.json`, the document written by serde with a `format` version,
//! - `images/<id>.<ext>`, the bitmaps the drawing places, as the PNG, JPEG,
//!   BMP or WebP file they were imported from,
//! - `heightfields/<id>.f32`, each relief component's heights (little endian
//!   f32 per cell, NaN where empty; its size is in `document.json`),
//! - `thumbnail.png`, optional, for the file browser.
//!
//! `document.json` carries everything else a job holds: job setup, layers,
//! sheets, vectors, notes, document variables, levels, calculated toolpaths
//! with their moves and settings, toolpath groups and tiling, and both sides
//! of a double sided job.

use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path as FsPath, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::doc::{Document, DocumentFile};
use crate::error::CoreError;

/// Name of the entry holding the document.
pub const DOCUMENT_ENTRY: &str = "document.json";
/// Folder holding placed bitmaps.
pub const IMAGES_FOLDER: &str = "images/";
/// Folder reserved for the relief model.
pub const HEIGHTFIELDS_FOLDER: &str = "heightfields/";
/// Optional preview image.
pub const THUMBNAIL_ENTRY: &str = "thumbnail.png";
/// Extension of a native document.
pub const EXTENSION: &str = "carve";

/// Entry holding a component's heights: little endian f32 per cell.
fn heightfield_entry(id: u64) -> String {
    format!("{HEIGHTFIELDS_FOLDER}{id}.f32")
}

/// Writes a document to a `.carve` file.
///
/// The file is written beside the target first and renamed into place, so a
/// save that fails leaves the last good copy as it was. A document that
/// could not be read back (a number that is not finite, which JSON writes as
/// `null`) is refused before anything is written.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be written,
/// [`CoreError::Encode`] when the document cannot be serialised and
/// [`CoreError::Validation`] when it holds a number that is not finite.
pub fn save(doc: &Document, path: &FsPath) -> Result<(), CoreError> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    if let Err(error) = write_archive(doc, &partial) {
        std::fs::remove_file(&partial).ok();
        return Err(error);
    }
    std::fs::rename(&partial, path).map_err(|error| {
        std::fs::remove_file(&partial).ok();
        CoreError::io(path.display(), error)
    })
}

/// The document's JSON, checked to read back: serde writes a number that is
/// not finite as `null`, and such a file would never open again.
fn document_json(document: &DocumentFile) -> Result<Vec<u8>, CoreError> {
    let json = serde_json::to_vec_pretty(document)?;
    if let Err(error) = serde_json::from_slice::<DocumentFile>(&json) {
        return Err(CoreError::validation(format!(
            "the job was not saved: it holds a number that is not finite, \
             most likely an object scaled or moved too far ({error})"
        )));
    }
    Ok(json)
}

/// Writes the archive itself to `path`.
fn write_archive(doc: &Document, path: &FsPath) -> Result<(), CoreError> {
    let document = doc.to_file();
    let json = document_json(&document)?;
    let file = File::create(path).map_err(|error| CoreError::io(path.display(), error))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(DOCUMENT_ENTRY, options)
        .map_err(|error| CoreError::io(path.display(), error))?;
    zip.write_all(&json)
        .map_err(|error| CoreError::io(path.display(), error))?;
    zip.add_directory(IMAGES_FOLDER, options)
        .map_err(|error| CoreError::io(path.display(), error))?;
    zip.add_directory(HEIGHTFIELDS_FOLDER, options)
        .map_err(|error| CoreError::io(path.display(), error))?;
    let bottom = document
        .bottom
        .iter()
        .flat_map(|side| side.components.iter());
    for component in document.components.iter().chain(bottom) {
        zip.start_file(heightfield_entry(component.id), options)
            .map_err(|error| CoreError::io(path.display(), error))?;
        zip.write_all(&component.field.z_bytes())
            .map_err(|error| CoreError::io(path.display(), error))?;
    }
    // Only images a bitmap still places; undone imports leave theirs behind.
    for id in crate::doc::images::referenced(&document) {
        if let Some(image) = doc.images().get(id) {
            zip.start_file(crate::doc::images::entry_name(id, image), options)
                .map_err(|error| CoreError::io(path.display(), error))?;
            zip.write_all(&image.bytes)
                .map_err(|error| CoreError::io(path.display(), error))?;
        }
    }
    zip.finish()
        .map_err(|error| CoreError::io(path.display(), error))?;
    Ok(())
}

/// Where autosave keeps the backup of a document: beside a saved document
/// as `<name>.backup.carve`, and as `Untitled.backup.carve` in `fallback_dir`
/// for a document that has never been saved.
pub fn backup_path(document: Option<&FsPath>, fallback_dir: &FsPath) -> PathBuf {
    match document {
        Some(path) => {
            let stem = path.file_stem().map_or_else(
                || "Untitled".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            );
            path.with_file_name(format!("{stem}.backup.{EXTENSION}"))
        }
        None => fallback_dir.join(format!("Untitled.backup.{EXTENSION}")),
    }
}

/// Writes a copy of a document without touching its own path or unsaved
/// state. The copy is written to a temporary file first and renamed into
/// place, so an interrupted write never leaves a broken backup behind.
///
/// # Errors
/// The errors of [`save`], and [`CoreError::Io`] when the folder cannot be
/// created or the finished copy cannot be renamed into place.
pub fn save_copy(doc: &Document, path: &FsPath) -> Result<(), CoreError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|error| CoreError::io(dir.display(), error))?;
    }
    save(doc, path)
}

/// Reads a document from a `.carve` file.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be read,
/// [`CoreError::Encode`] when `document.json` is not valid JSON and
/// [`CoreError::Validation`] when the archive has no document in it or the
/// format version is newer than this build understands.
pub fn open(path: &FsPath, revision: u64) -> Result<Document, CoreError> {
    let file = File::open(path).map_err(|error| CoreError::io(path.display(), error))?;
    let mut doc = read_archive(BufReader::new(file), &path.display().to_string(), revision)?;
    doc.mark_saved(path.to_path_buf());
    Ok(doc)
}

/// Reads a document held in memory. It has no file behind it, so the first
/// save asks where to put it, and no unsaved changes yet.
///
/// # Errors
/// As [`open`], with `name` standing for the file in the messages.
pub fn read_bytes(bytes: &[u8], name: &str, revision: u64) -> Result<Document, CoreError> {
    read_archive(Cursor::new(bytes), name, revision)
}

fn read_archive<R: Read + Seek>(
    reader: R,
    name: &str,
    revision: u64,
) -> Result<Document, CoreError> {
    let mut zip = ZipArchive::new(reader).map_err(|error| CoreError::io(name, error))?;
    let mut text = String::new();
    {
        let mut entry = zip
            .by_name(DOCUMENT_ENTRY)
            .map_err(|_| CoreError::validation("this is not a Zeroed document"))?;
        entry
            .read_to_string(&mut text)
            .map_err(|error| CoreError::io(name, error))?;
    }
    let mut file = read_document(&text)?;
    let DocumentFile {
        components, bottom, ..
    } = &mut file;
    let bottom = bottom.iter_mut().flat_map(|side| side.components_mut());
    for component in components.iter_mut().chain(bottom) {
        let mut bytes = Vec::new();
        zip.by_name(&heightfield_entry(component.id))
            .map_err(|_| CoreError::validation("a relief component is missing from the file"))?
            .read_to_end(&mut bytes)
            .map_err(|error| CoreError::io(name, error))?;
        component.field.set_z_bytes(&bytes)?;
    }
    let image_ids = crate::doc::images::referenced(&file);
    let mut doc = Document::from_file(file, revision)?;
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    for id in image_ids {
        let prefix = format!("{IMAGES_FOLDER}{id}.");
        let Some(name) = names.iter().find(|n| n.starts_with(&prefix)) else {
            continue;
        };
        let mut bytes = Vec::new();
        zip.by_name(name)
            .map_err(|error| CoreError::io(name, error))?
            .read_to_end(&mut bytes)
            .map_err(|error| CoreError::io(name, error))?;
        let ext = name.rsplit('.').next();
        let image = crate::doc::bitmap_cmd::stored_image(bytes, ext)?;
        doc.images_mut().restore(id, image);
    }
    Ok(doc)
}

/// Parses `document.json`. A file saved before saves refused numbers that
/// are not finite holds `null` where a number belongs (serde writes NaN and
/// infinity so); each such `null` is read as 0 so the job still opens. A
/// `null` counts as a number when it sits among numbers: in an array of
/// numbers or an object whose other members are numbers (a point, a
/// matrix), or as a point's `x`, `y` or `z`. Other `null`s are settings
/// left unset and stay as they are.
fn read_document(text: &str) -> Result<DocumentFile, CoreError> {
    let error = match serde_json::from_str::<DocumentFile>(text) {
        Ok(file) => return Ok(file),
        Err(error) => error,
    };
    let message = error.to_string();
    if !(message.contains("invalid type: null, expected f64")
        || message.contains("invalid type: null, expected f32"))
    {
        return Err(error.into());
    }
    let mut value: serde_json::Value = serde_json::from_str(text)?;
    if fill_null_numbers(&mut value) == 0 {
        return Err(error.into());
    }
    serde_json::from_value(value).map_err(|_| error.into())
}

/// Replaces the `null`s that stand for numbers with 0 (see
/// [`read_document`]), answering how many.
fn fill_null_numbers(value: &mut serde_json::Value) -> usize {
    use serde_json::Value;
    let numeric = |members: &mut dyn Iterator<Item = &Value>| {
        let mut numbers = 0;
        for member in members {
            match member {
                Value::Number(_) => numbers += 1,
                Value::Null => {}
                _ => return None,
            }
        }
        Some(numbers)
    };
    let mut filled = 0;
    match value {
        Value::Array(items) => {
            if numeric(&mut items.iter()).is_some_and(|n| n > 0) {
                for item in items.iter_mut().filter(|i| i.is_null()) {
                    *item = Value::from(0.0);
                    filled += 1;
                }
            }
            for item in items.iter_mut() {
                filled += fill_null_numbers(item);
            }
        }
        Value::Object(members) => {
            let point = members
                .keys()
                .all(|k| matches!(k.as_str(), "x" | "y" | "z"));
            let among = numeric(&mut members.values()).is_some_and(|n| n > 0 || point);
            if among && members.len() > 1 || point {
                for (key, member) in members.iter_mut() {
                    if member.is_null() && (among || matches!(key.as_str(), "x" | "y" | "z")) {
                        *member = Value::from(0.0);
                        filled += 1;
                    }
                }
            }
            for member in members.values_mut() {
                filled += fill_null_numbers(member);
            }
        }
        _ => {}
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{circle, rect, Point};

    fn scratch(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("carve-native-{}-{name}.carve", std::process::id()));
        path
    }

    fn sample() -> Document {
        let mut doc = Document::new();
        doc.begin("Create");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: Some("Plate".to_string()),
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(100.0, 60.0)).expect("rect"),
            },
        });
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(50.0, 30.0), 12.0).expect("circle"),
            },
        });
        doc.add_layer(Some("Cut".to_string()), None);
        let mut field = crate::model::field::Field::empty(3, 2, 0.5, Point::new(10.0, 10.0));
        field.z[1] = 2.5;
        field.z[4] = 1.0;
        doc.add_component(crate::model::Component {
            id: 0,
            name: "Dome".to_string(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        });
        doc.commit();
        doc
    }

    #[test]
    fn a_document_survives_a_save_and_open() {
        let doc = sample();
        let path = scratch("round-trip");
        save(&doc, &path).expect("saves");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back, doc);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn toolpaths_notes_and_variables_survive_a_save_and_open() {
        use crate::cam::tool::{Tool, ToolGeometry};
        use crate::cam::toolpath::{Move, StrategyKind, Toolpath};
        use crate::cam::Point3;
        let mut doc = sample();
        doc.begin("Toolpath");
        let tool = Tool {
            name: "6 mm end mill".to_string(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        let mut toolpath = Toolpath::new(0, "Profile 1", tool, StrategyKind::Profile);
        toolpath.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -3.0)),
            Move::feed(Point3::new(10.0, 0.0, -3.0)),
            Move::rapid(Point3::new(10.0, 0.0, 5.0)),
        ];
        let id = doc.add_toolpath(toolpath);
        doc.set_notes("Glue up first".to_string());
        let mut variables = std::collections::BTreeMap::new();
        variables.insert("depth".to_string(), 3.0);
        doc.set_variables(variables).expect("variables");
        doc.commit();
        let path = scratch("toolpaths");
        save(&doc, &path).expect("saves");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back, doc);
        let saved = back.toolpath(id).expect("the toolpath is back");
        assert_eq!(saved.name, "Profile 1");
        assert_eq!(saved.moves.len(), 4);
        assert_eq!(back.to_file().notes, "Glue up first");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn guide_lines_are_saved_with_the_job() {
        use crate::doc::{GuideAxis, GuideLine};
        let mut doc = sample();
        assert!(doc.to_file().guides.is_empty());
        let guides = vec![
            GuideLine {
                axis: GuideAxis::Y,
                position: 42.0,
                locked: false,
            },
            GuideLine {
                axis: GuideAxis::X,
                position: 150.5,
                locked: true,
            },
        ];
        doc.set_guides(guides.clone());
        assert!(doc.is_dirty());
        let path = scratch("guides");
        save(&doc, &path).expect("saves");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back.guides(), guides.as_slice());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn dimension_settings_are_saved_with_the_job() {
        let mut doc = sample();
        assert!(doc.to_file().dimension_style.is_none());
        let mut style = doc.dimension_style().clone();
        style.precision = 3;
        style.show_units = false;
        style.text.size_mm = 8.0;
        doc.set_dimension_style(style.clone());
        assert!(doc.is_dirty());
        let path = scratch("dimension-style");
        save(&doc, &path).expect("saves");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back.dimension_style(), &style);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn both_sides_of_a_double_sided_job_survive_a_save_and_open() {
        use crate::doc::{JobKind, JobSetup, Side};
        let mut doc = sample();
        doc.begin("Bottom");
        doc.set_job(JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        })
        .expect("job");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let mut field = crate::model::field::Field::empty(2, 2, 1.0, Point::new(0.0, 0.0));
        field.z[3] = 4.0;
        doc.add_component(crate::model::Component {
            id: 0,
            name: "Boss".to_string(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        });
        doc.commit();
        let path = scratch("two-sided");
        save(&doc, &path).expect("saves");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back, doc);
        assert_eq!(back.active_side(), Side::Bottom);
        assert_eq!(back.components()[0].field.z[3], 4.0);
        std::fs::remove_file(&path).ok();
    }

    fn write_json(path: &FsPath, doc: &Document, json: &str) {
        let file = File::create(path).expect("creates");
        let mut zip = ZipWriter::new(BufWriter::new(file));
        let options = SimpleFileOptions::default();
        zip.start_file(DOCUMENT_ENTRY, options).expect("starts");
        zip.write_all(json.as_bytes()).expect("writes");
        for component in doc.components() {
            zip.start_file(heightfield_entry(component.id), options)
                .expect("starts");
            zip.write_all(&component.field.z_bytes()).expect("writes");
        }
        zip.finish().expect("finishes");
    }

    // Review 033 row 3: a number that is not finite is refused before the
    // save, and the last good file is left as it was.
    #[test]
    fn a_save_that_could_not_be_read_back_is_refused_and_keeps_the_old_file() {
        let path = scratch("not-finite");
        let mut doc = sample();
        save(&doc, &path).expect("first save");
        let good = std::fs::read(&path).expect("reads");
        let id = doc.order()[0];
        let mut entity = doc.entity(id).expect("entity").clone();
        entity.kind = EntityKind::Path {
            path: crate::geom::SubPath::open(Point::new(0.0, 0.0))
                .line_to(Point::new(f64::INFINITY, 60.0))
                .clone()
                .into_path(),
        };
        doc.begin("Scale");
        doc.replace_entity(id, entity).expect("replaces");
        doc.commit();
        let error = save(&doc, &path).expect_err("refused");
        assert!(error.to_string().contains("not finite"), "{error}");
        assert_eq!(std::fs::read(&path).expect("reads"), good);
        let mut partial = path.as_os_str().to_owned();
        partial.push(".partial");
        assert!(!std::path::Path::new(&partial).exists());
        std::fs::remove_file(&path).ok();
    }

    // Review 033 row 3: a file saved before then, with `null` for a number,
    // still opens, the number read as 0.
    #[test]
    fn a_file_with_null_for_a_number_still_opens() {
        let doc = sample();
        let text = serde_json::to_string_pretty(&doc.to_file()).expect("serialises");
        assert!(text.contains("\"x\": 100.0"));
        let broken = text.replacen("\"x\": 100.0", "\"x\": null", 2);
        let path = scratch("null-number");
        write_json(&path, &doc, &broken);
        let back = open(&path, doc.revision()).expect("opens");
        let fixed = text.replacen("\"x\": 100.0", "\"x\": 0", 2);
        write_json(&path, &doc, &fixed);
        let want = open(&path, doc.revision()).expect("opens");
        assert_eq!(back, want);
        assert_ne!(back, doc);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_format_one_file_opens_with_everything_on_the_top_side() {
        let doc = sample();
        let mut json = serde_json::to_value(doc.to_file()).expect("serialises");
        let object = json.as_object_mut().expect("an object");
        object.insert("format".to_string(), serde_json::json!(1));
        object.remove("activeSide");
        object.remove("bottom");
        let path = scratch("format-one");
        let file = File::create(&path).expect("creates");
        let mut zip = ZipWriter::new(BufWriter::new(file));
        let options = SimpleFileOptions::default();
        zip.start_file(DOCUMENT_ENTRY, options).expect("starts");
        zip.write_all(json.to_string().as_bytes()).expect("writes");
        for component in doc.components() {
            zip.start_file(heightfield_entry(component.id), options)
                .expect("starts");
            zip.write_all(&component.field.z_bytes()).expect("writes");
        }
        zip.finish().expect("finishes");
        let back = open(&path, doc.revision()).expect("opens");
        assert_eq!(back, doc);
        assert_eq!(back.active_side(), crate::doc::Side::Top);
        assert!(back.other_side().is_none());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn opening_records_the_path_and_leaves_the_document_clean() {
        let doc = sample();
        let path = scratch("clean");
        save(&doc, &path).expect("saves");
        let back = open(&path, 9).expect("opens");
        assert_eq!(back.file_path(), Some(path.as_path()));
        assert!(!back.is_dirty());
        assert_eq!(back.revision(), 9);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_that_is_not_a_document_is_reported() {
        let path = scratch("not-a-document");
        std::fs::write(&path, b"not a zip at all").expect("writes");
        let error = open(&path, 0).expect_err("this is not a document");
        assert!(
            matches!(error, CoreError::Io { .. } | CoreError::Validation(_)),
            "{error}"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_zip_without_a_document_is_reported() {
        let path = scratch("empty-zip");
        let file = File::create(&path).expect("creates");
        let mut zip = ZipWriter::new(BufWriter::new(file));
        zip.add_directory(IMAGES_FOLDER, SimpleFileOptions::default())
            .expect("adds");
        zip.finish().expect("finishes");
        let error = open(&path, 0).expect_err("no document inside");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn saving_to_a_folder_that_does_not_exist_is_reported() {
        let doc = Document::new();
        let path = std::path::Path::new("/zeroed-no-such-folder/x.carve");
        let error = save(&doc, path).expect_err("no such folder");
        assert!(matches!(error, CoreError::Io { .. }), "{error}");
    }
}
