//! Vector file and bitmap trace commands: import and export vectors, read a
//! bitmap's size and trace it into vectors.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::{Document, Entity, EntityKind, Layer};
use crate::error::CoreError;
use crate::geom::text::FontLibrary;
use crate::geom::{self, Path, Point};
use crate::io;
use crate::kernel::{edit, parse_args, KernelReply};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "io.importVectors"
            | "io.vectorPages"
            | "io.exportVectors"
            | "io.saveText"
            | "io.readText"
            | "bitmap.info"
            | "bitmap.trace"
            | "bitmaps.load"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(
    document: &mut Document,
    fonts: Option<&FontLibrary>,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "io.importVectors" => {
            let args: ImportArgs = parse_args(name, args)?;
            let options = args.options.unwrap_or_default();
            let io::VectorImport {
                paths,
                images,
                mut warnings,
            } = io::import_vectors(
                &args.path,
                &io::ImportOptions {
                    scale: options.scale.unwrap_or(1.0),
                    units_hint: options.units_hint.as_deref(),
                    page: options.page,
                    fonts,
                },
            )?;
            // Check every picture decodes before the document changes.
            let mut pictures = Vec::with_capacity(images.len());
            for image in images {
                match crate::doc::bitmap_cmd::stored_image(
                    image.bytes.clone(),
                    image.ext.as_deref(),
                ) {
                    Ok(stored) => pictures.push((image, stored)),
                    Err(_) => {
                        if !warnings.contains(&io::ImportWarning::ImageSkipped) {
                            warnings.push(io::ImportWarning::ImageSkipped);
                        }
                    }
                }
            }
            if paths.is_empty() && pictures.is_empty() && warnings.is_empty() {
                return Err(CoreError::validation(
                    "that file has no vectors in it to import",
                ));
            }
            let target = options.layer;
            edit(document, "Import Vectors", |doc| {
                let mut ids = Vec::with_capacity(paths.len());
                for imported in paths {
                    let layer = match target {
                        Some(layer) => layer,
                        None => layer_named(doc, &imported.layer),
                    };
                    let sheet = doc.active_sheet();
                    ids.push(doc.insert_entity(Entity {
                        rotation: 0.0,
                        id: 0,
                        layer,
                        sheet,
                        revision: 0,
                        name: imported.name,
                        parent: None,
                        kind: EntityKind::Path {
                            path: imported.path,
                        },
                    }));
                }
                for (image, stored) in pictures {
                    let layer = match target {
                        Some(layer) => layer,
                        None => layer_named(doc, &image.layer),
                    };
                    let sheet = doc.active_sheet();
                    let image_id = doc.images_mut().insert(stored);
                    ids.push(doc.insert_entity(Entity {
                        rotation: 0.0,
                        id: 0,
                        layer,
                        sheet,
                        revision: 0,
                        name: image.name,
                        parent: None,
                        kind: EntityKind::Bitmap(crate::doc::BitmapRef {
                            image_id,
                            quad: image.quad.to_vec(),
                            opacity: 1.0,
                        }),
                    }));
                }
                doc.set_selection(&ids);
                Ok(json!({
                    "ids": ids,
                    "warnings": warnings
                        .iter()
                        .map(|w| {
                            let warning = w.to_warning();
                            json!({
                                "kind": w,
                                "code": warning.code,
                                "params": warning.params,
                                "message": warning.message,
                            })
                        })
                        .collect::<Vec<Value>>(),
                }))
            })
        }
        "io.vectorPages" => {
            let args: FileArgs = parse_args(name, args)?;
            KernelReply::ok(&json!({ "pages": io::page_count(&args.path)? }))
        }
        "io.exportVectors" => {
            let args: ExportArgs = parse_args(name, args)?;
            export_vectors(document, &args)?;
            KernelReply::ok(&json!({ "revision": document.revision() }))
        }
        "io.saveText" => {
            let args: SaveTextArgs = parse_args(name, args)?;
            std::fs::write(&args.path, args.text.as_bytes())
                .map_err(|e| CoreError::io(args.path.display(), e))?;
            KernelReply::ok(&json!({}))
        }
        "io.readText" => {
            let args: FileArgs = parse_args(name, args)?;
            KernelReply::ok(&json!({ "text": read_text(&args.path)? }))
        }
        "bitmap.info" => {
            let args: FileArgs = parse_args(name, args)?;
            let bitmap = io::bitmap::read(&args.path)?;
            KernelReply::ok(&json!({ "width": bitmap.width, "height": bitmap.height }))
        }
        "bitmap.trace" => {
            let args: TraceArgs = parse_args(name, args)?;
            let full = io::bitmap::read(&args.path)?;
            let (width, height) = (f64::from(full.width), f64::from(full.height));
            if width == 0.0 || height == 0.0 {
                return Err(CoreError::validation("the image has no pixels"));
            }
            let job = document.job_bounds();
            let pixel_size = args
                .pixel_size
                .unwrap_or_else(|| (job.width() / width).min(job.height() / height));
            let origin = args.origin.unwrap_or_else(|| {
                let centre = job.centre();
                Point::new(
                    centre.x - width * pixel_size / 2.0,
                    centre.y - height * pixel_size / 2.0,
                )
            });
            let factor = full.downsample_factor(TRACE_MAX_PIXELS);
            let bitmap = full.downsample(TRACE_MAX_PIXELS);
            let mut params = args.params;
            params.noise /= f64::from(factor * factor);
            params.smoothness = (params.smoothness / f64::from(factor)).max(0.05);
            // A downsampled image keeps its size in the job: the bottom
            // row of blocks may hang past the original's bottom edge by
            // less than one block, so anchor at the top edge.
            let shift = f64::from(bitmap.height * factor) - height;
            let placement = geom::trace::Placement {
                origin: Point::new(origin.x, origin.y - shift * pixel_size),
                pixel_size: pixel_size * f64::from(factor),
            };
            let paths = geom::trace::trace(&bitmap, &params, placement)?;
            if paths.is_empty() {
                return Err(CoreError::validation(
                    "nothing in the image passes the threshold; try another threshold or invert",
                ));
            }
            let layer = document.active_layer();
            let sheet = document.active_sheet();
            edit(document, "Trace Bitmap", |doc| {
                let ids: Vec<u64> = paths
                    .into_iter()
                    .map(|path| {
                        doc.insert_entity(Entity {
                            rotation: 0.0,
                            id: 0,
                            layer,
                            sheet,
                            revision: 0,
                            name: None,
                            parent: None,
                            kind: EntityKind::Path { path },
                        })
                    })
                    .collect();
                doc.set_selection(&ids);
                Ok(json!({ "ids": ids }))
            })
        }
        "bitmaps.load" => Err(CoreError::unsupported("bitmaps are not in this build yet")),
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// Writes the named vectors, or the whole active sheet, to a file.
fn export_vectors(document: &Document, args: &ExportArgs) -> Result<(), CoreError> {
    let doc = document;
    let ids = match args.ids.as_deref() {
        Some(ids) if !ids.is_empty() => ids.to_vec(),
        _ if !doc.selection().is_empty() => doc.selection().to_vec(),
        _ => doc.list(None).into_iter().map(|s| s.id).collect(),
    };
    if ids.is_empty() {
        return Err(CoreError::validation("there is nothing to export"));
    }
    // Group by layer so each format can write its own layer structure.
    let mut grouped: Vec<(Layer, Vec<Path>)> = doc
        .layers()
        .iter()
        .map(|layer| (layer.clone(), Vec::new()))
        .collect();
    for id in &ids {
        for (leaf, path) in doc.paths_of(*id) {
            let entity = doc.entity(leaf)?;
            // A placed bitmap is a picture, not a vector: its frame is not exported.
            if matches!(entity.kind, EntityKind::Bitmap(_)) {
                continue;
            }
            let layer = entity.layer;
            if let Some(slot) = grouped.iter_mut().find(|(l, _)| l.id == layer) {
                slot.1.push(path);
            }
        }
    }
    grouped.retain(|(_, paths)| !paths.is_empty());
    if grouped.is_empty() {
        return Err(CoreError::validation(
            "the objects selected have no geometry to export",
        ));
    }
    let format = args
        .format
        .or_else(|| io::VectorFormat::of_path(&args.path))
        .ok_or_else(|| {
            CoreError::validation("say which format to export, or use a known extension")
        })?;
    match format {
        io::VectorFormat::Svg => {
            let all: Vec<Path> = grouped.iter().flat_map(|(_, p)| p.clone()).collect();
            let bounds = io::svg::export_bounds(doc.job_bounds(), &all);
            let layers: Vec<io::svg::ExportLayer> = grouped
                .iter()
                .map(|(layer, paths)| io::svg::ExportLayer {
                    name: layer.name.clone(),
                    colour: layer.colour.clone(),
                    paths: paths.clone(),
                })
                .collect();
            let text = io::svg::export(&layers, bounds);
            std::fs::write(&args.path, text).map_err(|e| CoreError::io(args.path.display(), e))?;
            Ok(())
        }
        io::VectorFormat::Dxf => {
            let layers: Vec<io::dxf::ExportLayer> = grouped
                .iter()
                .map(|(layer, paths)| io::dxf::ExportLayer {
                    name: layer.name.clone(),
                    paths: paths.clone(),
                })
                .collect();
            io::dxf::export(&layers, &args.path)
        }
        io::VectorFormat::Eps | io::VectorFormat::Ai | io::VectorFormat::Pdf => {
            let all: Vec<Path> = grouped.iter().flat_map(|(_, p)| p.clone()).collect();
            let bounds = io::svg::export_bounds(doc.job_bounds(), &all);
            let layers: Vec<io::svg::ExportLayer> = grouped
                .iter()
                .map(|(layer, paths)| io::svg::ExportLayer {
                    name: layer.name.clone(),
                    colour: layer.colour.clone(),
                    paths: paths.clone(),
                })
                .collect();
            let bytes = io::export_page(format, &layers, bounds).ok_or_else(|| {
                CoreError::unsupported(format!("{} is not a page format", format.label()))
            })?;
            std::fs::write(&args.path, bytes).map_err(|e| CoreError::io(args.path.display(), e))
        }
    }
}

/// Finds a layer by name, adding one when the document has no such layer,
/// which is what an import does with the layers it finds in a file.
fn layer_named(doc: &mut Document, name: &str) -> u64 {
    if let Some(layer) = doc.layers().iter().find(|l| l.name == name) {
        return layer.id;
    }
    doc.add_layer(Some(name.to_string()), None)
}

/// Images larger than this are averaged down before tracing.
const TRACE_MAX_PIXELS: u64 = 4_000_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileArgs {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TraceArgs {
    path: PathBuf,
    #[serde(default)]
    params: geom::trace::TraceParams,
    #[serde(default)]
    origin: Option<Point>,
    #[serde(default)]
    pixel_size: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ImportOptions {
    layer: Option<u64>,
    scale: Option<f64>,
    units_hint: Option<String>,
    /// Page of a PDF to read, counting from 1.
    page: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportArgs {
    path: PathBuf,
    #[serde(default)]
    options: Option<ImportOptions>,
}

/// A text file to write as it is (a cut list CSV).
/// Largest settings file `io.readText` reads.
const MAX_TEXT_BYTES: u64 = 1 << 20;

/// A small UTF-8 text file, such as saved form settings.
fn read_text(path: &std::path::Path) -> Result<String, CoreError> {
    let size = std::fs::metadata(path)
        .map_err(|e| CoreError::io(path.display(), e))?
        .len();
    if size > MAX_TEXT_BYTES {
        return Err(CoreError::validation(
            "the file is too large to be a settings file",
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path.display(), e))?;
    String::from_utf8(bytes).map_err(|_| CoreError::validation("the file is not a text file"))
}

#[derive(Debug, Deserialize)]
struct SaveTextArgs {
    path: PathBuf,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    path: PathBuf,
    #[serde(default)]
    format: Option<io::VectorFormat>,
    #[serde(default)]
    ids: Option<Vec<u64>>,
}
