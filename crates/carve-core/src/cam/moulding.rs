//! Moulding toolpaths: a cross section profile swept along a rail.
//!
//! The first vector given is the rail, open or closed. The second is the
//! profile, drawn flat in the XY plane: its X is the distance across the
//! moulding and its Y is the height of the moulding there. The profile's
//! leftmost point lies on the rail and the moulding extends to the left of
//! the rail's direction of travel (inside a counter clockwise closed rail).
//! The profile's highest point is at the material surface.
//!
//! Along the rail, each pass runs parallel to the rail at one offset across
//! the moulding. Its depth comes from a drop cutter on the profile: at
//! offset `x` the tip sits at `max(profile(x + u) - tool(u))` over the
//! tool's footprint `u`, where `tool(u)` is the height of the cutter's side
//! above its tip at distance `|u|` from its axis. So a ball nose rolls over
//! the section instead of its tip following it, and never gouges a hollow
//! narrower than the ball. Outside the profile's width nothing holds the
//! tool up. The section is compensated across the rail only, which is exact
//! where the rail is straight.
//!
//! Across the rail, passes run from the rail out across the moulding,
//! stepping along the rail by the stepover and going back and forth. The
//! swept surface is mitred: each rail span owns the part of the moulding
//! between the bisectors of its corners, open rail ends carry on straight,
//! and the height at a point is the profile at its distance from the span
//! that owns it. Passes stop at the mitre of an inside corner, where one
//! more pass runs up the mitre itself, and fan round an outside corner. The
//! tip at each point of a pass is dropped onto that surface over the whole
//! footprint, neighbouring spans included, so it never gouges a corner.
//!
//! This is a finishing cut: each pass goes to its full depth in one go, so
//! deep mouldings want roughing first.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{flatten_subpath, parallel_offset_open, Path, Point, SubPath, Tolerance, Vec2};

use super::chain::Chain;
use super::common::{Emitter, Heights};
use super::profile::Calculated;
use super::surface::cut_run;
use super::tool::Tool;
use super::toolpath::Point3;

/// Most grid cells the profile is sampled into across its width.
const MAX_CELLS: f64 = 200_000.0;

/// Number of samples of the tool's side, axis to full radius.
const TOOL_SAMPLES: usize = 1025;

/// Which way the passes run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouldingStrategy {
    /// Passes parallel to the rail, stepping across the profile.
    #[default]
    Along,
    /// Passes across the profile, stepping along the rail.
    Across,
}

/// The Moulding form's settings. Lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MouldingParams {
    /// Distance between neighbouring passes across the moulding. Absent
    /// means the tool's stepover.
    pub stepover: Option<f64>,
    /// Factor the profile's width is scaled by.
    pub width_scale: f64,
    /// Factor the profile's height is scaled by.
    pub height_scale: f64,
    /// Material left on the moulding, in millimetres; negative cuts below
    /// the profile.
    pub allowance: f64,
    /// Which way the passes run.
    pub strategy: MouldingStrategy,
    /// How far below the material surface the profile's top sits, in
    /// millimetres (the moulding's position in the material).
    pub start_depth: f64,
    /// How far the passes carry on past each side of the profile at its
    /// lowest height, in millimetres, so vertical edges are cleaned.
    pub boundary_offset: f64,
    /// Along: space the passes by the distance over the profile's surface
    /// rather than straight across, so steep parts get more of them.
    pub vary_stepover: bool,
    /// Along: meet at sharp mitred corners where the rail turns, instead of
    /// rolling round them.
    pub sharp_corners: bool,
    /// A larger tool that roughs the moulding out level by level first,
    /// leaving `clearance_allowance` for the finishing tool.
    pub clearance_tool: Option<Tool>,
    /// Material the clearance tool leaves on the moulding, in millimetres.
    pub clearance_allowance: f64,
    /// Ramp the clearance tool into each level instead of plunging.
    pub clearance_ramp: super::common::RampSpec,
    /// The clearance tool also cuts the profile's flat stretches wide
    /// enough for it, to their exact height (no clearance allowance there).
    pub machine_flat_regions: bool,
    /// With `machine_flat_regions`, the finishing tool leaves out the passes
    /// that would only run over those flats again.
    pub skip_flat_regions: bool,
    /// Carry the passes past the profile's sides by the finishing tool's
    /// radius and a tenth, so steep or vertical ends are cut cleanly,
    /// instead of by `boundary_offset`.
    pub auto_boundary_offset: bool,
}

impl Default for MouldingParams {
    fn default() -> Self {
        MouldingParams {
            stepover: None,
            width_scale: 1.0,
            height_scale: 1.0,
            allowance: 0.0,
            strategy: MouldingStrategy::Along,
            start_depth: 0.0,
            boundary_offset: 0.0,
            vary_stepover: false,
            sharp_corners: false,
            clearance_tool: None,
            clearance_allowance: 0.5,
            clearance_ramp: super::common::RampSpec::default(),
            machine_flat_regions: false,
            skip_flat_regions: false,
            auto_boundary_offset: false,
        }
    }
}

/// Height differences under this count as flat, in millimetres.
const FLAT_TOL: f64 = 1e-3;

/// How far past the profile's sides the automatic boundary offset carries
/// the passes, as a multiple of the finishing tool's radius.
const AUTO_OFFSET: f64 = 1.1;

/// The profile's upper envelope sampled every `step` millimetres from 0 at
/// its leftmost point, heights measured from its lowest point, both scaled.
struct Section {
    step: f64,
    heights: Vec<f64>,
    top: f64,
}

impl Section {
    fn from_subpath(
        sub: &SubPath,
        width_scale: f64,
        height_scale: f64,
        tol: Tolerance,
    ) -> Result<Section, CoreError> {
        let line = flatten_subpath(sub, tol);
        let mut points = line.points.clone();
        if line.closed {
            if let Some(&first) = points.first() {
                points.push(first);
            }
        }
        let (mut x0, mut x1, mut y0) = (f64::MAX, f64::MIN, f64::MAX);
        for p in &points {
            x0 = x0.min(p.x);
            x1 = x1.max(p.x);
            y0 = y0.min(p.y);
        }
        let width = (x1 - x0) * width_scale;
        if points.len() < 2 || !(width.is_finite() && width > 1e-9) {
            return Err(CoreError::validation(
                "the molding profile needs some width",
            ));
        }
        let scaled: Vec<Point> = points
            .iter()
            .map(|p| Point::new((p.x - x0) * width_scale, (p.y - y0) * height_scale))
            .collect();
        let step = tol.get().clamp(1e-4, 0.05).max(width / MAX_CELLS);
        let cells = (width / step).ceil() as usize;
        let step = width / cells as f64;
        let mut heights = vec![f64::NEG_INFINITY; cells + 1];
        for pair in scaled.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (lo, hi) = if a.x <= b.x { (a, b) } else { (b, a) };
            let first = ((lo.x / step) - 1e-9).ceil().max(0.0) as usize;
            let last = (((hi.x / step) + 1e-9).floor() as usize).min(cells);
            for (i, h) in heights.iter_mut().enumerate().take(last + 1).skip(first) {
                let x = i as f64 * step;
                let span = hi.x - lo.x;
                let y = if span > 1e-12 {
                    lo.y + (hi.y - lo.y) * ((x - lo.x) / span).clamp(0.0, 1.0)
                } else {
                    lo.y.max(hi.y)
                };
                *h = h.max(y);
            }
        }
        let top = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Ok(Section { step, heights, top })
    }

    fn width(&self) -> f64 {
        self.step * (self.heights.len() - 1) as f64
    }

    /// The level stretches of the profile at least `min_width` wide, as
    /// (from, to, height) across it, left to right.
    fn flats(&self, min_width: f64) -> Vec<(f64, f64, f64)> {
        let mut out = Vec::new();
        let mut start = 0usize;
        let n = self.heights.len();
        for i in 1..=n {
            let level = i < n
                && self.heights[i].is_finite()
                && self.heights[start].is_finite()
                && (self.heights[i] - self.heights[start]).abs() <= FLAT_TOL;
            if level {
                continue;
            }
            let (from, to) = (start as f64 * self.step, (i - 1) as f64 * self.step);
            if self.heights[start].is_finite() && to - from >= min_width - 1e-9 {
                out.push((from, to, self.heights[start]));
            }
            start = i;
        }
        out
    }

    /// Height of the profile at `x` across it, from its lowest point;
    /// minus infinity outside it.
    fn height_at(&self, x: f64) -> f64 {
        let at = x / self.step;
        let last = self.heights.len() - 1;
        if !(at >= -1e-9 && at <= last as f64 + 1e-9) {
            return f64::NEG_INFINITY;
        }
        let k = (at.floor().max(0.0) as usize).min(last.saturating_sub(1));
        let (a, b) = (self.heights[k], self.heights[(k + 1).min(last)]);
        if a.is_finite() && b.is_finite() {
            a + (b - a) * (at - k as f64).clamp(0.0, 1.0)
        } else {
            a.max(b)
        }
    }

    /// Offsets across the moulding for Along passes a stepover apart over
    /// the profile's surface: the tool tip curve is walked and a pass put
    /// down every `stepover` of its length, both edges included.
    fn surface_offsets(&self, side: &Side, from: f64, to: f64, stepover: f64) -> Vec<f64> {
        let fine = (self.step * 4.0).max((to - from) / 20_000.0).max(1e-4);
        let count = ((to - from) / fine).ceil().max(1.0) as usize;
        let tip = |x: f64| {
            let t = self.tip_at(x, side);
            if t.is_finite() {
                t.max(0.0)
            } else {
                0.0
            }
        };
        let mut out = vec![from];
        let mut walked = 0.0;
        let mut last = (from, tip(from));
        for k in 1..=count {
            let x = from + (to - from) * k as f64 / count as f64;
            let here = (x, tip(x));
            walked += (here.0 - last.0).hypot(here.1 - last.1);
            last = here;
            if walked >= stepover - 1e-9 && k < count {
                out.push(x);
                walked = 0.0;
            }
        }
        if out.last().is_none_or(|x| (to - x).abs() > 1e-9) {
            out.push(to);
        }
        out
    }

    /// Height of the tool tip at `x` across the section.
    fn tip_at(&self, x: f64, side: &Side) -> f64 {
        let radius = side.radius;
        let last = self.heights.len() - 1;
        let from = ((x - radius) / self.step).ceil().max(0.0) as usize;
        let to = (((x + radius) / self.step).floor().max(0.0) as usize).min(last);
        let mut best = f64::NEG_INFINITY;
        for (j, &h) in self.heights.iter().enumerate().take(to + 1).skip(from) {
            let u = (j as f64 * self.step - x).abs();
            if u > radius + 1e-9 {
                continue;
            }
            best = best.max(h - side.lift(u));
        }
        best
    }
}

/// The cutter's side: its height above the tip sampled evenly from its
/// axis out to `radius` millimetres.
struct Side {
    radius: f64,
    heights: Vec<f64>,
}

impl Side {
    fn of(tool: &Tool) -> Result<Side, CoreError> {
        let heights = tool
            .profile(TOOL_SAMPLES)?
            .into_iter()
            .map(|(_, h)| h)
            .collect();
        Ok(Side {
            radius: tool.radius(),
            heights,
        })
    }

    /// Height of the side above the tip `u` millimetres from the axis.
    fn lift(&self, u: f64) -> f64 {
        let side = &self.heights;
        let spans = (side.len() - 1) as f64;
        let at = (u / self.radius * spans).min(spans);
        let k = (at.floor() as usize).min(side.len() - 2);
        let t = at - k as f64;
        side[k] + (side[k + 1] - side[k]) * t
    }
}

fn positive(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "the {what} must be more than zero"
        )))
    }
}

/// Offsets across the moulding the passes run at, evenly spread from 0 to
/// `width` millimetres, no more than `stepover` apart.
fn pass_offsets(width: f64, stepover: f64) -> Vec<f64> {
    let spans = ((width / stepover) - 1e-9).ceil().max(1.0) as usize;
    (0..=spans)
        .map(|i| width * i as f64 / spans as f64)
        .collect()
}

/// Calculates a moulding toolpath: the last vector is the profile and every
/// vector before it a rail (see the module documentation for how the
/// profile is read). The rails are machined one after another.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the rail or the profile is
/// missing, the rail has no length, the profile has no width, a scale or
/// the stepover is not more than zero, the allowance is not a number, or
/// the tool has no radius.
pub fn moulding(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &MouldingParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    moulding_skipping(paths, job, tool, params, &[], tol)
}

/// As [`moulding`], leaving out the Along passes whose offset across the
/// profile falls in one of the `skip` bands (from, to).
fn moulding_skipping(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &MouldingParams,
    skip: &[(f64, f64)],
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (profile, rails) = match paths.split_last() {
        Some((profile, rails)) if !rails.is_empty() => (profile, rails),
        _ => {
            return Err(CoreError::validation(if paths.is_empty() {
                "select a rail vector for the molding"
            } else {
                "select a profile vector for the molding"
            }))
        }
    };
    let profile = profile
        .subpaths
        .first()
        .filter(|s| !s.segments.is_empty())
        .ok_or_else(|| CoreError::validation("select a profile vector for the molding"))?;
    let rails: Vec<&SubPath> = rails
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter(|s| !s.segments.is_empty())
        .collect();
    if rails.is_empty() {
        return Err(CoreError::validation(
            "select a rail vector for the molding",
        ));
    }
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !(params.boundary_offset.is_finite() && params.boundary_offset >= 0.0) {
        return Err(CoreError::validation(
            "the boundary offset must be zero or more",
        ));
    }
    positive(params.width_scale, "width scale")?;
    positive(params.height_scale, "height scale")?;
    if !params.allowance.is_finite() {
        return Err(CoreError::validation("the allowance must be a number"));
    }
    let stepover = params.stepover.unwrap_or(tool.stepover);
    positive(stepover, "stepover")?;
    positive(tool.radius(), "tool radius")?;

    let section = Section::from_subpath(profile, params.width_scale, params.height_scale, tol)?;
    let side = Side::of(tool)?;
    let heights = Heights::from_job(job);
    let mut out = Emitter::new(heights);
    let mut warnings = Vec::new();
    let effective;
    let params = if params.auto_boundary_offset {
        effective = MouldingParams {
            boundary_offset: side.radius * AUTO_OFFSET,
            ..params.clone()
        };
        &effective
    } else {
        params
    };
    for rail in rails {
        match params.strategy {
            MouldingStrategy::Along => {
                let offsets = along_offsets(&section, &side, stepover, params)
                    .into_iter()
                    .filter(|x| !skip.iter().any(|(a, b)| *x > a + 1e-9 && *x < b - 1e-9))
                    .collect();
                along(
                    rail,
                    &section,
                    &side,
                    &mut out,
                    offsets,
                    params.allowance,
                    params,
                    &mut warnings,
                    tol,
                )?;
            }
            MouldingStrategy::Across => {
                let rail = Rail::new(rail, section.width(), side.radius, stepover, tol)?;
                across(
                    &rail,
                    &section,
                    &side,
                    &mut out,
                    stepover,
                    params,
                    &mut warnings,
                    tol,
                );
            }
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// A moulding and, with a clearance tool, the roughing that tool cuts
/// first: Z level raster clearing of the swept moulding, leaving the
/// clearance allowance, with each level's outline profiled.
///
/// # Errors
/// As [`moulding`], and what the roughing refuses.
pub fn moulding_with_clearance(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &MouldingParams,
    tol: Tolerance,
) -> Result<(Calculated, Option<Calculated>), CoreError> {
    let finish = moulding(paths, job, tool, params, tol)?;
    let Some(clear_tool) = &params.clearance_tool else {
        return Ok((finish, None));
    };
    clear_tool.validate()?;
    let Some((profile, rails)) = paths.split_last() else {
        return Ok((finish, None));
    };
    let Some(profile) = profile.subpaths.first() else {
        return Ok((finish, None));
    };
    let section = Section::from_subpath(profile, params.width_scale, params.height_scale, tol)?;
    let (field, footprint) = swept_field(rails, &section, job, tol)?;
    let surface = super::surface::Surface::from_composite(&field, job).lowered(params.start_depth);
    // The roughing keeps to the moulding, its tool reaching one radius past
    // it so the edges come down to the base.
    let rough_params = super::surface::RoughParams {
        allowance: params.clearance_allowance + params.allowance,
        ramp: params.clearance_ramp,
        profile: Some(super::surface::RoughProfile::Last),
        boundary: super::surface::Boundary::Selection,
        boundary_offset: clear_tool.radius(),
        ..super::surface::RoughParams::default()
    };
    let mut rough = super::surface::rough(
        &surface,
        job,
        clear_tool,
        &rough_params,
        Some(&footprint),
        tol,
    )?;
    if !params.machine_flat_regions {
        return Ok((finish, Some(rough)));
    }
    // The flats the clearance tool fits on are cut to their own height.
    let clear_side = Side::of(clear_tool)?;
    let flats = section.flats(2.0 * clear_side.radius);
    let heights = Heights::from_job(job);
    let mut out = Emitter::new(heights);
    let stepover = clear_tool.stepover;
    positive(stepover, "clearance tool stepover")?;
    for (from, to, _) in &flats {
        let (a, b) = (from + clear_side.radius, to - clear_side.radius);
        let offsets: Vec<f64> = if b - a <= 1e-9 {
            vec![(a + b) / 2.0]
        } else {
            pass_offsets(b - a, stepover)
                .into_iter()
                .map(|x| x + a)
                .collect()
        };
        for sub in rails.iter().flat_map(|p| p.subpaths.iter()) {
            if sub.segments.is_empty() {
                continue;
            }
            along(
                sub,
                &section,
                &clear_side,
                &mut out,
                offsets.clone(),
                0.0,
                params,
                &mut rough.warnings,
                tol,
            )?;
        }
    }
    rough.moves.extend(out.finish());
    if !params.skip_flat_regions {
        return Ok((finish, Some(rough)));
    }
    // Finishing passes that would only touch a cut flat are left out.
    let radius = tool.radius();
    let skip: Vec<(f64, f64)> = flats
        .iter()
        .map(|(from, to, _)| (from + radius, to - radius))
        .filter(|(a, b)| b > a)
        .collect();
    let finish = moulding_skipping(paths, job, tool, params, &skip, tol)?;
    Ok((finish, Some(rough)))
}

/// The swept moulding as a height grid over the job's model grid: each
/// rail span's share of the moulding holds the profile's height at the
/// distance across it, the highest where rails overlap. Also answers the
/// area the moulding covers.
fn swept_field(
    rails: &[Path],
    section: &Section,
    job: &JobSetup,
    tol: Tolerance,
) -> Result<(crate::model::field::Field, crate::geom::Region), CoreError> {
    let mut shares: Vec<Path> = Vec::new();
    let grid = crate::model::grid_for(job);
    let mut field =
        crate::model::field::Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    let width = section.width();
    for sub in rails.iter().flat_map(|p| p.subpaths.iter()) {
        if sub.segments.is_empty() {
            continue;
        }
        let rail = Rail::new(sub, width, 0.0, grid.cell, tol)?;
        for span in &rail.spans {
            let corners = [
                span.at(span.start, 0.0),
                span.at(span.end, 0.0),
                span.at(span.end + span.end_slope * width, width),
                span.at(span.start + span.start_slope * width, width),
            ];
            let mut quad = SubPath::open(corners[0]);
            for &corner in &corners[1..] {
                quad.line_to(corner);
            }
            quad.close_with_line(tol);
            shares.push(quad.into_path());
            let i0 = (((span.lo.x - grid.origin.x) / grid.cell).floor().max(0.0)) as usize;
            let j0 = (((span.lo.y - grid.origin.y) / grid.cell).floor().max(0.0)) as usize;
            let i1 = (((span.hi.x - grid.origin.x) / grid.cell).ceil().max(0.0) as usize)
                .min(grid.width.saturating_sub(1));
            let j1 = (((span.hi.y - grid.origin.y) / grid.cell).ceil().max(0.0) as usize)
                .min(grid.height.saturating_sub(1));
            for j in j0..=j1.max(j0) {
                for i in i0..=i1.max(i0) {
                    if i >= grid.width || j >= grid.height {
                        continue;
                    }
                    let p = Point::new(
                        grid.origin.x + i as f64 * grid.cell,
                        grid.origin.y + j as f64 * grid.cell,
                    );
                    let (v, d) = span.frame(p);
                    if !(0.0..=width).contains(&d) {
                        continue;
                    }
                    let (a, b) = span.inside_mitres(v, d);
                    if a < 0.0 || b < 0.0 {
                        continue;
                    }
                    let h = section.height_at(d);
                    if !h.is_finite() {
                        continue;
                    }
                    let cell = &mut field.z[j * grid.width + i];
                    if cell.is_nan() || f64::from(*cell) < h {
                        *cell = h as f32;
                    }
                }
            }
        }
    }
    if field.z.iter().all(|z| z.is_nan()) {
        return Err(CoreError::validation(
            "the molding lies outside the material, so there is nothing to rough",
        ));
    }
    let footprint = crate::geom::Region::from_paths(&crate::geom::weld(&shares, tol)?, tol);
    Ok((field, footprint))
}

/// Offsets across the moulding the Along passes run at, past both sides
/// by the boundary offset.
fn along_offsets(
    section: &Section,
    side: &Side,
    stepover: f64,
    params: &MouldingParams,
) -> Vec<f64> {
    let extra = params.boundary_offset;
    if params.vary_stepover {
        section.surface_offsets(side, -extra, section.width() + extra, stepover)
    } else {
        let mut offsets = pass_offsets(section.width() + 2.0 * extra, stepover);
        for offset in &mut offsets {
            *offset -= extra;
        }
        offsets
    }
}

/// Passes parallel to the rail, one per offset across the moulding,
/// leaving `allowance` on the profile.
#[allow(clippy::too_many_arguments)]
fn along(
    rail: &SubPath,
    section: &Section,
    side: &Side,
    out: &mut Emitter,
    offsets: Vec<f64>,
    allowance: f64,
    params: &MouldingParams,
    warnings: &mut Vec<crate::warning::Warning>,
    tol: Tolerance,
) -> Result<(), CoreError> {
    let heights = out.heights();
    let rail_path = Path::from_subpath(rail.clone());
    for offset in offsets {
        // Past the profile's sides nothing is lower than its base.
        let tip = if offset < -1e-9 || offset > section.width() + 1e-9 {
            section.tip_at(offset, side).max(0.0)
        } else {
            section.tip_at(offset, side)
        };
        let depth = (section.top - tip - allowance).max(0.0);
        // The profile's top sits at the start depth.
        let z = heights.z_at(params.start_depth + depth);
        let pieces = if offset.abs() <= 1e-9 {
            vec![rail_path.clone()]
        } else {
            parallel_offset_open(&rail_path, offset.abs(), offset > 0.0, tol)?
        };
        let chains: Vec<Chain> = pieces
            .iter()
            .flat_map(|p| p.subpaths.iter())
            .map(|s| Chain::from_subpath(s, tol))
            .filter(|c| !c.is_empty())
            .map(|c| {
                if params.sharp_corners && offset.abs() > 1e-9 {
                    sharpen(c, offset.abs(), &rail_path, tol)
                } else {
                    c
                }
            })
            .collect();
        if chains.is_empty() {
            warnings.push(crate::warning::Warning::moulding_pass_left_out(offset));
            continue;
        }
        for chain in chains {
            let here = out.at().xy();
            let chain = if chain.closed {
                chain.starting_near(here)
            } else {
                let to_start = chain.start().map_or(f64::MAX, |p| p.distance(here));
                let to_end = chain.end().map_or(f64::MAX, |p| p.distance(here));
                if to_end < to_start {
                    chain.reversed()
                } else {
                    chain
                }
            };
            let Some(start) = chain.start() else {
                continue;
            };
            out.travel_to(start);
            out.plunge_to(z);
            out.cut_flat(&chain.segs, z);
        }
    }
    Ok(())
}

/// The chain with each arc the offset rolled round a corner of `rail`
/// (radius the offset distance, centred on the corner) replaced by two
/// straight moves meeting at the mitre point, where the lines either side
/// of the corner cross. The offsets of the rail's own arcs stay round, even
/// one that comes out with the offset's radius.
fn sharpen(chain: Chain, offset: f64, rail: &Path, tol: Tolerance) -> Chain {
    let mut segs = Vec::with_capacity(chain.segs.len());
    for seg in chain.segs {
        let corner = seg.arc().is_some_and(|arc| {
            (arc.radius - offset).abs() <= 1e-6 * offset.max(1.0)
                && arc.sweep.abs() < std::f64::consts::PI
                && crate::geom::nearest_point_on_path(rail, arc.centre, tol)
                    .is_some_and(|near| near.distance <= tol.get())
        });
        if !corner {
            segs.push(seg);
            continue;
        }
        let sweep = 4.0 * seg.bulge.atan();
        // The tangents at the arc's ends meet over the chord's middle, the
        // half chord times the tangent of half the sweep out from it.
        let mid = seg.from.lerp(seg.to, 0.5);
        let half = seg.from.distance(seg.to) / 2.0;
        let Some(along) = seg.from.to(seg.to).normalised() else {
            segs.push(seg);
            continue;
        };
        // Left of travel for a counter clockwise (positive) bulge is inside.
        let out = if sweep > 0.0 {
            along.perp().scaled(-1.0)
        } else {
            along.perp()
        };
        let apex = mid.offset(out.scaled(half * (sweep.abs() / 2.0).tan()));
        segs.push(super::chain::CutSeg {
            from: seg.from,
            to: apex,
            bulge: 0.0,
        });
        segs.push(super::chain::CutSeg {
            from: apex,
            to: seg.to,
            bulge: 0.0,
        });
    }
    Chain { segs, ..chain }
}

/// Turns sharper than this, in radians, get a pass up an inside mitre.
const MITRE_PASS_TURN: f64 = 5.0 * std::f64::consts::PI / 180.0;

/// Steepest mitre slope kept, so a rail that doubles back stays finite.
const MAX_MITRE_SLOPE: f64 = 1000.0;

/// One straight span of the flattened rail and the part of the moulding it
/// owns. In the span's frame `v` runs along it from `a` and `d` across the
/// moulding to its left; it owns the points with `0 <= d <= width` and
/// `start + d * start_slope <= v <= end + d * end_slope`, the slopes
/// following the bisectors of its corners.
#[derive(Debug, Clone, Copy)]
struct Span {
    a: Point,
    u: Vec2,
    n: Vec2,
    len: f64,
    start: f64,
    start_slope: f64,
    end: f64,
    end_slope: f64,
    lo: Point,
    hi: Point,
}

impl Span {
    /// Rail frame coordinates `(v, d)` of `p`.
    fn frame(&self, p: Point) -> (f64, f64) {
        let w = self.a.to(p);
        (w.dot(self.u), w.dot(self.n))
    }

    fn at(&self, v: f64, d: f64) -> Point {
        self.a.offset(self.u.scaled(v).plus(self.n.scaled(d)))
    }

    /// Signed distances from `(v, d)` into the span's share past its start
    /// and end mitres (negative outside).
    fn inside_mitres(&self, v: f64, d: f64) -> (f64, f64) {
        let s = (v - self.start - self.start_slope * d) / self.start_slope.hypot(1.0);
        let e = (self.end + self.end_slope * d - v) / self.end_slope.hypot(1.0);
        (s, e)
    }

    fn touches(&self, lo: Point, hi: Point) -> bool {
        self.lo.x <= hi.x && lo.x <= self.hi.x && self.lo.y <= hi.y && lo.y <= self.hi.y
    }
}

/// A pass across the moulding, from the rail side out.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pass {
    from: Point,
    to: Point,
}

/// The rail flattened into spans with their mitres, and the passes across
/// it.
struct Rail {
    spans: Vec<Span>,
    closed: bool,
    width: f64,
}

/// Unit bisector between two left normals, pointing across the moulding.
fn bisector(n0: Vec2, n1: Vec2, u0: Vec2) -> Vec2 {
    n0.plus(n1).normalised().unwrap_or(u0)
}

/// Slope of a mitre along `b` in a span's frame.
fn mitre_slope(b: Vec2, u: Vec2, n: Vec2) -> f64 {
    (b.dot(u) / b.dot(n).max(1.0 / MAX_MITRE_SLOPE)).clamp(-MAX_MITRE_SLOPE, MAX_MITRE_SLOPE)
}

impl Rail {
    fn new(
        sub: &SubPath,
        width: f64,
        radius: f64,
        stepover: f64,
        tol: Tolerance,
    ) -> Result<Rail, CoreError> {
        let line = flatten_subpath(sub, tol);
        let mut points: Vec<Point> = Vec::with_capacity(line.points.len());
        for &p in &line.points {
            if points.last().is_none_or(|q| q.distance(p) > 1e-9) {
                points.push(p);
            }
        }
        let closed = line.closed && points.len() > 2;
        if closed && points.len() > 1 && points[0].distance(points[points.len() - 1]) <= 1e-9 {
            points.pop();
        }
        let count = if closed {
            points.len()
        } else {
            points.len().saturating_sub(1)
        };
        let mut spans = Vec::with_capacity(count);
        for i in 0..count {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            let len = a.distance(b);
            let Some(u) = a.to(b).normalised() else {
                continue;
            };
            spans.push(Span {
                a,
                u,
                n: u.perp(),
                len,
                start: 0.0,
                start_slope: 0.0,
                end: len,
                end_slope: 0.0,
                lo: a,
                hi: a,
            });
        }
        if spans.is_empty() {
            return Err(CoreError::validation("the rail needs some length"));
        }
        // Open ends carry straight on far enough to hold the tool up.
        let reach = 2.0 * radius + stepover;
        let last = spans.len() - 1;
        let frames: Vec<(Vec2, Vec2)> = spans.iter().map(|s| (s.u, s.n)).collect();
        for (i, span) in spans.iter_mut().enumerate() {
            let prev = match i {
                0 if closed => Some(last),
                0 => None,
                _ => Some(i - 1),
            };
            let next = match i {
                _ if i < last => Some(i + 1),
                _ if closed => Some(0),
                _ => None,
            };
            match prev {
                Some(p) => {
                    let b = bisector(frames[p].1, span.n, frames[p].0);
                    span.start_slope = mitre_slope(b, span.u, span.n);
                }
                None => span.start = -reach,
            }
            match next {
                Some(q) => {
                    let b = bisector(span.n, frames[q].1, span.u);
                    span.end_slope = mitre_slope(b, span.u, span.n);
                }
                None => span.end = span.len + reach,
            }
            let corners = [
                span.at(span.start, 0.0),
                span.at(span.end, 0.0),
                span.at(span.start + span.start_slope * width, width),
                span.at(span.end + span.end_slope * width, width),
            ];
            span.lo = Point::new(
                corners.iter().map(|p| p.x).fold(f64::MAX, f64::min),
                corners.iter().map(|p| p.y).fold(f64::MAX, f64::min),
            );
            span.hi = Point::new(
                corners.iter().map(|p| p.x).fold(f64::MIN, f64::max),
                corners.iter().map(|p| p.y).fold(f64::MIN, f64::max),
            );
        }
        Ok(Rail {
            spans,
            closed,
            width,
        })
    }

    /// The passes in order along the rail: one every `stepover` or less
    /// along its length (an open rail has one at each end, a closed one
    /// wraps without doubling up), cut short at inside mitres, plus a pass
    /// up each sharp inside mitre and a fan round each outside corner.
    fn passes(&self, stepover: f64) -> Vec<Pass> {
        let total: f64 = self.spans.iter().map(|s| s.len).sum();
        let spans = ((total / stepover) - 1e-9).ceil().max(1.0) as usize;
        let stations = if self.closed { spans } else { spans + 1 };
        let width = self.width;
        let mut out = Vec::new();
        let mut k = 0;
        let mut begin = 0.0;
        let last = self.spans.len() - 1;
        for (i, span) in self.spans.iter().enumerate() {
            let finish = begin + span.len;
            while k < stations {
                let at = total * k as f64 / spans as f64;
                if at >= finish - 1e-9 && i < last {
                    break;
                }
                let v = (at - begin).clamp(0.0, span.len);
                let mut reach = width;
                if span.start_slope > 1e-12 {
                    reach = reach.min((v - span.start) / span.start_slope);
                }
                if span.end_slope < -1e-12 {
                    reach = reach.min((span.end - v) / -span.end_slope);
                }
                if reach > 1e-6 {
                    out.push(Pass {
                        from: span.at(v, 0.0),
                        to: span.at(v, reach),
                    });
                }
                k += 1;
            }
            begin = finish;
            let next = if i < last {
                &self.spans[i + 1]
            } else if self.closed {
                &self.spans[0]
            } else {
                continue;
            };
            let corner = span.at(span.len, 0.0);
            let turn = span.u.cross(next.u).atan2(span.u.dot(next.u));
            if turn >= MITRE_PASS_TURN {
                let b = bisector(span.n, next.n, span.u);
                let length = width / b.dot(span.n).max(1.0 / MAX_MITRE_SLOPE);
                out.push(Pass {
                    from: corner,
                    to: corner.offset(b.scaled(length)),
                });
            } else if turn < 0.0 {
                let fan = ((-turn * width / stepover) - 1e-9).ceil().max(1.0) as usize;
                for j in 1..fan {
                    let dir = span.n.rotated(turn * j as f64 / fan as f64);
                    let length = width / dir.dot(span.n).max(dir.dot(next.n)).max(1e-3);
                    out.push(Pass {
                        from: corner,
                        to: corner.offset(dir.scaled(length)),
                    });
                }
            }
        }
        out
    }

    /// Spans whose share of the moulding meets the box from `lo` to `hi`.
    fn near(&self, lo: Point, hi: Point) -> Vec<&Span> {
        self.spans.iter().filter(|s| s.touches(lo, hi)).collect()
    }
}

/// The section's tip heights from [`Section::tip_at`], sampled at the
/// section's own step from one tool radius before it to one after.
struct Compensated {
    first: f64,
    step: f64,
    tips: Vec<f64>,
}

impl Compensated {
    fn new(section: &Section, side: &Side) -> Compensated {
        let before = (side.radius / section.step).ceil();
        let first = -before * section.step;
        let count = section.heights.len() + 2 * before as usize;
        let tips = (0..count)
            .map(|k| section.tip_at(first + k as f64 * section.step, side))
            .collect();
        Compensated {
            first,
            step: section.step,
            tips,
        }
    }

    /// Tip height at `x` across the section, the higher neighbour where
    /// only one of them stands on the profile.
    fn at(&self, x: f64) -> f64 {
        let at = (x - self.first) / self.step;
        if at.is_nan() || at < 0.0 || at > (self.tips.len() - 1) as f64 {
            return f64::NEG_INFINITY;
        }
        let k = (at.floor() as usize).min(self.tips.len().saturating_sub(2));
        let (a, b) = (self.tips[k], self.tips[(k + 1).min(self.tips.len() - 1)]);
        if a.is_finite() && b.is_finite() {
            a + (b - a) * (at - k as f64)
        } else {
            a.max(b)
        }
    }
}

/// Section samples per block in the drop cutter's first, coarse look.
const BLOCK: usize = 16;

/// Everything a drop cutter on the swept surface needs.
struct Swept<'a> {
    section: &'a Section,
    side: &'a Side,
    compensated: Compensated,
    /// Highest section height in each block of [`BLOCK`] samples.
    blocks: Vec<f64>,
}

impl<'a> Swept<'a> {
    fn new(section: &'a Section, side: &'a Side) -> Swept<'a> {
        let blocks = section
            .heights
            .chunks(BLOCK)
            .map(|c| c.iter().copied().fold(f64::NEG_INFINITY, f64::max))
            .collect();
        Swept {
            section,
            side,
            compensated: Compensated::new(section, side),
            blocks,
        }
    }

    /// How high the tip stands at `p` over what `span` owns, in the
    /// section's heights, or minus infinity when the tool misses it.
    /// `order` is scratch space for the block bounds, reused from one point
    /// to the next.
    fn tip_over(&self, span: &Span, p: Point, order: &mut Vec<(f64, usize)>) -> f64 {
        let radius = self.side.radius;
        let width = self.section.width();
        let (v, d) = span.frame(p);
        if d < -radius || d > width + radius {
            return f64::NEG_INFINITY;
        }
        let (s, e) = span.inside_mitres(v, d);
        if s < -radius || e < -radius {
            return f64::NEG_INFINITY;
        }
        if s >= radius && e >= radius {
            return self.compensated.at(d);
        }
        // Near a mitre: bound each block of samples from above, then look
        // closely only at blocks that could stand higher than the best yet.
        let step = self.section.step;
        let last = self.section.heights.len() - 1;
        let from = ((d - radius) / step).ceil().max(0.0) as usize;
        let to = (((d + radius) / step).floor().max(0.0) as usize).min(last);
        let mut best = f64::NEG_INFINITY;
        if from > to {
            return best;
        }
        order.clear();
        for block in from / BLOCK..=to / BLOCK {
            let high = self.blocks[block];
            if !high.is_finite() {
                continue;
            }
            let x0 = (block * BLOCK) as f64 * step;
            let x1 = ((block * BLOCK + BLOCK - 1).min(last)) as f64 * step;
            let off = (x0 - d).max(d - x1).max(0.0);
            if off > radius {
                continue;
            }
            let chord = (radius * radius - off * off).sqrt();
            let lo = (span.start + span.start_slope * x0).min(span.start + span.start_slope * x1);
            let hi = (span.end + span.end_slope * x0).max(span.end + span.end_slope * x1);
            let (lo, hi) = ((v - chord).max(lo), (v + chord).min(hi));
            if lo > hi {
                continue;
            }
            let along = v.clamp(lo, hi) - v;
            order.push((high - self.side.lift(along.hypot(off)), block));
        }
        order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for &(bound, block) in order.iter() {
            if bound <= best {
                break;
            }
            let first = (block * BLOCK).max(from);
            let end = (block * BLOCK + BLOCK - 1).min(to);
            best = best.max(self.exact(span, v, d, first, end));
        }
        best
    }

    /// Highest tip over section samples `first..=end` of `span`'s share,
    /// the tool at `(v, d)` in the span's frame.
    fn exact(&self, span: &Span, v: f64, d: f64, first: usize, end: usize) -> f64 {
        let (radius, step) = (self.side.radius, self.section.step);
        let mut best = f64::NEG_INFINITY;
        for m in first..=end {
            let h = self.section.heights[m];
            if !h.is_finite() {
                continue;
            }
            let across = m as f64 * step;
            let off = across - d;
            let chord = radius * radius - off * off;
            if chord < 0.0 {
                continue;
            }
            let chord = chord.sqrt();
            let lo = (v - chord).max(span.start + span.start_slope * across);
            let hi = (v + chord).min(span.end + span.end_slope * across);
            if lo > hi {
                continue;
            }
            let along = v.clamp(lo, hi) - v;
            best = best.max(h - self.side.lift(along.hypot(off)));
        }
        best
    }

    /// Tip height at `p` over all of `spans`.
    fn tip(&self, spans: &[&Span], p: Point, order: &mut Vec<(f64, usize)>) -> f64 {
        spans
            .iter()
            .map(|s| self.tip_over(s, p, order))
            .fold(f64::NEG_INFINITY, f64::max)
    }
}

/// Points every `step` millimetres or less from `a` to `b`, both ends
/// included.
fn samples(a: Point, b: Point, step: f64) -> impl Iterator<Item = Point> {
    let count = ((a.distance(b) / step).ceil().max(1.0)) as usize;
    (0..=count).map(move |k| a.lerp(b, k as f64 / count as f64))
}

/// Keeps the fewest points of a run such that the straight moves between
/// them are never more than `tol` millimetres above or below any dropped
/// point.
fn thin(run: &[Point3], tol: f64) -> Vec<Point3> {
    let Some(&first) = run.first() else {
        return Vec::new();
    };
    let mut out = vec![first];
    let mut anchor = 0;
    let mut end = 1;
    while end < run.len() {
        let fits = |to: usize| {
            let (a, b) = (run[anchor], run[to]);
            let length = a.xy().distance(b.xy());
            run[anchor + 1..to].iter().all(|p| {
                let t = if length > 0.0 {
                    a.xy().distance(p.xy()) / length
                } else {
                    0.0
                };
                (a.z + (b.z - a.z) * t - p.z).abs() <= tol
            })
        };
        if end + 1 < run.len() && fits(end + 1) {
            end += 1;
            continue;
        }
        out.push(run[end]);
        anchor = end;
        end += 1;
    }
    out
}

/// Passes across the rail, back and forth, staying down between passes
/// that are close together.
#[allow(clippy::too_many_arguments)]
fn across(
    rail: &Rail,
    section: &Section,
    side: &Side,
    out: &mut Emitter,
    stepover: f64,
    params: &MouldingParams,
    warnings: &mut Vec<crate::warning::Warning>,
    tol: Tolerance,
) {
    let heights = out.heights();
    let swept = Swept::new(section, side);
    let radius = side.radius;
    let step = section.step.max(tol.get()).max(radius / 32.0);
    let link_limit = 2.0 * stepover.max(radius);
    // With a boundary offset the passes carry on past the profile's sides,
    // where nothing is lower than its base.
    let extra = params.boundary_offset;
    let z_of = |tip: f64| {
        let tip = if extra > 0.0 { tip.max(0.0) } else { tip };
        let depth = if tip.is_finite() {
            (section.top - tip - params.allowance).max(0.0)
        } else {
            0.0
        };
        heights.z_at(params.start_depth + depth)
    };
    let drop = |a: Point, b: Point| -> Vec<Point3> {
        let lo = Point::new(a.x.min(b.x) - radius, a.y.min(b.y) - radius);
        let hi = Point::new(a.x.max(b.x) + radius, a.y.max(b.y) + radius);
        let spans = rail.near(lo, hi);
        let mut order = Vec::new();
        samples(a, b, step)
            .map(|p| Point3::new(p.x, p.y, z_of(swept.tip(&spans, p, &mut order))))
            .collect()
    };
    let passes: Vec<Pass> = rail
        .passes(stepover)
        .into_iter()
        .map(|pass| {
            let Some(dir) = pass.from.to(pass.to).normalised() else {
                return pass;
            };
            // Passes cut short at an inside mitre keep their outer end.
            let full = pass.from.distance(pass.to) >= rail.width - 1e-6;
            Pass {
                from: pass.from.offset(dir.scaled(-extra)),
                to: if full {
                    pass.to.offset(dir.scaled(extra))
                } else {
                    pass.to
                },
            }
        })
        .collect();
    // A later rail starts afresh, not linked to the last one's end.
    let mut started = false;
    if passes.is_empty() {
        warnings.push(crate::warning::Warning::moulding_no_room());
    }
    // Every pass drops the cutter on its own, so they run in parallel; the
    // links between them depend on where the last one ended and follow in
    // order.
    let ends = |k: usize, pass: &Pass| {
        if k % 2 == 0 {
            (pass.from, pass.to)
        } else {
            (pass.to, pass.from)
        }
    };
    let dropped: Vec<Vec<Point3>> = passes
        .par_iter()
        .enumerate()
        .map(|(k, pass)| {
            let (a, b) = ends(k, pass);
            drop(a, b)
        })
        .collect();
    for ((k, pass), mut run) in passes.iter().enumerate().zip(dropped) {
        let (a, _) = ends(k, pass);
        let here = out.at();
        let Some(first) = run.first().copied() else {
            continue;
        };
        let link = started && here.xy().distance(a) <= link_limit;
        if link {
            let way = drop(here.xy(), a);
            let spans = way.len().saturating_sub(1).max(1) as f64;
            let mut joined: Vec<Point3> = way
                .iter()
                .enumerate()
                .skip(1)
                .map(|(j, p)| {
                    let t = j as f64 / spans;
                    let under = here.z + (first.z - here.z) * t;
                    Point3::new(p.x, p.y, p.z.max(under))
                })
                .collect();
            joined.pop();
            joined.append(&mut run);
            run = joined;
        }
        cut_run(out, &thin(&run, tol.get()), link);
        started = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::rect;

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn ball(diameter: f64) -> Tool {
        Tool {
            tool_type: ToolGeometry::BallNose { diameter },
            ..Tool::default()
        }
    }

    fn line(a: Point, b: Point) -> Path {
        let mut sub = SubPath::open(a);
        sub.line_to(b);
        sub.into_path()
    }

    /// A half circle bump of radius 10 from x 0 to 20, highest in the
    /// middle.
    fn bump() -> Path {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(20.0, 0.0), -1.0);
        sub.into_path()
    }

    #[test]
    fn sharp_corners_leave_the_rail_s_own_arcs_round() {
        // Straight, a left turn of radius 20, straight, a right turn of
        // radius 20, straight: no corner to sharpen. On the inside of each
        // turn the pass 10 mm across is an arc of radius 10.
        let bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(50.0, 0.0));
        sub.arc_to(Point::new(70.0, 20.0), bulge);
        sub.line_to(Point::new(70.0, 70.0));
        sub.arc_to(Point::new(90.0, 90.0), -bulge);
        sub.line_to(Point::new(140.0, 90.0));
        let rail = sub.into_path();
        let params = MouldingParams {
            stepover: Some(1.0),
            sharp_corners: true,
            ..MouldingParams::default()
        };
        let done = moulding(
            &[rail.clone(), bump()],
            &JobSetup::default(),
            &ball(2.0),
            &params,
            tol(),
        )
        .expect("moulding");
        // Every cutting point lies a whole number of millimetres from the
        // rail: on one of the passes.
        for m in done.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            let near =
                crate::geom::nearest_point_on_path(&rail, m.to.xy(), tol()).expect("near the rail");
            let d = near.point.distance(m.to.xy());
            assert!(
                (d - d.round()).abs() < 0.02,
                "{:?} is {d} mm from the rail, off every pass",
                m.to
            );
        }
    }

    fn plunges(moves: &[Move]) -> Vec<&Move> {
        moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .collect()
    }

    fn params(stepover: f64) -> MouldingParams {
        MouldingParams {
            stepover: Some(stepover),
            ..MouldingParams::default()
        }
    }

    #[test]
    fn a_flat_profile_is_cut_at_one_depth() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let flat = line(Point::new(50.0, 50.0), Point::new(60.0, 50.0));
        let done = moulding(
            &[rail.clone(), flat.clone()],
            &JobSetup::default(),
            &ball(6.0),
            &params(2.5),
            tol(),
        )
        .expect("moulding");
        let cuts: Vec<&Move> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        assert!(!cuts.is_empty());
        assert!(cuts.iter().all(|m| m.to.z.abs() < 1e-9));
        let deeper = MouldingParams {
            allowance: -1.5,
            ..params(2.5)
        };
        let done = moulding(
            &[rail, flat],
            &JobSetup::default(),
            &ball(6.0),
            &deeper,
            tol(),
        )
        .expect("moulding");
        assert!(done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .all(|m| (m.to.z + 1.5).abs() < 1e-9));
    }

    #[test]
    fn a_ball_rolls_over_a_half_circle() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let done = moulding(
            &[rail, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &params(2.0),
            tol(),
        )
        .expect("moulding");
        // The ball's centre runs on a circle of radius 13 round the bump's
        // centre, so the tip at offset x is sqrt(13^2 - (x - 10)^2) - 3 high.
        let tip = |x: f64| (169.0 - (x - 10.0f64).powi(2)).sqrt() - 3.0;
        let z_on = |y: f64| {
            done.moves
                .iter()
                .filter(|m| m.kind == MoveKind::Feed && (m.to.y - y).abs() < 1e-6)
                .map(|m| m.to.z)
                .next()
                .expect("a pass")
        };
        for y in [0.0, 4.0, 10.0, 16.0, 20.0] {
            let want = tip(y) - 10.0;
            let got = z_on(y);
            assert!((got - want).abs() < 0.03, "at {y}: {got} not {want}");
        }
        assert!(z_on(10.0).abs() < 0.02);
        assert!((z_on(0.0) + 10.0 - 69f64.sqrt() + 3.0).abs() < 0.03);
    }

    #[test]
    fn a_flat_end_mill_stands_on_the_highest_point_in_its_footprint() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let done = moulding(
            &[rail, bump()],
            &JobSetup::default(),
            &Tool::default(),
            &params(2.0),
            tol(),
        )
        .expect("moulding");
        // A 6 mm end mill at offset 0 rests on the bump 3 mm in.
        let want = (100.0 - 49.0f64).sqrt() - 10.0;
        let got = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.to.y.abs() < 1e-6)
            .map(|m| m.to.z)
            .next()
            .expect("a pass");
        assert!((got - want).abs() < 0.03, "{got} not {want}");
    }

    #[test]
    fn the_pass_count_follows_the_stepover() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let count = |step: f64| {
            let done = moulding(
                &[rail.clone(), bump()],
                &JobSetup::default(),
                &ball(6.0),
                &params(step),
                tol(),
            )
            .expect("moulding");
            plunges(&done.moves).len()
        };
        assert_eq!(count(2.0), 11);
        assert_eq!(count(3.0), 8);
        assert_eq!(count(20.0), 2);
        assert_eq!(pass_offsets(20.0, 3.0).len(), 8);
        let scaled = MouldingParams {
            width_scale: 0.5,
            ..params(2.0)
        };
        let done = moulding(
            &[rail, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &scaled,
            tol(),
        )
        .expect("moulding");
        assert_eq!(plunges(&done.moves).len(), 6);
    }

    #[test]
    fn open_rails_zigzag_and_closed_rails_loop_inside() {
        let open = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let done = moulding(
            &[open, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &params(5.0),
            tol(),
        )
        .expect("moulding");
        let starts: Vec<f64> = plunges(&done.moves).iter().map(|m| m.to.x).collect();
        assert_eq!(starts.len(), 5);
        for pair in starts.windows(2) {
            assert!((pair[0] - pair[1]).abs() > 99.0, "{starts:?}");
        }

        let frame = rect(Point::new(0.0, 0.0), Point::new(100.0, 60.0)).expect("rect");
        let done = moulding(
            &[frame, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &params(5.0),
            tol(),
        )
        .expect("moulding");
        let down = plunges(&done.moves);
        assert_eq!(down.len(), 5);
        let cuts: Vec<&Move> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        for m in &cuts {
            assert!(
                m.to.x > -1e-6 && m.to.x < 100.0 + 1e-6 && m.to.y > -1e-6 && m.to.y < 60.0 + 1e-6,
                "{m:?}"
            );
        }
        // The innermost loop is 20 mm in and each loop ends where it began.
        assert!(cuts
            .iter()
            .any(|m| (m.to.x - 20.0).abs() < 1e-6 && (m.to.y - 20.0).abs() < 1e-6));
        let mut last_plunge = None;
        for (i, m) in done.moves.iter().enumerate() {
            if m.kind == MoveKind::Plunge {
                last_plunge = Some(m.to);
            }
            let lifts = done
                .moves
                .get(i + 1)
                .is_some_and(|n| n.kind == MoveKind::Rapid && n.to.z > m.to.z);
            if m.kind == MoveKind::Feed && lifts {
                let p = last_plunge.expect("plunged");
                assert!(m.to.xy().distance(p.xy()) < 1e-6, "{m:?} vs {p:?}");
            }
        }
    }

    #[test]
    fn a_missing_profile_and_bad_settings_are_refused() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let job = JobSetup::default();
        let refused = |paths: &[Path], params: &MouldingParams| {
            matches!(
                moulding(paths, &job, &ball(6.0), params, tol()),
                Err(CoreError::Validation(_))
            )
        };
        let ok = MouldingParams::default();
        assert!(refused(std::slice::from_ref(&rail), &ok));
        assert!(refused(&[], &ok));
        let dot = line(Point::new(5.0, 0.0), Point::new(5.0, 10.0));
        assert!(refused(&[rail.clone(), dot], &ok));
        let both = [rail, bump()];
        assert!(refused(&both, &params(0.0)));
        assert!(refused(
            &both,
            &MouldingParams {
                height_scale: 0.0,
                ..ok.clone()
            }
        ));
        let across = MouldingParams {
            strategy: MouldingStrategy::Across,
            ..ok
        };
        assert!(!refused(&both, &across));
        assert!(refused(&both, &params(0.0)));
    }

    fn across(stepover: f64) -> MouldingParams {
        MouldingParams {
            strategy: MouldingStrategy::Across,
            ..params(stepover)
        }
    }

    fn rail_of(path: &Path, width: f64, stepover: f64) -> Rail {
        let sub = path.subpaths.first().expect("a subpath");
        Rail::new(sub, width, 3.0, stepover, tol()).expect("rail")
    }

    /// Height of the bump's top surface in job Z at `d` across it.
    fn bump_z(d: f64) -> f64 {
        (100.0 - (d - 10.0).powi(2)).max(0.0).sqrt() - 10.0
    }

    #[test]
    fn across_passes_follow_the_stepover_and_zigzag() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        for (step, want) in [(10.0, 11), (5.0, 21), (3.0, 35), (100.0, 2)] {
            let passes = rail_of(&rail, 20.0, step).passes(step);
            assert_eq!(passes.len(), want, "stepover {step}");
            for p in &passes {
                assert!(
                    (p.to.y - p.from.y - 20.0).abs() < 1e-9 && (p.to.x - p.from.x).abs() < 1e-9
                );
            }
            assert!((passes[0].from.x).abs() < 1e-9);
            assert!((passes[passes.len() - 1].from.x - 100.0).abs() < 1e-9);
        }
        let done = moulding(
            &[rail, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &across(5.0),
            tol(),
        )
        .expect("moulding");
        // One plunge: neighbouring passes are joined, alternately outwards
        // and back to the rail.
        assert_eq!(plunges(&done.moves).len(), 1);
        let cuts: Vec<&Move> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        for x in [0.0, 5.0, 10.0, 95.0, 100.0] {
            let on: Vec<f64> = cuts
                .iter()
                .filter(|m| (m.to.x - x).abs() < 1e-6)
                .map(|m| m.to.y)
                .collect();
            assert!(on.iter().any(|y| y.abs() < 1e-6), "{x}: {on:?}");
            assert!(on.iter().any(|y| (y - 20.0).abs() < 1e-6), "{x}: {on:?}");
        }
        let firsts: Vec<f64> = [0.0, 5.0, 10.0]
            .iter()
            .map(|&x| {
                cuts.iter()
                    .find(|m| (m.to.x - x).abs() < 1e-6)
                    .map(|m| m.to.y)
                    .expect("a pass")
            })
            .collect();
        assert!(
            firsts[0] < 1.0 && firsts[1] > 19.0 && firsts[2] < 1.0,
            "{firsts:?}"
        );
    }

    #[test]
    fn across_heights_on_a_straight_rail_match_the_profile() {
        let rail = line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let done = moulding(
            &[rail.clone(), bump()],
            &JobSetup::default(),
            &ball(6.0),
            &across(5.0),
            tol(),
        )
        .expect("moulding");
        // The ball's centre rolls on a circle of radius 13 round the bump's
        // centre, as along the rail.
        let tip = |y: f64| (169.0 - (y - 10.0f64).powi(2)).sqrt() - 3.0 - 10.0;
        let mut checked = 0;
        for m in done.moves.iter().filter(|m| m.kind.is_cutting()) {
            if (m.to.x - 50.0).abs() < 1e-6 {
                assert!((m.to.z - tip(m.to.y)).abs() < 0.03, "{m:?}");
                checked += 1;
            }
        }
        assert!(checked > 5, "{checked}");
        let along = moulding(
            &[rail.clone(), bump()],
            &JobSetup::default(),
            &ball(6.0),
            &params(5.0),
            tol(),
        )
        .expect("moulding");
        let deepest = |moves: &[Move]| {
            moves
                .iter()
                .filter(|m| m.kind.is_cutting())
                .map(|m| m.to.z)
                .fold(f64::MAX, f64::min)
        };
        assert!((deepest(&done.moves) - deepest(&along.moves)).abs() < 0.03);
        let flat = line(Point::new(50.0, 50.0), Point::new(60.0, 50.0));
        let lower = MouldingParams {
            allowance: -1.5,
            ..across(2.5)
        };
        let done = moulding(
            &[rail, flat],
            &JobSetup::default(),
            &ball(6.0),
            &lower,
            tol(),
        )
        .expect("moulding");
        assert!(done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.kind != MoveKind::Plunge)
            .all(|m| (m.to.z + 1.5).abs() < 1e-9));
    }

    #[test]
    fn across_does_not_gouge_an_inside_corner() {
        let frame = rect(Point::new(0.0, 0.0), Point::new(100.0, 60.0)).expect("rect");
        let tool = ball(6.0);
        let done = moulding(
            &[frame, bump()],
            &JobSetup::default(),
            &tool,
            &across(2.0),
            tol(),
        )
        .expect("moulding");
        let side = Side::of(&tool).expect("side");
        // Inside the frame the mitred surface is the bump at the distance to
        // the nearest side.
        let surface = |x: f64, y: f64| {
            let d = x.min(y).min(100.0 - x).min(60.0 - y);
            if (0.0..=20.0).contains(&d) {
                bump_z(d)
            } else {
                f64::NEG_INFINITY
            }
        };
        // The profile is flattened to the tolerance, so move ends may stand
        // that much into the true arc, and points between them may dip by
        // the tolerance the moves were thinned to as well.
        let mut points: Vec<(Point3, f64)> = Vec::new();
        let mut from = done.moves[0].to;
        for m in &done.moves {
            if m.kind.is_cutting() && m.to.x < 30.0 && m.to.y < 30.0 {
                for k in 0..4 {
                    let t = k as f64 / 4.0;
                    let at = Point3::new(
                        from.x + (m.to.x - from.x) * t,
                        from.y + (m.to.y - from.y) * t,
                        from.z + (m.to.z - from.z) * t,
                    );
                    points.push((at, if k == 0 { 0.015 } else { 0.03 }));
                }
            }
            from = m.to;
        }
        assert!(points.len() > 50, "{}", points.len());
        let (grid, reach) = (0.05, 60);
        let mut closest = f64::MAX;
        for (p, slack) in &points {
            for i in -reach..=reach {
                for j in -reach..=reach {
                    let (dx, dy) = (i as f64 * grid, j as f64 * grid);
                    let u = dx.hypot(dy);
                    if u > 3.0 {
                        continue;
                    }
                    let h = surface(p.x + dx, p.y + dy);
                    if !h.is_finite() {
                        continue;
                    }
                    let clearance = p.z + side.lift(u) - h;
                    assert!(clearance > -slack, "gouge {clearance} at {p:?}");
                    closest = closest.min(clearance);
                }
            }
        }
        assert!(closest < 0.05, "{closest}");
        // The mitre itself gets a pass from the corner inwards.
        assert!(done
            .moves
            .iter()
            .any(|m| m.kind.is_cutting() && m.to.x > 1.0 && (m.to.x - m.to.y).abs() < 1e-6));
    }

    #[test]
    fn a_closed_rail_wraps_round_once() {
        let frame = rect(Point::new(0.0, 0.0), Point::new(100.0, 60.0)).expect("rect");
        let rail = rail_of(&frame, 20.0, 10.0);
        let passes = rail.passes(10.0);
        // 32 stations round 320 mm; the four on the corners have no room
        // before the mitre, and each corner gets a pass up its mitre.
        assert_eq!(passes.len(), 32);
        for (i, a) in passes.iter().enumerate() {
            for b in &passes[i + 1..] {
                assert!(a.from.distance(b.from) > 1e-6 || a.to.distance(b.to) > 1e-6);
            }
        }
        let mitres = passes
            .iter()
            .filter(|p| p.to.distance(p.from) > 20.0 + 1e-6)
            .count();
        assert_eq!(mitres, 4);
        let end = passes[passes.len() - 1];
        assert!(end.from.distance(Point::new(0.0, 0.0)) < 1e-9);
        assert!(end.to.distance(Point::new(20.0, 20.0)) < 1e-6);
        let first = passes[0];
        assert!(first.from.distance(Point::new(10.0, 0.0)) < 1e-9);
        // An open rail running round the same frame has passes at both ends
        // and no mitre where it starts.
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        for p in [(100.0, 0.0), (100.0, 60.0), (0.0, 60.0), (0.0, 0.0)] {
            open.line_to(Point::new(p.0, p.1));
        }
        let passes = rail_of(&open.into_path(), 20.0, 10.0).passes(10.0);
        assert_eq!(passes.len(), 33 - 3 + 3);
        assert!(passes[0].from.distance(Point::new(0.0, 0.0)) < 1e-9);
        assert!(passes[0].to.distance(Point::new(0.0, 20.0)) < 1e-9);
    }

    #[test]
    fn an_outside_corner_gets_a_fan() {
        // Clockwise, so the moulding lies outside the square.
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        for p in [(0.0, 50.0), (50.0, 50.0), (50.0, 0.0)] {
            sub.line_to(Point::new(p.0, p.1));
        }
        sub.close_with_line(tol());
        let square = sub.into_path();
        let passes = rail_of(&square, 20.0, 5.0).passes(5.0);
        // 40 stations and 6 extra directions round each quarter turn.
        assert_eq!(passes.len(), 40 + 4 * 6);
        let done = moulding(
            &[square, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &across(5.0),
            tol(),
        )
        .expect("moulding");
        for m in done.moves.iter().filter(|m| m.kind.is_cutting()) {
            let inside =
                m.to.x > 1e-6 && m.to.x < 50.0 - 1e-6 && m.to.y > 1e-6 && m.to.y < 50.0 - 1e-6;
            assert!(!inside, "{m:?}");
        }
    }

    #[test]
    fn params_use_camel_case_names() {
        let json = serde_json::to_value(MouldingParams::default()).expect("json");
        for key in [
            "stepover",
            "widthScale",
            "heightScale",
            "allowance",
            "strategy",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["strategy"], "along");
    }

    fn step() -> Path {
        // A 10 mm wide flat step 5 mm high: a vertical edge at each side.
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(0.0, 5.0));
        sub.line_to(Point::new(10.0, 5.0));
        sub.line_to(Point::new(10.0, 0.0));
        sub.into_path()
    }

    #[test]
    fn every_rail_but_the_last_vector_is_machined_below_the_start_depth() {
        let one = line(Point::new(0.0, 0.0), Point::new(50.0, 0.0));
        let two = line(Point::new(0.0, 40.0), Point::new(50.0, 40.0));
        let params = MouldingParams {
            start_depth: 2.0,
            stepover: Some(2.0),
            ..MouldingParams::default()
        };
        let done = moulding(
            &[one, two, bump()],
            &JobSetup::default(),
            &ball(6.0),
            &params,
            tol(),
        )
        .expect("moulding");
        let cuts: Vec<Point3> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| m.to)
            .collect();
        assert!(cuts.iter().any(|p| p.y < 15.0), "the first rail");
        assert!(cuts.iter().any(|p| p.y > 35.0), "the second rail");
        let highest = cuts.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!(
            (highest + 2.0).abs() < 1e-6,
            "the top is at the start depth: {highest}"
        );
        assert!(moulding(&[bump()], &JobSetup::default(), &ball(6.0), &params, tol()).is_err());
    }

    #[test]
    fn a_boundary_offset_carries_the_passes_past_the_sides_at_the_base() {
        let rail = line(Point::new(0.0, 0.0), Point::new(50.0, 0.0));
        let run = |boundary_offset| {
            let params = MouldingParams {
                stepover: Some(1.0),
                boundary_offset,
                ..MouldingParams::default()
            };
            moulding(
                &[rail.clone(), step()],
                &JobSetup::default(),
                &ball(4.0),
                &params,
                tol(),
            )
            .expect("moulding")
            .moves
        };
        let ys = |moves: &[Move]| -> (f64, f64) {
            moves
                .iter()
                .filter(|m| m.kind == MoveKind::Feed)
                .fold((f64::MAX, f64::MIN), |(lo, hi), m| {
                    (lo.min(m.to.y), hi.max(m.to.y))
                })
        };
        let (lo, hi) = ys(&run(0.0));
        assert!(lo >= -1e-6 && hi <= 10.0 + 1e-6, "{lo} {hi}");
        let wider = run(3.0);
        let (lo, hi) = ys(&wider);
        assert!(
            (lo + 3.0).abs() < 1e-6 && (hi - 13.0).abs() < 1e-6,
            "{lo} {hi}"
        );
        // Well past the step the tool runs at the profile's base, 5 mm down.
        let outer: Vec<f64> = wider
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.to.y < -2.5)
            .map(|m| m.to.z)
            .collect();
        assert!(!outer.is_empty());
        assert!(outer.iter().all(|z| (z + 5.0).abs() < 1e-6), "{outer:?}");
    }

    #[test]
    fn across_on_a_curved_rail_calculates_in_parallel_to_the_same_moves() {
        let rail = crate::geom::circle(Point::new(0.0, 0.0), 30.0).expect("circle");
        let run = || {
            moulding(
                &[rail.clone(), bump()],
                &JobSetup::default(),
                &ball(3.0),
                &across(1.0),
                tol(),
            )
            .expect("moulding")
            .moves
        };
        let first = run();
        assert!(first.len() > 100);
        for _ in 0..3 {
            assert_eq!(run(), first);
        }
    }

    /// Plunge depths of the Along passes, one per pass, in order.
    fn pass_depths(moves: &[Move]) -> Vec<f64> {
        plunges(moves).iter().map(|m| m.to.z).collect()
    }

    #[test]
    fn vary_stepover_puts_more_passes_where_the_profile_is_steep() {
        let rail = line(Point::new(0.0, 0.0), Point::new(50.0, 0.0));
        let run = |vary_stepover| {
            let params = MouldingParams {
                vary_stepover,
                ..params(1.0)
            };
            moulding(
                &[rail.clone(), bump()],
                &JobSetup::default(),
                &ball(3.0),
                &params,
                tol(),
            )
            .expect("moulding")
            .moves
        };
        let even = pass_depths(&run(false));
        let varied = pass_depths(&run(true));
        // The tip rolls over more than 20 mm of the half round's surface.
        assert_eq!(even.len(), 21);
        assert!(varied.len() > 23 && varied.len() < 36, "{}", varied.len());
        // Neighbouring passes are never more than the stepover apart over
        // the surface.
        let ys: Vec<f64> = run(true)
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.y)
            .collect();
        for (pair, zs) in ys.windows(2).zip(varied.windows(2)) {
            let apart = (pair[1] - pair[0]).hypot(zs[1] - zs[0]);
            assert!(apart <= 1.0 + 0.1, "{apart}");
        }
    }

    #[test]
    fn sharp_corners_mitre_the_outer_passes_of_a_square_rail() {
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        // Clockwise, so the moulding lies outside the square.
        let square = crate::geom::edit::reverse(&square);
        let run = |sharp_corners| {
            let params = MouldingParams {
                sharp_corners,
                ..params(5.0)
            };
            moulding(
                &[square.clone(), bump()],
                &JobSetup::default(),
                &ball(3.0),
                &params,
                tol(),
            )
            .expect("moulding")
            .moves
        };
        let rolled = run(false);
        assert!(rolled.iter().any(|m| m.kind.is_arc()));
        let sharp = run(true);
        assert!(!sharp.iter().any(|m| m.kind.is_arc()));
        // The outermost pass, 20 mm out, reaches the mitre corner.
        assert!(sharp
            .iter()
            .any(|m| (m.to.x + 20.0).abs() < 1e-6 && (m.to.y + 20.0).abs() < 1e-6));
    }

    #[test]
    fn a_boundary_offset_carries_across_passes_past_the_profile_too() {
        let rail = line(Point::new(0.0, 0.0), Point::new(50.0, 0.0));
        let params = MouldingParams {
            boundary_offset: 3.0,
            ..across(2.0)
        };
        let done = moulding(
            &[rail, step()],
            &JobSetup::default(),
            &ball(2.0),
            &params,
            tol(),
        )
        .expect("moulding");
        let cuts: Vec<Point3> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| m.to)
            .collect();
        let lo = cuts.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        let hi = cuts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!(lo <= -3.0 + 1e-6 && hi >= 13.0 - 1e-6, "{lo} {hi}");
        // Out past the step the tool is down at its base, 5 mm under.
        assert!(cuts
            .iter()
            .filter(|p| p.y < -2.0 || p.y > 12.0)
            .all(|p| (p.z + 5.0).abs() < 1e-6));
    }

    #[test]
    fn a_clearance_tool_roughs_the_moulding_first_leaving_its_allowance() {
        let rail = line(Point::new(-40.0, 0.0), Point::new(40.0, 0.0));
        let params = MouldingParams {
            clearance_tool: Some(Tool {
                name: "End Mill 6 mm".to_string(),
                tool_type: ToolGeometry::EndMill { diameter: 6.0 },
                pass_depth: 4.0,
                stepover: 2.4,
                ..Tool::default()
            }),
            clearance_allowance: 0.5,
            ..params(1.0)
        };
        let job = JobSetup::default();
        let (finish, rough) =
            moulding_with_clearance(&[rail.clone(), bump()], &job, &ball(3.0), &params, tol())
                .expect("moulding");
        let rough = rough.expect("a roughing pass");
        assert!(!finish.moves.is_empty());
        let deepest = rough.moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        // The bump is 10 mm high: the roughing goes down to its base less
        // nothing below it, and never into the allowance on the crown.
        assert!((-10.0 - 1e-6..-5.0).contains(&deepest), "{deepest}");
        let crown: Vec<f64> = rough
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.to.y > 9.0 && m.to.y < 11.0 && m.to.x.abs() < 30.0)
            .map(|m| m.to.z)
            .collect();
        assert!(crown.iter().all(|z| *z >= -0.5 - 1e-6), "{crown:?}");
        let none = MouldingParams {
            clearance_tool: None,
            ..params
        };
        let (_, nothing) = moulding_with_clearance(&[rail, bump()], &job, &ball(3.0), &none, tol())
            .expect("moulding");
        assert!(nothing.is_none());
    }

    /// A channel: flat rims 10 high from 0 to 10 and 30 to 40, a floor 2
    /// high from 10 to 30.
    fn channel() -> Path {
        let mut sub = SubPath::open(Point::new(0.0, 10.0));
        for (x, y) in [
            (10.0, 10.0),
            (10.0, 2.0),
            (30.0, 2.0),
            (30.0, 10.0),
            (40.0, 10.0),
        ] {
            sub.line_to(Point::new(x, y));
        }
        sub.into_path()
    }

    fn end_mill(diameter: f64) -> Tool {
        Tool {
            name: "End Mill".to_string(),
            tool_type: ToolGeometry::EndMill { diameter },
            pass_depth: 4.0,
            stepover: 2.0,
            ..Tool::default()
        }
    }

    #[test]
    fn the_flats_are_the_level_stretches_the_tool_fits() {
        let profile = channel();
        let section =
            Section::from_subpath(&profile.subpaths[0], 1.0, 1.0, tol()).expect("section");
        let wide = section.flats(12.0);
        assert_eq!(wide.len(), 1, "{wide:?}");
        let (from, to, h) = wide[0];
        // Heights count from the profile's lowest point.
        assert!((from - 10.0).abs() < 0.1 && (to - 30.0).abs() < 0.1 && h.abs() < 1e-9);
        let all = section.flats(6.0);
        assert_eq!(all.len(), 3, "{all:?}");
    }

    #[test]
    fn machine_flat_regions_cuts_the_flats_to_their_height_and_skips_them() {
        let rail = line(Point::new(0.0, 0.0), Point::new(80.0, 0.0));
        let base = MouldingParams {
            clearance_tool: Some(end_mill(6.0)),
            clearance_allowance: 0.5,
            ..params(1.0)
        };
        let job = JobSetup::default();
        let run = |params: &MouldingParams| {
            moulding_with_clearance(&[rail.clone(), channel()], &job, &ball(3.0), params, tol())
                .expect("moulding")
        };
        let deepest = |moves: &[Move]| moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        let (plain_finish, plain) = run(&base);
        let plain = plain.expect("roughing");
        // The floor is 8 below the top: the roughing leaves its allowance.
        assert!(
            deepest(&plain.moves) > -8.0 + 0.4,
            "{}",
            deepest(&plain.moves)
        );
        let flats = MouldingParams {
            machine_flat_regions: true,
            ..base.clone()
        };
        let (_, cut) = run(&flats);
        let cut = cut.expect("roughing");
        assert!(
            (deepest(&cut.moves) + 8.0).abs() < 1e-6,
            "{}",
            deepest(&cut.moves)
        );
        // The flat passes stay on the floor, clear of its walls.
        for m in cut
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 8.0).abs() < 1e-6)
        {
            assert!(m.to.y > 13.0 - 1e-6 && m.to.y < 27.0 + 1e-6, "{m:?}");
        }
        let skipping = MouldingParams {
            skip_flat_regions: true,
            ..flats
        };
        let (finish, _) = run(&skipping);
        let passes = |moves: &[Move]| moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
        assert!(passes(&finish.moves) < passes(&plain_finish.moves));
        for m in finish.moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
            assert!(!(m.to.y > 11.5 + 1e-6 && m.to.y < 28.5 - 1e-6), "{m:?}");
        }
    }

    #[test]
    fn the_automatic_boundary_offset_carries_past_the_sides_by_the_tool() {
        let rail = line(Point::new(0.0, 0.0), Point::new(80.0, 0.0));
        let job = JobSetup::default();
        let reach = |params: &MouldingParams| {
            let done = moulding(&[rail.clone(), bump()], &job, &ball(6.0), params, tol())
                .expect("moulding");
            let ys: Vec<f64> = done
                .moves
                .iter()
                .filter(|m| m.kind == MoveKind::Feed)
                .map(|m| m.to.y)
                .collect();
            (
                ys.iter().copied().fold(f64::MAX, f64::min),
                ys.iter().copied().fold(f64::MIN, f64::max),
            )
        };
        let (lo, hi) = reach(&params(1.0));
        assert!(lo.abs() < 1e-6 && (hi - 20.0).abs() < 1e-6, "{lo} {hi}");
        let auto = MouldingParams {
            auto_boundary_offset: true,
            boundary_offset: 1.0,
            ..params(1.0)
        };
        let (lo, hi) = reach(&auto);
        assert!(
            (lo + 3.3).abs() < 1e-6 && (hi - 23.3).abs() < 1e-6,
            "{lo} {hi}"
        );
    }
}
