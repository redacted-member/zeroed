//! Texture toolpaths: short strokes scattered over a closed region at
//! varying depths, giving the surface a hand tooled look.
//!
//! Strokes are laid along rows `spacing` millimetres apart, the rows running
//! in the stroke direction and centred across the region. Along a row the
//! strokes follow one another with a gap between each of 15 to 35 percent
//! of the stroke length, every other row starts half a stroke along so that
//! the rows are staggered, and each stroke's centre is nudged across its row
//! by up to `stepover_variation_percent` of the spacing (a tenth when unset). A stroke's length is `stroke_length`
//! varied by up to `length_variation` of itself either way, its direction
//! the row's turned by up to `angle_variation_deg` either way, and its
//! deepest point `depth` less up to `depth_variation` of it.
//!
//! Each stroke dips in a smooth arc: its depth at `t` of the way along is
//! `deepest * sin(pi t)` below the start depth, so it enters at the start
//! depth (the surface by default), is deepest in the middle and rises out. Strokes are then trimmed to the region shrunk by
//! the tool's cutting radius at the full texture depth, so no cut spills past
//! the vectors, and holes are left alone; a trimmed stroke keeps its arc and
//! enters at the depth the arc has where it crosses the boundary.
//!
//! The random choices come from a small local generator (splitmix64) seeded
//! by `seed`, so the same settings always give the same moves. Rows are cut
//! in turn, every other one the opposite way, to keep the rapids short.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Region, Tolerance};

use super::chain::CutSeg;
use super::common::{Emitter, Heights};
use super::profile::Calculated;
use super::tool::Tool;

/// The Texture form's settings. Lengths are millimetres, angles degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextureParams {
    /// Direction the strokes and their rows run, in degrees counter
    /// clockwise from the X axis.
    pub angle_deg: f64,
    /// Most a stroke's direction may turn from `angle_deg` either way, in
    /// degrees.
    pub angle_variation_deg: f64,
    /// Typical length of a stroke, in millimetres.
    pub stroke_length: f64,
    /// Fraction of the stroke length (0 to 1) a stroke may be longer or
    /// shorter by.
    pub length_variation: f64,
    /// Distance between the rows of strokes, in millimetres.
    pub spacing: f64,
    /// Most a stroke moves across its row either way, as a percentage (0 to
    /// 50) of `spacing`; a tenth of it when absent, as older toolpaths had.
    pub stepover_variation_percent: Option<f64>,
    /// Depth below the material surface the strokes start from, in
    /// millimetres.
    pub start_depth: f64,
    /// Deepest a stroke cuts below the surface, in millimetres.
    pub depth: f64,
    /// Fraction of the depth (0 to 1) a stroke may be shallower by.
    pub depth_variation: f64,
    /// Seed of the random choices; the same seed gives the same strokes.
    pub seed: u64,
    /// Shortest stroke as a percentage of `stroke_length`, which is then
    /// the longest; used instead of `length_variation` when set.
    pub min_length_percent: Option<f64>,
    /// Shallowest stroke as a percentage of `depth`, which is then the
    /// deepest; used instead of `depth_variation` when set.
    pub min_depth_percent: Option<f64>,
    /// Most a stroke overlaps the one before it along its row, as a
    /// percentage of its length; when absent the strokes have gaps of 15 to
    /// 35 percent between them.
    pub max_overlap_percent: Option<f64>,
    /// How much the overlap varies, as a percentage: 0 always overlaps by
    /// the maximum, 100 anywhere from none to the maximum.
    pub overlap_variation_percent: f64,
    /// Distance the strokes keep in from the vectors, in millimetres.
    pub boundary_offset: f64,
    /// Cut along the selected vectors themselves instead of scattering
    /// strokes inside them.
    pub vectors_as_pattern: bool,
    /// With a vector pattern, dip into and out of each vector like a flute;
    /// otherwise cut each one at its depth.
    pub ramp: bool,
}

impl Default for TextureParams {
    fn default() -> Self {
        TextureParams {
            angle_deg: 0.0,
            angle_variation_deg: 10.0,
            stroke_length: 10.0,
            length_variation: 0.3,
            spacing: 3.0,
            stepover_variation_percent: None,
            start_depth: 0.0,
            depth: 1.0,
            depth_variation: 0.5,
            seed: 1,
            min_length_percent: None,
            min_depth_percent: None,
            max_overlap_percent: None,
            overlap_variation_percent: 100.0,
            boundary_offset: 0.0,
            vectors_as_pattern: false,
            ramp: true,
        }
    }
}

impl TextureParams {
    /// Checks the settings.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a stroke length, spacing or
    /// depth that is not more than zero, a variation outside 0 to 1, or an
    /// angle variation outside 0 to 90 degrees.
    pub fn validate(&self) -> Result<(), CoreError> {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        let fraction = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        if !positive(self.stroke_length) {
            return Err(CoreError::validation(
                "the stroke length must be more than zero",
            ));
        }
        if !positive(self.spacing) {
            return Err(CoreError::validation("the spacing must be more than zero"));
        }
        if !positive(self.depth) {
            return Err(CoreError::validation("the depth must be more than zero"));
        }
        if !self.angle_deg.is_finite() {
            return Err(CoreError::validation("the angle must be a number"));
        }
        if !(self.angle_variation_deg.is_finite()
            && (0.0..=90.0).contains(&self.angle_variation_deg))
        {
            return Err(CoreError::validation(
                "the angle variation must be between 0 and 90 degrees",
            ));
        }
        if !fraction(self.length_variation) {
            return Err(CoreError::validation(
                "the length variation must be between 0 and 1",
            ));
        }
        if !fraction(self.depth_variation) {
            return Err(CoreError::validation(
                "the depth variation must be between 0 and 1",
            ));
        }
        let percent = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v > 0.0 && v <= 100.0);
        if !percent(self.min_length_percent) || !percent(self.min_depth_percent) {
            return Err(CoreError::validation(
                "the minimum length and depth must be more than 0 and at most 100 percent",
            ));
        }
        if !self
            .max_overlap_percent
            .is_none_or(|v| v.is_finite() && (0.0..=100.0).contains(&v))
            || !(self.overlap_variation_percent.is_finite()
                && (0.0..=100.0).contains(&self.overlap_variation_percent))
        {
            return Err(CoreError::validation(
                "the overlap and its variation must be between 0 and 100 percent",
            ));
        }
        if !self
            .stepover_variation_percent
            .is_none_or(|v| v.is_finite() && (0.0..=50.0).contains(&v))
        {
            return Err(CoreError::validation(
                "the stepover variation must be between 0 and 50 percent",
            ));
        }
        if !(self.start_depth.is_finite() && self.start_depth >= 0.0) {
            return Err(CoreError::validation(
                "the start depth must be zero or more",
            ));
        }
        if !(self.boundary_offset.is_finite() && self.boundary_offset >= 0.0) {
            return Err(CoreError::validation(
                "the boundary offset must be zero or more",
            ));
        }
        Ok(())
    }

    /// A stroke length for the random fraction `r` in [0, 1) and `signed`
    /// in [-1, 1).
    fn length_for(&self, r: f64, signed: f64) -> f64 {
        let length = self.stroke_length;
        match self.min_length_percent {
            Some(p) => length * (p / 100.0 + (1.0 - p / 100.0) * r),
            None => (length * (1.0 + self.length_variation * signed)).max(length * 0.05),
        }
    }

    /// A stroke's deepest point for the random fraction `r` in [0, 1).
    fn depth_for(&self, r: f64) -> f64 {
        match self.min_depth_percent {
            Some(p) => self.depth * (p / 100.0 + (1.0 - p / 100.0) * r),
            None => self.depth * (1.0 - self.depth_variation * r),
        }
    }

    /// The space after a stroke of `length` before the next one starts,
    /// negative for an overlap, for the random fraction `r` in [0, 1).
    fn gap_for(&self, length: f64, base: f64, r: f64) -> f64 {
        match self.max_overlap_percent {
            Some(most) => {
                let overlap = most / 100.0 * (1.0 - self.overlap_variation_percent / 100.0 * r);
                -overlap * length
            }
            None => base * (0.15 + 0.2 * r),
        }
    }
}

/// splitmix64: small, fast and the same on every platform.
struct Random(u64);

impl Random {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [-1, 1).
    fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
}

/// Even-odd point in polygon test against flattened loops.
fn inside(polygons: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for poly in polygons {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                odd = !odd;
            }
        }
    }
    odd
}

/// One stroke before trimming: its ends and deepest depth below the
/// surface, in millimetres.
#[derive(Debug, Clone, Copy)]
struct Stroke {
    from: Point,
    to: Point,
    deepest: f64,
}

impl Stroke {
    fn at(&self, t: f64) -> (Point, f64) {
        (self.from.lerp(self.to, t), self.deepest * (PI * t).sin())
    }
}

/// The strokes of every row, row by row, each row in cutting order.
fn lay_strokes(polygons: &[Vec<Point>], params: &TextureParams) -> Vec<Vec<Stroke>> {
    let angle = params.angle_deg.to_radians();
    let (u, v) = ((angle.cos(), angle.sin()), (-angle.sin(), angle.cos()));
    let along = |p: Point| p.x * u.0 + p.y * u.1;
    let across = |p: Point| p.x * v.0 + p.y * v.1;
    let points = polygons.iter().flatten();
    let (mut u_min, mut u_max, mut v_min, mut v_max) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &p in points {
        u_min = u_min.min(along(p));
        u_max = u_max.max(along(p));
        v_min = v_min.min(across(p));
        v_max = v_max.max(across(p));
    }
    if u_min > u_max {
        return Vec::new();
    }
    let span = v_max - v_min;
    let rows = ((span / params.spacing).floor() as usize).max(1);
    let first = v_min + (span - (rows - 1) as f64 * params.spacing) / 2.0;
    let length = params.stroke_length;
    let at = |s: f64, w: f64| Point::new(s * u.0 + w * v.0, s * u.1 + w * v.1);

    let mut random = Random(params.seed);
    let mut out = Vec::with_capacity(rows);
    for row in 0..rows {
        let w = first + row as f64 * params.spacing;
        let stagger = if row % 2 == 1 { 0.5 } else { 0.0 };
        let mut s = u_min - length * (1.0 + stagger + 0.3 * random.unit());
        let mut strokes = Vec::new();
        while s < u_max + length {
            let (r, signed) = {
                let signed = random.signed();
                ((signed + 1.0) / 2.0, signed)
            };
            let this_length = params.length_for(r, signed);
            let turn =
                (params.angle_deg + params.angle_variation_deg * random.signed()).to_radians();
            let deepest = params.depth_for(random.unit());
            let reach = params.stepover_variation_percent.map_or(0.1, |p| p / 100.0);
            let nudge = reach * params.spacing * random.signed();
            let gap = params.gap_for(this_length, length, random.unit());
            let centre = at(s + this_length / 2.0, w + nudge);
            let half = (
                turn.cos() * this_length / 2.0,
                turn.sin() * this_length / 2.0,
            );
            strokes.push(Stroke {
                from: Point::new(centre.x - half.0, centre.y - half.1),
                to: Point::new(centre.x + half.0, centre.y + half.1),
                deepest,
            });
            // Always move on, however much the strokes overlap.
            s += (this_length + gap).max(this_length * 0.05);
        }
        if row % 2 == 1 {
            strokes.reverse();
            for stroke in &mut strokes {
                std::mem::swap(&mut stroke.from, &mut stroke.to);
            }
        }
        out.push(strokes);
    }
    out
}

/// Where between `a` and `b` (fractions along a stroke, `a` inside and `b`
/// not, or the other way) the stroke crosses the boundary.
fn crossing(polygons: &[Vec<Point>], stroke: &Stroke, a: f64, b: f64) -> f64 {
    let (mut lo, mut hi) = (a, b);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if inside(polygons, stroke.at(mid).0) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The pieces of a stroke that lie inside the region, each as points with
/// their depth below the surface, in cutting order.
fn trim(polygons: &[Vec<Point>], stroke: &Stroke, step: f64) -> Vec<Vec<(Point, f64)>> {
    let length = stroke.from.distance(stroke.to);
    let n = ((length / step).ceil() as usize).clamp(12, 400);
    let mut pieces = Vec::new();
    let mut current: Vec<(Point, f64)> = Vec::new();
    let mut was_in = false;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let now_in = inside(polygons, stroke.at(t).0);
        if i > 0 && now_in != was_in {
            let prev = (i - 1) as f64 / n as f64;
            let edge = if now_in {
                crossing(polygons, stroke, t, prev)
            } else {
                crossing(polygons, stroke, prev, t)
            };
            current.push(stroke.at(edge));
            if !now_in {
                pieces.push(std::mem::take(&mut current));
            }
        }
        if now_in {
            current.push(stroke.at(t));
        }
        was_in = now_in;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces.retain(|piece| {
        piece.len() > 1
            && piece
                .first()
                .zip(piece.last())
                .is_some_and(|(a, b)| a.0.distance(b.0) > 1e-3)
    });
    pieces
}

/// Calculates a texture toolpath over the closed vectors in `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for bad settings (see
/// [`TextureParams::validate`]), a tool with no cutting diameter, when there
/// are no closed vectors, and when the region is too small for the tool.
pub fn texture(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &TextureParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    params.validate()?;
    if !(tool.diameter().is_finite() && tool.diameter() > 0.0) {
        return Err(CoreError::validation(
            "the tool's diameter must be more than zero",
        ));
    }
    if params.vectors_as_pattern {
        return along_vectors(paths, job, params, tol);
    }
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation("a texture needs closed vectors"));
    }
    let margin = tool.tool_type.radius_at_height(params.depth) + params.boundary_offset;
    let area = region.offset(margin, tol);
    if area.is_empty() {
        return Err(CoreError::validation(
            "the vectors are too small for the tool to texture",
        ));
    }
    let polygons: Vec<Vec<Point>> = area
        .loops
        .iter()
        .map(|sub| {
            let mut points = flatten_subpath(sub, tol).points;
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            points
        })
        .collect();

    let heights = Heights::from_job(job);
    let step = (params.stroke_length / 24.0).clamp(0.05, 0.5);
    let mut out = Emitter::new(heights);
    let mut count = 0usize;
    for row in lay_strokes(&polygons, params) {
        for stroke in &row {
            for piece in trim(&polygons, stroke, step) {
                let Some(&(first, first_depth)) = piece.first() else {
                    continue;
                };
                out.travel_to(first);
                out.plunge_to(heights.z_at(params.start_depth + first_depth));
                for &(p, depth) in piece.iter().skip(1) {
                    let seg = CutSeg {
                        from: out.at().xy(),
                        to: p,
                        bulge: 0.0,
                    };
                    out.cut(&seg, heights.z_at(params.start_depth + depth));
                }
                count += 1;
            }
        }
    }
    let mut warnings = Vec::new();
    if count == 0 {
        warnings.push(crate::warning::Warning::texture_no_strokes());
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// A texture cut along the selected vectors themselves, nearest first:
/// each at its own random depth, dipping in and out like a flute with
/// `ramp`, otherwise straight down to that depth and along.
fn along_vectors(
    paths: &[Path],
    job: &JobSetup,
    params: &TextureParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let chains: Vec<super::chain::Chain> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .map(|sub| super::chain::Chain::from_subpath(sub, tol))
        .filter(|c| !c.is_empty())
        .collect();
    if chains.is_empty() {
        return Err(CoreError::validation("select the vectors to texture"));
    }
    let heights = Heights::from_job(job);
    let home = heights.home.xy();
    let chains = super::common::order_nearest_either_way(
        chains,
        home,
        |c| c.start().unwrap_or(home),
        |c| c.end().unwrap_or(home),
        |c| if c.closed { c } else { c.reversed() },
    );
    let mut random = Random(params.seed);
    let mut out = Emitter::new(heights);
    for chain in &chains {
        let deepest = params.depth_for(random.unit());
        let Some(begin) = chain.start() else {
            continue;
        };
        out.travel_to(begin);
        let start = params.start_depth;
        if !params.ramp {
            let z = heights.z_at(start + deepest);
            out.plunge_to(z);
            out.cut_flat(&chain.segs, z);
            continue;
        }
        // Dip along the whole vector: surface, deepest halfway, surface.
        let length = chain.length();
        let step = (length / 48.0).clamp(0.05, 0.5);
        let n = ((length / step).ceil() as usize).max(2);
        let cuts: Vec<f64> = (1..n).map(|i| length * i as f64 / n as f64).collect();
        out.plunge_to(heights.z_at(start));
        let mut along = 0.0;
        for piece in chain.split_at(&cuts) {
            for seg in piece {
                along += seg.length();
                let t = (along / length).clamp(0.0, 1.0);
                out.cut(&seg, heights.z_at(start + deepest * (PI * t).sin()));
            }
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::cam::ToolGeometry;
    use crate::geom::rect;

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn vbit() -> Tool {
        Tool {
            name: "V".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
            ..Tool::default()
        }
    }

    fn run(paths: &[Path], params: &TextureParams) -> Vec<Move> {
        let done = texture(paths, &JobSetup::default(), &vbit(), params, tol()).expect("texture");
        assert!(done.warnings.is_empty(), "{:?}", done.warnings);
        done.moves
    }

    fn square() -> Path {
        rect(Point::new(0.0, 0.0), Point::new(40.0, 30.0)).expect("rect")
    }

    #[test]
    fn strokes_stay_inside_and_no_deeper_than_the_depth() {
        let outer = rect(Point::new(0.0, 0.0), Point::new(60.0, 40.0)).expect("rect");
        let hole = rect(Point::new(20.0, 15.0), Point::new(40.0, 25.0)).expect("rect");
        let params = TextureParams {
            angle_deg: 30.0,
            depth: 1.5,
            ..TextureParams::default()
        };
        let moves = run(&[outer, hole], &params);
        // A 90 degree bit 1.5 mm deep cuts 1.5 mm either side.
        let margin = 1.5 - 1e-6;
        let cuts: Vec<&Move> = moves.iter().filter(|m| m.kind.is_cutting()).collect();
        assert!(cuts.len() > 100);
        for m in &cuts {
            let p = m.to;
            assert!(p.z >= -1.5 - 1e-9 && p.z <= 0.0 + 1e-9, "{m:?}");
            assert!(
                p.x >= margin && p.x <= 60.0 - margin && p.y >= margin && p.y <= 40.0 - margin,
                "outside: {m:?}"
            );
            // Distance from the hole, rounded round its corners.
            let dx = (20.0 - p.x).max(p.x - 40.0).max(0.0);
            let dy = (15.0 - p.y).max(p.y - 25.0).max(0.0);
            let in_hole = dx.hypot(dy) < margin - 0.01;
            assert!(!in_hole, "in the hole: {m:?}");
        }
        let deepest = cuts.iter().map(|m| m.to.z).fold(0.0, f64::min);
        assert!(deepest < -0.5, "{deepest}");
    }

    #[test]
    fn strokes_dip_in_an_arc() {
        let params = TextureParams {
            depth_variation: 0.0,
            length_variation: 0.0,
            angle_variation_deg: 0.0,
            ..TextureParams::default()
        };
        let moves = run(&[square()], &params);
        // Somewhere a whole stroke enters at the surface and reaches full depth.
        assert!(moves
            .iter()
            .any(|m| m.kind == MoveKind::Plunge && m.to.z.abs() < 1e-9));
        assert!(moves.iter().any(|m| (m.to.z + 1.0).abs() < 1e-3));
    }

    #[test]
    fn the_start_depth_lowers_every_cut() {
        let params = TextureParams::default();
        let top = run(&[square()], &params);
        let lower = run(
            &[square()],
            &TextureParams {
                start_depth: 2.0,
                ..params
            },
        );
        assert_eq!(top.len(), lower.len());
        for (a, b) in top.iter().zip(&lower) {
            if a.kind != MoveKind::Rapid {
                assert!((a.to.z - 2.0 - b.to.z).abs() < 1e-9, "{a:?} {b:?}");
            }
        }
        let pattern = TextureParams {
            vectors_as_pattern: true,
            start_depth: 1.5,
            ..params
        };
        let cuts = run(&[square()], &pattern);
        let highest = cuts
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid)
            .map(|m| m.to.z)
            .fold(f64::MIN, f64::max);
        assert!((highest + 1.5).abs() < 1e-9, "{highest}");
        let bad = TextureParams {
            start_depth: -1.0,
            ..params
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn stepover_variation_spreads_strokes_across_their_rows() {
        let rows_of = |variation: f64| {
            let params = TextureParams {
                angle_variation_deg: 0.0,
                stepover_variation_percent: Some(variation),
                ..TextureParams::default()
            };
            let mut ys: Vec<i64> = run(&[square()], &params)
                .iter()
                .filter(|m| m.kind == MoveKind::Plunge)
                .map(|m| (m.to.y * 1000.0).round() as i64)
                .collect();
            ys.sort_unstable();
            ys.dedup();
            ys.len()
        };
        // Unvaried strokes sit on the row lines: one height per row.
        let straight = rows_of(0.0);
        assert!(straight <= 10, "{straight}");
        assert!(rows_of(50.0) > straight * 3, "{}", rows_of(50.0));
        let bad = TextureParams {
            stepover_variation_percent: Some(60.0),
            ..TextureParams::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn the_same_seed_gives_the_same_moves() {
        let params = TextureParams::default();
        let a = run(&[square()], &params);
        let b = run(&[square()], &params);
        assert_eq!(a, b);
        let other = TextureParams { seed: 2, ..params };
        let c = run(&[square()], &other);
        assert_ne!(a, c);
    }

    #[test]
    fn spacing_sets_the_number_of_rows() {
        let polygons = vec![vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 20.0),
            Point::new(0.0, 20.0),
        ]];
        for (spacing, rows) in [(2.0, 10), (4.0, 5), (30.0, 1)] {
            let params = TextureParams {
                spacing,
                ..TextureParams::default()
            };
            let laid = lay_strokes(&polygons, &params);
            assert_eq!(laid.len(), rows);
            assert!(laid.iter().all(|row| !row.is_empty()));
        }
        // Rows alternate direction.
        let laid = lay_strokes(&polygons, &TextureParams::default());
        let first = laid[0].first().expect("stroke");
        let second = laid[1].first().expect("stroke");
        assert!(first.to.x > first.from.x);
        assert!(second.to.x < second.from.x);
    }

    #[test]
    fn bad_settings_are_refused() {
        let bad = [
            TextureParams {
                stroke_length: 0.0,
                ..TextureParams::default()
            },
            TextureParams {
                spacing: -1.0,
                ..TextureParams::default()
            },
            TextureParams {
                depth: 0.0,
                ..TextureParams::default()
            },
            TextureParams {
                length_variation: 1.5,
                ..TextureParams::default()
            },
            TextureParams {
                depth_variation: -0.1,
                ..TextureParams::default()
            },
            TextureParams {
                angle_variation_deg: 120.0,
                ..TextureParams::default()
            },
        ];
        for params in &bad {
            let err = texture(&[square()], &JobSetup::default(), &vbit(), params, tol())
                .expect_err("refused");
            assert!(matches!(err, CoreError::Validation(_)));
        }
        let open = crate::geom::polyline_path(&[Point::new(0.0, 0.0), Point::new(9.0, 0.0)], false)
            .expect("line");
        assert!(texture(
            &[open],
            &JobSetup::default(),
            &vbit(),
            &TextureParams::default(),
            tol()
        )
        .is_err());
    }

    #[test]
    fn settings_read_camel_case_with_defaults() {
        let params: TextureParams =
            serde_json::from_str(r#"{"angleDeg":45,"strokeLength":6,"seed":7}"#).expect("json");
        assert_eq!(params.angle_deg, 45.0);
        assert_eq!(params.stroke_length, 6.0);
        assert_eq!(params.seed, 7);
        assert_eq!(params.spacing, TextureParams::default().spacing);
    }

    fn stroke_depths(moves: &[Move]) -> Vec<f64> {
        // The deepest point of each run between plunges.
        let mut out = Vec::new();
        let mut current: Option<f64> = None;
        for m in moves {
            match m.kind {
                MoveKind::Rapid => {
                    if let Some(d) = current.take() {
                        out.push(d);
                    }
                }
                _ if m.kind.is_cutting() => {
                    current = Some(current.map_or(-m.to.z, |d: f64| d.max(-m.to.z)));
                }
                _ => {}
            }
        }
        out.extend(current);
        out
    }

    #[test]
    fn minimum_percentages_bound_the_depths_and_lengths() {
        let square = rect(Point::new(0.0, 0.0), Point::new(80.0, 80.0)).expect("rect");
        let params = TextureParams {
            depth: 2.0,
            min_depth_percent: Some(50.0),
            stroke_length: 10.0,
            min_length_percent: Some(80.0),
            angle_variation_deg: 0.0,
            ..TextureParams::default()
        };
        let done =
            texture(&[square], &JobSetup::default(), &vbit(), &params, tol()).expect("texture");
        let depths = stroke_depths(&done.moves);
        assert!(depths.len() > 20);
        // Trimmed strokes at the edge may be shallower; most are in range.
        let in_range = depths
            .iter()
            .filter(|d| **d >= 1.0 - 1e-6 && **d <= 2.0 + 1e-6)
            .count();
        assert!(
            in_range * 10 >= depths.len() * 8,
            "{in_range} of {}",
            depths.len()
        );
        assert!(depths.iter().all(|d| *d <= 2.0 + 1e-6));
        let bad = TextureParams {
            min_depth_percent: Some(0.0),
            ..params
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn a_full_overlap_packs_more_strokes_than_gaps() {
        let square = rect(Point::new(0.0, 0.0), Point::new(80.0, 80.0)).expect("rect");
        let count = |max_overlap_percent| {
            let params = TextureParams {
                max_overlap_percent,
                overlap_variation_percent: 0.0,
                ..TextureParams::default()
            };
            stroke_depths(
                &texture(
                    std::slice::from_ref(&square),
                    &JobSetup::default(),
                    &vbit(),
                    &params,
                    tol(),
                )
                .expect("texture")
                .moves,
            )
            .len()
        };
        let gaps = count(None);
        let overlapped = count(Some(50.0));
        assert!(overlapped as f64 > gaps as f64 * 1.5, "{gaps} {overlapped}");
    }

    #[test]
    fn a_boundary_offset_keeps_the_strokes_further_in() {
        let square = rect(Point::new(0.0, 0.0), Point::new(80.0, 80.0)).expect("rect");
        let params = TextureParams {
            boundary_offset: 10.0,
            ..TextureParams::default()
        };
        let done =
            texture(&[square], &JobSetup::default(), &vbit(), &params, tol()).expect("texture");
        assert!(done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .all(|m| m.to.x > 10.0 && m.to.x < 70.0 && m.to.y > 10.0 && m.to.y < 70.0));
    }

    #[test]
    fn selected_vectors_can_be_the_pattern_with_or_without_ramps() {
        let a = crate::geom::line(Point::new(0.0, 0.0), Point::new(30.0, 0.0));
        let b = crate::geom::line(Point::new(0.0, 10.0), Point::new(30.0, 10.0));
        let run = |ramp| {
            let params = TextureParams {
                vectors_as_pattern: true,
                ramp,
                depth: 2.0,
                min_depth_percent: Some(100.0),
                ..TextureParams::default()
            };
            texture(
                &[a.clone(), b.clone()],
                &JobSetup::default(),
                &vbit(),
                &params,
                tol(),
            )
            .expect("texture")
            .moves
        };
        let ramped = run(true);
        assert_eq!(stroke_depths(&ramped).len(), 2);
        assert!(stroke_depths(&ramped)
            .iter()
            .all(|d| (d - 2.0).abs() < 0.01));
        // The ramped cut is deepest mid way along, at the surface at its ends.
        let deepest = ramped
            .iter()
            .filter(|m| m.kind.is_cutting())
            .min_by(|p, q| p.to.z.total_cmp(&q.to.z))
            .expect("a cut");
        assert!((deepest.to.x - 15.0).abs() < 1.0, "{:?}", deepest.to);
        let flat = run(false);
        let cuts: Vec<f64> = flat
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| m.to.z)
            .collect();
        assert!(cuts.iter().all(|z| (z + 2.0).abs() < 1e-6), "{cuts:?}");
    }
}
