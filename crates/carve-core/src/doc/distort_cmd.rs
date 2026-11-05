//! Distort Object: `ops.distort` bends vectors and relief components
//! through one distortion envelope, as one undo step; `ops.distortion`
//! answers the envelope a selection was bent with (it stays editable until
//! baked or edited another way) and `ops.bakeDistortion` bakes it.
//!
//! The envelope is a box whose nodes were dragged, or it is taken from the
//! last selected open vector (Along a Single Curve) or the last two (Between
//! Two Curves). Bending again an object whose distortion is still live
//! starts from its geometry before the first bend, so the envelope is
//! edited rather than stacked.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::entity::{merge, EntityKind};
use crate::doc::{DistortMode, Distortion, Document};
use crate::error::CoreError;
use crate::geom::distort::{
    distort_path, single_curve_edges, two_curve_edges, EdgeLines, Envelope,
};
use crate::geom::{path_bbox, transform_path, BBox, Mat3, Path, Point, Tolerance};
use crate::kernel::{edit, KernelReply};
use crate::model::distort::distort_field;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "ops.distort" | "ops.distortion" | "ops.bakeDistortion"
    )
}

/// The envelope's edges on the wire.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeArgs {
    /// Bottom edge nodes, left to right, in millimetres.
    bottom: Vec<Point>,
    /// Top edge nodes, left to right, in millimetres.
    top: Vec<Point>,
    /// Left side nodes, bottom to top; straight when absent.
    #[serde(default)]
    left: Option<Vec<Point>>,
    /// Right side nodes, bottom to top; straight when absent.
    #[serde(default)]
    right: Option<Vec<Point>>,
    /// Which spans are straight lines; every span a curve when absent.
    #[serde(default)]
    lines: EdgeLines,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DistortArgs {
    /// Vectors to bend; the selection (less the curves) when absent.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Relief components to bend.
    #[serde(default)]
    component_ids: Vec<u64>,
    /// Box, Curve or TwoCurves.
    #[serde(default)]
    mode: DistortMode,
    /// The curve (Curve) or the two curves (TwoCurves) the envelope follows.
    #[serde(default)]
    curves: Vec<u64>,
    /// The dragged box envelope; needed in Box mode.
    #[serde(default)]
    envelope: Option<EnvelopeArgs>,
    /// The original box `[minX, minY, maxX, maxY]` in the frame; the box
    /// round the vectors and components when absent.
    #[serde(default)]
    bbox: Option<[f64; 4]>,
    /// Use Rotated Bounds: the frame angle in degrees. The box and the
    /// envelope are in the frame turned by it about the job origin.
    #[serde(default)]
    angle: f64,
    /// Fit tolerance for the rebuilt vectors, in millimetres.
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdsArgs {
    /// The objects; the selection when absent.
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

/// Runs a distort command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] for unknown ids, a bad envelope or nothing that
/// can be bent.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "ops.distort" => {
            let args: DistortArgs =
                serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
            distort(document, args)
        }
        "ops.distortion" => {
            let args: IdsArgs =
                serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
            let ids = sorted(args.ids.unwrap_or_else(|| document.selection().to_vec()));
            let found = live_distortion(document, &ids);
            KernelReply::ok(&json!({ "distortion": found }))
        }
        "ops.bakeDistortion" => {
            let args: IdsArgs =
                serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
            let ids = args.ids.unwrap_or_else(|| document.selection().to_vec());
            let kept: Vec<Distortion> = document
                .distortions()
                .iter()
                .filter(|d| !d.ids.iter().any(|id| ids.contains(id)))
                .cloned()
                .collect();
            let baked = document.distortions().len() - kept.len();
            if baked == 0 {
                return Err(CoreError::validation(
                    "the selection has no distortion to bake",
                ));
            }
            edit(document, "Bake Distortion", |doc| {
                doc.set_distortions(kept);
                Ok(json!({ "baked": baked }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

fn sorted(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// True while no vector of `d` has been changed since it was bent.
fn is_live(document: &Document, d: &Distortion) -> bool {
    d.revisions.iter().all(|(id, revision)| {
        document
            .entity(*id)
            .is_ok_and(|entity| entity.revision == *revision)
    })
}

/// The live distortion of exactly `ids`, if any.
fn live_distortion<'a>(document: &'a Document, ids: &[u64]) -> Option<&'a Distortion> {
    document
        .distortions()
        .iter()
        .find(|d| d.ids == ids && is_live(document, d))
}

/// The one open vector `id` is, for a curve mode.
fn curve_path(document: &Document, id: u64) -> Result<Path, CoreError> {
    document
        .entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation("the distortion curve must be an open vector"))
}

fn distort(document: &mut Document, args: DistortArgs) -> Result<KernelReply, CoreError> {
    let wanted_curves = match args.mode {
        DistortMode::Box => 0,
        DistortMode::Curve => 1,
        DistortMode::TwoCurves => 2,
    };
    if args.curves.len() != wanted_curves {
        return Err(CoreError::validation(match args.mode {
            DistortMode::Curve => "select the objects, then the open vector to bend them along",
            DistortMode::TwoCurves => {
                "select the objects, then the two open vectors to bend them between"
            }
            DistortMode::Box => "a bounding box distortion takes no curves",
        }));
    }
    let ids: Vec<u64> = args
        .ids
        .clone()
        .unwrap_or_else(|| document.selection().to_vec())
        .into_iter()
        .filter(|id| !args.curves.contains(id))
        .collect();
    let ids = sorted(ids);
    for id in ids.iter().chain(&args.curves) {
        document.entity(*id)?;
    }
    //: vectors or components, never both in one go, and
    // one component at a time (several are baked into one first).
    if !ids.is_empty() && !args.component_ids.is_empty() {
        return Err(CoreError::validation(
            "distort vectors or a component, not both at once: select only one kind",
        ));
    }
    if args.component_ids.len() > 1 {
        return Err(CoreError::validation(
            "distort one component at a time: bake the selected components into one first",
        ));
    }
    let to_frame = Mat3::rotate(-args.angle.to_radians());
    let from_frame = Mat3::rotate(args.angle.to_radians());

    // The geometry to bend: what a live distortion of the same objects
    // started from, else the vectors as they are, in the frame.
    let previous = live_distortion(document, &ids).cloned();
    let mut sources: Vec<(u64, Path)> = Vec::new();
    for id in &ids {
        for leaf in document.leaves(*id) {
            if sources.iter().any(|(done, _)| *done == leaf) {
                continue;
            }
            let entity = document.entity(leaf)?;
            let current = match &entity.kind {
                EntityKind::Path { path } => path.clone(),
                EntityKind::Text(text) => merge(&text.paths),
                _ => continue,
            };
            let source = previous
                .as_ref()
                .and_then(|d| d.sources.get(&leaf).cloned())
                .unwrap_or(current);
            sources.push((leaf, source));
        }
    }
    let framed: Vec<Path> = sources
        .iter()
        .map(|(_, path)| transform_path(path, &to_frame))
        .collect();

    let mut component_box: Option<BBox> = None;
    for id in &args.component_ids {
        let field = &document.component(*id)?.field;
        let half = field.cell / 2.0;
        let b = BBox::new(
            Point::new(field.origin.x - half, field.origin.y - half),
            Point::new(
                field.origin.x + (field.width as f64 - 0.5) * field.cell,
                field.origin.y + (field.height as f64 - 0.5) * field.cell,
            ),
        );
        component_box = Some(component_box.map_or(b, |c| c.union(b)));
    }
    let vector_box = framed
        .iter()
        .filter_map(path_bbox)
        .reduce(|a, b| a.union(b));
    let bbox = match (args.bbox, previous.as_ref()) {
        (Some([x0, y0, x1, y1]), _) => BBox::new(Point::new(x0, y0), Point::new(x1, y1)),
        (None, Some(d)) => BBox::new(
            Point::new(d.bbox[0], d.bbox[1]),
            Point::new(d.bbox[2], d.bbox[3]),
        ),
        (None, None) => match (vector_box, component_box) {
            (Some(a), Some(b)) => a.union(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => {
                return Err(CoreError::validation(
                    "select the vectors or components to distort",
                ))
            }
        },
    };
    let tol = Tolerance::new(
        args.tolerance
            .unwrap_or_else(|| (bbox.width().hypot(bbox.height()) * 1e-4).clamp(0.001, 0.02)),
    )?;
    let (bottom, top, left, right, lines) = match args.mode {
        DistortMode::Box => {
            let edges = args.envelope.ok_or_else(|| {
                CoreError::validation("drag the envelope's nodes to distort the selection")
            })?;
            (
                edges.bottom,
                edges.top,
                edges.left,
                edges.right,
                edges.lines,
            )
        }
        DistortMode::Curve => {
            let curve = transform_path(&curve_path(document, args.curves[0])?, &to_frame);
            let (bottom, top) = single_curve_edges(&curve, bbox.height(), tol)?;
            (bottom, top, None, None, EdgeLines::default())
        }
        DistortMode::TwoCurves => {
            let a = transform_path(&curve_path(document, args.curves[0])?, &to_frame);
            let b = transform_path(&curve_path(document, args.curves[1])?, &to_frame);
            let (bottom, top) = two_curve_edges(&a, &b, tol)?;
            (bottom, top, None, None, EdgeLines::default())
        }
    };
    let envelope = Envelope::new(
        bbox,
        bottom.clone(),
        top.clone(),
        left.clone(),
        right.clone(),
    )?
    .with_lines(&lines)?;

    // Work everything out first, so a failure leaves the document alone.
    let mut vectors: Vec<(u64, Path)> = Vec::with_capacity(sources.len());
    for ((leaf, _), path) in sources.iter().zip(&framed) {
        let bent = distort_path(path, &envelope, tol)?;
        vectors.push((*leaf, transform_path(&bent, &from_frame)));
    }
    let mut fields = Vec::new();
    for id in &args.component_ids {
        let bent = distort_field(&document.component(*id)?.field, &envelope)?;
        let field = bent
            .cropped()
            .ok_or_else(|| CoreError::validation("the distorted component would be empty"))?;
        fields.push((*id, field));
    }
    if vectors.is_empty() && fields.is_empty() {
        return Err(CoreError::validation(
            "nothing selected can be distorted; select vectors, text or components",
        ));
    }
    let kept = (!vectors.is_empty()).then(|| Distortion {
        ids: ids.clone(),
        mode: args.mode,
        curves: args.curves.clone(),
        angle: args.angle,
        bbox: [bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y],
        bottom,
        top,
        left,
        right,
        lines,
        sources: sources.into_iter().collect::<BTreeMap<u64, Path>>(),
        revisions: BTreeMap::new(),
    });

    document.begin("Distort");
    let result = apply(document, vectors, fields, kept);
    match result {
        Ok(value) => {
            let revision = document.commit();
            let mut value = value;
            value["revision"] = json!(revision);
            KernelReply::ok(&value)
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

fn apply(
    document: &mut Document,
    vectors: Vec<(u64, Path)>,
    fields: Vec<(u64, crate::model::field::Field)>,
    kept: Option<Distortion>,
) -> Result<Value, CoreError> {
    let mut changed = Vec::with_capacity(vectors.len());
    for (id, path) in vectors {
        let mut entity = document.entity(id)?.clone();
        // Text becomes plain vectors once bent, as its glyphs no longer
        // follow the font.
        entity.kind = EntityKind::Path { path };
        document.replace_entity(id, entity)?;
        changed.push(id);
    }
    let mut components = Vec::with_capacity(fields.len());
    for (id, field) in fields {
        document.update_component(id, |c| c.field = field)?;
        components.push(id);
    }
    if let Some(mut kept) = kept {
        for id in &changed {
            kept.revisions.insert(*id, document.entity(*id)?.revision);
        }
        // The new envelope takes over from any distortion of these objects.
        let mut list: Vec<Distortion> = document
            .distortions()
            .iter()
            .filter(|d| !d.ids.iter().any(|id| kept.ids.contains(id)) && is_live(document, d))
            .cloned()
            .collect();
        list.push(kept);
        document.set_distortions(list);
    }
    Ok(json!({ "ids": changed, "componentIds": components }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::entity::Entity;
    use crate::geom::{path_bbox, SubPath};
    use crate::Kernel;

    fn square(doc: &mut Document) -> u64 {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(100.0, 0.0))
            .line_to(Point::new(100.0, 50.0))
            .line_to(Point::new(0.0, 50.0));
        sub.closed = true;
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: Path::from_subpath(sub),
            },
        })
    }

    fn arch(lift: f64) -> Value {
        json!({
            "bottom": [{ "x": 0.0, "y": 0.0 }, { "x": 50.0, "y": 0.0 }, { "x": 100.0, "y": 0.0 }],
            "top": [{ "x": 0.0, "y": 50.0 }, { "x": 50.0, "y": 50.0 + lift }, { "x": 100.0, "y": 50.0 }],
        })
    }

    fn result(reply: KernelReply) -> Value {
        match reply {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("json");
                value["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected json"),
        }
    }

    #[test]
    fn distort_bends_a_vector_in_one_undo_step() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        let args = json!({ "ids": [id], "envelope": arch(20.0) }).to_string();
        let reply = result(dispatch(&mut doc, "ops.distort", &args).expect("distort"));
        assert_eq!(reply["ids"], json!([id]));
        let path = doc.entity(id).expect("entity").path().expect("path");
        let b = path_bbox(&path).expect("bbox");
        assert!((b.max.y - 70.0).abs() < 0.05, "top at {}", b.max.y);
        assert!(b.min.y.abs() < 1e-6);
        assert_eq!(doc.undo().as_deref(), Some("Distort"));
        let back = path_bbox(&doc.entity(id).expect("entity").path().expect("path")).expect("bbox");
        assert!((back.max.y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn distort_uses_the_selection_without_ids() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        doc.set_selection(&[id]);
        let args = json!({ "envelope": arch(10.0) }).to_string();
        dispatch(&mut doc, "ops.distort", &args).expect("distort");
        let b = path_bbox(&doc.entity(id).expect("entity").path().expect("path")).expect("bbox");
        assert!((b.max.y - 60.0).abs() < 0.05);
    }

    #[test]
    fn distort_refuses_nothing_and_bad_envelopes() {
        let mut doc = Document::new();
        let args = json!({ "ids": [], "envelope": arch(10.0) }).to_string();
        assert!(dispatch(&mut doc, "ops.distort", &args).is_err());
        let id = square(&mut doc);
        let bad = json!({
            "ids": [id],
            "envelope": { "bottom": [{ "x": 0.0, "y": 0.0 }], "top": [] },
        })
        .to_string();
        assert!(dispatch(&mut doc, "ops.distort", &bad).is_err());
        assert!(dispatch(&mut doc, "ops.nope", "{}").is_err());
        assert!(!handles("ops.move"));
    }

    #[test]
    fn distort_runs_through_the_kernel_and_bends_components() {
        let mut kernel = Kernel::new();
        let id = square(kernel.document_mut());
        let reply = kernel.call("model.createShape", &json!({ "ids": [id] }).to_string());
        let component = result(reply)["id"].as_u64().expect("component id");
        let before = kernel
            .document()
            .component(component)
            .expect("component")
            .field
            .clone();
        let mixed = json!({
            "ids": [id],
            "componentIds": [component],
            "envelope": arch(20.0),
        })
        .to_string();
        let error = kernel.apply("ops.distort", &mixed).expect_err("mixed");
        assert!(error.to_string().contains("not both"), "{error}");
        let two =
            json!({ "ids": [], "componentIds": [component, component], "envelope": arch(20.0) });
        let error = kernel
            .apply("ops.distort", &two.to_string())
            .expect_err("two");
        assert!(error.to_string().contains("one component"), "{error}");
        let args = json!({
            "ids": [],
            "componentIds": [component],
            "envelope": arch(20.0),
            "bbox": [0.0, 0.0, 100.0, 50.0],
        })
        .to_string();
        let reply = result(kernel.apply("ops.distort", &args).expect("distort"));
        assert_eq!(reply["componentIds"], json!([component]));
        let after = &kernel
            .document()
            .component(component)
            .expect("component")
            .field;
        let top = |f: &crate::model::field::Field| f.origin.y + (f.height as f64 - 0.5) * f.cell;
        assert!(top(after) > top(&before) + 15.0);
    }

    fn line(doc: &mut Document, a: (f64, f64), b: (f64, f64)) -> u64 {
        let mut sub = SubPath::open(Point::new(a.0, a.1));
        sub.line_to(Point::new(b.0, b.1));
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: Path::from_subpath(sub),
            },
        })
    }

    fn bbox_of(doc: &Document, id: u64) -> BBox {
        path_bbox(&doc.entity(id).expect("entity").path().expect("path")).expect("bbox")
    }

    #[test]
    fn distort_along_one_curve_and_between_two() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        let curve = line(&mut doc, (200.0, 100.0), (0.0, 100.0));
        doc.set_selection(&[id, curve]);
        let args = json!({ "mode": "curve", "curves": [curve] }).to_string();
        dispatch(&mut doc, "ops.distort", &args).expect("along a curve");
        let b = bbox_of(&doc, id);
        assert!(
            b.min.x.abs() < 0.05 && (b.max.x - 200.0).abs() < 0.05,
            "{b:?}"
        );
        assert!(
            (b.min.y - 100.0).abs() < 0.05 && (b.max.y - 150.0).abs() < 0.05,
            "{b:?}"
        );
        // The curve itself is left alone.
        assert!((bbox_of(&doc, curve).max.x - 200.0).abs() < 1e-9);

        let low = line(&mut doc, (0.0, -40.0), (100.0, -40.0));
        let high = line(&mut doc, (0.0, 200.0), (100.0, 200.0));
        let args = json!({ "ids": [id], "mode": "twoCurves", "curves": [high, low] }).to_string();
        dispatch(&mut doc, "ops.distort", &args).expect("between two curves");
        let b = bbox_of(&doc, id);
        assert!(
            (b.min.y + 40.0).abs() < 0.05 && (b.max.y - 200.0).abs() < 0.05,
            "{b:?}"
        );
        assert!(
            b.min.x.abs() < 0.05 && (b.max.x - 100.0).abs() < 0.05,
            "{b:?}"
        );

        let missing = json!({ "ids": [id], "mode": "curve", "curves": [] }).to_string();
        assert!(dispatch(&mut doc, "ops.distort", &missing).is_err());
    }

    #[test]
    fn line_spans_bend_straight_and_are_kept_with_the_distortion() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        let mut envelope = arch(20.0);
        envelope["lines"] = json!({ "top": [true, true] });
        let args = json!({ "ids": [id], "envelope": envelope, "bbox": [0.0, 0.0, 100.0, 50.0] });
        dispatch(&mut doc, "ops.distort", &args.to_string()).expect("distort");
        // The top of the square runs up the straight spans to the peak:
        // every point of it on the left half lies on the line from (0, 50)
        // to (50, 70), where a curve through the nodes would bulge above.
        let path = doc.entity(id).expect("entity").path().expect("path");
        let top: Vec<Point> = crate::geom::flatten_path(&path, Tolerance::new(0.01).expect("tol"))
            .iter()
            .flat_map(|poly| poly.points.clone())
            .filter(|p| p.y > 45.0 && p.x <= 50.0)
            .collect();
        assert!(top.len() >= 2, "{top:?}");
        for p in &top {
            assert!((p.y - (50.0 + 0.4 * p.x)).abs() < 0.05, "{p:?}");
        }
        assert!((bbox_of(&doc, id).max.y - 70.0).abs() < 0.05);
        let found = result(
            dispatch(
                &mut doc,
                "ops.distortion",
                &json!({ "ids": [id] }).to_string(),
            )
            .expect("distortion"),
        );
        assert_eq!(found["distortion"]["lines"]["top"], json!([true, true]));
        let bad = json!({ "ids": [id], "envelope": { "bottom": arch(0.0)["bottom"], "top": arch(0.0)["top"],
                         "lines": { "top": [true] } } });
        assert!(dispatch(&mut doc, "ops.distort", &bad.to_string()).is_err());
    }

    #[test]
    fn a_live_distortion_is_edited_again_until_baked() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        let bend = |doc: &mut Document, lift: f64| {
            let args =
                json!({ "ids": [id], "envelope": arch(lift), "bbox": [0.0, 0.0, 100.0, 50.0] });
            dispatch(doc, "ops.distort", &args.to_string()).expect("distort");
        };
        bend(&mut doc, 20.0);
        bend(&mut doc, 10.0);
        // Edited, not stacked: the second envelope bends the square as drawn.
        assert!((bbox_of(&doc, id).max.y - 60.0).abs() < 0.05);
        let found = result(
            dispatch(
                &mut doc,
                "ops.distortion",
                &json!({ "ids": [id] }).to_string(),
            )
            .expect("distortion"),
        );
        assert_eq!(found["distortion"]["mode"], "box");
        assert!((found["distortion"]["top"][1]["y"].as_f64().expect("y") - 60.0).abs() < 1e-9);

        dispatch(
            &mut doc,
            "ops.bakeDistortion",
            &json!({ "ids": [id] }).to_string(),
        )
        .expect("bake");
        assert_eq!(doc.undo().as_deref(), Some("Bake Distortion"));
        assert_eq!(doc.distortions().len(), 1, "undo brings the envelope back");
        doc.redo();
        assert!(doc.distortions().is_empty());
        // Baked: a new bend starts from the bent shape.
        let args = json!({ "ids": [id], "envelope": arch(10.0), "bbox": [0.0, 0.0, 100.0, 50.0] });
        dispatch(&mut doc, "ops.distort", &args.to_string()).expect("distort");
        assert!(bbox_of(&doc, id).max.y > 60.5);
    }

    #[test]
    fn an_edit_elsewhere_bakes_the_distortion() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        let args = json!({ "ids": [id], "envelope": arch(20.0) });
        dispatch(&mut doc, "ops.distort", &args.to_string()).expect("distort");
        doc.begin("Move");
        doc.touch(id);
        doc.capture(id);
        doc.commit();
        let found = result(
            dispatch(
                &mut doc,
                "ops.distortion",
                &json!({ "ids": [id] }).to_string(),
            )
            .expect("distortion"),
        );
        assert!(found["distortion"].is_null());
    }

    #[test]
    fn rotated_bounds_bend_in_the_objects_frame() {
        let mut doc = Document::new();
        let id = square(&mut doc);
        // The square turned 90 degrees about the origin; in its frame it is
        // the plain 100 by 50 box again.
        let turn = crate::geom::Mat3::rotate(90f64.to_radians());
        let turned = transform_path(&doc.entity(id).expect("e").path().expect("p"), &turn);
        let mut entity = doc.entity(id).expect("e").clone();
        entity.kind = EntityKind::Path { path: turned };
        doc.replace_entity(id, entity).expect("replace");
        let args = json!({ "ids": [id], "envelope": arch(20.0), "angle": 90.0,
            "bbox": [0.0, 0.0, 100.0, 50.0] });
        dispatch(&mut doc, "ops.distort", &args.to_string()).expect("distort");
        let b = bbox_of(&doc, id);
        // The arch lifts the frame's top, which after the turn is -X.
        assert!((b.min.x + 70.0).abs() < 0.05, "{b:?}");
        assert!(
            b.min.y.abs() < 0.05 && (b.max.y - 100.0).abs() < 0.05,
            "{b:?}"
        );
    }
}
