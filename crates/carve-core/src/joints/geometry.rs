//! Plane geometry the joints are built from: edge frames, rectangles in an
//! edge's frame, booleans of rings and the corner reliefs a round tool
//! needs. Rings are counter-clockwise lists of points in job millimetres;
//! a shape is an outer ring followed by its holes (clockwise).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use crate::error::CoreError;
use crate::geom::{flatten_path, Path, Point, SubPath, Tolerance};

pub use crate::assembly::triangulate::signed_area2;

/// A point in job millimetres.
pub type P2 = [f64; 2];

/// Polygons with holes: each shape is an outer ring then its holes.
pub type Shapes = Vec<Vec<Vec<P2>>>;

/// Chord tolerance of relief circles and flattened outlines, in millimetres.
pub const ARC_TOLERANCE: f64 = 0.01;

/// Corners turning less than this (as the sine of the turn) get no relief.
const CORNER: f64 = 0.5;

/// Two points closer than this are the same point.
const SAME: f64 = 1e-3;

/// A relief circle is this much larger than the tool, so the tool always
/// fits in it.
pub const RELIEF_EXTRA: f64 = 0.05;

pub fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: P2, k: f64) -> P2 {
    [a[0] * k, a[1] * k]
}

pub fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub fn cross2(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub fn length(a: P2) -> f64 {
    dot(a, a).sqrt()
}

pub fn unit(v: P2) -> P2 {
    let len = length(v);
    if len < 1e-12 {
        [0.0, 0.0]
    } else {
        scale(v, 1.0 / len)
    }
}

/// A straight edge of a part with the part's material on the inside: `u`
/// runs along the edge from `a`, `n` points out of the part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub a: P2,
    pub u: P2,
    pub n: P2,
    pub len: f64,
}

impl Frame {
    /// The frame of the edge `from` to `to`, `n` pointing away from the
    /// material of `rings` (the part's outline rings).
    pub fn of_edge(from: P2, to: P2, rings: &[Vec<P2>]) -> Frame {
        let d = sub(to, from);
        let len = length(d);
        let u = unit(d);
        // Right of the direction; turned round when the part lies there.
        // Probed beside the edge's middle, else further along where a notch
        // or an arch leaves the middle with no part on either side.
        let right = [u[1], -u[0]];
        let on_right = std::iter::once(0.5)
            .chain((1..64).map(|k| f64::from(k) / 64.0))
            .map(|f| add(from, scale(d, f)))
            .find_map(|p| {
                let r = inside_rings(rings, add(p, scale(right, 0.05)));
                let l = inside_rings(rings, add(p, scale(right, -0.05)));
                (r != l).then_some(r)
            })
            .unwrap_or(false);
        if on_right {
            // The part lies on the right: run the edge the other way, so
            // every frame has the part on its left (counter-clockwise).
            let u = [-u[0], -u[1]];
            let n = [u[1], -u[0]];
            return Frame { a: to, u, n, len };
        }
        Frame {
            a: from,
            u,
            n: right,
            len,
        }
    }

    /// The job point at `s` along the edge and `t` out of the part.
    pub fn at(&self, s: f64, t: f64) -> P2 {
        [
            self.a[0] + self.u[0] * s + self.n[0] * t,
            self.a[1] + self.u[1] * s + self.n[1] * t,
        ]
    }

    /// `(s, t)` of a job point.
    pub fn local(&self, p: P2) -> P2 {
        let d = sub(p, self.a);
        [dot(d, self.u), dot(d, self.n)]
    }

    /// The rectangle `s0..s1` along the edge and `t0..t1` out of the part,
    /// counter-clockwise.
    pub fn rect(&self, s0: f64, s1: f64, t0: f64, t1: f64) -> Vec<P2> {
        ccw(vec![
            self.at(s0, t0),
            self.at(s1, t0),
            self.at(s1, t1),
            self.at(s0, t1),
        ])
    }

    /// A ring given as `(s, t)` points, counter-clockwise in the job.
    pub fn ring(&self, points: &[P2]) -> Vec<P2> {
        ccw(points.iter().map(|p| self.at(p[0], p[1])).collect())
    }
}

/// `ring` counter-clockwise.
pub fn ccw(mut ring: Vec<P2>) -> Vec<P2> {
    if signed_area2(&ring) < 0.0 {
        ring.reverse();
    }
    ring
}

/// Whether `p` lies inside `ring` (even-odd).
pub fn inside_ring(ring: &[P2], p: P2) -> bool {
    let n = ring.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let a = ring[i];
        let b = ring[j];
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Whether `p` lies inside the material of `rings` (even-odd over all).
pub fn inside_rings(rings: &[Vec<P2>], p: P2) -> bool {
    rings.iter().filter(|r| inside_ring(r, p)).count() % 2 == 1
}

/// Boolean of two ring sets (non-zero fill), outer rings counter-clockwise
/// and holes clockwise.
pub fn overlay(subject: &[Vec<P2>], clip: &[Vec<P2>], rule: OverlayRule) -> Shapes {
    if subject.is_empty() {
        return Vec::new();
    }
    let subject: Vec<Vec<P2>> = subject.to_vec();
    let clip: Vec<Vec<P2>> = clip.to_vec();
    let mut shapes = subject.overlay(&clip, rule, FillRule::NonZero);
    for shape in &mut shapes {
        for (i, ring) in shape.iter_mut().enumerate() {
            let is_ccw = signed_area2(ring) > 0.0;
            if is_ccw != (i == 0) {
                ring.reverse();
            }
        }
    }
    shapes
}

/// The union of rings as shapes.
pub fn union(rings: &[Vec<P2>]) -> Shapes {
    match rings.split_first() {
        None => Vec::new(),
        Some((first, rest)) => overlay(std::slice::from_ref(first), rest, OverlayRule::Union),
    }
}

/// Every ring of `shapes`, outer rings and holes alike.
pub fn all_rings(shapes: &Shapes) -> Vec<Vec<P2>> {
    shapes.iter().flatten().cloned().collect()
}

/// A circle as a polygon within [`ARC_TOLERANCE`].
pub fn disc(c: P2, r: f64) -> Vec<P2> {
    let step = 2.0 * (1.0 - (ARC_TOLERANCE / r).min(1.0)).acos();
    let n = ((std::f64::consts::TAU / step.max(1e-3)).ceil() as usize).clamp(24, 256);
    (0..n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
}

/// The rings of a path, flattened. Open subpaths are left out.
pub fn path_rings(path: &Path) -> Vec<Vec<P2>> {
    flatten_path(path, Tolerance::FLATTEN)
        .into_iter()
        .filter(|line| line.closed && line.points.len() >= 3)
        .map(|line| {
            let mut ring: Vec<P2> = line.points.iter().map(|p| [p.x, p.y]).collect();
            // A closed polyline may repeat its first point.
            if ring.len() > 3 && same(ring[0], ring[ring.len() - 1]) {
                ring.pop();
            }
            ring
        })
        .collect()
}

/// `ring` without points that repeat the one before or lie on the line
/// through their neighbors.
pub fn tidy(ring: &[P2]) -> Vec<P2> {
    let mut out: Vec<P2> = Vec::with_capacity(ring.len());
    for p in ring {
        if out.last().is_none_or(|q| length(sub(*p, *q)) > 1e-4) {
            out.push(*p);
        }
    }
    while out.len() > 3 && length(sub(out[0], out[out.len() - 1])) <= 1e-4 {
        out.pop();
    }
    let mut changed = true;
    while changed && out.len() > 3 {
        changed = false;
        let n = out.len();
        for i in 0..n {
            let a = out[(i + n - 1) % n];
            let b = out[i];
            let c = out[(i + 1) % n];
            let (ab, bc) = (sub(b, a), sub(c, b));
            if cross2(ab, bc).abs() <= 1e-9 * length(ab).max(1.0) * length(bc).max(1.0)
                && dot(ab, bc) > 0.0
            {
                out.remove(i);
                changed = true;
                break;
            }
        }
    }
    out
}

/// A path of closed polylines through each ring.
pub fn rings_path(rings: &[Vec<P2>]) -> Path {
    let tidied: Vec<Vec<P2>> = rings.iter().map(|r| tidy(r)).collect();
    let subpaths = tidied
        .iter()
        .filter(|r| r.len() >= 3)
        .map(|ring| {
            let mut sub = SubPath::open(Point::new(ring[0][0], ring[0][1]));
            for p in &ring[1..] {
                sub.line_to(Point::new(p[0], p[1]));
            }
            sub.line_to(Point::new(ring[0][0], ring[0][1]));
            sub.closed = true;
            sub
        })
        .collect();
    Path { subpaths }
}

/// A relief circle: its centre and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub c: P2,
    pub r: f64,
}

impl Circle {
    fn holds(&self, p: P2) -> bool {
        (length(sub(p, self.c)) - self.r).abs() < 2e-3
    }

    fn polygon(&self) -> Vec<P2> {
        disc(self.c, self.r)
    }
}

/// Signed angle from `p` to `q` about `c`, counter-clockwise positive.
fn turn_about(c: P2, p: P2, q: P2) -> f64 {
    let (a, b) = (sub(p, c), sub(q, c));
    cross2(a, b).atan2(dot(a, b))
}

/// A path through the rings in which each run of points on one of
/// `circles` is a true arc, so offsets and toolpaths follow the circle.
pub fn arcs_path(rings: &[Vec<P2>], circles: &[Circle]) -> Path {
    let mut subpaths = Vec::new();
    for ring in rings {
        let ring = tidy(ring);
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let on: Vec<Option<usize>> = ring
            .iter()
            .map(|p| circles.iter().position(|c| c.holds(*p)))
            .collect();
        // Segment i (ring[i] to ring[i + 1]) is part of an arc of circle k.
        let arc_of = |i: usize| -> Option<usize> {
            let j = (i + 1) % n;
            match (on[i], on[j]) {
                (Some(k), Some(l)) if k == l => {
                    let step = turn_about(circles[k].c, ring[i], ring[j]).abs();
                    (step < 35f64.to_radians()).then_some(k)
                }
                _ => None,
            }
        };
        // Start where an arc run begins or a straight segment is.
        let start = (0..n)
            .find(|&i| arc_of((i + n - 1) % n).is_none() || arc_of(i).is_none())
            .unwrap_or(0);
        let mut sub_path = SubPath::open(Point::new(ring[start][0], ring[start][1]));
        let mut i = start;
        let mut done = 0;
        while done < n {
            let j = (i + 1) % n;
            match arc_of(i) {
                Some(k) => {
                    let c = circles[k].c;
                    let mut sweep = turn_about(c, ring[i], ring[j]);
                    let mut end = j;
                    done += 1;
                    while done < n && arc_of(end) == Some(k) {
                        let next = (end + 1) % n;
                        let more = turn_about(c, ring[end], ring[next]);
                        if (sweep + more).abs() > 170f64.to_radians() {
                            break;
                        }
                        sweep += more;
                        end = next;
                        done += 1;
                    }
                    sub_path.arc_to(Point::new(ring[end][0], ring[end][1]), (sweep / 4.0).tan());
                    i = end;
                }
                None => {
                    sub_path.line_to(Point::new(ring[j][0], ring[j][1]));
                    i = j;
                    done += 1;
                }
            }
        }
        sub_path.closed = true;
        subpaths.push(sub_path);
    }
    Path { subpaths }
}

/// An open polyline path through `points`.
pub fn line_path(points: &[P2]) -> Path {
    let mut sub = SubPath::open(Point::new(points[0][0], points[0][1]));
    for p in &points[1..] {
        sub.line_to(Point::new(p[0], p[1]));
    }
    Path {
        subpaths: vec![sub],
    }
}

pub fn same(a: P2, b: P2) -> bool {
    (a[0] - b[0]).abs() < SAME && (a[1] - b[1]).abs() < SAME
}

/// How the inside corners a round tool cannot cut are relieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliefShape {
    None,
    /// A circle along the corner's bisector.
    DogBone,
    /// A circle along the wall across the way the parts go together, so the
    /// faces the other part bears on stay straight.
    TBone,
}

/// A corner of a ring: the vertex, the sine of its turn (positive left) and
/// the unit directions of the wall coming in and going out.
fn corners(ring: &[P2]) -> Vec<(P2, f64, P2, P2)> {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let v = ring[i];
            let p = ring[(i + n - 1) % n];
            let q = ring[(i + 1) % n];
            let e1 = unit(sub(v, p));
            let e2 = unit(sub(q, v));
            (v, cross2(e1, e2), e1, e2)
        })
        .collect()
}

/// The centre of the relief circle of radius `r` at corner `v`, whose walls
/// run along `e1` into it and `e2` out of it, the cavity on the side `side`
/// (+1 left of the walls, -1 right). `along` is the way the parts go
/// together (for the T-bone).
fn relief_centre(v: P2, e1: P2, e2: P2, side: f64, r: f64, shape: ReliefShape, along: P2) -> P2 {
    match shape {
        ReliefShape::TBone => {
            // Walls leave the corner along -e1 and e2; the centre sits on
            // the wall most across the way in, `r` from the corner.
            let back = [-e1[0], -e1[1]];
            let wall = if dot(back, along).abs() <= dot(e2, along).abs() {
                back
            } else {
                e2
            };
            add(v, scale(wall, r))
        }
        _ => {
            // The bisector of the two walls leaving the corner points into
            // the smaller angle between them, which is the cavity.
            let bis = unit(sub(e2, e1));
            let bis = if bis == [0.0, 0.0] {
                [-e1[1] * side, e1[0] * side]
            } else {
                bis
            };
            add(v, scale(bis, r))
        }
    }
}

/// Relief circles at the inside corners of material rings that `keep`
/// accepts. `rings` are the rings of one shape (outer ccw, holes cw), so
/// the material is always on the left.
pub fn material_reliefs(
    rings: &[Vec<P2>],
    r: f64,
    shape: ReliefShape,
    along: P2,
    keep: impl Fn(P2) -> bool,
) -> Vec<Circle> {
    if shape == ReliefShape::None || r <= 0.0 {
        return Vec::new();
    }
    let rr = r + RELIEF_EXTRA;
    let mut discs = Vec::new();
    for ring in rings {
        for (v, turn, e1, e2) in corners(ring) {
            // Material on the left: a right turn is an inside corner.
            if turn < -CORNER && keep(v) {
                discs.push(Circle {
                    c: relief_centre(v, e1, e2, -1.0, rr, shape, along),
                    r: rr,
                });
            }
        }
    }
    discs
}

/// Relief circles at the corners of a cavity ring (counter-clockwise, the
/// cavity on the left).
pub fn cavity_reliefs(ring: &[P2], r: f64, shape: ReliefShape, along: P2) -> Vec<Circle> {
    if shape == ReliefShape::None || r <= 0.0 {
        return Vec::new();
    }
    let rr = r + RELIEF_EXTRA;
    corners(ring)
        .into_iter()
        .filter(|(_, turn, _, _)| *turn > CORNER)
        .map(|(v, _, e1, e2)| Circle {
            c: relief_centre(v, e1, e2, 1.0, rr, shape, along),
            r: rr,
        })
        .collect()
}

/// A cavity (hole or pocket) with the relief `circles` joined on, as one
/// ring.
pub fn with_circles(ring: Vec<P2>, circles: &[Circle]) -> Vec<P2> {
    if circles.is_empty() {
        return ring;
    }
    let discs: Vec<Vec<P2>> = circles.iter().map(Circle::polygon).collect();
    let shapes = overlay(std::slice::from_ref(&ring), &discs, OverlayRule::Union);
    match shapes.as_slice() {
        [one] if !one.is_empty() => one[0].clone(),
        _ => ring,
    }
}

/// A cavity (hole or pocket) with every corner relieved: its ring and the
/// relief circles.
pub fn relieved_cavity(
    ring: Vec<P2>,
    r: f64,
    shape: ReliefShape,
    along: P2,
) -> (Vec<P2>, Vec<Circle>) {
    let circles = cavity_reliefs(&ring, r, shape, along);
    (with_circles(ring, &circles), circles)
}

/// A part's outline rings and the relief circles its arcs run on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outline {
    pub rings: Vec<Vec<P2>>,
    pub circles: Vec<Circle>,
}

/// `current` (the rings of one part) with `add` joined on and `cut` taken
/// away, and a relief in each inside corner that was not there before.
///
/// # Errors
/// [`CoreError::Validation`] when the part would fall into pieces.
pub fn rewrite(
    current: &[Vec<P2>],
    add: &[Vec<P2>],
    cut: &[Vec<P2>],
    r: f64,
    shape: ReliefShape,
    along: P2,
    what: &str,
) -> Result<Outline, CoreError> {
    let mut shapes: Shapes = union(current);
    if !add.is_empty() {
        shapes = overlay(&all_rings(&shapes), add, OverlayRule::Union);
    }
    if !cut.is_empty() {
        shapes = overlay(&all_rings(&shapes), cut, OverlayRule::Difference);
    }
    let shape_rings = one_shape(shapes, what)?;
    let old: Vec<P2> = current.iter().flatten().copied().collect();
    let is_new = |v: P2| !old.iter().any(|o| same(*o, v));
    let circles = material_reliefs(&shape_rings, r, shape, along, is_new);
    if circles.is_empty() {
        return Ok(Outline {
            rings: shape_rings,
            circles,
        });
    }
    let discs: Vec<Vec<P2>> = circles.iter().map(Circle::polygon).collect();
    let rings = one_shape(overlay(&shape_rings, &discs, OverlayRule::Difference), what)?;
    Ok(Outline { rings, circles })
}

/// The rings of the only shape, or an error naming `what`.
pub fn one_shape(shapes: Shapes, what: &str) -> Result<Vec<Vec<P2>>, CoreError> {
    let mut shapes: Vec<Vec<Vec<P2>>> = shapes
        .into_iter()
        .filter(|s| !s.is_empty() && signed_area2(&s[0]).abs() > 1e-4)
        .collect();
    match shapes.len() {
        1 => Ok(shapes.remove(0)),
        0 => Err(CoreError::validation(format!(
            "the joint would cut away all of {what}; use a smaller joint"
        ))),
        _ => Err(CoreError::validation(format!(
            "the joint would cut {what} into pieces; use a shallower joint or pick other edges"
        ))),
    }
}

/// Twice the area of the material of `rings` (outer rings add, holes take away).
pub fn area2(rings: &[Vec<P2>]) -> f64 {
    rings.iter().map(|r| signed_area2(r)).sum()
}

/// The run of material of `rings` along the line `t` out of `frame`'s edge
/// (negative is inside the part): the `s` where it starts and ends, the run
/// holding `near` when there is one, else the longest.
pub fn chord(rings: &[Vec<P2>], frame: &Frame, t: f64, near: f64) -> Option<(f64, f64)> {
    let p = frame.at(0.0, t);
    let d = frame.u;
    let mut cuts: Vec<f64> = Vec::new();
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let e = sub(b, a);
            let den = cross2(d, e);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = sub(a, p);
            let s = cross2(w, e) / den;
            let k = cross2(w, d) / den;
            if (0.0..1.0).contains(&k) {
                cuts.push(s);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let runs: Vec<(f64, f64)> = cuts
        .windows(2)
        .filter(|w| w[1] - w[0] > 1e-6)
        .map(|w| (w[0], w[1]))
        .filter(|(a, b)| inside_rings(rings, frame.at((a + b) / 2.0, t)))
        .collect();
    runs.iter()
        .copied()
        .find(|(a, b)| (*a..=*b).contains(&near))
        .or_else(|| {
            runs.iter()
                .copied()
                .max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
        })
}

/// A counter-clockwise ring of straight edges with every edge moved `d` to
/// its left (inwards for `d` > 0) and the corners where the moved edges
/// meet.
pub fn offset_polygon(ring: &[P2], d: f64) -> Vec<P2> {
    let n = ring.len();
    let line = |i: usize| {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let u = unit(sub(b, a));
        let left = [-u[1], u[0]];
        (add(a, scale(left, d)), u)
    };
    (0..n)
        .map(|i| {
            let (p, e) = line((i + n - 1) % n);
            let (q, f) = line(i);
            let den = cross2(e, f);
            if den.abs() < 1e-12 {
                return q;
            }
            let k = cross2(sub(q, p), f) / den;
            add(p, scale(e, k))
        })
        .collect()
}

/// A counter-clockwise ring of straight edges with each corner rounded:
/// outside corners with radius `convex`, inside corners with `concave`
/// (0 leaves it sharp). Answers the ring and the circles its arcs run on.
pub fn fillet_polygon(ring: &[P2], convex: f64, concave: f64) -> (Vec<P2>, Vec<Circle>) {
    let n = ring.len();
    let mut out = Vec::new();
    let mut circles = Vec::new();
    for i in 0..n {
        let v = ring[i];
        let p = ring[(i + n - 1) % n];
        let q = ring[(i + 1) % n];
        let e1 = unit(sub(v, p));
        let e2 = unit(sub(q, v));
        let turn = cross2(e1, e2);
        let r = if turn > 1e-9 { convex } else { concave };
        if r <= 0.0 || turn.abs() < 1e-9 {
            out.push(v);
            continue;
        }
        let phi = turn.atan2(dot(e1, e2)).abs();
        let interior = std::f64::consts::PI - phi;
        let reach = (length(sub(v, p)) / 2.0).min(length(sub(q, v)) / 2.0);
        let tl = (r / (interior / 2.0).tan()).min(reach);
        let r = tl * (interior / 2.0).tan();
        let t1 = sub(v, scale(e1, tl));
        let t2 = add(v, scale(e2, tl));
        let side = if turn > 0.0 { 1.0 } else { -1.0 };
        let c = add(t1, scale([-e1[1], e1[0]], r * side));
        let a1 = (t1[1] - c[1]).atan2(t1[0] - c[0]);
        let mut sweep = (t2[1] - c[1]).atan2(t2[0] - c[0]) - a1;
        while sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        while sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        let steps = ((sweep.abs() / 0.1).ceil() as usize).max(2);
        for k in 0..=steps {
            let a = a1 + sweep * k as f64 / steps as f64;
            out.push([c[0] + r * a.cos(), c[1] + r * a.sin()]);
        }
        circles.push(Circle { c, r });
    }
    (out, circles)
}

/// Where a ray from `p` along `d` first leaves the material of `rings`: the
/// distance to the nearest crossing of any ring, or `None`.
pub fn ray_exit(rings: &[Vec<P2>], p: P2, d: P2) -> Option<f64> {
    let mut best: Option<f64> = None;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let e = sub(b, a);
            let den = cross2(d, e);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = sub(a, p);
            let t = cross2(w, e) / den;
            let s = cross2(w, d) / den;
            if t > 1e-6 && (-1e-9..=1.0 + 1e-9).contains(&s) {
                best = Some(best.map_or(t, |b: f64| b.min(t)));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_frame_finds_its_part_past_a_notch_in_the_middle() {
        // A 100 by 50 part with a notch 20 wide and 10 deep in the middle
        // of its top edge: the edge picked from left to right has the part
        // on its right, so the frame runs it right to left, the part on its
        // left, and points out of the part along +Y.
        let ring = vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 50.0],
            [60.0, 50.0],
            [60.0, 40.0],
            [40.0, 40.0],
            [40.0, 50.0],
            [0.0, 50.0],
        ];
        let f = Frame::of_edge([0.0, 50.0], [100.0, 50.0], std::slice::from_ref(&ring));
        assert_eq!(f.a, [100.0, 50.0]);
        assert_eq!(f.u, [-1.0, 0.0]);
        assert_eq!(f.n, [0.0, 1.0]);
        // Picked the other way round, the same frame.
        let g = Frame::of_edge([100.0, 50.0], [0.0, 50.0], &[ring]);
        assert_eq!((g.a, g.u, g.n), (f.a, f.u, f.n));
    }

    fn square() -> Vec<P2> {
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]]
    }

    #[test]
    fn an_edge_frame_points_out_of_the_part_whichever_way_it_runs() {
        let rings = vec![square()];
        let f = Frame::of_edge([0.0, 0.0], [100.0, 0.0], &rings);
        assert!((f.n[1] + 1.0).abs() < 1e-12);
        let g = Frame::of_edge([100.0, 0.0], [0.0, 0.0], &rings);
        assert!((g.n[1] + 1.0).abs() < 1e-12);
        assert_eq!(g.a, [0.0, 0.0], "turned to run with the part on its left");
        assert_eq!(f.local(f.at(20.0, -3.0)), [20.0, -3.0]);
    }

    #[test]
    fn a_notch_gets_a_dog_bone_in_each_inside_corner_only() {
        let rings = vec![square()];
        let f = Frame::of_edge([0.0, 0.0], [100.0, 0.0], &rings);
        let notch = f.rect(40.0, 60.0, -10.0, 1.0);
        let plain = rewrite(
            &rings,
            &[],
            std::slice::from_ref(&notch),
            0.0,
            ReliefShape::None,
            f.n,
            "a",
        )
        .expect("rewrite")
        .rings;
        let boned = rewrite(&rings, &[], &[notch], 3.175, ReliefShape::DogBone, f.n, "a")
            .expect("rewrite")
            .rings;
        // Two circles of r + extra, each taking away more than a quarter
        // of its area beyond the notch's corner.
        let taken = (area2(&plain) - area2(&boned)) / 2.0;
        let r = 3.175 + RELIEF_EXTRA;
        assert!(
            taken > 0.5 * r * r && taken < 2.0 * std::f64::consts::PI * r * r,
            "{taken}"
        );
        // The walls next to the corner are cut back.
        for p in [[39.95, 9.0], [41.0, 10.05]] {
            assert!(inside_rings(&plain, p));
            assert!(!inside_rings(&boned, p), "{p:?}");
        }
    }

    #[test]
    fn a_t_bone_keeps_the_notch_walls_straight() {
        let rings = vec![square()];
        let f = Frame::of_edge([0.0, 0.0], [100.0, 0.0], &rings);
        let notch = f.rect(40.0, 60.0, -10.0, 1.0);
        let boned = rewrite(&rings, &[], &[notch], 3.175, ReliefShape::TBone, f.n, "a")
            .expect("rewrite")
            .rings;
        // The relief digs in beyond the notch floor (y 10), not into the
        // walls at x 40 and 60.
        assert!(!inside_rings(&boned, [41.0, 12.0]));
        assert!(inside_rings(&boned, [39.9, 5.0]));
        assert!(inside_rings(&boned, [60.1, 5.0]));
    }

    #[test]
    fn a_cavity_is_relieved_at_every_corner() {
        let slot = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]];
        let (relieved, circles) =
            relieved_cavity(slot.clone(), 3.0, ReliefShape::DogBone, [1.0, 0.0]);
        assert_eq!(circles.len(), 4);
        // Written as a path, each relief is a true arc.
        let path = arcs_path(std::slice::from_ref(&relieved), &circles);
        let arcs = path.subpaths[0]
            .segments
            .iter()
            .filter(|s| matches!(s, crate::geom::Segment::Arc { .. }))
            .count();
        assert!((4..=8).contains(&arcs), "{arcs}");
        for c in [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]] {
            // Just outside each wall near the corner.
            let sx = if c[0] > 1.0 { 1.0 } else { -1.0 };
            let sy = if c[1] > 1.0 { 1.0 } else { -1.0 };
            for p in [[c[0] + 0.3 * sx, c[1] - sy], [c[0] - sx, c[1] + 0.3 * sy]] {
                assert!(!inside_ring(&slot, p));
                assert!(inside_ring(&relieved, p), "{c:?} {p:?}");
            }
        }
    }

    #[test]
    fn a_ray_finds_the_far_side() {
        let rings = vec![square()];
        let t = ray_exit(&rings, [30.0, 0.0], [0.0, 1.0]).expect("hit");
        assert!((t - 50.0).abs() < 1e-9);
    }
}
