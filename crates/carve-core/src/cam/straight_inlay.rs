//! Straight walled inlays: a male insert and the female pocket or hole it
//! fits, cut with the same end mill from the same vectors.
//!
//! The female side is a pocket (or a hole cut through with an inside
//! profile), so the tool rounds its corners that point away from the
//! cavity to the tool's radius. The male is cut round the shape with those
//! same corners rounded (the shape opened by the tool radius: shrunk and
//! grown back), so the two meet. A stepped male leaves a flange a step
//! width wide round the shape, cut down by the step depth, for letters that
//! push through a hole from behind.
//!
//! Allowances make room for glue: the male's allowance cuts it smaller and
//! the female's pocket allowance cuts the cavity larger.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{subpath_signed_area, Path, Region, Tolerance};

use super::common::TabSpec;
use super::pocket::{
    pocket, pocket_tool_list, pocket_with_clearance, ClearStrategy, PocketParams,
    TwoToolPocketParams,
};
use super::profile::{profile, Calculated, CutDirection, LastPass, ProfileParams, ProfileSide};
use super::tool::Tool;

/// Which half of the inlay a toolpath cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InlayHalf {
    /// The cavity.
    #[default]
    Female,
    /// The insert.
    Male,
}

/// The shape of the insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaleKind {
    /// Straight sides all the way down.
    #[default]
    Straight,
    /// With a flange round its foot.
    Stepped,
}

/// The shape of the cavity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FemaleKind {
    /// A pocket cleared to a floor.
    #[default]
    Pocket,
    /// A hole cut round its edge, usually right through.
    Hole,
}

/// The Inlay form's settings. Lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StraightInlayParams {
    /// Which half this toolpath cuts.
    pub part: InlayHalf,
    /// Straight or stepped insert.
    pub male: MaleKind,
    /// Pocket or hole.
    pub female: FemaleKind,
    /// Depth the cut starts at.
    pub start_depth: f64,
    /// Depth the cut finishes at.
    pub cut_depth: f64,
    /// Depth of each pass; the tool's own when absent.
    pub pass_depth: Option<f64>,
    /// Extra cut off the insert, and off the hole's edge.
    pub allowance: f64,
    /// Extra cut round the pocket's edge.
    pub pocket_allowance: f64,
    /// How far below the start depth a stepped insert's flange is cut.
    pub step_depth: f64,
    /// How wide a stepped insert's flange is, from the shape to its edge.
    pub step_width: f64,
    /// Climb or conventional.
    pub direction: CutDirection,
    /// Tabs holding the insert, and a hole's waste, in the sheet.
    pub tabs: TabSpec,
    /// Sideways step for the pocket; the tool's own when absent.
    pub stepover: Option<f64>,
    /// Offset or raster pocket clearing.
    pub strategy: ClearStrategy,
    /// Raster angle in degrees.
    pub raster_angle: f64,
    /// A larger tool that clears the pocket first (older toolpaths; the
    /// form sends `rest_tools`).
    pub clearance_tool: Option<Tool>,
    /// The Profile options of an insert or a hole (passes, tabs, ramps,
    /// leads, order, start point), as the Profile form sends them. The
    /// side, depths and allowance are the inlay's own, and sharp corners
    /// and a separate last pass do not apply.
    pub profile_options: Option<ProfileParams>,
    /// The Pocket options of a pocket (passes, clearing, ramps, allowance,
    /// selection order), as the Pocket form sends them; the depths are the
    /// inlay's own.
    pub pocket_options: Option<PocketParams>,
    /// A pocket's tools after the request's own, in cutting order; the last
    /// one should be the insert's tool.
    pub rest_tools: Vec<Tool>,
}

impl Default for StraightInlayParams {
    fn default() -> Self {
        StraightInlayParams {
            part: InlayHalf::Female,
            male: MaleKind::Straight,
            female: FemaleKind::Pocket,
            start_depth: 0.0,
            cut_depth: 6.0,
            pass_depth: None,
            allowance: 0.0,
            pocket_allowance: 0.0,
            step_depth: 3.0,
            step_width: 5.0,
            direction: CutDirection::Climb,
            tabs: TabSpec::default(),
            stepover: None,
            strategy: ClearStrategy::Offset,
            raster_angle: 0.0,
            clearance_tool: None,
            profile_options: None,
            pocket_options: None,
            rest_tools: Vec::new(),
        }
    }
}

/// One half of an inlay, and for a pocket with a larger tool the pass that
/// tool cuts first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlayCut {
    /// The main tool's moves.
    pub main: Calculated,
    /// The clearance tool's moves, cut first.
    pub clearance: Option<Calculated>,
    /// A pocket's later tools, in list order; `None` for one with nothing
    /// left to cut.
    pub later: Vec<Option<Calculated>>,
}

/// Calculates one half of a straight walled inlay from `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] when there is no closed vector, the
/// shape is too thin for the tool, for step sizes that make no sense, and
/// whatever the profile and pocket strategies refuse.
pub fn straight_inlay(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &StraightInlayParams,
    tol: Tolerance,
) -> Result<InlayCut, CoreError> {
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation(
            "an inlay needs at least one closed vector",
        ));
    }
    for value in [params.allowance, params.pocket_allowance] {
        if !value.is_finite() {
            return Err(CoreError::validation("the allowances must be numbers"));
        }
    }
    let profile_params =
        |side: ProfileSide, allowance: f64, start: f64, cut: f64| match &params.profile_options {
            Some(options) => ProfileParams {
                start_depth: start,
                cut_depth: cut,
                side,
                allowance,
                sharp_corners: false,
                sharp_internal_corners: false,
                last_pass: LastPass::default(),
                ..options.clone()
            },
            None => ProfileParams {
                start_depth: start,
                cut_depth: cut,
                pass_depth: params.pass_depth,
                side,
                direction: params.direction,
                allowance,
                tabs: params.tabs,
                ..ProfileParams::default()
            },
        };
    match params.part {
        InlayHalf::Female => match params.female {
            FemaleKind::Pocket => {
                if let Some(options) = &params.pocket_options {
                    let options = PocketParams {
                        start_depth: params.start_depth,
                        cut_depth: params.cut_depth,
                        ..options.clone()
                    };
                    for tool in &params.rest_tools {
                        tool.validate()?;
                    }
                    let out =
                        pocket_tool_list(paths, job, tool, &params.rest_tools, &options, tol)?;
                    return Ok(InlayCut {
                        main: out.first,
                        clearance: None,
                        later: out.later,
                    });
                }
                let pocket_params = TwoToolPocketParams {
                    pocket: PocketParams {
                        start_depth: params.start_depth,
                        cut_depth: params.cut_depth,
                        pass_depth: params.pass_depth,
                        stepover: params.stepover,
                        strategy: params.strategy,
                        raster_angle: params.raster_angle,
                        direction: params.direction,
                        allowance: -params.pocket_allowance,
                        ..PocketParams::default()
                    },
                    clearance_tool: params.clearance_tool.clone(),
                };
                let out = pocket_with_clearance(paths, job, tool, &pocket_params, tol)?;
                Ok(InlayCut {
                    main: out.pocket,
                    clearance: out.clearance,
                    later: Vec::new(),
                })
            }
            FemaleKind::Hole => {
                let hole = profile_params(
                    ProfileSide::Inside,
                    -params.allowance,
                    params.start_depth,
                    params.cut_depth,
                );
                Ok(InlayCut {
                    main: profile(paths, job, tool, &hole, tol)?,
                    clearance: None,
                    later: Vec::new(),
                })
            }
        },
        InlayHalf::Male => {
            let radius = tool.radius();
            let shrunk = region.offset(radius, tol);
            if shrunk.is_empty() {
                return Err(CoreError::validation(
                    "the shapes are too thin for the tool to cut their pocket, so no insert fits",
                ));
            }
            // The shape as the pocket comes out: corners rounded by the tool.
            let insert = shrunk.offset(-radius, tol);
            match params.male {
                MaleKind::Straight => {
                    let cut_out = profile_params(
                        ProfileSide::Outside,
                        -params.allowance,
                        params.start_depth,
                        params.cut_depth,
                    );
                    Ok(InlayCut {
                        main: profile(&[insert.as_path()], job, tool, &cut_out, tol)?,
                        clearance: None,
                        later: Vec::new(),
                    })
                }
                MaleKind::Stepped => stepped(&insert, job, tool, params, &profile_params, tol),
            }
        }
    }
}

/// A stepped insert: the flange round the shape is pocketed down by the
/// step depth, then the flange's edge (and the shape's counters) are cut
/// out to the full depth.
fn stepped(
    insert: &Region,
    job: &JobSetup,
    tool: &Tool,
    params: &StraightInlayParams,
    profile_params: &dyn Fn(ProfileSide, f64, f64, f64) -> ProfileParams,
    tol: Tolerance,
) -> Result<InlayCut, CoreError> {
    if !(params.step_width.is_finite() && params.step_width > 0.0) {
        return Err(CoreError::validation(
            "the step width must be more than zero",
        ));
    }
    if !(params.step_depth.is_finite()
        && params.step_depth > 0.0
        && params.start_depth + params.step_depth < params.cut_depth)
    {
        return Err(CoreError::validation(
            "the step depth must be more than zero and less than the cut depth",
        ));
    }
    // The shape itself, allowance taken off, stands up inside the flange.
    let face = insert.offset(params.allowance, tol);
    let flange = insert.offset(-params.step_width, tol);
    let outers = |r: &Region| -> Vec<Path> {
        r.loops
            .iter()
            .filter(|s| subpath_signed_area(s) > 0.0)
            .map(|s| s.clone().into_path())
            .collect()
    };
    let counters: Vec<Path> = face
        .loops
        .iter()
        .filter(|s| subpath_signed_area(s) < 0.0)
        .map(|s| s.clone().into_path())
        .collect();
    // The ring between the flange's edge and the shape's outline.
    let mut ring = outers(&flange);
    ring.extend(outers(&face));
    let step = PocketParams {
        start_depth: params.start_depth,
        cut_depth: params.start_depth + params.step_depth,
        pass_depth: params.pass_depth,
        stepover: params.stepover,
        strategy: params.strategy,
        raster_angle: params.raster_angle,
        direction: params.direction,
        ..PocketParams::default()
    };
    let lowered = pocket(&ring, job, tool, &step, tol)?;
    // The flange's edge with the counters as holes, cut through.
    let mut cut_out_paths = outers(&flange);
    cut_out_paths.extend(counters);
    let cut_out = profile_params(
        ProfileSide::Outside,
        0.0,
        params.start_depth,
        params.cut_depth,
    );
    let through = profile(&cut_out_paths, job, tool, &cut_out, tol)?;
    // One move list: the step first, then the cut out, both from home.
    let mut moves = lowered.moves;
    moves.extend(through.moves);
    let mut warnings = lowered.warnings;
    warnings.extend(through.warnings);
    Ok(InlayCut {
        main: Calculated { moves, warnings },
        clearance: None,
        later: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::{rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(1e-3).expect("tolerance")
    }

    fn mill() -> Tool {
        let mut tool = Tool {
            name: "End mill".to_string(),
            pass_depth: 3.0,
            ..Tool::default()
        };
        tool.set_stepover(2.4);
        tool
    }

    fn square() -> Path {
        rect(Point::new(0.0, 0.0), Point::new(40.0, 30.0)).expect("rect")
    }

    fn deep(moves: &[Move], z: f64) -> Vec<Point> {
        moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z - z).abs() < 1e-9)
            .map(|m| m.to.xy())
            .collect()
    }

    fn cut(params: &StraightInlayParams) -> InlayCut {
        straight_inlay(&[square()], &JobSetup::default(), &mill(), params, tol()).expect("inlay")
    }

    #[test]
    fn the_male_is_cut_round_the_shape_with_rounded_corners() {
        let done = cut(&StraightInlayParams {
            part: InlayHalf::Male,
            ..StraightInlayParams::default()
        });
        let pts = deep(&done.main.moves, -6.0);
        assert!(!pts.is_empty());
        // Straight sides run a radius outside the square.
        assert!(pts.iter().any(|p| (p.x + 3.0).abs() < 1e-6));
        assert!(pts.iter().all(|p| p.x >= -3.0 - 1e-6 && p.x <= 43.0 + 1e-6));
        // The corner is rounded to the tool radius, so the path's corner arc
        // runs from (-3, 3) to (3, -3) round (3, 3); unrounded it would run
        // from (-3, 0) to (0, -3) round the corner itself.
        let near = pts
            .iter()
            .map(|p| p.distance(Point::new(0.0, 0.0)))
            .fold(f64::MAX, f64::min);
        assert!((near - 18.0_f64.sqrt()).abs() < 1e-6, "{near}");
        assert!(done.main.moves.iter().any(|m| m.kind.is_arc()));
    }

    #[test]
    fn profile_options_reach_the_insert_but_not_its_side_or_corners() {
        let options = ProfileParams {
            pass_depth: Some(2.0),
            side: ProfileSide::Inside,
            sharp_corners: true,
            ..ProfileParams::default()
        };
        let done = cut(&StraightInlayParams {
            part: InlayHalf::Male,
            profile_options: Some(options),
            ..StraightInlayParams::default()
        });
        // Passes of the options' 2 mm: 2, 4 and 6 deep.
        for z in [-2.0, -4.0, -6.0] {
            assert!(!deep(&done.main.moves, z).is_empty(), "no pass at {z}");
        }
        // Still round the outside with the rounded corner.
        let pts = deep(&done.main.moves, -6.0);
        assert!(pts.iter().any(|p| (p.x + 3.0).abs() < 1e-6));
        let near = pts
            .iter()
            .map(|p| p.distance(Point::new(0.0, 0.0)))
            .fold(f64::MAX, f64::min);
        assert!((near - 18.0_f64.sqrt()).abs() < 1e-6, "{near}");
    }

    #[test]
    fn pocket_options_and_a_tool_list_cut_the_cavity() {
        let options = PocketParams {
            allowance: -0.5,
            strategy: ClearStrategy::Raster,
            ..PocketParams::default()
        };
        let mut small = mill();
        small.name = "Small".to_string();
        small.tool_type = crate::cam::ToolGeometry::EndMill { diameter: 2.0 };
        let done = cut(&StraightInlayParams {
            pocket_options: Some(options),
            rest_tools: vec![small],
            ..StraightInlayParams::default()
        });
        let pts = deep(&done.main.moves, -6.0);
        assert!(!pts.is_empty());
        assert_eq!(done.later.len(), 1);
        assert!(done.later[0].is_some(), "the small tool cleans the corners");
    }

    #[test]
    fn the_female_pocket_is_cleared_and_its_allowance_grows_it() {
        let done = cut(&StraightInlayParams {
            pocket_allowance: 0.5,
            ..StraightInlayParams::default()
        });
        let pts = deep(&done.main.moves, -6.0);
        let min_x = pts.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        assert!((min_x - 2.5).abs() < 1e-6, "{min_x}");
        assert!(done.clearance.is_none());
    }

    #[test]
    fn a_female_hole_is_an_inside_profile() {
        let done = cut(&StraightInlayParams {
            female: FemaleKind::Hole,
            ..StraightInlayParams::default()
        });
        let pts = deep(&done.main.moves, -6.0);
        let min_x = pts.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        let max_x = pts.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        assert!((min_x - 3.0).abs() < 1e-6 && (max_x - 37.0).abs() < 1e-6);
    }

    #[test]
    fn a_stepped_male_lowers_a_flange_then_cuts_its_edge_through() {
        let done = cut(&StraightInlayParams {
            part: InlayHalf::Male,
            male: MaleKind::Stepped,
            step_depth: 2.0,
            step_width: 8.0,
            ..StraightInlayParams::default()
        });
        let moves = &done.main.moves;
        // The flange is pocketed to 2 mm, between the shape and 8 mm out.
        let step = deep(moves, -2.0);
        assert!(!step.is_empty());
        assert!(step.iter().all(|p| p.x < 3.0 + 1e-6
            || p.x > 37.0 - 1e-6
            || p.y < 3.0 + 1e-6
            || p.y > 27.0 - 1e-6));
        // The cut out runs a radius outside the flange's edge, to full depth.
        let through = deep(moves, -6.0);
        let min_x = through.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        assert!((min_x + 11.0).abs() < 1e-6, "{min_x}");
        // The step comes before the cut out.
        let first_step = moves
            .iter()
            .position(|m| (m.to.z + 2.0).abs() < 1e-9 && m.kind.is_cutting());
        let first_through = moves.iter().position(|m| (m.to.z + 6.0).abs() < 1e-9);
        assert!(first_step < first_through);
        assert_eq!(moves.last().map(|m| m.kind), Some(MoveKind::Rapid));
    }

    #[test]
    fn bad_steps_and_thin_shapes_are_refused() {
        let params = StraightInlayParams {
            part: InlayHalf::Male,
            male: MaleKind::Stepped,
            step_depth: 7.0,
            ..StraightInlayParams::default()
        };
        assert!(
            straight_inlay(&[square()], &JobSetup::default(), &mill(), &params, tol()).is_err()
        );
        let thin = rect(Point::new(0.0, 0.0), Point::new(40.0, 4.0)).expect("rect");
        let male = StraightInlayParams {
            part: InlayHalf::Male,
            ..StraightInlayParams::default()
        };
        assert!(straight_inlay(&[thin], &JobSetup::default(), &mill(), &male, tol()).is_err());
    }
}
