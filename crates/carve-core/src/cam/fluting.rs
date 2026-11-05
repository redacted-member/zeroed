//! Fluting toolpaths: the tool runs along each vector and sinks into the
//! material gradually, so the flute tapers in at its start and out at its
//! end.
//!
//! The depth along a vector of length `L` at distance `s` from its start is
//! `depth * min(rise(s / r), rise((L - s) / r))`, where `r` is the ramp length
//! and `rise` is 0 at 0 and 1 from 1 on: a straight line for a linear ramp or
//! the cosine ease `(1 - cos(pi t)) / 2` for a smooth one. An end that is not
//! ramped is cut at full depth right to the end.
//!
//! Deep flutes are cut in passes, pass `k` of `n` following the same shape
//! scaled to `k / n` of the final depth. Passes zigzag: each one runs the
//! vector the opposite way to the one before, starting where the last
//! finished, so the tool does not rapid back to the start between passes.
//! Works with any tool that cuts on its tip (ball nose, end mill, V-bit).

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{Path, Tolerance};

use super::chain::{Chain, CutSeg};
use super::common::{order_nearest_either_way, Emitter, Heights};
use super::profile::Calculated;
use super::tool::Tool;

/// How a flute's depth rises along a ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RampKind {
    /// Depth rises at a steady slope.
    Linear,
    /// Depth eases in and out along a half cosine.
    #[default]
    Smooth,
}

/// The Fluting form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlutingParams {
    /// Depth the flute starts from, in millimetres below the surface: zero
    /// on the surface, the pocket depth inside a pocket.
    pub start_depth: f64,
    /// Full depth of the flute below the start depth, in millimetres.
    pub depth: f64,
    /// Shape of the ramps.
    pub ramp: RampKind,
    /// Taper in at the start of each vector.
    pub ramp_start: bool,
    /// Taper out at the end of each vector.
    pub ramp_end: bool,
    /// Length of each ramp along the vector, in millimetres. Absent means
    /// the ramps take the whole vector: half of it each when both ends ramp.
    pub ramp_length: Option<f64>,
    /// Ramp length as a percentage of the longest ramp each vector allows,
    /// used instead of `ramp_length` when set.
    pub ramp_percent: Option<f64>,
    /// Cut the vectors in the order they were selected instead of the
    /// nearest next (Use Vector Selection Order).
    pub selection_order: bool,
}

impl Default for FlutingParams {
    fn default() -> Self {
        FlutingParams {
            start_depth: 0.0,
            depth: 3.0,
            ramp: RampKind::Smooth,
            ramp_start: true,
            ramp_end: true,
            ramp_length: None,
            ramp_percent: None,
            selection_order: false,
        }
    }
}

/// Fraction of the full depth a ramp reaches at `t` of its length.
fn rise(kind: RampKind, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        RampKind::Linear => t,
        RampKind::Smooth => (1.0 - (PI * t).cos()) / 2.0,
    }
}

/// The depth shape of one vector: fraction of the full depth at a distance
/// along it, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shape {
    kind: RampKind,
    length: f64,
    ramp: f64,
    start: bool,
    end: bool,
}

impl Shape {
    fn at(&self, s: f64) -> f64 {
        let mut f: f64 = 1.0;
        if self.ramp > 1e-12 {
            if self.start {
                f = f.min(rise(self.kind, s / self.ramp));
            }
            if self.end {
                f = f.min(rise(self.kind, (self.length - s) / self.ramp));
            }
        }
        f
    }
}

/// A span of a flute with the depth fractions at its two ends.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FluteSpan {
    seg: CutSeg,
    from: f64,
    to: f64,
}

/// A vector split into spans short enough for the depth between their ends
/// to be followed within tolerance.
#[derive(Debug, Clone, PartialEq)]
struct Flute {
    spans: Vec<FluteSpan>,
}

impl Flute {
    fn start(&self) -> Option<crate::geom::Point> {
        self.spans.first().map(|s| s.seg.from)
    }

    fn end(&self) -> Option<crate::geom::Point> {
        self.spans.last().map(|s| s.seg.to)
    }

    fn reversed(&self) -> Flute {
        Flute {
            spans: self
                .spans
                .iter()
                .rev()
                .map(|s| FluteSpan {
                    seg: s.seg.reversed(),
                    from: s.to,
                    to: s.from,
                })
                .collect(),
        }
    }
}

/// Distances along a vector to split it at: the ramp ends, and within smooth
/// ramps every `step` millimetres.
fn breakpoints(shape: &Shape, step: f64) -> Vec<f64> {
    let mut cuts = Vec::new();
    let mut ramp_cuts = |from: f64, to: f64| {
        if shape.kind == RampKind::Smooth {
            let count = ((to - from) / step).ceil().max(1.0) as usize;
            for i in 1..count {
                cuts.push(from + (to - from) * i as f64 / count as f64);
            }
        }
        cuts.push(from);
        cuts.push(to);
    };
    if shape.ramp > 1e-12 {
        if shape.start {
            ramp_cuts(0.0, shape.ramp);
        }
        if shape.end {
            ramp_cuts(shape.length - shape.ramp, shape.length);
        }
    }
    cuts.retain(|&d| d > 1e-9 && d < shape.length - 1e-9);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    cuts
}

fn flute(chain: &Chain, shape: &Shape, step: f64) -> Flute {
    let cuts = breakpoints(shape, step);
    let mut spans = Vec::new();
    let mut along = 0.0;
    for piece in chain.split_at(&cuts) {
        for seg in piece {
            let from = shape.at(along);
            along += seg.length();
            spans.push(FluteSpan {
                seg,
                from,
                to: shape.at(along),
            });
        }
    }
    Flute { spans }
}

/// Calculates a fluting toolpath along `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a depth, ramp length or tool pass
/// depth that is not more than zero, or when there is nothing to cut.
pub fn fluting(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &FlutingParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    if !(params.depth.is_finite() && params.depth > 0.0) {
        return Err(CoreError::validation(
            "the flute depth must be more than zero",
        ));
    }
    if let Some(r) = params.ramp_length {
        if !(r.is_finite() && r > 0.0) {
            return Err(CoreError::validation(
                "the ramp length must be more than zero",
            ));
        }
    }
    if let Some(p) = params.ramp_percent {
        if !(p.is_finite() && p > 0.0 && p <= 100.0) {
            return Err(CoreError::validation(
                "the ramp percentage must be more than 0 and at most 100",
            ));
        }
    }
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !(tool.pass_depth.is_finite() && tool.pass_depth > 0.0) {
        return Err(CoreError::validation(
            "the tool's pass depth must be more than zero",
        ));
    }
    let chains: Vec<Chain> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .map(|sub| Chain::from_subpath(sub, tol))
        .filter(|c| !c.is_empty())
        .collect();
    if chains.is_empty() {
        return Err(CoreError::validation("select the vectors to flute"));
    }

    let ends = usize::from(params.ramp_start) + usize::from(params.ramp_end);
    // Linear interpolation of the cosine keeps within `tol` of it when the
    // step is no more than sqrt(8 tol / max curvature).
    let curvature_step = |ramp: f64| {
        let bend = params.depth * PI * PI / (2.0 * ramp * ramp);
        (8.0 * tol.get() / bend).sqrt().clamp(1e-3, ramp.max(1e-3))
    };
    let mut clamped = false;
    let flutes: Vec<Flute> = chains
        .iter()
        .map(|chain| {
            let length = chain.length();
            let most = if ends == 2 { length / 2.0 } else { length };
            let ramp = match (params.ramp_percent, params.ramp_length) {
                (Some(p), _) => most * p / 100.0,
                (None, Some(r)) if r > most + 1e-9 && ends > 0 => {
                    clamped = true;
                    most
                }
                (None, Some(r)) => r,
                (None, None) => most,
            };
            let shape = Shape {
                kind: params.ramp,
                length,
                ramp: if ends == 0 { 0.0 } else { ramp },
                start: params.ramp_start,
                end: params.ramp_end,
            };
            flute(chain, &shape, curvature_step(ramp.max(1e-9)))
        })
        .collect();
    let mut warnings = Vec::new();
    if clamped {
        warnings.push(crate::warning::Warning::fluting_ramp_shortened());
    }

    let heights = Heights::from_job(job);
    let home = heights.home.xy();
    let flutes = if params.selection_order {
        flutes
    } else {
        order_nearest_either_way(
            flutes,
            home,
            |f| f.start().unwrap_or(home),
            |f| f.end().unwrap_or(home),
            |f| f.reversed(),
        )
    };
    let passes = ((params.depth / tool.pass_depth) - 1e-9).ceil().max(1.0) as usize;
    let mut out = Emitter::new(heights);
    for forward in &flutes {
        let backward = forward.reversed();
        let Some(begin) = forward.start() else {
            continue;
        };
        out.travel_to(begin);
        for pass in 1..=passes {
            let full = params.depth * pass as f64 / passes as f64;
            let run = if pass % 2 == 1 { forward } else { &backward };
            let start = params.start_depth;
            if let Some(first) = run.spans.first() {
                out.plunge_to(heights.z_at(start + full * first.from));
            }
            for span in &run.spans {
                out.cut(&span.seg, heights.z_at(start + full * span.to));
            }
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::cam::ToolGeometry;
    use crate::geom::{line, Point};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn bar() -> Path {
        line(Point::new(0.0, 0.0), Point::new(100.0, 0.0))
    }

    fn ball(pass_depth: f64) -> Tool {
        Tool {
            name: "Ball".to_string(),
            tool_type: ToolGeometry::BallNose { diameter: 6.0 },
            pass_depth,
            ..Tool::default()
        }
    }

    fn cutting(moves: &[Move]) -> Vec<&Move> {
        moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.kind != MoveKind::Plunge)
            .collect()
    }

    /// Depth reached at `x` along the bar in the last pass.
    fn depth_near(moves: &[Move], x: f64) -> f64 {
        cutting(moves)
            .iter()
            .filter(|m| (m.to.x - x).abs() < 1e-6)
            .map(|m| -m.to.z)
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_flute_is_zero_at_its_ramped_ends_and_full_in_the_middle() {
        let params = FlutingParams {
            depth: 4.0,
            ramp: RampKind::Linear,
            ..FlutingParams::default()
        };
        let done =
            fluting(&[bar()], &JobSetup::default(), &ball(10.0), &params, tol()).expect("fluting");
        assert!(done.warnings.is_empty());
        let cuts = cutting(&done.moves);
        let first = done
            .moves
            .iter()
            .find(|m| m.kind.is_cutting())
            .expect("a cut");
        assert!(first.to.z.abs() < 1e-9);
        assert!(cuts.last().expect("cut").to.z.abs() < 1e-9);
        assert!((depth_near(&done.moves, 50.0) - 4.0).abs() < 1e-9);
        let deepest = done.moves.iter().map(|m| m.to.z).fold(0.0, f64::min);
        assert!((deepest + 4.0).abs() < 1e-9);
    }

    #[test]
    fn the_shape_is_symmetric() {
        for kind in [RampKind::Linear, RampKind::Smooth] {
            let shape = Shape {
                kind,
                length: 80.0,
                ramp: 40.0,
                start: true,
                end: true,
            };
            for s in [0.0, 5.0, 13.0, 27.5, 40.0] {
                assert!((shape.at(s) - shape.at(80.0 - s)).abs() < 1e-12);
            }
            assert_eq!(shape.at(0.0), 0.0);
            assert_eq!(shape.at(40.0), 1.0);
        }
    }

    #[test]
    fn smooth_and_linear_ramps_meet_at_the_midpoint_but_differ_either_side() {
        let linear = Shape {
            kind: RampKind::Linear,
            length: 100.0,
            ramp: 20.0,
            start: true,
            end: false,
        };
        let smooth = Shape {
            kind: RampKind::Smooth,
            ..linear
        };
        assert!((linear.at(10.0) - 0.5).abs() < 1e-12);
        assert!((smooth.at(10.0) - 0.5).abs() < 1e-12);
        assert!((linear.at(5.0) - 0.25).abs() < 1e-12);
        assert!((smooth.at(5.0) - (1.0 - (PI / 4.0).cos()) / 2.0).abs() < 1e-12);
        assert!(smooth.at(5.0) < linear.at(5.0));
        assert_eq!(smooth.at(60.0), 1.0);
        assert_eq!(smooth.at(100.0), 1.0);
    }

    #[test]
    fn a_smooth_flute_follows_the_cosine() {
        let params = FlutingParams {
            depth: 2.0,
            ramp_length: Some(20.0),
            ramp_end: false,
            ..FlutingParams::default()
        };
        let done =
            fluting(&[bar()], &JobSetup::default(), &ball(10.0), &params, tol()).expect("fluting");
        let ramp: Vec<&Move> = cutting(&done.moves)
            .into_iter()
            .filter(|m| m.to.x > 0.0 && m.to.x < 20.0)
            .collect();
        assert!(ramp.len() > 3, "ramp spans {}", ramp.len());
        for m in ramp {
            let expected = 2.0 * rise(RampKind::Smooth, m.to.x / 20.0);
            assert!((-m.to.z - expected).abs() < 1e-9);
        }
        assert!((depth_near(&done.moves, 100.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn deep_flutes_zigzag_in_scaled_passes() {
        let params = FlutingParams {
            depth: 6.0,
            ramp: RampKind::Linear,
            ramp_end: false,
            ..FlutingParams::default()
        };
        let done =
            fluting(&[bar()], &JobSetup::default(), &ball(2.5), &params, tol()).expect("fluting");
        // 6 mm at 2.5 a pass is three passes of 2, 4 and 6 at the end.
        let at_end: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .filter(|m| (m.to.x - 100.0).abs() < 1e-9)
            .map(|m| -m.to.z)
            .collect();
        for d in [2.0, 4.0, 6.0] {
            assert!(at_end.iter().any(|z| (z - d).abs() < 1e-9), "{at_end:?}");
        }
        // No rapids between the passes of one vector.
        let first_cut = done
            .moves
            .iter()
            .position(|m| m.kind.is_cutting())
            .expect("cut");
        let last_cut = done
            .moves
            .iter()
            .rposition(|m| m.kind.is_cutting())
            .expect("cut");
        assert!(done.moves[first_cut..=last_cut]
            .iter()
            .all(|m| m.kind != MoveKind::Rapid));
        // The second pass runs back to the start.
        let back = cutting(&done.moves)
            .iter()
            .filter(|m| m.to.x.abs() < 1e-9)
            .count();
        assert!(back >= 1);
    }

    #[test]
    fn a_ramp_longer_than_half_the_vector_is_shortened_with_a_warning() {
        let params = FlutingParams {
            depth: 3.0,
            ramp: RampKind::Linear,
            ramp_length: Some(80.0),
            ..FlutingParams::default()
        };
        let done =
            fluting(&[bar()], &JobSetup::default(), &ball(10.0), &params, tol()).expect("fluting");
        assert_eq!(done.warnings.len(), 1);
        assert!((depth_near(&done.moves, 50.0) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn works_with_an_end_mill_and_a_vbit() {
        let params = FlutingParams::default();
        for geometry in [
            ToolGeometry::EndMill { diameter: 6.0 },
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
        ] {
            let tool = Tool {
                tool_type: geometry,
                pass_depth: 5.0,
                ..Tool::default()
            };
            assert!(fluting(&[bar()], &JobSetup::default(), &tool, &params, tol()).is_ok());
        }
    }

    #[test]
    fn bad_settings_are_refused() {
        let job = JobSetup::default();
        let zero = FlutingParams {
            depth: 0.0,
            ..FlutingParams::default()
        };
        assert!(fluting(&[bar()], &job, &ball(3.0), &zero, tol()).is_err());
        let no_ramp = FlutingParams {
            ramp_length: Some(0.0),
            ..FlutingParams::default()
        };
        assert!(fluting(&[bar()], &job, &ball(3.0), &no_ramp, tol()).is_err());
        assert!(fluting(&[bar()], &job, &ball(0.0), &FlutingParams::default(), tol()).is_err());
        assert!(fluting(&[], &job, &ball(3.0), &FlutingParams::default(), tol()).is_err());
    }

    #[test]
    fn settings_read_camel_case_with_defaults() {
        let params: FlutingParams =
            serde_json::from_str(r#"{"depth":2,"ramp":"linear","rampEnd":false,"rampLength":10}"#)
                .expect("json");
        assert_eq!(params.ramp, RampKind::Linear);
        assert!(params.ramp_start);
        assert!(!params.ramp_end);
        assert_eq!(params.ramp_length, Some(10.0));
    }

    #[test]
    fn a_start_depth_lowers_the_whole_flute_and_a_percentage_sets_the_ramp() {
        let params = FlutingParams {
            start_depth: 2.0,
            depth: 3.0,
            ramp: RampKind::Linear,
            ramp_start: true,
            ramp_end: true,
            ramp_percent: Some(50.0),
            ..FlutingParams::default()
        };
        let done =
            fluting(&[bar()], &JobSetup::default(), &ball(10.0), &params, tol()).expect("flutes");
        let cuts: Vec<&Move> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        let lowest = cuts.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        let highest = cuts.iter().map(|m| m.to.z).fold(f64::MIN, f64::max);
        assert!((lowest + 5.0).abs() < 1e-6, "{lowest}");
        assert!(
            (highest + 2.0).abs() < 1e-6,
            "the ends sit at the start depth"
        );
        // Half of the longest ramp (50 mm each way) is 25 mm: full depth
        // from x = 25 to x = 75.
        let full: Vec<f64> = cuts
            .iter()
            .filter(|m| (m.to.z + 5.0).abs() < 1e-6)
            .map(|m| m.to.x)
            .collect();
        assert!(full.iter().any(|x| (x - 25.0).abs() < 1e-6), "{full:?}");
        assert!(full.iter().any(|x| (x - 75.0).abs() < 1e-6), "{full:?}");
        let bad = FlutingParams {
            ramp_percent: Some(150.0),
            ..params
        };
        assert!(fluting(&[bar()], &JobSetup::default(), &ball(10.0), &bad, tol()).is_err());
    }

    /// Which of the shapes (numbered by `bucket`) each plunge goes into, in
    /// cutting order, one entry per run of plunges into the same shape.
    fn plunge_order(
        moves: &[crate::cam::toolpath::Move],
        bucket: impl Fn(f64, f64) -> usize,
    ) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::new();
        for m in moves
            .iter()
            .filter(|m| m.kind == crate::cam::toolpath::MoveKind::Plunge)
        {
            let b = bucket(m.to.x, m.to.y);
            if order.last() != Some(&b) {
                order.push(b);
            }
        }
        order
    }

    #[test]
    fn use_vector_selection_order_flutes_in_the_order_selected() {
        let bars: Vec<Path> = [30.0, 0.0, 15.0]
            .iter()
            .map(|y| line(Point::new(0.0, *y), Point::new(100.0, *y)))
            .collect();
        let by_y = |_x: f64, y: f64| ((y + 5.0) / 15.0) as usize;
        let params = FlutingParams {
            depth: 2.0,
            ..FlutingParams::default()
        };
        let nearest =
            fluting(&bars, &JobSetup::default(), &ball(10.0), &params, tol()).expect("fluting");
        assert_eq!(plunge_order(&nearest.moves, by_y), vec![0, 1, 2]);
        let params = FlutingParams {
            selection_order: true,
            ..params
        };
        let picked =
            fluting(&bars, &JobSetup::default(), &ball(10.0), &params, tol()).expect("fluting");
        assert_eq!(plunge_order(&picked.moves, by_y), vec![2, 0, 1]);
    }
}
