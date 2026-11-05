//! Drawing tab edit tools: Select Duplicates, drag snipping (interactive
//! trim), freehand sketching and the Measure tool's vector pick.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::entity::{Entity, EntityKind};
use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::duplicates::duplicate_indices;
use crate::geom::freehand::freehand_subpath;
use crate::geom::{
    nearest_point_on_path, path_bbox, path_intersections, path_is_closed, path_length,
    segment_length, trim_to_intersection, ArcGeom, Path, Point, Segment, SubPath, Tolerance,
};
use crate::kernel::KernelReply;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "ops.selectDuplicates" | "ops.snip" | "ops.freehand" | "measure.at"
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DuplicatesArgs {
    /// Entities to search; every pickable entity when absent.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// How far apart two shapes may be and still match, in millimetres.
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnipArgs {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FreehandArgs {
    /// The pointer trail in job millimetres.
    points: Vec<Point>,
    /// Averaging passes before the fit.
    #[serde(default)]
    smoothing: usize,
    /// Fit tolerance in millimetres.
    #[serde(default)]
    tolerance: Option<f64>,
    /// Join the end back to the start.
    #[serde(default)]
    closed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickArgs {
    x: f64,
    y: f64,
    /// Pick radius in millimetres.
    tolerance: f64,
}

/// Runs a drawing tool command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse,
/// [`CoreError::Validation`] for bad values and [`CoreError::Geometry`] when
/// a snip crosses nothing or a sketch has no length.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    let trimmed = args.trim();
    let value: Value = if trimmed.is_empty() {
        json!({})
    } else {
        serde_json::from_str(trimmed).map_err(|error| CoreError::invalid_args(name, error))?
    };
    match name {
        "ops.selectDuplicates" => {
            let args: DuplicatesArgs = from_value(name, value)?;
            let ids = select_duplicates(document, args.ids.as_deref(), args.tolerance)?;
            document.bump();
            KernelReply::ok(&json!({ "ids": ids, "revision": document.revision() }))
        }
        "ops.snip" => {
            let args: SnipArgs = from_value(name, value)?;
            let from = Point::new(args.x0, args.y0);
            let to = Point::new(args.x1, args.y1);
            in_step(document, "Snip", |doc| {
                let (trimmed, deleted) = snip(doc, from, to)?;
                Ok(json!({ "ids": trimmed, "deleted": deleted }))
            })
        }
        "ops.freehand" => {
            let args: FreehandArgs = from_value(name, value)?;
            let tol = Tolerance::new(args.tolerance.unwrap_or(0.1))?;
            let sub = freehand_subpath(&args.points, args.smoothing.min(50), tol, args.closed)?;
            in_step(document, "Freehand", |doc| {
                let id = doc.insert_entity(Entity {
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
                });
                doc.set_selection(&[id]);
                Ok(json!({ "id": id }))
            })
        }
        "measure.at" => {
            let args: PickArgs = from_value(name, value)?;
            if !(args.tolerance.is_finite() && args.tolerance >= 0.0) {
                return Err(CoreError::validation(
                    "the pick radius must be zero or more",
                ));
            }
            let hit = measure_at(document, Point::new(args.x, args.y), args.tolerance);
            KernelReply::ok(&json!({ "hit": hit }))
        }
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

fn from_value<T: serde::de::DeserializeOwned>(name: &str, value: Value) -> Result<T, CoreError> {
    serde_json::from_value(value).map_err(|error| CoreError::invalid_args(name, error))
}

/// Runs `action` as one undo step called `label`, rolling back on failure.
fn in_step<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(mut value) => {
            let revision = document.commit();
            value["revision"] = json!(revision);
            KernelReply::ok(&value)
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

/// Selects every entity that exactly overlaps one drawn before it, and
/// answers their ids. Text only matches text, vectors only vectors.
///
/// # Errors
/// [`CoreError::Validation`] for a bad tolerance or unknown ids.
pub fn select_duplicates(
    document: &mut Document,
    ids: Option<&[u64]>,
    tolerance: Option<f64>,
) -> Result<Vec<u64>, CoreError> {
    let tol = Tolerance::new(tolerance.unwrap_or(0.01))?;
    let pool: Vec<u64> = match ids {
        Some(ids) if !ids.is_empty() => {
            for id in ids {
                document.entity(*id)?;
            }
            // Keep z-order so the copy drawn first is the one kept.
            document
                .selectable()
                .into_iter()
                .filter(|id| ids.contains(id))
                .collect()
        }
        _ => document.selectable(),
    };
    let mut kinds: Vec<&'static str> = Vec::new();
    for id in &pool {
        let tag = document.entity(*id)?.kind.tag();
        if !kinds.contains(&tag) {
            kinds.push(tag);
        }
    }
    let mut found: Vec<u64> = Vec::new();
    for tag in kinds {
        let mut members: Vec<u64> = Vec::new();
        let mut paths: Vec<Path> = Vec::new();
        for id in &pool {
            if document.entity(*id)?.kind.tag() != tag {
                continue;
            }
            let mut merged = Path::new();
            for (_, path) in document.paths_of(*id) {
                for sub in path.subpaths {
                    merged.push(sub);
                }
            }
            members.push(*id);
            paths.push(merged);
        }
        for index in duplicate_indices(&paths, tol) {
            found.push(members[index]);
        }
    }
    let order = document.order().to_vec();
    found.sort_by_key(|id| order.iter().position(|o| o == id));
    document.set_selection(&found);
    Ok(found)
}

/// Drag snipping: every plain vector the line from `from` to `to` crosses
/// loses the span it was crossed on, back to the nearest places it meets
/// other vectors (or its own ends). Answers the ids that were cut and the
/// ids that were cut away completely.
///
/// All spans are worked out against the drawing as it was before the drag,
/// so the order the vectors are visited in does not matter.
///
/// # Errors
/// [`CoreError::Geometry`] when the line crosses no vector.
pub fn snip(doc: &mut Document, from: Point, to: Point) -> Result<(Vec<u64>, Vec<u64>), CoreError> {
    let tol = Tolerance::default();
    if from.distance(to) < 1e-9 {
        return Err(CoreError::geometry("drag across the vectors to snip them"));
    }
    let mut blade = SubPath::open(from);
    blade.line_to(to);
    let blade = Path::from_subpath(blade);

    let pool = doc.selectable();
    let mut geometry: Vec<(u64, Path)> = Vec::new();
    for id in &pool {
        for (leaf, path) in doc.paths_of(*id) {
            geometry.push((leaf, path));
        }
    }

    let mut results: Vec<(u64, Path)> = Vec::new();
    for id in &pool {
        let entity = doc.entity(*id)?;
        let EntityKind::Path { path } = &entity.kind else {
            continue;
        };
        let hits = path_intersections(path, &blade, tol);
        if hits.is_empty() {
            continue;
        }
        let mut cutter = Path::new();
        for (other, geom) in &geometry {
            if other != id {
                for sub in &geom.subpaths {
                    cutter.push(sub.clone());
                }
            }
        }
        let mut next = Path::new();
        for (sub_index, sub) in path.subpaths.iter().enumerate() {
            let mut pieces = vec![sub.clone()];
            let mut crossings: Vec<Point> = hits
                .iter()
                .filter(|h| h.a_subpath == sub_index)
                .map(|h| h.point)
                .collect();
            crossings.sort_by(|a, b| from.distance(*a).total_cmp(&from.distance(*b)));
            for at in crossings {
                let Some(index) = piece_at(&pieces, at, tol) else {
                    continue;
                };
                let piece = pieces.remove(index);
                match trim_to_intersection(&piece, &cutter, at, tol) {
                    Ok(rest) => {
                        for (k, p) in rest.into_iter().enumerate() {
                            pieces.insert(index + k, p);
                        }
                    }
                    // Nothing bounds this span, so all of it goes.
                    Err(CoreError::Geometry(_)) => {}
                    Err(other) => return Err(other),
                }
            }
            for piece in pieces {
                next.push(piece);
            }
        }
        results.push((*id, next));
    }
    if results.is_empty() {
        return Err(CoreError::geometry("the line does not cross any vector"));
    }
    let mut trimmed = Vec::new();
    let mut deleted = Vec::new();
    for (id, path) in results {
        if path.subpaths.is_empty() {
            deleted.push(id);
        } else {
            crate::doc::ops::set_path(doc, id, path)?;
            trimmed.push(id);
        }
    }
    if !deleted.is_empty() {
        doc.delete_entities(&deleted);
    }
    Ok((trimmed, deleted))
}

/// The piece of a cut up subpath that passes through `at`.
fn piece_at(pieces: &[SubPath], at: Point, tol: Tolerance) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, piece) in pieces.iter().enumerate() {
        let path = Path::from_subpath(piece.clone());
        if let Some(near) = nearest_point_on_path(&path, at, tol) {
            if best.is_none_or(|(_, d)| near.distance < d) {
                best = Some((i, near.distance));
            }
        }
    }
    best.filter(|(_, d)| *d < 1e-3).map(|(i, _)| i)
}

/// What the Measure tool reports about the vector nearest a point within
/// `radius` millimetres, or null when there is none.
pub fn measure_at(doc: &Document, at: Point, radius: f64) -> Value {
    let tol = Tolerance::default();
    let mut best: Option<(u64, Path, crate::geom::NearestPoint)> = None;
    for id in doc.selectable() {
        for (leaf, path) in doc.paths_of(id) {
            if let Some(near) = nearest_point_on_path(&path, at, tol) {
                if near.distance <= radius
                    && best
                        .as_ref()
                        .is_none_or(|(_, _, b)| near.distance < b.distance)
                {
                    best = Some((leaf, path, near));
                }
            }
        }
    }
    let Some((id, path, near)) = best else {
        return Value::Null;
    };
    let Some(sub) = path.subpaths.get(near.subpath_index) else {
        return Value::Null;
    };
    let Some(segment) = sub.segments.get(near.segment_index) else {
        return Value::Null;
    };
    let start = sub.segment_start(near.segment_index).unwrap_or(sub.start);
    let end = segment.end();
    let mut seg = json!({
        "kind": segment.kind(),
        "length": segment_length(start, segment, tol),
        "start": start,
        "end": end,
    });
    if let Segment::Arc { bulge, .. } = segment {
        if let Some(arc) = ArcGeom::from_bulge(start, end, *bulge) {
            seg["radius"] = json!(arc.radius);
            seg["centre"] = json!(arc.centre);
            seg["sweepDeg"] = json!(arc.sweep.to_degrees());
        }
    }
    let bbox = path_bbox(&path);
    json!({
        "id": id,
        "point": near.point,
        "distance": near.distance,
        "length": path_length(&path, tol),
        "closed": path_is_closed(&path),
        "area": if path_is_closed(&path) {
            crate::geom::path_area(&path, tol).abs()
        } else {
            0.0
        },
        "spans": path.subpaths.iter().map(|s| s.segments.len()).sum::<usize>(),
        "path": path,
        "width": bbox.map_or(0.0, |b| b.width()),
        "height": bbox.map_or(0.0, |b| b.height()),
        "segment": seg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kernel;

    fn run(kernel: &mut Kernel, name: &str, args: Value) -> Value {
        match kernel.apply(name, &args.to_string()) {
            Ok(KernelReply::Json(text)) => {
                let envelope: Value = serde_json::from_str(&text).expect("json");
                envelope["result"].clone()
            }
            Ok(_) => panic!("{name} answered bytes"),
            Err(error) => panic!("{name}: {error}"),
        }
    }

    fn line(kernel: &mut Kernel, a: (f64, f64), b: (f64, f64)) -> u64 {
        let args = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
            "start": { "x": a.0, "y": a.1 },
            "segments": [{ "kind": "line", "to": { "x": b.0, "y": b.1 } }],
            "closed": false }] } } });
        run(kernel, "entities.create", args)["id"]
            .as_u64()
            .expect("id")
    }

    fn square(kernel: &mut Kernel, x: f64, y: f64, s: f64) -> u64 {
        let args = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
            "start": { "x": x, "y": y },
            "segments": [
                { "kind": "line", "to": { "x": x + s, "y": y } },
                { "kind": "line", "to": { "x": x + s, "y": y + s } },
                { "kind": "line", "to": { "x": x, "y": y + s } },
                { "kind": "line", "to": { "x": x, "y": y } }],
            "closed": true }] } } });
        run(kernel, "entities.create", args)["id"]
            .as_u64()
            .expect("id")
    }

    fn length_of(kernel: &Kernel, id: u64) -> f64 {
        let path = kernel
            .document()
            .entity(id)
            .expect("entity")
            .path()
            .expect("path");
        path_length(&path, Tolerance::default())
    }

    #[test]
    fn select_duplicates_picks_the_later_copies() {
        let mut kernel = Kernel::new();
        let a = square(&mut kernel, 0.0, 0.0, 10.0);
        let b = square(&mut kernel, 0.0, 0.0, 10.0);
        let c = square(&mut kernel, 30.0, 0.0, 10.0);
        let d = line(&mut kernel, (0.0, 0.0), (10.0, 0.0));
        let e = line(&mut kernel, (10.0, 0.0), (0.0, 0.0));
        let reply = run(&mut kernel, "ops.selectDuplicates", json!({}));
        assert_eq!(reply["ids"], json!([b, e]));
        assert_eq!(kernel.document().selection(), &[b, e]);
        let within = run(
            &mut kernel,
            "ops.selectDuplicates",
            json!({ "ids": [a, c, d] }),
        );
        assert_eq!(within["ids"], json!([]));
        assert!(kernel
            .apply("ops.selectDuplicates", r#"{"tolerance": -1}"#)
            .is_err());
    }

    #[test]
    fn snipping_across_a_grid_removes_only_the_crossed_spans() {
        let mut kernel = Kernel::new();
        // Three vertical lines and one horizontal line through them.
        let h = line(&mut kernel, (0.0, 50.0), (100.0, 50.0));
        let v1 = line(&mut kernel, (20.0, 0.0), (20.0, 100.0));
        let v2 = line(&mut kernel, (50.0, 0.0), (50.0, 100.0));
        let v3 = line(&mut kernel, (80.0, 0.0), (80.0, 100.0));
        // A drag from (35, 56) to (65, 40) crosses the horizontal line at
        // x = 46.25, between v1 and v2, and v2 at y = 48, below the
        // horizontal line.
        let reply = run(
            &mut kernel,
            "ops.snip",
            json!({ "x0": 35.0, "y0": 56.0, "x1": 65.0, "y1": 40.0 }),
        );
        let ids = reply["ids"].as_array().expect("ids").clone();
        assert!(
            ids.contains(&json!(h)) && ids.contains(&json!(v2)),
            "{reply}"
        );
        assert!((length_of(&kernel, h) - 70.0).abs() < 1e-6);
        assert!(
            (length_of(&kernel, v2) - 50.0).abs() < 1e-6,
            "{}",
            length_of(&kernel, v2)
        );
        assert!((length_of(&kernel, v1) - 100.0).abs() < 1e-9);
        assert!((length_of(&kernel, v3) - 100.0).abs() < 1e-9);
        // One undo puts everything back.
        run(&mut kernel, "undo", json!({}));
        assert!((length_of(&kernel, h) - 100.0).abs() < 1e-9);
        assert!((length_of(&kernel, v2) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_span_nothing_bounds_is_removed_whole() {
        let mut kernel = Kernel::new();
        let lone = line(&mut kernel, (0.0, 0.0), (10.0, 0.0));
        let reply = run(
            &mut kernel,
            "ops.snip",
            json!({ "x0": 5.0, "y0": -5.0, "x1": 5.0, "y1": 5.0 }),
        );
        assert_eq!(reply["deleted"], json!([lone]));
        assert!(kernel.document().entity(lone).is_err());
        assert!(kernel
            .apply(
                "ops.snip",
                &json!({ "x0": 50.0, "y0": 50.0, "x1": 60.0, "y1": 60.0 }).to_string()
            )
            .is_err());
    }

    #[test]
    fn snipping_a_square_edge_between_two_crossing_lines() {
        let mut kernel = Kernel::new();
        let sq = square(&mut kernel, 0.0, 0.0, 40.0);
        line(&mut kernel, (10.0, -10.0), (10.0, 10.0));
        line(&mut kernel, (30.0, -10.0), (30.0, 10.0));
        run(
            &mut kernel,
            "ops.snip",
            json!({ "x0": 20.0, "y0": -5.0, "x1": 20.0, "y1": 5.0 }),
        );
        assert!((length_of(&kernel, sq) - 140.0).abs() < 1e-6);
    }

    #[test]
    fn freehand_makes_a_selected_curve() {
        let mut kernel = Kernel::new();
        let points: Vec<Value> = (0..=50)
            .map(|i| {
                let x = f64::from(i);
                json!({ "x": x, "y": (x / 8.0).sin() * 10.0 })
            })
            .collect();
        let reply = run(
            &mut kernel,
            "ops.freehand",
            json!({ "points": points, "smoothing": 2, "tolerance": 0.1 }),
        );
        let id = reply["id"].as_u64().expect("id");
        assert_eq!(kernel.document().selection(), &[id]);
        assert!(length_of(&kernel, id) > 50.0);
        run(&mut kernel, "undo", json!({}));
        assert!(kernel.document().entity(id).is_err());
        assert!(kernel
            .apply("ops.freehand", r#"{"points":[{"x":1,"y":1}]}"#)
            .is_err());
    }

    #[test]
    fn measure_reports_the_arc_under_the_pointer() {
        let mut kernel = Kernel::new();
        let args = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
            "start": { "x": 10.0, "y": 0.0 },
            "segments": [
                { "kind": "arc", "to": { "x": -10.0, "y": 0.0 }, "bulge": 1.0 },
                { "kind": "arc", "to": { "x": 10.0, "y": 0.0 }, "bulge": 1.0 }],
            "closed": true }] } } });
        let id = run(&mut kernel, "entities.create", args)["id"]
            .as_u64()
            .expect("id");
        let reply = run(
            &mut kernel,
            "measure.at",
            json!({ "x": 0.0, "y": 10.5, "tolerance": 1.0 }),
        );
        let hit = &reply["hit"];
        assert_eq!(hit["id"], id);
        assert_eq!(hit["segment"]["kind"], "arc");
        assert!((hit["segment"]["radius"].as_f64().unwrap_or(0.0) - 10.0).abs() < 1e-9);
        assert!((hit["length"].as_f64().unwrap_or(0.0) - 20.0 * std::f64::consts::PI).abs() < 0.01);
        assert_eq!(hit["closed"], true);
        let area = hit["area"].as_f64().unwrap_or(0.0);
        assert!((area - 100.0 * std::f64::consts::PI).abs() < 0.01, "{area}");
        assert_eq!(hit["spans"], 2);
        assert_eq!(hit["segment"]["start"]["x"], 10.0);
        let miss = run(
            &mut kernel,
            "measure.at",
            json!({ "x": 0.0, "y": 30.0, "tolerance": 1.0 }),
        );
        assert!(miss["hit"].is_null());
    }
}
