//! Offsetting vectors, arc preserving, built on `cavalier_contours`.
//!
//! Closed vectors may be wound either way: nesting decides which loops are
//! holes, so offsetting inward always takes material away and offsetting
//! outward always adds it, holes included.

use cavalier_contours::polyline::{PlineSource, PlineSourceMut, Polyline as CPolyline};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::flatten::flatten_segment_into;
use super::metrics::subpath_signed_area;
use super::path::{ArcGeom, Path, Segment, SubPath};
use super::point::{Point, Tolerance};

/// Which way an offset moves relative to the material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetSide {
    /// Take material away: outer loops shrink and holes grow.
    Inward,
    /// Add material: outer loops grow and holes shrink.
    Outward,
    /// Both results, inward first.
    Both,
}

/// How an offset treats a corner that opens up as it moves outward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetCorner {
    /// Bridge the corner with an arc of the offset radius.
    Round,
    /// Carry both edges on to the point where they meet.
    Sharp,
}

/// How far past the offset ends a sharp corner is allowed to reach, as a
/// multiple of the offset distance. Sharper corners keep their arc.
const MITRE_LIMIT: f64 = 10.0;

/// Converts a subpath into a bulge polyline, flattening any cubics.
pub(crate) fn to_pline(subpath: &SubPath, tol: Tolerance) -> CPolyline {
    let mut pline = CPolyline::new();
    for (start, seg) in subpath.iter() {
        match *seg {
            Segment::Line { .. } => pline.add(start.x, start.y, 0.0),
            Segment::Arc { bulge, .. } => pline.add(start.x, start.y, bulge),
            Segment::Cubic { .. } => {
                let mut pts = Vec::new();
                flatten_segment_into(start, seg, tol, &mut pts);
                pline.add(start.x, start.y, 0.0);
                for p in pts.iter().take(pts.len().saturating_sub(1)) {
                    pline.add(p.x, p.y, 0.0);
                }
            }
        }
    }
    if subpath.closed {
        pline.set_is_closed(true);
    } else {
        let end = subpath.end();
        pline.add(end.x, end.y, 0.0);
    }
    pline
}

/// Converts a bulge polyline back into a subpath.
pub(crate) fn from_pline(pline: &CPolyline) -> Option<SubPath> {
    let n = pline.vertex_count();
    if n < 2 {
        return None;
    }
    let first = pline.at(0);
    let mut sub = SubPath::open(Point::new(first.x, first.y));
    for i in 0..n - 1 {
        let v = pline.at(i);
        let next = pline.at(i + 1);
        sub.arc_to(Point::new(next.x, next.y), v.bulge);
    }
    if pline.is_closed() {
        let last = pline.at(n - 1);
        sub.arc_to(Point::new(first.x, first.y), last.bulge);
        sub.closed = true;
    }
    Some(normalise_bulges(sub))
}

/// Replaces arcs whose bulge is effectively zero with straight lines.
fn normalise_bulges(sub: SubPath) -> SubPath {
    let segments = sub
        .segments
        .into_iter()
        .map(|seg| match seg {
            Segment::Arc { to, bulge } if bulge.abs() < 1e-12 => Segment::Line { to },
            other => other,
        })
        .collect();
    SubPath::new(sub.start, segments, sub.closed)
}

fn check_distance(distance: f64) -> Result<(), CoreError> {
    if !distance.is_finite() || distance <= 0.0 {
        return Err(CoreError::geometry(format!(
            "offset distance must be a positive number of millimeters, got {distance}"
        )));
    }
    Ok(())
}

/// Offsets every closed subpath of the path.
///
/// The closed subpaths are offset together as one area: nesting decides
/// which loops are holes (even-odd), whatever way each was drawn, so a
/// clockwise outline (every SVG import) still grows outward, and a ring
/// shrunk past half its wall gives nothing rather than crossing loops.
/// Each resulting loop comes back as its own single subpath [`Path`]. A loop
/// that collapses to nothing at that distance is dropped. Open subpaths get
/// the closed outline treatment of [`outline_open`].
pub fn offset_path(
    path: &Path,
    distance: f64,
    side: OffsetSide,
    corner: OffsetCorner,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    check_distance(distance)?;
    let signs: &[f64] = match side {
        OffsetSide::Inward => &[1.0],
        OffsetSide::Outward => &[-1.0],
        OffsetSide::Both => &[1.0, -1.0],
    };
    let closed = Path::from_subpaths(
        path.subpaths
            .iter()
            .filter(|s| s.closed && !s.segments.is_empty())
            .cloned()
            .collect(),
    );
    let region = super::region::Region::from_paths(&[closed], tol);
    let mut out = Vec::new();
    for sign in signs {
        for sub in &path.subpaths {
            if !sub.segments.is_empty() && !sub.closed {
                out.extend(outline_open(sub, distance, tol)?);
            }
        }
        for mut piece in region.offset(sign * distance, tol).loops {
            if corner == OffsetCorner::Sharp {
                piece = sharpen_corners(&piece, distance, tol);
            }
            if subpath_signed_area(&piece).abs() <= tol.get() * tol.get() {
                continue;
            }
            out.push(piece.into_path());
        }
    }
    Ok(out)
}

/// The parallel curve of an open vector on one side only.
///
/// `left` picks the side: true offsets to the left of the direction of
/// travel, false to the right. Closed subpaths are offset the same way.
pub fn parallel_offset_open(
    path: &Path,
    distance: f64,
    left: bool,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    check_distance(distance)?;
    let sign = if left { 1.0 } else { -1.0 };
    let mut out = Vec::new();
    for sub in &path.subpaths {
        if sub.segments.is_empty() {
            continue;
        }
        let pline = to_pline(sub, tol);
        for result in pline.parallel_offset(sign * distance) {
            if let Some(piece) = from_pline(&result) {
                out.push(piece.into_path());
            }
        }
    }
    Ok(out)
}

/// Wraps an open vector in a closed outline at `distance` from it, with
/// round ends, which is what offsetting an open vector produces.
///
/// When the two sides break into more than one piece (a vector that turns
/// back on itself closer than twice the distance), the outline is built as
/// the union of round ended strips along each flattened span instead, so it
/// is still closed.
pub fn outline_open(
    subpath: &SubPath,
    distance: f64,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    check_distance(distance)?;
    if subpath.segments.len() > 1 && subpath.start.distance(subpath.end()) <= tol.get() {
        if let Some(ring) = outline_ring(subpath, distance, tol) {
            return Ok(ring);
        }
    }
    let pline = to_pline(subpath, tol);
    let left = pline.parallel_offset(distance);
    let right = pline.parallel_offset(-distance);
    if left.len() != 1 || right.len() != 1 {
        return outline_by_strips(subpath, distance, tol);
    }
    let (Some(left_sub), Some(right_sub)) = (from_pline(&left[0]), from_pline(&right[0])) else {
        return Ok(Vec::new());
    };
    let right_rev = super::edit::reverse_subpath(&right_sub);

    let mut loop_sub = SubPath::open(left_sub.start);
    loop_sub.segments.extend(left_sub.segments.iter().copied());
    // Round cap at the far end, a half turn clockwise round the true end.
    loop_sub.arc_to(right_rev.start, -1.0);
    loop_sub.segments.extend(right_rev.segments.iter().copied());
    // Round cap back at the start.
    loop_sub.arc_to(left_sub.start, -1.0);
    loop_sub.closed = true;
    if subpath_signed_area(&loop_sub) < 0.0 {
        loop_sub = super::edit::reverse_subpath(&loop_sub);
    }
    Ok(vec![loop_sub.into_path()])
}

/// The outline of an open vector whose ends meet (a closed shape not marked
/// closed, common from DXF): a ring `distance` either side of it, as one
/// vector with the inner loop as its hole. None for a shape that crosses
/// itself (a figure of eight), which is outlined as a line.
fn outline_ring(subpath: &SubPath, distance: f64, tol: Tolerance) -> Option<Vec<Path>> {
    let mut closed = subpath.clone();
    if let Some(last) = closed.segments.last_mut() {
        last.set_end(closed.start);
    }
    closed.closed = true;
    if subpath_signed_area(&closed).abs() <= tol.get() * tol.get()
        || super::intersect::subpath_self_intersects(&closed, tol)
    {
        return None;
    }
    let region = super::region::Region::from_paths(&[closed.into_path()], tol);
    let mut ring = Path::new();
    for outer in region.offset(-distance, tol).loops {
        ring.push(outer);
    }
    for inner in region.offset(distance, tol).loops {
        ring.push(super::edit::reverse_subpath(&inner));
    }
    Some(if ring.is_empty() {
        Vec::new()
    } else {
        vec![ring]
    })
}

/// The outline of an open vector as the union of a round ended strip along
/// each span of its flattened line; a straight span always offsets into one
/// piece on each side, so every strip is one closed loop.
fn outline_by_strips(
    subpath: &SubPath,
    distance: f64,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    // A single straight span that still splits has nothing smaller to try.
    if let [Segment::Line { .. }] = subpath.segments.as_slice() {
        return Ok(Vec::new());
    }
    let line = super::flatten::flatten_subpath(subpath, tol);
    let mut strips = Vec::new();
    for pair in line.points.windows(2) {
        if pair[0].distance(pair[1]) <= tol.get() {
            continue;
        }
        let mut span = SubPath::open(pair[0]);
        span.line_to(pair[1]);
        strips.extend(outline_open(&span, distance, tol)?);
    }
    if strips.is_empty() {
        return Ok(Vec::new());
    }
    super::boolean::weld(&strips, tol)
}

/// Replaces the corner arcs an offset introduces with mitred points.
///
/// Only arcs whose radius matches the offset distance and that sit between
/// two straight segments are changed, and only when the point they meet at is
/// within the mitre limit.
pub fn sharpen_corners(subpath: &SubPath, radius: f64, tol: Tolerance) -> SubPath {
    let n = subpath.segments.len();
    if n < 3 {
        return subpath.clone();
    }
    let rotated = rotate_to_line_start(subpath);
    let segs = &rotated.segments;
    let n = segs.len();
    if !matches!(segs.first(), Some(Segment::Line { .. })) {
        return subpath.clone();
    }

    let mut out: Vec<Segment> = Vec::with_capacity(n);
    let mut start = rotated.start;
    let mut cursor = rotated.start;
    let mut out_start = rotated.start;
    let limit = radius * MITRE_LIMIT;

    for (i, seg) in segs.iter().enumerate() {
        let seg_start = cursor;
        cursor = seg.end();
        let Segment::Arc { to, bulge } = *seg else {
            out.push(*seg);
            continue;
        };
        let Some(arc) = ArcGeom::from_bulge(seg_start, to, bulge) else {
            out.push(*seg);
            continue;
        };
        if (arc.radius - radius).abs() > tol.get().max(radius * 1e-6) {
            out.push(*seg);
            continue;
        }
        let wraps = i + 1 == n;
        let next = if wraps {
            if !rotated.closed {
                out.push(*seg);
                continue;
            }
            segs[0]
        } else {
            segs[i + 1]
        };
        let (Some(Segment::Line { .. }), Segment::Line { to: next_end }) = (out.last(), next)
        else {
            out.push(*seg);
            continue;
        };
        let prev_start = out
            .len()
            .checked_sub(2)
            .map(|k| out[k].end())
            .unwrap_or(out_start);
        let Some(meet) = line_meet(prev_start, seg_start, to, next_end) else {
            out.push(*seg);
            continue;
        };
        if meet.distance(seg_start) > limit || meet.distance(to) > limit {
            out.push(*seg);
            continue;
        }
        if let Some(last) = out.last_mut() {
            last.set_end(meet);
        }
        if wraps {
            start = meet;
            out_start = meet;
        }
    }
    let _ = out_start;
    SubPath::new(start, out, rotated.closed)
}

/// Rotates a closed subpath so that it begins with a straight segment.
fn rotate_to_line_start(subpath: &SubPath) -> SubPath {
    if !subpath.closed {
        return subpath.clone();
    }
    let at = subpath
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Line { .. }));
    let Some(at) = at else {
        return subpath.clone();
    };
    if at == 0 {
        return subpath.clone();
    }
    let nodes = subpath.nodes();
    let mut segments = Vec::with_capacity(subpath.segments.len());
    segments.extend_from_slice(&subpath.segments[at..]);
    segments.extend_from_slice(&subpath.segments[..at]);
    SubPath::new(nodes[at], segments, true)
}

/// Where two infinite lines meet, or `None` when they are parallel.
fn line_meet(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<Point> {
    let da = a0.to(a1);
    let db = b0.to(b1);
    let denom = da.cross(db);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = a0.to(b0).cross(db) / denom;
    Some(a0.lerp(a1, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::metrics::{path_area, path_bbox};

    const FLAT: Tolerance = Tolerance::FLATTEN;

    #[test]
    fn a_hairpin_open_vector_still_gets_one_closed_outline() {
        // Turns back 1 mm from itself, so a 2 mm outline overlaps its own sides.
        let mut hairpin = SubPath::open(Point::ORIGIN);
        hairpin
            .line_to(Point::new(20.0, 0.0))
            .line_to(Point::new(20.0, 1.0))
            .line_to(Point::new(0.0, 1.0));
        let out = outline_open(&hairpin, 2.0, FLAT).expect("outline");
        assert!(!out.is_empty());
        assert!(out.iter().flat_map(|p| &p.subpaths).all(|s| s.closed));
        let bbox = path_bbox(&out[0]).expect("bbox");
        assert!(
            (bbox.min.x + 2.0).abs() < 0.05 && (bbox.max.x - 22.0).abs() < 0.05,
            "{bbox:?}"
        );
        assert!(
            (bbox.min.y + 2.0).abs() < 0.05 && (bbox.max.y - 3.0).abs() < 0.05,
            "{bbox:?}"
        );
        let strips = outline_by_strips(&hairpin, 2.0, FLAT).expect("strips");
        assert_eq!(strips.len(), 1);
        assert!(strips[0].subpaths.iter().all(|s| s.closed));
    }

    #[test]
    fn outward_offset_of_a_rectangle_is_round_by_default() {
        let path = rect(Point::ORIGIN, Point::new(20.0, 10.0)).expect("rect");
        let out = offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Round, FLAT)
            .expect("offset");
        assert_eq!(out.len(), 1);
        let sub = &out[0].subpaths[0];
        assert_eq!(sub.segments.len(), 8);
        let arcs = sub
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Arc { .. }))
            .count();
        assert_eq!(arcs, 4);
        let area = path_area(&out[0], FLAT);
        let expected = 24.0 * 14.0 - 4.0 * 4.0 + std::f64::consts::PI * 4.0;
        assert!((area - expected).abs() < 1e-6, "area {area}");
    }

    #[test]
    fn outward_offset_with_sharp_corners_stays_four_lines() {
        let path = rect(Point::ORIGIN, Point::new(20.0, 10.0)).expect("rect");
        let out = offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Sharp, FLAT)
            .expect("offset");
        assert_eq!(out.len(), 1);
        let sub = &out[0].subpaths[0];
        assert_eq!(sub.segments.len(), 4);
        assert!(sub
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
        let area = path_area(&out[0], FLAT);
        assert!((area - 24.0 * 14.0).abs() < 1e-6, "area {area}");
        let bbox = path_bbox(&out[0]).expect("bbox");
        assert!((bbox.min.x + 2.0).abs() < 1e-9);
        assert!((bbox.max.y - 12.0).abs() < 1e-9);
    }

    #[test]
    fn inward_offset_of_a_rectangle_has_no_corner_arcs() {
        let path = rect(Point::ORIGIN, Point::new(20.0, 10.0)).expect("rect");
        let out =
            offset_path(&path, 2.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).expect("offset");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subpaths[0].segments.len(), 4);
        assert!((path_area(&out[0], FLAT) - 16.0 * 6.0).abs() < 1e-9);
    }

    #[test]
    fn inward_offset_that_collapses_gives_nothing() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 4.0)).expect("rect");
        let out =
            offset_path(&path, 3.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).expect("offset");
        assert!(out.is_empty());
    }

    #[test]
    fn offset_of_a_circle_changes_the_radius() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        let bigger = offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Round, FLAT)
            .expect("offset");
        let smaller =
            offset_path(&path, 2.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).expect("offset");
        assert!((path_area(&bigger[0], FLAT) - std::f64::consts::PI * 144.0).abs() < 1e-6);
        assert!((path_area(&smaller[0], FLAT) - std::f64::consts::PI * 64.0).abs() < 1e-6);
    }

    #[test]
    fn both_sides_returns_two_results() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        let out =
            offset_path(&path, 1.0, OffsetSide::Both, OffsetCorner::Round, FLAT).expect("offset");
        assert_eq!(out.len(), 2);
        assert!(path_area(&out[0], FLAT) < path_area(&out[1], FLAT));
    }

    #[test]
    fn a_hole_grows_when_the_material_shrinks() {
        let mut path = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
        let hole = crate::geom::edit::reverse(
            &rect(Point::new(10.0, 10.0), Point::new(30.0, 30.0)).expect("rect"),
        );
        path.extend(hole);
        let out =
            offset_path(&path, 2.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT).expect("offset");
        assert_eq!(out.len(), 2);
        let outer = path_area(&out[0], FLAT);
        let inner = path_area(&out[1], FLAT);
        assert!((outer - 36.0 * 36.0).abs() < 1e-6, "outer {outer}");
        assert!((inner + 24.0 * 24.0).abs() < 1e-6, "inner {inner}");
    }

    #[test]
    fn offset_rejects_a_bad_distance() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        assert!(offset_path(&path, 0.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).is_err());
        assert!(offset_path(&path, -1.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).is_err());
        assert!(parallel_offset_open(&path, 0.0, true, FLAT).is_err());
    }

    #[test]
    fn open_vector_offsets_to_a_closed_outline() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(20.0, 0.0));
        let out = offset_path(
            &sub.into_path(),
            2.0,
            OffsetSide::Outward,
            OffsetCorner::Round,
            FLAT,
        )
        .expect("offset");
        assert_eq!(out.len(), 1);
        assert!(out[0].subpaths[0].closed);
        let area = path_area(&out[0], FLAT);
        let expected = 20.0 * 4.0 + std::f64::consts::PI * 4.0;
        assert!((area - expected).abs() < 1e-6, "area {area}");
    }

    #[test]
    fn parallel_offset_of_an_open_line_stays_open() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(20.0, 0.0));
        let path = sub.into_path();
        let out = parallel_offset_open(&path, 3.0, true, FLAT).expect("offset");
        assert_eq!(out.len(), 1);
        assert!(!out[0].subpaths[0].closed);
        let bbox = path_bbox(&out[0]).expect("bbox");
        assert!((bbox.min.y - 3.0).abs() < 1e-9);
        let right = parallel_offset_open(&path, 3.0, false, FLAT).expect("offset");
        let rb = path_bbox(&right[0]).expect("bbox");
        assert!((rb.min.y + 3.0).abs() < 1e-9);
    }

    #[test]
    fn offset_of_a_path_with_cubics_works() {
        let path = crate::geom::construct::ellipse(
            Point::ORIGIN,
            20.0,
            10.0,
            crate::geom::construct::EllipseForm::Cubic,
        )
        .expect("ellipse");
        let out = offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Round, FLAT)
            .expect("offset");
        assert_eq!(out.len(), 1);
        let area = path_area(&out[0], FLAT);
        assert!(area > std::f64::consts::PI * 200.0);
    }

    #[test]
    fn sharpen_leaves_a_shape_without_corner_arcs_alone() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        let same = sharpen_corners(&path.subpaths[0], 2.0, FLAT);
        assert_eq!(same.segments.len(), 2);
    }

    #[test]
    #[ignore = "timing check, run with cargo test -- --ignored"]
    fn offsetting_a_thousand_rectangles_is_quick() {
        let start = std::time::Instant::now();
        for i in 0..1000 {
            let w = 10.0 + (i % 7) as f64;
            let path = rect(Point::ORIGIN, Point::new(w, 8.0)).expect("rect");
            let out = offset_path(&path, 1.0, OffsetSide::Outward, OffsetCorner::Round, FLAT)
                .expect("offset");
            assert_eq!(out.len(), 1);
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
    }
}
