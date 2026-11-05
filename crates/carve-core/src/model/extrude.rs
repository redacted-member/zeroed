//! Extrude and Weave: cross sections, or a component, carried along one or
//! more drive rails.
//!
//! Every cell near a rail finds its nearest place on the rail: the arc
//! length there says how far along the extrusion it is, and its signed
//! distance from the rail says where across it is (positive on the left,
//! looking along the rail).
//!
//! With vector cross sections the section's left end as drawn lies on the
//! rail's left. Sections sit on a rail at stations (a fraction of its
//! length each) and blend from one to the next as in the Two Rail Sweep;
//! a rail with no stations carries the first section all the way. On an
//! open rail with stations the shape runs from the first to the last one.
//!
//! With a component, its X runs along the rail and its Y across, centred:
//! either one copy stretched along the whole rail, or copies at their own
//! size one after another, each overlapping the one before by a share of
//! its length, alternate ones flipped across if asked. Overlaps merge.
//!
//! Corners are rounded round the rail's node, or with sharp corners split
//! on their bisector so the edges meet in a mitre.
//!
//! Weave: where rails cross (each other or themselves), one strand passes
//! over and the other under, alternating along every strand as far as the
//! crossings allow. At an over crossing the strand's heights are scaled to
//! the Z Over percentage (or raised by that share of the shape's height
//! with Add Base), at an under crossing to Z Under; between crossings the
//! change eases from one to the next, and past the last crossing of an
//! open strand it eases back to the shape as made.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Tolerance, Vec2};

use super::field::Field;
use super::shape::Grid;
use super::sweep::{place, start_at, Blend, Station};

/// How long a mitre may reach, as a multiple of the half width.
const MITRE_LIMIT: f64 = 4.0;

/// Where one cross section sits on a drive rail.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtrudeStation {
    /// Index into the drive rails.
    pub rail: usize,
    /// Index into the cross sections.
    pub section: usize,
    /// Fraction of the rail's length, 0 at its start to 1 at its end.
    pub at: f64,
    /// Flow smoothly through this section; off makes a crease there.
    #[serde(default = "yes")]
    pub smooth: bool,
}

fn yes() -> bool {
    true
}

/// How a component fills a rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentFill {
    /// One copy stretched along the whole rail.
    #[default]
    Stretch,
    /// Copies at the component's own size, one after another.
    Copies,
}

/// How a weave changes the strands at the crossings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WeaveMode {
    /// Scale the heights to the percentages.
    #[default]
    ScaleShape,
    /// Keep the heights and raise them by the percentages of the shape's
    /// height.
    AddBase,
}

/// The Extrude and Weave form's settings. Lengths are millimetres,
/// percentages are 0 to 100 (and beyond for Z Over).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtrudeParams {
    /// Run a rail from its far end, per rail.
    pub reverse: Vec<bool>,
    /// Start of a closed rail: the point on it nearest this job position,
    /// per rail.
    pub starts: Vec<Option<Point>>,
    /// Where the cross sections sit on the rails. A rail with none carries
    /// the first section all the way.
    pub stations: Vec<ExtrudeStation>,
    /// Mitre the corners of the rails instead of rounding them.
    pub sharp_corners: bool,
    /// Blend sections with as many spans span by span.
    pub sweep_between_spans: bool,
    /// With a component: stretch one copy or repeat copies.
    pub fill: ComponentFill,
    /// With copies: how much of its length each overlaps the one before.
    pub overlap: f64,
    /// With copies: flip every other copy across the rail.
    pub flip_alternate: bool,
    /// Raise and lower the strands where the rails cross.
    pub weave: bool,
    pub weave_mode: WeaveMode,
    /// Percentage at the crossings a strand passes under.
    pub z_under: f64,
    /// Percentage at the crossings a strand passes over.
    pub z_over: f64,
    /// Scale the result so its highest point is `height`.
    pub exact_height: bool,
    pub height: f64,
}

impl Default for ExtrudeParams {
    fn default() -> Self {
        ExtrudeParams {
            reverse: Vec::new(),
            starts: Vec::new(),
            stations: Vec::new(),
            sharp_corners: false,
            sweep_between_spans: false,
            fill: ComponentFill::Stretch,
            overlap: 0.0,
            flip_alternate: false,
            weave: false,
            weave_mode: WeaveMode::ScaleShape,
            z_under: 70.0,
            z_over: 130.0,
            exact_height: false,
            height: 10.0,
        }
    }
}

/// What is carried along the rails.
pub enum Profile<'a> {
    /// Open cross section vectors.
    Sections(&'a [Path]),
    /// A component's heights, X along the rail and Y across.
    Component(&'a Field),
}

/// One run of a rail, flattened, started and turned as asked. A closed
/// run repeats its first point at the end.
struct Strand {
    points: Vec<Point>,
    closed: bool,
    /// Arc length at each point.
    along: Vec<f64>,
    /// The rail it belongs to.
    rail: usize,
    /// The rail is this one run, so its stations apply.
    whole: bool,
}

impl Strand {
    fn length(&self) -> f64 {
        self.along.last().copied().unwrap_or(0.0)
    }

    fn segments(&self) -> usize {
        self.points.len() - 1
    }
}

fn strands_of(
    path: &Path,
    rail: usize,
    params: &ExtrudeParams,
    tol: Tolerance,
) -> Result<Vec<Strand>, CoreError> {
    let reverse = params.reverse.get(rail).copied().unwrap_or(false);
    let start = params.starts.get(rail).copied().flatten();
    let whole = path.subpaths.len() == 1;
    let mut out = Vec::new();
    for sub in &path.subpaths {
        let flat = flatten_subpath(sub, tol);
        let mut points: Vec<Point> = Vec::with_capacity(flat.points.len() + 1);
        for p in flat.points {
            if !p.is_finite() {
                return Err(CoreError::validation(
                    "a rail has a point that is not a number",
                ));
            }
            if points.last().is_none_or(|q| q.distance(p) > 1e-9) {
                points.push(p);
            }
        }
        if sub.closed && points.len() > 2 {
            if points
                .first()
                .zip(points.last())
                .is_some_and(|(a, b)| a.distance(*b) <= 1e-9)
            {
                points.pop();
            }
            if let (Some(from), true) = (start, whole) {
                points = start_at(&points, from);
            }
            points.push(points[0]);
        }
        if points.len() < 2 {
            continue;
        }
        if reverse {
            points.reverse();
        }
        let mut along = Vec::with_capacity(points.len());
        let mut total = 0.0;
        along.push(0.0);
        for pair in points.windows(2) {
            total += pair[0].distance(pair[1]);
            along.push(total);
        }
        out.push(Strand {
            points,
            closed: sub.closed,
            along,
            rail,
            whole,
        });
    }
    Ok(out)
}

/// A place where two strands (or one strand twice) cross: `(strand, arc
/// length)` for each pass, the first pass being the lower strand index or,
/// on one strand, the earlier pass.
#[derive(Debug, Clone, Copy)]
struct Crossing {
    passes: [(usize, f64); 2],
}

fn crossings(strands: &[Strand]) -> Vec<Crossing> {
    let mut out = Vec::new();
    for (si, a) in strands.iter().enumerate() {
        for (sj, b) in strands.iter().enumerate().skip(si) {
            for p in 0..a.segments() {
                let (a0, a1) = (a.points[p], a.points[p + 1]);
                let q_start = if si == sj { p + 2 } else { 0 };
                for q in q_start..b.segments() {
                    if si == sj && a.closed && p == 0 && q == b.segments() - 1 {
                        continue;
                    }
                    let (b0, b1) = (b.points[q], b.points[q + 1]);
                    if a0.x.max(a1.x) < b0.x.min(b1.x)
                        || b0.x.max(b1.x) < a0.x.min(a1.x)
                        || a0.y.max(a1.y) < b0.y.min(b1.y)
                        || b0.y.max(b1.y) < a0.y.min(a1.y)
                    {
                        continue;
                    }
                    let (r, s) = (a0.to(a1), b0.to(b1));
                    let den = r.cross(s);
                    if den.abs() < 1e-12 {
                        continue;
                    }
                    let w = a0.to(b0);
                    let t = w.cross(s) / den;
                    let u = w.cross(r) / den;
                    if !(0.0..1.0).contains(&t) || !(0.0..1.0).contains(&u) {
                        continue;
                    }
                    let sa = a.along[p] + t * (a.along[p + 1] - a.along[p]);
                    let sb = b.along[q] + u * (b.along[q + 1] - b.along[q]);
                    out.push(Crossing {
                        passes: [(si, sa), (sj, sb)],
                    });
                }
            }
        }
    }
    out
}

/// For every crossing, whether its first pass goes over. Neighbouring
/// crossings along a strand alternate; where a loop of crossings cannot
/// alternate all the way round, the first choice made stands.
fn over_under(strands: &[Strand], found: &[Crossing]) -> Vec<bool> {
    // Passes on each strand in order along it: (arc length, crossing, pass).
    let mut on: Vec<Vec<(f64, usize, usize)>> = vec![Vec::new(); strands.len()];
    for (c, crossing) in found.iter().enumerate() {
        for (k, &(strand, s)) in crossing.passes.iter().enumerate() {
            on[strand].push((s, c, k));
        }
    }
    // Neighbours: (other crossing, pass here, pass there).
    let mut links: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); found.len()];
    for (strand, list) in on.iter_mut().enumerate() {
        list.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut pairs: Vec<(usize, usize)> = (1..list.len()).map(|k| (k - 1, k)).collect();
        if strands[strand].closed && list.len() > 2 {
            pairs.push((list.len() - 1, 0));
        }
        for (x, y) in pairs {
            let ((_, c1, k1), (_, c2, k2)) = (list[x], list[y]);
            links[c1].push((c2, k1, k2));
            links[c2].push((c1, k2, k1));
        }
    }
    let mut first_over: Vec<Option<bool>> = vec![None; found.len()];
    let mut queue = VecDeque::new();
    for start in 0..found.len() {
        if first_over[start].is_some() {
            continue;
        }
        first_over[start] = Some(true);
        queue.push_back(start);
        while let Some(c) = queue.pop_front() {
            let here = first_over[c].unwrap_or(true);
            for &(next, k_here, k_there) in &links[c] {
                if first_over[next].is_some() {
                    continue;
                }
                // The strand's state here is over when (first over) xor (it
                // is the second pass); it must be the opposite there.
                let state_here = here ^ (k_here == 1);
                first_over[next] = Some(!state_here ^ (k_there == 1));
                queue.push_back(next);
            }
        }
    }
    first_over.into_iter().map(|v| v.unwrap_or(true)).collect()
}

/// The cells of a window that a strand reaches, with where each lies
/// along and across the strand.
struct Nearest {
    i0: usize,
    j0: usize,
    w: usize,
    h: usize,
    dist: Vec<f64>,
    along: Vec<f64>,
    across: Vec<f64>,
}

impl Nearest {
    fn offer(&mut self, index: usize, dist: f64, along: f64, across: f64) {
        if dist < self.dist[index] {
            self.dist[index] = dist;
            self.along[index] = along;
            self.across[index] = across;
        }
    }
}

/// Inclusive cell range covering `[lo, hi]` on one axis, if any.
fn cell_range(lo: f64, hi: f64, origin: f64, cell: f64, n: usize) -> Option<(usize, usize)> {
    let a = ((lo - origin) / cell).ceil().max(0.0);
    let b = ((hi - origin) / cell).floor().min(n as f64 - 1.0);
    (b >= a).then_some((a as usize, b as usize))
}

/// Unit left normal of a segment.
fn normal(a: Point, b: Point) -> Vec2 {
    a.to(b).normalised().unwrap_or(Vec2::new(1.0, 0.0)).perp()
}

/// Bisector direction at node `k` of a strand, for sharp corners.
fn mitre(strand: &Strand, k: usize) -> Vec2 {
    let n = strand.segments();
    let this = |seg: usize| normal(strand.points[seg], strand.points[seg + 1]);
    let (prev, next) = if k == 0 {
        if !strand.closed {
            return this(0);
        }
        (this(n - 1), this(0))
    } else if k == n {
        if !strand.closed {
            return this(n - 1);
        }
        (this(n - 1), this(0))
    } else {
        (this(k - 1), this(k))
    };
    prev.plus(next).normalised().unwrap_or(next)
}

fn nearest_cells(strand: &Strand, grid: &Grid, reach: f64, sharp: bool) -> Option<Nearest> {
    let pad = reach * MITRE_LIMIT + grid.cell;
    let lo_x = strand
        .points
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min)
        - pad;
    let hi_x = strand
        .points
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max)
        + pad;
    let lo_y = strand
        .points
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        - pad;
    let hi_y = strand
        .points
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        + pad;
    let (i0, i1) = cell_range(lo_x, hi_x, grid.origin.x, grid.cell, grid.width)?;
    let (j0, j1) = cell_range(lo_y, hi_y, grid.origin.y, grid.cell, grid.height)?;
    let (w, h) = (i1 - i0 + 1, j1 - j0 + 1);
    let mut near = Nearest {
        i0,
        j0,
        w,
        h,
        dist: vec![f64::INFINITY; w * h],
        along: vec![0.0; w * h],
        across: vec![0.0; w * h],
    };
    let centre = |i: usize, j: usize| {
        Point::new(
            grid.origin.x + i as f64 * grid.cell,
            grid.origin.y + j as f64 * grid.cell,
        )
    };
    let n = strand.segments();
    let eps = 1e-9;
    for k in 0..n {
        let (a, b) = (strand.points[k], strand.points[k + 1]);
        let len = a.distance(b);
        let Some(e) = a.to(b).normalised() else {
            continue;
        };
        let left = e.perp();
        let spread = if sharp { reach * MITRE_LIMIT } else { reach };
        let range_i = cell_range(
            a.x.min(b.x) - spread,
            a.x.max(b.x) + spread,
            grid.origin.x,
            grid.cell,
            grid.width,
        );
        let range_j = cell_range(
            a.y.min(b.y) - spread,
            a.y.max(b.y) + spread,
            grid.origin.y,
            grid.cell,
            grid.height,
        );
        let (Some((ci0, ci1)), Some((cj0, cj1))) = (range_i, range_j) else {
            continue;
        };
        let first = k == 0 && !strand.closed;
        let last = k == n - 1 && !strand.closed;
        let (m_a, m_b) = (mitre(strand, k), mitre(strand, k + 1));
        let (ref_a, ref_b) = (m_a.cross(e), m_b.cross(e.scaled(-1.0)));
        for j in cj0.max(j0)..=cj1.min(j1) {
            for i in ci0.max(i0)..=ci1.min(i1) {
                let c = centre(i, j);
                let v = a.to(c);
                let along = v.dot(e);
                let index = (j - j0) * w + (i - i0);
                if sharp {
                    if m_a.cross(v) * ref_a < -eps || m_b.cross(b.to(c)) * ref_b < -eps {
                        continue;
                    }
                    let d = v.dot(left);
                    let tip = c.distance(a).min(c.distance(b));
                    if d.abs() > reach
                        || (along.clamp(0.0, len) != along && tip > reach * MITRE_LIMIT)
                    {
                        continue;
                    }
                    near.offer(index, d.abs(), strand.along[k] + along.clamp(0.0, len), d);
                } else {
                    if (first && along < 0.0) || (last && along > len) {
                        continue;
                    }
                    let t = along.clamp(0.0, len);
                    let foot = a.offset(e.scaled(t));
                    let dist = foot.distance(c);
                    if dist > reach {
                        continue;
                    }
                    let side = if v.dot(left) < 0.0 { -1.0 } else { 1.0 };
                    near.offer(index, dist, strand.along[k] + t, side * dist);
                }
            }
        }
    }
    Some(near)
}

/// Smooth step from 0 at 0 to 1 at 1.
fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f64::consts::PI * t).cos()
}

fn check(params: &ExtrudeParams) -> Result<(), CoreError> {
    let finite = [params.overlap, params.z_under, params.z_over, params.height];
    if finite.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation(
            "every extrusion setting must be a number",
        ));
    }
    if !(0.0..=95.0).contains(&params.overlap) {
        return Err(CoreError::validation(
            "the overlap must be from 0 to 95 percent",
        ));
    }
    if params.weave && (params.z_under < 0.0 || params.z_over < 0.0) {
        return Err(CoreError::validation(
            "the weave percentages must be 0 or more",
        ));
    }
    if params.exact_height && params.height <= 0.0 {
        return Err(CoreError::validation(
            "the exact height must be more than zero",
        ));
    }
    Ok(())
}

/// The weave's change along one strand: `(arc length, value)` at each
/// crossing in order, and the value where the strand is left as made.
struct WeaveLine {
    marks: Vec<(f64, f64)>,
    neutral: f64,
    /// Distance over which an open end eases back to `neutral`.
    reach: f64,
    closed: bool,
    length: f64,
}

impl WeaveLine {
    fn at(&self, s: f64) -> f64 {
        let n = self.marks.len();
        let Some(&(first_s, first_v)) = self.marks.first() else {
            return self.neutral;
        };
        let (last_s, last_v) = self.marks[n - 1];
        if self.closed && n > 1 {
            let mut s = s;
            if s < first_s {
                s += self.length;
            }
            for k in 0..n {
                let (a, va) = self.marks[k];
                let (b, vb) = if k + 1 < n {
                    self.marks[k + 1]
                } else {
                    (first_s + self.length, first_v)
                };
                if s >= a && s <= b {
                    let t = if b > a { (s - a) / (b - a) } else { 0.0 };
                    return va + (vb - va) * ease(t);
                }
            }
            return self.neutral;
        }
        if s <= first_s {
            let t = ((first_s - s) / self.reach).min(1.0);
            return first_v + (self.neutral - first_v) * ease(t);
        }
        if s >= last_s {
            let t = ((s - last_s) / self.reach).min(1.0);
            return last_v + (self.neutral - last_v) * ease(t);
        }
        for pair in self.marks.windows(2) {
            let ((a, va), (b, vb)) = (pair[0], pair[1]);
            if s >= a && s <= b {
                let t = if b > a { (s - a) / (b - a) } else { 0.0 };
                return va + (vb - va) * ease(t);
            }
        }
        self.neutral
    }
}

/// A component's heights with the box of its cells, sampled between
/// cells.
struct Carried<'a> {
    field: &'a Field,
    /// Left edge of the cells along X and their length, mm.
    x0: f64,
    length: f64,
    /// Middle of the cells across Y and their width, mm.
    mid_y: f64,
    width: f64,
}

impl<'a> Carried<'a> {
    fn new(field: &'a Field) -> Option<Carried<'a>> {
        let (mut i0, mut i1, mut j0, mut j1) = (usize::MAX, 0, usize::MAX, 0);
        for j in 0..field.height {
            for i in 0..field.width {
                if !field.z[j * field.width + i].is_nan() {
                    i0 = i0.min(i);
                    i1 = i1.max(i);
                    j0 = j0.min(j);
                    j1 = j1.max(j);
                }
            }
        }
        if i0 > i1 || j0 > j1 {
            return None;
        }
        let lo = field.centre(i0, j0);
        let hi = field.centre(i1, j1);
        Some(Carried {
            field,
            x0: lo.x - field.cell / 2.0,
            length: hi.x - lo.x + field.cell,
            mid_y: (lo.y + hi.y) / 2.0,
            width: hi.y - lo.y + field.cell,
        })
    }

    /// Height at `along` mm from the left edge and `across` mm from the
    /// middle (up in Y), NaN outside the component.
    fn sample(&self, along: f64, across: f64) -> f64 {
        let f = self.field;
        let x = (self.x0 + along.clamp(0.0, self.length) - f.origin.x) / f.cell;
        let y = (self.mid_y + across - f.origin.y) / f.cell;
        let (i, j) = (x.floor(), y.floor());
        let (fx, fy) = (x - i, y - j);
        let (i, j) = (i as i64, j as i64);
        let corners = [
            (f.get(i, j), (1.0 - fx) * (1.0 - fy)),
            (f.get(i + 1, j), fx * (1.0 - fy)),
            (f.get(i, j + 1), (1.0 - fx) * fy),
            (f.get(i + 1, j + 1), fx * fy),
        ];
        let (mut sum, mut weight) = (0.0, 0.0);
        for (v, w) in corners {
            if !v.is_nan() && w > 0.0 {
                sum += f64::from(v) * w;
                weight += w;
            }
        }
        // Only where the nearest cell has a height, so the outline stays.
        if weight <= 0.0 || f.get(x.round() as i64, y.round() as i64).is_nan() {
            return f64::NAN;
        }
        sum / weight
    }

    fn highest(&self) -> f64 {
        self.field.max().map_or(0.0, f64::from)
    }
}

/// The stations of `rail`, sorted along it; with none, the first section
/// at the start (and at the end of an open rail).
fn rail_stations(params: &ExtrudeParams, rail: usize, closed: bool) -> (Vec<Station>, bool) {
    let mut stations: Vec<Station> = params
        .stations
        .iter()
        .filter(|s| s.rail == rail)
        .map(|s| Station {
            section: s.section,
            a: s.at,
            b: s.at,
            smooth: s.smooth,
        })
        .collect();
    let given = !stations.is_empty();
    if !given {
        stations.push(Station {
            section: 0,
            a: 0.0,
            b: 0.0,
            smooth: true,
        });
    }
    stations.sort_by(|p, q| p.a.total_cmp(&q.a));
    if closed && stations.len() > 1 {
        // The end of a closed rail is its start: one station there.
        if let (Some(first), Some(last)) = (stations.first(), stations.last()) {
            if first.a <= 1e-9 && last.a >= 1.0 - 1e-9 {
                stations.pop();
            }
        }
    }
    (stations, given)
}

/// Builds the heights of an extrusion over the whole grid, NaN where it
/// does not reach.
///
/// `rails` are the drive rails, open or closed vectors in job space.
/// `profile` is what they carry: open cross section vectors whose chord is
/// the width across and whose offset from the chord is the height, or a
/// component's heights. Where extrusions overlap the higher wins.
///
/// # Errors
/// Returns [`CoreError::Validation`] for no rails or a rail with no length,
/// no cross section, a closed, empty or zero width section, a station off
/// its rail or naming a missing section, an empty component, or settings
/// out of range.
pub fn extrude(
    rails: &[Path],
    profile: Profile<'_>,
    grid: &Grid,
    params: &ExtrudeParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    check(params)?;
    for s in &params.stations {
        if s.rail >= rails.len() || !(s.at.is_finite() && (0.0..=1.0).contains(&s.at)) {
            return Err(CoreError::validation(
                "cross sections must sit on a drive rail, between its start and end",
            ));
        }
    }
    let mut strands = Vec::new();
    for (k, rail) in rails.iter().enumerate() {
        strands.extend(strands_of(rail, k, params, tol)?);
    }
    if strands.is_empty() {
        return Err(CoreError::validation("choose at least one drive rail"));
    }
    if strands.iter().any(|s| s.length() <= 0.0) {
        return Err(CoreError::validation("a drive rail has no length"));
    }

    // One blend per strand for vector sections, or the carried component.
    let mut blends: Vec<(Blend, bool)> = Vec::new();
    let mut carried = None;
    let (reach, tallest) = match profile {
        Profile::Sections(sections) => {
            if sections.is_empty() {
                return Err(CoreError::validation("choose a cross section to extrude"));
            }
            if params.stations.iter().any(|s| s.section >= sections.len()) {
                return Err(CoreError::validation(
                    "a cross section position names a section that is not there",
                ));
            }
            let mut widest: f64 = 0.0;
            let mut tallest: f64 = 0.0;
            for strand in &strands {
                let (stations, given) = if strand.whole {
                    rail_stations(params, strand.rail, strand.closed)
                } else {
                    rail_stations(&ExtrudeParams::default(), 0, strand.closed)
                };
                let count = stations.len();
                let blend = Blend::new(
                    sections,
                    stations,
                    strand.closed,
                    params.sweep_between_spans,
                    tol,
                )?;
                widest = widest.max(blend.widest());
                for k in 0..count {
                    for step in 0..=64 {
                        let h = blend.at(k as f64, f64::from(step) / 64.0).0;
                        if h.is_finite() {
                            tallest = tallest.max(h);
                        }
                    }
                }
                blends.push((blend, given));
            }
            if widest <= 0.0 {
                return Err(CoreError::validation(
                    "the cross section's ends must be apart, it sets the width across",
                ));
            }
            (widest / 2.0, tallest)
        }
        Profile::Component(field) => {
            let c = Carried::new(field)
                .ok_or_else(|| CoreError::validation("the component to extrude is empty"))?;
            let out = (c.width / 2.0, c.highest());
            carried = Some(c);
            out
        }
    };

    // The weave's change along each strand.
    let neutral = match params.weave_mode {
        WeaveMode::ScaleShape => 1.0,
        WeaveMode::AddBase => 0.0,
    };
    let mut weaves: Vec<WeaveLine> = strands
        .iter()
        .map(|s| WeaveLine {
            marks: Vec::new(),
            neutral,
            reach: reach * 2.0,
            closed: s.closed,
            length: s.length(),
        })
        .collect();
    if params.weave {
        let (under, over) = (params.z_under / 100.0, params.z_over / 100.0);
        let found = crossings(&strands);
        let first_over = over_under(&strands, &found);
        for (crossing, first) in found.iter().zip(first_over) {
            for (k, &(strand, s)) in crossing.passes.iter().enumerate() {
                let is_over = first == (k == 0);
                weaves[strand]
                    .marks
                    .push((s, if is_over { over } else { under }));
            }
        }
        for w in &mut weaves {
            w.marks.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
    }

    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for (index, strand) in strands.iter().enumerate() {
        let Some(near) = nearest_cells(strand, grid, reach, params.sharp_corners) else {
            continue;
        };
        let length = strand.length();
        let weave = &weaves[index];
        for jj in 0..near.h {
            for ii in 0..near.w {
                let cell = jj * near.w + ii;
                if near.dist[cell].is_infinite() {
                    continue;
                }
                let s = near.along[cell];
                let across = near.across[cell];
                let h = match (&carried, blends.get(index)) {
                    (Some(c), _) => carry(c, params, s, length, across),
                    (None, Some((blend, given))) => {
                        section_height(blend, *given, strand.closed, s / length, across)
                    }
                    (None, None) => f64::NAN,
                };
                if h.is_nan() {
                    continue;
                }
                let g = weave.at(s);
                let z = match params.weave_mode {
                    WeaveMode::ScaleShape => h * g,
                    WeaveMode::AddBase => h + g * tallest,
                } as f32;
                let at = &mut field.z[(near.j0 + jj) * grid.width + near.i0 + ii];
                if at.is_nan() || z > *at {
                    *at = z;
                }
            }
        }
    }
    if params.exact_height {
        let top = field.max().map_or(0.0, f64::from);
        if top > 1e-9 {
            let factor = (params.height / top) as f32;
            for z in field.z.iter_mut().filter(|z| !z.is_nan()) {
                *z *= factor;
            }
        }
    }
    Ok(field)
}

/// Height of vector sections at fraction `f` along a strand and `across`
/// mm to its left, NaN outside the section or, on an open rail with
/// stations, before the first or after the last.
fn section_height(blend: &Blend, given: bool, closed: bool, f: f64, across: f64) -> f64 {
    let stations = blend.stations();
    if given && !closed {
        let (first, last) = (stations[0].a, stations[stations.len() - 1].a);
        if f < first - 1e-9 || f > last + 1e-9 {
            return f64::NAN;
        }
    }
    let pos = place(stations, closed, f).1;
    let width = blend.at(pos, 0.5).1;
    if width <= 0.0 || across.abs() > width / 2.0 + 1e-9 {
        return f64::NAN;
    }
    let u = (0.5 - across / width).clamp(0.0, 1.0);
    blend.at(pos, u).0
}

/// Height of a carried component `s` mm along a strand of `length` mm and
/// `across` mm to its left.
fn carry(c: &Carried<'_>, params: &ExtrudeParams, s: f64, length: f64, across: f64) -> f64 {
    match params.fill {
        ComponentFill::Stretch => c.sample(s / length * c.length, across),
        ComponentFill::Copies => {
            let step = c.length * (1.0 - params.overlap / 100.0);
            if step <= 0.0 {
                return f64::NAN;
            }
            // Copies whose span can hold `s`: from the first that ends
            // after it to the last that starts before it.
            let last = (s / step).floor() as i64;
            let first = ((s - c.length) / step).ceil().max(0.0) as i64;
            let mut best = f64::NAN;
            for k in first..=last {
                let local = s - k as f64 * step;
                if local < 0.0 || local > c.length {
                    continue;
                }
                let flip = params.flip_alternate && k % 2 == 1;
                let h = c.sample(local, if flip { -across } else { across });
                if !h.is_nan() && (best.is_nan() || h > best) {
                    best = h;
                }
            }
            best
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{line, polyline_path, rect};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn grid(cell: f64) -> Grid {
        Grid {
            width: (100.0 / cell) as usize,
            height: (100.0 / cell) as usize,
            cell,
            origin: Point::new(cell / 2.0, cell / 2.0),
        }
    }

    /// A semicircle 10 wide and 5 high.
    fn semicircle() -> Path {
        let points: Vec<Point> = (0..=180)
            .map(|k| {
                let a = std::f64::consts::PI * (1.0 - f64::from(k) / 180.0);
                Point::new(5.0 + 5.0 * a.cos(), 5.0 * a.sin())
            })
            .collect();
        polyline_path(&points, false).expect("semicircle")
    }

    fn flat(height: f64) -> Path {
        polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(0.0, height),
                Point::new(10.0, height),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("flat")
    }

    fn at(field: &Field, x: f64, y: f64) -> f64 {
        f64::from(field.at(Point::new(x, y)))
    }

    fn straight() -> Path {
        line(Point::new(20.0, 50.0), Point::new(80.0, 50.0))
    }

    fn sections(paths: &[Path]) -> Profile<'_> {
        Profile::Sections(paths)
    }

    #[test]
    fn a_semicircle_along_a_line_is_a_half_cylinder_with_square_ends() {
        let s = [semicircle()];
        let field = extrude(
            &[straight()],
            sections(&s),
            &grid(0.1),
            &ExtrudeParams::default(),
            tol(),
        )
        .expect("extrude");
        for x in [25.0, 50.0, 75.0] {
            assert!((at(&field, x, 50.0) - 5.0).abs() < 0.1);
            assert!((at(&field, x, 53.0) - 4.0).abs() < 0.1);
            assert!((at(&field, x, 47.0) - 4.0).abs() < 0.1);
        }
        assert!(at(&field, 50.0, 56.0).is_nan());
        assert!(at(&field, 18.0, 50.0).is_nan());
        assert!(at(&field, 82.0, 50.0).is_nan());
    }

    #[test]
    fn attached_sections_blend_and_limit_the_extrusion() {
        let s = [flat(1.0), flat(3.0)];
        let stations = vec![
            ExtrudeStation {
                rail: 0,
                section: 0,
                at: 0.25,
                smooth: true,
            },
            ExtrudeStation {
                rail: 0,
                section: 1,
                at: 0.75,
                smooth: true,
            },
        ];
        let params = ExtrudeParams {
            stations,
            ..ExtrudeParams::default()
        };
        let field =
            extrude(&[straight()], sections(&s), &grid(0.2), &params, tol()).expect("extrude");
        // Rail 20 to 80: stations at 35 and 65, blending between.
        assert!((at(&field, 35.2, 50.0) - 1.0).abs() < 0.05);
        assert!((at(&field, 50.0, 50.0) - 2.0).abs() < 0.05);
        assert!((at(&field, 64.8, 50.0) - 3.0).abs() < 0.05);
        assert!(
            at(&field, 30.0, 50.0).is_nan(),
            "starts at the first section"
        );
        assert!(at(&field, 70.0, 50.0).is_nan(), "stops at the last section");
    }

    #[test]
    fn a_reversed_rail_hangs_the_section_the_other_way() {
        // A section high at its left end (as drawn).
        let ramp = polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(0.0, 4.0),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("ramp");
        let s = [ramp];
        let plain = extrude(
            &[straight()],
            sections(&s),
            &grid(0.2),
            &ExtrudeParams::default(),
            tol(),
        )
        .expect("extrude");
        let reversed = ExtrudeParams {
            reverse: vec![true],
            ..ExtrudeParams::default()
        };
        let turned =
            extrude(&[straight()], sections(&s), &grid(0.2), &reversed, tol()).expect("extrude");
        // Along +X the left is +Y.
        assert!(at(&plain, 50.0, 54.0) > at(&plain, 50.0, 46.0) + 2.0);
        assert!(at(&turned, 50.0, 46.0) > at(&turned, 50.0, 54.0) + 2.0);
    }

    #[test]
    fn corners_are_round_or_sharp() {
        let rail = polyline_path(
            &[
                Point::new(20.0, 20.0),
                Point::new(70.0, 20.0),
                Point::new(70.0, 80.0),
            ],
            false,
        )
        .expect("rail");
        let s = [flat(2.0)];
        let round = extrude(
            std::slice::from_ref(&rail),
            sections(&s),
            &grid(0.2),
            &ExtrudeParams::default(),
            tol(),
        )
        .expect("extrude");
        let sharp = ExtrudeParams {
            sharp_corners: true,
            ..ExtrudeParams::default()
        };
        let mitred = extrude(&[rail], sections(&s), &grid(0.2), &sharp, tol()).expect("extrude");
        // The outer corner of the band is at (75, 15).
        assert!(round.at(Point::new(74.5, 15.5)).is_nan());
        assert!((at(&mitred, 74.5, 15.5) - 2.0).abs() < 1e-6);
        for field in [&round, &mitred] {
            assert!((at(field, 45.0, 20.0) - 2.0).abs() < 1e-6);
            assert!((at(field, 70.0, 50.0) - 2.0).abs() < 1e-6);
        }
    }

    #[test]
    fn closed_rails_extrude_all_the_way_round() {
        let square = rect(Point::new(30.0, 30.0), Point::new(70.0, 70.0)).expect("rect");
        let s = [semicircle()];
        let field = extrude(
            &[square],
            sections(&s),
            &grid(0.2),
            &ExtrudeParams::default(),
            tol(),
        )
        .expect("extrude");
        for (x, y) in [(50.0, 30.0), (70.0, 50.0), (50.0, 70.0), (30.0, 50.0)] {
            assert!((at(&field, x, y) - 5.0).abs() < 0.1, "{x} {y}");
        }
        assert!(at(&field, 50.0, 50.0).is_nan());
    }

    /// A component 10 long in X, 4 wide in Y, rising from 1 to 2 along X.
    fn wedge() -> Field {
        let mut field = Field::empty(50, 20, 0.2, Point::new(0.1, 0.1));
        for j in 0..20 {
            for i in 0..50 {
                field.z[j * 50 + i] = 1.0 + i as f32 / 49.0;
            }
        }
        field
    }

    #[test]
    fn a_stretched_component_runs_the_whole_rail() {
        let w = wedge();
        let field = extrude(
            &[straight()],
            Profile::Component(&w),
            &grid(0.2),
            &ExtrudeParams::default(),
            tol(),
        )
        .expect("extrude");
        assert!((at(&field, 20.4, 50.0) - 1.0).abs() < 0.05);
        assert!((at(&field, 50.0, 50.0) - 1.5).abs() < 0.05);
        assert!((at(&field, 79.6, 50.0) - 2.0).abs() < 0.05);
        assert!(!at(&field, 50.0, 51.8).is_nan());
        assert!(at(&field, 50.0, 52.6).is_nan(), "4 wide across the rail");
    }

    #[test]
    fn copies_repeat_at_their_own_size_with_overlap() {
        let w = wedge();
        let params = ExtrudeParams {
            fill: ComponentFill::Copies,
            ..ExtrudeParams::default()
        };
        let field = extrude(
            &[straight()],
            Profile::Component(&w),
            &grid(0.2),
            &params,
            tol(),
        )
        .expect("extrude");
        // Copies every 10 mm from x = 20: low at each start, high at each end.
        assert!((at(&field, 30.4, 50.0) - 1.0).abs() < 0.05);
        assert!((at(&field, 39.6, 50.0) - 2.0).abs() < 0.05);
        let overlapped = ExtrudeParams {
            fill: ComponentFill::Copies,
            overlap: 50.0,
            ..ExtrudeParams::default()
        };
        let field = extrude(
            &[straight()],
            Profile::Component(&w),
            &grid(0.2),
            &overlapped,
            tol(),
        )
        .expect("extrude");
        // Copies every 5 mm; the higher of two overlapping ends wins.
        assert!((at(&field, 29.6, 50.0) - 2.0).abs() < 0.05);
        assert!(at(&field, 27.0, 50.0) > 1.6);
    }

    #[test]
    fn alternate_copies_flip_across_the_rail() {
        // Higher on the +Y half.
        let mut half = Field::empty(50, 20, 0.2, Point::new(0.1, 0.1));
        for j in 0..20 {
            for i in 0..50 {
                half.z[j * 50 + i] = if j >= 10 { 3.0 } else { 1.0 };
            }
        }
        let params = ExtrudeParams {
            fill: ComponentFill::Copies,
            flip_alternate: true,
            ..ExtrudeParams::default()
        };
        let field = extrude(
            &[straight()],
            Profile::Component(&half),
            &grid(0.2),
            &params,
            tol(),
        )
        .expect("extrude");
        assert!(
            (at(&field, 25.0, 51.0) - 3.0).abs() < 1e-3,
            "first copy as made"
        );
        assert!(
            (at(&field, 35.0, 51.0) - 1.0).abs() < 1e-3,
            "second copy flipped"
        );
        assert!((at(&field, 45.0, 51.0) - 3.0).abs() < 1e-3);
    }

    fn woven(mode: WeaveMode, under: f64, over: f64) -> Field {
        let across = line(Point::new(10.0, 50.0), Point::new(90.0, 50.0));
        let mut rails = vec![across];
        rails.extend(
            [30.0, 50.0, 70.0]
                .iter()
                .map(|&x| line(Point::new(x, 10.0), Point::new(x, 90.0))),
        );
        let params = ExtrudeParams {
            weave: true,
            weave_mode: mode,
            z_under: under,
            z_over: over,
            ..ExtrudeParams::default()
        };
        let s = [flat(4.0)];
        extrude(&rails, sections(&s), &grid(0.2), &params, tol()).expect("extrude")
    }

    #[test]
    fn a_weave_scales_the_strands_over_and_under_at_each_crossing() {
        let field = woven(WeaveMode::ScaleShape, 50.0, 150.0);
        // At each crossing the strand on top shows: 150% of 4.
        for x in [30.0, 50.0, 70.0] {
            assert!((at(&field, x, 50.0) - 6.0).abs() < 0.05, "{x}");
        }
        // The across strand alternates: just beside each crossing it is
        // near 6 or near 2.
        let beside: Vec<bool> = [30.0, 50.0, 70.0]
            .iter()
            .map(|&x| at(&field, x - 5.2, 50.0) > 4.0)
            .collect();
        assert_eq!(beside, [beside[0], !beside[0], beside[0]]);
        // Halfway between crossings it passes through the middle.
        assert!((at(&field, 40.0, 50.0) - 4.0).abs() < 0.1);
        // Far from any crossing nothing changes.
        assert!((at(&field, 50.0, 85.0) - 4.0).abs() < 1e-3);
    }

    #[test]
    fn add_base_raises_the_strands_keeping_their_shape() {
        let field = woven(WeaveMode::AddBase, 0.0, 50.0);
        for x in [30.0, 50.0, 70.0] {
            assert!((at(&field, x, 50.0) - 6.0).abs() < 0.05, "{x}");
        }
        assert!((at(&field, 50.0, 85.0) - 4.0).abs() < 1e-3);
    }

    #[test]
    fn exact_height_scales_the_top() {
        let s = [semicircle()];
        let params = ExtrudeParams {
            exact_height: true,
            height: 12.0,
            ..ExtrudeParams::default()
        };
        let field =
            extrude(&[straight()], sections(&s), &grid(0.2), &params, tol()).expect("extrude");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 12.0).abs() < 1e-3);
    }

    #[test]
    fn bad_input_is_refused() {
        let closed = rect(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).expect("rect");
        let g = grid(1.0);
        let p = ExtrudeParams::default();
        let refused = |r: Result<Field, CoreError>| matches!(r, Err(CoreError::Validation(_)));
        let s = [semicircle()];
        assert!(refused(extrude(&[], sections(&s), &g, &p, tol())));
        assert!(refused(extrude(
            &[straight()],
            sections(&[]),
            &g,
            &p,
            tol()
        )));
        let c = [closed];
        assert!(refused(extrude(&[straight()], sections(&c), &g, &p, tol())));
        let off = ExtrudeParams {
            stations: vec![ExtrudeStation {
                rail: 3,
                section: 0,
                at: 0.5,
                smooth: true,
            }],
            ..ExtrudeParams::default()
        };
        assert!(refused(extrude(
            &[straight()],
            sections(&s),
            &g,
            &off,
            tol()
        )));
        let overlap = ExtrudeParams {
            overlap: 100.0,
            ..ExtrudeParams::default()
        };
        assert!(refused(extrude(
            &[straight()],
            sections(&s),
            &g,
            &overlap,
            tol()
        )));
        let empty = Field::empty(4, 4, 1.0, Point::new(0.0, 0.0));
        assert!(refused(extrude(
            &[straight()],
            Profile::Component(&empty),
            &g,
            &p,
            tol()
        )));
    }
}
