//! Rounding: turning a rotary blank, round or square, down to a finished
//! diameter along the whole length of the job.
//!
//! The toolpath is worked out flat, as every rotary toolpath is: along the
//! axis is job X (or Y for a blank along Y), round the blank is the other
//! flat axis, one full turn being the job's flat height, and Z is the
//! distance from the axis measured from the job's Z zero. A rotary post
//! wraps it when it is saved.
//!
//! Three methods: radial turns the blank a full revolution, steps along the
//! axis and turns back; raster runs along the axis, turns the blank one
//! stepover and runs back; optimised raster does the same but only where a
//! square blank still has material at that pass, skipping the air between
//! its corners. Each method cuts in levels a pass depth apart, from the
//! outside of the blank down to the finished radius plus the allowance.

use serde::{Deserialize, Serialize};

use crate::doc::{JobSetup, RotaryAxis};
use crate::error::CoreError;
use crate::geom::Point;

use super::chain::CutSeg;
use super::common::{Emitter, Heights};
use super::profile::Calculated;
use super::tool::Tool;

/// Cross section of the blank being rounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlankShape {
    /// Round stock.
    #[default]
    Round,
    /// Square stock.
    Square,
}

/// How the blank is machined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoundingMethod {
    /// A full turn of the blank, a step along the axis, a turn back.
    #[default]
    Radial,
    /// Along the axis, a stepover round the blank, back along the axis.
    Raster,
    /// Raster, only where a square blank has material left.
    OptimizedRaster,
}

/// The Create Rounding Toolpath settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoundingParams {
    /// Round or square stock.
    pub blank: BlankShape,
    /// Diameter of round stock or side of square stock, millimetres; zero
    /// takes the job's rotary blank.
    pub blank_size: f64,
    /// Diameter to turn the blank down to, millimetres.
    pub finish_diameter: f64,
    /// Material left on the finished diameter, millimetres.
    pub allowance: f64,
    /// Radial, raster or optimised raster.
    pub method: RoundingMethod,
}

impl Default for RoundingParams {
    fn default() -> Self {
        RoundingParams {
            blank: BlankShape::Round,
            blank_size: 0.0,
            finish_diameter: 40.0,
            allowance: 0.0,
            method: RoundingMethod::Radial,
        }
    }
}

/// Most levels or lines a rounding may make, a guard against a tiny pass
/// depth or stepover.
const MAX_STEPS: usize = 100_000;

/// The rounding toolpath for a rotary job.
///
/// # Errors
/// [`CoreError::Validation`] when the job is not rotary, the sizes make no
/// sense (the finish bigger than the blank), optimised raster is asked of a
/// round blank, or the tool has no stepover or pass depth.
pub fn rounding(
    job: &JobSetup,
    tool: &Tool,
    params: &RoundingParams,
) -> Result<Calculated, CoreError> {
    if !job.is_rotary() {
        return Err(CoreError::validation(
            "rounding is for rotary jobs: set the job up as rotary first",
        ));
    }
    let rotary = job.rotary;
    let size = if params.blank_size > 0.0 {
        params.blank_size
    } else {
        rotary.diameter
    };
    let finish = params.finish_diameter / 2.0 + params.allowance;
    if !size.is_finite() || !finish.is_finite() || params.finish_diameter <= 0.0 {
        return Err(CoreError::validation(
            "the finished diameter must be more than zero",
        ));
    }
    if params.allowance < 0.0 {
        return Err(CoreError::validation("the allowance must be zero or more"));
    }
    if params.method == RoundingMethod::OptimizedRaster && params.blank != BlankShape::Square {
        return Err(CoreError::validation(
            "optimized raster only skips the air round a square blank: pick raster or radial for round stock",
        ));
    }
    let outer = match params.blank {
        BlankShape::Round => size / 2.0,
        BlankShape::Square => size / 2.0 * std::f64::consts::SQRT_2,
    };
    if finish >= outer - 1e-9 {
        return Err(CoreError::validation(
            "the finished diameter plus the allowance leaves nothing to cut from the blank",
        ));
    }
    let stepover = tool.stepover;
    let pass = tool.pass_depth;
    if !stepover.is_finite() || stepover <= 0.0 || !pass.is_finite() || pass <= 0.0 {
        return Err(CoreError::validation(
            "the tool needs a stepover and a pass depth",
        ));
    }

    // Flat layout: along the axis and round the blank.
    let (dx, dy) = job.datum();
    let (along_lo, along_hi, round_lo, round_hi) = match rotary.axis {
        RotaryAxis::X => (-dx, job.width - dx, -dy, job.height - dy),
        RotaryAxis::Y => (-dy, job.height - dy, -dx, job.width - dx),
    };
    let wrap_radius = rotary.diameter / 2.0;
    let job_heights = Heights::from_job(job);
    // Job Z of a radius: the job's surface is the wrap radius.
    let z_of = |r: f64| job_heights.surface - (wrap_radius - r);
    // Rapids clear the corners of a square blank too.
    let lift = (z_of(outer) - job_heights.surface).max(0.0);
    let heights = Heights {
        surface: job_heights.surface + lift,
        safe: job_heights.safe + lift,
        plunge_from: job_heights.plunge_from + lift,
        home: job_heights.home,
    };
    let point = |along: f64, round: f64| match rotary.axis {
        RotaryAxis::X => Point::new(along, round),
        RotaryAxis::Y => Point::new(round, along),
    };
    // Material radius of the blank a flat distance round from the start.
    let material = |round: f64| -> f64 {
        match params.blank {
            BlankShape::Round => outer,
            BlankShape::Square => {
                let angle = (round - round_lo) / wrap_radius;
                (size / 2.0) / angle.cos().abs().max(angle.sin().abs())
            }
        }
    };

    let mut levels = Vec::new();
    let mut r = outer;
    while r > finish + 1e-9 {
        r = (r - pass).max(finish);
        levels.push(r);
        if levels.len() > MAX_STEPS {
            return Err(CoreError::validation(
                "the pass depth makes too many levels",
            ));
        }
    }

    let mut out = Emitter::new(heights);
    let line = |out: &mut Emitter, a: Point, b: Point, z: f64| {
        out.cut_flat(
            &[CutSeg {
                from: a,
                to: b,
                bulge: 0.0,
            }],
            z,
        );
    };
    for &level in &levels {
        let z = z_of(level);
        match params.method {
            RoundingMethod::Radial => {
                let steps = count(along_hi - along_lo, stepover)?;
                let mut forward = true;
                for k in 0..=steps {
                    let along = along_lo + (along_hi - along_lo) * k as f64 / steps.max(1) as f64;
                    let (from, to) = if forward {
                        (round_lo, round_hi)
                    } else {
                        (round_hi, round_lo)
                    };
                    let start = point(along, from);
                    if k == 0 {
                        out.travel_to(start);
                        out.plunge_to(z);
                    } else {
                        let here = out.at().xy();
                        line(&mut out, here, start, z);
                    }
                    line(&mut out, start, point(along, to), z);
                    forward = !forward;
                }
            }
            RoundingMethod::Raster | RoundingMethod::OptimizedRaster => {
                // Stepover measured round the surface being cut.
                let step = stepover * wrap_radius / level;
                let steps = count(round_hi - round_lo, step)?;
                let mut forward = true;
                let mut down = false;
                for k in 0..steps {
                    let round = round_lo + (round_hi - round_lo) * k as f64 / steps as f64;
                    if params.method == RoundingMethod::OptimizedRaster
                        && material(round) <= level + 1e-6
                    {
                        if down {
                            out.retract();
                            down = false;
                        }
                        continue;
                    }
                    let (from, to) = if forward {
                        (along_lo, along_hi)
                    } else {
                        (along_hi, along_lo)
                    };
                    let start = point(from, round);
                    if down {
                        let here = out.at().xy();
                        line(&mut out, here, start, z);
                    } else {
                        out.travel_to(start);
                        out.plunge_to(z);
                        down = true;
                    }
                    line(&mut out, start, point(to, round), z);
                    forward = !forward;
                }
                out.retract();
            }
        }
        out.retract();
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

/// How many steps of at most `step` cover `length`.
fn count(length: f64, step: f64) -> Result<usize, CoreError> {
    let steps = (length / step).ceil().max(1.0);
    if steps > MAX_STEPS as f64 {
        return Err(CoreError::validation("the stepover makes too many passes"));
    }
    Ok(steps as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::doc::{JobKind, RotarySetup};

    fn rotary_job(blank: crate::doc::RotaryBlank) -> JobSetup {
        let setup = JobSetup {
            kind: JobKind::Rotary,
            rotary: RotarySetup {
                diameter: 50.0,
                length: 100.0,
                blank,
                ..RotarySetup::default()
            },
            ..JobSetup::default()
        };
        setup.normalised()
    }

    fn tool() -> Tool {
        Tool {
            name: "End Mill 6 mm".to_string(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            pass_depth: 3.0,
            stepover: 2.5,
            ..Tool::default()
        }
    }

    fn cut_radii(job: &JobSetup, moves: &[crate::cam::toolpath::Move]) -> Vec<f64> {
        let surface = job.surface_z();
        moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| 25.0 - (surface - m.to.z))
            .collect()
    }

    #[test]
    fn a_round_blank_is_turned_down_in_levels_to_the_finished_radius() {
        let job = rotary_job(crate::doc::RotaryBlank::Cylinder);
        let params = RoundingParams {
            finish_diameter: 40.0,
            allowance: 0.5,
            ..RoundingParams::default()
        };
        let done = rounding(&job, &tool(), &params).expect("rounds");
        let radii = cut_radii(&job, &done.moves);
        let lowest = radii.iter().copied().fold(f64::MAX, f64::min);
        assert!((lowest - 20.5).abs() < 1e-6, "{lowest}");
        // 25 down to 20.5 in 3 mm passes: 22 then 20.5.
        let mut levels: Vec<i64> = radii.iter().map(|r| (r * 1000.0).round() as i64).collect();
        levels.sort_unstable();
        levels.dedup();
        assert_eq!(levels, vec![20_500, 22_000]);
        // Radial: each revolution spans the whole flat height.
        let ys: Vec<f64> = done.moves.iter().map(|m| m.to.y).collect();
        let span = ys.iter().copied().fold(f64::MIN, f64::max)
            - ys.iter().copied().fold(f64::MAX, f64::min);
        assert!(span >= job.height - 1e-6, "{span}");
    }

    #[test]
    fn optimised_raster_skips_the_air_round_a_square_blank() {
        let job = rotary_job(crate::doc::RotaryBlank::Square);
        let raster = RoundingParams {
            blank: BlankShape::Square,
            finish_diameter: 48.0,
            method: RoundingMethod::Raster,
            ..RoundingParams::default()
        };
        let optimised = RoundingParams {
            method: RoundingMethod::OptimizedRaster,
            ..raster
        };
        let all = rounding(&job, &tool(), &raster).expect("raster");
        let some = rounding(&job, &tool(), &optimised).expect("optimised");
        let cutting = |c: &Calculated| c.moves.iter().filter(|m| m.kind.is_cutting()).count();
        assert!(
            cutting(&some) < cutting(&all) / 2,
            "{} vs {}",
            cutting(&some),
            cutting(&all)
        );
        // Rapids clear the square's corners, 25 sqrt 2 from the axis.
        let surface = job.surface_z();
        let safe = all
            .moves
            .iter()
            .filter(|m| m.kind == crate::cam::toolpath::MoveKind::Rapid)
            .map(|m| m.to.z)
            .fold(f64::MIN, f64::max);
        assert!(safe > surface + (25.0 * std::f64::consts::SQRT_2 - 25.0));
        assert!(rounding(
            &job,
            &tool(),
            &RoundingParams {
                blank: BlankShape::Round,
                ..optimised
            }
        )
        .is_err());
    }

    #[test]
    fn rounding_needs_a_rotary_job_and_something_to_cut() {
        let flat = JobSetup::default();
        assert!(rounding(&flat, &tool(), &RoundingParams::default()).is_err());
        let job = rotary_job(crate::doc::RotaryBlank::Cylinder);
        let too_big = RoundingParams {
            finish_diameter: 60.0,
            ..RoundingParams::default()
        };
        assert!(rounding(&job, &tool(), &too_big).is_err());
    }
}
