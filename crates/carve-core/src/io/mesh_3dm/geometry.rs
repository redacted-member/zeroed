//! Curves and surfaces of 3DM objects: how openNURBS serialises them and
//! how they are evaluated.
//!
//! Curves: `ON_NurbsCurve`, `ON_LineCurve`, `ON_PolylineCurve`,
//! `ON_ArcCurve` and `ON_PolyCurve` (a chain of the others). Surfaces:
//! `ON_NurbsSurface`, `ON_PlaneSurface`, `ON_RevSurface` (a profile turned
//! about an axis) and `ON_SumSurface` (two curves added). NURBS control
//! points are stored homogeneous (x w, y w, z w, w) when rational, and the
//! knot vector without the two end knots classic B-spline texts add, which
//! de Boor's algorithm never reads.

use std::ops::Range;

use super::{bad, Archive, Reader};
use crate::error::CoreError;

/// A point or vector in the file's units.
pub(super) type P3 = [f64; 3];

/// Deepest nesting of curves inside curves (a polycurve of polycurves).
const MAX_DEPTH: usize = 8;
/// Highest NURBS order read (degree 15).
const MAX_ORDER: usize = 16;
/// Most control points of one curve or surface.
const MAX_CVS: usize = 4_000_000;
/// Most grid lines a surface is cut into along one direction.
pub(super) const MAX_GRID: usize = 256;
/// Most pieces a NURBS span is cut into.
const MAX_PER_SPAN: usize = 64;

/// A class UUID as 3DM files store it: the first three fields little
/// endian, from the usual written form.
pub(super) const fn wire(c: [u8; 16]) -> [u8; 16] {
    [
        c[3], c[2], c[1], c[0], c[5], c[4], c[7], c[6], c[8], c[9], c[10], c[11], c[12], c[13],
        c[14], c[15],
    ]
}

const fn on(a: u8) -> [u8; 16] {
    wire([
        0x4e, 0xd7, 0xd4, a, 0xe9, 0x47, 0x11, 0xd3, 0xbf, 0xe5, 0x00, 0x10, 0x83, 0x01, 0x22, 0xf0,
    ])
}

/// `ON_LineCurve`, {4ED7D4DB-E947-11D3-BFE5-0010830122F0}.
pub(super) const LINE: [u8; 16] = on(0xdb);
/// `ON_NurbsCurve`, {4ED7D4DD-...}.
pub(super) const NURBS_CURVE: [u8; 16] = on(0xdd);
/// `ON_NurbsSurface`, {4ED7D4DE-...}.
pub(super) const NURBS_SURFACE: [u8; 16] = on(0xde);
/// `ON_PlaneSurface`, {4ED7D4DF-...}.
pub(super) const PLANE_SURFACE: [u8; 16] = on(0xdf);
/// `ON_PolyCurve`, {4ED7D4E0-...}.
pub(super) const POLYCURVE: [u8; 16] = on(0xe0);
/// `ON_PolylineCurve`, {4ED7D4E6-...}.
pub(super) const POLYLINE: [u8; 16] = on(0xe6);
/// `ON_ArcCurve`, {CF33BE2A-09B4-11D4-BFFB-0010830122F0}.
pub(super) const ARC: [u8; 16] = wire([
    0xcf, 0x33, 0xbe, 0x2a, 0x09, 0xb4, 0x11, 0xd4, 0xbf, 0xfb, 0x00, 0x10, 0x83, 0x01, 0x22, 0xf0,
]);
/// `ON_RevSurface`, {A16220D3-163B-11D4-8000-0010830122F0}.
pub(super) const REV_SURFACE: [u8; 16] = wire([
    0xa1, 0x62, 0x20, 0xd3, 0x16, 0x3b, 0x11, 0xd4, 0x80, 0x00, 0x00, 0x10, 0x83, 0x01, 0x22, 0xf0,
]);
/// `ON_SumSurface`, {C4CD5359-446D-4690-9FF5-29059732472B}.
pub(super) const SUM_SURFACE: [u8; 16] = wire([
    0xc4, 0xcd, 0x53, 0x59, 0x44, 0x6d, 0x46, 0x90, 0x9f, 0xf5, 0x29, 0x05, 0x97, 0x32, 0x47, 0x2b,
]);

/// True for the surface classes [`Archive::surface`] reads.
pub(super) fn is_surface(uuid: &[u8]) -> bool {
    [NURBS_SURFACE, PLANE_SURFACE, REV_SURFACE, SUM_SURFACE]
        .iter()
        .any(|c| c == uuid)
}

/// A NURBS curve, or one direction of a surface: order, control point
/// count, the knots with one extra at each end, and homogeneous control
/// points.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Nurbs {
    order: usize,
    count: usize,
    knots: Vec<f64>,
    cvs: Vec<[f64; 4]>,
}

/// Knots as stored (`order + count - 2`) padded at both ends.
fn padded(stored: Vec<f64>) -> Vec<f64> {
    let first = stored.first().copied().unwrap_or(0.0);
    let last = stored.last().copied().unwrap_or(0.0);
    let mut out = Vec::with_capacity(stored.len() + 2);
    out.push(first);
    out.extend(stored);
    out.push(last);
    out
}

/// The span `k` (`knots[k] <= t < knots[k + 1]`) de Boor's algorithm
/// evaluates `t` in; the last span for `t` at or past the end.
fn span(knots: &[f64], order: usize, count: usize, t: f64) -> usize {
    let p = order - 1;
    p + knots[p + 1..count].partition_point(|k| *k <= t)
}

/// De Boor's algorithm on the `order` points of span `k`, in place.
fn de_boor(knots: &[f64], order: usize, k: usize, t: f64, d: &mut [[f64; 4]]) -> [f64; 4] {
    let p = order - 1;
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let denominator = knots[i + p + 1 - r] - knots[i];
            let alpha = if denominator.abs() > 0.0 {
                (t - knots[i]) / denominator
            } else {
                0.0
            };
            let before = d[j - 1];
            for (c, value) in d[j].iter_mut().enumerate() {
                *value = (1.0 - alpha) * before[c] + alpha * *value;
            }
        }
    }
    d[p]
}

fn dehomogenise(h: [f64; 4]) -> P3 {
    if h[3] != 0.0 && h[3] != 1.0 {
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    } else {
        [h[0], h[1], h[2]]
    }
}

impl Nurbs {
    fn domain(&self) -> [f64; 2] {
        [self.knots[self.order - 1], self.knots[self.count]]
    }

    fn at(&self, t: f64) -> P3 {
        let k = span(&self.knots, self.order, self.count, t);
        let mut d = [[0.0; 4]; MAX_ORDER];
        let p = self.order - 1;
        d[..self.order].copy_from_slice(&self.cvs[k - p..=k]);
        dehomogenise(de_boor(&self.knots, self.order, k, t, &mut d))
    }

    /// Distinct knots inside `[a, b]`, with `a` and `b`.
    fn breaks(&self, a: f64, b: f64) -> Vec<f64> {
        breaks(&self.knots[self.order - 1..=self.count], a, b)
    }

    /// Pieces per span that keep the chord within `tol`, from the control
    /// polygon's largest second difference.
    fn per_span(&self, tol: f64) -> usize {
        let points: Vec<P3> = self.cvs.iter().map(|h| dehomogenise(*h)).collect();
        pieces(self.order, second_difference(points.iter().copied()), tol)
    }

    fn params(&self, a: f64, b: f64, tol: f64) -> Vec<f64> {
        subdivide(&self.breaks(a, b), self.per_span(tol))
    }
}

/// `values` inside `(a, b)`, deduplicated, between `a` and `b`.
fn breaks(values: &[f64], a: f64, b: f64) -> Vec<f64> {
    let mut out = vec![a];
    for v in values {
        if *v > a && *v < b && out.last().is_some_and(|l| v > l) {
            out.push(*v);
        }
    }
    out.push(b);
    out
}

/// Every interval of `breaks` cut into `n` equal pieces.
fn subdivide(breaks: &[f64], n: usize) -> Vec<f64> {
    let n = n.max(1);
    let mut out = Vec::with_capacity(breaks.len() * n);
    for w in breaks.windows(2) {
        for i in 0..n {
            out.push(w[0] + (w[1] - w[0]) * i as f64 / n as f64);
        }
    }
    if let Some(last) = breaks.last() {
        out.push(*last);
    }
    out
}

fn second_difference(points: impl Iterator<Item = P3>) -> f64 {
    let mut last: [Option<P3>; 2] = [None, None];
    let mut most: f64 = 0.0;
    for p in points {
        if let [Some(a), Some(b)] = last {
            let d = [
                p[0] - 2.0 * b[0] + a[0],
                p[1] - 2.0 * b[1] + a[1],
                p[2] - 2.0 * b[2] + a[2],
            ];
            most = most.max(length(d));
        }
        last = [last[1], Some(p)];
    }
    most
}

/// Pieces per span of a polynomial of `order` whose control points bend by
/// `bend` (largest second difference) so the chord stays within `tol`.
fn pieces(order: usize, bend: f64, tol: f64) -> usize {
    if order <= 2 || bend <= 0.0 || tol <= 0.0 {
        return 1;
    }
    let p = (order - 1) as f64;
    let n = (p * (p - 1.0) / 8.0 * bend / tol).sqrt().ceil();
    (n as usize).clamp(1, MAX_PER_SPAN)
}

/// Pieces an arc of `radius` through `angle` radians needs so its chord
/// stays within `tol`.
fn arc_pieces(radius: f64, angle: f64, tol: f64) -> usize {
    let step = if radius > tol {
        2.0 * (1.0 - tol / radius).acos()
    } else {
        angle
    };
    let n = (angle.abs() / step.max(1e-3)).ceil();
    (n as usize).clamp(2, 720)
}

pub(super) fn length(v: P3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub(super) fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(super) fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn unit(v: P3) -> Option<P3> {
    let l = length(v);
    (l.is_finite() && l > 1e-12).then(|| scale(v, 1.0 / l))
}

fn lerp(domain: [f64; 2], range: [f64; 2], t: f64) -> f64 {
    let span = domain[1] - domain[0];
    if span.abs() > 0.0 {
        range[0] + (t - domain[0]) / span * (range[1] - range[0])
    } else {
        range[0]
    }
}

/// A curve of any class read.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Curve {
    Nurbs(Nurbs),
    Line {
        from: P3,
        to: P3,
        domain: [f64; 2],
    },
    Polyline {
        points: Vec<P3>,
        params: Vec<f64>,
    },
    Arc {
        centre: P3,
        x: P3,
        y: P3,
        radius: f64,
        angle: [f64; 2],
        domain: [f64; 2],
    },
    Poly {
        params: Vec<f64>,
        segments: Vec<Curve>,
    },
}

impl Curve {
    pub(super) fn domain(&self) -> [f64; 2] {
        match self {
            Curve::Nurbs(n) => n.domain(),
            Curve::Line { domain, .. } | Curve::Arc { domain, .. } => *domain,
            Curve::Polyline { params, .. } | Curve::Poly { params, .. } => {
                [params[0], params[params.len() - 1]]
            }
        }
    }

    pub(super) fn at(&self, t: f64) -> P3 {
        match self {
            Curve::Nurbs(n) => n.at(t),
            Curve::Line { from, to, domain } => {
                let s = lerp(*domain, [0.0, 1.0], t);
                add(*from, scale(sub(*to, *from), s))
            }
            Curve::Polyline { points, params } => {
                let i = params[1..params.len() - 1].partition_point(|p| *p <= t);
                let s = lerp([params[i], params[i + 1]], [0.0, 1.0], t);
                add(points[i], scale(sub(points[i + 1], points[i]), s))
            }
            Curve::Arc {
                centre,
                x,
                y,
                radius,
                angle,
                domain,
            } => {
                let a = lerp(*domain, *angle, t);
                add(
                    *centre,
                    add(scale(*x, radius * a.cos()), scale(*y, radius * a.sin())),
                )
            }
            Curve::Poly { params, segments } => {
                let i = params[1..params.len() - 1].partition_point(|p| *p <= t);
                let segment = &segments[i];
                segment.at(lerp([params[i], params[i + 1]], segment.domain(), t))
            }
        }
    }

    /// Parameters from `a` to `b` (`a < b`, inside the domain) close
    /// enough that the polyline through them stays within `tol`.
    pub(super) fn params(&self, a: f64, b: f64, tol: f64) -> Vec<f64> {
        match self {
            Curve::Nurbs(n) => n.params(a, b, tol),
            Curve::Line { .. } => vec![a, b],
            Curve::Polyline { params, .. } => breaks(params, a, b),
            Curve::Arc {
                radius,
                angle,
                domain,
                ..
            } => {
                let turn = lerp(*domain, *angle, b) - lerp(*domain, *angle, a);
                subdivide(&[a, b], arc_pieces(*radius, turn, tol))
            }
            Curve::Poly { params, segments } => {
                let mut out = vec![a];
                for (i, segment) in segments.iter().enumerate() {
                    let lo = params[i].max(a);
                    let hi = params[i + 1].min(b);
                    if hi <= lo {
                        continue;
                    }
                    let own = segment.domain();
                    let inner = segment.params(
                        lerp([params[i], params[i + 1]], own, lo),
                        lerp([params[i], params[i + 1]], own, hi),
                        tol,
                    );
                    for t in inner.into_iter().skip(1) {
                        let mapped = lerp(own, [params[i], params[i + 1]], t);
                        if out.last().is_some_and(|l| mapped > *l) {
                            out.push(mapped);
                        }
                    }
                }
                if out.last().is_some_and(|l| *l < b) {
                    out.push(b);
                }
                out
            }
        }
    }

    /// The pieces of a polycurve, or the curve itself.
    pub(super) fn pieces(self) -> Vec<Curve> {
        match self {
            Curve::Poly { segments, .. } => segments,
            other => vec![other],
        }
    }
}

/// A surface of any class read.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Surface {
    Nurbs {
        order: [usize; 2],
        count: [usize; 2],
        knots: [Vec<f64>; 2],
        /// Control points by u then v: `cvs[i * count[1] + j]`.
        cvs: Vec<[f64; 4]>,
    },
    Plane {
        origin: P3,
        x: P3,
        y: P3,
        domain: [[f64; 2]; 2],
        extents: [[f64; 2]; 2],
    },
    Rev {
        origin: P3,
        axis: P3,
        angle: [f64; 2],
        domain: [f64; 2],
        transposed: bool,
        profile: Box<Curve>,
    },
    Sum {
        base: P3,
        curves: Box<[Curve; 2]>,
    },
}

/// `p` turned by `angle` about the line through `origin` along unit `axis`.
fn rotate(p: P3, origin: P3, axis: P3, angle: f64) -> P3 {
    let v = sub(p, origin);
    let (s, c) = angle.sin_cos();
    let turned = add(
        add(scale(v, c), scale(cross(axis, v), s)),
        scale(axis, dot(axis, v) * (1.0 - c)),
    );
    add(origin, turned)
}

impl Surface {
    /// The parameter interval along direction 0 (u) or 1 (v).
    pub(super) fn domain(&self, dir: usize) -> [f64; 2] {
        match self {
            Surface::Nurbs {
                order,
                count,
                knots,
                ..
            } => [knots[dir][order[dir] - 1], knots[dir][count[dir]]],
            Surface::Plane { domain, .. } => domain[dir],
            Surface::Rev {
                domain,
                transposed,
                profile,
                ..
            } => {
                if (dir == 0) != *transposed {
                    *domain
                } else {
                    profile.domain()
                }
            }
            Surface::Sum { curves, .. } => curves[dir].domain(),
        }
    }

    pub(super) fn at(&self, u: f64, v: f64) -> P3 {
        match self {
            Surface::Nurbs {
                order,
                count,
                knots,
                cvs,
            } => {
                let ku = span(&knots[0], order[0], count[0], u);
                let kv = span(&knots[1], order[1], count[1], v);
                let (pu, pv) = (order[0] - 1, order[1] - 1);
                let mut column = [[0.0; 4]; MAX_ORDER];
                let mut row = [[0.0; 4]; MAX_ORDER];
                for (a, slot) in column.iter_mut().take(order[0]).enumerate() {
                    let i = ku - pu + a;
                    let start = i * count[1] + kv - pv;
                    row[..order[1]].copy_from_slice(&cvs[start..start + order[1]]);
                    *slot = de_boor(&knots[1], order[1], kv, v, &mut row);
                }
                dehomogenise(de_boor(&knots[0], order[0], ku, u, &mut column))
            }
            Surface::Plane {
                origin,
                x,
                y,
                domain,
                extents,
            } => {
                let s = lerp(domain[0], extents[0], u);
                let t = lerp(domain[1], extents[1], v);
                add(*origin, add(scale(*x, s), scale(*y, t)))
            }
            Surface::Rev {
                origin,
                axis,
                angle,
                domain,
                transposed,
                profile,
            } => {
                let (a, t) = if *transposed { (v, u) } else { (u, v) };
                rotate(profile.at(t), *origin, *axis, lerp(*domain, *angle, a))
            }
            Surface::Sum { base, curves } => add(*base, add(curves[0].at(u), curves[1].at(v))),
        }
    }

    /// Grid lines along direction `dir` across `[a, b]` fine enough for
    /// `tol`, at most [`MAX_GRID`] of them.
    pub(super) fn grid(&self, dir: usize, a: f64, b: f64, tol: f64) -> Vec<f64> {
        let params = match self {
            Surface::Nurbs {
                order,
                count,
                knots,
                cvs,
            } => {
                let other = 1 - dir;
                let mut bend: f64 = 0.0;
                for line in 0..count[other] {
                    let points = (0..count[dir]).map(|k| {
                        let (i, j) = if dir == 0 { (k, line) } else { (line, k) };
                        dehomogenise(cvs[i * count[1] + j])
                    });
                    bend = bend.max(second_difference(points));
                }
                let breaks = breaks(&knots[dir][order[dir] - 1..=count[dir]], a, b);
                let spans = breaks.len().saturating_sub(1).max(1);
                let per_span = pieces(order[dir], bend, tol).min((MAX_GRID / spans).max(1));
                subdivide(&breaks, per_span)
            }
            Surface::Plane { .. } => vec![a, b],
            Surface::Rev {
                origin,
                axis,
                angle,
                domain,
                transposed,
                profile,
            } => {
                if (dir == 0) != *transposed {
                    let [t0, t1] = profile.domain();
                    let radius = profile
                        .params(t0, t1, tol)
                        .into_iter()
                        .map(|t| {
                            let v = sub(profile.at(t), *origin);
                            length(sub(v, scale(*axis, dot(*axis, v))))
                        })
                        .fold(0.0, f64::max);
                    let turn = lerp(*domain, *angle, b) - lerp(*domain, *angle, a);
                    subdivide(&[a, b], arc_pieces(radius, turn, tol))
                } else {
                    profile.params(a, b, tol)
                }
            }
            Surface::Sum { curves, .. } => curves[dir].params(a, b, tol),
        };
        thin(params, MAX_GRID)
    }
}

/// At most `most` of `params`, keeping both ends.
fn thin(params: Vec<f64>, most: usize) -> Vec<f64> {
    if params.len() <= most {
        return params;
    }
    let step = params.len().div_ceil(most - 1);
    let mut out: Vec<f64> = params.iter().step_by(step).copied().collect();
    if let (Some(last), Some(end)) = (out.last().copied(), params.last()) {
        if last < *end {
            out.push(*end);
        }
    }
    out
}

impl Reader<'_> {
    pub(super) fn f64(&mut self) -> Result<f64, CoreError> {
        let b = self.take(8)?;
        let value = f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(bad("a 3DM object holds a number that is not finite"))
        }
    }

    pub(super) fn point(&mut self) -> Result<P3, CoreError> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }

    pub(super) fn interval(&mut self) -> Result<[f64; 2], CoreError> {
        Ok([self.f64()?, self.f64()?])
    }

    fn size(&mut self, cap: usize) -> Result<usize, CoreError> {
        usize::try_from(self.i32()?)
            .ok()
            .filter(|n| *n <= cap)
            .ok_or_else(|| bad("a 3DM object has an impossible size"))
    }
}

/// A valid, increasing interval.
fn increasing(interval: [f64; 2]) -> Result<[f64; 2], CoreError> {
    if interval[1] > interval[0] {
        Ok(interval)
    } else {
        Err(bad("a 3DM curve or surface has an empty domain"))
    }
}

/// Stored knots, checked: `order + count - 2` values that never decrease
/// and leave a non-empty domain.
fn knots(r: &mut Reader<'_>, order: usize, count: usize) -> Result<Vec<f64>, CoreError> {
    let n = r.size(MAX_CVS + MAX_ORDER)?;
    if n != order + count - 2 {
        return Err(bad("a 3DM NURBS has the wrong number of knots"));
    }
    let stored = (0..n).map(|_| r.f64()).collect::<Result<Vec<_>, _>>()?;
    if stored.windows(2).any(|w| w[1] < w[0]) {
        return Err(bad("a 3DM NURBS has knots out of order"));
    }
    let knots = padded(stored);
    increasing([knots[order - 1], knots[count]])?;
    Ok(knots)
}

/// Homogeneous control points of `dim` coordinates, plus a weight when
/// rational.
fn cvs(
    r: &mut Reader<'_>,
    n: usize,
    dim: usize,
    rational: bool,
) -> Result<Vec<[f64; 4]>, CoreError> {
    (0..n)
        .map(|_| {
            let mut h = [0.0, 0.0, 0.0, 1.0];
            for c in h.iter_mut().take(dim) {
                *c = r.f64()?;
            }
            if rational {
                h[3] = r.f64()?;
                if h[3] <= 0.0 {
                    return Err(bad("a 3DM NURBS has a weight that is not positive"));
                }
            }
            Ok(h)
        })
        .collect()
}

/// Order and control point count, checked.
fn order_count(r: &mut Reader<'_>) -> Result<(usize, usize), CoreError> {
    let order = r.size(MAX_ORDER)?;
    let count = r.size(MAX_CVS)?;
    if order < 2 || count < order {
        return Err(bad("a 3DM NURBS has an impossible order"));
    }
    Ok((order, count))
}

impl Archive {
    /// The class UUID and data of a `TCODE_OPENNURBS_CLASS` chunk starting
    /// at `at`, and where the chunk ends.
    pub(super) fn class_at(
        &self,
        bytes: &[u8],
        at: usize,
        end: usize,
    ) -> Result<([u8; 16], Range<usize>, usize), CoreError> {
        let chunk = self.chunk(bytes, at, end)?;
        if chunk.typecode != super::OPENNURBS_CLASS {
            return Err(bad("a 3DM object is not where it should be"));
        }
        let (uuid, data) = self.class(bytes, chunk.body.clone())?;
        Ok((uuid, data, chunk.next))
    }

    /// The class UUID and data inside a class chunk's body.
    pub(super) fn class(
        &self,
        bytes: &[u8],
        body: Range<usize>,
    ) -> Result<([u8; 16], Range<usize>), CoreError> {
        let parts = self.children(bytes, body)?;
        let uuid = parts
            .iter()
            .find(|c| c.typecode == super::CLASS_UUID)
            .and_then(|c| bytes.get(c.body.start..c.body.start + 16))
            .and_then(|b| <[u8; 16]>::try_from(b).ok())
            .ok_or_else(|| bad("a 3DM object has no class"))?;
        let data = parts
            .iter()
            .find(|c| c.typecode == super::CLASS_DATA)
            .map(|c| c.body.clone())
            .ok_or_else(|| bad("a 3DM object has no data"))?;
        Ok((uuid, data))
    }

    /// The curve a class chunk at `r` holds (moving past it); `None` for a
    /// class this reader does not know.
    pub(super) fn curve_at(
        &self,
        bytes: &[u8],
        r: &mut Reader<'_>,
        depth: usize,
    ) -> Result<Option<Curve>, CoreError> {
        let (uuid, data, next) = self.class_at(bytes, r.at, r.end)?;
        r.at = next;
        self.curve(bytes, &uuid, data, depth)
    }

    /// A serialised curve of class `uuid`; `None` for a class this reader
    /// does not know.
    pub(super) fn curve(
        &self,
        bytes: &[u8],
        uuid: &[u8; 16],
        data: Range<usize>,
        depth: usize,
    ) -> Result<Option<Curve>, CoreError> {
        if depth > MAX_DEPTH {
            return Err(bad("a 3DM curve is nested too deeply"));
        }
        let mut r = Reader::new(bytes, data.start, data.end);
        let version = r.u8()?;
        let major = version >> 4;
        let curve = match *uuid {
            NURBS_CURVE if major == 1 => {
                let dim = r.size(3)?;
                let rational = r.i32()? != 0;
                let (order, count) = order_count(&mut r)?;
                // Two reserved numbers and a bounding box.
                r.skip(8 + 48)?;
                let knots = knots(&mut r, order, count)?;
                if r.size(MAX_CVS)? != count || dim == 0 {
                    return Err(bad("a 3DM NURBS curve's points do not match its count"));
                }
                Curve::Nurbs(Nurbs {
                    order,
                    count,
                    knots,
                    cvs: cvs(&mut r, count, dim, rational)?,
                })
            }
            LINE if major == 1 => {
                let from = r.point()?;
                let to = r.point()?;
                Curve::Line {
                    from,
                    to,
                    domain: increasing(r.interval()?)?,
                }
            }
            POLYLINE if major == 1 => {
                let n = r.size(MAX_CVS)?;
                let points = (0..n).map(|_| r.point()).collect::<Result<Vec<_>, _>>()?;
                let m = r.size(MAX_CVS)?;
                let params = (0..m).map(|_| r.f64()).collect::<Result<Vec<_>, _>>()?;
                if n < 2 || m != n || params.windows(2).any(|w| w[1] <= w[0]) {
                    return Err(bad("a 3DM polyline is damaged"));
                }
                Curve::Polyline { points, params }
            }
            ARC if major == 1 => {
                // The circle's plane (origin, three axes, equation), radius
                // and three points kept for old readers.
                let centre = r.point()?;
                let x = r.point()?;
                let y = r.point()?;
                r.skip(3 * 8 + 4 * 8)?;
                let radius = r.f64()?;
                r.skip(9 * 8)?;
                let angle = increasing(r.interval()?)?;
                let domain = increasing(r.interval()?)?;
                Curve::Arc {
                    centre,
                    x,
                    y,
                    radius,
                    angle,
                    domain,
                }
            }
            POLYCURVE if major == 1 => {
                let n = r.size(MAX_CVS)?;
                // Two reserved numbers and a bounding box.
                r.skip(8 + 48)?;
                let m = r.size(MAX_CVS)?;
                let params = (0..m).map(|_| r.f64()).collect::<Result<Vec<_>, _>>()?;
                if n == 0 || m != n + 1 || params.windows(2).any(|w| w[1] <= w[0]) {
                    return Err(bad("a 3DM polycurve is damaged"));
                }
                let mut segments = Vec::with_capacity(n);
                for _ in 0..n {
                    match self.curve_at(bytes, &mut r, depth + 1)? {
                        Some(segment) => segments.push(segment),
                        None => return Ok(None),
                    }
                }
                Curve::Poly { params, segments }
            }
            _ => return Ok(None),
        };
        Ok(Some(curve))
    }

    /// A serialised surface of class `uuid`; `None` for a class this
    /// reader does not know.
    pub(super) fn surface(
        &self,
        bytes: &[u8],
        uuid: &[u8; 16],
        data: Range<usize>,
    ) -> Result<Option<Surface>, CoreError> {
        let mut r = Reader::new(bytes, data.start, data.end);
        let version = r.u8()?;
        let (major, minor) = (version >> 4, version & 0x0f);
        let surface = match *uuid {
            NURBS_SURFACE if major == 1 => {
                let dim = r.size(3)?;
                let rational = r.i32()? != 0;
                let order = [r.size(MAX_ORDER)?, r.size(MAX_ORDER)?];
                let count = [r.size(MAX_CVS)?, r.size(MAX_CVS)?];
                if order.iter().any(|o| *o < 2)
                    || count[0] < order[0]
                    || count[1] < order[1]
                    || count[0].saturating_mul(count[1]) > MAX_CVS
                    || dim == 0
                {
                    return Err(bad("a 3DM NURBS surface has an impossible size"));
                }
                r.skip(8 + 48)?;
                let knots = [
                    knots(&mut r, order[0], count[0])?,
                    knots(&mut r, order[1], count[1])?,
                ];
                let n = count[0] * count[1];
                if r.size(MAX_CVS)? != n {
                    return Err(bad("a 3DM NURBS surface's points do not match its count"));
                }
                Surface::Nurbs {
                    order,
                    count,
                    knots,
                    cvs: cvs(&mut r, n, dim, rational)?,
                }
            }
            PLANE_SURFACE if major == 1 => {
                let origin = r.point()?;
                let x = r.point()?;
                let y = r.point()?;
                // The z axis and the plane equation.
                r.skip(7 * 8)?;
                let domain = [increasing(r.interval()?)?, increasing(r.interval()?)?];
                let extents = if minor >= 1 {
                    [increasing(r.interval()?)?, increasing(r.interval()?)?]
                } else {
                    domain
                };
                Surface::Plane {
                    origin,
                    x,
                    y,
                    domain,
                    extents,
                }
            }
            REV_SURFACE if major == 1 || major == 2 => {
                let origin = r.point()?;
                let to = r.point()?;
                let angle = increasing(r.interval()?)?;
                let domain = if major >= 2 {
                    increasing(r.interval()?)?
                } else {
                    angle
                };
                r.skip(48)?;
                let transposed = r.i32()? != 0;
                if r.u8()? == 0 {
                    return Err(bad("a 3DM surface of revolution has no profile"));
                }
                let axis = unit(sub(to, origin)).ok_or_else(|| bad("a 3DM revolve has no axis"))?;
                match self.curve_at(bytes, &mut r, 1)? {
                    Some(profile) => Surface::Rev {
                        origin,
                        axis,
                        angle,
                        domain,
                        transposed,
                        profile: Box::new(profile),
                    },
                    None => return Ok(None),
                }
            }
            SUM_SURFACE if major == 1 => {
                let base = r.point()?;
                r.skip(48)?;
                let first = self.curve_at(bytes, &mut r, 1)?;
                let second = self.curve_at(bytes, &mut r, 1)?;
                match (first, second) {
                    (Some(a), Some(b)) => Surface::Sum {
                        base,
                        curves: Box::new([a, b]),
                    },
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(surface))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P3, b: P3) -> bool {
        length(sub(a, b)) < 1e-9
    }

    #[test]
    fn a_rational_quadratic_traces_a_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let n = Nurbs {
            order: 3,
            count: 3,
            knots: padded(vec![0.0, 0.0, 1.0, 1.0]),
            cvs: vec![
                [10.0, 0.0, 0.0, 1.0],
                [10.0 * w, 10.0 * w, 0.0, w],
                [0.0, 10.0, 0.0, 1.0],
            ],
        };
        for k in 0..=10 {
            let p = n.at(f64::from(k) / 10.0);
            assert!((length(p) - 10.0).abs() < 1e-9, "{p:?}");
        }
        assert!(close(n.at(0.0), [10.0, 0.0, 0.0]));
        assert!(close(n.at(1.0), [0.0, 10.0, 0.0]));
        // A tighter tolerance cuts the span finer.
        assert!(n.params(0.0, 1.0, 0.001).len() > n.params(0.0, 1.0, 0.1).len());
    }

    #[test]
    fn polycurves_map_their_pieces() {
        let c = Curve::Poly {
            params: vec![0.0, 1.0, 3.0],
            segments: vec![
                Curve::Line {
                    from: [0.0, 0.0, 0.0],
                    to: [1.0, 0.0, 0.0],
                    domain: [0.0, 5.0],
                },
                Curve::Polyline {
                    points: vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
                    params: vec![0.0, 1.0, 2.0],
                },
            ],
        };
        assert!(close(c.at(0.5), [0.5, 0.0, 0.0]));
        assert!(close(c.at(2.0), [1.0, 1.0, 0.0]));
        assert!(close(c.at(3.0), [0.0, 1.0, 0.0]));
        assert_eq!(c.params(0.0, 3.0, 0.01), vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn revolved_and_summed_surfaces_evaluate() {
        let rev = Surface::Rev {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            angle: [0.0, std::f64::consts::TAU],
            domain: [0.0, 1.0],
            transposed: false,
            profile: Box::new(Curve::Line {
                from: [5.0, 0.0, 0.0],
                to: [5.0, 0.0, 10.0],
                domain: [0.0, 1.0],
            }),
        };
        assert!(close(rev.at(0.25, 1.0), [0.0, 5.0, 10.0]));
        assert!(rev.grid(0, 0.0, 1.0, 0.01).len() > 20);
        assert_eq!(rev.grid(1, 0.0, 1.0, 0.01), vec![0.0, 1.0]);
        let sum = Surface::Sum {
            base: [1.0, 1.0, 1.0],
            curves: Box::new([
                Curve::Line {
                    from: [0.0; 3],
                    to: [2.0, 0.0, 0.0],
                    domain: [0.0, 1.0],
                },
                Curve::Line {
                    from: [0.0; 3],
                    to: [0.0, 0.0, 3.0],
                    domain: [0.0, 1.0],
                },
            ]),
        };
        assert!(close(sum.at(1.0, 1.0), [3.0, 1.0, 4.0]));
    }

    #[test]
    fn grids_never_pass_the_cap() {
        let params: Vec<f64> = (0..1000).map(f64::from).collect();
        let thinned = thin(params, MAX_GRID);
        assert!(thinned.len() <= MAX_GRID);
        assert_eq!(thinned.first(), Some(&0.0));
        assert_eq!(thinned.last(), Some(&999.0));
    }
}
