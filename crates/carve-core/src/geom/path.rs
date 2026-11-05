//! The path data model: segments, subpaths, paths, polylines and polygons.
//!
//! A [`Path`] is a list of [`SubPath`]s. A subpath has a start point and an
//! ordered list of [`Segment`]s, each of which carries only its own end point
//! and control data: the start of a segment is the end of the one before it.
//! A closed subpath includes its closing segment, so the last segment ends on
//! the start point.
//!
//! Arcs use the DXF bulge convention: `bulge = tan(sweep / 4)`, positive for a
//! counter clockwise sweep from the segment start to the segment end. A bulge
//! of zero is a straight line and a bulge of 1 is a half circle.

use serde::{Deserialize, Serialize};

use super::bbox::BBox;
use super::point::{Point, Tolerance, Vec2};

/// One step along a subpath, from the previous node to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Segment {
    /// A straight line to `to`.
    Line {
        /// End point, in millimetres.
        to: Point,
    },
    /// A circular arc to `to` with the given DXF bulge.
    Arc {
        /// End point, in millimetres.
        to: Point,
        /// `tan(sweep / 4)`, positive counter clockwise.
        bulge: f64,
    },
    /// A cubic Bezier to `to` with two control points.
    Cubic {
        /// First control point, in millimetres.
        c1: Point,
        /// Second control point, in millimetres.
        c2: Point,
        /// End point, in millimetres.
        to: Point,
    },
}

/// Which of the three segment shapes a segment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// Straight line.
    Line,
    /// Circular arc.
    Arc,
    /// Cubic Bezier.
    Cubic,
}

impl Segment {
    /// The end point of the segment, in millimetres.
    pub fn end(&self) -> Point {
        match *self {
            Segment::Line { to } | Segment::Arc { to, .. } | Segment::Cubic { to, .. } => to,
        }
    }

    /// Replaces the end point, keeping the segment shape.
    pub fn set_end(&mut self, p: Point) {
        match self {
            Segment::Line { to } | Segment::Arc { to, .. } | Segment::Cubic { to, .. } => *to = p,
        }
    }

    /// Which shape this segment is.
    pub fn kind(&self) -> SegmentKind {
        match self {
            Segment::Line { .. } => SegmentKind::Line,
            Segment::Arc { .. } => SegmentKind::Arc,
            Segment::Cubic { .. } => SegmentKind::Cubic,
        }
    }

    /// A line segment to `to`.
    pub fn line(to: Point) -> Segment {
        Segment::Line { to }
    }

    /// An arc segment to `to` with the given bulge.
    pub fn arc(to: Point, bulge: f64) -> Segment {
        Segment::Arc { to, bulge }
    }

    /// A cubic segment to `to` with the two control points.
    pub fn cubic(c1: Point, c2: Point, to: Point) -> Segment {
        Segment::Cubic { c1, c2, to }
    }
}

/// One continuous run of segments, open or closed.
///
/// Reading one from JSON adds the closing line when a closed subpath's last
/// segment stops short of its start, so every subpath in the kernel keeps
/// the closing segment invariant whatever wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "SubPathFields")]
pub struct SubPath {
    /// First node of the run, in millimetres.
    pub start: Point,
    /// Segments in order. For a closed subpath the last one ends on `start`.
    pub segments: Vec<Segment>,
    /// True when the run forms a loop.
    pub closed: bool,
}

/// A [`SubPath`] as written, before the closing segment is checked.
#[derive(Deserialize)]
struct SubPathFields {
    start: Point,
    segments: Vec<Segment>,
    closed: bool,
}

impl From<SubPathFields> for SubPath {
    fn from(fields: SubPathFields) -> Self {
        let mut sub = SubPath::new(fields.start, fields.segments, fields.closed);
        if sub.closed && !sub.segments.is_empty() && sub.end().distance(sub.start) > 1e-9 {
            let start = sub.start;
            sub.line_to(start);
        }
        sub
    }
}

impl SubPath {
    /// An empty open subpath that begins at `start`.
    pub fn open(start: Point) -> Self {
        SubPath {
            start,
            segments: Vec::new(),
            closed: false,
        }
    }

    /// Builds a subpath from its parts without checking the closing segment.
    pub fn new(start: Point, segments: Vec<Segment>, closed: bool) -> Self {
        SubPath {
            start,
            segments,
            closed,
        }
    }

    /// The last point of the subpath, in millimetres.
    pub fn end(&self) -> Point {
        self.segments.last().map_or(self.start, Segment::end)
    }

    /// True when the subpath has no segments at all.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Number of segments in the subpath.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Start point of segment `index`, in millimetres.
    pub fn segment_start(&self, index: usize) -> Option<Point> {
        if index > self.segments.len() {
            return None;
        }
        if index == 0 {
            Some(self.start)
        } else {
            self.segments.get(index - 1).map(Segment::end)
        }
    }

    /// Every node of the subpath in order. A closed subpath does not repeat
    /// its start point at the end.
    pub fn nodes(&self) -> Vec<Point> {
        let mut out = Vec::with_capacity(self.segments.len() + 1);
        out.push(self.start);
        for seg in &self.segments {
            out.push(seg.end());
        }
        if self.closed && out.len() > 1 {
            out.pop();
        }
        out
    }

    /// Number of nodes, which is the number of editable points in the 2D view.
    pub fn node_count(&self) -> usize {
        if self.segments.is_empty() {
            1
        } else if self.closed {
            self.segments.len()
        } else {
            self.segments.len() + 1
        }
    }

    /// Iterates the segments together with their start points.
    pub fn iter(&self) -> impl Iterator<Item = (Point, &Segment)> {
        let mut cursor = self.start;
        self.segments.iter().map(move |seg| {
            let from = cursor;
            cursor = seg.end();
            (from, seg)
        })
    }

    /// Appends a straight line to `to`.
    pub fn line_to(&mut self, to: Point) -> &mut Self {
        self.segments.push(Segment::Line { to });
        self
    }

    /// Appends a circular arc to `to` with the given bulge.
    pub fn arc_to(&mut self, to: Point, bulge: f64) -> &mut Self {
        self.segments.push(Segment::Arc { to, bulge });
        self
    }

    /// Appends a cubic Bezier to `to`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, to: Point) -> &mut Self {
        self.segments.push(Segment::Cubic { c1, c2, to });
        self
    }

    /// Closes the subpath, adding a straight closing segment when the last
    /// node is not already on the start point.
    pub fn close_with_line(&mut self, tol: Tolerance) -> &mut Self {
        if !self.end().approx_eq(self.start, tol) {
            let start = self.start;
            self.line_to(start);
        }
        self.closed = true;
        self
    }

    /// Wraps the subpath in a single subpath [`Path`].
    pub fn into_path(self) -> Path {
        Path {
            subpaths: vec![self],
        }
    }
}

/// A complete vector: one or more subpaths.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Path {
    /// The runs that make up the vector.
    pub subpaths: Vec<SubPath>,
}

impl Path {
    /// An empty path.
    pub fn new() -> Self {
        Path::default()
    }

    /// A path holding exactly one subpath.
    pub fn from_subpath(subpath: SubPath) -> Self {
        Path {
            subpaths: vec![subpath],
        }
    }

    /// A path holding the given subpaths.
    pub fn from_subpaths(subpaths: Vec<SubPath>) -> Self {
        Path { subpaths }
    }

    /// True when the path has no subpaths, or only empty ones.
    pub fn is_empty(&self) -> bool {
        self.subpaths.iter().all(SubPath::is_empty)
    }

    /// Adds a subpath to the end of the path.
    pub fn push(&mut self, subpath: SubPath) -> &mut Self {
        self.subpaths.push(subpath);
        self
    }

    /// Appends every subpath of `other`.
    pub fn extend(&mut self, other: Path) -> &mut Self {
        self.subpaths.extend(other.subpaths);
        self
    }

    /// True when every subpath is closed and there is at least one.
    pub fn is_closed(&self) -> bool {
        !self.subpaths.is_empty() && self.subpaths.iter().all(|s| s.closed)
    }

    /// Total number of segments across all subpaths.
    pub fn segment_count(&self) -> usize {
        self.subpaths.iter().map(SubPath::len).sum()
    }

    /// First point of the first subpath.
    pub fn start_point(&self) -> Option<Point> {
        self.subpaths.first().map(|s| s.start)
    }

    /// Last point of the last subpath.
    pub fn end_point(&self) -> Option<Point> {
        self.subpaths.last().map(SubPath::end)
    }
}

/// Which way round a closed loop runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Winding {
    /// Counter clockwise, positive signed area. Outer boundaries use this.
    Ccw,
    /// Clockwise, negative signed area. Holes use this.
    Cw,
    /// The loop encloses no area.
    Degenerate,
}

impl Winding {
    /// The winding implied by a signed area in square millimetres.
    pub fn of_area(area: f64, tol: Tolerance) -> Winding {
        if area > tol.get() {
            Winding::Ccw
        } else if area < -tol.get() {
            Winding::Cw
        } else {
            Winding::Degenerate
        }
    }
}

/// A flattened path: straight segments between consecutive points.
///
/// A closed polyline does not repeat its first point at the end.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Polyline {
    /// The points in order, in millimetres.
    pub points: Vec<Point>,
    /// True when the last point joins back to the first.
    pub closed: bool,
}

impl Polyline {
    /// Builds a polyline from points.
    pub fn new(points: Vec<Point>, closed: bool) -> Self {
        Polyline { points, closed }
    }

    /// Number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// True when the polyline holds no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Total length along the polyline, in millimetres.
    pub fn length(&self) -> f64 {
        let mut total = 0.0;
        for pair in self.points.windows(2) {
            total += pair[0].distance(pair[1]);
        }
        if self.closed && self.points.len() > 2 {
            if let (Some(first), Some(last)) = (self.points.first(), self.points.last()) {
                total += last.distance(*first);
            }
        }
        total
    }

    /// Signed area in square millimetres, positive counter clockwise. The
    /// polyline is treated as closed whether or not its flag says so.
    pub fn signed_area(&self) -> f64 {
        let n = self.points.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            sum += a.x * b.y - b.x * a.y;
        }
        sum * 0.5
    }

    /// Which way round the polyline runs.
    pub fn winding(&self) -> Winding {
        Winding::of_area(self.signed_area(), Tolerance::EQUALITY)
    }

    /// The tight bounding box, or `None` when there are no points.
    pub fn bbox(&self) -> Option<BBox> {
        BBox::of_points(self.points.iter().copied())
    }

    /// The same polyline with the point order reversed.
    pub fn reversed(&self) -> Polyline {
        let mut points = self.points.clone();
        points.reverse();
        Polyline {
            points,
            closed: self.closed,
        }
    }

    /// Reverses the point order in place when the winding is not `want`.
    pub fn orient(&mut self, want: Winding) {
        if self.winding() != want && self.winding() != Winding::Degenerate {
            self.points.reverse();
        }
    }

    /// Turns the polyline into a subpath of straight segments.
    pub fn to_subpath(&self) -> Option<SubPath> {
        let (first, rest) = self.points.split_first()?;
        let mut sub = SubPath::open(*first);
        for p in rest {
            sub.line_to(*p);
        }
        if self.closed && self.points.len() > 1 {
            sub.line_to(*first);
            sub.closed = true;
        }
        Some(sub)
    }
}

/// A closed area: one outer loop and any number of hole loops.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Polygon {
    /// Outer boundary, counter clockwise once normalised.
    pub outer: Polyline,
    /// Holes, clockwise once normalised.
    pub holes: Vec<Polyline>,
}

impl Polygon {
    /// Builds a polygon from an outer loop and its holes.
    pub fn new(outer: Polyline, holes: Vec<Polyline>) -> Self {
        Polygon { outer, holes }
    }

    /// Area of the outer loop less the holes, in square millimetres.
    pub fn area(&self) -> f64 {
        let mut area = self.outer.signed_area().abs();
        for hole in &self.holes {
            area -= hole.signed_area().abs();
        }
        area
    }

    /// The tight bounding box of the outer loop.
    pub fn bbox(&self) -> Option<BBox> {
        self.outer.bbox()
    }

    /// Normalises winding so the outer loop is counter clockwise and every
    /// hole is clockwise.
    pub fn normalise_winding(&mut self) {
        self.outer.orient(Winding::Ccw);
        for hole in &mut self.holes {
            hole.orient(Winding::Cw);
        }
    }
}

/// The circle a bulge arc lies on, with the swept angles in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcGeom {
    /// Centre of the circle, in millimetres.
    pub centre: Point,
    /// Radius, in millimetres, always positive.
    pub radius: f64,
    /// Angle of the start point from the centre, in radians.
    pub start_angle: f64,
    /// Signed swept angle in radians, positive counter clockwise.
    pub sweep: f64,
}

/// The bulge value for a signed swept angle in radians.
pub fn bulge_from_sweep(sweep: f64) -> f64 {
    (sweep / 4.0).tan()
}

/// The signed swept angle in radians for a bulge value.
pub fn sweep_from_bulge(bulge: f64) -> f64 {
    4.0 * bulge.atan()
}

impl ArcGeom {
    /// Resolves the circle an arc segment lies on.
    ///
    /// Returns `None` when the bulge is too small to be an arc or the start
    /// and end points coincide, both of which mean "treat this as a line".
    pub fn from_bulge(start: Point, end: Point, bulge: f64) -> Option<ArcGeom> {
        if !bulge.is_finite() || bulge.abs() < 1e-12 {
            return None;
        }
        let chord = start.to(end);
        let chord_len = chord.length();
        if chord_len < 1e-12 {
            return None;
        }
        let br = 0.5 * (1.0 / bulge - bulge);
        let centre = Point::new(
            0.5 * (start.x + end.x - br * chord.y),
            0.5 * (start.y + end.y + br * chord.x),
        );
        let radius = chord_len * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
        let start_angle = centre.to(start).angle();
        Some(ArcGeom {
            centre,
            radius,
            start_angle,
            sweep: sweep_from_bulge(bulge),
        })
    }

    /// Builds an arc from its circle and swept angles.
    pub fn from_angles(centre: Point, radius: f64, start_angle: f64, sweep: f64) -> ArcGeom {
        ArcGeom {
            centre,
            radius,
            start_angle,
            sweep,
        }
    }

    /// Angle of the end point from the centre, in radians.
    pub fn end_angle(&self) -> f64 {
        self.start_angle + self.sweep
    }

    /// True when the arc runs counter clockwise.
    pub fn is_ccw(&self) -> bool {
        self.sweep > 0.0
    }

    /// The point at fraction `t` of the sweep, `t` from 0 to 1.
    pub fn point_at(&self, t: f64) -> Point {
        let a = self.start_angle + self.sweep * t;
        Point::new(
            self.centre.x + self.radius * a.cos(),
            self.centre.y + self.radius * a.sin(),
        )
    }

    /// The unit tangent at fraction `t` of the sweep, pointing along travel.
    pub fn tangent_at(&self, t: f64) -> Vec2 {
        let a = self.start_angle + self.sweep * t;
        let radial = Vec2::from_angle(a);
        if self.sweep >= 0.0 {
            radial.perp()
        } else {
            radial.perp().scaled(-1.0)
        }
    }

    /// Arc length in millimetres.
    pub fn length(&self) -> f64 {
        self.radius * self.sweep.abs()
    }

    /// Start point of the arc.
    pub fn start_point(&self) -> Point {
        self.point_at(0.0)
    }

    /// End point of the arc.
    pub fn end_point(&self) -> Point {
        self.point_at(1.0)
    }

    /// True when the angle in radians lies inside the swept range.
    pub fn contains_angle(&self, angle: f64) -> bool {
        let two_pi = std::f64::consts::TAU;
        let sweep = self.sweep;
        let mut delta = (angle - self.start_angle) % two_pi;
        if sweep >= 0.0 {
            if delta < 0.0 {
                delta += two_pi;
            }
            delta <= sweep + 1e-12
        } else {
            if delta > 0.0 {
                delta -= two_pi;
            }
            delta >= sweep - 1e-12
        }
    }

    /// The tight bounding box of the arc, including any axis extreme the
    /// sweep passes through.
    pub fn bbox(&self) -> BBox {
        let mut bbox = BBox::new(self.start_point(), self.end_point());
        let quarter = std::f64::consts::FRAC_PI_2;
        for k in 0..4 {
            let angle = quarter * k as f64;
            if self.contains_angle(angle) {
                bbox = bbox.with_point(Point::new(
                    self.centre.x + self.radius * angle.cos(),
                    self.centre.y + self.radius * angle.sin(),
                ));
            }
        }
        bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Tolerance = Tolerance::EQUALITY;

    fn quarter_arc() -> ArcGeom {
        ArcGeom::from_bulge(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            bulge_from_sweep(std::f64::consts::FRAC_PI_2),
        )
        .expect("an arc")
    }

    #[test]
    fn segment_end_and_kind() {
        assert_eq!(
            Segment::line(Point::new(1.0, 2.0)).end(),
            Point::new(1.0, 2.0)
        );
        assert_eq!(
            Segment::arc(Point::new(1.0, 2.0), 0.5).kind(),
            SegmentKind::Arc
        );
        assert_eq!(
            Segment::cubic(Point::ORIGIN, Point::ORIGIN, Point::new(3.0, 0.0)).kind(),
            SegmentKind::Cubic
        );
    }

    #[test]
    fn segment_set_end_keeps_the_shape() {
        let mut seg = Segment::arc(Point::new(1.0, 0.0), 0.25);
        seg.set_end(Point::new(2.0, 0.0));
        assert_eq!(seg, Segment::arc(Point::new(2.0, 0.0), 0.25));
    }

    #[test]
    fn subpath_tracks_start_end_and_nodes() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(1.0, 1.0));
        assert_eq!(sub.end(), Point::new(1.0, 1.0));
        assert_eq!(sub.nodes().len(), 3);
        assert_eq!(sub.node_count(), 3);
        assert_eq!(sub.segment_start(1), Some(Point::new(1.0, 0.0)));
        assert_eq!(sub.segment_start(0), Some(Point::ORIGIN));
    }

    #[test]
    fn closed_subpath_does_not_repeat_the_start_node() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(1.0, 1.0))
            .close_with_line(TOL);
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3);
        assert_eq!(sub.nodes().len(), 3);
        assert_eq!(sub.node_count(), 3);
    }

    #[test]
    fn close_with_line_does_not_add_a_zero_length_segment() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0)).line_to(Point::ORIGIN);
        sub.close_with_line(TOL);
        assert_eq!(sub.segments.len(), 2);
    }

    #[test]
    fn subpath_iter_yields_start_points() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(2.0, 0.0));
        let starts: Vec<Point> = sub.iter().map(|(s, _)| s).collect();
        assert_eq!(starts, vec![Point::ORIGIN, Point::new(1.0, 0.0)]);
    }

    #[test]
    fn empty_path_reports_empty() {
        assert!(Path::new().is_empty());
        assert!(Path::from_subpath(SubPath::open(Point::ORIGIN)).is_empty());
    }

    #[test]
    fn path_counts_segments_and_ends() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(1.0, 0.0));
        let mut b = SubPath::open(Point::new(5.0, 5.0));
        b.line_to(Point::new(6.0, 5.0))
            .line_to(Point::new(6.0, 6.0));
        let path = Path::from_subpaths(vec![a, b]);
        assert_eq!(path.segment_count(), 3);
        assert_eq!(path.start_point(), Some(Point::ORIGIN));
        assert_eq!(path.end_point(), Some(Point::new(6.0, 6.0)));
        assert!(!path.is_closed());
    }

    #[test]
    fn polyline_length_and_area() {
        let square = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(2.0, 0.0),
                Point::new(2.0, 2.0),
                Point::new(0.0, 2.0),
            ],
            true,
        );
        assert!((square.length() - 8.0).abs() < 1e-12);
        assert!((square.signed_area() - 4.0).abs() < 1e-12);
        assert_eq!(square.winding(), Winding::Ccw);
        assert_eq!(square.reversed().winding(), Winding::Cw);
    }

    #[test]
    fn polyline_orient_flips_only_when_needed() {
        let mut p = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(0.0, 1.0),
                Point::new(1.0, 1.0),
            ],
            true,
        );
        assert_eq!(p.winding(), Winding::Cw);
        p.orient(Winding::Ccw);
        assert_eq!(p.winding(), Winding::Ccw);
        let before = p.points.clone();
        p.orient(Winding::Ccw);
        assert_eq!(p.points, before);
    }

    #[test]
    fn degenerate_polyline_has_no_area() {
        let line = Polyline::new(vec![Point::ORIGIN, Point::new(1.0, 0.0)], false);
        assert_eq!(line.signed_area(), 0.0);
        assert_eq!(line.winding(), Winding::Degenerate);
        assert!(Polyline::default().is_empty());
    }

    #[test]
    fn polyline_to_subpath_closes_the_loop() {
        let square = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
            ],
            true,
        );
        let sub = square.to_subpath().expect("points");
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3);
        assert!(sub.end().approx_eq(sub.start, TOL));
    }

    #[test]
    fn polygon_area_subtracts_holes() {
        let outer = Polyline::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            true,
        );
        let hole = Polyline::new(
            vec![
                Point::new(2.0, 2.0),
                Point::new(2.0, 4.0),
                Point::new(4.0, 4.0),
                Point::new(4.0, 2.0),
            ],
            true,
        );
        let mut poly = Polygon::new(outer, vec![hole]);
        poly.normalise_winding();
        assert_eq!(poly.outer.winding(), Winding::Ccw);
        assert_eq!(poly.holes[0].winding(), Winding::Cw);
        assert!((poly.area() - 96.0).abs() < 1e-12);
        assert!(poly.bbox().expect("bbox").width() > 9.9);
    }

    #[test]
    fn bulge_and_sweep_round_trip() {
        for sweep in [0.1_f64, 1.0, 3.0, -0.5, -2.5] {
            let b = bulge_from_sweep(sweep);
            assert!((sweep_from_bulge(b) - sweep).abs() < 1e-12);
        }
        assert!((bulge_from_sweep(std::f64::consts::PI) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn half_circle_bulge_gives_the_right_circle() {
        let arc =
            ArcGeom::from_bulge(Point::new(0.0, 0.0), Point::new(2.0, 0.0), 1.0).expect("an arc");
        assert!(arc.centre.approx_eq(Point::new(1.0, 0.0), TOL));
        assert!((arc.radius - 1.0).abs() < 1e-12);
        assert!((arc.sweep - std::f64::consts::PI).abs() < 1e-12);
        assert!((arc.length() - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn quarter_arc_end_points_match_the_segment() {
        let arc = quarter_arc();
        assert!(arc.start_point().approx_eq(Point::new(0.0, 0.0), TOL));
        assert!(arc.end_point().approx_eq(Point::new(1.0, 1.0), TOL));
        assert!((arc.radius - 1.0).abs() < 1e-9);
        assert!(arc.is_ccw());
    }

    #[test]
    fn negative_bulge_sweeps_clockwise() {
        let arc =
            ArcGeom::from_bulge(Point::new(0.0, 0.0), Point::new(2.0, 0.0), -1.0).expect("an arc");
        assert!(!arc.is_ccw());
        assert!(arc.point_at(0.5).approx_eq(Point::new(1.0, 1.0), TOL));
    }

    #[test]
    fn zero_bulge_is_not_an_arc() {
        assert!(ArcGeom::from_bulge(Point::ORIGIN, Point::new(1.0, 0.0), 0.0).is_none());
        assert!(ArcGeom::from_bulge(Point::ORIGIN, Point::ORIGIN, 1.0).is_none());
    }

    #[test]
    fn arc_tangent_points_along_travel() {
        let arc =
            ArcGeom::from_bulge(Point::new(0.0, 0.0), Point::new(2.0, 0.0), 1.0).expect("an arc");
        let t = arc.tangent_at(0.0);
        assert!(t.approx_eq(Vec2::new(0.0, -1.0), Tolerance::EQUALITY));
    }

    #[test]
    fn arc_bbox_includes_the_axis_extreme() {
        let arc =
            ArcGeom::from_bulge(Point::new(0.0, 0.0), Point::new(2.0, 0.0), 1.0).expect("an arc");
        let b = arc.bbox();
        assert!((b.min.y + 1.0).abs() < 1e-12);
        assert!((b.max.y).abs() < 1e-12);
        assert!((b.width() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn contains_angle_respects_direction() {
        let ccw = ArcGeom::from_angles(Point::ORIGIN, 1.0, 0.0, std::f64::consts::FRAC_PI_2);
        assert!(ccw.contains_angle(0.5));
        assert!(!ccw.contains_angle(-0.5));
        let cw = ArcGeom::from_angles(Point::ORIGIN, 1.0, 0.0, -std::f64::consts::FRAC_PI_2);
        assert!(cw.contains_angle(-0.5));
        assert!(!cw.contains_angle(0.5));
    }

    #[test]
    fn a_closed_subpath_read_without_its_closing_line_gets_one() {
        let text = r#"{ "start": { "x": 0.0, "y": 0.0 }, "closed": true, "segments": [
            { "kind": "line", "to": { "x": 10.0, "y": 0.0 } },
            { "kind": "line", "to": { "x": 10.0, "y": 5.0 } } ] }"#;
        let sub: SubPath = serde_json::from_str(text).expect("reads");
        assert_eq!(sub.segments.len(), 3);
        assert_eq!(sub.end(), sub.start);

        let explicit = r#"{ "start": { "x": 0.0, "y": 0.0 }, "closed": true, "segments": [
            { "kind": "line", "to": { "x": 10.0, "y": 0.0 } },
            { "kind": "line", "to": { "x": 0.0, "y": 0.0 } } ] }"#;
        let sub: SubPath = serde_json::from_str(explicit).expect("reads");
        assert_eq!(sub.segments.len(), 2);

        let open = r#"{ "start": { "x": 0.0, "y": 0.0 }, "closed": false, "segments": [
            { "kind": "line", "to": { "x": 10.0, "y": 0.0 } } ] }"#;
        let sub: SubPath = serde_json::from_str(open).expect("reads");
        assert_eq!(sub.segments.len(), 1);
    }
}
