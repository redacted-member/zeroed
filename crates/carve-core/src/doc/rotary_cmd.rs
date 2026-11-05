//! Rotary job vector commands.
//!
//! `rotary.unwrapContour` is the Vector Unwrapper: it unrolls a closed
//! cross-section contour about a centre of rotation into an open vector
//! whose width is once round the blank (angle along it) and whose height is
//! the contour's distance from the centre at that angle, the profile a Two
//! Rail Sweep turns into a rotary model.
//!
//! `rotary.unwrapVectors` maps a side view drawing onto the unwrapped
//! surface and back.
//!
//! A design is often drawn as the blank looks from the side, where the
//! visible half of the cylinder spans one diameter across the axis. Laid on
//! the unwrapped surface as it is, such a design comes out squashed towards
//! its edges once wrapped. Unwrapping maps a distance `d` from the centre
//! line of the layout to the arc length `r * asin(d / r)`, so that, wrapped,
//! the design looks from the side as it was drawn. Wrapping is the inverse,
//! `r * sin(s / r)`. Curves are flattened and densified before they are
//! mapped, so the results are polylines.

use serde::Deserialize;
use serde_json::json;

use crate::doc::entity::{merge, EntityKind};
use crate::doc::{Document, RotaryAxis};
use crate::error::CoreError;
use crate::geom::flatten::flatten_path;
use crate::geom::{Path, Point, SubPath, Tolerance};
use crate::kernel::KernelReply;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(name, "rotary.unwrapVectors" | "rotary.unwrapContour")
}

/// Samples round the centre of an unwrapped contour: every half degree.
pub const UNWRAP_SAMPLES: usize = 720;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContourArgs {
    /// The closed contour; the single selected vector when absent.
    #[serde(default)]
    id: Option<u64>,
    /// Centre of rotation, millimetres; the middle of the contour's box
    /// when absent.
    #[serde(default)]
    centre: Option<Point>,
    /// Refine unwrapped vectors: far fewer nodes.
    #[serde(default)]
    refine: bool,
}

/// The distance from `centre` to the outermost crossing of `contour` along
/// the ray at `angle` radians, or `None` when the ray misses it.
fn reach(contour: &[Point], centre: Point, angle: f64) -> Option<f64> {
    let (dy, dx) = angle.sin_cos();
    let mut best: Option<f64> = None;
    let n = contour.len();
    for i in 0..n {
        let a = contour[i];
        let b = contour[(i + 1) % n];
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let denom = dx * ey - dy * ex;
        if denom.abs() < 1e-12 {
            continue;
        }
        let (wx, wy) = (a.x - centre.x, a.y - centre.y);
        let t = (wx * ey - wy * ex) / denom;
        let s = (wx * dy - wy * dx) / denom;
        if t >= 0.0 && (-1e-9..=1.0 + 1e-9).contains(&s) {
            best = Some(best.map_or(t, |r: f64| r.max(t)));
        }
    }
    best
}

/// The unrolled profile of a closed `contour` about `centre`:
/// `samples + 1` points from angle 0 (to the right of the centre) once round
/// anticlockwise, `width` long, each `origin.y` plus the contour's distance
/// from the centre at that angle.
///
/// # Errors
/// Returns [`CoreError::Validation`] when no ray from the centre meets the
/// contour.
pub fn unroll_contour(
    contour: &[Point],
    centre: Point,
    width: f64,
    origin: Point,
    samples: usize,
) -> Result<Vec<Point>, CoreError> {
    let samples = samples.max(8);
    let mut out = Vec::with_capacity(samples + 1);
    let mut hits = 0;
    for i in 0..=samples {
        let f = i as f64 / samples as f64;
        let r = reach(contour, centre, f * std::f64::consts::TAU);
        if r.is_some() {
            hits += 1;
        }
        out.push(Point::new(
            origin.x + f * width,
            origin.y + r.unwrap_or(0.0),
        ));
    }
    if hits == 0 {
        return Err(CoreError::validation(
            "the center of rotation must be inside the contour",
        ));
    }
    Ok(out)
}

fn unwrap_contour(document: &mut Document, args: ContourArgs) -> Result<KernelReply, CoreError> {
    let job = *document.job_setup();
    if !job.is_rotary() {
        return Err(CoreError::validation(
            "the vector unwrapper works on rotary jobs only",
        ));
    }
    let id = match args.id {
        Some(id) => id,
        None => match document.selection() {
            [one] => *one,
            _ => return Err(CoreError::validation("select one closed contour to unwrap")),
        },
    };
    let entity = document.entity(id)?;
    let (layer, sheet) = (entity.layer, entity.sheet);
    let path = entity
        .path()
        .ok_or_else(|| CoreError::validation("select one closed contour to unwrap"))?;
    let [sub] = path.subpaths.as_slice() else {
        return Err(CoreError::validation("select one closed contour to unwrap"));
    };
    if !sub.closed {
        return Err(CoreError::validation(
            "the contour to unwrap must be closed",
        ));
    }
    let box_ = crate::geom::path_bbox(&path)
        .ok_or_else(|| CoreError::validation("the contour has no size"))?;
    let centre = args.centre.unwrap_or_else(|| box_.centre());
    let tol = Tolerance::new((box_.width().max(box_.height()) / 5000.0).clamp(0.001, 0.02))?;
    let contour = crate::geom::flatten::flatten_subpath(sub, tol).points;
    let origin = Point::new(box_.max.x + box_.width().max(10.0) * 0.25, box_.min.y);
    let points = unroll_contour(
        &contour,
        centre,
        job.rotary.circumference(),
        origin,
        UNWRAP_SAMPLES,
    )?;
    let polyline = crate::geom::Polyline::new(points, false);
    let unrolled = if args.refine {
        let fit = Tolerance::new(0.02)?;
        crate::geom::flatten::path_from_polyline(
            &crate::geom::simplify::simplify_polyline(&polyline, fit),
            fit,
        )
    } else {
        let mut sub = SubPath::open(polyline.points[0]);
        for p in &polyline.points[1..] {
            sub.line_to(*p);
        }
        Path::from_subpath(sub)
    };
    crate::kernel::edit(document, "Vector Unwrapper", |doc| {
        let made = doc.insert_entity(crate::doc::Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path: unrolled },
        });
        doc.set_selection(&[made]);
        Ok(json!({ "id": made }))
    })
}

/// Which way the vectors are mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// From a side view drawing to the unwrapped surface.
    #[default]
    Unwrap,
    /// From the unwrapped surface back to a side view drawing.
    Wrap,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnwrapArgs {
    /// Vectors to map; the selection when absent.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    direction: Direction,
}

/// Maps a distance across the axis from the layout's centre line, in
/// millimetres, for a blank of `radius`. Unwrapping clamps to the visible
/// half of the blank (a quarter turn either way).
pub fn map_across(offset: f64, radius: f64, direction: Direction) -> f64 {
    if radius <= 0.0 {
        return offset;
    }
    match direction {
        Direction::Unwrap => radius * (offset / radius).clamp(-1.0, 1.0).asin(),
        Direction::Wrap => {
            let limit = radius * std::f64::consts::FRAC_PI_2;
            radius * (offset.clamp(-limit, limit) / radius).sin()
        }
    }
}

/// Runs the unwrapper.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] when the job is not rotary, an id is unknown or
/// nothing selected is a vector.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if name == "rotary.unwrapContour" {
        let args: ContourArgs =
            serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
        return unwrap_contour(document, args);
    }
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let args: UnwrapArgs =
        serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
    let job = *document.job_setup();
    if !job.is_rotary() {
        return Err(CoreError::validation(
            "the vector unwrapper works on rotary jobs only",
        ));
    }
    let radius = job.rotary.radius();
    let bounds = document.job_bounds();
    let axis = job.rotary.axis;
    let centre = match axis {
        RotaryAxis::X => (bounds.min.y + bounds.max.y) / 2.0,
        RotaryAxis::Y => (bounds.min.x + bounds.max.x) / 2.0,
    };
    // Short enough that a straight line stays close to its mapped curve.
    let step = (radius / 60.0).max(0.05);
    let tol = Tolerance::new((radius / 2000.0).clamp(0.002, 0.02))?;
    let map = |p: Point| -> Point {
        match axis {
            RotaryAxis::X => Point::new(
                p.x,
                centre + map_across(p.y - centre, radius, args.direction),
            ),
            RotaryAxis::Y => Point::new(
                centre + map_across(p.x - centre, radius, args.direction),
                p.y,
            ),
        }
    };

    let ids = args
        .ids
        .clone()
        .unwrap_or_else(|| document.selection().to_vec());
    let mut mapped: Vec<(u64, Path)> = Vec::new();
    for id in &ids {
        document.entity(*id)?;
        for leaf in document.leaves(*id) {
            if mapped.iter().any(|(done, _)| *done == leaf) {
                continue;
            }
            let source = match &document.entity(leaf)?.kind {
                EntityKind::Path { path } => path.clone(),
                EntityKind::Text(text) => merge(&text.paths),
                _ => continue,
            };
            mapped.push((leaf, map_path(&source, tol, step, map)));
        }
    }
    if mapped.is_empty() {
        return Err(CoreError::validation(
            "select the vectors to unwrap; only vectors and text can be unwrapped",
        ));
    }

    let label = match args.direction {
        Direction::Unwrap => "Unwrap Vectors",
        Direction::Wrap => "Wrap Vectors",
    };
    document.begin(label);
    let mut changed = Vec::with_capacity(mapped.len());
    for (id, path) in mapped {
        let mut entity = match document.entity(id) {
            Ok(entity) => entity.clone(),
            Err(error) => {
                document.rollback();
                return Err(error);
            }
        };
        entity.kind = EntityKind::Path { path };
        if let Err(error) = document.replace_entity(id, entity) {
            document.rollback();
            return Err(error);
        }
        changed.push(id);
    }
    let revision = document.commit();
    KernelReply::ok(&json!({ "ids": changed, "revision": revision }))
}

/// Flattens `path`, splits long segments so no piece is longer than `step`,
/// and maps every point.
fn map_path(path: &Path, tol: Tolerance, step: f64, map: impl Fn(Point) -> Point) -> Path {
    let mut out = Path::new();
    for line in flatten_path(path, tol) {
        let Some(first) = line.points.first().copied() else {
            continue;
        };
        let mut sub = SubPath::open(map(first));
        let mut previous = first;
        let mut points: Vec<Point> = line.points.iter().skip(1).copied().collect();
        if line.closed {
            points.push(first);
        }
        let count = points.len();
        for (index, point) in points.into_iter().enumerate() {
            let length = previous.distance(point);
            let pieces = ((length / step).ceil() as usize).clamp(1, 10_000);
            for k in 1..=pieces {
                if line.closed && index + 1 == count && k == pieces {
                    break;
                }
                let t = k as f64 / pieces as f64;
                let at = Point::new(
                    previous.x + (point.x - previous.x) * t,
                    previous.y + (point.y - previous.y) * t,
                );
                sub.line_to(map(at));
            }
            previous = point;
        }
        sub.closed = line.closed;
        out.push(sub);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::entity::Entity;
    use crate::doc::{JobKind, JobSetup, RotarySetup, XyOrigin};
    use crate::geom::path_bbox;

    fn rotary_doc(axis: RotaryAxis) -> Document {
        let mut doc = Document::default();
        let job = JobSetup {
            kind: JobKind::Rotary,
            xy_origin: XyOrigin::BottomLeft,
            rotary: RotarySetup {
                diameter: 40.0,
                axis,
                length: 100.0,
                ..RotarySetup::default()
            },
            ..JobSetup::default()
        };
        doc.set_job(job).expect("rotary job");
        doc
    }

    fn line(doc: &mut Document, from: Point, to: Point) -> u64 {
        let mut sub = SubPath::open(from);
        sub.line_to(to);
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

    #[test]
    fn unwrapping_and_wrapping_are_inverses() {
        for offset in [-19.0, -7.5, 0.0, 3.25, 20.0] {
            let there = map_across(offset, 20.0, Direction::Unwrap);
            let back = map_across(there, 20.0, Direction::Wrap);
            assert!((back - offset).abs() < 1e-9, "{offset} came back {back}");
        }
        // The edge of the side view is a quarter of the way round.
        let edge = map_across(20.0, 20.0, Direction::Unwrap);
        assert!((edge - 20.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(map_across(30.0, 20.0, Direction::Unwrap), edge);
    }

    #[test]
    fn a_side_view_line_is_stretched_round_the_blank() {
        let mut doc = rotary_doc(RotaryAxis::X);
        let centre = std::f64::consts::PI * 40.0 / 2.0;
        // A line across the side view from the axis line to the silhouette.
        let id = line(
            &mut doc,
            Point::new(10.0, centre),
            Point::new(10.0, centre + 20.0),
        );
        let reply = dispatch(
            &mut doc,
            "rotary.unwrapVectors",
            &json!({ "ids": [id] }).to_string(),
        )
        .expect("unwraps");
        assert!(matches!(reply, KernelReply::Json(_)));
        let EntityKind::Path { path } = &doc.entity(id).expect("still there").kind else {
            panic!("a path");
        };
        let bbox = path_bbox(path).expect("has a box");
        assert!((bbox.max.y - (centre + 20.0 * std::f64::consts::FRAC_PI_2)).abs() < 1e-6);
        assert!((bbox.min.y - centre).abs() < 1e-6);
        assert!(doc.undo().is_some());
    }

    #[test]
    fn a_blank_along_y_maps_x() {
        let mut doc = rotary_doc(RotaryAxis::Y);
        let centre = std::f64::consts::PI * 40.0 / 2.0;
        let id = line(
            &mut doc,
            Point::new(centre - 10.0, 5.0),
            Point::new(centre - 10.0, 50.0),
        );
        dispatch(
            &mut doc,
            "rotary.unwrapVectors",
            &json!({ "ids": [id] }).to_string(),
        )
        .expect("unwraps");
        let EntityKind::Path { path } = &doc.entity(id).expect("still there").kind else {
            panic!("a path");
        };
        let bbox = path_bbox(path).expect("has a box");
        let expected = centre - 20.0 * (0.5f64).asin();
        assert!((bbox.min.x - expected).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.min.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn flat_jobs_are_refused() {
        let mut doc = Document::default();
        let id = line(&mut doc, Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        assert!(dispatch(
            &mut doc,
            "rotary.unwrapVectors",
            &json!({ "ids": [id] }).to_string()
        )
        .is_err());
    }

    #[test]
    fn a_circle_unrolls_flat_and_a_square_in_four_humps() {
        let square = [
            Point::new(-10.0, -10.0),
            Point::new(10.0, -10.0),
            Point::new(10.0, 10.0),
            Point::new(-10.0, 10.0),
        ];
        let out = unroll_contour(
            &square,
            Point::new(0.0, 0.0),
            100.0,
            Point::new(5.0, 0.0),
            8,
        )
        .expect("unrolled");
        assert_eq!(out.len(), 9);
        assert!((out[0].x - 5.0).abs() < 1e-9 && (out[8].x - 105.0).abs() < 1e-9);
        // Straight out to a side, then to a corner, at 45 degree steps.
        assert!((out[0].y - 10.0).abs() < 1e-9);
        assert!((out[1].y - 10.0 * 2f64.sqrt()).abs() < 1e-9);
        assert!((out[2].y - 10.0).abs() < 1e-9);
        assert!(
            unroll_contour(
                &square,
                Point::new(50.0, 0.0),
                100.0,
                Point::new(0.0, 0.0),
                8
            )
            .is_ok(),
            "a centre outside still meets it on some rays"
        );
    }

    #[test]
    fn the_unwrapper_makes_a_profile_once_round_the_blank() {
        let mut doc = rotary_doc(RotaryAxis::X);
        let mut sub = SubPath::open(Point::new(40.0, 20.0));
        sub.line_to(Point::new(60.0, 20.0))
            .line_to(Point::new(60.0, 40.0))
            .line_to(Point::new(40.0, 40.0));
        sub.closed = true;
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
        let args = json!({ "id": id, "refine": false }).to_string();
        let reply = dispatch(&mut doc, "rotary.unwrapContour", &args).expect("unwrap");
        let KernelReply::Json(text) = reply else {
            panic!("json");
        };
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        let made = value["result"]["id"].as_u64().expect("id");
        let path = doc.entity(made).expect("made").path().expect("path");
        let b = path_bbox(&path).expect("box");
        let round = doc.job_setup().rotary.circumference();
        assert!((b.width() - round).abs() < 1e-6, "{b:?}");
        assert!(
            (b.min.y - 30.0).abs() < 1e-6 && (b.max.y - (20.0 + 10.0 * 2f64.sqrt())).abs() < 1e-6
        );
        assert_eq!(path.subpaths[0].segments.len(), UNWRAP_SAMPLES);
        let refined = json!({ "id": id, "refine": true }).to_string();
        dispatch(&mut doc, "rotary.unwrapContour", &refined).expect("refined");
        let fewer = doc
            .entity(doc.selection()[0])
            .expect("made")
            .path()
            .expect("path");
        assert!(fewer.subpaths[0].segments.len() < UNWRAP_SAMPLES / 4);
        let open = line(&mut doc, Point::new(0.0, 0.0), Point::new(5.0, 5.0));
        let bad = json!({ "id": open }).to_string();
        assert!(dispatch(&mut doc, "rotary.unwrapContour", &bad).is_err());
    }
}
