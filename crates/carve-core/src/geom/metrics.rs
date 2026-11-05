//! Measuring paths: length, area, centroid, bounding box, winding, the point
//! at an arc length fraction and the nearest point on a path.

use serde::{Deserialize, Serialize};

use super::bbox::BBox;
use super::flatten::{cubic_derivative_at, cubic_point_at, flatten_subpath};
use super::path::{ArcGeom, Path, Segment, SubPath, Winding};
use super::point::{Point, Tolerance, Vec2};

/// Where a path comes closest to a query point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NearestPoint {
    /// Distance from the query point, in millimetres.
    pub distance: f64,
    /// The closest point on the path, in millimetres.
    pub point: Point,
    /// Index of the subpath the point lies on.
    pub subpath_index: usize,
    /// Index of the segment inside that subpath.
    pub segment_index: usize,
    /// Parameter along that segment, 0 at its start and 1 at its end.
    pub t: f64,
}

/// Length of one segment, in millimetres.
///
/// Lines and arcs are exact. Cubics are measured by adaptive subdivision
/// until the control polygon and the chord agree inside `tol`.
pub fn segment_length(start: Point, segment: &Segment, tol: Tolerance) -> f64 {
    match *segment {
        Segment::Line { to } => start.distance(to),
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => arc.length(),
            None => start.distance(to),
        },
        Segment::Cubic { c1, c2, to } => cubic_length(start, c1, c2, to, tol.get(), 0),
    }
}

fn cubic_length(p0: Point, c1: Point, c2: Point, p3: Point, tol: f64, depth: u32) -> f64 {
    const MAX_DEPTH: u32 = 22;
    let chord = p0.distance(p3);
    let control = p0.distance(c1) + c1.distance(c2) + c2.distance(p3);
    if depth >= MAX_DEPTH || control - chord <= tol {
        return (control + chord) * 0.5;
    }
    let p01 = p0.midpoint(c1);
    let p12 = c1.midpoint(c2);
    let p23 = c2.midpoint(p3);
    let p012 = p01.midpoint(p12);
    let p123 = p12.midpoint(p23);
    let mid = p012.midpoint(p123);
    cubic_length(p0, p01, p012, mid, tol, depth + 1)
        + cubic_length(mid, p123, p23, p3, tol, depth + 1)
}

/// Length of one subpath, in millimetres.
pub fn subpath_length(subpath: &SubPath, tol: Tolerance) -> f64 {
    subpath
        .iter()
        .map(|(start, seg)| segment_length(start, seg, tol))
        .sum()
}

/// Total length of every subpath, in millimetres.
pub fn path_length(path: &Path, tol: Tolerance) -> f64 {
    path.subpaths.iter().map(|s| subpath_length(s, tol)).sum()
}

/// Three point Gauss Legendre nodes and weights on the interval 0 to 1.
const GAUSS: [(f64, f64); 3] = [
    (0.112_701_665_379_258_3, 0.277_777_777_777_777_8),
    (0.5, 0.444_444_444_444_444_4),
    (0.887_298_334_620_741_7, 0.277_777_777_777_777_8),
];

/// Twice the signed area swept by one segment about the origin.
fn segment_double_area(start: Point, segment: &Segment) -> f64 {
    match *segment {
        Segment::Line { to } => start.x * to.y - to.x * start.y,
        Segment::Arc { to, bulge } => {
            let chord = start.x * to.y - to.x * start.y;
            match ArcGeom::from_bulge(start, to, bulge) {
                Some(arc) => {
                    let r2 = arc.radius * arc.radius;
                    chord + r2 * (arc.sweep - arc.sweep.sin())
                }
                None => chord,
            }
        }
        Segment::Cubic { c1, c2, to } => {
            // The integrand x y' - y x' is a degree five polynomial, so three
            // Gauss Legendre nodes integrate it exactly.
            let mut sum = 0.0;
            for (t, w) in GAUSS {
                let p = cubic_point_at(start, c1, c2, to, t);
                let d = cubic_derivative_at(start, c1, c2, to, t);
                sum += w * (p.x * d.y - p.y * d.x);
            }
            sum
        }
    }
}

/// Signed area of a closed subpath in square millimetres, positive counter
/// clockwise. An open subpath has zero area.
pub fn subpath_signed_area(subpath: &SubPath) -> f64 {
    if !subpath.closed {
        return 0.0;
    }
    let mut double = 0.0;
    for (start, seg) in subpath.iter() {
        double += segment_double_area(start, seg);
    }
    // Close the loop if the last segment does not land on the start point.
    let end = subpath.end();
    if !end.approx_eq(subpath.start, Tolerance::EQUALITY) {
        double += end.x * subpath.start.y - subpath.start.x * end.y;
    }
    double * 0.5
}

/// Signed area of every closed subpath, in square millimetres.
///
/// Holes wound clockwise subtract from outer loops wound counter clockwise,
/// so a shape with a hole reports the material area. `tol` is unused for
/// lines and arcs and only bounds the cubic quadrature.
pub fn path_area(path: &Path, tol: Tolerance) -> f64 {
    let _ = tol;
    path.subpaths.iter().map(subpath_signed_area).sum()
}

/// Which way round the path runs overall.
pub fn path_winding(path: &Path, tol: Tolerance) -> Winding {
    Winding::of_area(path_area(path, tol), Tolerance::EQUALITY)
}

/// True when the path has at least one subpath and all of them are closed.
pub fn path_is_closed(path: &Path) -> bool {
    path.is_closed()
}

/// Area centroid of the closed subpaths, in millimetres.
///
/// Computed from the flattened outline, so the result is accurate to about
/// the flattening tolerance. Returns `None` when the path encloses no area.
pub fn path_centroid(path: &Path, tol: Tolerance) -> Option<Point> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut area2 = 0.0;
    for sub in &path.subpaths {
        if !sub.closed {
            continue;
        }
        let poly = flatten_subpath(sub, tol);
        let n = poly.points.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let a = poly.points[i];
            let b = poly.points[(i + 1) % n];
            let cross = a.x * b.y - b.x * a.y;
            area2 += cross;
            cx += (a.x + b.x) * cross;
            cy += (a.y + b.y) * cross;
        }
    }
    if area2.abs() < 1e-12 {
        return None;
    }
    Some(Point::new(cx / (3.0 * area2), cy / (3.0 * area2)))
}

/// Tight bounding box of one segment, including curve extremes.
pub fn segment_bbox(start: Point, segment: &Segment) -> BBox {
    match *segment {
        Segment::Line { to } => BBox::new(start, to),
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => arc.bbox(),
            None => BBox::new(start, to),
        },
        Segment::Cubic { c1, c2, to } => cubic_bbox(start, c1, c2, to),
    }
}

fn cubic_bbox(p0: Point, c1: Point, c2: Point, p3: Point) -> BBox {
    let mut bbox = BBox::new(p0, p3);
    for axis in 0..2 {
        let (a0, a1, a2, a3) = match axis {
            0 => (p0.x, c1.x, c2.x, p3.x),
            _ => (p0.y, c1.y, c2.y, p3.y),
        };
        // Derivative of the cubic is a quadratic in t.
        let a = 3.0 * (-a0 + 3.0 * a1 - 3.0 * a2 + a3);
        let b = 6.0 * (a0 - 2.0 * a1 + a2);
        let c = 3.0 * (a1 - a0);
        for t in quadratic_roots(a, b, c) {
            if t > 0.0 && t < 1.0 {
                bbox = bbox.with_point(cubic_point_at(p0, c1, c2, p3, t));
            }
        }
    }
    bbox
}

fn quadratic_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return Vec::new();
        }
        return vec![-c / b];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return Vec::new();
    }
    let root = disc.sqrt();
    vec![(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)]
}

/// Tight bounding box of one subpath, or `None` when it has no points.
pub fn subpath_bbox(subpath: &SubPath) -> Option<BBox> {
    let mut bbox = BBox::from_point(subpath.start);
    for (start, seg) in subpath.iter() {
        bbox = bbox.union(segment_bbox(start, seg));
    }
    Some(bbox)
}

/// Tight bounding box of the whole path, or `None` when it has no subpaths.
pub fn path_bbox(path: &Path) -> Option<BBox> {
    let mut result: Option<BBox> = None;
    for sub in &path.subpaths {
        if let Some(b) = subpath_bbox(sub) {
            result = Some(match result {
                Some(existing) => existing.union(b),
                None => b,
            });
        }
    }
    result
}

/// Point on a segment at parameter `t`, 0 at the start and 1 at the end.
///
/// For lines and arcs the parameter is proportional to arc length. For
/// cubics it is the Bezier parameter.
pub fn segment_point_at(start: Point, segment: &Segment, t: f64) -> Point {
    match *segment {
        Segment::Line { to } => start.lerp(to, t),
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => arc.point_at(t),
            None => start.lerp(to, t),
        },
        Segment::Cubic { c1, c2, to } => cubic_point_at(start, c1, c2, to, t),
    }
}

/// Unit tangent of a segment at parameter `t`, pointing along travel.
pub fn segment_tangent_at(start: Point, segment: &Segment, t: f64) -> Vec2 {
    let fallback = Vec2::new(1.0, 0.0);
    match *segment {
        Segment::Line { to } => start.to(to).normalised().unwrap_or(fallback),
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => arc.tangent_at(t),
            None => start.to(to).normalised().unwrap_or(fallback),
        },
        Segment::Cubic { c1, c2, to } => {
            let d = cubic_derivative_at(start, c1, c2, to, t);
            d.normalised().unwrap_or_else(|| {
                let eps = if t < 0.5 { 1e-6 } else { -1e-6 };
                cubic_derivative_at(start, c1, c2, to, t + eps)
                    .normalised()
                    .unwrap_or(fallback)
            })
        }
    }
}

/// Finds the segment holding the arc length fraction `t` of the whole path.
fn locate(path: &Path, t: f64, tol: Tolerance) -> Option<(usize, usize, f64)> {
    let total = path_length(path, tol);
    if total <= 0.0 {
        let sub = path.subpaths.first()?;
        if sub.segments.is_empty() {
            return None;
        }
        return Some((0, 0, 0.0));
    }
    let target = total * t.clamp(0.0, 1.0);
    let mut walked = 0.0;
    for (si, sub) in path.subpaths.iter().enumerate() {
        for (gi, (start, seg)) in sub.iter().enumerate() {
            let len = segment_length(start, seg, tol);
            if walked + len >= target || (si + 1 == path.subpaths.len() && gi + 1 == sub.len()) {
                let local = if len > 0.0 {
                    ((target - walked) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let local = match seg {
                    Segment::Cubic { c1, c2, to } => {
                        cubic_parameter_at_length(start, *c1, *c2, *to, local * len, tol)
                    }
                    _ => local,
                };
                return Some((si, gi, local));
            }
            walked += len;
        }
    }
    None
}

/// Bezier parameter reaching a given arc length along a cubic.
fn cubic_parameter_at_length(
    p0: Point,
    c1: Point,
    c2: Point,
    p3: Point,
    target: f64,
    tol: Tolerance,
) -> f64 {
    const STEPS: usize = 64;
    if target <= 0.0 {
        return 0.0;
    }
    let mut walked = 0.0;
    let mut prev = p0;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let p = cubic_point_at(p0, c1, c2, p3, t);
        let step = prev.distance(p);
        if walked + step >= target {
            let frac = if step > 0.0 {
                (target - walked) / step
            } else {
                0.0
            };
            return ((i - 1) as f64 + frac) / STEPS as f64;
        }
        walked += step;
        prev = p;
    }
    let _ = tol;
    1.0
}

/// Point at the arc length fraction `t` of the whole path, `t` from 0 to 1.
pub fn path_point_at(path: &Path, t: f64, tol: Tolerance) -> Option<Point> {
    let (si, gi, local) = locate(path, t, tol)?;
    let sub = path.subpaths.get(si)?;
    let start = sub.segment_start(gi)?;
    let seg = sub.segments.get(gi)?;
    Some(segment_point_at(start, seg, local))
}

/// Unit tangent at the arc length fraction `t` of the whole path.
pub fn path_tangent_at(path: &Path, t: f64, tol: Tolerance) -> Option<Vec2> {
    let (si, gi, local) = locate(path, t, tol)?;
    let sub = path.subpaths.get(si)?;
    let start = sub.segment_start(gi)?;
    let seg = sub.segments.get(gi)?;
    Some(segment_tangent_at(start, seg, local))
}

/// Closest point of one segment to `query`, as (parameter, point).
pub fn nearest_on_segment(start: Point, segment: &Segment, query: Point) -> (f64, Point) {
    match *segment {
        Segment::Line { to } => {
            let dir = start.to(to);
            let len2 = dir.length_squared();
            if len2 <= f64::MIN_POSITIVE {
                return (0.0, start);
            }
            let t = (start.to(query).dot(dir) / len2).clamp(0.0, 1.0);
            (t, start.lerp(to, t))
        }
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => {
                let dir = arc.centre.to(query);
                match dir.normalised() {
                    Some(unit) => {
                        let angle = unit.angle();
                        if arc.contains_angle(angle) {
                            let mut delta = (angle - arc.start_angle) % std::f64::consts::TAU;
                            if arc.sweep >= 0.0 && delta < 0.0 {
                                delta += std::f64::consts::TAU;
                            }
                            if arc.sweep < 0.0 && delta > 0.0 {
                                delta -= std::f64::consts::TAU;
                            }
                            let t = (delta / arc.sweep).clamp(0.0, 1.0);
                            (t, arc.point_at(t))
                        } else if query.distance(start) <= query.distance(to) {
                            (0.0, start)
                        } else {
                            (1.0, to)
                        }
                    }
                    None => (0.0, start),
                }
            }
            None => nearest_on_segment(start, &Segment::Line { to }, query),
        },
        Segment::Cubic { c1, c2, to } => {
            const SAMPLES: usize = 32;
            let mut best_t = 0.0;
            let mut best_d = f64::INFINITY;
            for i in 0..=SAMPLES {
                let t = i as f64 / SAMPLES as f64;
                let p = cubic_point_at(start, c1, c2, to, t);
                let d = p.distance_squared(query);
                if d < best_d {
                    best_d = d;
                    best_t = t;
                }
            }
            let mut t = best_t;
            for _ in 0..8 {
                let p = cubic_point_at(start, c1, c2, to, t);
                let d1 = cubic_derivative_at(start, c1, c2, to, t);
                let diff = query.to(p);
                let denom = d1.length_squared();
                if denom <= f64::MIN_POSITIVE {
                    break;
                }
                let step = diff.dot(d1) / denom;
                let next = (t - step).clamp(0.0, 1.0);
                if (next - t).abs() < 1e-12 {
                    t = next;
                    break;
                }
                t = next;
            }
            (t, cubic_point_at(start, c1, c2, to, t))
        }
    }
}

/// Closest point of the whole path to `query`.
///
/// Returns `None` when the path has no segments.
pub fn nearest_point_on_path(path: &Path, query: Point, tol: Tolerance) -> Option<NearestPoint> {
    let _ = tol;
    let mut best: Option<NearestPoint> = None;
    for (si, sub) in path.subpaths.iter().enumerate() {
        for (gi, (start, seg)) in sub.iter().enumerate() {
            let (t, point) = nearest_on_segment(start, seg, query);
            let distance = point.distance(query);
            if best.as_ref().map(|b| distance < b.distance) != Some(false) {
                best = Some(NearestPoint {
                    distance,
                    point,
                    subpath_index: si,
                    segment_index: gi,
                    t,
                });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, ellipse, rect, EllipseForm};

    const FLAT: Tolerance = Tolerance::FLATTEN;
    const EQ: Tolerance = Tolerance::EQUALITY;

    fn fine() -> Tolerance {
        Tolerance::new(1e-5).expect("tolerance")
    }

    #[test]
    fn line_length_is_exact() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(3.0, 4.0));
        assert!((subpath_length(&sub, FLAT) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn arc_length_is_exact() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(2.0, 0.0), 1.0);
        assert!((subpath_length(&sub, FLAT) - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn cubic_length_converges_on_a_straight_control_polygon() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(3.0, 0.0),
            Point::new(6.0, 0.0),
            Point::new(9.0, 0.0),
        );
        assert!((subpath_length(&sub, fine()) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn cubic_length_matches_a_fine_flattening() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let exact = subpath_length(&sub, fine());
        let flat = flatten_subpath(&sub, Tolerance::new(1e-6).expect("tolerance")).length();
        assert!((exact - flat).abs() < 1e-3, "{exact} vs {flat}");
    }

    #[test]
    fn zero_length_segment_measures_zero() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::ORIGIN);
        assert_eq!(subpath_length(&sub, FLAT), 0.0);
    }

    #[test]
    fn square_area_is_positive_counter_clockwise() {
        let path = rect(Point::ORIGIN, Point::new(4.0, 3.0)).expect("rect");
        assert!((path_area(&path, FLAT) - 12.0).abs() < 1e-12);
        assert_eq!(path_winding(&path, FLAT), Winding::Ccw);
    }

    #[test]
    fn circle_area_from_arcs_is_exact() {
        let path = circle(Point::new(1.0, 2.0), 3.0).expect("circle");
        let expected = std::f64::consts::PI * 9.0;
        assert!((path_area(&path, FLAT) - expected).abs() < 1e-9);
    }

    #[test]
    fn cubic_area_quadrature_is_exact_for_an_ellipse() {
        let path = ellipse(Point::ORIGIN, 6.0, 3.0, EllipseForm::Cubic).expect("ellipse");
        let approx = std::f64::consts::PI * 18.0;
        let area = path_area(&path, FLAT);
        assert!((area - approx).abs() / approx < 0.003, "area {area}");
    }

    #[test]
    fn open_paths_have_no_area() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(1.0, 1.0));
        assert_eq!(path_area(&sub.into_path(), FLAT), 0.0);
    }

    #[test]
    fn a_hole_subtracts_from_the_outer_loop() {
        let mut path = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let mut hole = rect(Point::new(2.0, 2.0), Point::new(4.0, 4.0)).expect("rect");
        hole.subpaths[0] = crate::geom::edit::reverse_subpath(&hole.subpaths[0]);
        path.extend(hole);
        assert!((path_area(&path, FLAT) - 96.0).abs() < 1e-12);
        assert!(path_is_closed(&path));
    }

    #[test]
    fn centroid_of_a_square_is_its_centre() {
        let path = rect(Point::new(0.0, 0.0), Point::new(4.0, 6.0)).expect("rect");
        let c = path_centroid(&path, FLAT).expect("centroid");
        assert!(c.approx_eq(
            Point::new(2.0, 3.0),
            Tolerance::new(1e-9).expect("tolerance")
        ));
    }

    #[test]
    fn centroid_of_an_open_path_is_none() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert!(path_centroid(&sub.into_path(), FLAT).is_none());
    }

    #[test]
    fn bbox_of_a_circle_is_the_full_square() {
        let path = circle(Point::new(2.0, -1.0), 5.0).expect("circle");
        let b = path_bbox(&path).expect("bbox");
        assert!((b.min.x + 3.0).abs() < 1e-12);
        assert!((b.max.x - 7.0).abs() < 1e-12);
        assert!((b.min.y + 6.0).abs() < 1e-12);
        assert!((b.max.y - 4.0).abs() < 1e-12);
    }

    #[test]
    fn bbox_of_a_cubic_includes_the_curve_extreme() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let b = subpath_bbox(&sub).expect("bbox");
        assert!(b.max.y > 7.0 && b.max.y < 7.6, "top {}", b.max.y);
        assert!((b.min.y).abs() < 1e-12);
    }

    #[test]
    fn bbox_of_an_empty_path_is_none() {
        assert!(path_bbox(&Path::new()).is_none());
    }

    #[test]
    fn point_at_walks_the_arc_length() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let start = path_point_at(&path, 0.0, FLAT).expect("point");
        assert!(start.approx_eq(Point::ORIGIN, EQ));
        let quarter = path_point_at(&path, 0.25, FLAT).expect("point");
        assert!(quarter.approx_eq(Point::new(10.0, 0.0), Tolerance::new(1e-9).expect("tol")));
        let half = path_point_at(&path, 0.5, FLAT).expect("point");
        assert!(half.approx_eq(Point::new(10.0, 10.0), Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn tangent_at_points_along_travel() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let t = path_tangent_at(&path, 0.1, FLAT).expect("tangent");
        assert!(t.approx_eq(Vec2::new(1.0, 0.0), Tolerance::new(1e-9).expect("tol")));
        let t2 = path_tangent_at(&path, 0.35, FLAT).expect("tangent");
        assert!(t2.approx_eq(Vec2::new(0.0, 1.0), Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn point_at_on_a_cubic_is_arc_length_even() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let path = sub.into_path();
        let total = path_length(&path, fine());
        let mid = path_point_at(&path, 0.5, fine()).expect("point");
        let mut walk = SubPath::open(path.subpaths[0].start);
        walk.segments = path.subpaths[0].segments.clone();
        let head = flatten_subpath(&walk, Tolerance::new(1e-6).expect("tol"));
        let mut acc = 0.0;
        let mut found = head.points[0];
        for pair in head.points.windows(2) {
            let step = pair[0].distance(pair[1]);
            if acc + step >= total * 0.5 {
                found = pair[0].lerp(pair[1], (total * 0.5 - acc) / step);
                break;
            }
            acc += step;
        }
        assert!(mid.distance(found) < 0.05, "{mid:?} vs {found:?}");
    }

    #[test]
    fn point_at_on_an_empty_path_is_none() {
        assert!(path_point_at(&Path::new(), 0.5, FLAT).is_none());
        assert!(path_tangent_at(&Path::new(), 0.5, FLAT).is_none());
    }

    #[test]
    fn nearest_point_on_a_line() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0));
        let path = sub.into_path();
        let n = nearest_point_on_path(&path, Point::new(4.0, 3.0), FLAT).expect("nearest");
        assert!((n.distance - 3.0).abs() < 1e-12);
        assert!(n.point.approx_eq(Point::new(4.0, 0.0), EQ));
        assert_eq!(n.segment_index, 0);
        assert!((n.t - 0.4).abs() < 1e-12);
    }

    #[test]
    fn nearest_point_clamps_past_the_end() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0));
        let path = sub.into_path();
        let n = nearest_point_on_path(&path, Point::new(20.0, 0.0), FLAT).expect("nearest");
        assert!(n.point.approx_eq(Point::new(10.0, 0.0), EQ));
        assert!((n.t - 1.0).abs() < 1e-12);
    }

    #[test]
    fn nearest_point_on_a_circle_is_radial() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        let n = nearest_point_on_path(&path, Point::new(0.0, 20.0), FLAT).expect("nearest");
        assert!((n.distance - 15.0).abs() < 1e-9);
        assert!(n
            .point
            .approx_eq(Point::new(0.0, 5.0), Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn nearest_point_on_a_cubic_is_close() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let path = sub.into_path();
        let n = nearest_point_on_path(&path, Point::new(5.0, 20.0), FLAT).expect("nearest");
        assert!(n.point.x > 4.0 && n.point.x < 6.0);
        assert!(n.point.y > 7.0);
    }

    #[test]
    fn nearest_point_on_an_empty_path_is_none() {
        assert!(nearest_point_on_path(&Path::new(), Point::ORIGIN, FLAT).is_none());
    }

    #[test]
    fn nearest_point_picks_the_closest_subpath() {
        let mut path = circle(Point::ORIGIN, 1.0).expect("circle");
        path.extend(circle(Point::new(100.0, 0.0), 1.0).expect("circle"));
        let n = nearest_point_on_path(&path, Point::new(99.0, 0.0), FLAT).expect("nearest");
        assert_eq!(n.subpath_index, 1);
    }

    #[test]
    fn segment_bbox_of_an_arc_with_no_bulge_is_the_chord() {
        let b = segment_bbox(Point::ORIGIN, &Segment::arc(Point::new(4.0, 2.0), 0.0));
        assert!((b.width() - 4.0).abs() < 1e-12);
        assert!((b.height() - 2.0).abs() < 1e-12);
    }
}
