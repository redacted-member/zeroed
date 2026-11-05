//! Construction helpers for the shapes the draw tools create.
//!
//! Every closed shape is returned counter clockwise so that outer boundaries
//! have positive signed area.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::flatten::{fit_arcs, flatten_subpath};
use super::path::{bulge_from_sweep, Path, Polyline, SubPath};
use super::point::{Point, Tolerance, Vec2};

/// How a rectangle treats its corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RectCorner {
    /// Sharp corner.
    Square,
    /// Convex round, the usual rounded rectangle.
    RadiusExternal,
    /// Concave round, the arc is cut out of the corner.
    RadiusInternal,
    /// Straight cut across the corner.
    Chamfer,
}

/// Whether a regular polygon's radius reaches its corners or its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadiusKind {
    /// The radius reaches the midpoint of each edge.
    Inscribed,
    /// The radius reaches each corner.
    Circumscribed,
}

/// How a star's inner radius is given.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum StarInner {
    /// An absolute inner radius in millimetres.
    Radius(f64),
    /// A percentage of the outer radius, 0 to 100.
    Percent(f64),
}

/// Whether an ellipse is built from cubics or approximated with arcs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EllipseForm {
    /// Four cubic Beziers, the exact standard approximation.
    Cubic,
    /// Circular arcs fitted inside the flattening tolerance.
    Arc,
}

fn check_positive(name: &str, value: f64) -> Result<(), CoreError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(CoreError::geometry(format!(
            "{name} must be a positive finite number of millimeters, got {value}"
        )));
    }
    Ok(())
}

/// A straight line from `a` to `b` as an open path.
pub fn line(a: Point, b: Point) -> Path {
    let mut sub = SubPath::open(a);
    sub.line_to(b);
    sub.into_path()
}

/// An open or closed run of straight segments through the points.
///
/// Returns an error when fewer than two points are given.
pub fn polyline_path(points: &[Point], closed: bool) -> Result<Path, CoreError> {
    if points.len() < 2 {
        return Err(CoreError::geometry("a polyline needs at least two points"));
    }
    let poly = Polyline::new(points.to_vec(), closed);
    poly.to_subpath()
        .map(SubPath::into_path)
        .ok_or_else(|| CoreError::geometry("a polyline needs at least two points"))
}

/// An axis aligned rectangle with square corners, counter clockwise.
///
/// Returns an error when the two corners give a zero width or height.
pub fn rect(a: Point, b: Point) -> Result<Path, CoreError> {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    check_positive("rectangle width", x1 - x0)?;
    check_positive("rectangle height", y1 - y0)?;
    let mut sub = SubPath::open(Point::new(x0, y0));
    sub.line_to(Point::new(x1, y0))
        .line_to(Point::new(x1, y1))
        .line_to(Point::new(x0, y1))
        .line_to(Point::new(x0, y0));
    sub.closed = true;
    Ok(sub.into_path())
}

/// An axis aligned rectangle whose corners get the given treatment.
///
/// `radius` is the cut back distance along each edge, in millimetres. It is
/// ignored for [`RectCorner::Square`].
pub fn rounded_rect(
    a: Point,
    b: Point,
    radius: f64,
    corner: RectCorner,
) -> Result<Path, CoreError> {
    if corner == RectCorner::Square {
        return rect(a, b);
    }
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    check_positive("rectangle width", x1 - x0)?;
    check_positive("rectangle height", y1 - y0)?;
    check_positive("corner radius", radius)?;
    let shortest = (x1 - x0).min(y1 - y0);
    if radius * 2.0 > shortest + 1e-12 {
        return Err(CoreError::geometry(format!(
            "corner radius {radius} mm does not fit a {shortest} mm side"
        )));
    }
    let corners = [
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ];
    corner_treated_loop(&corners, radius, corner).map(SubPath::into_path)
}

/// Builds a closed loop from corner points with the corners cut back.
fn corner_treated_loop(
    corners: &[Point],
    radius: f64,
    corner: RectCorner,
) -> Result<SubPath, CoreError> {
    let n = corners.len();
    if n < 3 {
        return Err(CoreError::geometry("a closed loop needs three corners"));
    }
    let mut cuts = Vec::with_capacity(n);
    for i in 0..n {
        let prev = corners[(i + n - 1) % n];
        let cur = corners[i];
        let next = corners[(i + 1) % n];
        let dir_in = prev
            .to(cur)
            .normalised()
            .ok_or_else(|| CoreError::geometry("a corner cannot repeat the previous point"))?;
        let dir_out = cur
            .to(next)
            .normalised()
            .ok_or_else(|| CoreError::geometry("a corner cannot repeat the next point"))?;
        let turn = dir_in.cross(dir_out).atan2(dir_in.dot(dir_out));
        let start = cur.offset(dir_in.scaled(-radius));
        let end = cur.offset(dir_out.scaled(radius));
        cuts.push((start, end, turn));
    }

    let mut sub = SubPath::open(cuts[0].0);
    for (i, &(start, end, turn)) in cuts.iter().enumerate() {
        if i > 0 {
            sub.line_to(start);
        }
        match corner {
            RectCorner::Square => sub.line_to(end),
            RectCorner::Chamfer => sub.line_to(end),
            RectCorner::RadiusExternal => sub.arc_to(end, bulge_from_sweep(turn)),
            RectCorner::RadiusInternal => {
                let sweep = turn - turn.signum() * std::f64::consts::PI;
                sub.arc_to(end, bulge_from_sweep(sweep))
            }
        };
    }
    let first = cuts[0].0;
    sub.line_to(first);
    sub.closed = true;
    Ok(sub)
}

/// A circle as two half circle arcs, counter clockwise.
pub fn circle(centre: Point, radius: f64) -> Result<Path, CoreError> {
    check_positive("circle radius", radius)?;
    let right = Point::new(centre.x + radius, centre.y);
    let left = Point::new(centre.x - radius, centre.y);
    let mut sub = SubPath::open(right);
    sub.arc_to(left, 1.0).arc_to(right, 1.0);
    sub.closed = true;
    Ok(sub.into_path())
}

/// An ellipse centred on `centre` with the given semi axes, counter
/// clockwise, using the default flattening tolerance for the arc form.
pub fn ellipse(
    centre: Point,
    radius_x: f64,
    radius_y: f64,
    form: EllipseForm,
) -> Result<Path, CoreError> {
    ellipse_with_tolerance(centre, radius_x, radius_y, form, Tolerance::FLATTEN)
}

/// An ellipse, with an explicit tolerance for [`EllipseForm::Arc`].
pub fn ellipse_with_tolerance(
    centre: Point,
    radius_x: f64,
    radius_y: f64,
    form: EllipseForm,
    tol: Tolerance,
) -> Result<Path, CoreError> {
    check_positive("ellipse X radius", radius_x)?;
    check_positive("ellipse Y radius", radius_y)?;
    if form == EllipseForm::Arc {
        if (radius_x - radius_y).abs() <= Tolerance::EQUALITY.get() {
            return circle(centre, radius_x);
        }
        let cubic = ellipse_cubics(centre, radius_x, radius_y);
        let dense = Tolerance::new(tol.get() * 0.25)?;
        let poly = flatten_subpath(&cubic, dense);
        let sub = fit_arcs(&poly, tol)
            .ok_or_else(|| CoreError::geometry("could not fit arcs to the ellipse"))?;
        return Ok(sub.into_path());
    }
    Ok(ellipse_cubics(centre, radius_x, radius_y).into_path())
}

/// Circle to cubic Bezier magic number, `4 / 3 * (sqrt(2) - 1)`.
const KAPPA: f64 = 0.552_284_749_830_793_4;

fn ellipse_cubics(centre: Point, rx: f64, ry: f64) -> SubPath {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let right = Point::new(centre.x + rx, centre.y);
    let top = Point::new(centre.x, centre.y + ry);
    let left = Point::new(centre.x - rx, centre.y);
    let bottom = Point::new(centre.x, centre.y - ry);
    let mut sub = SubPath::open(right);
    sub.cubic_to(
        Point::new(right.x, right.y + ky),
        Point::new(top.x + kx, top.y),
        top,
    )
    .cubic_to(
        Point::new(top.x - kx, top.y),
        Point::new(left.x, left.y + ky),
        left,
    )
    .cubic_to(
        Point::new(left.x, left.y - ky),
        Point::new(bottom.x - kx, bottom.y),
        bottom,
    )
    .cubic_to(
        Point::new(bottom.x + kx, bottom.y),
        Point::new(right.x, right.y - ky),
        right,
    );
    sub.closed = true;
    sub
}

/// A regular polygon with `sides` sides, counter clockwise.
///
/// `rotation` is in radians counter clockwise, zero putting the first corner
/// straight up from the centre.
pub fn regular_polygon(
    centre: Point,
    sides: usize,
    radius: f64,
    kind: RadiusKind,
    rotation: f64,
) -> Result<Path, CoreError> {
    if sides < 3 {
        return Err(CoreError::geometry(format!(
            "a polygon needs at least three sides, got {sides}"
        )));
    }
    check_positive("polygon radius", radius)?;
    let step = std::f64::consts::TAU / sides as f64;
    let corner_radius = match kind {
        RadiusKind::Circumscribed => radius,
        RadiusKind::Inscribed => radius / (step * 0.5).cos(),
    };
    let base = std::f64::consts::FRAC_PI_2 + rotation;
    let points: Vec<Point> = (0..sides)
        .map(|i| {
            let a = base + step * i as f64;
            Point::new(
                centre.x + corner_radius * a.cos(),
                centre.y + corner_radius * a.sin(),
            )
        })
        .collect();
    polyline_path(&points, true)
}

/// A star with `points` spikes, counter clockwise.
///
/// `rotation` is in radians counter clockwise, zero putting the first spike
/// straight up from the centre.
pub fn star(
    centre: Point,
    points: usize,
    outer_radius: f64,
    inner: StarInner,
    rotation: f64,
) -> Result<Path, CoreError> {
    if points < 3 {
        return Err(CoreError::geometry(format!(
            "a star needs at least three points, got {points}"
        )));
    }
    check_positive("star outer radius", outer_radius)?;
    let inner_radius = match inner {
        StarInner::Radius(r) => r,
        StarInner::Percent(p) => {
            if !(0.0..=100.0).contains(&p) {
                return Err(CoreError::geometry(format!(
                    "star inner percentage must be between 0 and 100, got {p}"
                )));
            }
            outer_radius * p / 100.0
        }
    };
    check_positive("star inner radius", inner_radius)?;
    if inner_radius >= outer_radius {
        return Err(CoreError::geometry(
            "a star's inner radius must be smaller than its outer radius",
        ));
    }
    let step = std::f64::consts::PI / points as f64;
    let base = std::f64::consts::FRAC_PI_2 + rotation;
    let corners: Vec<Point> = (0..points * 2)
        .map(|i| {
            let r = if i % 2 == 0 {
                outer_radius
            } else {
                inner_radius
            };
            let a = base + step * i as f64;
            Point::new(centre.x + r * a.cos(), centre.y + r * a.sin())
        })
        .collect();
    polyline_path(&corners, true)
}

/// An open arc through three points, from `a` through `via` to `b`.
///
/// Returns an error when the three points are colinear or coincident.
pub fn arc_by_3_points(a: Point, via: Point, b: Point) -> Result<Path, CoreError> {
    let cross = a.to(via).cross(a.to(b));
    let scale = a.distance(via).max(a.distance(b)).max(1.0);
    if cross.abs() <= 1e-12 * scale * scale {
        return Err(CoreError::geometry(
            "three points on a straight line do not define an arc",
        ));
    }
    // The circle through all three points, then the sweep from a to b in
    // whichever direction passes via.
    let d = 2.0 * (a.x * (via.y - b.y) + via.x * (b.y - a.y) + b.x * (a.y - via.y));
    let (a2, v2, b2) = (
        a.x * a.x + a.y * a.y,
        via.x * via.x + via.y * via.y,
        b.x * b.x + b.y * b.y,
    );
    let centre = Point::new(
        (a2 * (via.y - b.y) + v2 * (b.y - a.y) + b2 * (a.y - via.y)) / d,
        (a2 * (b.x - via.x) + v2 * (a.x - b.x) + b2 * (via.x - a.x)) / d,
    );
    let tau = std::f64::consts::TAU;
    let angle = |p: Point| (p.y - centre.y).atan2(p.x - centre.x);
    let ccw_to = |p: Point| (angle(p) - angle(a)).rem_euclid(tau);
    let to_b = ccw_to(b);
    let sweep = if ccw_to(via) < to_b { to_b } else { to_b - tau };
    let mut sub = SubPath::open(a);
    sub.arc_to(b, bulge_from_sweep(sweep));
    Ok(sub.into_path())
}

/// An open arc on the circle around `centre`, from `start` to `end`.
///
/// `end` is projected onto the circle through `start`, so the caller does not
/// have to place it exactly. `ccw` picks the sweep direction.
pub fn arc_centre_start_end(
    centre: Point,
    start: Point,
    end: Point,
    ccw: bool,
) -> Result<Path, CoreError> {
    let radius = centre.distance(start);
    check_positive("arc radius", radius)?;
    let end_dir = centre
        .to(end)
        .normalised()
        .ok_or_else(|| CoreError::geometry("the arc end cannot sit on the center"))?;
    let on_circle = centre.offset(end_dir.scaled(radius));
    let start_angle = centre.to(start).angle();
    let end_angle = end_dir.angle();
    let two_pi = std::f64::consts::TAU;
    let mut sweep = end_angle - start_angle;
    if ccw {
        while sweep <= 0.0 {
            sweep += two_pi;
        }
    } else {
        while sweep >= 0.0 {
            sweep -= two_pi;
        }
    }
    if sweep.abs() >= two_pi - 1e-12 {
        return circle(centre, radius);
    }
    let mut sub = SubPath::open(start);
    sub.arc_to(on_circle, bulge_from_sweep(sweep));
    Ok(sub.into_path())
}

/// A smooth open cubic spline that passes through every given point.
///
/// Uses a Catmull-Rom basis converted to cubic Bezier control points, which
/// is what the freehand and curve tools draw.
pub fn bezier_through_points(points: &[Point]) -> Result<Path, CoreError> {
    if points.len() < 2 {
        return Err(CoreError::geometry(
            "a curve through points needs at least two points",
        ));
    }
    let n = points.len();
    let mut sub = SubPath::open(points[0]);
    for i in 0..n - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[n - 1]
        };
        let c1 = Point::new(p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0);
        let c2 = Point::new(p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0);
        sub.cubic_to(c1, c2, p2);
    }
    Ok(sub.into_path())
}

/// Unit direction from an angle in radians, for callers building shapes.
pub fn direction(angle: f64) -> Vec2 {
    Vec2::from_angle(angle)
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_arc_by_three_points_passes_through_the_middle_one() {
        use crate::geom::nearest_point_on_path;
        let tol = Tolerance::new(1e-6).expect("tolerance");
        for via in [
            Point::new(5.0, 5.0),
            Point::new(5.0, -5.0),
            Point::new(3.0, 1.0),
        ] {
            let arc =
                arc_by_3_points(Point::new(0.0, 0.0), via, Point::new(10.0, 0.0)).expect("arc");
            let near = nearest_point_on_path(&arc, via, tol).expect("near");
            assert!(near.distance < 1e-6, "misses {via:?} by {}", near.distance);
        }
    }

    use super::*;
    use crate::geom::metrics::{path_area, path_bbox, path_length};
    use crate::geom::path::{Segment, Winding};

    const EQ: Tolerance = Tolerance::EQUALITY;

    #[test]
    fn rect_is_four_lines_counter_clockwise() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 5.0)).expect("rect");
        assert_eq!(path.subpaths.len(), 1);
        assert_eq!(path.subpaths[0].segments.len(), 4);
        assert!(path.subpaths[0].closed);
        assert!((path_area(&path, Tolerance::FLATTEN) - 50.0).abs() < 1e-9);
        assert_eq!(path.subpaths[0].nodes().len(), 4);
    }

    #[test]
    fn rect_rejects_a_zero_side() {
        assert!(rect(Point::ORIGIN, Point::new(0.0, 5.0)).is_err());
        assert!(rect(Point::ORIGIN, Point::new(5.0, 0.0)).is_err());
    }

    #[test]
    fn rect_sorts_the_corners() {
        let a = rect(Point::new(10.0, 5.0), Point::ORIGIN).expect("rect");
        assert!(a.subpaths[0].start.approx_eq(Point::ORIGIN, EQ));
    }

    #[test]
    fn rounded_rect_external_has_four_arcs() {
        let path = rounded_rect(
            Point::ORIGIN,
            Point::new(20.0, 10.0),
            2.0,
            RectCorner::RadiusExternal,
        )
        .expect("rect");
        let arcs = path.subpaths[0]
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Arc { .. }))
            .count();
        assert_eq!(arcs, 4);
        let area = path_area(&path, Tolerance::new(0.001).expect("tolerance"));
        let expected = 200.0 - 4.0 * 4.0 + std::f64::consts::PI * 4.0;
        assert!((area - expected).abs() < 0.05, "area {area} vs {expected}");
    }

    #[test]
    fn rounded_rect_internal_arcs_bulge_the_other_way() {
        let path = rounded_rect(
            Point::ORIGIN,
            Point::new(20.0, 10.0),
            2.0,
            RectCorner::RadiusInternal,
        )
        .expect("rect");
        let bulges: Vec<f64> = path.subpaths[0]
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Arc { bulge, .. } => Some(*bulge),
                _ => None,
            })
            .collect();
        assert_eq!(bulges.len(), 4);
        assert!(bulges.iter().all(|b| *b < 0.0));
        let area = path_area(&path, Tolerance::new(0.001).expect("tolerance"));
        let expected = 200.0 - 4.0 * std::f64::consts::PI;
        assert!((area - expected).abs() < 0.05, "area {area} vs {expected}");
    }

    #[test]
    fn chamfer_corners_are_straight() {
        let path = rounded_rect(
            Point::ORIGIN,
            Point::new(20.0, 10.0),
            2.0,
            RectCorner::Chamfer,
        )
        .expect("rect");
        assert!(path.subpaths[0]
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
        let area = path_area(&path, Tolerance::FLATTEN);
        assert!((area - (200.0 - 4.0 * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn square_corner_type_falls_back_to_a_plain_rect() {
        let path = rounded_rect(Point::ORIGIN, Point::new(4.0, 4.0), 1.0, RectCorner::Square)
            .expect("rect");
        assert_eq!(path.subpaths[0].segments.len(), 4);
    }

    #[test]
    fn corner_radius_must_fit_the_side() {
        assert!(rounded_rect(
            Point::ORIGIN,
            Point::new(10.0, 4.0),
            3.0,
            RectCorner::RadiusExternal
        )
        .is_err());
        assert!(rounded_rect(
            Point::ORIGIN,
            Point::new(10.0, 4.0),
            0.0,
            RectCorner::RadiusExternal
        )
        .is_err());
    }

    #[test]
    fn circle_is_two_arcs_with_the_right_area() {
        let path = circle(Point::new(3.0, 4.0), 5.0).expect("circle");
        assert_eq!(path.subpaths[0].segments.len(), 2);
        let area = path_area(&path, Tolerance::new(0.0005).expect("tolerance"));
        let expected = std::f64::consts::PI * 25.0;
        assert!((area - expected).abs() < 0.02, "area {area}");
        let bbox = path_bbox(&path).expect("bbox");
        assert!((bbox.width() - 10.0).abs() < 1e-9);
        assert!((bbox.height() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn circle_rejects_a_zero_radius() {
        assert!(circle(Point::ORIGIN, 0.0).is_err());
        assert!(circle(Point::ORIGIN, -1.0).is_err());
    }

    #[test]
    fn circle_length_is_the_circumference() {
        let path = circle(Point::ORIGIN, 7.0).expect("circle");
        let len = path_length(&path, Tolerance::FLATTEN);
        assert!((len - std::f64::consts::TAU * 7.0).abs() < 1e-9);
    }

    #[test]
    fn ellipse_cubic_form_has_four_segments() {
        let path = ellipse(Point::ORIGIN, 10.0, 5.0, EllipseForm::Cubic).expect("ellipse");
        assert_eq!(path.subpaths[0].segments.len(), 4);
        let area = path_area(&path, Tolerance::new(0.001).expect("tolerance"));
        let expected = std::f64::consts::PI * 10.0 * 5.0;
        assert!((area - expected).abs() / expected < 0.005);
    }

    #[test]
    fn ellipse_arc_form_of_a_circle_is_a_circle() {
        let path = ellipse(Point::ORIGIN, 4.0, 4.0, EllipseForm::Arc).expect("ellipse");
        assert_eq!(path.subpaths[0].segments.len(), 2);
    }

    #[test]
    fn ellipse_arc_form_fits_arcs() {
        let path = ellipse(Point::ORIGIN, 10.0, 5.0, EllipseForm::Arc).expect("ellipse");
        assert!(path.subpaths[0].closed);
        let area = path_area(&path, Tolerance::new(0.001).expect("tolerance"));
        let expected = std::f64::consts::PI * 10.0 * 5.0;
        assert!((area - expected).abs() / expected < 0.02);
    }

    #[test]
    fn ellipse_rejects_zero_axes() {
        assert!(ellipse(Point::ORIGIN, 0.0, 5.0, EllipseForm::Cubic).is_err());
        assert!(ellipse(Point::ORIGIN, 5.0, -1.0, EllipseForm::Cubic).is_err());
    }

    #[test]
    fn regular_polygon_corner_count_and_winding() {
        let hex = regular_polygon(Point::ORIGIN, 6, 10.0, RadiusKind::Circumscribed, 0.0)
            .expect("polygon");
        assert_eq!(hex.subpaths[0].nodes().len(), 6);
        let area = path_area(&hex, Tolerance::FLATTEN);
        let expected = 1.5 * 3.0_f64.sqrt() * 100.0;
        assert!((area - expected).abs() < 1e-9);
        assert_eq!(
            crate::geom::metrics::path_winding(&hex, Tolerance::FLATTEN),
            Winding::Ccw
        );
    }

    #[test]
    fn inscribed_radius_reaches_the_edge_midpoint() {
        let square =
            regular_polygon(Point::ORIGIN, 4, 5.0, RadiusKind::Inscribed, 0.0).expect("polygon");
        let area = path_area(&square, Tolerance::FLATTEN);
        assert!((area - 100.0).abs() < 1e-9);
    }

    #[test]
    fn polygon_needs_three_sides() {
        assert!(regular_polygon(Point::ORIGIN, 2, 5.0, RadiusKind::Inscribed, 0.0).is_err());
        assert!(regular_polygon(Point::ORIGIN, 5, 0.0, RadiusKind::Inscribed, 0.0).is_err());
    }

    #[test]
    fn star_has_twice_as_many_nodes_as_points() {
        let s = star(Point::ORIGIN, 5, 10.0, StarInner::Percent(40.0), 0.0).expect("star");
        assert_eq!(s.subpaths[0].nodes().len(), 10);
        assert!(path_area(&s, Tolerance::FLATTEN) > 0.0);
    }

    #[test]
    fn star_inner_radius_forms_agree() {
        let by_percent = star(Point::ORIGIN, 6, 10.0, StarInner::Percent(50.0), 0.0).expect("star");
        let by_radius = star(Point::ORIGIN, 6, 10.0, StarInner::Radius(5.0), 0.0).expect("star");
        let a = path_area(&by_percent, Tolerance::FLATTEN);
        let b = path_area(&by_radius, Tolerance::FLATTEN);
        assert!((a - b).abs() < 1e-9);
    }

    #[test]
    fn star_rejects_bad_radii() {
        assert!(star(Point::ORIGIN, 5, 10.0, StarInner::Radius(10.0), 0.0).is_err());
        assert!(star(Point::ORIGIN, 5, 10.0, StarInner::Percent(150.0), 0.0).is_err());
        assert!(star(Point::ORIGIN, 2, 10.0, StarInner::Percent(50.0), 0.0).is_err());
    }

    #[test]
    fn arc_by_3_points_passes_through_the_middle() {
        let path = arc_by_3_points(
            Point::new(-1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(1.0, 0.0),
        )
        .expect("arc");
        let len = path_length(&path, Tolerance::FLATTEN);
        assert!((len - std::f64::consts::PI).abs() < 1e-9, "len {len}");
    }

    #[test]
    fn arc_by_3_points_rejects_colinear_points() {
        assert!(arc_by_3_points(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0)
        )
        .is_err());
    }

    #[test]
    fn arc_centre_start_end_respects_direction() {
        let ccw = arc_centre_start_end(
            Point::ORIGIN,
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            true,
        )
        .expect("arc");
        let cw = arc_centre_start_end(
            Point::ORIGIN,
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            false,
        )
        .expect("arc");
        let quarter = std::f64::consts::FRAC_PI_2;
        assert!((path_length(&ccw, Tolerance::FLATTEN) - quarter).abs() < 1e-9);
        assert!(
            (path_length(&cw, Tolerance::FLATTEN) - (std::f64::consts::TAU - quarter)).abs() < 1e-9
        );
    }

    #[test]
    fn arc_centre_start_end_projects_the_end_point() {
        let path = arc_centre_start_end(
            Point::ORIGIN,
            Point::new(2.0, 0.0),
            Point::new(0.0, 7.0),
            true,
        )
        .expect("arc");
        let end = path.subpaths[0].end();
        assert!(end.approx_eq(
            Point::new(0.0, 2.0),
            Tolerance::new(1e-9).expect("tolerance")
        ));
    }

    #[test]
    fn arc_centre_start_end_rejects_a_degenerate_radius() {
        assert!(
            arc_centre_start_end(Point::ORIGIN, Point::ORIGIN, Point::new(1.0, 0.0), true).is_err()
        );
    }

    #[test]
    fn bezier_through_points_passes_through_every_point() {
        let pts = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(20.0, -5.0),
            Point::new(30.0, 0.0),
        ];
        let path = bezier_through_points(&pts).expect("curve");
        assert_eq!(path.subpaths[0].segments.len(), 3);
        let nodes = path.subpaths[0].nodes();
        for (a, b) in nodes.iter().zip(pts.iter()) {
            assert!(a.approx_eq(*b, EQ));
        }
    }

    #[test]
    fn bezier_through_points_needs_two_points() {
        assert!(bezier_through_points(&[Point::ORIGIN]).is_err());
    }

    #[test]
    fn line_and_polyline_helpers() {
        let l = line(Point::ORIGIN, Point::new(3.0, 4.0));
        assert!((path_length(&l, Tolerance::FLATTEN) - 5.0).abs() < 1e-12);
        assert!(polyline_path(&[Point::ORIGIN], false).is_err());
        let p = polyline_path(
            &[Point::ORIGIN, Point::new(1.0, 0.0), Point::new(1.0, 1.0)],
            true,
        )
        .expect("polyline");
        assert!(p.subpaths[0].closed);
        assert!(direction(0.0).approx_eq(Vec2::new(1.0, 0.0), EQ));
    }
}
