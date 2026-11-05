//! Chamfer toolpaths: a V shaped cutter runs along the edges of vectors and
//! its cone face cuts a bevel into the material edge.
//!
//! Geometry, for a cone of half angle `a` with a flat tip of radius `f` (zero
//! for a pointed V-bit). The vector is the line where the bevel meets the
//! wall below it; the bevel's top edge is `width` millimetres from the vector
//! on the material side, and the bevel reaches `width / tan(a)` below the top
//! (so a 90 degree bit cuts a 45 degree bevel as deep as it is wide). The tool
//! axis runs `f` to the waste side of the vector with the tip at that depth,
//! which puts the edge of the flat on the vector and the cone face on the
//! bevel. A pointed V-bit therefore runs with its tip right on the vector.
//!
//! When the bevel is wider than the cone face reaches (`radius - f`), or
//! deeper than the tool's pass depth, it is cut in passes whose cone faces all
//! lie on the finished bevel: the first pass sits furthest into the material
//! and shallowest, and each later pass steps the tip out towards the waste
//! and down, the last one landing on the final position. The other face of
//! the cone stays above the bevel, so no pass cuts into the finished part.
//!
//! Outside and Inside apply to closed vectors, as in a profile: Outside
//! bevels the edge of what is inside the vectors, Inside the edge of a hole.
//! On (and every open vector) bevels both sides at once, a V groove whose
//! sides are each `width` wide measured from the flat's edges. Loops are cut
//! climb or conventional.
//!
//! An overcut moves the whole cut that far towards the waste and down the
//! same slope, so the cone's edge rather than its tip finishes the bottom
//! of the bevel and leaves no witness line. A downward slope puts the
//! vector on the bevel's top edge instead of its bottom edge.
//!
//! A ball nose cuts the bevel at an angle the form gives, in passes a
//! stepover apart down the slope, the ball touching the bevel face.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{parallel_offset_open, path_bbox, BBox, Path, Point, Region, SubPath, Tolerance};

use super::chain::Chain;
use super::common::{order_nearest_either_way, Emitter, Heights};
use super::profile::{Calculated, CutDirection, ProfileSide};
use super::tool::{Tool, ToolGeometry};

/// The Chamfer form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChamferParams {
    /// Horizontal width of the bevel, from the vector to its top edge, in
    /// millimetres. Used when `depth` is absent.
    pub width: f64,
    /// Depth of the bevel below its top edge, in millimetres. When given it
    /// drives the chamfer and the width follows from the tool angle.
    pub depth: Option<f64>,
    /// Which side of closed vectors the material is on. Open vectors are
    /// always cut on the line.
    pub side: ProfileSide,
    /// Depth below the surface the bevel's top edge sits at, in millimetres.
    pub start_depth: f64,
    /// Climb or conventional, for closed vectors.
    pub direction: CutDirection,
    /// Distance the cut is moved out towards the waste and down the slope,
    /// in millimetres, so the bevel's foot is cut by the cone's edge.
    pub overcut: f64,
    /// Whether the bevel rises from the vector into the material (the
    /// vector is its foot) or falls from it (the vector is its top edge).
    pub slope: ChamferSlope,
    /// Angle of the bevel from the vertical, in degrees, for a ball nose;
    /// a V shaped tool always cuts at its own half angle.
    pub angle: f64,
    /// Cut the vectors in the order they were selected instead of the
    /// nearest next (Use Vector Selection Order).
    pub selection_order: bool,
}

/// Which edge of the bevel the vector marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChamferSlope {
    /// The vector is the bevel's foot; it rises into the material.
    #[default]
    Up,
    /// The vector is the bevel's top edge; it falls towards the waste.
    Down,
}

impl Default for ChamferParams {
    fn default() -> Self {
        ChamferParams {
            width: 3.0,
            depth: None,
            side: ProfileSide::Outside,
            start_depth: 0.0,
            direction: CutDirection::Climb,
            overcut: 0.0,
            slope: ChamferSlope::Up,
            angle: 45.0,
            selection_order: false,
        }
    }
}

/// The shape of the cutter's end as a chamfer sees it.
#[derive(Debug, Clone, Copy)]
enum Cutter {
    /// A cone: half angle in radians, flat tip radius, cone radius.
    Cone { half: f64, flat: f64, radius: f64 },
    /// A ball of this radius.
    Ball { radius: f64 },
}

fn cutter(tool: &Tool) -> Result<Cutter, CoreError> {
    match tool.tool_type {
        ToolGeometry::BallNose { diameter } => {
            if !(diameter.is_finite() && diameter > 0.0) {
                return Err(CoreError::validation(
                    "the tool's diameter must be more than zero",
                ));
            }
            Ok(Cutter::Ball {
                radius: diameter / 2.0,
            })
        }
        _ => cone(tool).map(|(half, flat, radius)| Cutter::Cone { half, flat, radius }),
    }
}

/// Half angle in radians, flat tip radius and cone radius of a V shaped
/// tool, all lengths in millimetres.
fn cone(tool: &Tool) -> Result<(f64, f64, f64), CoreError> {
    let (angle, flat, diameter) = match tool.tool_type {
        ToolGeometry::VBit {
            included_angle,
            diameter,
        } => (included_angle, 0.0, diameter),
        ToolGeometry::EngravingTool {
            included_angle,
            flat_diameter,
            diameter,
        } => (included_angle, flat_diameter, diameter),
        _ => {
            return Err(CoreError::validation(
                "a chamfer needs a V-bit, an engraving tool or a ball nose",
            ))
        }
    };
    if !(angle > 0.0 && angle < 180.0) {
        return Err(CoreError::validation(
            "the tool's included angle must be between 0 and 180 degrees",
        ));
    }
    let flat_radius = flat.max(0.0) / 2.0;
    let radius = diameter / 2.0;
    if !(radius.is_finite() && radius > flat_radius) {
        return Err(CoreError::validation(
            "the tool's diameter must be wider than its flat tip",
        ));
    }
    Ok(((angle / 2.0).to_radians(), flat_radius, radius))
}

/// One pass of a chamfer: where the flat's edge sits, measured from the
/// vector into the material, and the tip depth below the bevel's top edge.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChamferPass {
    inset: f64,
    depth: f64,
}

/// Passes that step down the bevel face from the top edge to the vector,
/// evenly spaced, each no wider than the cone reaches and no deeper than
/// `pass_depth` below the one before.
fn chamfer_passes(width: f64, tan_a: f64, reach: f64, pass_depth: f64) -> Vec<ChamferPass> {
    let depth = width / tan_a;
    let by_width = ((width / reach) - 1e-9).ceil().max(1.0);
    let by_depth = ((depth / pass_depth) - 1e-9).ceil().max(1.0);
    let count = by_width.max(by_depth) as usize;
    (1..=count)
        .map(|k| {
            let t = k as f64 / count as f64;
            if k == count {
                ChamferPass { inset: 0.0, depth }
            } else {
                ChamferPass {
                    inset: width * (1.0 - t),
                    depth: depth * t,
                }
            }
        })
        .collect()
}

/// Passes of a ball of radius `radius` down a bevel `width` wide at
/// `tan_a` (width over depth), contact points `step` apart along the face
/// from the top edge to the foot. `inset` is where the tool axis sits from
/// the foot into the material and `depth` how far its tip is below the top
/// edge.
fn ball_passes(width: f64, tan_a: f64, radius: f64, step: f64) -> Vec<ChamferPass> {
    let depth = width / tan_a;
    let face = width.hypot(depth);
    let count = ((face / step) - 1e-9).ceil().max(1.0) as usize;
    // The face's normal out of the material, towards the waste and up.
    let (nu, nz) = (-depth / face, width / face);
    (0..=count)
        .map(|k| {
            let t = k as f64 / count as f64;
            let (u, z) = (width * (1.0 - t), -depth * t);
            ChamferPass {
                inset: u + radius * nu,
                depth: -(z + radius * nz - radius),
            }
        })
        .filter(|p| p.depth > 1e-9)
        .collect()
}

/// Calculates a chamfer toolpath along `paths` with a V-bit, an engraving
/// tool or a ball nose.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped, a width
/// or depth that is not more than zero, a negative start depth, a pass depth
/// that is not more than zero, or when there is nothing to chamfer.
pub fn chamfer(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &ChamferParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let shape = cutter(tool)?;
    let tan_a = match shape {
        Cutter::Cone { half, .. } => half.tan(),
        Cutter::Ball { .. } => {
            if !(params.angle.is_finite() && params.angle > 0.0 && params.angle < 90.0) {
                return Err(CoreError::validation(
                    "the chamfer angle must be between 0 and 90 degrees",
                ));
            }
            params.angle.to_radians().tan()
        }
    };
    let width = match params.depth {
        Some(depth) => {
            if !(depth.is_finite() && depth > 0.0) {
                return Err(CoreError::validation(
                    "the chamfer depth must be more than zero",
                ));
            }
            depth * tan_a
        }
        None => params.width,
    };
    if !(width.is_finite() && width > 0.0) {
        return Err(CoreError::validation(
            "the chamfer width must be more than zero",
        ));
    }
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !(params.overcut.is_finite() && params.overcut >= 0.0) {
        return Err(CoreError::validation("the overcut must be zero or more"));
    }
    if !(tool.pass_depth.is_finite() && tool.pass_depth > 0.0) {
        return Err(CoreError::validation(
            "the tool's pass depth must be more than zero",
        ));
    }
    // A cut on the line has a bevel each side, so it keeps its own shape.
    let one_sided = params.side != ProfileSide::On;
    let overcut = if one_sided { params.overcut } else { 0.0 };
    let shift = overcut
        + if one_sided && params.slope == ChamferSlope::Down {
            width
        } else {
            0.0
        };
    let mut warnings = Vec::new();
    let (flat, passes) = match shape {
        Cutter::Cone { flat, radius, .. } => {
            let reach = radius - flat;
            if width + overcut > reach + 1e-9 {
                warnings.push(crate::warning::Warning::chamfer_multi_pass(reach));
            }
            (
                flat,
                chamfer_passes(width + overcut, tan_a, reach, tool.pass_depth),
            )
        }
        Cutter::Ball { radius } => {
            let step = if tool.stepover > 1e-9 {
                tool.stepover
            } else {
                radius / 2.0
            };
            (0.0, ball_passes(width + overcut, tan_a, radius, step))
        }
    };
    let passes: Vec<ChamferPass> = passes
        .into_iter()
        .map(|p| ChamferPass {
            inset: p.inset - shift,
            depth: p.depth,
        })
        .collect();
    let conventional = params.direction == CutDirection::Conventional;

    let region = Region::from_paths(paths, tol);
    let open: Vec<SubPath> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter(|s| !s.closed && !s.segments.is_empty())
        .cloned()
        .collect();
    if region.is_empty() && open.is_empty() {
        return Err(CoreError::validation("select the vectors to chamfer"));
    }

    // Every input vector's box, in selection order, to put the passes'
    // loops (which the offsets merge and reorder) back in that order.
    let sources: Vec<BBox> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter_map(|s| path_bbox(&s.clone().into_path()))
        .collect();
    let heights = Heights::from_job(job);
    let home = heights.home.xy();
    let mut out = Emitter::new(heights);
    let mut lost = false;
    for pass in &passes {
        let chains = pass_chains(
            &region,
            &open,
            params.side,
            flat,
            pass.inset,
            conventional,
            tol,
        )?;
        if chains.is_empty() {
            lost = true;
            continue;
        }
        let from = out.at().xy();
        let chains = if params.selection_order {
            let mut ranked: Vec<(usize, Chain)> = chains
                .into_iter()
                .map(|c| (selection_rank(c.start().unwrap_or(home), &sources), c))
                .collect();
            // Stable: the loops of one vector keep their order.
            ranked.sort_by_key(|(rank, _)| *rank);
            ranked.into_iter().map(|(_, c)| c).collect()
        } else {
            order_nearest_either_way(
                chains,
                from,
                |c| c.start().unwrap_or(home),
                |c| c.end().unwrap_or(home),
                |c| if c.closed { c } else { c.reversed() },
            )
        };
        let z = heights.z_at(params.start_depth + pass.depth);
        for chain in &chains {
            let Some(begin) = chain.start() else {
                continue;
            };
            out.travel_to(begin);
            out.plunge_to(z);
            out.cut_flat(&chain.segs, z);
        }
    }
    if lost {
        warnings.push(crate::warning::Warning::chamfer_passes_left_out());
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// The tool centre chains of one pass. `inset` is how far the flat's edge
/// sits from the vector into the material.
/// Place in the selection of the vector a pass loop starting at `at` was
/// made from: the vector whose box is nearest (inside counts as nearest),
/// the smaller box when nested ones both hold it.
fn selection_rank(at: Point, sources: &[BBox]) -> usize {
    let gap = |b: &BBox| {
        let dx = (b.min.x - at.x).max(at.x - b.max.x).max(0.0);
        let dy = (b.min.y - at.y).max(at.y - b.max.y).max(0.0);
        dx.hypot(dy)
    };
    let area = |b: &BBox| (b.max.x - b.min.x) * (b.max.y - b.min.y);
    sources
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| gap(a).total_cmp(&gap(b)).then(area(a).total_cmp(&area(b))))
        .map_or(0, |(i, _)| i)
}

fn pass_chains(
    region: &Region,
    open: &[SubPath],
    side: ProfileSide,
    flat: f64,
    inset: f64,
    conventional: bool,
    tol: Tolerance,
) -> Result<Vec<Chain>, CoreError> {
    let mut chains = Vec::new();
    // For one sided cuts the axis sits `flat - inset` to the waste side; a
    // region offset shrinks for positive values.
    let mut add_region = |r: &Region, reverse: bool| {
        for sub in &r.loops {
            let chain = Chain::from_subpath(sub, tol);
            if chain.is_empty() {
                continue;
            }
            chains.push(if reverse != conventional {
                chain.reversed()
            } else {
                chain
            });
        }
    };
    match side {
        ProfileSide::Outside => add_region(&region.offset(inset - flat, tol), true),
        ProfileSide::Inside => add_region(&region.offset(flat - inset, tol), false),
        ProfileSide::On => {
            if inset <= 1e-9 {
                add_region(region, false);
            } else {
                add_region(&region.offset(inset, tol), false);
                add_region(&region.offset(-inset, tol), false);
            }
        }
    }
    for sub in open {
        if inset <= 1e-9 {
            let chain = Chain::from_subpath(sub, tol);
            if !chain.is_empty() {
                chains.push(chain);
            }
            continue;
        }
        let path = sub.clone().into_path();
        for left in [true, false] {
            for piece in parallel_offset_open(&path, inset, left, tol)? {
                for s in &piece.subpaths {
                    let chain = Chain::from_subpath(s, tol);
                    if !chain.is_empty() {
                        chains.push(chain);
                    }
                }
            }
        }
    }
    Ok(chains)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::Move;
    use crate::geom::{line, rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn square() -> Path {
        rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")
    }

    fn vbit(angle: f64, diameter: f64) -> Tool {
        Tool {
            name: "V".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter,
                included_angle: angle,
            },
            pass_depth: 10.0,
            ..Tool::default()
        }
    }

    fn engraver(flat: f64) -> Tool {
        Tool {
            name: "E".to_string(),
            tool_type: ToolGeometry::EngravingTool {
                diameter: 12.0,
                included_angle: 90.0,
                flat_diameter: flat,
            },
            pass_depth: 10.0,
            ..Tool::default()
        }
    }

    fn cutting(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind.is_cutting()).collect()
    }

    fn feeds_at(moves: &[Move], z: f64) -> Vec<&Move> {
        cutting(moves)
            .into_iter()
            .filter(|m| (m.to.z - z).abs() < 1e-9)
            .collect()
    }

    fn deepest(moves: &[Move]) -> f64 {
        moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min)
    }

    #[test]
    fn a_ninety_degree_bit_runs_on_the_square_as_deep_as_the_width() {
        let params = ChamferParams {
            width: 2.0,
            ..ChamferParams::default()
        };
        let done = chamfer(
            &[square()],
            &JobSetup::default(),
            &vbit(90.0, 12.0),
            &params,
            tol(),
        )
        .expect("chamfer");
        assert!(done.warnings.is_empty(), "{:?}", done.warnings);
        assert!((deepest(&done.moves) + 2.0).abs() < 1e-9);
        let at_depth = feeds_at(&done.moves, -2.0);
        assert!(at_depth.len() >= 4);
        for m in at_depth {
            let on_edge = m.to.x.abs() < 1e-6
                || (m.to.x - 10.0).abs() < 1e-6
                || m.to.y.abs() < 1e-6
                || (m.to.y - 10.0).abs() < 1e-6;
            assert!(on_edge, "{m:?}");
        }
    }

    #[test]
    fn a_depth_drives_the_width_through_the_angle() {
        let params = ChamferParams {
            depth: Some(1.5),
            width: 99.0,
            ..ChamferParams::default()
        };
        let done = chamfer(
            &[square()],
            &JobSetup::default(),
            &vbit(60.0, 12.0),
            &params,
            tol(),
        )
        .expect("chamfer");
        assert!((deepest(&done.moves) + 1.5).abs() < 1e-9);
        // 60 degrees: the width a 1.5 deep bevel makes is 1.5 tan 30.
        let passes = chamfer_passes(
            1.5 * 30f64.to_radians().tan(),
            30f64.to_radians().tan(),
            6.0,
            10.0,
        );
        assert_eq!(passes.len(), 1);
        assert!((passes[0].depth - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_flat_tip_runs_to_the_waste_side() {
        let params = ChamferParams {
            width: 1.0,
            start_depth: 0.5,
            ..ChamferParams::default()
        };
        let outside = chamfer(
            &[square()],
            &JobSetup::default(),
            &engraver(1.0),
            &params,
            tol(),
        )
        .expect("outside");
        let z = -1.5;
        assert!((deepest(&outside.moves) - z).abs() < 1e-9);
        // The straight sides of the outside path lie 0.5 outside the square.
        assert!(feeds_at(&outside.moves, z)
            .iter()
            .any(|m| (m.to.x + 0.5).abs() < 1e-6 || (m.to.x - 10.5).abs() < 1e-6));
        assert!(feeds_at(&outside.moves, z)
            .iter()
            .all(|m| m.to.x <= -0.5 + 1e-6
                || m.to.x >= 10.5 - 1e-6
                || m.to.y <= -0.5 + 1e-6
                || m.to.y >= 10.5 - 1e-6));
        let inside = chamfer(
            &[square()],
            &JobSetup::default(),
            &engraver(1.0),
            &ChamferParams {
                side: ProfileSide::Inside,
                ..params
            },
            tol(),
        )
        .expect("inside");
        assert!(feeds_at(&inside.moves, z)
            .iter()
            .all(|m| m.to.x >= 0.5 - 1e-6
                && m.to.x <= 9.5 + 1e-6
                && m.to.y >= 0.5 - 1e-6
                && m.to.y <= 9.5 + 1e-6));
    }

    #[test]
    fn a_wide_chamfer_steps_out_from_the_material_and_warns() {
        let params = ChamferParams {
            width: 5.0,
            ..ChamferParams::default()
        };
        // A 4 mm bit reaches 2 mm, so three passes.
        let done = chamfer(
            &[square()],
            &JobSetup::default(),
            &vbit(90.0, 4.0),
            &params,
            tol(),
        )
        .expect("chamfer");
        assert_eq!(done.warnings.len(), 1);
        let passes = chamfer_passes(5.0, 1.0, 2.0, 10.0);
        assert_eq!(passes.len(), 3);
        assert!(passes
            .windows(2)
            .all(|w| w[1].inset < w[0].inset && w[1].depth > w[0].depth));
        for p in &passes {
            // Every tip lies on the finished bevel face.
            assert!((p.inset + p.depth - 5.0).abs() < 1e-9);
        }
        // The first pass is inside the square and shallow.
        let first = -5.0 / 3.0;
        let inset = 5.0 - 5.0 / 3.0;
        assert!(feeds_at(&done.moves, first)
            .iter()
            .all(|m| m.to.x >= inset - 1e-6 && m.to.x <= 10.0 - inset + 1e-6));
        assert!((deepest(&done.moves) + 5.0).abs() < 1e-9);
        let z_order: Vec<f64> = cutting(&done.moves).iter().map(|m| m.to.z).collect();
        let first_deep = z_order.iter().position(|z| (*z + 5.0).abs() < 1e-9);
        let last_shallow = z_order.iter().rposition(|z| (*z - first).abs() < 1e-9);
        assert!(last_shallow < first_deep);
    }

    #[test]
    fn the_pass_depth_splits_a_deep_chamfer() {
        let mut tool = vbit(90.0, 20.0);
        tool.pass_depth = 1.0;
        let passes = chamfer_passes(3.0, 1.0, 10.0, tool.pass_depth);
        assert_eq!(passes.len(), 3);
        let done = chamfer(
            &[square()],
            &JobSetup::default(),
            &tool,
            &ChamferParams {
                width: 3.0,
                ..ChamferParams::default()
            },
            tol(),
        )
        .expect("chamfer");
        assert!(done.warnings.is_empty());
        for z in [-1.0, -2.0, -3.0] {
            assert!(!feeds_at(&done.moves, z).is_empty(), "no pass at {z}");
        }
    }

    #[test]
    fn an_open_vector_is_cut_on_the_line_and_on_both_sides_when_wide() {
        let open = line(Point::new(0.0, 0.0), Point::new(20.0, 0.0));
        let done = chamfer(
            std::slice::from_ref(&open),
            &JobSetup::default(),
            &vbit(90.0, 12.0),
            &ChamferParams {
                width: 1.0,
                ..ChamferParams::default()
            },
            tol(),
        )
        .expect("chamfer");
        let at = feeds_at(&done.moves, -1.0);
        assert!(!at.is_empty());
        assert!(at.iter().all(|m| m.to.y.abs() < 1e-9));
        let wide = chamfer(
            &[open],
            &JobSetup::default(),
            &vbit(90.0, 4.0),
            &ChamferParams {
                width: 4.0,
                ..ChamferParams::default()
            },
            tol(),
        )
        .expect("wide");
        let first = feeds_at(&wide.moves, -2.0);
        assert!(first.iter().any(|m| (m.to.y - 2.0).abs() < 1e-6));
        assert!(first.iter().any(|m| (m.to.y + 2.0).abs() < 1e-6));
        assert!(feeds_at(&wide.moves, -4.0)
            .iter()
            .all(|m| m.to.y.abs() < 1e-9));
    }

    #[test]
    fn bad_settings_are_refused() {
        let job = JobSetup::default();
        let end_mill = Tool::default();
        assert!(chamfer(
            &[square()],
            &job,
            &end_mill,
            &ChamferParams::default(),
            tol()
        )
        .is_err());
        let v = vbit(90.0, 12.0);
        let zero = ChamferParams {
            width: 0.0,
            ..ChamferParams::default()
        };
        assert!(chamfer(&[square()], &job, &v, &zero, tol()).is_err());
        let no_depth = ChamferParams {
            depth: Some(-1.0),
            ..ChamferParams::default()
        };
        assert!(chamfer(&[square()], &job, &v, &no_depth, tol()).is_err());
        assert!(chamfer(&[], &job, &v, &ChamferParams::default(), tol()).is_err());
    }

    #[test]
    fn settings_read_camel_case_with_defaults() {
        let params: ChamferParams =
            serde_json::from_str(r#"{"width":2,"side":"inside","startDepth":1}"#).expect("json");
        assert_eq!(params.side, ProfileSide::Inside);
        assert_eq!(params.start_depth, 1.0);
        assert_eq!(params.depth, None);
    }

    fn run_square(params: &ChamferParams, tool: &Tool) -> Calculated {
        chamfer(&[square()], &JobSetup::default(), tool, params, tol()).expect("chamfer")
    }

    fn signed_area_at(moves: &[Move], z: f64) -> f64 {
        let pts: Vec<Point> = feeds_at(moves, z).iter().map(|m| m.to.xy()).collect();
        pts.windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum()
    }

    #[test]
    fn an_overcut_moves_the_tip_out_and_down_the_same_face() {
        let params = ChamferParams {
            width: 2.0,
            overcut: 0.5,
            ..ChamferParams::default()
        };
        let done = run_square(&params, &vbit(90.0, 12.0));
        assert!((deepest(&done.moves) + 2.5).abs() < 1e-9);
        // The tip runs half a millimetre outside the square.
        assert!(feeds_at(&done.moves, -2.5)
            .iter()
            .any(|m| (m.to.x + 0.5).abs() < 1e-6));
    }

    #[test]
    fn a_downward_slope_puts_the_vector_on_the_top_edge() {
        let params = ChamferParams {
            width: 2.0,
            slope: ChamferSlope::Down,
            ..ChamferParams::default()
        };
        let done = run_square(&params, &vbit(90.0, 12.0));
        assert!(feeds_at(&done.moves, -2.0)
            .iter()
            .all(|m| m.to.x <= -2.0 + 1e-6
                || m.to.x >= 12.0 - 1e-6
                || m.to.y <= -2.0 + 1e-6
                || m.to.y >= 12.0 - 1e-6));
    }

    #[test]
    fn conventional_runs_the_loops_the_other_way() {
        let climb = run_square(&ChamferParams::default(), &vbit(90.0, 12.0));
        let conventional = run_square(
            &ChamferParams {
                direction: CutDirection::Conventional,
                ..ChamferParams::default()
            },
            &vbit(90.0, 12.0),
        );
        let (a, b) = (
            signed_area_at(&climb.moves, -3.0),
            signed_area_at(&conventional.moves, -3.0),
        );
        assert!(a * b < 0.0, "{a} {b}");
    }

    #[test]
    fn a_ball_nose_steps_down_the_face_at_the_given_angle() {
        let mut ball = Tool {
            name: "B".to_string(),
            tool_type: ToolGeometry::BallNose { diameter: 2.0 },
            pass_depth: 10.0,
            ..Tool::default()
        };
        ball.set_stepover(0.5);
        let params = ChamferParams {
            width: 3.0,
            angle: 45.0,
            ..ChamferParams::default()
        };
        let done = run_square(&params, &ball);
        let levels: std::collections::BTreeSet<i64> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.to.z < 0.0)
            .map(|m| (m.to.z * 1000.0).round() as i64)
            .collect();
        assert!(levels.len() >= 8, "{levels:?}");
        // The last pass touches the foot: centre 1 mm off the face, so the
        // tip is 3 + (1 - 1 / sqrt 2) deep and the axis 1 / sqrt 2 out.
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let want = -(3.0 + 1.0 - half);
        assert!(
            (deepest(&done.moves) - want).abs() < 1e-6,
            "{}",
            deepest(&done.moves)
        );
        assert!(feeds_at(&done.moves, want)
            .iter()
            .any(|m| (m.to.x + half).abs() < 1e-6));
        // An angle is needed for a ball.
        let flat = ChamferParams {
            angle: 90.0,
            ..params
        };
        assert!(chamfer(&[square()], &JobSetup::default(), &ball, &flat, tol()).is_err());
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
    fn use_vector_selection_order_chamfers_in_the_order_selected() {
        let squares: Vec<Path> = [100.0, 0.0, 50.0]
            .iter()
            .map(|x| rect(Point::new(*x, 0.0), Point::new(x + 10.0, 10.0)).expect("rect"))
            .collect();
        let by_x = |x: f64, _y: f64| ((x + 20.0) / 50.0) as usize;
        let tool = vbit(90.0, 12.0);
        let nearest = chamfer(
            &squares,
            &JobSetup::default(),
            &tool,
            &ChamferParams::default(),
            tol(),
        )
        .expect("chamfer");
        assert_eq!(plunge_order(&nearest.moves, by_x), vec![0, 1, 2]);
        let params = ChamferParams {
            selection_order: true,
            ..ChamferParams::default()
        };
        let picked =
            chamfer(&squares, &JobSetup::default(), &tool, &params, tol()).expect("chamfer");
        assert_eq!(plunge_order(&picked.moves, by_x), vec![2, 0, 1]);
    }
}
