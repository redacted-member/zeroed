//! Two-Rail Sweep: one or more cross sections carried along a pair of rails.
//! Both rails are measured by arc length and matched at equal fractions of
//! their length; at each fraction the section spans the gap between the two
//! rail points. The strip between rails is cut into thin triangles and each
//! grid cell inside one takes the section height at its place across.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Tolerance};

use super::field::Field;
use super::shape::Grid;

/// Samples across a section's height table.
const SECTION_SAMPLES: usize = 512;

/// Most steps along the rails, whatever their length.
const MAX_RAIL_STEPS: usize = 20_000;

/// Where one cross section sits on the rails.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    /// Index into the sweep's cross sections.
    pub section: usize,
    /// Fraction of the first rail's length, 0 at its start to 1 at its end.
    pub a: f64,
    /// Fraction of the second rail's length.
    pub b: f64,
    /// Flow smoothly through this section; off makes a crease there.
    #[serde(default = "yes")]
    pub smooth: bool,
}

fn yes() -> bool {
    true
}

/// The Two-Rail Sweep form's settings. Lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SweepParams {
    /// Scale the section's height in step with the gap between the rails.
    /// When off, heights stay as drawn, in millimetres.
    pub scale_height_with_width: bool,
    /// Height of the base the sweep stands on.
    pub base_height: f64,
    /// Run the first and the second rail from their far ends.
    pub reverse: [bool; 2],
    /// Start of a closed rail: the point on it nearest this job position.
    pub starts: [Option<Point>; 2],
    /// Where the sections sit on the rails. Empty spreads the sections
    /// evenly from the start to the end, a single section at both.
    pub stations: Vec<Station>,
    /// Blend sections with the same number of spans span by span, so each
    /// span of one flows into the same span of the next.
    pub sweep_between_spans: bool,
    /// With two closed rails, fill the inside of the inner one at the
    /// height the sections reach along it.
    pub fill_inner: bool,
    /// Scale the sweep so its highest point (above the base) is `height`.
    pub exact_height: bool,
    /// Height for `exact_height`.
    pub height: f64,
}

impl Default for SweepParams {
    fn default() -> Self {
        SweepParams {
            scale_height_with_width: true,
            base_height: 0.0,
            reverse: [false, false],
            starts: [None, None],
            stations: Vec::new(),
            sweep_between_spans: false,
            fill_inner: false,
            exact_height: false,
            height: 10.0,
        }
    }
}

/// Heights sampled at even steps over `[0, length]`, the highest where a
/// vector doubles back. NaN marks a step no part of the vector reaches.
#[derive(Debug, Clone)]
pub(super) struct Table {
    /// Length the table covers, millimetres.
    pub length: f64,
    /// Heights at `k * length / (len - 1)`.
    pub values: Vec<f64>,
}

impl Table {
    /// Builds a table from `(position, height)` points joined in order.
    pub(super) fn from_points(points: &[(f64, f64)], length: f64, samples: usize) -> Table {
        let n = samples.max(2);
        let step = length / (n - 1) as f64;
        let mut values = vec![f64::NAN; n];
        let mut put = |k: usize, h: f64| {
            let v = &mut values[k];
            if v.is_nan() || h > *v {
                *v = h;
            }
        };
        let index = |s: f64| ((s / step).round().max(0.0) as usize).min(n - 1);
        for pair in points.windows(2) {
            let ((s0, h0), (s1, h1)) = (pair[0], pair[1]);
            put(index(s0), h0);
            put(index(s1), h1);
            let (lo, hi) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
            let first = (lo / step).ceil().max(0.0) as usize;
            let last = ((hi / step).floor().max(0.0) as usize).min(n - 1);
            if hi - lo <= 1e-12 {
                continue;
            }
            for k in first..=last {
                let s = k as f64 * step;
                if s < lo || s > hi {
                    continue;
                }
                let f = (s - s0) / (s1 - s0);
                put(k, h0 + (h1 - h0) * f);
            }
        }
        if let [(s, h)] = points {
            put(index(*s), *h);
        }
        Table { length, values }
    }

    /// Height at a position, linear between samples, NaN outside or where
    /// the vector does not reach.
    pub(super) fn at(&self, s: f64) -> f64 {
        let n = self.values.len();
        if !(s >= -1e-9 && s <= self.length + 1e-9) || self.length <= 0.0 {
            return f64::NAN;
        }
        let x = (s / self.length * (n - 1) as f64).clamp(0.0, (n - 1) as f64);
        let k = (x.floor() as usize).min(n - 2);
        let f = x - k as f64;
        let (a, b) = (self.values[k], self.values[k + 1]);
        match (a.is_nan(), b.is_nan()) {
            (false, false) => a + (b - a) * f,
            (false, true) if f < 0.5 => a,
            (true, false) if f >= 0.5 => b,
            _ => f64::NAN,
        }
    }
}

/// The points of an open vector with one run, or a validation error naming
/// what the vector is for.
pub(super) fn open_points(
    path: &Path,
    what: &str,
    tol: Tolerance,
) -> Result<Vec<Point>, CoreError> {
    let [sub] = path.subpaths.as_slice() else {
        return Err(CoreError::validation(format!(
            "the {what} must be a single open vector"
        )));
    };
    if sub.closed {
        return Err(CoreError::validation(format!(
            "the {what} must be an open vector, not a closed one"
        )));
    }
    let points = flatten_subpath(sub, tol).points;
    if points.len() < 2 || points.iter().any(|p| !p.is_finite()) {
        return Err(CoreError::validation(format!("the {what} is empty")));
    }
    Ok(points)
}

/// A cross section as heights over `u` in `[0, 1]` across, plus the width
/// it was drawn at (millimetres).
pub(super) struct Section {
    pub(super) table: Table,
    pub(super) width: f64,
}

pub(super) fn section(path: &Path, tol: Tolerance) -> Result<Section, CoreError> {
    let points = open_points(path, "cross section", tol)?;
    let (mut start, mut end) = (points[0], points[points.len() - 1]);
    if end.x < start.x {
        std::mem::swap(&mut start, &mut end);
    }
    let chord = start.to(end);
    let width = chord.length();
    let Some(along) = chord.normalised() else {
        return Err(CoreError::validation(
            "the cross section's ends must be apart, it sets the width across",
        ));
    };
    let up = along.perp();
    let pairs: Vec<(f64, f64)> = points
        .iter()
        .map(|p| {
            let v = start.to(*p);
            ((v.dot(along) / width).clamp(0.0, 1.0), v.dot(up))
        })
        .collect();
    Ok(Section {
        table: Table::from_points(&pairs, 1.0, SECTION_SAMPLES),
        width,
    })
}

/// A rail as a polyline from its start (after any reversal and, for a
/// closed rail, the chosen start point) to its end; a closed rail ends
/// where it starts.
struct Rail {
    points: Vec<Point>,
    closed: bool,
    /// Distance along the rail at each point.
    lengths: Vec<f64>,
}

impl Rail {
    fn new(
        path: &Path,
        what: &str,
        reverse: bool,
        start: Option<Point>,
        tol: Tolerance,
    ) -> Result<Rail, CoreError> {
        let [sub] = path.subpaths.as_slice() else {
            return Err(CoreError::validation(format!(
                "the {what} must be a single vector"
            )));
        };
        let mut points = flatten_subpath(sub, tol).points;
        if points.len() < 2 || points.iter().any(|p| !p.is_finite()) {
            return Err(CoreError::validation(format!("the {what} is empty")));
        }
        let closed = sub.closed;
        if closed {
            if let Some(from) = start {
                points = start_at(&points, from);
            }
            let first = points[0];
            points.push(first);
        }
        if reverse {
            points.reverse();
        }
        let mut lengths = Vec::with_capacity(points.len());
        let mut total = 0.0;
        lengths.push(0.0);
        for pair in points.windows(2) {
            total += pair[0].distance(pair[1]);
            lengths.push(total);
        }
        if total <= 0.0 {
            return Err(CoreError::validation(format!("the {what} has no length")));
        }
        Ok(Rail {
            points,
            closed,
            lengths,
        })
    }

    fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The point `f` of the way along (clamped for an open rail, wrapped
    /// for a closed one).
    fn at(&self, f: f64) -> Point {
        let f = if self.closed {
            f.rem_euclid(1.0)
        } else {
            f.clamp(0.0, 1.0)
        };
        let target = f * self.length();
        let k = self
            .lengths
            .partition_point(|&l| l < target)
            .clamp(1, self.points.len() - 1);
        let (l0, l1) = (self.lengths[k - 1], self.lengths[k]);
        let t = if l1 > l0 {
            (target - l0) / (l1 - l0)
        } else {
            0.0
        };
        self.points[k - 1].lerp(self.points[k], t.clamp(0.0, 1.0))
    }
}

/// A closed loop (last point not repeated) started at its point nearest
/// `from`, inserting that point when it falls inside a span.
pub(super) fn start_at(points: &[Point], from: Point) -> Vec<Point> {
    let n = points.len();
    let mut best = (f64::INFINITY, 0usize, points[0]);
    for k in 0..n {
        let (a, b) = (points[k], points[(k + 1) % n]);
        let ab = a.to(b);
        let len2 = ab.dot(ab);
        let t = if len2 > 0.0 {
            (a.to(from).dot(ab) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p = a.lerp(b, t);
        let d = p.distance(from);
        if d < best.0 {
            best = (d, k, p);
        }
    }
    let (_, k, p) = best;
    let mut out = Vec::with_capacity(n + 1);
    out.push(p);
    for s in 1..=n {
        let q = points[(k + s) % n];
        if q.distance(p) > 1e-9 && out.last().is_none_or(|l| l.distance(q) > 1e-9) {
            out.push(q);
        }
    }
    out
}

/// Weights of stations `i - 1 ..= i + 2` for a place `f` of the way from
/// station `i` to `i + 1`: a cubic that passes through every station,
/// flowing through the smooth ones and creased at the others. `smooth`
/// holds the flags of the four stations, false where one does not exist.
fn station_weights(f: f64, smooth: [bool; 4], exists: [bool; 4]) -> [f64; 4] {
    let (f2, f3) = (f * f, f * f * f);
    let h00 = 2.0 * f3 - 3.0 * f2 + 1.0;
    let h10 = f3 - 2.0 * f2 + f;
    let h01 = -2.0 * f3 + 3.0 * f2;
    let h11 = f3 - f2;
    let mut w = [0.0, h00, h01, 0.0];
    // Tangent at station i.
    if smooth[1] && exists[0] {
        w[2] += 0.5 * h10;
        w[0] -= 0.5 * h10;
    } else {
        w[2] += h10;
        w[1] -= h10;
    }
    // Tangent at station i + 1.
    if smooth[2] && exists[3] {
        w[3] += 0.5 * h11;
        w[1] -= 0.5 * h11;
    } else {
        w[2] += h11;
        w[1] -= h11;
    }
    w
}

/// A section as `(across, height)` points, `across` 0 to 1 over its chord,
/// each span sampled at the same count so sections with as many spans can
/// be blended span by span.
fn span_points(path: &Path, tol: Tolerance) -> Option<Vec<(f64, f64)>> {
    const PER_SPAN: usize = 32;
    let sub = path.subpaths.first()?;
    let mut spans: Vec<Vec<Point>> = Vec::new();
    for (start, seg) in sub.iter() {
        let mut points = vec![start];
        crate::geom::flatten::flatten_segment_into(start, seg, tol, &mut points);
        spans.push(points);
    }
    let first = sub.start;
    let last = *spans.last()?.last()?;
    let (start, end) = if last.x < first.x {
        (last, first)
    } else {
        (first, last)
    };
    let chord = start.to(end);
    let width = chord.length();
    let along = chord.normalised()?;
    let up = along.perp();
    let mut out = Vec::new();
    for span in &spans {
        let mut lengths = vec![0.0];
        for pair in span.windows(2) {
            let l = lengths.last().copied().unwrap_or(0.0) + pair[0].distance(pair[1]);
            lengths.push(l);
        }
        let total = lengths.last().copied().unwrap_or(0.0);
        for k in 0..PER_SPAN {
            let target = total * k as f64 / PER_SPAN as f64;
            let j = lengths
                .partition_point(|&l| l < target)
                .clamp(1, span.len() - 1);
            let (l0, l1) = (lengths[j - 1], lengths[j]);
            let t = if l1 > l0 {
                (target - l0) / (l1 - l0)
            } else {
                0.0
            };
            let p = span[j - 1].lerp(span[j], t);
            let v = start.to(p);
            out.push(((v.dot(along) / width).clamp(0.0, 1.0), v.dot(up)));
        }
    }
    let v = start.to(last);
    out.push(((v.dot(along) / width).clamp(0.0, 1.0), v.dot(up)));
    Some(out)
}

/// The sections as they blend along the rails.
pub(super) struct Blend {
    sections: Vec<Section>,
    /// Stations in order along the first rail.
    stations: Vec<Station>,
    /// The rails are closed: the last station blends back into the first.
    wraps: bool,
    /// Span by span: per interval between stations, section tables at
    /// even steps from one station to the next.
    levels: Option<Vec<Vec<Table>>>,
}

/// Steps between two stations when blending span by span.
const SPAN_LEVELS: usize = 256;

impl Blend {
    /// The four stations around interval `i` and whether each exists.
    fn around(&self, i: usize) -> ([usize; 4], [bool; 4]) {
        let n = self.stations.len() as i64;
        let pick = |k: i64| -> (usize, bool) {
            if self.wraps {
                (k.rem_euclid(n) as usize, true)
            } else if k < 0 || k >= n {
                (k.clamp(0, n - 1) as usize, false)
            } else {
                (k as usize, true)
            }
        };
        let i = i as i64;
        let (a, ea) = pick(i - 1);
        let (b, eb) = pick(i);
        let (c, ec) = pick(i + 1);
        let (d, ed) = pick(i + 2);
        ([a, b, c, d], [ea, eb, ec, ed])
    }

    fn weights(&self, i: usize, f: f64) -> ([usize; 4], [f64; 4]) {
        let (idx, exists) = self.around(i);
        let smooth = [0, 1, 2, 3].map(|k| exists[k] && self.stations[idx[k]].smooth);
        (idx, station_weights(f, smooth, exists))
    }

    /// The blend of `sections` through `stations` (sorted along the rail),
    /// span by span when asked and the sections allow it.
    pub(super) fn new(
        sections: &[Path],
        stations: Vec<Station>,
        wraps: bool,
        between_spans: bool,
        tol: Tolerance,
    ) -> Result<Blend, CoreError> {
        let levels = if between_spans {
            span_levels(sections, &stations, wraps, tol)
        } else {
            None
        };
        Ok(Blend {
            sections: sections
                .iter()
                .map(|s| section(s, tol))
                .collect::<Result<Vec<_>, _>>()?,
            stations,
            wraps,
            levels,
        })
    }

    /// The stations, in order along the rail.
    pub(super) fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// The widest drawn section, millimetres.
    pub(super) fn widest(&self) -> f64 {
        self.sections.iter().map(|s| s.width).fold(0.0, f64::max)
    }

    /// Height as drawn and drawn width at station position `pos` (station
    /// `i` at `i`, blending towards `i + 1`) and `u` across.
    pub(super) fn at(&self, pos: f64, u: f64) -> (f64, f64) {
        let n = self.stations.len();
        let last = if self.wraps { n } else { n - 1 };
        let pos = pos.clamp(0.0, last as f64);
        let i = (pos.floor() as usize).min(last.saturating_sub(1));
        let f = if last == 0 { 0.0 } else { pos - i as f64 };
        let width_of = |k: usize| self.sections[self.stations[k].section].width;
        if n == 1 {
            let s = &self.sections[self.stations[0].section];
            return (s.table.at(u), s.width);
        }
        let (idx, w) = self.weights(i, f);
        let wa = width_of(idx[1]);
        let wb = width_of(idx[2]);
        let width = wa + (wb - wa) * f;
        if let Some(levels) = &self.levels {
            let tables = &levels[i];
            let x = f * (SPAN_LEVELS - 1) as f64;
            let k = (x.floor() as usize).min(SPAN_LEVELS - 2);
            let g = x - k as f64;
            let (a, b) = (tables[k].at(u), tables[k + 1].at(u));
            let h = match (a.is_nan(), b.is_nan()) {
                (false, false) => a + (b - a) * g,
                (false, true) => a,
                (true, false) => b,
                _ => f64::NAN,
            };
            return (h, width);
        }
        let mut h = 0.0;
        for (k, weight) in idx.iter().zip(w) {
            if weight == 0.0 {
                continue;
            }
            let v = self.sections[self.stations[*k].section].table.at(u);
            if v.is_nan() {
                // Where a section does not reach, blend the two nearest
                // straight, as far as they reach.
                let (ha, hb) = (
                    self.sections[self.stations[idx[1]].section].table.at(u),
                    self.sections[self.stations[idx[2]].section].table.at(u),
                );
                let h = match (ha.is_nan(), hb.is_nan()) {
                    (false, false) => ha + (hb - ha) * f,
                    (false, true) => ha,
                    (true, false) => hb,
                    _ => f64::NAN,
                };
                return (h, width);
            }
            h += weight * v;
        }
        (h, width)
    }
}

/// Stations sorted along the first rail, or the default spread.
fn stations_for(
    params: &SweepParams,
    count: usize,
    closed: bool,
) -> Result<Vec<Station>, CoreError> {
    let mut stations = params.stations.clone();
    if stations.is_empty() {
        stations = if count == 1 && !closed {
            [0.0, 1.0]
                .into_iter()
                .map(|f| Station {
                    section: 0,
                    a: f,
                    b: f,
                    smooth: true,
                })
                .collect()
        } else {
            let spread = if closed { count } else { count - 1 };
            (0..count)
                .map(|k| {
                    let f = if spread == 0 {
                        0.0
                    } else {
                        k as f64 / spread as f64
                    };
                    Station {
                        section: k,
                        a: f,
                        b: f,
                        smooth: true,
                    }
                })
                .collect()
        };
    }
    for s in &stations {
        if s.section >= count {
            return Err(CoreError::validation(
                "a cross section position names a section that is not there",
            ));
        }
        if !(s.a.is_finite()
            && s.b.is_finite()
            && (0.0..=1.0).contains(&s.a)
            && (0.0..=1.0).contains(&s.b))
        {
            return Err(CoreError::validation(
                "cross sections must sit between the start and the end of the rails",
            ));
        }
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
    Ok(stations)
}

/// Where along both rails and between which stations step `s` (0 to 1
/// along the first rail) lies: `(fraction on the second rail, station
/// position)`.
pub(super) fn place(stations: &[Station], wraps: bool, s: f64) -> (f64, f64) {
    let n = stations.len();
    let first = stations[0];
    let last = stations[n - 1];
    if n == 1 {
        return (s - first.a + first.b, 0.0);
    }
    if !wraps {
        if s <= first.a {
            let f = if first.a > 0.0 { s / first.a } else { 1.0 };
            return (first.b * f, 0.0);
        }
        if s >= last.a {
            let f = if last.a < 1.0 {
                (s - last.a) / (1.0 - last.a)
            } else {
                0.0
            };
            return (last.b + (1.0 - last.b) * f, (n - 1) as f64);
        }
    }
    // Between two stations; a closed rail wraps from the last to the first.
    let mut s = s;
    if wraps && s < first.a {
        s += 1.0;
    }
    for i in 0..n {
        let (p, q) = if i + 1 < n {
            (stations[i], stations[i + 1])
        } else if wraps {
            let mut q = first;
            q.a += 1.0;
            if q.b <= last.b {
                q.b += 1.0;
            }
            (last, q)
        } else {
            break;
        };
        if s >= p.a && s <= q.a {
            let f = if q.a > p.a {
                (s - p.a) / (q.a - p.a)
            } else {
                0.0
            };
            return (p.b + (q.b - p.b) * f, i as f64 + f);
        }
    }
    (s, (n - 1) as f64)
}

/// Builds the heights of a two-rail sweep over the whole grid, NaN where
/// the sweep does not reach.
///
/// Rails are open or closed vectors in job space. Each section is an open
/// vector whose chord (start to end) is the width across and whose offset
/// from the chord is the height, in millimetres. Sections sit on the rails
/// at their stations (or spread evenly) and blend from one to the next.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an empty rail or section, no
/// sections, a section with no width, a station off the rails, one rail
/// closed and the other open, or a base height that is not a number.
pub fn two_rail_sweep(
    rail_a: &Path,
    rail_b: &Path,
    sections: &[Path],
    grid: &Grid,
    params: &SweepParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    if !params.base_height.is_finite() {
        return Err(CoreError::validation("the base height must be a number"));
    }
    if params.exact_height && !(params.height.is_finite() && params.height > 0.0) {
        return Err(CoreError::validation(
            "the exact height must be more than zero",
        ));
    }
    let a = Rail::new(
        rail_a,
        "first rail",
        params.reverse[0],
        params.starts[0],
        tol,
    )?;
    let b = Rail::new(
        rail_b,
        "second rail",
        params.reverse[1],
        params.starts[1],
        tol,
    )?;
    if a.closed != b.closed {
        return Err(CoreError::validation(
            "the rails must both be open or both be closed",
        ));
    }
    if sections.is_empty() {
        return Err(CoreError::validation(
            "a sweep needs at least one cross section",
        ));
    }
    let wraps = a.closed;
    let stations = stations_for(params, sections.len(), wraps)?;
    let blend = Blend::new(sections, stations, wraps, params.sweep_between_spans, tol)?;

    let longest = a.length().max(b.length());
    let steps = ((longest / (grid.cell * 0.5)).ceil() as usize).clamp(1, MAX_RAIL_STEPS);
    let mut pa = Vec::with_capacity(steps + 1);
    let mut pb = Vec::with_capacity(steps + 1);
    let mut pos = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let s = k as f64 / steps as f64;
        let (fb, at) = place(&blend.stations, wraps, s);
        pa.push(a.at(s));
        pb.push(b.at(fb));
        pos.push(at);
    }
    if wraps {
        // Round the whole loop back into the first station.
        let n = blend.stations.len();
        if n > 1 {
            if let Some(p) = pos.last_mut() {
                if *p < 1e-9 {
                    *p = n as f64;
                }
            }
        }
    }
    let spans: Vec<f64> = pa.iter().zip(&pb).map(|(p, q)| p.distance(*q)).collect();

    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    let mut inner_heights = (0.0, 0usize);
    let inner_side = if params.fill_inner && wraps {
        inner_rail(&a, &b)
    } else {
        None
    };
    let mut paint = |corners: [Point; 3], tu: [(f64, f64); 3], span: [f64; 3]| {
        let [p0, p1, p2] = corners;
        let e1 = p0.to(p1);
        let e2 = p0.to(p2);
        let det = e1.cross(e2);
        if det.abs() < 1e-12 {
            return;
        }
        let xs = [p0.x, p1.x, p2.x];
        let ys = [p0.y, p1.y, p2.y];
        let lo = |v: [f64; 3]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = |v: [f64; 3]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let to_i = |x: f64| (x - grid.origin.x) / grid.cell;
        let to_j = |y: f64| (y - grid.origin.y) / grid.cell;
        let i0 = to_i(lo(xs)).ceil().max(0.0);
        let i1 = to_i(hi(xs)).floor().min(grid.width as f64 - 1.0);
        let j0 = to_j(lo(ys)).ceil().max(0.0);
        let j1 = to_j(hi(ys)).floor().min(grid.height as f64 - 1.0);
        if i1 < i0 || j1 < j0 {
            return;
        }
        let eps = 1e-9;
        for j in j0 as usize..=j1 as usize {
            for i in i0 as usize..=i1 as usize {
                let c = field.centre(i, j);
                let v = p0.to(c);
                let w1 = v.cross(e2) / det;
                let w2 = e1.cross(v) / det;
                let w0 = 1.0 - w1 - w2;
                if w0 < -eps || w1 < -eps || w2 < -eps {
                    continue;
                }
                let t = w0 * tu[0].0 + w1 * tu[1].0 + w2 * tu[2].0;
                let u = w0 * tu[0].1 + w1 * tu[1].1 + w2 * tu[2].1;
                let gap = w0 * span[0] + w1 * span[1] + w2 * span[2];
                let (h, drawn) = blend.at(t, u.clamp(0.0, 1.0));
                if h.is_nan() {
                    continue;
                }
                let scale = if params.scale_height_with_width && drawn > 0.0 {
                    gap / drawn
                } else {
                    1.0
                };
                let z = (params.base_height + h * scale) as f32;
                let cell = &mut field.z[j * grid.width + i];
                if cell.is_nan() || z > *cell {
                    *cell = z;
                }
            }
        }
    };
    for k in 0..steps {
        let (t0, t1) = (pos[k], pos[k + 1]);
        paint(
            [pa[k], pb[k], pb[k + 1]],
            [(t0, 0.0), (t0, 1.0), (t1, 1.0)],
            [spans[k], spans[k], spans[k + 1]],
        );
        paint(
            [pa[k], pb[k + 1], pa[k + 1]],
            [(t0, 0.0), (t1, 1.0), (t1, 0.0)],
            [spans[k], spans[k + 1], spans[k + 1]],
        );
    }
    if let Some(side) = inner_side {
        // The height the sections reach along the inner rail.
        let u = if side == 0 { 0.0 } else { 1.0 };
        for k in 0..=steps {
            let (h, drawn) = blend.at(pos[k], u);
            if h.is_finite() {
                let scale = if params.scale_height_with_width && drawn > 0.0 {
                    spans[k] / drawn
                } else {
                    1.0
                };
                inner_heights.0 += h * scale;
                inner_heights.1 += 1;
            }
        }
        if inner_heights.1 > 0 {
            let level = (params.base_height + inner_heights.0 / inner_heights.1 as f64) as f32;
            let loop_points = if side == 0 { &a.points } else { &b.points };
            let polygon = &loop_points[..loop_points.len() - 1];
            for j in 0..grid.height {
                for i in 0..grid.width {
                    let index = j * grid.width + i;
                    if field.z[index].is_nan() && inside(polygon, field.centre(i, j)) {
                        field.z[index] = level;
                    }
                }
            }
        }
    }
    if params.exact_height {
        let base = params.base_height as f32;
        let top = field
            .z
            .iter()
            .filter(|z| !z.is_nan())
            .fold(f32::NEG_INFINITY, |m, &z| m.max(z - base));
        if top > 1e-9 {
            let factor = params.height as f32 / top;
            for z in field.z.iter_mut().filter(|z| !z.is_nan()) {
                *z = base + (*z - base) * factor;
            }
        }
    }
    Ok(field)
}

/// Which of two closed rails lies inside the other: 0 for the first, 1
/// for the second, none when neither does.
fn inner_rail(a: &Rail, b: &Rail) -> Option<usize> {
    let loop_of = |r: &Rail| r.points[..r.points.len() - 1].to_vec();
    let (la, lb) = (loop_of(a), loop_of(b));
    if inside(&lb, la[0]) {
        Some(0)
    } else if inside(&la, lb[0]) {
        Some(1)
    } else {
        None
    }
}

/// Even-odd point in polygon.
fn inside(polygon: &[Point], p: Point) -> bool {
    let mut odd = false;
    let n = polygon.len();
    for k in 0..n {
        let (a, b) = (polygon[k], polygon[(k + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            odd = !odd;
        }
    }
    odd
}

/// Span by span section tables between every pair of stations, when every
/// section has the same number of spans; none otherwise.
fn span_levels(
    sections: &[Path],
    stations: &[Station],
    wraps: bool,
    tol: Tolerance,
) -> Option<Vec<Vec<Table>>> {
    let spans: Vec<usize> = sections
        .iter()
        .map(|p| p.subpaths.first().map_or(0, |s| s.segments.len()))
        .collect();
    if spans.iter().any(|&s| s != spans[0]) || spans[0] == 0 {
        return None;
    }
    let points: Vec<Vec<(f64, f64)>> = sections
        .iter()
        .map(|p| span_points(p, tol))
        .collect::<Option<Vec<_>>>()?;
    let n = stations.len();
    let intervals = if wraps { n } else { n.saturating_sub(1) };
    let blend = Blend {
        sections: Vec::new(),
        stations: stations.to_vec(),
        wraps,
        levels: None,
    };
    let mut out = Vec::with_capacity(intervals);
    for i in 0..intervals {
        let (idx, _) = blend.around(i);
        let mut tables = Vec::with_capacity(SPAN_LEVELS);
        for level in 0..SPAN_LEVELS {
            let f = level as f64 / (SPAN_LEVELS - 1) as f64;
            let (_, w) = blend.weights(i, f);
            let count = points[stations[idx[1]].section].len();
            let morphed: Vec<(f64, f64)> = (0..count)
                .map(|m| {
                    let mut p = (0.0, 0.0);
                    for (k, weight) in idx.iter().zip(w) {
                        let q = points[stations[*k].section][m];
                        p.0 += weight * q.0;
                        p.1 += weight * q.1;
                    }
                    (p.0.clamp(0.0, 1.0), p.1)
                })
                .collect();
            tables.push(Table::from_points(&morphed, 1.0, SECTION_SAMPLES));
        }
        out.push(tables);
    }
    Some(out)
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
            width: (60.0 / cell) as usize,
            height: (60.0 / cell) as usize,
            cell,
            origin: Point::new(cell / 2.0, cell / 2.0),
        }
    }

    fn semicircle() -> Path {
        let points: Vec<Point> = (0..=180)
            .map(|k| {
                let a = std::f64::consts::PI * (1.0 - f64::from(k) / 180.0);
                Point::new(5.0 + 5.0 * a.cos(), 5.0 * a.sin())
            })
            .collect();
        polyline_path(&points, false).expect("semicircle")
    }

    #[test]
    fn a_semicircle_between_parallel_rails_is_a_half_cylinder() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let field = two_rail_sweep(
            &a,
            &b,
            &[semicircle()],
            &grid(0.1),
            &SweepParams::default(),
            tol(),
        )
        .expect("sweep");
        for x in [15.0, 30.0, 45.0] {
            let crest = f64::from(field.at(Point::new(x, 30.0)));
            assert!((crest - 5.0).abs() < 0.1, "{x}: {crest}");
            let off = f64::from(field.at(Point::new(x, 33.0)));
            assert!((off - 16f64.sqrt()).abs() < 0.1, "{x}: {off}");
        }
        assert!(field.at(Point::new(30.0, 40.0)).is_nan());
        assert!(field.at(Point::new(5.0, 30.0)).is_nan());
    }

    #[test]
    fn rails_of_different_lengths_still_cover() {
        let a = line(Point::new(20.0, 20.0), Point::new(40.0, 20.0));
        let b = line(Point::new(10.0, 40.0), Point::new(50.0, 40.0));
        let field = two_rail_sweep(
            &a,
            &b,
            &[semicircle()],
            &grid(0.2),
            &SweepParams::default(),
            tol(),
        )
        .expect("sweep");
        for x in [16.0, 30.0, 44.0] {
            assert!(!field.at(Point::new(x, 37.0)).is_nan(), "{x}");
        }
        assert!(!field.at(Point::new(30.0, 30.0)).is_nan());
    }

    #[test]
    fn scaling_with_width_raises_the_section_where_the_rails_part() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 55.0));
        let scaled = two_rail_sweep(
            &a,
            &b,
            &[semicircle()],
            &grid(0.2),
            &SweepParams::default(),
            tol(),
        )
        .expect("sweep");
        let kept = SweepParams {
            scale_height_with_width: false,
            ..SweepParams::default()
        };
        let fixed =
            two_rail_sweep(&a, &b, &[semicircle()], &grid(0.2), &kept, tol()).expect("sweep");
        let wide_scaled = scaled.max().unwrap_or(0.0);
        let wide_fixed = fixed.max().unwrap_or(0.0);
        assert!(wide_scaled > 12.0, "{wide_scaled}");
        assert!((f64::from(wide_fixed) - 5.0).abs() < 0.1, "{wide_fixed}");
    }

    #[test]
    fn several_sections_blend_along_the_rails() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let flat = polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(0.0, 1.0),
                Point::new(10.0, 1.0),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("flat");
        let tall = polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(0.0, 3.0),
                Point::new(10.0, 3.0),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("tall");
        let kept = SweepParams {
            scale_height_with_width: false,
            ..SweepParams::default()
        };
        let field = two_rail_sweep(&a, &b, &[flat, tall], &grid(0.2), &kept, tol()).expect("sweep");
        let middle = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((middle - 2.0).abs() < 0.05, "{middle}");
    }

    #[test]
    fn bad_input_is_refused() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let closed = rect(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).expect("rect");
        let p = SweepParams::default();
        let g = grid(0.5);
        assert!(matches!(
            two_rail_sweep(&closed, &b, &[semicircle()], &g, &p, tol()),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            two_rail_sweep(&a, &b, &[Path::new()], &g, &p, tol()),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            two_rail_sweep(&a, &b, &[], &g, &p, tol()),
            Err(CoreError::Validation(_))
        ));
    }

    fn circle_path(r: f64) -> Path {
        crate::geom::circle(Point::new(30.0, 30.0), r).expect("circle")
    }

    #[test]
    fn closed_rails_sweep_all_round_and_can_fill_the_inside() {
        // A ramp from the outer rail up to a 3 mm wall at the inner one.
        let ramp = polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 3.0),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("ramp");
        let params = SweepParams {
            scale_height_with_width: false,
            ..SweepParams::default()
        };
        let field = two_rail_sweep(
            &circle_path(20.0),
            &circle_path(10.0),
            std::slice::from_ref(&ramp),
            &grid(0.2),
            &params,
            tol(),
        )
        .expect("sweep");
        for p in [
            Point::new(45.0, 30.0),
            Point::new(30.0, 15.0),
            Point::new(15.0, 30.0),
        ] {
            let h = f64::from(field.at(p));
            assert!((h - 1.5).abs() < 0.2, "{p:?}: {h}");
        }
        assert!(
            field.at(Point::new(30.0, 30.0)).is_nan(),
            "the middle stays open"
        );
        let filled = SweepParams {
            fill_inner: true,
            ..params.clone()
        };
        let field = two_rail_sweep(
            &circle_path(20.0),
            &circle_path(10.0),
            &[ramp],
            &grid(0.2),
            &filled,
            tol(),
        )
        .expect("sweep");
        let middle = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((middle - 3.0).abs() < 0.1, "{middle}");
        let open = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        assert!(two_rail_sweep(
            &open,
            &circle_path(10.0),
            &[semicircle()],
            &grid(0.5),
            &params,
            tol()
        )
        .is_err());
    }

    fn block(height: f64) -> Path {
        polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(0.0, height),
                Point::new(10.0, height),
                Point::new(10.0, 0.0),
            ],
            false,
        )
        .expect("block")
    }

    #[test]
    fn sections_sit_where_their_stations_put_them() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let station = |section, f: f64, smooth| Station {
            section,
            a: f,
            b: f,
            smooth,
        };
        let creased = SweepParams {
            scale_height_with_width: false,
            stations: vec![
                station(0, 0.0, false),
                station(1, 0.25, false),
                station(0, 1.0, false),
            ],
            ..SweepParams::default()
        };
        let sections = [block(1.0), block(3.0)];
        let field = two_rail_sweep(&a, &b, &sections, &grid(0.2), &creased, tol()).expect("sweep");
        let at = |x: f64| f64::from(field.at(Point::new(x, 30.0)));
        assert!(
            (at(20.0) - 3.0).abs() < 0.05,
            "the tall section at a quarter"
        );
        assert!(
            (at(15.0) - 2.0).abs() < 0.1,
            "straight between: {}",
            at(15.0)
        );
        assert!((at(35.0) - 2.0).abs() < 0.1, "{}", at(35.0));
        // Smooth through the tall section: the blend bulges past straight.
        let smooth = SweepParams {
            stations: creased
                .stations
                .iter()
                .map(|s| Station { smooth: true, ..*s })
                .collect(),
            ..creased.clone()
        };
        let field = two_rail_sweep(&a, &b, &sections, &grid(0.2), &smooth, tol()).expect("sweep");
        let s35 = f64::from(field.at(Point::new(35.0, 30.0)));
        assert!(s35 > 2.1, "{s35}");
        // A station off the rails, or naming no section, is refused.
        let bad = SweepParams {
            stations: vec![station(3, 0.5, true)],
            ..SweepParams::default()
        };
        assert!(two_rail_sweep(&a, &b, &sections, &grid(0.5), &bad, tol()).is_err());
    }

    #[test]
    fn a_reversed_rail_twists_the_sweep_and_a_start_point_turns_a_closed_one() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let reversed = SweepParams {
            reverse: [false, true],
            ..SweepParams::default()
        };
        let field =
            two_rail_sweep(&a, &b, &[semicircle()], &grid(0.2), &reversed, tol()).expect("sweep");
        // The rails now cross over: the middle is covered, the ends only
        // along the diagonals.
        assert!(!field.at(Point::new(30.0, 30.0)).is_nan());
        let r = Rail::new(
            &circle_path(10.0),
            "rail",
            false,
            Some(Point::new(30.0, 45.0)),
            tol(),
        )
        .expect("rail");
        let p = r.points[0];
        assert!(
            (p.x - 30.0).abs() < 1e-6 && (p.y - 40.0).abs() < 1e-6,
            "{p:?}"
        );
        let q = r.at(1.0);
        assert!(q.distance(p) < 1e-9, "closed: ends where it starts");
    }

    #[test]
    fn exact_height_scales_the_top() {
        let a = line(Point::new(10.0, 25.0), Point::new(50.0, 25.0));
        let b = line(Point::new(10.0, 35.0), Point::new(50.0, 35.0));
        let params = SweepParams {
            exact_height: true,
            height: 2.0,
            base_height: 1.0,
            ..SweepParams::default()
        };
        let field =
            two_rail_sweep(&a, &b, &[semicircle()], &grid(0.2), &params, tol()).expect("sweep");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 3.0).abs() < 1e-4);
        let bad = SweepParams {
            height: 0.0,
            ..params
        };
        assert!(two_rail_sweep(&a, &b, &[semicircle()], &grid(0.5), &bad, tol()).is_err());
    }

    #[test]
    fn sweeping_between_spans_moves_each_span_into_the_next() {
        let a = line(Point::new(10.0, 20.0), Point::new(50.0, 20.0));
        let b = line(Point::new(10.0, 40.0), Point::new(50.0, 40.0));
        let bump = |x0: f64| {
            polyline_path(
                &[
                    Point::new(0.0, 0.0),
                    Point::new(x0, 0.0),
                    Point::new(x0 + 2.0, 4.0),
                    Point::new(x0 + 4.0, 0.0),
                    Point::new(20.0, 0.0),
                ],
                false,
            )
            .expect("bump")
        };
        let sections = [bump(1.0), bump(15.0)];
        let plain = SweepParams {
            scale_height_with_width: false,
            ..SweepParams::default()
        };
        let spans = SweepParams {
            sweep_between_spans: true,
            ..plain.clone()
        };
        // Halfway along, the bump has moved to the middle across.
        let field = two_rail_sweep(&a, &b, &sections, &grid(0.2), &spans, tol()).expect("sweep");
        let middle = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((middle - 4.0).abs() < 0.3, "{middle}");
        let field = two_rail_sweep(&a, &b, &sections, &grid(0.2), &plain, tol()).expect("sweep");
        let middle = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!(middle < 0.5, "{middle}");
    }
}
