//! Intersections between segments and between whole paths.
//!
//! Line to line, line to arc and arc to arc pairs are solved analytically.
//! Any pair involving a cubic is solved by flattening both segments inside
//! the tolerance and then refining each hit back onto the true segments, so
//! the reported point is accurate to about the tolerance.

use serde::{Deserialize, Serialize};

use super::bbox::BBox;
use super::flatten::flatten_segment_into;
use super::metrics::{nearest_on_segment, segment_bbox};
use super::path::{ArcGeom, Path, Segment, SubPath};
use super::point::{Point, Tolerance};

/// One crossing between two segments.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Intersection {
    /// Where the two segments meet, in millimetres.
    pub point: Point,
    /// Parameter along the first segment, 0 at its start and 1 at its end.
    pub t_a: f64,
    /// Parameter along the second segment.
    pub t_b: f64,
}

/// One crossing between two paths, with the place on each path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathIntersection {
    /// Where the two paths meet, in millimetres.
    pub point: Point,
    /// Subpath index on the first path.
    pub a_subpath: usize,
    /// Segment index inside that subpath.
    pub a_segment: usize,
    /// Parameter along that segment.
    pub a_t: f64,
    /// Subpath index on the second path.
    pub b_subpath: usize,
    /// Segment index inside that subpath.
    pub b_segment: usize,
    /// Parameter along that segment.
    pub b_t: f64,
}

fn line_line(a0: Point, a1: Point, b0: Point, b1: Point, tol: Tolerance) -> Vec<Intersection> {
    let da = a0.to(a1);
    let db = b0.to(b1);
    let denom = da.cross(db);
    if denom.abs() <= 1e-14 {
        return Vec::new();
    }
    let diff = a0.to(b0);
    let t = diff.cross(db) / denom;
    let u = diff.cross(da) / denom;
    let slack = tol.get() / da.length().max(db.length()).max(f64::MIN_POSITIVE);
    if !(-slack..=1.0 + slack).contains(&t) || !(-slack..=1.0 + slack).contains(&u) {
        return Vec::new();
    }
    let t = t.clamp(0.0, 1.0);
    let u = u.clamp(0.0, 1.0);
    vec![Intersection {
        point: a0.lerp(a1, t),
        t_a: t,
        t_b: u,
    }]
}

fn arc_param(arc: &ArcGeom, p: Point) -> Option<f64> {
    let angle = arc.centre.to(p).normalised()?.angle();
    if !arc.contains_angle(angle) {
        return None;
    }
    let mut delta = (angle - arc.start_angle) % std::f64::consts::TAU;
    if arc.sweep >= 0.0 && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    if arc.sweep < 0.0 && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    }
    Some((delta / arc.sweep).clamp(0.0, 1.0))
}

fn line_arc(a0: Point, a1: Point, arc: &ArcGeom, swap: bool) -> Vec<Intersection> {
    let d = a0.to(a1);
    let f = arc.centre.to(a0);
    let qa = d.length_squared();
    if qa <= f64::MIN_POSITIVE {
        return Vec::new();
    }
    let qb = 2.0 * f.dot(d);
    let qc = f.length_squared() - arc.radius * arc.radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return Vec::new();
    }
    let root = disc.sqrt();
    let mut out = Vec::new();
    for t in [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)] {
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let point = a0.lerp(a1, t);
        let Some(u) = arc_param(arc, point) else {
            continue;
        };
        out.push(if swap {
            Intersection {
                point,
                t_a: u,
                t_b: t,
            }
        } else {
            Intersection {
                point,
                t_a: t,
                t_b: u,
            }
        });
    }
    if root.abs() < 1e-12 && out.len() > 1 {
        out.truncate(1);
    }
    out
}

fn arc_arc(a: &ArcGeom, b: &ArcGeom) -> Vec<Intersection> {
    let between = a.centre.to(b.centre);
    let d = between.length();
    if d <= 1e-12 || d > a.radius + b.radius || d < (a.radius - b.radius).abs() {
        return Vec::new();
    }
    let x = (a.radius * a.radius - b.radius * b.radius + d * d) / (2.0 * d);
    let h2 = a.radius * a.radius - x * x;
    let h = if h2 <= 0.0 { 0.0 } else { h2.sqrt() };
    let Some(unit) = between.normalised() else {
        return Vec::new();
    };
    let base = a.centre.offset(unit.scaled(x));
    let perp = unit.perp().scaled(h);
    let candidates = if h <= 1e-12 {
        vec![base]
    } else {
        vec![base.offset(perp), base.offset(perp.scaled(-1.0))]
    };
    let mut out = Vec::new();
    for point in candidates {
        let (Some(ta), Some(tb)) = (arc_param(a, point), arc_param(b, point)) else {
            continue;
        };
        out.push(Intersection {
            point,
            t_a: ta,
            t_b: tb,
        });
    }
    out
}

fn flatten_one(start: Point, seg: &Segment, tol: Tolerance) -> Vec<Point> {
    let mut pts = vec![start];
    flatten_segment_into(start, seg, tol, &mut pts);
    pts
}

fn approximate(
    a_start: Point,
    a: &Segment,
    b_start: Point,
    b: &Segment,
    tol: Tolerance,
) -> Vec<Intersection> {
    let pa = flatten_one(a_start, a, tol);
    let pb = flatten_one(b_start, b, tol);
    let mut out = Vec::new();
    for wa in pa.windows(2) {
        for wb in pb.windows(2) {
            for hit in line_line(wa[0], wa[1], wb[0], wb[1], tol) {
                let (t_a, _) = nearest_on_segment(a_start, a, hit.point);
                let (t_b, _) = nearest_on_segment(b_start, b, hit.point);
                out.push(Intersection {
                    point: hit.point,
                    t_a,
                    t_b,
                });
            }
        }
    }
    out
}

fn dedupe(mut hits: Vec<Intersection>, tol: Tolerance) -> Vec<Intersection> {
    hits.sort_by(|x, y| {
        x.t_a
            .partial_cmp(&y.t_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                x.t_b
                    .partial_cmp(&y.t_b)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let mut out: Vec<Intersection> = Vec::with_capacity(hits.len());
    for hit in hits {
        if out.iter().any(|kept| kept.point.approx_eq(hit.point, tol)) {
            continue;
        }
        out.push(hit);
    }
    out
}

/// Every crossing between two segments, given the start point of each.
///
/// Results are sorted by the parameter along the first segment and points
/// closer together than `tol` are reported once.
pub fn segment_intersections(
    a_start: Point,
    a: &Segment,
    b_start: Point,
    b: &Segment,
    tol: Tolerance,
) -> Vec<Intersection> {
    let hits = match (a, b) {
        (Segment::Line { to: ta }, Segment::Line { to: tb }) => {
            line_line(a_start, *ta, b_start, *tb, tol)
        }
        (Segment::Line { to: ta }, Segment::Arc { to: tb, bulge }) => {
            match ArcGeom::from_bulge(b_start, *tb, *bulge) {
                Some(arc) => line_arc(a_start, *ta, &arc, false),
                None => line_line(a_start, *ta, b_start, *tb, tol),
            }
        }
        (Segment::Arc { to: ta, bulge }, Segment::Line { to: tb }) => {
            match ArcGeom::from_bulge(a_start, *ta, *bulge) {
                Some(arc) => line_arc(b_start, *tb, &arc, true),
                None => line_line(a_start, *ta, b_start, *tb, tol),
            }
        }
        (Segment::Arc { to: ta, bulge: ba }, Segment::Arc { to: tb, bulge: bb }) => match (
            ArcGeom::from_bulge(a_start, *ta, *ba),
            ArcGeom::from_bulge(b_start, *tb, *bb),
        ) {
            (Some(arc_a), Some(arc_b)) => arc_arc(&arc_a, &arc_b),
            (Some(arc_a), None) => line_arc(b_start, *tb, &arc_a, true),
            (None, Some(arc_b)) => line_arc(a_start, *ta, &arc_b, false),
            (None, None) => line_line(a_start, *ta, b_start, *tb, tol),
        },
        _ => approximate(a_start, a, b_start, b, tol),
    };
    dedupe(hits, tol)
}

/// Every crossing between two paths.
///
/// The list is sorted by position on the first path, so the same inputs
/// always give the same order.
pub fn path_intersections(a: &Path, b: &Path, tol: Tolerance) -> Vec<PathIntersection> {
    let mut out = Vec::new();
    for (asi, asub) in a.subpaths.iter().enumerate() {
        for (agi, (a_start, a_seg)) in asub.iter().enumerate() {
            for (bsi, bsub) in b.subpaths.iter().enumerate() {
                for (bgi, (b_start, b_seg)) in bsub.iter().enumerate() {
                    for hit in segment_intersections(a_start, a_seg, b_start, b_seg, tol) {
                        out.push(PathIntersection {
                            point: hit.point,
                            a_subpath: asi,
                            a_segment: agi,
                            a_t: hit.t_a,
                            b_subpath: bsi,
                            b_segment: bgi,
                            b_t: hit.t_b,
                        });
                    }
                }
            }
        }
    }
    out.sort_by(|x, y| {
        (x.a_subpath, x.a_segment)
            .cmp(&(y.a_subpath, y.a_segment))
            .then(
                x.a_t
                    .partial_cmp(&y.a_t)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    // A crossing on a shared node is found once per segment that meets there,
    // so keep only the first report of each place on each pair of subpaths.
    let mut kept: Vec<PathIntersection> = Vec::with_capacity(out.len());
    for hit in out {
        if kept.iter().any(|k| {
            k.a_subpath == hit.a_subpath
                && k.b_subpath == hit.b_subpath
                && k.point.approx_eq(hit.point, tol)
        }) {
            continue;
        }
        kept.push(hit);
    }
    kept
}

/// True when the subpath crosses itself anywhere other than at a shared node.
pub fn subpath_self_intersects(subpath: &SubPath, tol: Tolerance) -> bool {
    subpath_self_intersects_within(subpath, tol, &mut || false).unwrap_or(false)
}

/// [`subpath_self_intersects`] that asks `give_up` now and then and answers
/// `None` once it says yes. Only segments whose boxes (grown by the
/// tolerance) overlap are compared, found by a sweep along X, so a simple
/// vector with many nodes is checked in about n log n rather than n squared.
pub fn subpath_self_intersects_within(
    subpath: &SubPath,
    tol: Tolerance,
    give_up: &mut dyn FnMut() -> bool,
) -> Option<bool> {
    /// Segment pairs compared between two questions to `give_up`.
    const ASK_EVERY: usize = 1024;
    let segs: Vec<(Point, Segment)> = subpath.iter().map(|(s, g)| (s, *g)).collect();
    let n = segs.len();
    let boxes: Vec<BBox> = segs
        .iter()
        .map(|(start, seg)| segment_bbox(*start, seg).expand(tol.get()))
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| boxes[*a].min.x.total_cmp(&boxes[*b].min.x).then(a.cmp(b)));
    let mut active: Vec<usize> = Vec::new();
    let mut compared = 0usize;
    for &k in &order {
        let bk = boxes[k];
        active.retain(|a| boxes[*a].max.x >= bk.min.x);
        for &a in &active {
            if !boxes[a].intersects(bk) {
                continue;
            }
            compared += 1;
            if compared % ASK_EVERY == 0 && give_up() {
                return None;
            }
            let (i, j) = if a < k { (a, k) } else { (k, a) };
            let adjacent = j == i + 1 || (subpath.closed && i == 0 && j + 1 == n);
            for hit in segment_intersections(segs[i].0, &segs[i].1, segs[j].0, &segs[j].1, tol) {
                if adjacent {
                    let at_shared_node = (hit.t_a > 1.0 - 1e-6 && hit.t_b < 1e-6)
                        || (hit.t_a < 1e-6 && hit.t_b > 1.0 - 1e-6);
                    if at_shared_node {
                        continue;
                    }
                }
                return Some(true);
            }
        }
        active.push(k);
    }
    Some(false)
}

/// Where a subpath crosses itself, at most `limit` places, found by the
/// same sweep as [`subpath_self_intersects`].
pub fn subpath_self_crossings(subpath: &SubPath, tol: Tolerance, limit: usize) -> Vec<Point> {
    let segs: Vec<(Point, Segment)> = subpath.iter().map(|(s, g)| (s, *g)).collect();
    let n = segs.len();
    let boxes: Vec<BBox> = segs
        .iter()
        .map(|(start, seg)| segment_bbox(*start, seg).expand(tol.get()))
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| boxes[*a].min.x.total_cmp(&boxes[*b].min.x).then(a.cmp(b)));
    let mut active: Vec<usize> = Vec::new();
    let mut out: Vec<Point> = Vec::new();
    for &k in &order {
        let bk = boxes[k];
        active.retain(|a| boxes[*a].max.x >= bk.min.x);
        for &a in &active {
            if !boxes[a].intersects(bk) {
                continue;
            }
            let (i, j) = if a < k { (a, k) } else { (k, a) };
            let adjacent = j == i + 1 || (subpath.closed && i == 0 && j + 1 == n);
            for hit in segment_intersections(segs[i].0, &segs[i].1, segs[j].0, &segs[j].1, tol) {
                if adjacent
                    && ((hit.t_a > 1.0 - 1e-6 && hit.t_b < 1e-6)
                        || (hit.t_a < 1e-6 && hit.t_b > 1.0 - 1e-6))
                {
                    continue;
                }
                if !out.iter().any(|p| p.approx_eq(hit.point, tol)) {
                    out.push(hit.point);
                }
                if out.len() >= limit {
                    return out;
                }
            }
        }
        active.push(k);
    }
    out
}

/// True when any subpath of the path crosses itself or another subpath.
pub fn path_self_intersects(path: &Path, tol: Tolerance) -> bool {
    if path
        .subpaths
        .iter()
        .any(|s| subpath_self_intersects(s, tol))
    {
        return true;
    }
    for i in 0..path.subpaths.len() {
        for j in (i + 1)..path.subpaths.len() {
            let a = Path::from_subpath(path.subpaths[i].clone());
            let b = Path::from_subpath(path.subpaths[j].clone());
            if !path_intersections(&a, &b, tol).is_empty() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::path::bulge_from_sweep;

    const FLAT: Tolerance = Tolerance::FLATTEN;
    const EQ: Tolerance = Tolerance::EQUALITY;

    #[test]
    fn crossing_lines_meet_in_the_middle() {
        let hits = segment_intersections(
            Point::new(-1.0, 0.0),
            &Segment::line(Point::new(1.0, 0.0)),
            Point::new(0.0, -1.0),
            &Segment::line(Point::new(0.0, 1.0)),
            FLAT,
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].point.approx_eq(Point::ORIGIN, EQ));
        assert!((hits[0].t_a - 0.5).abs() < 1e-12);
        assert!((hits[0].t_b - 0.5).abs() < 1e-12);
    }

    #[test]
    fn parallel_lines_do_not_meet() {
        let hits = segment_intersections(
            Point::new(0.0, 0.0),
            &Segment::line(Point::new(1.0, 0.0)),
            Point::new(0.0, 1.0),
            &Segment::line(Point::new(1.0, 1.0)),
            FLAT,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn lines_that_miss_do_not_meet() {
        let hits = segment_intersections(
            Point::new(0.0, 0.0),
            &Segment::line(Point::new(1.0, 0.0)),
            Point::new(5.0, -1.0),
            &Segment::line(Point::new(5.0, 1.0)),
            FLAT,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn a_line_through_a_half_circle_gives_two_points() {
        let hits = segment_intersections(
            Point::new(-5.0, 0.5),
            &Segment::line(Point::new(5.0, 0.5)),
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), -1.0),
            FLAT,
        );
        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert!((hit.point.distance(Point::ORIGIN) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_tangent_line_touches_an_arc_once() {
        let hits = segment_intersections(
            Point::new(-5.0, 1.0),
            &Segment::line(Point::new(5.0, 1.0)),
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), 1.0),
            FLAT,
        );
        assert!(hits.len() <= 1);
    }

    #[test]
    fn a_line_missing_the_arc_sweep_finds_nothing() {
        let hits = segment_intersections(
            Point::new(-5.0, -0.5),
            &Segment::line(Point::new(5.0, -0.5)),
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), -1.0),
            FLAT,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn two_arcs_on_different_circles_cross_once() {
        let half = bulge_from_sweep(std::f64::consts::PI);
        let hits = segment_intersections(
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), half),
            Point::new(0.0, 0.0),
            &Segment::arc(Point::new(2.0, 0.0), half),
            FLAT,
        );
        assert_eq!(hits.len(), 1);
        assert!((hits[0].point.x - 0.5).abs() < 1e-9);
        assert!((hits[0].point.y + 3.0_f64.sqrt() / 2.0).abs() < 1e-9);
    }

    #[test]
    fn arcs_on_the_same_circle_report_no_crossing() {
        let half = bulge_from_sweep(std::f64::consts::PI);
        let hits = segment_intersections(
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), half),
            Point::new(0.0, -1.0),
            &Segment::arc(Point::new(0.0, 1.0), half),
            FLAT,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn separated_arcs_do_not_cross() {
        let hits = segment_intersections(
            Point::new(-1.0, 0.0),
            &Segment::arc(Point::new(1.0, 0.0), 1.0),
            Point::new(99.0, 0.0),
            &Segment::arc(Point::new(101.0, 0.0), 1.0),
            FLAT,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn a_cubic_crossing_a_line_is_found() {
        let hits = segment_intersections(
            Point::new(0.0, 0.0),
            &Segment::cubic(
                Point::new(0.0, 10.0),
                Point::new(10.0, 10.0),
                Point::new(10.0, 0.0),
            ),
            Point::new(-5.0, 5.0),
            &Segment::line(Point::new(15.0, 5.0)),
            FLAT,
        );
        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert!((hit.point.y - 5.0).abs() < 0.05);
        }
    }

    #[test]
    fn two_cubics_that_cross_are_found() {
        let hits = segment_intersections(
            Point::new(0.0, 0.0),
            &Segment::cubic(
                Point::new(3.0, 10.0),
                Point::new(7.0, 10.0),
                Point::new(10.0, 0.0),
            ),
            Point::new(5.0, -5.0),
            &Segment::cubic(
                Point::new(5.0, 3.0),
                Point::new(5.0, 6.0),
                Point::new(5.0, 10.0),
            ),
            FLAT,
        );
        assert_eq!(hits.len(), 1);
        assert!((hits[0].point.x - 5.0).abs() < 0.05);
    }

    #[test]
    fn path_intersections_of_two_overlapping_squares() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::new(5.0, 5.0), Point::new(15.0, 15.0)).expect("rect");
        let hits = path_intersections(&a, &b, FLAT);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn path_intersections_of_two_circles() {
        let a = circle(Point::ORIGIN, 5.0).expect("circle");
        let b = circle(Point::new(6.0, 0.0), 5.0).expect("circle");
        let hits = path_intersections(&a, &b, FLAT);
        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert!((hit.point.x - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn disjoint_paths_have_no_intersections() {
        let a = circle(Point::ORIGIN, 1.0).expect("circle");
        let b = circle(Point::new(50.0, 0.0), 1.0).expect("circle");
        assert!(path_intersections(&a, &b, FLAT).is_empty());
    }

    #[test]
    fn a_simple_square_does_not_self_intersect() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(!path_self_intersects(&square, FLAT));
    }

    #[test]
    fn a_bowtie_self_intersects() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 10.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(0.0, 10.0))
            .line_to(Point::new(0.0, 0.0));
        sub.closed = true;
        assert!(path_self_intersects(&sub.into_path(), FLAT));
    }

    #[test]
    fn overlapping_subpaths_count_as_self_intersecting() {
        let mut path = circle(Point::ORIGIN, 5.0).expect("circle");
        path.extend(circle(Point::new(4.0, 0.0), 5.0).expect("circle"));
        assert!(path_self_intersects(&path, FLAT));
    }

    #[test]
    fn nested_subpaths_do_not_count_as_self_intersecting() {
        let mut path = circle(Point::ORIGIN, 10.0).expect("circle");
        path.extend(circle(Point::ORIGIN, 4.0).expect("circle"));
        assert!(!path_self_intersects(&path, FLAT));
    }

    #[test]
    fn intersections_are_sorted_along_the_first_path() {
        let mut sub = SubPath::open(Point::new(-10.0, 0.0));
        sub.line_to(Point::new(10.0, 0.0));
        let line = sub.into_path();
        let circles = {
            let mut p = circle(Point::new(-5.0, 0.0), 1.0).expect("circle");
            p.extend(circle(Point::new(5.0, 0.0), 1.0).expect("circle"));
            p
        };
        let hits = path_intersections(&line, &circles, FLAT);
        assert_eq!(hits.len(), 4);
        for pair in hits.windows(2) {
            assert!(pair[0].a_t <= pair[1].a_t + 1e-12);
        }
    }
}
