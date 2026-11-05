//! Distortion envelopes: bending geometry by reshaping the box round it.
//!
//! An [`Envelope`] starts as the bounding box of what it bends. Its bottom
//! and top edges are curves through node points (left to right), and its
//! left and right sides are either straight or curves through node points of
//! their own (bottom to top). Each curve is a uniform Catmull Rom spline, so
//! it passes through every node and evenly spaced nodes on a straight line
//! give back that straight line exactly. A span can be a straight line
//! instead (node editing's line and curve spans, see [`EdgeLines`]).
//!
//! A point of the original box is placed by its normalised position
//! `(u, v)` (0 at the left or bottom edge, 1 at the right or top edge) and
//! lands on the bilinearly blended Coons patch of the four edge curves.
//! With straight sides this is the ruled surface between the bottom and top
//! curves. An envelope whose nodes still sit evenly on the box edges maps
//! every point to itself.

use serde::{Deserialize, Serialize};

use super::bbox::BBox;
use super::flatten::{fit_cubics, flatten_segment_into};
use super::path::{Path, Segment, SubPath};
use super::point::{Point, Tolerance, Vec2};
use crate::error::CoreError;

/// Most nodes one envelope edge may have.
pub const MAX_EDGE_NODES: usize = 64;

/// Longest run of samples one segment is cut into before it is mapped.
const MAX_SEGMENT_SAMPLES: usize = 512;

/// A curve through node points, parameterised over `0..=1` with node `k` of
/// `n` at `k / (n - 1)`.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeCurve {
    nodes: Vec<Point>,
    /// For each span, true when it is a straight line; empty when every
    /// span is a curve.
    lines: Vec<bool>,
}

/// Which spans of an envelope's edges are straight lines, each edge's list
/// in its node order (one flag per span). An empty list leaves every span
/// of that edge a curve.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EdgeLines {
    /// Bottom edge spans, left to right.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bottom: Vec<bool>,
    /// Top edge spans, left to right.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub top: Vec<bool>,
    /// Left side spans, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<bool>,
    /// Right side spans, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub right: Vec<bool>,
}

impl EdgeLines {
    /// True when every span of every edge is a curve.
    pub fn is_empty(&self) -> bool {
        [&self.bottom, &self.top, &self.left, &self.right]
            .iter()
            .all(|l| !l.iter().any(|x| *x))
    }
}

impl EdgeCurve {
    /// A curve through `nodes`, first to last.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for fewer than two or more than
    /// [`MAX_EDGE_NODES`] nodes, or a node that is not finite.
    pub fn new(nodes: Vec<Point>) -> Result<EdgeCurve, CoreError> {
        if nodes.len() < 2 || nodes.len() > MAX_EDGE_NODES {
            return Err(CoreError::validation(format!(
                "an envelope edge needs between 2 and {MAX_EDGE_NODES} nodes"
            )));
        }
        if !nodes.iter().all(|p| p.is_finite()) {
            return Err(CoreError::validation("envelope nodes must be numbers"));
        }
        Ok(EdgeCurve {
            nodes,
            lines: Vec::new(),
        })
    }

    /// The same nodes with the spans flagged true in `lines` straight.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when `lines` is neither empty nor
    /// one flag per span.
    pub fn with_lines(mut self, lines: &[bool]) -> Result<EdgeCurve, CoreError> {
        if !lines.is_empty() && lines.len() != self.nodes.len() - 1 {
            return Err(CoreError::validation(
                "an envelope edge needs one line or curve flag per span",
            ));
        }
        self.lines = lines.to_vec();
        Ok(self)
    }

    /// Whether span `k` is a straight line.
    fn is_line(&self, k: i64) -> bool {
        usize::try_from(k)
            .ok()
            .and_then(|k| self.lines.get(k))
            .copied()
            .unwrap_or(false)
    }

    /// `count` nodes spread evenly along the straight line from `a` to `b`.
    pub fn straight(a: Point, b: Point, count: usize) -> EdgeCurve {
        let count = count.clamp(2, MAX_EDGE_NODES);
        let nodes = (0..count)
            .map(|k| a.lerp(b, k as f64 / (count - 1) as f64))
            .collect();
        EdgeCurve {
            nodes,
            lines: Vec::new(),
        }
    }

    /// The nodes, first to last.
    pub fn nodes(&self) -> &[Point] {
        &self.nodes
    }

    /// First node.
    pub fn first(&self) -> Point {
        self.nodes[0]
    }

    /// Last node.
    pub fn last(&self) -> Point {
        self.nodes[self.nodes.len() - 1]
    }

    /// Node `k`, extended past both ends by reflecting the neighbour, so the
    /// end spans keep the slope of the first and last pairs.
    fn node(&self, k: i64) -> Point {
        let n = self.nodes.len() as i64;
        if k < 0 {
            let (a, b) = (self.nodes[0], self.nodes[1]);
            Point::new(2.0 * a.x - b.x, 2.0 * a.y - b.y)
        } else if k >= n {
            let (a, b) = (self.nodes[(n - 1) as usize], self.nodes[(n - 2) as usize]);
            Point::new(2.0 * a.x - b.x, 2.0 * a.y - b.y)
        } else {
            self.nodes[k as usize]
        }
    }

    /// The span holding parameter `t` and the local parameter inside it;
    /// outside `0..=1` the end spans carry on.
    fn span(&self, t: f64) -> (i64, f64) {
        let spans = (self.nodes.len() - 1) as f64;
        let s = t * spans;
        let k = (s.floor() as i64).min(self.nodes.len() as i64 - 2).max(0);
        (k, s - k as f64)
    }

    /// Point at parameter `t`; outside `0..=1` the end spans carry on.
    pub fn at(&self, t: f64) -> Point {
        let (k, s) = self.span(t);
        if self.is_line(k) {
            return self.node(k).lerp(self.node(k + 1), s);
        }
        let (p0, p1, p2, p3) = (
            self.node(k - 1),
            self.node(k),
            self.node(k + 1),
            self.node(k + 2),
        );
        let (s2, s3) = (s * s, s * s * s);
        let f = |a: f64, b: f64, c: f64, d: f64| {
            0.5 * (2.0 * b
                + (-a + c) * s
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * s2
                + (-a + 3.0 * b - 3.0 * c + d) * s3)
        };
        Point::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
    }

    /// Derivative with respect to `t` at parameter `t`.
    pub fn derivative(&self, t: f64) -> Vec2 {
        let (k, s) = self.span(t);
        let spans = (self.nodes.len() - 1) as f64;
        if self.is_line(k) {
            let (a, b) = (self.node(k), self.node(k + 1));
            return Vec2::new((b.x - a.x) * spans, (b.y - a.y) * spans);
        }
        let (p0, p1, p2, p3) = (
            self.node(k - 1),
            self.node(k),
            self.node(k + 1),
            self.node(k + 2),
        );
        let f = |a: f64, b: f64, c: f64, d: f64| {
            0.5 * ((-a + c)
                + 2.0 * (2.0 * a - 5.0 * b + 4.0 * c - d) * s
                + 3.0 * (-a + 3.0 * b - 3.0 * c + d) * s * s)
                * spans
        };
        Vec2::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
    }

    fn with_ends(mut self, first: Point, last: Point) -> EdgeCurve {
        let n = self.nodes.len();
        self.nodes[0] = first;
        self.nodes[n - 1] = last;
        self
    }
}

/// A box bent into a patch by four edge curves.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    bbox: BBox,
    bottom: EdgeCurve,
    top: EdgeCurve,
    left: EdgeCurve,
    right: EdgeCurve,
}

impl Envelope {
    /// An envelope round `bbox` with the given edges. `bottom` and `top` run
    /// left to right; `left` and `right` run bottom to top and are straight
    /// between the corners when absent. The corners are the ends of the
    /// bottom and top curves; the side curves' ends are moved onto them.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a box without width or height,
    /// or an edge [`EdgeCurve::new`] refuses.
    pub fn new(
        bbox: BBox,
        bottom: Vec<Point>,
        top: Vec<Point>,
        left: Option<Vec<Point>>,
        right: Option<Vec<Point>>,
    ) -> Result<Envelope, CoreError> {
        if !(bbox.min.is_finite() && bbox.max.is_finite())
            || bbox.width() <= 1e-9
            || bbox.height() <= 1e-9
        {
            return Err(CoreError::validation(
                "the envelope box needs a width and a height",
            ));
        }
        let bottom = EdgeCurve::new(bottom)?;
        let top = EdgeCurve::new(top)?;
        let side =
            |nodes: Option<Vec<Point>>, a: Point, b: Point| -> Result<EdgeCurve, CoreError> {
                match nodes {
                    Some(nodes) => Ok(EdgeCurve::new(nodes)?.with_ends(a, b)),
                    None => Ok(EdgeCurve::straight(a, b, 2)),
                }
            };
        let left = side(left, bottom.first(), top.first())?;
        let right = side(right, bottom.last(), top.last())?;
        Ok(Envelope {
            bbox,
            bottom,
            top,
            left,
            right,
        })
    }

    /// The same envelope with the spans `lines` flags made straight lines.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an edge whose flags are neither
    /// empty nor one per span.
    pub fn with_lines(mut self, lines: &EdgeLines) -> Result<Envelope, CoreError> {
        self.bottom = self.bottom.with_lines(&lines.bottom)?;
        self.top = self.top.with_lines(&lines.top)?;
        self.left = self.left.with_lines(&lines.left)?;
        self.right = self.right.with_lines(&lines.right)?;
        Ok(self)
    }

    /// The envelope that changes nothing, with `nodes` nodes on the bottom
    /// and top edges and straight sides.
    pub fn identity(bbox: BBox, nodes: usize) -> Envelope {
        let (min, max) = (bbox.min, bbox.max);
        Envelope {
            bbox,
            bottom: EdgeCurve::straight(min, Point::new(max.x, min.y), nodes),
            top: EdgeCurve::straight(Point::new(min.x, max.y), max, nodes),
            left: EdgeCurve::straight(min, Point::new(min.x, max.y), 2),
            right: EdgeCurve::straight(Point::new(max.x, min.y), max, 2),
        }
    }

    /// The original box.
    pub fn bbox(&self) -> BBox {
        self.bbox
    }

    /// Normalised position of a point of the original box.
    pub fn normalise(&self, p: Point) -> (f64, f64) {
        (
            (p.x - self.bbox.min.x) / self.bbox.width(),
            (p.y - self.bbox.min.y) / self.bbox.height(),
        )
    }

    /// The point of the original box at a normalised position.
    pub fn denormalise(&self, u: f64, v: f64) -> Point {
        Point::new(
            self.bbox.min.x + u * self.bbox.width(),
            self.bbox.min.y + v * self.bbox.height(),
        )
    }

    /// Where the normalised position `(u, v)` lands. Positions outside
    /// `0..=1` extend the edge curves' end spans.
    pub fn map_uv(&self, u: f64, v: f64) -> Point {
        let (b, t) = (self.bottom.at(u), self.top.at(u));
        let (l, r) = (self.left.at(v), self.right.at(v));
        let (p00, p10) = (self.bottom.first(), self.bottom.last());
        let (p01, p11) = (self.top.first(), self.top.last());
        let blend = |b: f64, t: f64, l: f64, r: f64, c00: f64, c10: f64, c01: f64, c11: f64| {
            (1.0 - v) * b + v * t + (1.0 - u) * l + u * r
                - ((1.0 - u) * (1.0 - v) * c00
                    + u * (1.0 - v) * c10
                    + (1.0 - u) * v * c01
                    + u * v * c11)
        };
        Point::new(
            blend(b.x, t.x, l.x, r.x, p00.x, p10.x, p01.x, p11.x),
            blend(b.y, t.y, l.y, r.y, p00.y, p10.y, p01.y, p11.y),
        )
    }

    /// Partial derivatives of [`Envelope::map_uv`] along `u` and `v`.
    fn jacobian(&self, u: f64, v: f64) -> (Vec2, Vec2) {
        let (b, t) = (self.bottom.at(u), self.top.at(u));
        let (l, r) = (self.left.at(v), self.right.at(v));
        let (db, dt) = (self.bottom.derivative(u), self.top.derivative(u));
        let (dl, dr) = (self.left.derivative(v), self.right.derivative(v));
        let (p00, p10) = (self.bottom.first(), self.bottom.last());
        let (p01, p11) = (self.top.first(), self.top.last());
        let du = |db: f64, dt: f64, l: f64, r: f64, c00: f64, c10: f64, c01: f64, c11: f64| {
            (1.0 - v) * db + v * dt - l + r
                - (-(1.0 - v) * c00 + (1.0 - v) * c10 - v * c01 + v * c11)
        };
        let dv = |b: f64, t: f64, dl: f64, dr: f64, c00: f64, c10: f64, c01: f64, c11: f64| {
            -b + t + (1.0 - u) * dl + u * dr
                - (-(1.0 - u) * c00 - u * c10 + (1.0 - u) * c01 + u * c11)
        };
        (
            Vec2::new(
                du(db.x, dt.x, l.x, r.x, p00.x, p10.x, p01.x, p11.x),
                du(db.y, dt.y, l.y, r.y, p00.y, p10.y, p01.y, p11.y),
            ),
            Vec2::new(
                dv(b.x, t.x, dl.x, dr.x, p00.x, p10.x, p01.x, p11.x),
                dv(b.y, t.y, dl.y, dr.y, p00.y, p10.y, p01.y, p11.y),
            ),
        )
    }

    /// Where a point of the original box lands.
    pub fn map(&self, p: Point) -> Point {
        let (u, v) = self.normalise(p);
        self.map_uv(u, v)
    }

    /// The normalised position that lands on `q`, found by Newton steps from
    /// `guess`, or `None` when the steps do not settle within `tol`
    /// millimetres.
    pub fn inverse_uv(&self, q: Point, guess: (f64, f64), tol: f64) -> Option<(f64, f64)> {
        let (mut u, mut v) = guess;
        for _ in 0..24 {
            let p = self.map_uv(u, v);
            let (ex, ey) = (q.x - p.x, q.y - p.y);
            if ex.abs() <= tol && ey.abs() <= tol {
                return Some((u, v));
            }
            let (du, dv) = self.jacobian(u, v);
            let det = du.x * dv.y - dv.x * du.y;
            if det.abs() < 1e-18 {
                return None;
            }
            let step_u = (ex * dv.y - dv.x * ey) / det;
            let step_v = (du.x * ey - ex * du.y) / det;
            // Keep a step from throwing the guess far outside the patch.
            let len = step_u.hypot(step_v);
            let scale = if len > 0.5 { 0.5 / len } else { 1.0 };
            u += step_u * scale;
            v += step_v * scale;
            if !(u.is_finite() && v.is_finite()) {
                return None;
            }
        }
        let p = self.map_uv(u, v);
        (p.distance(q) <= tol * 2.0).then_some((u, v))
    }

    /// The outline of the bent patch, counter clockwise, `per_edge` points
    /// to each side.
    pub fn outline(&self, per_edge: usize) -> Vec<Point> {
        let n = per_edge.max(2);
        let mut out = Vec::with_capacity(n * 4);
        for k in 0..n {
            out.push(self.map_uv(k as f64 / n as f64, 0.0));
        }
        for k in 0..n {
            out.push(self.map_uv(1.0, k as f64 / n as f64));
        }
        for k in 0..n {
            out.push(self.map_uv(1.0 - k as f64 / n as f64, 1.0));
        }
        for k in 0..n {
            out.push(self.map_uv(0.0, 1.0 - k as f64 / n as f64));
        }
        out
    }

    /// Box round the bent patch, sampled on a `samples` by `samples` grid
    /// of normalised positions.
    pub fn mapped_bbox(&self, samples: usize) -> BBox {
        let n = samples.max(2);
        let mut bbox = BBox::from_point(self.map_uv(0.0, 0.0));
        for j in 0..=n {
            for i in 0..=n {
                bbox = bbox.with_point(self.map_uv(i as f64 / n as f64, j as f64 / n as f64));
            }
        }
        bbox
    }
}

/// Bends a vector through an envelope.
///
/// Every segment is cut into samples no further apart than a sixty fourth
/// of the envelope box's shorter side (arcs and curves are flattened within
/// `tol` first), the samples are mapped, and the segment is rebuilt from
/// them: a line when they stay within `tol` of a straight line, else a run
/// of cubic Beziers fitted within `tol`. Nodes stay nodes, so corners stay
/// corners, and closed subpaths stay closed.
///
/// # Errors
/// Returns what [`fit_cubics`] reports for a degenerate run.
pub fn distort_path(path: &Path, envelope: &Envelope, tol: Tolerance) -> Result<Path, CoreError> {
    let bbox = envelope.bbox();
    let step = (bbox.width().min(bbox.height()) / 64.0).max(tol.get());
    let fine = Tolerance::new((tol.get() / 4.0).max(1e-6))?;
    let mut out = Path::new();
    let mut scratch = (Vec::new(), Vec::new());
    for sub in &path.subpaths {
        let mut next = SubPath::open(envelope.map(sub.start));
        for (start, segment) in sub.iter() {
            bend_segment(
                start,
                segment,
                envelope,
                (step, fine, tol),
                &mut scratch,
                &mut next,
                false,
            )?;
        }
        // A closed subpath's closing line is implicit; bent, it becomes a
        // curve, which has to be spelled out.
        if sub.closed && !sub.end().approx_eq(sub.start, Tolerance::EQUALITY) {
            let closing = Segment::line(sub.start);
            bend_segment(
                sub.end(),
                &closing,
                envelope,
                (step, fine, tol),
                &mut scratch,
                &mut next,
                true,
            )?;
        }
        next.closed = sub.closed;
        out.push(next);
    }
    Ok(out)
}

/// Maps one segment and appends its rebuilt form to `next`. A closing
/// segment that stays straight is left implicit.
fn bend_segment(
    start: Point,
    segment: &Segment,
    envelope: &Envelope,
    (step, fine, tol): (f64, Tolerance, Tolerance),
    (samples, mapped): &mut (Vec<Point>, Vec<Point>),
    next: &mut SubPath,
    closing: bool,
) -> Result<(), CoreError> {
    samples.clear();
    samples.push(start);
    flatten_segment_into(start, segment, fine, samples);
    densify(samples, step);
    mapped.clear();
    mapped.extend(samples.iter().map(|p| envelope.map(*p)));
    let end = mapped[mapped.len() - 1];
    if within_chord(mapped, tol.get()) {
        if !closing {
            next.line_to(end);
        }
        return Ok(());
    }
    let fitted = fit_cubics(mapped, tol)?;
    let count = fitted.segments.len();
    for (k, mut piece) in fitted.segments.into_iter().enumerate() {
        if k + 1 == count {
            piece.set_end(end);
        }
        next.segments.push(piece);
    }
    Ok(())
}

/// Inserts points so no two neighbours are further apart than `step`,
/// without letting one run grow past [`MAX_SEGMENT_SAMPLES`].
fn densify(points: &mut Vec<Point>, step: f64) {
    let total: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let step = step.max(total / MAX_SEGMENT_SAMPLES as f64);
    if step <= 0.0 || points.len() < 2 {
        return;
    }
    let source = std::mem::take(points);
    points.push(source[0]);
    for pair in source.windows(2) {
        let pieces = (pair[0].distance(pair[1]) / step).ceil().max(1.0) as usize;
        for k in 1..=pieces {
            points.push(pair[0].lerp(pair[1], k as f64 / pieces as f64));
        }
    }
}

/// True when every point is within `tol` of the chord from first to last.
fn within_chord(points: &[Point], tol: f64) -> bool {
    let (a, b) = (points[0], points[points.len() - 1]);
    let chord = a.to(b);
    let len = chord.length();
    if len <= 1e-12 {
        return points.iter().all(|p| p.distance(a) <= tol);
    }
    points.iter().all(|p| {
        let along = a.to(*p).dot(chord) / len;
        (-tol..=len + tol).contains(&along) && (chord.cross(a.to(*p)) / len).abs() <= tol
    })
}

/// Nodes each edge gets when it is taken from a curve (Along a Single
/// Curve, Between Two Curves).
pub const CURVE_EDGE_NODES: usize = MAX_EDGE_NODES;

/// `count` points spread evenly by length along an open curve, from its
/// left end: a curve drawn right to left is read the other way, so the
/// object keeps reading left to right.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a closed curve, one of several
/// parts, or one without length.
pub fn curve_nodes(curve: &Path, count: usize, tol: Tolerance) -> Result<Vec<Point>, CoreError> {
    let [sub] = curve.subpaths.as_slice() else {
        return Err(CoreError::validation(
            "a distortion curve must be one open vector",
        ));
    };
    if sub.closed {
        return Err(CoreError::validation(
            "a distortion curve must be an open vector",
        ));
    }
    let points = super::flatten::flatten_subpath(sub, tol).points;
    let mut along = Vec::with_capacity(points.len());
    let mut total = 0.0;
    along.push(0.0);
    for pair in points.windows(2) {
        total += pair[0].distance(pair[1]);
        along.push(total);
    }
    if total <= 1e-9 {
        return Err(CoreError::validation("the distortion curve has no length"));
    }
    let count = count.clamp(2, MAX_EDGE_NODES);
    let mut nodes = Vec::with_capacity(count);
    let mut k = 0;
    for i in 0..count {
        let want = total * i as f64 / (count - 1) as f64;
        while k + 2 < along.len() && along[k + 1] < want {
            k += 1;
        }
        let span = along[k + 1] - along[k];
        let t = if span <= 1e-12 {
            0.0
        } else {
            ((want - along[k]) / span).clamp(0.0, 1.0)
        };
        nodes.push(points[k].lerp(points[k + 1], t));
    }
    if nodes[0].x > nodes[count - 1].x {
        nodes.reverse();
    }
    Ok(nodes)
}

/// The bottom and top edges that bend a box `height` high along one curve:
/// the bottom lies on the curve and the top follows it `height` away on its
/// upper side, so the object sits on the curve, stretched along its length.
///
/// # Errors
/// As [`curve_nodes`].
pub fn single_curve_edges(
    curve: &Path,
    height: f64,
    tol: Tolerance,
) -> Result<(Vec<Point>, Vec<Point>), CoreError> {
    let bottom = curve_nodes(curve, CURVE_EDGE_NODES, tol)?;
    let n = bottom.len();
    let top = (0..n)
        .map(|i| {
            let before = bottom[i.saturating_sub(1)];
            let after = bottom[(i + 1).min(n - 1)];
            let normal = before
                .to(after)
                .normalised()
                .map_or(Vec2::new(0.0, 1.0), |d| d.perp());
            let p = bottom[i];
            Point::new(p.x + normal.x * height, p.y + normal.y * height)
        })
        .collect();
    Ok((bottom, top))
}

/// The bottom and top edges that bend a box between two curves: the lower
/// curve (by its middle height) is the bottom.
///
/// # Errors
/// As [`curve_nodes`].
pub fn two_curve_edges(
    a: &Path,
    b: &Path,
    tol: Tolerance,
) -> Result<(Vec<Point>, Vec<Point>), CoreError> {
    let first = curve_nodes(a, CURVE_EDGE_NODES, tol)?;
    let second = curve_nodes(b, CURVE_EDGE_NODES, tol)?;
    let middle = |nodes: &[Point]| nodes.iter().map(|p| p.y).sum::<f64>() / nodes.len() as f64;
    if middle(&first) <= middle(&second) {
        Ok((first, second))
    } else {
        Ok((second, first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::flatten::flatten_path;

    fn bbox() -> BBox {
        BBox::new(Point::new(10.0, 20.0), Point::new(110.0, 70.0))
    }

    /// Bottom edge left alone, top edge bowed up by `lift` in the middle.
    fn arched(lift: f64) -> Envelope {
        let b = bbox();
        Envelope::new(
            b,
            vec![b.min, Point::new(60.0, 20.0), Point::new(b.max.x, b.min.y)],
            vec![
                Point::new(b.min.x, b.max.y),
                Point::new(60.0, 70.0 + lift),
                b.max,
            ],
            None,
            None,
        )
        .expect("envelope")
    }

    fn rect(b: BBox) -> Path {
        let mut sub = SubPath::open(b.min);
        sub.line_to(Point::new(b.max.x, b.min.y))
            .line_to(b.max)
            .line_to(Point::new(b.min.x, b.max.y));
        sub.closed = true;
        Path::from_subpath(sub)
    }

    #[test]
    fn straight_nodes_give_a_straight_curve() {
        let curve = EdgeCurve::straight(Point::new(0.0, 0.0), Point::new(9.0, 3.0), 4);
        for k in 0..=20 {
            let t = k as f64 / 20.0;
            let p = curve.at(t);
            assert!((p.x - 9.0 * t).abs() < 1e-12 && (p.y - 3.0 * t).abs() < 1e-12);
            let d = curve.derivative(t);
            assert!((d.x - 9.0).abs() < 1e-9 && (d.y - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_curve_passes_through_its_nodes() {
        let nodes = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 4.0),
            Point::new(3.0, -2.0),
            Point::new(5.0, 1.0),
        ];
        let curve = EdgeCurve::new(nodes.clone()).expect("curve");
        for (k, node) in nodes.iter().enumerate() {
            let p = curve.at(k as f64 / 3.0);
            assert!(p.distance(*node) < 1e-12, "node {k}");
        }
    }

    #[test]
    fn the_curve_derivative_matches_differences() {
        let curve = EdgeCurve::new(vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 5.0),
            Point::new(6.0, 1.0),
        ])
        .expect("curve");
        for t in [0.1, 0.3, 0.7, 0.9] {
            let h = 1e-6;
            let (a, b) = (curve.at(t - h), curve.at(t + h));
            let d = curve.derivative(t);
            assert!(((b.x - a.x) / (2.0 * h) - d.x).abs() < 1e-4);
            assert!(((b.y - a.y) / (2.0 * h) - d.y).abs() < 1e-4);
        }
    }

    #[test]
    fn the_identity_envelope_maps_points_to_themselves() {
        let envelope = Envelope::identity(bbox(), 5);
        for p in [
            Point::new(10.0, 20.0),
            Point::new(37.5, 61.25),
            Point::new(110.0, 70.0),
            Point::new(80.0, 25.0),
        ] {
            assert!(envelope.map(p).distance(p) < 1e-9);
        }
    }

    #[test]
    fn the_patch_meets_every_edge_curve() {
        let b = bbox();
        let envelope = Envelope::new(
            b,
            vec![b.min, Point::new(60.0, 10.0), Point::new(b.max.x, b.min.y)],
            vec![Point::new(b.min.x, b.max.y), Point::new(60.0, 90.0), b.max],
            Some(vec![
                b.min,
                Point::new(0.0, 45.0),
                Point::new(b.min.x, b.max.y),
            ]),
            Some(vec![
                Point::new(b.max.x, b.min.y),
                Point::new(130.0, 45.0),
                b.max,
            ]),
        )
        .expect("envelope");
        assert!(envelope.map_uv(0.5, 0.0).distance(Point::new(60.0, 10.0)) < 1e-9);
        assert!(envelope.map_uv(0.5, 1.0).distance(Point::new(60.0, 90.0)) < 1e-9);
        assert!(envelope.map_uv(0.0, 0.5).distance(Point::new(0.0, 45.0)) < 1e-9);
        assert!(envelope.map_uv(1.0, 0.5).distance(Point::new(130.0, 45.0)) < 1e-9);
        assert!(envelope.map_uv(1.0, 1.0).distance(b.max) < 1e-12);
    }

    #[test]
    fn side_curve_ends_snap_to_the_corners() {
        let b = bbox();
        let envelope = Envelope::new(
            b,
            vec![b.min, Point::new(b.max.x, b.min.y)],
            vec![Point::new(b.min.x, b.max.y), b.max],
            Some(vec![Point::new(-50.0, -50.0), Point::new(-60.0, 200.0)]),
            None,
        )
        .expect("envelope");
        assert!(envelope.map(b.min).distance(b.min) < 1e-12);
        let top_left = Point::new(b.min.x, b.max.y);
        assert!(envelope.map(top_left).distance(top_left) < 1e-12);
    }

    #[test]
    fn an_arch_lifts_the_middle_and_leaves_the_bottom() {
        let envelope = arched(20.0);
        let top_mid = envelope.map(Point::new(60.0, 70.0));
        assert!((top_mid.y - 90.0).abs() < 1e-9);
        let bottom_mid = envelope.map(Point::new(60.0, 20.0));
        assert!((bottom_mid.y - 20.0).abs() < 1e-9);
        let centre = envelope.map(Point::new(60.0, 45.0));
        assert!((centre.y - 55.0).abs() < 1e-9);
    }

    #[test]
    fn the_inverse_undoes_the_map() {
        let envelope = arched(15.0);
        for (u, v) in [(0.1, 0.2), (0.5, 0.5), (0.9, 0.95), (0.0, 1.0)] {
            let q = envelope.map_uv(u, v);
            let (iu, iv) = envelope.inverse_uv(q, (0.5, 0.5), 1e-9).expect("inverse");
            assert!((iu - u).abs() < 1e-7 && (iv - v).abs() < 1e-7);
        }
    }

    #[test]
    fn bad_envelopes_are_refused() {
        let b = bbox();
        assert!(Envelope::new(b, vec![b.min], vec![b.max, b.max], None, None).is_err());
        let flat = BBox::new(Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        assert!(Envelope::new(
            flat,
            vec![flat.min, flat.max],
            vec![flat.min, flat.max],
            None,
            None
        )
        .is_err());
        assert!(Envelope::new(
            b,
            vec![b.min, Point::new(f64::NAN, 0.0)],
            vec![b.min, b.max],
            None,
            None
        )
        .is_err());
    }

    #[test]
    fn the_identity_keeps_a_rectangle_straight() {
        let b = bbox();
        let tol = Tolerance::new(0.01).expect("tol");
        let out = distort_path(&rect(b), &Envelope::identity(b, 4), tol).expect("distort");
        let sub = &out.subpaths[0];
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3);
        assert!(sub
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
        assert!(sub.start.distance(b.min) < 1e-9);
    }

    #[test]
    fn an_arched_rectangle_follows_the_envelope() {
        let b = bbox();
        let envelope = arched(20.0);
        let tol = Tolerance::new(0.01).expect("tol");
        let out = distort_path(&rect(b), &envelope, tol).expect("distort");
        let sub = &out.subpaths[0];
        assert!(sub.closed);
        // The bottom stays a line, the top becomes curves.
        assert!(matches!(sub.segments[0], Segment::Line { .. }));
        assert!(sub
            .segments
            .iter()
            .any(|s| matches!(s, Segment::Cubic { .. })));
        // Every point of the result is near the mapped outline.
        let expected = envelope.outline(400);
        for poly in flatten_path(&out, tol) {
            for p in poly.points {
                let near = expected
                    .iter()
                    .map(|q| q.distance(p))
                    .fold(f64::INFINITY, f64::min);
                assert!(near < 0.5, "{p:?} is {near} from the envelope outline");
            }
        }
        // The top middle node of the arch is where the envelope put it.
        let top = flatten_path(&out, tol)[0]
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 90.0).abs() < 0.05);
    }

    #[test]
    fn the_implicit_closing_edge_bends_too() {
        let b = bbox();
        // Starts top right, so the closing line runs along the top.
        let mut sub = SubPath::open(b.max);
        sub.line_to(Point::new(b.max.x, b.min.y))
            .line_to(b.min)
            .line_to(Point::new(b.min.x, b.max.y));
        sub.closed = true;
        let tol = Tolerance::new(0.01).expect("tol");
        let out = distort_path(&Path::from_subpath(sub), &arched(20.0), tol).expect("distort");
        let top = flatten_path(&out, tol)[0]
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 90.0).abs() < 0.05, "top at {top}");
        assert!(out.subpaths[0].end().distance(b.max) < 1e-9);
    }

    #[test]
    fn open_subpaths_stay_open_and_keep_their_ends() {
        let mut sub = SubPath::open(Point::new(10.0, 45.0));
        sub.arc_to(Point::new(110.0, 45.0), -0.3);
        let path = Path::from_subpath(sub);
        let envelope = arched(10.0);
        let tol = Tolerance::new(0.01).expect("tol");
        let out = distort_path(&path, &envelope, tol).expect("distort");
        let bent = &out.subpaths[0];
        assert!(!bent.closed);
        assert!(bent.start.distance(envelope.map(Point::new(10.0, 45.0))) < 1e-9);
        assert!(bent.end().distance(envelope.map(Point::new(110.0, 45.0))) < 1e-9);
    }

    fn line(a: Point, b: Point) -> Path {
        let mut sub = SubPath::open(a);
        sub.segments.push(Segment::line(b));
        let mut path = Path::new();
        path.push(sub);
        path
    }

    #[test]
    fn curve_nodes_run_left_to_right_evenly() {
        let tol = Tolerance::default();
        let nodes = curve_nodes(&line(Point::new(100.0, 5.0), Point::new(0.0, 5.0)), 5, tol)
            .expect("nodes");
        let xs: Vec<f64> = nodes.iter().map(|p| p.x).collect();
        for (x, want) in xs.iter().zip([0.0, 25.0, 50.0, 75.0, 100.0]) {
            assert!((x - want).abs() < 1e-9, "{xs:?}");
        }
        let mut closed = line(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        closed.subpaths[0].closed = true;
        assert!(curve_nodes(&closed, 5, tol).is_err());
    }

    #[test]
    fn a_box_bent_along_a_straight_curve_sits_on_it() {
        let tol = Tolerance::default();
        let curve = line(Point::new(0.0, 100.0), Point::new(200.0, 100.0));
        let (bottom, top) = single_curve_edges(&curve, 50.0, tol).expect("edges");
        let env = Envelope::new(bbox(), bottom, top, None, None).expect("envelope");
        // The box's bottom left lands on the curve's start, its top right
        // above the curve's end, the box stretched to the curve's length.
        let bl = env.map(bbox().min);
        let tr = env.map(bbox().max);
        assert!(bl.distance(Point::new(0.0, 100.0)) < 1e-6, "{bl:?}");
        assert!(tr.distance(Point::new(200.0, 150.0)) < 1e-6, "{tr:?}");
    }

    #[test]
    fn two_curves_take_the_lower_as_the_bottom() {
        let tol = Tolerance::default();
        let high = line(Point::new(0.0, 80.0), Point::new(100.0, 90.0));
        let low = line(Point::new(100.0, 0.0), Point::new(0.0, 10.0));
        let (bottom, top) = two_curve_edges(&high, &low, tol).expect("edges");
        assert!((bottom[0].y - 10.0).abs() < 1e-9 && bottom[0].x.abs() < 1e-9);
        assert!((top[0].y - 80.0).abs() < 1e-9);
        let env = Envelope::new(bbox(), bottom, top, None, None).expect("envelope");
        let mid = env.map(Point::new(60.0, 45.0));
        assert!(
            (mid.x - 50.0).abs() < 1e-6 && (mid.y - 45.0).abs() < 1e-6,
            "{mid:?}"
        );
    }
    #[test]
    fn line_spans_run_straight_between_their_nodes() {
        let b = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 50.0));
        // The bottom edge dips at its middle node: a curve through it bends
        // smoothly, straight spans make a sharp V.
        let bottom = vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, -20.0),
            Point::new(100.0, 0.0),
        ];
        let top = vec![Point::new(0.0, 50.0), Point::new(100.0, 50.0)];
        let curved = Envelope::new(b, bottom.clone(), top.clone(), None, None).expect("curved");
        let lines = EdgeLines {
            bottom: vec![true, true],
            ..EdgeLines::default()
        };
        let straight = Envelope::new(b, bottom, top, None, None)
            .expect("envelope")
            .with_lines(&lines)
            .expect("lines");
        // A quarter of the way along: on the straight span from (0, 0) to
        // (50, -20), below it on the curve.
        let q = straight.map(Point::new(25.0, 0.0));
        assert!(
            (q.x - 25.0).abs() < 1e-9 && (q.y + 10.0).abs() < 1e-9,
            "{q:?}"
        );
        let c = curved.map(Point::new(25.0, 0.0));
        assert!(c.y < -10.5, "{c:?}");
        // One flag per span, or none.
        let wrong = EdgeLines {
            bottom: vec![true],
            ..EdgeLines::default()
        };
        assert!(curved.clone().with_lines(&wrong).is_err());
        assert!(EdgeLines::default().is_empty() && !lines.is_empty());
        // The derivative of a line span is its direction times the spans.
        let d = EdgeCurve::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ])
        .expect("curve")
        .with_lines(&[true, false])
        .expect("lines")
        .derivative(0.25);
        assert!((d.x - 20.0).abs() < 1e-9 && d.y.abs() < 1e-9, "{d:?}");
    }
}
