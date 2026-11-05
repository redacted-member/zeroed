//! Reducing the number of points and nodes without moving the shape.

use super::path::{ArcGeom, Path, Polyline, Segment, SubPath};
use super::point::{Point, Tolerance};

/// Douglas Peucker simplification of a polyline.
///
/// Every dropped point was within `tol` millimetres of the line that replaces
/// it. The first and last points of an open polyline are always kept.
pub fn simplify_polyline(polyline: &Polyline, tol: Tolerance) -> Polyline {
    let pts = &polyline.points;
    if pts.len() < 3 {
        return polyline.clone();
    }
    if polyline.closed {
        // Cut the loop at the point furthest from the first one and simplify
        // the two open chains, so the result does not depend on where the
        // loop happens to start.
        let first = pts[0];
        let mut far = 0usize;
        let mut far_d = -1.0;
        for (i, p) in pts.iter().enumerate() {
            let d = first.distance_squared(*p);
            if d > far_d {
                far_d = d;
                far = i;
            }
        }
        if far == 0 {
            return polyline.clone();
        }
        let head = douglas_peucker(&pts[..=far], tol.get());
        let mut tail_input: Vec<Point> = pts[far..].to_vec();
        tail_input.push(first);
        let tail = douglas_peucker(&tail_input, tol.get());
        let mut points = head;
        let inner = tail.len().saturating_sub(1);
        points.extend(tail.into_iter().take(inner).skip(1));
        return Polyline::new(points, true);
    }
    Polyline::new(douglas_peucker(pts, tol.get()), false)
}

fn douglas_peucker(points: &[Point], tol: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        if last <= first + 1 {
            continue;
        }
        let a = points[first];
        let b = points[last];
        let chord = a.to(b);
        let len = chord.length();
        let mut worst = 0.0;
        let mut at = first;
        for (i, p) in points.iter().enumerate().take(last).skip(first + 1) {
            let d = if len <= f64::MIN_POSITIVE {
                a.distance(*p)
            } else {
                (chord.cross(a.to(*p)) / len).abs()
            };
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > tol && at > first {
            keep[at] = true;
            stack.push((first, at));
            stack.push((at, last));
        }
    }
    points
        .iter()
        .zip(keep.iter())
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

/// Merges neighbouring segments that can be described by one segment.
///
/// Straight segments that run in the same direction become one line, and
/// arcs on the same circle that run the same way become one arc. Nothing
/// moves by more than `tol`.
pub fn reduce_nodes(path: &Path, tol: Tolerance) -> Path {
    Path::from_subpaths(
        path.subpaths
            .iter()
            .map(|s| reduce_subpath(s, tol))
            .collect(),
    )
}

/// Merges neighbouring segments of one subpath.
pub fn reduce_subpath(subpath: &SubPath, tol: Tolerance) -> SubPath {
    if subpath.segments.len() < 2 {
        return subpath.clone();
    }
    let mut start = subpath.start;
    let mut out: Vec<Segment> = Vec::with_capacity(subpath.segments.len());
    let mut current_start = start;
    let mut current = subpath.segments[0];

    for seg in subpath.segments.iter().skip(1) {
        match merge(current_start, &current, &seg.clone(), tol) {
            Some(merged) => current = merged,
            None => {
                out.push(current);
                current_start = current.end();
                current = *seg;
            }
        }
    }
    out.push(current);

    // On a closed loop the last and first segments meet at the start node and
    // may also be mergeable.
    if subpath.closed && out.len() > 2 {
        let last = out[out.len() - 1];
        let last_start = if out.len() >= 2 {
            out[out.len() - 2].end()
        } else {
            start
        };
        if let Some(merged) = merge(last_start, &last, &out[0], tol) {
            // The merged segment runs from the start of the old last segment,
            // across the old start node, to the end of the old first one. The
            // loop therefore has to begin where the old first segment ended,
            // with the merged segment closing it.
            let first_end = out[0].end();
            out.remove(0);
            let n = out.len();
            out[n - 1] = merged;
            start = first_end;
        }
    }
    SubPath::new(start, out, subpath.closed)
}

fn merge(start: Point, a: &Segment, b: &Segment, tol: Tolerance) -> Option<Segment> {
    match (a, b) {
        (Segment::Line { to: mid }, Segment::Line { to }) => {
            let d1 = start.to(*mid).normalised()?;
            let d2 = mid.to(*to).normalised()?;
            let span = start.distance(*to).max(1.0);
            if d1.cross(d2).abs() * span <= tol.get() && d1.dot(d2) > 0.0 {
                Some(Segment::Line { to: *to })
            } else {
                None
            }
        }
        (Segment::Arc { to: mid, bulge: b1 }, Segment::Arc { to, bulge: b2 }) => {
            if b1.signum() != b2.signum() {
                return None;
            }
            let first = ArcGeom::from_bulge(start, *mid, *b1)?;
            let second = ArcGeom::from_bulge(*mid, *to, *b2)?;
            if !first.centre.approx_eq(second.centre, tol)
                || (first.radius - second.radius).abs() > tol.get()
            {
                return None;
            }
            let sweep = first.sweep + second.sweep;
            if sweep.abs() >= std::f64::consts::TAU - 1e-9 {
                return None;
            }
            Some(Segment::Arc {
                to: *to,
                bulge: super::path::bulge_from_sweep(sweep),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::flatten::flatten_subpath;
    use crate::geom::metrics::{path_area, path_length};

    const FLAT: Tolerance = Tolerance::FLATTEN;

    #[test]
    fn simplify_drops_points_on_a_straight_run() {
        let poly = Polyline::new((0..=10).map(|i| Point::new(i as f64, 0.0)).collect(), false);
        let out = simplify_polyline(&poly, FLAT);
        assert_eq!(out.points.len(), 2);
    }

    #[test]
    fn simplify_keeps_a_real_corner() {
        let poly = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(5.0, 5.0),
            ],
            false,
        );
        assert_eq!(simplify_polyline(&poly, FLAT).points.len(), 3);
    }

    #[test]
    fn simplify_leaves_short_polylines_alone() {
        let poly = Polyline::new(vec![Point::ORIGIN, Point::new(1.0, 0.0)], false);
        assert_eq!(simplify_polyline(&poly, FLAT).points.len(), 2);
    }

    #[test]
    fn simplify_of_a_dense_circle_keeps_the_shape() {
        let path = circle(Point::ORIGIN, 20.0).expect("circle");
        let dense = flatten_subpath(
            &path.subpaths[0],
            Tolerance::new(0.0005).expect("tolerance"),
        );
        let out = simplify_polyline(&dense, Tolerance::new(0.05).expect("tolerance"));
        assert!(out.closed);
        assert!(out.points.len() < dense.points.len());
        assert!(out.points.len() > 20);
        let area = out.signed_area();
        let expected = std::f64::consts::PI * 400.0;
        assert!((area - expected).abs() / expected < 0.01);
    }

    #[test]
    fn simplify_of_a_square_outline_keeps_four_corners() {
        let poly = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 5.0),
                Point::new(10.0, 10.0),
                Point::new(5.0, 10.0),
                Point::new(0.0, 10.0),
                Point::new(0.0, 5.0),
            ],
            true,
        );
        let out = simplify_polyline(&poly, FLAT);
        assert_eq!(out.points.len(), 4);
        assert!((out.signed_area() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn reduce_nodes_merges_colinear_lines() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(5.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 5.0));
        let reduced = reduce_subpath(&sub, FLAT);
        assert_eq!(reduced.segments.len(), 2);
        assert!((path_length(&reduced.into_path(), FLAT) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn reduce_nodes_merges_co_circular_arcs() {
        let path = circle(Point::ORIGIN, 8.0).expect("circle");
        let split = crate::geom::edit::insert_node(&path.subpaths[0], 0, 0.5).expect("insert");
        assert_eq!(split.segments.len(), 3);
        let reduced = reduce_subpath(&split, FLAT);
        assert_eq!(reduced.segments.len(), 2);
        let area = path_area(&reduced.into_path(), FLAT);
        assert!((area - std::f64::consts::PI * 64.0).abs() < 1e-6);
    }

    #[test]
    fn reduce_nodes_keeps_real_corners() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let reduced = reduce_nodes(&square, FLAT);
        assert_eq!(reduced.subpaths[0].segments.len(), 4);
        assert!((path_area(&reduced, FLAT) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn reduce_nodes_leaves_a_single_segment_alone() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert_eq!(reduce_subpath(&sub, FLAT).segments.len(), 1);
    }

    #[test]
    fn reduce_nodes_does_not_merge_opposite_arcs() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(2.0, 0.0), 1.0)
            .arc_to(Point::new(4.0, 0.0), -1.0);
        assert_eq!(reduce_subpath(&sub, FLAT).segments.len(), 2);
    }
}
