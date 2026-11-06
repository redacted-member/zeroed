//! Kernel commands of the 3D model import flow: `model.import.info`,
//! `model.import.preview`, `model.import.apply`,
//! `model.import.segmentPreview`, `model.import.segmentApply` and
//! `model.import.settings`. The mesh read last is kept so that previews
//! do not read the file again.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::Tolerance;
use crate::io;
use crate::io::mesh::Triangle;
use crate::kernel::KernelReply;

use super::super::field::Field;
use super::super::{contour, grid_for, Combine, Component};
use super::orient::{natural_size, orient, Orientation, Oriented};
use super::place::{place, undercuts, Placement};
use super::rotary::{lay_on_axis, radial_field, RotaryFit};
use super::segment::{self, SegmentPlan};
use super::ImportSource;

/// Most triangles a preview sends; larger meshes are thinned for display.
pub const PREVIEW_TRIANGLES: usize = 400_000;

/// The mesh file read last, kept while its size and time stay the same.
#[derive(Debug, Default)]
pub struct MeshCache {
    key: Option<(PathBuf, Option<SystemTime>, u64)>,
    triangles: Arc<Vec<Triangle>>,
}

impl MeshCache {
    /// The triangles of `path`, from the cache when the file is unchanged.
    ///
    /// # Errors
    /// What [`io::mesh::read`] reports.
    pub fn read(&mut self, path: &FsPath) -> Result<Arc<Vec<Triangle>>, CoreError> {
        let meta = std::fs::metadata(path).map_err(|error| CoreError::io(path.display(), error))?;
        let key = (path.to_path_buf(), meta.modified().ok(), meta.len());
        if self.key.as_ref() != Some(&key) {
            self.triangles = Arc::new(io::mesh::read(path)?);
            self.key = Some(key);
        }
        Ok(Arc::clone(&self.triangles))
    }
}

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.import.info"
            | "model.import.preview"
            | "model.import.apply"
            | "model.import.segmentPreview"
            | "model.import.segmentApply"
            | "model.import.settings"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshArgs {
    path: PathBuf,
    #[serde(default)]
    orient: Orientation,
    #[serde(default)]
    place: Placement,
    #[serde(default)]
    plan: SegmentPlan,
    /// For a segment preview: laid out (true) or where they are in the
    /// model (false).
    #[serde(default)]
    layout: bool,
    #[serde(default)]
    combine: Combine,
    #[serde(default)]
    name: Option<String>,
    /// Components the import replaces (a reimport).
    #[serde(default)]
    replace: Vec<u64>,
    /// A full 3D model on a rotary job: how it sits on the axis. Needs a
    /// rotary job.
    #[serde(default)]
    rotary: Option<RotaryFit>,
    /// With `rotary`: make the blank the model's length and diameter.
    #[serde(default)]
    resize_material: bool,
}

#[derive(Deserialize)]
struct IdArgs {
    id: u64,
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] or [`CoreError::Io`] for files and settings
/// that cannot be imported.
pub fn dispatch(
    document: &mut Document,
    cache: &mut MeshCache,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    if name == "model.import.settings" {
        let args: IdArgs = parse(name, args)?;
        return settings(document, args.id);
    }
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let args: MeshArgs = parse(name, args)?;
    let triangles = cache.read(&args.path)?;
    let centre = document.job_bounds().centre();
    let oriented = orient(&triangles, &args.orient, centre)?;
    if let Some(fit) = args.rotary {
        return rotary_command(document, name, &args, &triangles, &oriented, &fit);
    }
    match name {
        "model.import.info" => KernelReply::ok(&json!({
            "triangles": triangles.len(),
            "natural": natural_size(&triangles, &args.orient),
            "size": oriented.size,
            "centre": oriented.centre,
        })),
        "model.import.preview" => {
            let flags: Vec<u8> = undercuts(&oriented.triangles)
                .into_iter()
                .map(u8::from)
                .collect();
            let segment_of = vec![0u16; oriented.triangles.len()];
            Ok(blob(
                json!({ "size": oriented.size, "centre": oriented.centre }),
                &oriented.triangles,
                &flags,
                &segment_of,
            ))
        }
        "model.import.apply" => apply(document, &args, &oriented),
        "model.import.segmentPreview" => segment_preview(document, &args, &oriented),
        _ => segment_apply(document, &args, &oriented),
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

/// Every `step`th triangle, so a preview stays under [`PREVIEW_TRIANGLES`].
fn thinned<T: Copy>(items: &[T]) -> Vec<T> {
    let step = items.len().div_ceil(PREVIEW_TRIANGLES).max(1);
    items.iter().step_by(step).copied().collect()
}

/// A preview reply: `xyz` (nine floats per triangle, job millimetres),
/// `flags` (bit 0: undercut) and `segment` (index into the header's
/// segment list) per triangle.
fn blob(
    mut header: Value,
    triangles: &[Triangle],
    flags: &[u8],
    segment_of: &[u16],
) -> KernelReply {
    let tris = thinned(triangles);
    let flags = thinned(flags);
    let segs = thinned(segment_of);
    let count = tris.len();
    let xyz_bytes = count * 9 * 4;
    let seg_offset = xyz_bytes;
    let flag_offset = seg_offset + count * 2;
    header["count"] = json!(count);
    header["sections"] = json!([
        { "name": "xyz", "dtype": "f32", "offset": 0, "count": count * 9 },
        { "name": "segment", "dtype": "u16", "offset": seg_offset, "count": count },
        { "name": "flags", "dtype": "u8", "offset": flag_offset, "count": count },
    ]);
    let head = header.to_string().into_bytes();
    let mut out = Vec::with_capacity(4 + head.len() + flag_offset + count);
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for t in &tris {
        for v in t {
            for c in v {
                out.extend_from_slice(&(*c as f32).to_le_bytes());
            }
        }
    }
    for s in &segs {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out.extend_from_slice(&flags);
    KernelReply::Bytes(out)
}

fn file_name(path: &FsPath) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Model")
        .to_string()
}

fn next_session(document: &Document) -> u64 {
    document
        .all_components()
        .iter()
        .filter_map(|c| c.extra.import.as_ref().map(|s| s.session))
        .max()
        .unwrap_or(0)
        + 1
}

fn component(name: String, combine: Combine, field: Field, source: &ImportSource) -> Component {
    let mut c = Component {
        id: 0,
        name,
        visible: true,
        combine,
        height_scale: 1.0,
        level: 0,
        extra: Default::default(),
        field,
    };
    c.extra.import = Some(Box::new(source.clone()));
    c
}

/// Runs `action` as one undo step, answering its result with the revision.
fn edit<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(result) => {
            let revision = document.commit();
            let mut object = match result {
                Value::Object(map) => map,
                _ => Map::new(),
            };
            object.insert("revision".to_string(), json!(revision));
            KernelReply::ok(&Value::Object(object))
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

fn apply(
    document: &mut Document,
    args: &MeshArgs,
    oriented: &Oriented,
) -> Result<KernelReply, CoreError> {
    let job = *document.job_setup();
    let grid = grid_for(document.job());
    let placed = place(&oriented.triangles, &grid, &args.place, &job)?;
    let name = args
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file_name(&args.path));
    let source = ImportSource {
        path: args.path.clone(),
        // Kept where it landed, so a reimport after a job change stays put.
        orient: Orientation {
            centre: Some(oriented.centre),
            ..args.orient.clone()
        },
        place: Some(args.place),
        plan: None,
        session: next_session(document),
        combine: args.combine,
        rotary: None,
    };
    let replace = args.replace.clone();
    edit(document, "Import Component", |doc| {
        if !replace.is_empty() {
            doc.delete_components(&replace)?;
        }
        let mut ids = Vec::new();
        if let Some(top) = placed.top {
            ids.push(doc.add_component(component(name.clone(), args.combine, top, &source)));
        }
        let mut other = None;
        if let Some(bottom) = placed.bottom {
            let back = doc.active_side();
            doc.set_active_side(back.other())?;
            let id = doc.add_component(component(
                format!("{name} (back)"),
                args.combine,
                bottom,
                &source,
            ));
            doc.set_active_side(back)?;
            other = Some(id);
        }
        Ok(json!({ "ids": ids, "otherSide": other }))
    })
}

/// The rotary job's axis, radius and the middle of the blank along it.
fn rotary_job(document: &Document) -> Result<(crate::doc::RotaryAxis, f64, f64), CoreError> {
    let job = document.job();
    if job.kind != crate::doc::JobKind::Rotary {
        return Err(CoreError::validation(
            "a full 3D model is laid on the axis of a rotary job only",
        ));
    }
    let centre = document.job_bounds().centre();
    let along = match job.rotary.axis {
        crate::doc::RotaryAxis::X => centre.x,
        crate::doc::RotaryAxis::Y => centre.y,
    };
    Ok((job.rotary.axis, job.rotary.radius(), along))
}

/// Info, preview and apply for a full 3D model on a rotary job.
fn rotary_command(
    document: &mut Document,
    name: &str,
    args: &MeshArgs,
    triangles: &[Triangle],
    oriented: &Oriented,
    fit: &RotaryFit,
) -> Result<KernelReply, CoreError> {
    let (axis, _, along) = rotary_job(document)?;
    let (mesh, cylinder) = lay_on_axis(&oriented.triangles, axis, fit, along)?;
    let natural = lay_on_axis(&oriented.triangles, axis, &RotaryFit::default(), along)?.1;
    match name {
        "model.import.info" => KernelReply::ok(&json!({
            "triangles": triangles.len(),
            "natural": natural_size(triangles, &args.orient),
            "size": oriented.size,
            "centre": oriented.centre,
            "cylinder": cylinder,
            "naturalCylinder": natural,
        })),
        "model.import.preview" => {
            let flags = vec![0u8; mesh.len()];
            let segment_of = vec![0u16; mesh.len()];
            Ok(blob(
                json!({
                    "size": oriented.size,
                    "centre": oriented.centre,
                    "rotary": true,
                    "cylinder": cylinder,
                }),
                &mesh,
                &flags,
                &segment_of,
            ))
        }
        "model.import.apply" => rotary_apply(document, args, fit, cylinder, &oriented.triangles),
        _ => Err(CoreError::validation(
            "3D Segmenting is for flat jobs; import the model whole",
        )),
    }
}

fn rotary_apply(
    document: &mut Document,
    args: &MeshArgs,
    fit: &RotaryFit,
    cylinder: super::rotary::Cylinder,
    oriented: &[Triangle],
) -> Result<KernelReply, CoreError> {
    let name = args
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file_name(&args.path));
    let source = ImportSource {
        path: args.path.clone(),
        orient: args.orient.clone(),
        place: None,
        plan: None,
        session: next_session(document),
        combine: args.combine,
        rotary: Some(*fit),
    };
    let replace = args.replace.clone();
    let resize = args.resize_material;
    let combine = args.combine;
    edit(document, "Import Component", |doc| {
        if resize {
            let mut job = *doc.job();
            job.rotary.length = cylinder.length;
            job.rotary.diameter = cylinder.diameter;
            doc.set_job(job)?;
        }
        // Lay the model again on the (maybe resized) blank.
        let (axis, radius, along) = rotary_job(doc)?;
        let (mesh, _) = lay_on_axis(oriented, axis, fit, along)?;
        let grid = grid_for(doc.job());
        let field = radial_field(&mesh, &grid, axis, radius)
            .cropped()
            .ok_or_else(|| CoreError::validation("the model misses the blank"))?;
        if !replace.is_empty() {
            doc.delete_components(&replace)?;
        }
        let id = doc.add_component(component(name, combine, field, &source));
        Ok(json!({ "ids": [id], "otherSide": null }))
    })
}

fn segment_preview(
    document: &Document,
    args: &MeshArgs,
    oriented: &Oriented,
) -> Result<KernelReply, CoreError> {
    segment::check(&args.plan)?;
    let segs = segment::segments(&args.plan);
    let area = document.job_bounds();
    let (laid, extent) = segment::lay_out(&oriented.triangles, &segs, &args.plan, area);
    let mut triangles = Vec::new();
    let mut segment_of = Vec::new();
    let mut flags = Vec::new();
    let mut list = Vec::new();
    for (k, (seg, place)) in segs.iter().zip(&laid).enumerate() {
        let mut part = segment::segment_triangles(&oriented.triangles, seg);
        if args.layout {
            for t in &mut part {
                *t = t.map(|v| place.place(v));
            }
            flags.extend(undercuts(&part).into_iter().map(u8::from));
        } else {
            flags.extend(std::iter::repeat_n(0u8, part.len()));
        }
        segment_of.extend(std::iter::repeat_n(k as u16, part.len()));
        triangles.extend(part);
        list.push(json!({ "id": seg.id, "leaves": seg.leaf_ids, "size": place.size }));
    }
    Ok(blob(
        json!({
            "size": oriented.size,
            "centre": oriented.centre,
            "segments": list,
            "extent": [extent.min.x, extent.min.y, extent.max.x, extent.max.y],
        }),
        &triangles,
        &flags,
        &segment_of,
    ))
}

fn segment_apply(
    document: &mut Document,
    args: &MeshArgs,
    oriented: &Oriented,
) -> Result<KernelReply, CoreError> {
    segment::check(&args.plan)?;
    let segs = segment::segments(&args.plan);
    let plan = &args.plan;
    let base_name = args
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file_name(&args.path));
    let source = ImportSource {
        path: args.path.clone(),
        // Kept where it landed, so a reimport after a job change stays put.
        orient: Orientation {
            centre: Some(oriented.centre),
            ..args.orient.clone()
        },
        place: None,
        plan: Some(plan.clone()),
        session: next_session(document),
        combine: args.combine,
        rotary: None,
    };
    let replace = args.replace.clone();
    let tol = Tolerance::new(0.01)?;
    edit(document, "Import Segments", |doc| {
        if !replace.is_empty() {
            doc.delete_components(&replace)?;
        }
        let (mut laid, extent) =
            segment::lay_out(&oriented.triangles, &segs, plan, doc.job_bounds());
        if plan.resize_material {
            let mut job = *doc.job_setup();
            let margin = if plan.gap.is_finite() && plan.gap > 0.0 {
                plan.gap
            } else {
                0.0
            };
            job.width = extent.width() + 2.0 * margin;
            job.height = extent.height() + 2.0 * margin;
            let tallest = laid.iter().map(|l| l.size[2]).fold(0.0, f64::max);
            job.thickness = job.thickness.max(tallest);
            doc.set_job(job)?;
            laid = segment::lay_out(&oriented.triangles, &segs, plan, doc.job_bounds()).0;
        }
        let grid = grid_for(doc.job());
        let (layer, sheet) = (doc.active_layer(), doc.active_sheet());
        let mut ids = Vec::new();
        let mut vectors = Vec::new();
        for (k, (seg, place)) in segs.iter().zip(&laid).enumerate() {
            let projected = segment::project(&oriented.triangles, seg, place, &grid, plan.caps);
            let Some(top) = projected.top else {
                continue;
            };
            if plan.boundary_vectors {
                let mut outlines = contour::boundary(&top, -1.0e30, 0.0, tol)?;
                if let Some(base) = &projected.base {
                    outlines.extend(contour::boundary(base, 0.5, 0.0, tol)?);
                }
                for path in outlines {
                    vectors.push(doc.insert_entity(Entity {
                        rotation: 0.0,
                        id: 0,
                        layer,
                        sheet,
                        revision: 0,
                        name: None,
                        parent: None,
                        kind: EntityKind::Path { path },
                    }));
                }
            }
            let name = format!("{base_name} {}", k + 1);
            ids.push(doc.add_component(component(name, args.combine, top, &source)));
        }
        if ids.is_empty() {
            return Err(CoreError::validation(
                "the segments land outside the material",
            ));
        }
        Ok(json!({ "ids": ids, "vectors": vectors }))
    })
}

fn settings(document: &Document, id: u64) -> Result<KernelReply, CoreError> {
    let source = document.component(id)?.extra.import.clone();
    let ids: Vec<u64> = match &source {
        Some(s) => document
            .all_components()
            .iter()
            .filter(|c| {
                c.extra
                    .import
                    .as_ref()
                    .is_some_and(|o| o.session == s.session)
            })
            .map(|c| c.id)
            .collect(),
        None => Vec::new(),
    };
    KernelReply::ok(&json!({ "source": source, "ids": ids }))
}

#[cfg(test)]
mod tests {
    use super::super::raster::tests::cube;
    use super::super::segment::{CutPlane, SegmentNode};
    use super::*;
    use crate::doc::{JobKind, Side};

    fn stl(dir: &FsPath, name: &str, triangles: &[Triangle]) -> PathBuf {
        let mut text = String::from("solid t\n");
        for t in triangles {
            text.push_str("facet normal 0 0 0\nouter loop\n");
            for v in t {
                text.push_str(&format!("vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            text.push_str("endloop\nendfacet\n");
        }
        text.push_str("endsolid t\n");
        let path = dir.join(name);
        std::fs::write(&path, text).expect("writes");
        path
    }

    fn run(doc: &mut Document, cache: &mut MeshCache, name: &str, args: Value) -> Value {
        match dispatch(doc, cache, name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                let v: Value = serde_json::from_str(&text).expect("json");
                v["result"].clone()
            }
            KernelReply::Bytes(bytes) => {
                let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                serde_json::from_slice(&bytes[4..4 + len]).expect("header")
            }
        }
    }

    #[test]
    fn import_orient_place_and_reimport() {
        let temp = crate::test_support::TempDir::new("import-flow");
        let path = stl(
            temp.path(),
            "box.stl",
            &cube([0.0, 0.0, 0.0], [10.0, 20.0, 5.0]),
        );
        let mut doc = Document::new();
        let mut cache = MeshCache::default();
        let info = run(
            &mut doc,
            &mut cache,
            "model.import.info",
            json!({ "path": path, "orient": { "top": "front" } }),
        );
        assert_eq!(info["triangles"], 12);
        assert_eq!(info["natural"], json!([10.0, 5.0, 20.0]));
        let preview = run(
            &mut doc,
            &mut cache,
            "model.import.preview",
            json!({ "path": path }),
        );
        assert_eq!(preview["count"], 12);
        let reply = run(
            &mut doc,
            &mut cache,
            "model.import.apply",
            json!({ "path": path, "orient": { "size": [40.0, 80.0, 20.0] }, "place": { "zeroPlane": 5.0 } }),
        );
        let id = reply["ids"][0].as_u64().expect("an id");
        let c = doc.component(id).expect("made");
        assert_eq!(c.name, "box");
        assert_eq!(c.field.max(), Some(15.0));
        assert!((c.field.width as f64 * c.field.cell - 40.0).abs() <= 2.0 * c.field.cell);
        let settings = run(
            &mut doc,
            &mut cache,
            "model.import.settings",
            json!({ "id": id }),
        );
        assert_eq!(settings["source"]["place"]["zeroPlane"], 5.0);
        assert!(settings["source"]["orient"]["centre"]["x"].is_f64());
        assert_eq!(settings["ids"], json!([id]));
        // Reimporting replaces the component in one step.
        let again = run(
            &mut doc,
            &mut cache,
            "model.import.apply",
            json!({ "path": path, "replace": [id] }),
        );
        assert!(doc.component(id).is_err());
        let new_id = again["ids"][0].as_u64().expect("an id");
        assert_eq!(doc.component(new_id).expect("made").field.max(), Some(5.0));
        doc.undo();
        assert!(doc.component(id).is_ok());
    }

    #[test]
    fn both_sides_puts_the_underside_on_the_other_side() {
        let temp = crate::test_support::TempDir::new("import-sides");
        let path = stl(
            temp.path(),
            "box.stl",
            &cube([0.0, 0.0, 0.0], [10.0, 10.0, 6.0]),
        );
        let mut doc = Document::new();
        let mut job = *doc.job_setup();
        job.kind = JobKind::DoubleSided;
        doc.begin("job");
        doc.set_job(job).expect("job");
        doc.commit();
        let mut cache = MeshCache::default();
        let reply = run(
            &mut doc,
            &mut cache,
            "model.import.apply",
            json!({ "path": path, "place": { "zeroPlane": 2.0, "bothSides": true } }),
        );
        assert!(reply["otherSide"].is_u64());
        assert_eq!(doc.components().len(), 1);
        assert_eq!(doc.active_side(), Side::Top);
        doc.set_active_side(Side::Bottom).expect("bottom");
        assert_eq!(doc.components().len(), 1);
        assert_eq!(doc.components()[0].field.max(), Some(2.0));
    }

    #[test]
    fn segments_become_components_with_outlines() {
        let temp = crate::test_support::TempDir::new("import-segments");
        let path = stl(
            temp.path(),
            "tall.stl",
            &cube([0.0, 0.0, 0.0], [20.0, 20.0, 30.0]),
        );
        let mut doc = Document::new();
        let mut cache = MeshCache::default();
        let plan = SegmentPlan {
            root: SegmentNode {
                cut: Some(CutPlane {
                    point: [0.0, 0.0, 10.0],
                    ..CutPlane::default()
                }),
                children: vec![SegmentNode::default(), SegmentNode::default()],
            },
            boundary_vectors: true,
            resize_material: true,
            ..SegmentPlan::default()
        };
        let plan = serde_json::to_value(&plan).expect("plan");
        let preview = run(
            &mut doc,
            &mut cache,
            "model.import.segmentPreview",
            json!({ "path": path, "plan": plan, "layout": true }),
        );
        assert_eq!(preview["segments"].as_array().map(Vec::len), Some(2));
        let reply = run(
            &mut doc,
            &mut cache,
            "model.import.segmentApply",
            json!({ "path": path, "plan": plan }),
        );
        let ids: Vec<u64> = serde_json::from_value(reply["ids"].clone()).expect("ids");
        assert_eq!(ids.len(), 2);
        let heights: Vec<f32> = ids
            .iter()
            .map(|id| doc.component(*id).expect("made").field.max().unwrap_or(0.0))
            .collect();
        assert_eq!(heights, [20.0, 10.0]);
        // Two outlines per segment: the top and where it rests.
        assert_eq!(reply["vectors"].as_array().map(Vec::len), Some(4));
        // The material shrank to the two 20 wide segments and the gaps.
        assert!((doc.job_setup().width - 55.0).abs() < 1e-9);
        assert!(doc.job_setup().thickness >= 20.0);
        let settings = run(
            &mut doc,
            &mut cache,
            "model.import.settings",
            json!({ "id": ids[1] }),
        );
        assert_eq!(settings["ids"], json!(ids));
        assert!(settings["source"]["plan"]["root"]["cut"].is_object());
    }

    #[test]
    fn a_full_3d_model_on_a_rotary_job_lies_on_the_axis() {
        let temp = crate::test_support::TempDir::new("import-rotary");
        // A 60 long bar, 8 by 8 across.
        let path = stl(
            temp.path(),
            "bar.stl",
            &cube([0.0, 0.0, 0.0], [60.0, 8.0, 8.0]),
        );
        let mut doc = Document::new();
        let mut job = *doc.job_setup();
        job.kind = JobKind::Rotary;
        job.rotary.diameter = 40.0;
        job.rotary.length = 200.0;
        job.rotary.axis = crate::doc::RotaryAxis::X;
        doc.begin("job");
        doc.set_job(job).expect("job");
        doc.commit();
        let mut cache = MeshCache::default();
        let info = run(
            &mut doc,
            &mut cache,
            "model.import.info",
            json!({ "path": path, "rotary": {} }),
        );
        assert!((info["cylinder"]["length"].as_f64().unwrap_or(0.0) - 60.0).abs() < 1e-9);
        let diameter = info["cylinder"]["diameter"].as_f64().unwrap_or(0.0);
        assert!((diameter - 2.0 * 32f64.sqrt()).abs() < 1e-9, "{diameter}");
        let preview = run(
            &mut doc,
            &mut cache,
            "model.import.preview",
            json!({ "path": path, "rotary": { "radialScale": 2.0 } }),
        );
        assert_eq!(preview["rotary"], Value::Bool(true));
        assert!(
            (preview["cylinder"]["diameter"].as_f64().unwrap_or(0.0) - 2.0 * diameter).abs() < 1e-6
        );
        let reply = run(
            &mut doc,
            &mut cache,
            "model.import.apply",
            json!({ "path": path, "rotary": {}, "resizeMaterial": true }),
        );
        assert!(
            (doc.job().rotary.length - 60.0).abs() < 1e-9,
            "blank fitted"
        );
        let id = reply["ids"][0].as_u64().expect("an id");
        let c = doc.component(id).expect("made");
        let top = f64::from(c.field.max().unwrap_or(0.0));
        assert!((top - 32f64.sqrt()).abs() < 0.2, "the bar's edges {top}");
        let low = f64::from(c.field.min().unwrap_or(0.0));
        assert!((low - 4.0).abs() < 0.1, "its faces {low}");
        let source = c.extra.import.as_ref().and_then(|s| s.rotary);
        assert!(source.is_some());
        doc.undo();
        assert!(
            (doc.job().rotary.length - 200.0).abs() < 1e-9,
            "one undo step"
        );
        let mut flat = Document::new();
        assert!(dispatch(
            &mut flat,
            &mut cache,
            "model.import.apply",
            &json!({ "path": path, "rotary": {} }).to_string()
        )
        .is_err());
    }
}
