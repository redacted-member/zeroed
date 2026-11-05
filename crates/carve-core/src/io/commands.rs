//! Kernel commands for the model and view exports: `model.exportMesh`
//! (STL, OBJ or POV-Ray), `model.meshStats` (the triangle count and
//! deviation an export would have), `model.meshPreview` (the triangles
//! themselves, for the 3D view), `model.exportHeightmap` (greyscale PNG) and
//! `io.saveImage` (a picture of a view the front end rendered). None of
//! them change the document.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::Point;
use crate::kernel::KernelReply;
use crate::model::{self, field::Field, Component};

use super::heightmap;
use super::mesh_export::{self, BackFace, Backing, MeshFormat, RotaryWrap};
use super::mesh_reduce::Reduction;
use super::view_image;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.exportMesh"
            | "model.meshStats"
            | "model.meshPreview"
            | "model.exportHeightmap"
            | "io.saveImage"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse,
/// [`CoreError::Validation`] when there is no model or a value is not
/// legal, and [`CoreError::Io`] when the file cannot be written.
pub fn dispatch(document: &Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "model.exportMesh" => {
            let args: ExportMeshArgs = parse(name, args)?;
            let format = match args.format {
                Some(format) => format,
                None => format_of_path(&args.path)?,
            };
            let reduction = args.shape.reduction()?;
            let meshed = meshed_model(document, &args.shape)?;
            let field = &meshed.field;
            let lowest = f64::from(field.min().unwrap_or(0.0)).min(0.0);
            let texture =
                (format == MeshFormat::Obj).then(|| texture(document, field, meshed.offset));
            let stats = mesh_export::write_model(
                field,
                meshed.backing(),
                lowest - args.shape.base,
                args.shape.back,
                reduction,
                format,
                texture.as_ref(),
                &args.path,
            )?;
            KernelReply::ok(&json!({
                "format": format,
                "triangles": stats.triangles,
                "deviation": stats.deviation,
            }))
        }
        "model.meshStats" => {
            let args: MeshShape = parse(name, args)?;
            let reduction = args.reduction()?;
            let meshed = meshed_model(document, &args)?;
            let lowest = f64::from(meshed.field.min().unwrap_or(0.0)).min(0.0);
            let (_, stats) = mesh_export::model_triangles(
                &meshed.field,
                meshed.backing(),
                lowest - args.base,
                args.back,
                reduction,
            );
            KernelReply::ok(&stats)
        }
        "model.meshPreview" => {
            let args: MeshShape = parse(name, args)?;
            let reduction = args.reduction()?;
            let meshed = meshed_model(document, &args)?;
            let lowest = f64::from(meshed.field.min().unwrap_or(0.0)).min(0.0);
            let (triangles, stats) = mesh_export::model_triangles(
                &meshed.field,
                meshed.backing(),
                lowest - args.base,
                args.back,
                reduction,
            );
            if triangles.len() > MAX_PREVIEW_TRIANGLES {
                return Err(CoreError::validation(
                    "the mesh has too many triangles to show; reduce it first",
                ));
            }
            Ok(KernelReply::Bytes(preview_blob(&triangles, stats)?))
        }
        "model.exportHeightmap" => {
            let args: ExportHeightmapArgs = parse(name, args)?;
            let field = match args.ids.as_deref() {
                None | Some([]) => {
                    if !document.components().iter().any(|c| c.visible) {
                        return Err(CoreError::validation("there is no model yet"));
                    }
                    document.composite()
                }
                Some(ids) => {
                    let chosen = chosen(document, ids)?;
                    model::bake(&chosen, &model::grid_for(document.job()))
                }
            };
            let info = heightmap::write(&field, args.bits, &args.path)?;
            KernelReply::ok(&info)
        }
        "io.saveImage" => {
            let args: SaveImageArgs = parse(name, args)?;
            view_image::save(&args.data, &args.path)?;
            KernelReply::ok(&json!({}))
        }
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

fn parse<T: serde::de::DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        trimmed
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

fn format_of_path(path: &std::path::Path) -> Result<MeshFormat, CoreError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "stl" => Ok(MeshFormat::Stl),
        "obj" => Ok(MeshFormat::Obj),
        "pov" => Ok(MeshFormat::Pov),
        _ => Err(CoreError::validation(
            "the model can be exported as STL, OBJ or POV",
        )),
    }
}

/// The chosen components, shown even when hidden in the model.
fn chosen(document: &Document, ids: &[u64]) -> Result<Vec<Component>, CoreError> {
    ids.iter()
        .map(|id| {
            document.component(*id).map(|c| Component {
                visible: true,
                ..c.clone()
            })
        })
        .collect()
}

/// Most triangles `model.meshPreview` sends to the view.
const MAX_PREVIEW_TRIANGLES: usize = 3_000_000;

/// The preview blob: the header `{triangles, deviation}` and one `f32`
/// section `xyz`, nine numbers a triangle in job millimetres (a wrapped
/// rotary model round its axis).
fn preview_blob(
    triangles: &[super::mesh::Triangle],
    stats: mesh_export::MeshStats,
) -> Result<Vec<u8>, CoreError> {
    let count = triangles.len() * 9;
    let header = serde_json::to_vec(&json!({
        "triangles": stats.triangles,
        "deviation": stats.deviation,
        "sections": [{ "name": "xyz", "dtype": "f32", "offset": 0, "count": count }],
    }))?;
    let mut out = Vec::with_capacity(4 + header.len() + count * 4);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    for v in triangles.iter().flatten().flatten() {
        out.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    Ok(out)
}

/// The model a mesh export covers and what its back needs.
struct Meshed {
    field: Field,
    /// Where `field` starts on the job's model grid, in cells.
    offset: (usize, usize),
    /// For a back closed with the other side: how deep that side's surface
    /// lies below this side's zero plane, turned over onto the same cells.
    other: Option<Field>,
    /// For a rotary model closed round its blank.
    wrap: Option<RotaryWrap>,
}

impl Meshed {
    fn backing(&self) -> Backing<'_> {
        Backing {
            other: self.other.as_ref(),
            wrap: self.wrap,
        }
    }
}

/// The model a mesh export covers: the chosen components, or for a back
/// closed with the other side or round a rotary blank, the whole model over
/// the job.
fn meshed_model(document: &Document, shape: &MeshShape) -> Result<Meshed, CoreError> {
    let job = document.job();
    match shape.back {
        BackFace::OtherSide => {}
        BackFace::Sides => {
            if job.kind != crate::doc::JobKind::Rotary {
                return Err(CoreError::validation(
                    "closing the sides needs a rotary job",
                ));
            }
            if !document.components().iter().any(|c| c.visible) {
                return Err(CoreError::validation("there is no model yet"));
            }
            let field = document.composite();
            // The model's highest point sits on the blank's surface, as the
            // 3D view wraps it.
            let radius = job.rotary.diameter / 2.0;
            let peak = f64::from(field.max().unwrap_or(0.0)).max(0.0);
            let wrap = RotaryWrap {
                radius,
                along_x: job.rotary.axis == crate::doc::RotaryAxis::X,
                zero: radius - peak.min(job.rotary.diameter),
            };
            return Ok(Meshed {
                field,
                offset: (0, 0),
                other: None,
                wrap: Some(wrap),
            });
        }
        _ => {
            let (field, offset) = model_field(document, shape.ids.as_deref())?;
            return Ok(Meshed {
                field,
                offset,
                other: None,
                wrap: None,
            });
        }
    }
    let job = document.job();
    let other = document
        .other_side_composite()
        .filter(|_| job.kind == crate::doc::JobKind::DoubleSided)
        .ok_or_else(|| {
            CoreError::validation(
                "closing with the other side needs a double sided job with a model on the other side",
            )
        })?;
    let field = document.composite();
    // Both sides' models stand on one shared modelling plane, facing away
    // from each other, as the 3D view shows a double sided job. Measured
    // down from this side's zero plane, the other side's surface is `h`
    // deep, and never above this side's surface.
    let height = |f: &Field, i: i64, j: i64| {
        let z = f.get(i, j);
        if z.is_nan() {
            0.0
        } else {
            f64::from(z)
        }
    };
    let mut turned = field.clone();
    for j in 0..field.height {
        for i in 0..field.width {
            let c = field.centre(i, j);
            let (x, y) = job.flip_xy(c.x, c.y);
            let oi = ((x - other.origin.x) / other.cell).round() as i64;
            let oj = ((y - other.origin.y) / other.cell).round() as i64;
            let depth = height(&other, oi, oj);
            let top = height(&field, i as i64, j as i64);
            turned.z[j * field.width + i] = depth.max(-top) as f32;
        }
    }
    Ok(Meshed {
        field,
        offset: (0, 0),
        other: Some(turned),
        wrap: None,
    })
}

/// The component colours over `field`, which starts `offset` cells into the
/// job's model grid; cells with no colour take the job's material colour.
fn texture(document: &Document, field: &Field, offset: (usize, usize)) -> mesh_export::Texture {
    let grid = model::grid_for(document.job());
    let colours = document.composite_colors();
    let plain = document.job().appearance.colour;
    let (w, h) = (field.width, field.height);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for row in 0..h {
        let j = h - 1 - row;
        for i in 0..w {
            let (gi, gj) = (offset.0 + i, offset.1 + j);
            let at = (gj * grid.width + gi) * 4;
            match colours.get(at..at + 4) {
                Some(px) if px[3] != 0 && gi < grid.width => rgb.extend_from_slice(&px[..3]),
                _ => rgb.extend_from_slice(&plain),
            }
        }
    }
    mesh_export::Texture {
        width: u32::try_from(w).unwrap_or(u32::MAX),
        height: u32::try_from(h).unwrap_or(u32::MAX),
        rgb,
    }
}

/// The model on its grid, cut down to the box the components cover plus a
/// cell of margin: every visible component, or only the chosen ones, baked
/// with empty cells at the zero plane. The same area the STL export covers.
/// Also answers where the cut starts on the job's model grid, in cells.
fn model_field(
    document: &Document,
    ids: Option<&[u64]>,
) -> Result<(Field, (usize, usize)), CoreError> {
    let chosen: Vec<Component> = match ids {
        None | Some([]) => document
            .components()
            .into_iter()
            .filter(|c| c.visible)
            .cloned()
            .collect(),
        Some(ids) => chosen(document, ids)?,
    };
    if chosen.is_empty() {
        return Err(CoreError::validation("there is no model yet"));
    }
    let grid = model::grid_for(document.job());
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &chosen {
        let f = &c.field;
        lo_x = lo_x.min(f.origin.x);
        lo_y = lo_y.min(f.origin.y);
        hi_x = hi_x.max(f.origin.x + (f.width as f64 - 1.0) * f.cell);
        hi_y = hi_y.max(f.origin.y + (f.height as f64 - 1.0) * f.cell);
    }
    let cell_of = |v: f64, o: f64, n: usize| {
        ((v - o) / grid.cell)
            .round()
            .clamp(0.0, n.saturating_sub(1) as f64) as usize
    };
    let i0 = cell_of(lo_x, grid.origin.x, grid.width).saturating_sub(1);
    let j0 = cell_of(lo_y, grid.origin.y, grid.height).saturating_sub(1);
    let i1 = (cell_of(hi_x, grid.origin.x, grid.width) + 1).min(grid.width - 1);
    let j1 = (cell_of(hi_y, grid.origin.y, grid.height) + 1).min(grid.height - 1);
    let local = model::shape::Grid {
        width: i1 - i0 + 1,
        height: j1 - j0 + 1,
        cell: grid.cell,
        origin: Point::new(
            grid.origin.x + i0 as f64 * grid.cell,
            grid.origin.y + j0 as f64 * grid.cell,
        ),
    };
    Ok((model::bake(&chosen, &local), (i0, j0)))
}

fn one_millimetre() -> f64 {
    1.0
}

fn sixteen() -> u8 {
    16
}

/// Which model is meshed and how.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshShape {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Gap between the lowest point and the flat (or mirror) plane, mm.
    #[serde(default = "one_millimetre")]
    base: f64,
    #[serde(default)]
    back: BackFace,
    /// Largest height difference allowed, mm; every cell when absent.
    #[serde(default)]
    tolerance: Option<f64>,
    /// Most triangles the surface may have; wins over `tolerance`.
    #[serde(default)]
    max_triangles: Option<usize>,
}

impl MeshShape {
    fn reduction(&self) -> Result<Reduction, CoreError> {
        if !(self.base.is_finite() && self.base >= 0.0) {
            return Err(CoreError::validation("the base must be 0 or more"));
        }
        match (self.max_triangles, self.tolerance) {
            (Some(count), _) if count >= 2 => Ok(Reduction::MaxTriangles(count)),
            (Some(_), _) => Err(CoreError::validation(
                "the model needs at least 2 triangles",
            )),
            (None, Some(t)) if t.is_finite() && t > 0.0 => Ok(Reduction::Tolerance(t)),
            (None, Some(_)) => Err(CoreError::validation("the tolerance must be above 0")),
            (None, None) => Ok(Reduction::Full),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportMeshArgs {
    path: PathBuf,
    /// Taken from the path's extension when absent.
    #[serde(default)]
    format: Option<MeshFormat>,
    #[serde(flatten)]
    shape: MeshShape,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportHeightmapArgs {
    path: PathBuf,
    #[serde(default = "sixteen")]
    bits: u8,
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveImageArgs {
    path: PathBuf,
    /// PNG bytes as base64, optionally as a `data:` URL.
    data: String,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::kernel::Kernel;

    fn reply(kernel: &mut Kernel, name: &str, args: Value) -> Value {
        match kernel.apply(name, &args.to_string()).expect(name) {
            crate::kernel::KernelReply::Json(text) => {
                let v: Value = serde_json::from_str(&text).expect("json");
                v["result"].clone()
            }
            crate::kernel::KernelReply::Bytes(_) => Value::Null,
        }
    }

    fn with_model() -> Kernel {
        let mut kernel = Kernel::new();
        let path = crate::geom::rect(
            crate::geom::Point::new(20.0, 20.0),
            crate::geom::Point::new(60.0, 50.0),
        )
        .expect("rect");
        let made = reply(
            &mut kernel,
            "entities.create",
            serde_json::json!({ "entity": { "kind": "path", "layer": 1, "sheet": 1, "path": path } }),
        );
        let id = made["id"].as_u64().expect("id");
        reply(
            &mut kernel,
            "model.createShape",
            serde_json::json!({ "ids": [id] }),
        );
        kernel
    }

    fn dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("carve_io_commands_test");
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn the_model_exports_in_each_mesh_format() {
        let mut kernel = with_model();
        for ext in ["stl", "obj", "pov"] {
            let path = dir().join(format!("model.{ext}"));
            let out = reply(
                &mut kernel,
                "model.exportMesh",
                serde_json::json!({ "path": path }),
            );
            assert_eq!(out["format"], ext);
            let text = std::fs::read(&path).expect("written");
            assert!(!text.is_empty());
        }
        let obj = crate::io::mesh::read(&dir().join("model.obj")).expect("obj");
        let stl = crate::io::mesh::read(&dir().join("model.stl")).expect("stl");
        assert_eq!(obj.len(), stl.len());
        let error = kernel
            .apply(
                "model.exportMesh",
                &serde_json::json!({ "path": dir().join("model.xyz") }).to_string(),
            )
            .expect_err("unknown format");
        assert!(error.to_string().contains("OBJ"), "{error}");
    }

    #[test]
    fn stats_follow_the_reduction_and_the_back() {
        let mut kernel = with_model();
        let full = reply(&mut kernel, "model.meshStats", serde_json::json!({}));
        let reduced = reply(
            &mut kernel,
            "model.meshStats",
            serde_json::json!({ "tolerance": 0.1 }),
        );
        let budget = reply(
            &mut kernel,
            "model.meshStats",
            serde_json::json!({ "maxTriangles": 300, "back": "open" }),
        );
        let count = |v: &serde_json::Value| v["triangles"].as_u64().expect("count");
        assert!(count(&reduced) < count(&full));
        assert!(count(&budget) <= 300);
        assert_eq!(full["deviation"], 0.0);
        assert!(reduced["deviation"].as_f64().expect("deviation") <= 0.1);
        let path = dir().join("stats.obj");
        let written = reply(
            &mut kernel,
            "model.exportMesh",
            serde_json::json!({ "path": path, "tolerance": 0.1, "back": "inverted" }),
        );
        assert_eq!(written["format"], "obj");
        assert!(count(&written) > count(&reduced));
        assert!(dir().join("stats.mtl").exists() && dir().join("stats.jpg").exists());
        for bad in [
            serde_json::json!({ "tolerance": 0 }),
            serde_json::json!({ "maxTriangles": 1 }),
            serde_json::json!({ "base": -1 }),
        ] {
            assert!(kernel.apply("model.meshStats", &bad.to_string()).is_err());
        }
    }

    #[test]
    fn a_double_sided_model_closes_with_its_other_side() {
        let mut kernel = with_model();
        let error = kernel
            .apply("model.meshStats", r#"{"back":"otherSide"}"#)
            .expect_err("single sided");
        assert!(error.to_string().contains("double sided"), "{error}");
        let mut job = *kernel.document().job_setup();
        job.kind = crate::doc::JobKind::DoubleSided;
        reply(
            &mut kernel,
            "job.set",
            serde_json::to_value(job).expect("job"),
        );
        reply(
            &mut kernel,
            "job.setSide",
            serde_json::json!({ "side": "bottom" }),
        );
        let path = crate::geom::rect(
            crate::geom::Point::new(10.0, 10.0),
            crate::geom::Point::new(40.0, 30.0),
        )
        .expect("rect");
        let (layer, sheet) = (
            kernel.document().active_layer(),
            kernel.document().active_sheet(),
        );
        let made = reply(
            &mut kernel,
            "entities.create",
            serde_json::json!({ "entity": { "kind": "path", "layer": layer, "sheet": sheet, "path": path } }),
        );
        reply(
            &mut kernel,
            "model.createShape",
            serde_json::json!({ "ids": [made["id"]] }),
        );
        reply(
            &mut kernel,
            "job.setSide",
            serde_json::json!({ "side": "top" }),
        );
        let path = dir().join("both.stl");
        let written = reply(
            &mut kernel,
            "model.exportMesh",
            serde_json::json!({ "path": path, "back": "otherSide", "tolerance": 0.05 }),
        );
        assert!(written["triangles"].as_u64().expect("count") > 4);
        let triangles = crate::io::mesh::read(&path).expect("reads");
        let lowest = triangles
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::MAX, f64::min);
        let highest = triangles
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::MIN, f64::max);
        assert!(lowest < -1.0 && highest > 1.0, "{lowest} {highest}");
    }

    #[test]
    fn a_rotary_model_closes_round_its_blank() {
        let mut kernel = with_model();
        let error = kernel
            .apply("model.meshStats", r#"{"back":"sides"}"#)
            .expect_err("not rotary");
        assert!(error.to_string().contains("rotary"), "{error}");
        let mut job = *kernel.document().job_setup();
        job.kind = crate::doc::JobKind::Rotary;
        job.rotary.diameter = 40.0;
        reply(
            &mut kernel,
            "job.set",
            serde_json::to_value(job).expect("job"),
        );
        let path = dir().join("turned.stl");
        let written = reply(
            &mut kernel,
            "model.exportMesh",
            serde_json::json!({ "path": path, "back": "sides", "tolerance": 0.05 }),
        );
        assert!(written["triangles"].as_u64().expect("count") > 100);
        let triangles = crate::io::mesh::read(&path).expect("reads");
        // Everything lies within the blank's radius of the axis.
        let axis_x = kernel.document().job().rotary.axis == crate::doc::RotaryAxis::X;
        let reach = triangles
            .iter()
            .flatten()
            .map(|p| {
                if axis_x {
                    p[1].hypot(p[2])
                } else {
                    p[0].hypot(p[2])
                }
            })
            .fold(0.0, f64::max);
        assert!(reach <= 20.0 + 1e-6 && reach > 19.0, "{reach}");
    }

    #[test]
    fn the_preview_sends_the_triangles_the_export_writes() {
        let mut kernel = with_model();
        let blob = match kernel
            .apply("model.meshPreview", r#"{"tolerance":0.1,"back":"open"}"#)
            .expect("preview")
        {
            crate::kernel::KernelReply::Bytes(bytes) => bytes,
            crate::kernel::KernelReply::Json(_) => panic!("expected bytes"),
        };
        let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        let header: Value = serde_json::from_slice(&blob[4..4 + len]).expect("header");
        let triangles = header["triangles"].as_u64().expect("count") as usize;
        assert_eq!(header["sections"][0]["count"], triangles * 9);
        assert_eq!(blob.len(), 4 + len + triangles * 36);
        let stats = reply(
            &mut kernel,
            "model.meshStats",
            serde_json::json!({ "tolerance": 0.1, "back": "open" }),
        );
        assert_eq!(stats["triangles"], triangles);
    }

    #[test]
    fn the_heightmap_covers_the_job() {
        let mut kernel = with_model();
        let path = dir().join("height.png");
        let info = reply(
            &mut kernel,
            "model.exportHeightmap",
            serde_json::json!({ "path": path }),
        );
        let composite = kernel.document().composite();
        assert_eq!(info["width"], composite.width);
        assert_eq!(info["height"], composite.height);
        assert!(info["maxZ"].as_f64().expect("max") > 4.0);
        let image = image::open(&path).expect("png");
        assert!(image.as_luma16().is_some());
    }

    #[test]
    fn an_empty_model_is_refused() {
        let mut kernel = Kernel::new();
        let path = dir().join("empty.obj");
        assert!(kernel
            .apply(
                "model.exportMesh",
                &serde_json::json!({ "path": path }).to_string()
            )
            .is_err());
        assert!(kernel
            .apply(
                "model.exportHeightmap",
                &serde_json::json!({ "path": dir().join("empty.png") }).to_string()
            )
            .is_err());
    }
}
