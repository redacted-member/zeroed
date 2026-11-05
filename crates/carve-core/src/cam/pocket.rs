//! Pocket toolpaths: clear the area inside closed vectors, islands left
//! standing, level by level down to the cut depth.
//!
//! Offset clearing cuts rings a stepover apart from the middle outwards and
//! finishes with the boundary; raster clearing cuts back and forth lines at
//! an angle, then runs the boundary. Moves between cuts stay down when the
//! link is short and inside the pocket, and lift otherwise.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{
    flatten_subpath, intersect_paths, subtract, Mat3, Path, Point, Region, Tolerance,
};

use super::chain::{Chain, CutSeg};
use super::common::{
    order_nearest, planned_depths, region_parts, Emitter, Heights, PassPlan, RampSpec,
};
use super::profile::{Calculated, CutDirection};
use super::tool::Tool;

/// How the inside of a pocket is cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClearStrategy {
    /// Rings following the boundary.
    #[default]
    Offset,
    /// Straight lines at an angle.
    Raster,
}

/// When raster clearing runs round the pocket's boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfilePassWhen {
    /// After the raster lines, cleaning up their ends.
    #[default]
    Last,
    /// Before the raster lines.
    First,
}

/// The Pocket form's settings. Depths are millimetres below the surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PocketParams {
    /// Depth the cut starts at.
    pub start_depth: f64,
    /// Depth of the pocket floor.
    pub cut_depth: f64,
    /// Depth of each pass; the tool's own pass depth when absent.
    pub pass_depth: Option<f64>,
    /// Evenly spaced passes, this many, instead of by the pass depth.
    pub pass_count: Option<usize>,
    /// Thickness of the last depth pass, in millimetres, when it is set
    /// apart from the others.
    pub last_pass_thickness: Option<f64>,
    /// Keep every pass exactly the pass depth instead of letting it grow by
    /// up to 15% to save a pass.
    pub exact_pass_depth: bool,
    /// Custom pass depths below the surface, in millimetres, from the Edit
    /// Passes dialog; they win over the other pass settings.
    pub pass_list: Option<Vec<f64>>,
    /// Sideways step between rings or lines; the tool's own when absent.
    pub stepover: Option<f64>,
    /// Offset or raster clearing.
    pub strategy: ClearStrategy,
    /// Raster line angle in degrees from the X axis.
    pub raster_angle: f64,
    /// Climb or conventional.
    pub direction: CutDirection,
    /// Material left on the walls, in millimetres.
    pub allowance: f64,
    /// Run round the boundary as well as the raster lines.
    pub profile_pass: bool,
    /// Whether that boundary pass comes before or after the lines.
    pub profile_pass_when: ProfilePassWhen,
    /// Ramping into each level.
    pub ramp: RampSpec,
    /// Clear separate pockets in the order their vectors were selected;
    /// otherwise the nearest next.
    pub selection_order: bool,
    /// Raster clearing cuts each separate strip of the area back and forth
    /// before the next (the "Use new raster" preference). Set by the
    /// calculation from the preferences, never read from the form.
    #[serde(skip)]
    pub strips: bool,
}

impl Default for PocketParams {
    fn default() -> Self {
        PocketParams {
            start_depth: 0.0,
            cut_depth: 3.0,
            pass_depth: None,
            pass_count: None,
            last_pass_thickness: None,
            exact_pass_depth: false,
            pass_list: None,
            stepover: None,
            strategy: ClearStrategy::Offset,
            raster_angle: 0.0,
            direction: CutDirection::Climb,
            allowance: 0.0,
            profile_pass: true,
            profile_pass_when: ProfilePassWhen::Last,
            ramp: RampSpec::default(),
            selection_order: false,
            strips: false,
        }
    }
}

/// How a flat area clearance tool clears: the choices a strategy that
/// clears flat areas as a side job (V-Groove, inlay) offers for it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClearanceOptions {
    /// Offset or raster clearing.
    pub strategy: ClearStrategy,
    /// Raster line angle in degrees from the X axis.
    pub raster_angle: f64,
    /// Climb or conventional.
    pub direction: CutDirection,
    /// Ramping into each level.
    pub ramp: RampSpec,
}

impl Default for ClearanceOptions {
    fn default() -> Self {
        ClearanceOptions {
            strategy: ClearStrategy::Offset,
            raster_angle: 0.0,
            direction: CutDirection::Climb,
            ramp: RampSpec::default(),
        }
    }
}

impl ClearanceOptions {
    /// Pocket settings clearing from `start_depth` to `cut_depth` this way.
    pub fn pocket_params(&self, start_depth: f64, cut_depth: f64) -> PocketParams {
        PocketParams {
            start_depth,
            cut_depth,
            strategy: self.strategy,
            raster_angle: self.raster_angle,
            direction: self.direction,
            ramp: self.ramp,
            ..PocketParams::default()
        }
    }
}

/// The Pocket form's settings with the optional second, larger tool. The
/// pocket settings sit at the top level of the JSON beside
/// `clearanceTool`, so plain pocket settings parse unchanged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwoToolPocketParams {
    /// Depths, stepover, strategy and the rest, shared by both tools.
    #[serde(flatten)]
    pub pocket: PocketParams,
    /// Larger tool that clears the bulk of the pocket first; the main tool
    /// then only cuts what it could not reach. `None` for a one tool pocket.
    #[serde(default)]
    pub clearance_tool: Option<Tool>,
}

/// A pocket and, when a clearance tool is given and fits, the clearance
/// tool's pass. The clearance runs first, then the main tool.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PocketOutput {
    /// The main tool's moves: the whole pocket, or with a clearance tool
    /// only the rest of it and the boundary.
    pub pocket: Calculated,
    /// The clearance tool's moves, cut before the main tool.
    pub clearance: Option<Calculated>,
}

/// How far, as a fraction of the main tool's radius, the rest area reaches
/// back into what the clearance tool cut, so the two tools' passes overlap.
const REST_OVERLAP: f64 = 0.5;

/// Calculates a pocket toolpath inside the closed vectors of `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for bad depths or stepover, when there
/// is no closed vector, and when the tool fits nowhere.
pub fn pocket(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &PocketParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    // The offsetting library can fault on closed vectors that cross or
    // overlap one another; that is said in plain words, never a crash.
    let tried = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let region = pocket_region(paths, tol)?;
        let plan = Plan::new(&region, tool, params, tol)?;
        let parts = ordered_parts(&plan.boundary, paths, params, job, tol);
        let chains: Vec<Vec<Chain>> = parts
            .iter()
            .map(|part| {
                clearing_chains(
                    part,
                    plan.stepover,
                    plan.radius,
                    params,
                    params.profile_pass,
                    tol,
                )
            })
            .collect();
        Ok(plan.emit_parts(job, params, &chains, tol))
    }));
    tried.unwrap_or_else(|_| Err(CoreError::validation(OVERLAPPING)))
}

/// What a pocket says when its closed vectors overlap in a way it cannot clear.
pub const OVERLAPPING: &str = "the pocket could not be worked out because some of the selected closed vectors overlap or cross each other; weld them into one shape, or make a pocket for each";

/// The separate pockets of a boundary, each outer loop with its islands,
/// in cutting order: the order their vectors were selected in, or nearest
/// next from home.
fn ordered_parts(
    boundary: &Region,
    paths: &[Path],
    params: &PocketParams,
    job: &JobSetup,
    tol: Tolerance,
) -> Vec<Region> {
    let parts = region_parts(boundary, tol);
    let first = |r: &Region| r.loops.first().map_or(Point::ORIGIN, |s| s.start);
    if params.selection_order {
        let mut keyed: Vec<(usize, Region)> = parts
            .into_iter()
            .map(|r| (nearest_path(paths, first(&r)), r))
            .collect();
        keyed.sort_by_key(|(i, _)| *i);
        keyed.into_iter().map(|(_, r)| r).collect()
    } else {
        let home = Heights::from_job(job).home.xy();
        order_nearest(parts, home, first, first)
    }
}

/// Index of the path with a node nearest `p`.
fn nearest_path(paths: &[Path], p: Point) -> usize {
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let d = path
                .subpaths
                .iter()
                .flat_map(|s| std::iter::once(s.start).chain(s.segments.iter().map(|g| g.end())))
                .map(|q| q.distance_squared(p))
                .fold(f64::MAX, f64::min);
            (i, d)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map_or(0, |(i, _)| i)
}

/// A pocket cut with an ordered list of tools (the Pocket form's tool
/// list).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PocketTools {
    /// The first tool's cut: the whole pocket, as far as it reaches.
    pub first: Calculated,
    /// Each later tool's cut, in list order: only what every tool before
    /// it could not reach, finished round its own outline. `None` when the
    /// tool has nothing left to cut.
    pub later: Vec<Option<Calculated>>,
}

/// What every tool of a tool list but the last leaves on the pocket's
/// walls for the last one to remove, as a fraction of the last tool's
/// diameter at the cut depth, on top of the pocket allowance.
pub const FINAL_TOOL_ALLOWANCE: f64 = 0.1;

/// Calculates a pocket with an ordered list of tools: `first` clears all
/// it can, then each tool of `later` clears as much as it can of what the
/// tools before it left, see [`rest_pocket`]. Tools are compared by their
/// cutting radius at the cut depth; what a tool left is measured against
/// the smallest tool before it. With more than one tool, every tool but
/// the last leaves [`FINAL_TOOL_ALLOWANCE`] on the walls, which the last
/// tool cuts away round the pocket's outline.
///
/// # Errors
/// Returns [`CoreError::Validation`] for everything [`pocket`] and
/// [`rest_pocket`] refuse.
pub fn pocket_tool_list(
    paths: &[Path],
    job: &JobSetup,
    first: &Tool,
    later: &[Tool],
    params: &PocketParams,
    tol: Tolerance,
) -> Result<PocketTools, CoreError> {
    let radius = |tool: &Tool| tool.effective_radius_at_depth(-params.cut_depth);
    let left = later
        .last()
        .map(|last| FINAL_TOOL_ALLOWANCE * 2.0 * radius(last))
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(0.0);
    let leaving = PocketParams {
        allowance: params.allowance + left,
        ..params.clone()
    };
    let done = pocket(paths, job, first, &leaving, tol)?;
    let region = pocket_region(paths, tol)?;
    let mut smallest = radius(first);
    let mut cuts = Vec::with_capacity(later.len());
    for (index, tool) in later.iter().enumerate() {
        let cut = if index + 1 == later.len() {
            rest_pocket_within(&region, smallest, left, job, tool, params, tol)?
        } else {
            rest_pocket(&region, smallest, job, tool, &leaving, tol)?
        }
        .filter(|c| c.moves.iter().any(|m| m.kind.is_cutting()));
        cuts.push(cut);
        let r = radius(tool);
        if r.is_finite() && r < smallest {
            smallest = r;
        }
    }
    Ok(PocketTools {
        first: done,
        later: cuts,
    })
}

/// Calculates a pocket with an optional larger clearance tool.
///
/// Without a clearance tool this is [`pocket`]. With one, the clearance
/// tool clears the pocket first, in the same offset or raster style and at
/// the same pass depths, with its own stepover (and its own pass depth when
/// the settings leave it to the tool). The main tool then clears only the
/// rest area, see [`rest_area`], and always finishes with the pocket's
/// boundary, which cleans out the corners. When the clearance tool fits
/// nowhere, `clearance` is `None` and the main tool cuts the whole pocket.
/// Tools are compared by their cutting radius at the cut depth.
///
/// # Errors
/// Returns [`CoreError::Validation`] for everything [`pocket`] refuses, and
/// when the clearance tool is not larger than the main tool.
pub fn pocket_with_clearance(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &TwoToolPocketParams,
    tol: Tolerance,
) -> Result<PocketOutput, CoreError> {
    let settings = &params.pocket;
    let Some(clear_tool) = &params.clearance_tool else {
        return pocket(paths, job, tool, settings, tol).map(|pocket| PocketOutput {
            pocket,
            clearance: None,
        });
    };
    let region = pocket_region(paths, tol)?;
    let plan = Plan::new(&region, tool, settings, tol)?;
    let clear_radius = clear_tool.effective_radius_at_depth(-settings.cut_depth);
    if clear_radius.is_nan() || clear_radius <= plan.radius {
        return Err(CoreError::validation(
            "the clearance tool must be larger than the pocket tool",
        ));
    }
    let clear_settings = PocketParams {
        stepover: None,
        ..settings.clone()
    };
    let clear_plan = match Plan::new(&region, clear_tool, &clear_settings, tol) {
        Ok(plan) => plan,
        Err(_)
            if region
                .offset(clear_radius + settings.allowance, tol)
                .is_empty() =>
        {
            let pocket = pocket(paths, job, tool, settings, tol)?;
            return Ok(PocketOutput {
                pocket,
                clearance: None,
            });
        }
        Err(error) => return Err(error),
    };
    let clear_chains: Vec<Vec<Chain>> =
        ordered_parts(&clear_plan.boundary, paths, settings, job, tol)
            .iter()
            .map(|part| {
                clearing_chains(
                    part,
                    clear_plan.stepover,
                    clear_plan.radius,
                    settings,
                    settings.profile_pass,
                    tol,
                )
            })
            .collect();
    let clearance = clear_plan.emit_parts(job, settings, &clear_chains, tol);

    let rest = rest_area(
        &region,
        clear_radius,
        settings.allowance,
        REST_OVERLAP * plan.radius,
        tol,
    )?;
    // Tool centres that reach rest material: inside the main tool's
    // boundary and within the rest area.
    let centres = Region::from_paths(
        &intersect_paths(&[plan.boundary.as_path()], &[rest.as_path()], tol)?,
        tol,
    );
    let mut chains = if centres.is_empty() {
        Vec::new()
    } else {
        clearing_chains(&centres, plan.stepover, plan.radius, settings, false, tol)
    };
    for sub in &plan.boundary.loops {
        chains.push(orient(settings, Chain::from_subpath(sub, tol)));
    }
    Ok(PocketOutput {
        pocket: plan.emit(job, settings, &chains, tol),
        clearance: Some(clearance),
    })
}

/// Clears only what a tool of radius `earlier_radius` (mm) left inside
/// `region`: the rest area (see [`rest_area`]) where `tool` fits, cleared
/// in the pocket's style and finished round its own outline, which runs
/// into the corners. `None` when the tool reaches no rest material.
///
/// # Errors
/// Returns [`CoreError::Validation`] for bad depths or stepover and when
/// the rest area cannot be computed.
pub fn rest_pocket(
    region: &Region,
    earlier_radius: f64,
    job: &JobSetup,
    tool: &Tool,
    params: &PocketParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    rest_pocket_within(region, earlier_radius, 0.0, job, tool, params, tol)
}

/// [`rest_pocket`] after earlier tools that also left `left` millimetres
/// on the walls beyond the pocket allowance: that band is rest material
/// too.
///
/// # Errors
/// As [`rest_pocket`].
pub fn rest_pocket_within(
    region: &Region,
    earlier_radius: f64,
    left: f64,
    job: &JobSetup,
    tool: &Tool,
    params: &PocketParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    let plan = match Plan::new(region, tool, params, tol) {
        Ok(plan) => plan,
        Err(_)
            if region
                .offset(tool.radius() + params.allowance, tol)
                .is_empty() =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    let rest = rest_area_within(
        region,
        earlier_radius,
        params.allowance,
        left,
        REST_OVERLAP * plan.radius,
        tol,
    )?;
    if rest.is_empty() {
        return Ok(None);
    }
    let centres = Region::from_paths(
        &intersect_paths(&[plan.boundary.as_path()], &[rest.as_path()], tol)?,
        tol,
    );
    if left > 0.0 {
        // The band left on the walls: clear what else is left, then run
        // round the whole outline, as a clearance tool's pocket finishes.
        let mut chains = if centres.is_empty() {
            Vec::new()
        } else {
            clearing_chains(&centres, plan.stepover, plan.radius, params, false, tol)
        };
        for sub in &plan.boundary.loops {
            chains.push(orient(params, Chain::from_subpath(sub, tol)));
        }
        return Ok(Some(plan.emit(job, params, &chains, tol)));
    }
    if centres.is_empty() {
        return Ok(None);
    }
    let chains = clearing_chains(&centres, plan.stepover, plan.radius, params, true, tol);
    if chains.is_empty() {
        return Ok(None);
    }
    Ok(Some(plan.emit(job, params, &chains, tol)))
}

/// The material a clearance tool of radius `clearance_radius` (mm) leaves
/// behind inside `region` shrunk by `allowance` (mm): the region less the
/// area the tool's edge sweeps when its centre runs everywhere it fits,
/// grown back `overlap` millimetres into the swept area so the next tool's
/// passes overlap the clearance tool's. Empty when the clearance tool
/// reaches everything; the whole region when it fits nowhere.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the area cannot be computed.
pub fn rest_area(
    region: &Region,
    clearance_radius: f64,
    allowance: f64,
    overlap: f64,
    tol: Tolerance,
) -> Result<Region, CoreError> {
    rest_area_within(region, clearance_radius, allowance, 0.0, overlap, tol)
}

/// [`rest_area`] when the clearance tool also stopped `left` millimetres
/// short of `region` shrunk by `allowance`: the band it left on the walls
/// is part of the rest.
///
/// # Errors
/// As [`rest_area`].
pub fn rest_area_within(
    region: &Region,
    clearance_radius: f64,
    allowance: f64,
    left: f64,
    overlap: f64,
    tol: Tolerance,
) -> Result<Region, CoreError> {
    let material = region.offset(allowance, tol);
    let centres = region.offset(clearance_radius + allowance + left, tol);
    if centres.is_empty() {
        return Ok(material);
    }
    let swept = centres.offset(-clearance_radius, tol).offset(overlap, tol);
    if swept.is_empty() {
        return Ok(material);
    }
    let left = subtract(&[material.as_path()], &[swept.as_path()], tol)?;
    Ok(Region::from_paths(&left, tol))
}

/// The pocket's area from the closed vectors of `paths`.
fn pocket_region(paths: &[Path], tol: Tolerance) -> Result<Region, CoreError> {
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation(
            "a pocket needs at least one closed vector",
        ));
    }
    Ok(region)
}

/// One tool's pocket: its pass depths, stepover and the region its centre
/// may reach, with the warnings found along the way.
struct Plan {
    depths: Vec<f64>,
    stepover: f64,
    radius: f64,
    boundary: Region,
    warnings: Vec<crate::warning::Warning>,
}

impl Plan {
    fn new(
        region: &Region,
        tool: &Tool,
        params: &PocketParams,
        tol: Tolerance,
    ) -> Result<Plan, CoreError> {
        let depths = planned_depths(
            params.start_depth,
            params.cut_depth,
            params.pass_depth.unwrap_or(tool.pass_depth),
            &PassPlan {
                count: params.pass_count,
                last_thickness: params.last_pass_thickness,
                exact: params.exact_pass_depth,
                list: params.pass_list.as_deref(),
            },
        )?;
        let stepover = params.stepover.unwrap_or(tool.stepover);
        if !stepover.is_finite() || stepover <= 0.0 {
            return Err(CoreError::validation("the stepover must be more than zero"));
        }
        let radius = tool.effective_radius_at_depth(-params.cut_depth);
        let boundary = region.offset(radius + params.allowance, tol);
        if boundary.is_empty() {
            return Err(CoreError::validation(
                "the tool is too big to fit inside the pocket",
            ));
        }
        let mut warnings = Vec::new();
        if stepover > 2.0 * radius {
            warnings.push(crate::warning::Warning::pocket_stepover_wide());
        }
        Ok(Plan {
            depths,
            stepover,
            radius,
            boundary,
            warnings,
        })
    }

    /// Cuts `chains` at every level, staying down for short links that stay
    /// inside the boundary.
    fn emit(
        &self,
        job: &JobSetup,
        params: &PocketParams,
        chains: &[Chain],
        tol: Tolerance,
    ) -> Calculated {
        self.emit_parts(job, params, &[chains.to_vec()], tol)
    }

    /// Cuts each part's chains at every level before moving to the next
    /// part, so that one pocket is finished before the next is started.
    fn emit_parts(
        &self,
        job: &JobSetup,
        params: &PocketParams,
        parts: &[Vec<Chain>],
        tol: Tolerance,
    ) -> Calculated {
        let heights = Heights::from_job(job);
        let mut out = Emitter::new(heights);
        for chains in parts {
            self.emit_levels(&mut out, params, chains, tol);
        }
        Calculated {
            moves: out.finish(),
            warnings: self.warnings.clone(),
        }
    }

    fn emit_levels(
        &self,
        out: &mut Emitter,
        params: &PocketParams,
        chains: &[Chain],
        tol: Tolerance,
    ) {
        let heights = out.heights();
        let link_limit = self.stepover * 2.0;
        // Raster lines and rings end on the boundary itself, where a point
        // test can fall either way; a hair of slack keeps those links down.
        let link_area = self.boundary.offset(-LINK_SLACK, tol);
        let mut above = heights.z_at(params.start_depth);
        for depth in &self.depths {
            let z = heights.z_at(*depth);
            let mut down = false;
            for chain in chains {
                let chain = chain.starting_near(out.at().xy());
                let Some(begin) = chain.start() else {
                    continue;
                };
                let here = out.at().xy();
                let stay_down = down
                    && here.distance(begin) <= link_limit
                    && link_inside(&link_area, here, begin, tol);
                if stay_down {
                    out.cut(
                        &CutSeg {
                            from: here,
                            to: begin,
                            bulge: 0.0,
                        },
                        z,
                    );
                } else {
                    out.travel_to(begin);
                    out.plunge_to(above);
                    if params.ramp.enabled {
                        out.cut_chain_ramp(&chain, z, &params.ramp);
                        down = true;
                        continue;
                    }
                    out.plunge_to(z);
                }
                out.cut_flat(&chain.segs, z);
                down = true;
            }
            out.retract();
            above = z;
        }
    }
}

/// A closed chain turned to the cut direction.
fn orient(params: &PocketParams, chain: Chain) -> Chain {
    if params.direction == CutDirection::Climb {
        chain
    } else {
        chain.reversed()
    }
}

/// The chains that clear `boundary` (the region the tool centre may reach),
/// cut the same at every level. Offset rings run from the middle outwards
/// and end with the boundary, their sharp corners cleaned up where the
/// stepover leaves material a tool of `radius` misses; raster lines end
/// with the boundary when `profile_pass` is set.
fn clearing_chains(
    boundary: &Region,
    stepover: f64,
    radius: f64,
    params: &PocketParams,
    profile_pass: bool,
    tol: Tolerance,
) -> Vec<Chain> {
    let mut chains: Vec<Chain> = Vec::new();
    match params.strategy {
        ClearStrategy::Offset => {
            let mut rings: Vec<Region> = Vec::new();
            let mut next = boundary.offset(stepover, tol);
            while !next.is_empty() && rings.len() < 10_000 {
                let inner = next.offset(stepover, tol);
                rings.push(next);
                next = inner;
            }
            // A stepover past the tool radius can leave the middle of the
            // innermost ring out of reach: rings a radius apart fill it.
            if radius > 0.0 && stepover > radius {
                let mut middle = rings.last().map(|ring| ring.offset(radius, tol));
                while let Some(ring) = middle.take().filter(|r| !r.is_empty()) {
                    if rings.len() >= 10_000 {
                        break;
                    }
                    middle = Some(ring.offset(radius, tol));
                    rings.push(ring);
                }
            }
            let mut loops: Vec<(Chain, &Region)> = Vec::new();
            for ring in rings.iter().rev().chain(std::iter::once(boundary)) {
                for sub in &ring.loops {
                    loops.push((Chain::from_subpath(sub, tol), ring));
                }
            }
            let cleaned = clean_corners(&loops, boundary, stepover, radius, tol);
            chains.extend(cleaned.into_iter().map(|chain| orient(params, chain)));
        }
        ClearStrategy::Raster => {
            let rings: Vec<Chain> = if profile_pass {
                boundary
                    .loops
                    .iter()
                    .map(|sub| orient(params, Chain::from_subpath(sub, tol)))
                    .collect()
            } else {
                Vec::new()
            };
            let lines = if params.strips {
                raster_strips(boundary, stepover, params.raster_angle, tol)
            } else {
                raster_lines(boundary, stepover, params.raster_angle, tol)
            };
            if params.profile_pass_when == ProfilePassWhen::First {
                chains.extend(rings);
                chains.extend(lines);
            } else {
                chains.extend(lines);
                chains.extend(rings);
            }
        }
    }
    chains
}

/// The direction a span leaves its start and arrives at its end.
fn tangents(seg: &CutSeg) -> Option<(Point, Point)> {
    let (dx, dy) = (seg.to.x - seg.from.x, seg.to.y - seg.from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let (cx, cy) = (dx / len, dy / len);
    // A bulge turns the start tangent clockwise and the end tangent
    // anticlockwise from the chord by a quarter of the included angle each
    // way (half the angle is 2 atan(bulge)).
    let half = 2.0 * seg.bulge.atan();
    let turn = |a: f64| Point::new(cx * a.cos() - cy * a.sin(), cx * a.sin() + cy * a.cos());
    Some((turn(-half), turn(half)))
}

/// Offset rings `loops` (each with the region it bounds) with a short cut
/// out and back added at each sharp corner that leaves material the rings
/// miss: when the stepover is more than about the tool radius, the gap
/// between one ring's corner and the next ring out is wider than the tool,
/// so the tool runs out along the corner's bisector as far as the uncut
/// material reaches (inside `boundary`, where its centre may go) and back.
fn clean_corners(
    loops: &[(Chain, &Region)],
    boundary: &Region,
    stepover: f64,
    radius: f64,
    tol: Tolerance,
) -> Vec<Chain> {
    let chains: Vec<Chain> = loops.iter().map(|(c, _)| c.clone()).collect();
    // With rings no more than about a radius apart no corner is left.
    if radius <= 0.0 || stepover <= radius * 1.2 {
        return chains;
    }
    // What the tool clears at most: its centre anywhere in the boundary.
    let reach = boundary.offset(-radius, tol);
    // The spans in buckets, to find those near a corner quickly.
    let bucket = (stepover * 2.0 + radius).max(1.0);
    let key = |p: Point| ((p.x / bucket).floor() as i64, (p.y / bucket).floor() as i64);
    let mut buckets: std::collections::BTreeMap<(i64, i64), Vec<(usize, usize)>> =
        std::collections::BTreeMap::new();
    for (c, chain) in chains.iter().enumerate() {
        for (k, seg) in chain.segs.iter().enumerate() {
            let (a, b) = (key(seg.from), key(seg.to));
            let mid = key(seg.point_at(seg.length() / 2.0));
            let (i0, i1) = (a.0.min(b.0).min(mid.0), a.0.max(b.0).max(mid.0));
            let (j0, j1) = (a.1.min(b.1).min(mid.1), a.1.max(b.1).max(mid.1));
            for i in i0..=i1 {
                for j in j0..=j1 {
                    buckets.entry((i, j)).or_default().push((c, k));
                }
            }
        }
    }
    let covered = |p: Point| {
        let (i, j) = key(p);
        (i - 1..=i + 1).any(|bi| {
            (j - 1..=j + 1).any(|bj| {
                buckets.get(&(bi, bj)).is_some_and(|list| {
                    list.iter()
                        .any(|&(c, k)| chains[c].segs[k].nearest(p).1 <= radius * 0.98)
                })
            })
        })
    };
    let step = (radius / 8.0).max(0.01);
    let mut out = Vec::with_capacity(chains.len());
    for (chain, (_, ring)) in chains.iter().zip(loops) {
        let n = chain.segs.len();
        if !chain.closed || n < 2 {
            out.push(chain.clone());
            continue;
        }
        // Which side the ring's region lies on: left or right of travel.
        let first = &chain.segs[0];
        let Some((t0, _)) = tangents(first) else {
            out.push(chain.clone());
            continue;
        };
        let mid = first.point_at(first.length() / 2.0);
        let probe = Point::new(mid.x - t0.y * 1e-2, mid.y + t0.x * 1e-2);
        let side = if ring.contains(probe, tol) { 1.0 } else { -1.0 };
        let mut segs: Vec<CutSeg> = Vec::with_capacity(n + 8);
        for k in 0..n {
            let seg = chain.segs[k];
            let prev = chain.segs[(k + n - 1) % n];
            if let (Some((_, a)), Some((b, _))) = (tangents(&prev), tangents(&seg)) {
                let turn = (a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y) * side;
                // A corner of the region sharper than 150 degrees.
                if turn > 30f64.to_radians() {
                    let v = seg.from;
                    // Away from the region: the right of travel when it lies
                    // on the left.
                    let (ox, oy) = ((a.y + b.y) * side, -(a.x + b.x) * side);
                    let len = (ox * ox + oy * oy).sqrt();
                    let u = Point::new(ox / len, oy / len);
                    let at = |t: f64| Point::new(v.x + u.x * t, v.y + u.y * t);
                    // The next ring out lies a stepover beyond each side, so
                    // its corner is this far along the bisector.
                    let half = ((std::f64::consts::PI - turn) / 2.0).sin().max(0.05);
                    let far = stepover / half + radius;
                    let mut uncut: Option<f64> = None;
                    let mut t = step;
                    while t <= far && reach.contains(at(t), tol) {
                        if !covered(at(t)) {
                            uncut = Some(t);
                        }
                        t += step;
                    }
                    if let Some(mut t) = uncut {
                        while t > step && !boundary.contains(at(t), tol) {
                            t -= step;
                        }
                        if t > step {
                            let tip = at(t);
                            segs.push(CutSeg {
                                from: v,
                                to: tip,
                                bulge: 0.0,
                            });
                            segs.push(CutSeg {
                                from: tip,
                                to: v,
                                bulge: 0.0,
                            });
                        }
                    }
                }
            }
            segs.push(seg);
        }
        out.push(Chain { segs, closed: true });
    }
    out
}

/// True when the straight move from `a` to `b` stays inside `region`,
/// checked every half millimetre.
/// How far outside the boundary a link between cuts may run and still
/// stay down, in millimetres.
const LINK_SLACK: f64 = 1e-3;

fn link_inside(region: &Region, a: Point, b: Point, tol: Tolerance) -> bool {
    let steps = (a.distance(b) / 0.5).ceil().max(1.0) as usize;
    (0..=steps).all(|i| region.contains(a.lerp(b, i as f64 / steps as f64), tol))
        || a.distance(b) < 1e-9
}

/// One hatch row: its height and its spans left to right, turned flat.
type RasterRow = (f64, Vec<(f64, f64)>);

/// The spans of hatch lines `stepover` apart at `angle_deg` inside the
/// region, worked out with the lines turned horizontal: each row's height and
/// its spans left to right, and the turn that puts them back.
fn raster_rows(
    region: &Region,
    stepover: f64,
    angle_deg: f64,
    tol: Tolerance,
) -> (Vec<RasterRow>, Mat3) {
    // Work with the lines horizontal: turn the region by -angle, hatch, and
    // turn the lines back.
    let angle = angle_deg.to_radians();
    let to_flat = Mat3::rotate(-angle);
    let back = Mat3::rotate(angle);
    let polygons: Vec<Vec<Point>> = region
        .loops
        .iter()
        .map(|sub| {
            flatten_subpath(sub, tol)
                .points
                .iter()
                .map(|p| to_flat.apply(*p))
                .collect()
        })
        .collect();
    let (mut min_y, mut max_y) = (f64::MAX, f64::MIN);
    for p in polygons.iter().flatten() {
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    let mut rows = Vec::new();
    if min_y > max_y {
        return (rows, back);
    }
    let mut y = min_y + stepover / 2.0;
    while y < max_y {
        let mut xs: Vec<f64> = Vec::new();
        for poly in &polygons {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let spans: Vec<(f64, f64)> = xs
            .chunks_exact(2)
            .map(|c| (c[0], c[1]))
            .filter(|(a, b)| (b - a).abs() >= 1e-9)
            .collect();
        rows.push((y, spans));
        y += stepover;
    }
    (rows, back)
}

fn line_chain(back: &Mat3, y: f64, from: f64, to: f64) -> Chain {
    Chain {
        segs: vec![CutSeg {
            from: back.apply(Point::new(from, y)),
            to: back.apply(Point::new(to, y)),
            bulge: 0.0,
        }],
        closed: false,
    }
}

/// Back and forth lines `stepover` apart at `angle_deg`, clipped to the
/// region, as open chains in cutting order.
pub(crate) fn raster_lines(
    region: &Region,
    stepover: f64,
    angle_deg: f64,
    tol: Tolerance,
) -> Vec<Chain> {
    let (rows, back) = raster_rows(region, stepover, angle_deg, tol);
    let mut lines = Vec::new();
    for (row, (y, mut spans)) in rows.into_iter().enumerate() {
        if row % 2 == 1 {
            spans.reverse();
        }
        for (x0, x1) in spans {
            let (from, to) = if row % 2 == 0 { (x0, x1) } else { (x1, x0) };
            lines.push(line_chain(&back, y, from, to));
        }
    }
    lines
}

/// The same hatch lines as [`raster_lines`], cut strip by strip: a strip
/// follows overlapping spans from row to row, back and forth, so a shape
/// with several lobes has each lobe cleared in one consistent run instead of
/// the tool crossing between lobes on every row.
pub(crate) fn raster_strips(
    region: &Region,
    stepover: f64,
    angle_deg: f64,
    tol: Tolerance,
) -> Vec<Chain> {
    let (rows, back) = raster_rows(region, stepover, angle_deg, tol);
    let mut used: Vec<Vec<bool>> = rows.iter().map(|(_, s)| vec![false; s.len()]).collect();
    let mut lines = Vec::new();
    for first_row in 0..rows.len() {
        while let Some(first) = used[first_row].iter().position(|u| !*u) {
            used[first_row][first] = true;
            let mut strip = vec![(first_row, first)];
            let mut row = first_row;
            let mut span = rows[row].1[first];
            while row + 1 < rows.len() {
                let next = rows[row + 1]
                    .1
                    .iter()
                    .enumerate()
                    .position(|(k, s)| !used[row + 1][k] && s.0.max(span.0) < s.1.min(span.1));
                let Some(k) = next else {
                    break;
                };
                row += 1;
                used[row][k] = true;
                span = rows[row].1[k];
                strip.push((row, k));
            }
            for (n, (r, k)) in strip.into_iter().enumerate() {
                let (y, spans) = &rows[r];
                let (x0, x1) = spans[k];
                let (from, to) = if n % 2 == 0 { (x0, x1) } else { (x1, x0) };
                lines.push(line_chain(&back, *y, from, to));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::rect;

    fn tol() -> Tolerance {
        Tolerance::new(1e-3).expect("tolerance")
    }

    fn tool() -> Tool {
        let mut tool = Tool {
            pass_depth: 3.0,
            ..Tool::default()
        };
        tool.set_stepover(2.4);
        tool
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
        rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect")
    }

    fn cutting(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind.is_cutting()).collect()
    }

    /// Rabbets, dados and grooves of a bookcase side laid on its sheet,
    /// some overlapping where they meet, as one pocket.
    const OVERLAPPING_JOINTS: &str = r#"[{"subpaths":[{"start":{"x":1815.675,"y":624.025000005},"segments":[{"kind":"line","to":{"x":8.325000000000045,"y":624.025000005}},{"kind":"line","to":{"x":8.325000000000045,"y":611.2500000050001}},{"kind":"line","to":{"x":1815.675,"y":611.2500000050001}},{"kind":"line","to":{"x":1815.675,"y":624.025000005}}],"closed":true}]},{"subpaths":[{"start":{"x":1815.675,"y":315.675},"segments":[{"kind":"line","to":{"x":8.325000000000045,"y":315.6750000000001}},{"kind":"line","to":{"x":8.325000000000045,"y":302.9000000000001}},{"kind":"line","to":{"x":1815.675,"y":302.9}},{"kind":"line","to":{"x":1815.675,"y":315.675}}],"closed":true}]},{"subpaths":[{"start":{"x":-0.6749999999999998,"y":625.0250000100001},"segments":[{"kind":"line","to":{"x":806.675,"y":625.0250000100001}},{"kind":"line","to":{"x":806.675,"y":637.8000000100001}},{"kind":"line","to":{"x":-0.6749999999999998,"y":637.8000000100001}},{"kind":"line","to":{"x":-0.6749999999999998,"y":625.0250000100001}}],"closed":true}]},{"subpaths":[{"start":{"x":8.325,"y":624.025000005},"segments":[{"kind":"line","to":{"x":8.325,"y":316.67500000500013}},{"kind":"line","to":{"x":30.1,"y":316.67500000500013}},{"kind":"line","to":{"x":30.1,"y":624.025000005}},{"kind":"line","to":{"x":8.325,"y":624.025000005}}],"closed":true}]},{"subpaths":[{"start":{"x":1815.675,"y":8.325},"segments":[{"kind":"line","to":{"x":1815.675,"y":315.675}},{"kind":"line","to":{"x":1793.9,"y":315.675}},{"kind":"line","to":{"x":1793.9,"y":8.325}},{"kind":"line","to":{"x":1815.675,"y":8.325}}],"closed":true}]}]"#;

    #[test]
    fn overlapping_closed_vectors_pocket_in_plain_words_never_a_panic() {
        let joints: Vec<Path> = serde_json::from_str(OVERLAPPING_JOINTS).expect("paths");
        let cases: Vec<Vec<Path>> = vec![
            joints,
            vec![square(0.0, 0.0, 40.0, 40.0), square(20.0, 20.0, 60.0, 60.0)],
            vec![square(0.0, 0.0, 40.0, 40.0), square(0.0, 0.0, 40.0, 40.0)],
        ];
        for (i, paths) in cases.iter().enumerate() {
            let tried = std::panic::catch_unwind(|| {
                pocket(
                    paths,
                    &JobSetup::default(),
                    &tool(),
                    &PocketParams::default(),
                    tol(),
                )
            });
            let Ok(done) = tried else {
                panic!("case {i} panicked");
            };
            if let Err(error) = done {
                assert!(error.to_string().contains(OVERLAPPING), "case {i}: {error}");
            }
        }
    }

    #[test]
    fn an_offset_pocket_stays_a_radius_inside_and_reaches_the_floor() {
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &PocketParams::default(),
            tol(),
        )
        .expect("pocket");
        let cuts = cutting(&done.moves);
        assert!(!cuts.is_empty());
        for m in &cuts {
            assert!(m.to.x >= 3.0 - 1e-6 && m.to.x <= 37.0 + 1e-6, "{m:?}");
            assert!(m.to.y >= 3.0 - 1e-6 && m.to.y <= 27.0 + 1e-6, "{m:?}");
            assert!(m.to.z >= -3.0 - 1e-9);
        }
        // The boundary ring is cut last.
        let last = cuts.last().expect("cuts");
        let on_edge = (last.to.x - 3.0).abs() < 1e-6
            || (last.to.x - 37.0).abs() < 1e-6
            || (last.to.y - 3.0).abs() < 1e-6
            || (last.to.y - 27.0).abs() < 1e-6;
        assert!(on_edge, "{last:?}");
    }

    #[test]
    fn a_negative_allowance_cuts_the_pocket_larger() {
        let params = PocketParams {
            allowance: -1.0,
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let cuts = cutting(&done.moves);
        let min_x = cuts.iter().map(|m| m.to.x).fold(f64::INFINITY, f64::min);
        let max_x = cuts
            .iter()
            .map(|m| m.to.x)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - 2.0).abs() < 1e-3, "{min_x}");
        assert!((max_x - 38.0).abs() < 1e-3, "{max_x}");
    }

    #[test]
    fn rings_link_without_lifting_inside_a_plain_pocket() {
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &PocketParams::default(),
            tol(),
        )
        .expect("pocket");
        let lifts = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Rapid && m.to.z > 0.0)
            .count();
        // One trip in and out: travel, lift after the level, and home.
        assert!(lifts <= 6, "lifts {lifts}");
    }

    #[test]
    fn an_island_is_left_standing() {
        let done = pocket(
            &[square(0.0, 0.0, 60.0, 60.0), square(25.0, 25.0, 35.0, 35.0)],
            &JobSetup::default(),
            &tool(),
            &PocketParams::default(),
            tol(),
        )
        .expect("pocket");
        for m in cutting(&done.moves) {
            let inside_island = m.to.x > 22.0 + 1e-6
                && m.to.x < 38.0 - 1e-6
                && m.to.y > 22.0 + 1e-6
                && m.to.y < 38.0 - 1e-6;
            assert!(!inside_island, "cut into the island: {m:?}");
        }
    }

    #[test]
    fn a_raster_pocket_cuts_lines_at_the_angle_then_the_boundary() {
        let params = PocketParams {
            strategy: ClearStrategy::Raster,
            raster_angle: 0.0,
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let flat_lines = done
            .moves
            .windows(2)
            .filter(|w| {
                w[1].kind == MoveKind::Feed
                    && (w[0].to.y - w[1].to.y).abs() < 1e-9
                    && (w[0].to.x - w[1].to.x).abs() > 20.0
            })
            .count();
        assert!(flat_lines >= 8, "raster lines {flat_lines}");
        for m in cutting(&done.moves) {
            assert!(m.to.x >= 3.0 - 1e-6 && m.to.x <= 37.0 + 1e-6);
        }
    }

    #[test]
    fn raster_lines_stay_down_between_rows_on_both_sides() {
        // Each row ends on the boundary; the step to the next row runs along
        // it on the right as on the left, so the tool never lifts between
        // rows of one level.
        for (last, ramp) in [(false, false), (true, false), (true, true)] {
            let params = PocketParams {
                strategy: ClearStrategy::Raster,
                raster_angle: 0.0,
                profile_pass: last,
                profile_pass_when: ProfilePassWhen::Last,
                cut_depth: 4.0,
                ramp: RampSpec {
                    enabled: ramp,
                    distance: 10.0,
                    ..RampSpec::default()
                },
                ..PocketParams::default()
            };
            let done = pocket(
                &[square(20.0, 20.0, 120.0, 80.0)],
                &JobSetup::default(),
                &tool(),
                &params,
                tol(),
            )
            .expect("pocket");
            // Two levels (3 mm passes to 4 mm), each entered once and left
            // once, with room for the boundary run.
            let levels = 4;
            let lifts = done
                .moves
                .windows(2)
                .filter(|w| w[0].to.z < -1e-9 && w[1].to.z > w[0].to.z + 1e-9)
                .count();
            assert!(
                lifts <= levels + 1,
                "{last} {ramp}: {lifts} lifts, {levels} levels"
            );
        }
    }

    #[test]
    fn strips_clear_each_lobe_of_a_u_shape_in_one_run() {
        // A U: two legs joined along the bottom. Plain raster lines cross
        // between the legs on every row; strips finish one leg first.
        let mut u = crate::geom::SubPath::open(Point::new(0.0, 0.0));
        for p in [
            (30.0, 0.0),
            (30.0, 30.0),
            (20.0, 30.0),
            (20.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ] {
            u.line_to(Point::new(p.0, p.1));
        }
        u.closed = true;
        let region = Region::from_paths(&[u.into_path()], tol());
        let plain = raster_lines(&region, 2.0, 0.0, tol());
        let strips = raster_strips(&region, 2.0, 0.0, tol());
        assert_eq!(plain.len(), strips.len());
        // Which leg a line lies in: left, right, or across the bottom.
        let leg = |c: &Chain| {
            let s = &c.segs[0];
            if s.from.x.max(s.to.x) <= 10.5 {
                'l'
            } else if s.from.x.min(s.to.x) >= 19.5 {
                'r'
            } else {
                'b'
            }
        };
        let crossings = |lines: &[Chain]| {
            let legs: Vec<char> = lines.iter().map(leg).filter(|l| *l != 'b').collect();
            legs.windows(2).filter(|w| w[0] != w[1]).count()
        };
        assert!(
            crossings(&strips) < crossings(&plain),
            "{} {}",
            crossings(&strips),
            crossings(&plain)
        );
        assert_eq!(crossings(&strips), 1);
    }

    #[test]
    fn a_raster_at_ninety_degrees_runs_along_y() {
        let params = PocketParams {
            strategy: ClearStrategy::Raster,
            raster_angle: 90.0,
            profile_pass: false,
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let feeds: Vec<&Move> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .collect();
        let vertical = done
            .moves
            .windows(2)
            .filter(|w| {
                w[1].kind == MoveKind::Feed
                    && (w[0].to.x - w[1].to.x).abs() < 1e-6
                    && (w[0].to.y - w[1].to.y).abs() > 15.0
            })
            .count();
        assert!(
            !feeds.is_empty() && vertical >= 8,
            "vertical lines {vertical}"
        );
    }

    #[test]
    fn two_levels_when_the_floor_is_deeper_than_a_pass() {
        let params = PocketParams {
            cut_depth: 5.0,
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 20.0, 20.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let levels: std::collections::BTreeSet<i64> = cutting(&done.moves)
            .iter()
            .map(|m| (m.to.z * 1000.0).round() as i64)
            .collect();
        assert!(
            levels.contains(&-2500) && levels.contains(&-5000),
            "{levels:?}"
        );
    }

    #[test]
    fn a_pocket_needs_a_closed_vector_and_room_for_the_tool() {
        let mut sub = crate::geom::SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 0.0));
        assert!(pocket(
            &[sub.into_path()],
            &JobSetup::default(),
            &tool(),
            &PocketParams::default(),
            tol()
        )
        .is_err());
        assert!(pocket(
            &[square(0.0, 0.0, 4.0, 4.0)],
            &JobSetup::default(),
            &tool(),
            &PocketParams::default(),
            tol()
        )
        .is_err());
    }
    fn end_mill(diameter: f64) -> Tool {
        let mut tool = Tool {
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
            pass_depth: 3.0,
            ..Tool::default()
        };
        tool.set_stepover(diameter * 0.4);
        tool
    }

    fn two_tools(clear: f64) -> TwoToolPocketParams {
        TwoToolPocketParams {
            pocket: PocketParams::default(),
            clearance_tool: Some(end_mill(clear)),
        }
    }

    #[test]
    fn the_rest_area_keeps_the_corners_but_not_the_middle() {
        let region = Region::from_paths(&[square(0.0, 0.0, 60.0, 60.0)], tol());
        let rest = rest_area(&region, 6.0, 0.0, 0.75, tol()).expect("rest");
        assert!(!rest.is_empty());
        for corner in [(0.5, 0.5), (59.5, 0.5), (0.5, 59.5), (59.5, 59.5)] {
            assert!(
                rest.contains(Point::new(corner.0, corner.1), tol()),
                "{corner:?}"
            );
        }
        for cleared in [(30.0, 30.0), (30.0, 5.0), (5.0, 30.0), (10.0, 10.0)] {
            assert!(
                !rest.contains(Point::new(cleared.0, cleared.1), tol()),
                "{cleared:?}"
            );
        }
        // Grown back past the clearance tool's reach, so the passes overlap.
        assert!(rest.contains(Point::new(30.0, 0.5), tol()));
    }

    #[test]
    fn two_tools_stay_inside_and_reach_the_floor() {
        let paths = [square(0.0, 0.0, 60.0, 60.0)];
        let job = JobSetup::default();
        let done = pocket_with_clearance(&paths, &job, &end_mill(3.0), &two_tools(12.0), tol())
            .expect("pocket");
        let clearance = done.clearance.expect("the clearance tool fits");
        let clear_cuts = cutting(&clearance.moves);
        assert!(!clear_cuts.is_empty());
        for m in &clear_cuts {
            assert!(m.to.x >= 6.0 - 1e-6 && m.to.x <= 54.0 + 1e-6, "{m:?}");
            assert!(m.to.y >= 6.0 - 1e-6 && m.to.y <= 54.0 + 1e-6, "{m:?}");
            assert!(m.to.z >= -3.0 - 1e-9);
        }
        let main_cuts = cutting(&done.pocket.moves);
        assert!(!main_cuts.is_empty());
        for m in &main_cuts {
            assert!(m.to.x >= 1.5 - 1e-6 && m.to.x <= 58.5 + 1e-6, "{m:?}");
            assert!(m.to.y >= 1.5 - 1e-6 && m.to.y <= 58.5 + 1e-6, "{m:?}");
            assert!(m.to.z >= -3.0 - 1e-9);
            // The middle was the clearance tool's.
            assert!(m.to.xy().distance(Point::new(30.0, 30.0)) > 15.0, "{m:?}");
        }
        let floor = |cuts: &[&Move]| cuts.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        assert!((floor(&clear_cuts) + 3.0).abs() < 1e-9);
        assert!((floor(&main_cuts) + 3.0).abs() < 1e-9);
        // The corners are cleaned out by the main tool.
        assert!(main_cuts
            .iter()
            .any(|m| m.to.xy().distance(Point::new(1.5, 1.5)) < 1e-6));
        // Far less work for the small tool than a full pocket.
        let full = pocket(
            &paths,
            &job,
            &end_mill(3.0),
            &PocketParams::default(),
            tol(),
        )
        .expect("pocket");
        assert!(main_cuts.len() * 2 < cutting(&full.moves).len());
    }

    #[test]
    fn a_big_clearance_tool_leaves_rest_passes_in_the_corners() {
        let paths = [square(0.0, 0.0, 60.0, 60.0)];
        let done = pocket_with_clearance(
            &paths,
            &JobSetup::default(),
            &end_mill(3.0),
            &two_tools(25.0),
            tol(),
        )
        .expect("pocket");
        assert!(done.clearance.is_some());
        // Something beyond the boundary ring: cuts off the boundary lines.
        let off_boundary = cutting(&done.pocket.moves).iter().any(|m| {
            m.to.x > 1.5 + 0.1 && m.to.x < 58.5 - 0.1 && m.to.y > 1.5 + 0.1 && m.to.y < 58.5 - 0.1
        });
        assert!(off_boundary);
    }

    #[test]
    fn the_main_tool_cuts_everything_when_the_clearance_tool_does_not_fit() {
        let paths = [square(0.0, 0.0, 10.0, 10.0)];
        let job = JobSetup::default();
        let done = pocket_with_clearance(&paths, &job, &end_mill(3.0), &two_tools(12.0), tol())
            .expect("pocket");
        assert!(done.clearance.is_none());
        let full = pocket(
            &paths,
            &job,
            &end_mill(3.0),
            &PocketParams::default(),
            tol(),
        )
        .expect("pocket");
        assert_eq!(done.pocket, full);
    }

    #[test]
    fn the_clearance_tool_must_be_larger() {
        let error = pocket_with_clearance(
            &[square(0.0, 0.0, 60.0, 60.0)],
            &JobSetup::default(),
            &end_mill(6.0),
            &two_tools(6.0),
            tol(),
        )
        .expect_err("not larger");
        assert!(matches!(error, CoreError::Validation(_)));
    }

    #[test]
    fn pocket_settings_without_a_clearance_tool_still_parse() {
        let old = r#"{"startDepth":0.5,"cutDepth":4.0,"strategy":"raster","rasterAngle":45.0}"#;
        let params: TwoToolPocketParams = serde_json::from_str(old).expect("parse");
        assert!(params.clearance_tool.is_none());
        assert_eq!(params.pocket.cut_depth, 4.0);
        assert_eq!(params.pocket.strategy, ClearStrategy::Raster);
        assert_eq!(params.pocket.direction, PocketParams::default().direction);
        let empty: TwoToolPocketParams = serde_json::from_str("{}").expect("parse");
        assert_eq!(empty, TwoToolPocketParams::default());

        let both = two_tools(12.0);
        let json = serde_json::to_string(&both).expect("json");
        assert!(json.contains("\"clearanceTool\"") && json.contains("\"cutDepth\""));
        let back: TwoToolPocketParams = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, both);
    }

    #[test]
    fn a_tool_list_leaves_an_allowance_on_the_walls_for_the_final_tool() {
        let paths = [square(0.0, 0.0, 60.0, 60.0)];
        let job = JobSetup::default();
        let params = PocketParams {
            allowance: 0.5,
            ..PocketParams::default()
        };
        let done = pocket_tool_list(
            &paths,
            &job,
            &end_mill(12.0),
            &[end_mill(3.0), end_mill(2.0)],
            &params,
            tol(),
        )
        .expect("pocket");
        // Every tool but the last stops a tenth of the last tool's
        // diameter (0.2 mm) short of the pocket allowance.
        let left = FINAL_TOOL_ALLOWANCE * 2.0;
        let near_wall = |moves: &[Move], radius: f64| {
            cutting(moves)
                .iter()
                .map(|m| m.to.x.min(m.to.y).min(60.0 - m.to.x).min(60.0 - m.to.y) - radius)
                .fold(f64::MAX, f64::min)
        };
        let first = near_wall(&done.first.moves, 6.0);
        assert!((first - (0.5 + left)).abs() < 1e-3, "{first}");
        let middle = done.later[0].as_ref().expect("the 3 mm tool cuts corners");
        let middle = near_wall(&middle.moves, 1.5);
        assert!((middle - (0.5 + left)).abs() < 1e-3, "{middle}");
        // The last tool cuts round the outline at the pocket allowance,
        // along every wall, not only in the corners.
        let last = done.later[1]
            .as_ref()
            .expect("the last tool has the walls to cut");
        assert!((near_wall(&last.moves, 1.0) - 0.5).abs() < 1e-3);
        let along_bottom = last.moves.windows(2).any(|pair| {
            let (a, b) = (&pair[0], &pair[1]);
            b.kind.is_cutting()
                && (a.to.y - 1.5).abs() < 1e-3
                && (b.to.y - 1.5).abs() < 1e-3
                && (a.to.x - b.to.x).abs() > 50.0
        });
        assert!(along_bottom, "the last tool runs the length of a wall");
        // One tool alone keeps no extra allowance.
        let alone =
            pocket_tool_list(&paths, &job, &end_mill(12.0), &[], &params, tol()).expect("pocket");
        assert!((near_wall(&alone.first.moves, 6.0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_tool_list_clears_with_the_first_and_leaves_the_rest_to_the_later_tools() {
        let paths = [square(0.0, 0.0, 60.0, 60.0)];
        let job = JobSetup::default();
        let params = PocketParams::default();
        let done = pocket_tool_list(
            &paths,
            &job,
            &end_mill(12.0),
            &[end_mill(3.0), end_mill(12.0), end_mill(1.0)],
            &params,
            tol(),
        )
        .expect("pocket");
        // The large tool clears the whole pocket a radius in.
        let first = cutting(&done.first.moves);
        assert!(first.iter().any(|m| (m.to.x - 30.0).abs() < 7.0));
        for m in &first {
            assert!(m.to.x >= 6.0 - 1e-6 && m.to.x <= 54.0 + 1e-6, "{m:?}");
        }
        assert_eq!(done.later.len(), 3);
        // The 3 mm tool only cleans the corners the 12 mm tool left.
        let small = done.later[0]
            .as_ref()
            .expect("the small tool has corners to cut");
        let small_cuts = cutting(&small.moves);
        assert!(!small_cuts.is_empty());
        assert!(small_cuts
            .iter()
            .all(|m| !(m.to.x > 20.0 && m.to.x < 40.0 && m.to.y > 20.0 && m.to.y < 40.0)));
        for m in &small_cuts {
            assert!(m.to.x >= 1.5 - 1e-6 && m.to.x <= 58.5 + 1e-6, "{m:?}");
        }
        // A tool no smaller than one before it finds nothing left.
        assert!(done.later[1].is_none());
        // The 1 mm tool reaches further into the corners than the 3 mm one.
        let tiny = done.later[2]
            .as_ref()
            .expect("the 1 mm tool has corners left");
        let reach = |moves: &[Move]| {
            cutting(moves)
                .iter()
                .map(|m| m.to.x.min(m.to.y))
                .fold(f64::MAX, f64::min)
        };
        assert!(reach(&tiny.moves) < reach(&small.moves));
    }

    #[test]
    fn each_pocket_is_finished_before_the_next_is_started() {
        let params = PocketParams {
            cut_depth: 6.0,
            ..PocketParams::default()
        };
        let done = pocket(
            &[
                square(0.0, 0.0, 30.0, 30.0),
                square(100.0, 0.0, 130.0, 30.0),
            ],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        // Plunges: both levels of the first pocket, then both of the second.
        let plunges: Vec<(f64, f64)> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| (m.to.x, m.to.z))
            .collect();
        let switch = plunges
            .iter()
            .position(|p| p.0 > 50.0)
            .expect("the second pocket");
        assert!(plunges[..switch].iter().all(|p| p.0 < 50.0), "{plunges:?}");
        assert!(plunges[switch..].iter().all(|p| p.0 > 50.0), "{plunges:?}");
        assert!((plunges[switch - 1].1 + 6.0).abs() < 1e-9, "{plunges:?}");
    }

    #[test]
    fn selection_order_follows_the_vectors_order() {
        let params = PocketParams {
            selection_order: true,
            ..PocketParams::default()
        };
        let done = pocket(
            &[
                square(100.0, 0.0, 130.0, 30.0),
                square(0.0, 0.0, 30.0, 30.0),
            ],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let first = done
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .expect("a plunge");
        assert!(first.to.x > 50.0, "{first:?}");
    }

    #[test]
    fn the_profile_pass_can_come_first() {
        let params = PocketParams {
            strategy: ClearStrategy::Raster,
            profile_pass_when: ProfilePassWhen::First,
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let cuts = cutting(&done.moves);
        // The first four cuts run round the boundary, 3 mm in.
        let on_edge = |m: &&Move| {
            (m.to.x - 3.0).abs() < 1e-6
                || (m.to.x - 37.0).abs() < 1e-6
                || (m.to.y - 3.0).abs() < 1e-6
                || (m.to.y - 27.0).abs() < 1e-6
        };
        assert!(cuts.iter().take(4).all(on_edge), "{:?}", &cuts[..4]);
        let last = cuts.last().expect("cuts");
        assert!(!((last.to.y - 3.0).abs() < 1e-6 && (last.to.x - 3.0).abs() < 1e-6));
    }

    #[test]
    fn pass_count_spreads_the_levels() {
        let params = PocketParams {
            cut_depth: 6.0,
            pass_count: Some(3),
            ..PocketParams::default()
        };
        let done = pocket(
            &[square(0.0, 0.0, 40.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("pocket");
        let mut levels: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| m.to.z)
            .collect();
        levels.dedup();
        assert_eq!(levels, vec![-2.0, -4.0, -6.0]);
    }

    #[test]
    fn old_pocket_settings_parse_with_the_new_defaults() {
        let params: PocketParams =
            serde_json::from_value(serde_json::json!({ "cutDepth": 4.0, "profilePass": false }))
                .expect("parse");
        assert_eq!(params.profile_pass_when, ProfilePassWhen::Last);
        assert!(!params.selection_order);
        assert_eq!(params.pass_count, None);
    }

    #[test]
    fn a_form_request_with_pass_options_parses() {
        let json = serde_json::json!({"allowance":0,"clearanceTool":null,"cutDepth":6,"direction":"climb","lastPassThickness":null,"passCount":2,"passDepth":null,"profilePass":true,"profilePassWhen":"first","ramp":{"distance":20,"enabled":false},"rasterAngle":0,"selectionOrder":false,"startDepth":0,"stepover":null,"strategy":"raster"});
        let params: TwoToolPocketParams = serde_json::from_value(json).expect("parse");
        assert_eq!(params.pocket.pass_count, Some(2));
        assert_eq!(params.pocket.profile_pass_when, ProfilePassWhen::First);
    }
    #[test]
    fn a_stepover_wider_than_the_tool_warns_of_ridges_and_a_narrower_one_does_not() {
        let warns = |stepover: f64| {
            pocket(
                &[square(0.0, 0.0, 40.0, 30.0)],
                &JobSetup::default(),
                &tool(),
                &PocketParams {
                    stepover: Some(stepover),
                    ..PocketParams::default()
                },
                tol(),
            )
            .expect("pocket")
            .warnings
            .iter()
            .any(|w| w.code == "pocket.stepoverWide")
        };
        // The tool is 6 mm across: rings 6 apart still meet, 6.1 leave a ridge.
        assert!(!warns(5.9));
        assert!(!warns(6.0));
        assert!(warns(6.1));
    }
}
