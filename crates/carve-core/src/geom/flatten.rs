//! Turning curves into polylines and polylines back into curves.
//!
//! Flattening uses a chord tolerance: no point of the true curve is further
//! than the tolerance from the straight segment that replaces it.

use crate::error::CoreError;

use super::path::{ArcGeom, Path, Polyline, Segment, SubPath};
use super::point::{Point, Tolerance, Vec2};

/// Point on a cubic Bezier at parameter `t`, `t` from 0 to 1.
pub fn cubic_point_at(p0: Point, c1: Point, c2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let (uu, tt) = (u * u, t * t);
    let (uuu, ttt) = (uu * u, tt * t);
    Point::new(
        uuu * p0.x + 3.0 * uu * t * c1.x + 3.0 * u * tt * c2.x + ttt * p3.x,
        uuu * p0.y + 3.0 * uu * t * c1.y + 3.0 * u * tt * c2.y + ttt * p3.y,
    )
}

/// First derivative of a cubic Bezier at parameter `t`.
pub fn cubic_derivative_at(p0: Point, c1: Point, c2: Point, p3: Point, t: f64) -> Vec2 {
    let u = 1.0 - t;
    Vec2::new(
        3.0 * u * u * (c1.x - p0.x) + 6.0 * u * t * (c2.x - c1.x) + 3.0 * t * t * (p3.x - c2.x),
        3.0 * u * u * (c1.y - p0.y) + 6.0 * u * t * (c2.y - c1.y) + 3.0 * t * t * (p3.y - c2.y),
    )
}

/// Second derivative of a cubic Bezier at parameter `t`.
pub fn cubic_second_derivative_at(p0: Point, c1: Point, c2: Point, p3: Point, t: f64) -> Vec2 {
    let u = 1.0 - t;
    Vec2::new(
        6.0 * u * (c2.x - 2.0 * c1.x + p0.x) + 6.0 * t * (p3.x - 2.0 * c2.x + c1.x),
        6.0 * u * (c2.y - 2.0 * c1.y + p0.y) + 6.0 * t * (p3.y - 2.0 * c2.y + c1.y),
    )
}

/// Number of straight pieces needed to hold an arc inside a chord tolerance.
fn arc_segment_count(arc: &ArcGeom, tol: Tolerance) -> usize {
    let sweep = arc.sweep.abs();
    if arc.radius <= 0.0 || sweep <= 0.0 {
        return 1;
    }
    let ratio = 1.0 - tol.get() / arc.radius;
    if ratio <= -1.0 {
        return 1;
    }
    let step = 2.0 * ratio.clamp(-1.0, 1.0).acos();
    if step <= 1e-9 {
        return 1;
    }
    ((sweep / step).ceil() as usize).max(1)
}

/// Appends the flattened points of one segment, not including `start`.
pub fn flatten_segment_into(start: Point, segment: &Segment, tol: Tolerance, out: &mut Vec<Point>) {
    match *segment {
        Segment::Line { to } => out.push(to),
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => {
                let n = arc_segment_count(&arc, tol);
                for i in 1..n {
                    out.push(arc.point_at(i as f64 / n as f64));
                }
                out.push(to);
            }
            None => out.push(to),
        },
        Segment::Cubic { c1, c2, to } => {
            flatten_cubic_into(start, c1, c2, to, tol.get(), 0, out);
            out.push(to);
        }
    }
}

/// Recursive adaptive subdivision of a cubic. Does not push the end point.
fn flatten_cubic_into(
    p0: Point,
    c1: Point,
    c2: Point,
    p3: Point,
    tol: f64,
    depth: u32,
    out: &mut Vec<Point>,
) {
    const MAX_DEPTH: u32 = 24;
    if depth >= MAX_DEPTH || is_cubic_flat(p0, c1, c2, p3, tol) {
        return;
    }
    // de Casteljau split at t = 0.5.
    let p01 = p0.midpoint(c1);
    let p12 = c1.midpoint(c2);
    let p23 = c2.midpoint(p3);
    let p012 = p01.midpoint(p12);
    let p123 = p12.midpoint(p23);
    let mid = p012.midpoint(p123);
    flatten_cubic_into(p0, p01, p012, mid, tol, depth + 1, out);
    out.push(mid);
    flatten_cubic_into(mid, p123, p23, p3, tol, depth + 1, out);
}

/// True when both control points lie within `tol` of the chord.
fn is_cubic_flat(p0: Point, c1: Point, c2: Point, p3: Point, tol: f64) -> bool {
    let chord = p0.to(p3);
    let len = chord.length();
    if len <= f64::MIN_POSITIVE {
        return p0.distance(c1) <= tol && p0.distance(c2) <= tol;
    }
    let d1 = (chord.cross(p0.to(c1)) / len).abs();
    let d2 = (chord.cross(p0.to(c2)) / len).abs();
    d1 <= tol && d2 <= tol
}

/// Flattens one subpath to a polyline within the chord tolerance.
pub fn flatten_subpath(subpath: &SubPath, tol: Tolerance) -> Polyline {
    let mut points = Vec::with_capacity(subpath.segments.len() * 2 + 1);
    points.push(subpath.start);
    for (start, seg) in subpath.iter() {
        flatten_segment_into(start, seg, tol, &mut points);
    }
    if subpath.closed && points.len() > 1 {
        if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied()) {
            if first.approx_eq(last, Tolerance::EQUALITY) {
                points.pop();
            }
        }
    }
    Polyline::new(points, subpath.closed)
}

/// Flattens every subpath of a path.
pub fn flatten_path(path: &Path, tol: Tolerance) -> Vec<Polyline> {
    path.subpaths
        .iter()
        .map(|s| flatten_subpath(s, tol))
        .collect()
}

/// Rebuilds a path from a polyline, fitting circular arcs where the points
/// lie on a common circle inside the tolerance.
pub fn path_from_polyline(polyline: &Polyline, tol: Tolerance) -> Path {
    match fit_arcs(polyline, tol) {
        Some(sub) => Path::from_subpath(sub),
        None => Path::new(),
    }
}

/// Circle through three points, or `None` when they are colinear.
fn circle_through(a: Point, b: Point, c: Point) -> Option<(Point, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let centre = Point::new(ux, uy);
    Some((centre, centre.distance(a)))
}

/// Signed bulge for an arc from `start` to `end` on the circle `centre`,
/// choosing the sweep that passes through `via`.
fn bulge_through(start: Point, via: Point, end: Point, centre: Point) -> Option<f64> {
    let a0 = centre.to(start).angle();
    let a1 = centre.to(via).angle();
    let a2 = centre.to(end).angle();
    let two_pi = std::f64::consts::TAU;
    let norm = |x: f64| {
        let mut v = x % two_pi;
        if v < 0.0 {
            v += two_pi;
        }
        v
    };
    let via_ccw = norm(a1 - a0);
    let end_ccw = norm(a2 - a0);
    let sweep = if via_ccw <= end_ccw {
        end_ccw
    } else {
        end_ccw - two_pi
    };
    if sweep.abs() < 1e-12 || sweep.abs() >= two_pi - 1e-12 {
        return None;
    }
    Some(super::path::bulge_from_sweep(sweep))
}

/// Greedily fits lines and circular arcs to a polyline.
///
/// Returns `None` when the polyline has no points. Runs of three or more
/// points that sit on a common circle within `tol` become a single arc, and
/// everything else becomes straight lines.
pub fn fit_arcs(polyline: &Polyline, tol: Tolerance) -> Option<SubPath> {
    let pts = &polyline.points;
    let first = *pts.first()?;
    let mut sub = SubPath::open(first);
    if pts.len() == 1 {
        return Some(sub);
    }
    let mut ring: Vec<Point> = pts.clone();
    if polyline.closed {
        ring.push(first);
    }

    let mut i = 0usize;
    while i + 1 < ring.len() {
        let mut best: Option<(usize, f64)> = None;
        let mut j = i + 2;
        while j < ring.len() {
            let mid = ring[i + (j - i) / 2];
            let Some((centre, radius)) = circle_through(ring[i], mid, ring[j]) else {
                break;
            };
            let on_circle = ring[i..=j]
                .iter()
                .all(|p| (p.distance(centre) - radius).abs() <= tol.get());
            if !on_circle {
                break;
            }
            // The points sitting on a circle is not enough: three corners of a
            // rectangle do that. The straight run between them also has to
            // stay within the tolerance of the arc, which is what tells a
            // flattened arc apart from a genuine corner.
            let follows_arc = ring[i..=j].windows(2).all(|pair| {
                let half = pair[0].distance(pair[1]) * 0.5;
                if half >= radius {
                    return false;
                }
                radius - (radius * radius - half * half).sqrt() <= tol.get()
            });
            if !follows_arc {
                break;
            }
            match bulge_through(ring[i], mid, ring[j], centre) {
                Some(bulge) => {
                    if super::path::sweep_from_bulge(bulge).abs() > std::f64::consts::PI + 1e-9 {
                        break;
                    }
                    best = Some((j, bulge));
                }
                None => break,
            }
            j += 1;
        }
        match best {
            Some((end, bulge)) => {
                sub.arc_to(ring[end], bulge);
                i = end;
            }
            None => {
                sub.line_to(ring[i + 1]);
                i += 1;
            }
        }
    }
    sub.closed = polyline.closed;
    Some(sub)
}

/// Fits a chain of cubic Beziers through the points using Schneider's
/// algorithm, so the result stays within `tol` of every input point.
///
/// Returns an error when fewer than two points are given.
pub fn fit_cubics(points: &[Point], tol: Tolerance) -> Result<SubPath, CoreError> {
    if points.len() < 2 {
        return Err(CoreError::geometry(
            "fitting cubics needs at least two points",
        ));
    }
    let mut cleaned: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if cleaned
            .last()
            .map(|last: &Point| last.approx_eq(*p, Tolerance::EQUALITY))
            != Some(true)
        {
            cleaned.push(*p);
        }
    }
    if cleaned.len() < 2 {
        return Err(CoreError::geometry(
            "fitting cubics needs at least two distinct points",
        ));
    }
    let left = cleaned[0].to(cleaned[1]).normalised().unwrap_or(Vec2::ZERO);
    let n = cleaned.len();
    let right = cleaned[n - 1]
        .to(cleaned[n - 2])
        .normalised()
        .unwrap_or(Vec2::ZERO);
    let mut sub = SubPath::open(cleaned[0]);
    fit_cubic_run(&cleaned, left, right, tol.get(), 0, &mut sub);
    Ok(sub)
}

fn fit_cubic_run(
    pts: &[Point],
    left_tangent: Vec2,
    right_tangent: Vec2,
    tol: f64,
    depth: u32,
    out: &mut SubPath,
) {
    const MAX_DEPTH: u32 = 16;
    if pts.len() == 2 {
        let dist = pts[0].distance(pts[1]) / 3.0;
        out.cubic_to(
            pts[0].offset(left_tangent.scaled(dist)),
            pts[1].offset(right_tangent.scaled(dist)),
            pts[1],
        );
        return;
    }

    let mut u = chord_length_parameters(pts);
    let mut curve = generate_bezier(pts, &u, left_tangent, right_tangent);
    let (mut max_err, mut split) = max_fit_error(pts, &u, &curve);

    if max_err <= tol {
        out.cubic_to(curve[1], curve[2], curve[3]);
        return;
    }

    if max_err <= tol * tol && depth < MAX_DEPTH {
        for _ in 0..4 {
            u = reparameterise(pts, &u, &curve);
            curve = generate_bezier(pts, &u, left_tangent, right_tangent);
            let (err, at) = max_fit_error(pts, &u, &curve);
            max_err = err;
            split = at;
            if max_err <= tol {
                out.cubic_to(curve[1], curve[2], curve[3]);
                return;
            }
        }
    }

    if depth >= MAX_DEPTH {
        out.cubic_to(curve[1], curve[2], curve[3]);
        return;
    }

    let split = split.clamp(1, pts.len() - 2);
    let centre = centre_tangent(pts, split);
    fit_cubic_run(&pts[..=split], left_tangent, centre, tol, depth + 1, out);
    fit_cubic_run(
        &pts[split..],
        centre.scaled(-1.0),
        right_tangent,
        tol,
        depth + 1,
        out,
    );
}

fn centre_tangent(pts: &[Point], at: usize) -> Vec2 {
    let before = pts[at - 1].to(pts[at]);
    let after = pts[at].to(pts[at + 1]);
    let sum = Vec2::new(before.x + after.x, before.y + after.y);
    sum.normalised()
        .map(|v| v.scaled(-1.0))
        .unwrap_or(Vec2::new(-1.0, 0.0))
}

fn chord_length_parameters(pts: &[Point]) -> Vec<f64> {
    let mut u = Vec::with_capacity(pts.len());
    u.push(0.0);
    for i in 1..pts.len() {
        let prev = u[i - 1];
        u.push(prev + pts[i - 1].distance(pts[i]));
    }
    let total = *u.last().unwrap_or(&0.0);
    if total > 0.0 {
        for value in u.iter_mut() {
            *value /= total;
        }
    }
    u
}

fn bernstein(t: f64) -> [f64; 4] {
    let u = 1.0 - t;
    [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t]
}

fn generate_bezier(pts: &[Point], u: &[f64], left: Vec2, right: Vec2) -> [Point; 4] {
    let first = pts[0];
    let last = pts[pts.len() - 1];
    let mut c = [[0.0f64; 2]; 2];
    let mut x = [0.0f64; 2];

    for (i, point) in pts.iter().enumerate() {
        let b = bernstein(u[i]);
        let a0 = left.scaled(b[1]);
        let a1 = right.scaled(b[2]);
        c[0][0] += a0.dot(a0);
        c[0][1] += a0.dot(a1);
        c[1][0] = c[0][1];
        c[1][1] += a1.dot(a1);
        let base = Point::new(
            first.x * (b[0] + b[1]) + last.x * (b[2] + b[3]),
            first.y * (b[0] + b[1]) + last.y * (b[2] + b[3]),
        );
        let tmp = base.to(*point);
        x[0] += a0.dot(tmp);
        x[1] += a1.dot(tmp);
    }

    let det_c = c[0][0] * c[1][1] - c[1][0] * c[0][1];
    let det_x0 = c[0][0] * x[1] - c[0][1] * x[0];
    let det_x1 = x[0] * c[1][1] - x[1] * c[0][1];

    let seg_len = first.distance(last);
    let (mut alpha_l, mut alpha_r) = if det_c.abs() < 1e-12 {
        (seg_len / 3.0, seg_len / 3.0)
    } else {
        (det_x1 / det_c, det_x0 / det_c)
    };
    if alpha_l < 1e-9 || alpha_r < 1e-9 || !alpha_l.is_finite() || !alpha_r.is_finite() {
        alpha_l = seg_len / 3.0;
        alpha_r = seg_len / 3.0;
    }

    [
        first,
        first.offset(left.scaled(alpha_l)),
        last.offset(right.scaled(alpha_r)),
        last,
    ]
}

fn max_fit_error(pts: &[Point], u: &[f64], curve: &[Point; 4]) -> (f64, usize) {
    let mut max = 0.0f64;
    let mut at = pts.len() / 2;
    for (i, point) in pts.iter().enumerate() {
        let on_curve = cubic_point_at(curve[0], curve[1], curve[2], curve[3], u[i]);
        let dist = on_curve.distance(*point);
        if dist > max {
            max = dist;
            at = i;
        }
    }
    (max, at)
}

fn reparameterise(pts: &[Point], u: &[f64], curve: &[Point; 4]) -> Vec<f64> {
    pts.iter()
        .zip(u.iter())
        .map(|(p, t)| newton_root(curve, *p, *t))
        .collect()
}

fn newton_root(curve: &[Point; 4], p: Point, t: f64) -> f64 {
    let on = cubic_point_at(curve[0], curve[1], curve[2], curve[3], t);
    let d1 = cubic_derivative_at(curve[0], curve[1], curve[2], curve[3], t);
    let d2 = cubic_second_derivative_at(curve[0], curve[1], curve[2], curve[3], t);
    let diff = p.to(on);
    let numerator = diff.dot(d1);
    let denominator = d1.dot(d1) + diff.dot(d2);
    if denominator.abs() < 1e-12 {
        t
    } else {
        (t - numerator / denominator).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::circle;
    use crate::geom::path::bulge_from_sweep;

    const EQ: Tolerance = Tolerance::EQUALITY;

    #[test]
    fn line_flattens_to_two_points() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0));
        let poly = flatten_subpath(&sub, Tolerance::FLATTEN);
        assert_eq!(poly.points.len(), 2);
        assert!((poly.length() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn arc_flattening_respects_the_chord_tolerance() {
        let tol = Tolerance::new(0.01).expect("tolerance");
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(20.0, 0.0), 1.0);
        let poly = flatten_subpath(&sub, tol);
        let centre = Point::new(10.0, 0.0);
        for pair in poly.points.windows(2) {
            let mid = pair[0].midpoint(pair[1]);
            assert!(10.0 - mid.distance(centre) <= tol.get() + 1e-9);
        }
        assert!(poly.points.len() > 10);
    }

    #[test]
    fn tighter_tolerance_gives_more_points() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(20.0, 0.0), 1.0);
        let coarse = flatten_subpath(&sub, Tolerance::new(0.1).expect("tolerance"));
        let fine = flatten_subpath(&sub, Tolerance::new(0.001).expect("tolerance"));
        assert!(fine.points.len() > coarse.points.len());
    }

    #[test]
    fn zero_bulge_arc_flattens_as_a_line() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.arc_to(Point::new(5.0, 0.0), 0.0);
        assert_eq!(flatten_subpath(&sub, Tolerance::FLATTEN).points.len(), 2);
    }

    #[test]
    fn cubic_flattening_follows_the_curve() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let poly = flatten_subpath(&sub, Tolerance::new(0.01).expect("tolerance"));
        assert!(poly.points.len() > 8);
        assert!(poly.points[0].approx_eq(Point::ORIGIN, EQ));
        let last = *poly.points.last().expect("points");
        assert!(last.approx_eq(Point::new(10.0, 0.0), EQ));
    }

    #[test]
    fn closed_flatten_does_not_repeat_the_first_point() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        let poly = flatten_subpath(&path.subpaths[0], Tolerance::FLATTEN);
        assert!(poly.closed);
        let first = poly.points[0];
        let last = *poly.points.last().expect("points");
        assert!(!first.approx_eq(last, EQ));
    }

    #[test]
    fn flatten_path_returns_one_polyline_per_subpath() {
        let mut path = circle(Point::ORIGIN, 5.0).expect("circle");
        path.extend(circle(Point::new(20.0, 0.0), 2.0).expect("circle"));
        assert_eq!(flatten_path(&path, Tolerance::FLATTEN).len(), 2);
    }

    #[test]
    fn cubic_point_at_hits_the_ends() {
        let p0 = Point::new(0.0, 0.0);
        let p3 = Point::new(3.0, 0.0);
        let c1 = Point::new(1.0, 2.0);
        let c2 = Point::new(2.0, 2.0);
        assert!(cubic_point_at(p0, c1, c2, p3, 0.0).approx_eq(p0, EQ));
        assert!(cubic_point_at(p0, c1, c2, p3, 1.0).approx_eq(p3, EQ));
    }

    #[test]
    fn cubic_derivative_matches_a_finite_difference() {
        let p0 = Point::new(0.0, 0.0);
        let c1 = Point::new(1.0, 2.0);
        let c2 = Point::new(2.0, -1.0);
        let p3 = Point::new(3.0, 0.0);
        let h = 1e-6;
        let a = cubic_point_at(p0, c1, c2, p3, 0.4 - h);
        let b = cubic_point_at(p0, c1, c2, p3, 0.4 + h);
        let numeric = Vec2::new((b.x - a.x) / (2.0 * h), (b.y - a.y) / (2.0 * h));
        let exact = cubic_derivative_at(p0, c1, c2, p3, 0.4);
        assert!((numeric.x - exact.x).abs() < 1e-4);
        assert!((numeric.y - exact.y).abs() < 1e-4);
    }

    #[test]
    fn fit_arcs_recovers_a_circle() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        let poly = flatten_subpath(&path.subpaths[0], Tolerance::new(0.001).expect("tolerance"));
        let fitted = fit_arcs(&poly, Tolerance::new(0.01).expect("tolerance")).expect("fit");
        assert!(fitted.closed);
        assert!(fitted.segments.len() <= 4);
        assert!(fitted
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Arc { .. })));
    }

    #[test]
    fn fit_arcs_keeps_straight_runs_straight() {
        let poly = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(2.0, 0.0),
                Point::new(3.0, 0.0),
            ],
            false,
        );
        let fitted = fit_arcs(&poly, Tolerance::FLATTEN).expect("fit");
        assert!(fitted
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
    }

    #[test]
    fn fit_arcs_on_a_single_point_gives_an_empty_subpath() {
        let poly = Polyline::new(vec![Point::ORIGIN], false);
        assert!(fit_arcs(&poly, Tolerance::FLATTEN).expect("fit").is_empty());
        assert!(fit_arcs(&Polyline::default(), Tolerance::FLATTEN).is_none());
    }

    #[test]
    fn path_from_polyline_round_trips_a_square() {
        let poly = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(4.0, 0.0),
                Point::new(4.0, 4.0),
                Point::new(0.0, 4.0),
            ],
            true,
        );
        let path = path_from_polyline(&poly, Tolerance::FLATTEN);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        assert!(path_from_polyline(&Polyline::default(), Tolerance::FLATTEN).is_empty());
    }

    #[test]
    fn fit_cubics_stays_inside_the_tolerance() {
        let mut pts = Vec::new();
        for i in 0..=40 {
            let t = i as f64 / 40.0 * std::f64::consts::PI;
            pts.push(Point::new(t * 5.0, t.sin() * 10.0));
        }
        let tol = Tolerance::new(0.05).expect("tolerance");
        let sub = fit_cubics(&pts, tol).expect("fit");
        assert!(!sub.segments.is_empty());
        let path = sub.into_path();
        for p in &pts {
            let near =
                crate::geom::metrics::nearest_point_on_path(&path, *p, tol).expect("nearest");
            assert!(
                near.distance <= tol.get(),
                "point {p:?} off by {}",
                near.distance
            );
        }
    }

    #[test]
    fn fit_cubics_on_two_points_gives_one_segment() {
        let sub = fit_cubics(
            &[Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            Tolerance::FLATTEN,
        )
        .expect("fit");
        assert_eq!(sub.segments.len(), 1);
        assert!(matches!(sub.segments[0], Segment::Cubic { .. }));
    }

    #[test]
    fn fit_cubics_rejects_too_few_points() {
        assert!(fit_cubics(&[], Tolerance::FLATTEN).is_err());
        assert!(fit_cubics(&[Point::ORIGIN], Tolerance::FLATTEN).is_err());
        assert!(fit_cubics(&[Point::ORIGIN, Point::ORIGIN], Tolerance::FLATTEN).is_err());
    }

    #[test]
    fn quarter_arc_flattens_symmetrically() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(
            Point::new(1.0, 1.0),
            bulge_from_sweep(std::f64::consts::FRAC_PI_2),
        );
        let poly = flatten_subpath(&sub, Tolerance::new(0.0001).expect("tolerance"));
        let centre = Point::new(0.0, 1.0);
        for p in &poly.points {
            assert!((p.distance(centre) - 1.0).abs() < 1e-9);
        }
    }
}
