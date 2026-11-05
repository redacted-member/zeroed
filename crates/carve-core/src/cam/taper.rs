//! Tapered walls for the Profile toolpath: a wall that leans back into the
//! material by a set angle as it rises. The vector marks the wall's foot at
//! the cut depth; above it the wall stands `(cut depth - depth) x tan(angle)`
//! further into the material.
//!
//! With an end mill the wall is cut in fine levels from the start depth
//! down, each `h` deep, `h` chosen so a step stands out of the slope by no
//! more than the scallop tolerance. Each level's pass puts the wall where
//! the slope is at the top of that level, so the wall is never left proud
//! of the angle: the top corner of every step lies on it and a step cuts at
//! most the scallop past it (a joint's clearance only grows).
//!
//! Roughing passes at the tool's pass depth come first, their wall where
//! the slope is at the bottom of the pass, cleared out to where the tool
//! runs at the foot, so a fine level takes off only a sliver. Levels below
//! the last roughing pass clear out to the foot themselves, so a part comes
//! free on the last pass, as in an upright profile.
//!
//! A V shaped tool whose sides lean at the wall angle cuts the wall in the
//! usual passes, the profile setting it off by its width at the cut depth.
//!
//! Closed vectors lean back square to each wall (the region shrinks or
//! grows); an open vector cut beside the line moves square to itself. A
//! rigid open vector (the Joint Creator's socket walls) moves as a whole,
//! square to its longest straight span, so a wall it turns onto at its end
//! stays upright: the back of a socket.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{parallel_offset_open, Path, Point, Region, Segment, SubPath, Tolerance, Vec2};

use super::chain::{Chain, CutSeg};
use super::common::{planned_depths, region_parts, restart_nearest, Emitter, Heights, PassPlan};
use super::profile::{profile, Calculated, CutDirection, OpenSide, ProfileParams, ProfileSide};
use super::tool::{Tool, ToolGeometry};
use super::toolpath::{Move, MoveKind, Point3};

/// Steepest wall, degrees from upright.
pub const MAX_WALL_ANGLE: f64 = 45.0;

/// How far a V shaped tool's lean may be from the wall angle, degrees.
const ANGLE_MATCH: f64 = 0.1;

/// Finest and coarsest scallop tolerance, mm.
const MIN_SCALLOP: f64 = 0.001;
const MAX_SCALLOP: f64 = 1.0;

/// Most fine levels one wall may take.
const MAX_LEVELS: usize = 5000;

/// Clearing passes stand this share of the tool's diameter apart.
const CLEAR_STEP: f64 = 0.75;

/// A rigid open vector's tapered cut, for the exact solids: the tool's
/// passes at the foot and how the cut moves as the wall rises. At a depth
/// `d` from `top` to `bottom` the cut is the foot passes' kerf swept over
/// `(cut - d) x shift`; below `bottom` it stays as the last level left it,
/// swept over `(cut - bottom) x shift`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlopeCut {
    /// The passes at the foot, each from a rapid to its start, at the cut
    /// depth's Z.
    pub moves: Vec<Move>,
    /// How far the cut moves in job X and Y for each millimetre it rises.
    pub shift: [f64; 2],
    /// Depth below the surface the wall starts at.
    pub top: f64,
    /// Depth the last level starts at, below which the wall stands upright.
    pub bottom: f64,
    /// Depth of the foot.
    pub cut: f64,
}

/// The fine levels of a wall from `start` to `cut` depth leaning `angle`
/// degrees, each step at most `scallop` proud: the level depths, deepest
/// last and exactly `cut`.
///
/// # Errors
/// [`CoreError::Validation`] when the wall would need more than
/// [`MAX_LEVELS`] levels.
pub fn fine_levels(start: f64, cut: f64, angle: f64, scallop: f64) -> Result<Vec<f64>, CoreError> {
    let tan = angle.to_radians().tan();
    let span = cut - start;
    let count = ((span * tan / scallop - 1e-9).ceil().max(1.0)) as usize;
    if count > MAX_LEVELS {
        return Err(CoreError::validation(format!(
            "the tapered wall would take more than {MAX_LEVELS} steps; use a larger scallop tolerance"
        )));
    }
    let h = span / count as f64;
    let mut out: Vec<f64> = (1..count).map(|k| start + h * k as f64).collect();
    out.push(cut);
    Ok(out)
}

fn fmt(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    format!("{rounded}")
}

/// Calculates a profile whose walls lean `params.wall_angle` degrees, and
/// the exact description of each rigid open vector's cut.
///
/// # Errors
/// [`CoreError::Validation`] for an angle, scallop or tool that cannot cut
/// the wall, for a cut on the vector (no side to lean), and as
/// [`profile`].
pub fn tapered(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &ProfileParams,
    tol: Tolerance,
) -> Result<(Calculated, Vec<SlopeCut>), CoreError> {
    let angle = params.wall_angle;
    if !(angle.is_finite() && angle > 0.0 && angle <= MAX_WALL_ANGLE) {
        return Err(CoreError::validation(format!(
            "the wall angle must be from 0 to {MAX_WALL_ANGLE} degrees"
        )));
    }
    match tool.tool_type {
        ToolGeometry::EndMill { .. } => {}
        ToolGeometry::VBit { included_angle, .. }
        | ToolGeometry::EngravingTool { included_angle, .. } => {
            let lean = included_angle / 2.0;
            if (lean - angle).abs() <= ANGLE_MATCH {
                let upright = ProfileParams {
                    wall_angle: 0.0,
                    ..params.clone()
                };
                return Ok((profile(paths, job, tool, &upright, tol)?, Vec::new()));
            }
            return Err(CoreError::validation(format!(
                "this tool's sides lean {} degrees; pick one whose sides lean {} degrees, or an end mill to cut the wall in steps",
                fmt(lean),
                fmt(angle)
            )));
        }
        _ => return Err(CoreError::validation(
            "a tapered wall is cut with an end mill, or a V-bit whose sides lean at the wall angle",
        )),
    }
    let scallop = params.scallop;
    if !(scallop.is_finite() && (MIN_SCALLOP..=MAX_SCALLOP).contains(&scallop)) {
        return Err(CoreError::validation(format!(
            "the scallop tolerance must be from {MIN_SCALLOP} to {MAX_SCALLOP} mm"
        )));
    }
    let closed = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .any(|s| s.closed && !s.segments.is_empty());
    let open = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .any(|s| !s.closed && !s.segments.is_empty());
    if (closed && params.side == ProfileSide::On) || (open && params.open_side == OpenSide::On) {
        return Err(CoreError::validation(
            "a tapered wall needs the tool to one side of the vector: pick outside or inside, or left or right of an open vector",
        ));
    }
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
    let fine = fine_levels(params.start_depth, params.cut_depth, angle, scallop)?;
    let tan = angle.to_radians().tan();
    let radius = tool.radius();
    let reach = radius + params.allowance;
    if reach < 0.0 {
        return Err(CoreError::validation(
            "the allowance cuts past the tool's centre; make it smaller",
        ));
    }
    let levels = plan_levels(params, &depths, &fine, tan);
    let step = CLEAR_STEP * 2.0 * radius;

    let heights = Heights::from_job(job);
    let mut out = Cutter {
        em: Emitter::new(heights),
        link: 2.0 * radius,
    };
    let mut slopes = Vec::new();

    // Closed vectors, part by part, smaller first.
    let region = Region::from_paths(paths, tol);
    let mut parts = region_parts(&region, tol);
    parts.sort_by(|a, b| a.area().abs().total_cmp(&b.area().abs()));
    let sign = if params.side == ProfileSide::Outside {
        -1.0
    } else {
        1.0
    };
    let reverse = match params.side {
        ProfileSide::Inside => params.direction == CutDirection::Conventional,
        ProfileSide::Outside | ProfileSide::On => params.direction == CutDirection::Climb,
    };
    for part in &parts {
        let chains = |w: f64| -> Vec<Chain> {
            let mut loops: Vec<(f64, Chain)> = part
                .offset(sign * (reach - w), tol)
                .loops
                .iter()
                .map(|sub| {
                    let chain = Chain::from_subpath(sub, tol);
                    let area = crate::geom::subpath_signed_area(sub).abs();
                    (area, if reverse { chain.reversed() } else { chain })
                })
                .collect();
            loops.sort_by(|a, b| a.0.total_cmp(&b.0));
            loops.into_iter().map(|(_, c)| c).collect()
        };
        for level in &levels {
            let z = heights.z_at(level.depth);
            for w in level.offsets(step, 2.0 * radius) {
                for chain in chains(w) {
                    out.pass(chain, z);
                }
            }
        }
        out.em.retract();
    }

    // Open vectors, one at a time, back and forth.
    let left = params.open_side == OpenSide::Left;
    for sub in paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter(|s| !s.closed && !s.segments.is_empty())
    {
        let line = sub.clone().into_path();
        let lean = if params.rigid {
            // Away from the tool: the tool is on the left of travel for Left.
            longest_span(sub).map(|e| {
                if left {
                    Vec2::new(e.y, -e.x)
                } else {
                    Vec2::new(-e.y, e.x)
                }
            })
        } else {
            None
        };
        let foot: Vec<Chain> = match lean {
            Some(_) => beside(&line, reach, left, tol)?,
            None => Vec::new(),
        };
        let chains = |w: f64| -> Result<Vec<Chain>, CoreError> {
            match lean {
                Some(m) => Ok(foot.iter().map(|c| shifted(c, m.scaled(w))).collect()),
                None => beside(&line, reach - w, left, tol),
            }
        };
        let mut forward = true;
        for level in &levels {
            let z = heights.z_at(level.depth);
            for w in level.offsets(step, 2.0 * radius) {
                for chain in chains(w)? {
                    let chain = if forward { chain } else { chain.reversed() };
                    out.pass(chain, z);
                    forward = !forward;
                }
            }
        }
        out.em.retract();
        if let Some(m) = lean {
            let z = heights.z_at(params.cut_depth);
            let mut moves = Vec::new();
            for chain in &foot {
                let Some(begin) = chain.start() else {
                    continue;
                };
                moves.push(Move::rapid(Point3::new(begin.x, begin.y, z)));
                for seg in &chain.segs {
                    let to = Point3::new(seg.to.x, seg.to.y, z);
                    moves.push(match seg.arc() {
                        Some(arc) if arc.sweep > 0.0 => Move::arc_ccw(arc.centre, to),
                        Some(arc) => Move::arc_cw(arc.centre, to),
                        None => Move::feed(to),
                    });
                }
            }
            let bottom = if fine.len() >= 2 {
                fine[fine.len() - 2]
            } else {
                params.start_depth
            };
            slopes.push(SlopeCut {
                moves,
                shift: [m.x * tan, m.y * tan],
                top: params.start_depth,
                bottom,
                cut: params.cut_depth,
            });
        }
    }
    let moves = out.em.finish();
    if !moves.iter().any(|m| {
        matches!(
            m.kind,
            MoveKind::Feed | MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. }
        )
    }) {
        return Err(CoreError::validation(
            "there is nothing to profile: select at least one vector the tool fits",
        ));
    }
    let mut warnings = Vec::new();
    if params.tabs.enabled
        || params.ramp.enabled
        || params.lead.is_active()
        || params.last_pass.enabled
    {
        warnings.push(crate::warning::Warning::new(
            "profile.taperPlain",
            "A tapered wall is cut without tabs, ramps, leads or a separate last pass.",
        ));
    }
    Ok((Calculated { moves, warnings }, slopes))
}

/// One level of a tapered cut: its depth, where its wall stands (how far
/// into the material from the foot) and how far out its passes clear: to
/// the wall a roughing pass left (`Some`) or to the foot (`None`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Level {
    depth: f64,
    wall: f64,
    floor: Option<f64>,
}

impl Level {
    /// The walls of the level's passes, the finished wall first, each
    /// `step` further out, until a tool `kerf` wide reaches the floor.
    fn offsets(&self, step: f64, kerf: f64) -> Vec<f64> {
        let mut out = vec![self.wall];
        let mut w = self.wall;
        match self.floor {
            None => {
                while w > 1e-9 {
                    w = (w - step).max(0.0);
                    out.push(w);
                }
            }
            Some(floor) => {
                while w - kerf > floor + 1e-9 {
                    w -= step;
                    out.push(w);
                }
            }
        }
        out
    }
}

/// The cutting order: each roughing pass (all but the last, which the
/// fine levels finish), then the fine levels it has cleared for, then the
/// fine levels below the last roughing pass.
fn plan_levels(params: &ProfileParams, rough: &[f64], fine: &[f64], tan: f64) -> Vec<Level> {
    let wall = |d: f64| (params.cut_depth - d).max(0.0) * tan;
    let top_of = |k: usize| {
        if k == 0 {
            params.start_depth
        } else {
            fine[k - 1]
        }
    };
    let mut out = Vec::new();
    let mut k = 0;
    for &depth in &rough[..rough.len().saturating_sub(1)] {
        out.push(Level {
            depth,
            wall: wall(depth),
            floor: None,
        });
        while k < fine.len() && fine[k] <= depth + 1e-9 {
            out.push(Level {
                depth: fine[k],
                wall: wall(top_of(k)),
                floor: Some(wall(depth)),
            });
            k += 1;
        }
    }
    while k < fine.len() {
        out.push(Level {
            depth: fine[k],
            wall: wall(top_of(k)),
            floor: None,
        });
        k += 1;
    }
    out
}

/// The unit direction of a subpath's longest straight span.
fn longest_span(sub: &SubPath) -> Option<Vec2> {
    let mut best: Option<(f64, Vec2)> = None;
    for (start, seg) in sub.iter() {
        if let Segment::Line { to } = *seg {
            let d = start.to(to);
            let len = d.length();
            if len > 1e-9 && best.is_none_or(|(l, _)| len > l) {
                best = d.normalised().map(|u| (len, u));
            }
        }
    }
    best.map(|(_, u)| u)
}

/// The track of a tool whose edge runs `reach` beside an open vector, on
/// the given side (the other side for a negative reach).
fn beside(line: &Path, reach: f64, left: bool, tol: Tolerance) -> Result<Vec<Chain>, CoreError> {
    if reach.abs() <= 1e-9 {
        return Ok(line
            .subpaths
            .iter()
            .map(|s| Chain::from_subpath(s, tol))
            .collect());
    }
    let side = if reach > 0.0 { left } else { !left };
    Ok(parallel_offset_open(line, reach.abs(), side, tol)?
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .map(|s| Chain::from_subpath(s, tol))
        .filter(|c| !c.is_empty())
        .collect())
}

/// A chain moved by `by`.
fn shifted(chain: &Chain, by: Vec2) -> Chain {
    Chain {
        segs: chain
            .segs
            .iter()
            .map(|s| CutSeg {
                from: s.from.offset(by),
                to: s.to.offset(by),
                bulge: s.bulge,
            })
            .collect(),
        closed: chain.closed,
    }
}

/// Writes the passes, linking one to the next at depth when it starts
/// within `link` of where the last ended (inside the cut), else by a rapid
/// at the clearance height.
struct Cutter {
    em: Emitter,
    link: f64,
}

impl Cutter {
    fn pass(&mut self, chain: Chain, z: f64) {
        let at = self.em.at();
        let here = Point::new(at.x, at.y);
        let chain = if chain.closed {
            restart_nearest(&chain, here)
        } else {
            chain
        };
        let Some(begin) = chain.start() else {
            return;
        };
        let heights = self.em.heights();
        let down = at.z < heights.plunge_from - 1e-9;
        if down && here.distance(begin) <= self.link {
            let across = CutSeg {
                from: here,
                to: begin,
                bulge: 0.0,
            };
            if z > at.z {
                self.em.plunge_to(z);
                self.em.cut(&across, z);
            } else {
                self.em.cut(&across, at.z);
                self.em.plunge_to(z);
            }
        } else {
            self.em.travel_to(begin);
            self.em.plunge_to(z);
        }
        self.em.cut_flat(&chain.segs, z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{polyline_path, rect};

    fn job() -> JobSetup {
        JobSetup::default()
    }

    fn end_mill(diameter: f64) -> Tool {
        Tool {
            name: "end mill".into(),
            tool_type: ToolGeometry::EndMill { diameter },
            pass_depth: 6.0,
            ..Tool::default()
        }
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.001).expect("tolerance")
    }

    #[test]
    fn fine_levels_step_so_the_scallop_stays_within_tolerance() {
        let levels = fine_levels(0.0, 12.0, 14.0, 0.02).expect("levels");
        let tan = 14f64.to_radians().tan();
        let h = levels[0];
        assert!(h * tan <= 0.02 + 1e-12, "{h}");
        assert!(h * tan > 0.019, "{h}");
        assert_eq!(*levels.last().expect("last"), 12.0);
        assert!(levels.windows(2).all(|w| (w[1] - w[0] - h).abs() < 1e-9));
    }

    /// Where the tool's wall side stands at each depth an open line's cut
    /// reaches, measured across the line from its foot.
    fn walls(moves: &[Move], surface: f64, radius: f64) -> Vec<(f64, f64)> {
        // The wall of a line along Y at x = 0, material at x < 0, the tool
        // to its right: the tool's left edge at each depth.
        let mut out: Vec<(f64, f64)> = Vec::new();
        for m in moves {
            if matches!(m.kind, MoveKind::Feed) && m.to.z < surface - 1e-9 {
                let depth = surface - m.to.z;
                let edge = m.to.x - radius;
                match out.iter_mut().find(|(d, _)| (d - depth).abs() < 1e-9) {
                    Some((_, e)) => *e = e.min(edge),
                    None => out.push((depth, edge)),
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    #[test]
    fn an_open_line_leans_back_at_the_angle_with_every_step_within_the_scallop() {
        let job = job();
        let tool = end_mill(6.0);
        let line =
            polyline_path(&[Point::new(0.0, 40.0), Point::new(0.0, 10.0)], false).expect("line");
        let params = ProfileParams {
            cut_depth: 12.0,
            open_side: OpenSide::Left,
            wall_angle: 14.0,
            scallop: 0.02,
            ..ProfileParams::default()
        };
        let (done, slopes) = tapered(&[line], &job, &tool, &params, tol()).expect("cuts");
        assert!(slopes.is_empty());
        let surface = job.surface_z();
        let at = walls(&done.moves, surface, 3.0);
        let tan = 14f64.to_radians().tan();
        // Going down the line (-Y) the left is +X: the tool cuts at +X and
        // the wall leans back to -X as it rises.
        let deepest = at.last().expect("a level");
        assert!((deepest.0 - 12.0).abs() < 1e-9);
        for (depth, edge) in &at {
            let ideal = -(12.0 - depth) * tan;
            // Never proud of the slope, and never more than the scallop past it
            // over the level above.
            assert!(*edge <= ideal + 1e-9, "{depth}: {edge} vs {ideal}");
        }
        // The top corners of the steps lie on a line at the wall angle.
        let fine: Vec<&(f64, f64)> = at.iter().filter(|(d, _)| *d > 0.0).collect();
        let first = fine[0];
        let last = fine[fine.len() - 2];
        let slope = (last.1 - first.1) / (last.0 - first.0);
        let lean = slope.atan().to_degrees();
        assert!((lean - 14.0).abs() < 0.1, "{lean}");
        let h = fine[1].0 - fine[0].0;
        assert!(h * tan <= 0.02 + 1e-9);
    }

    #[test]
    fn a_closed_square_shrinks_as_it_rises_and_frees_the_part_last() {
        let job = job();
        let tool = end_mill(6.0);
        let square = rect(Point::new(10.0, 10.0), Point::new(60.0, 60.0)).expect("rect");
        let params = ProfileParams {
            cut_depth: 12.0,
            side: ProfileSide::Outside,
            wall_angle: 10.0,
            scallop: 0.05,
            ..ProfileParams::default()
        };
        let (done, _) = tapered(&[square], &job, &tool, &params, tol()).expect("cuts");
        let surface = job.surface_z();
        let cutting: Vec<&Move> = done
            .moves
            .iter()
            .filter(|m| m.to.z < surface - 1e-9)
            .collect();
        // The last cutting move is at the foot.
        let last = cutting.last().expect("moves");
        assert!((last.to.z - (surface - 12.0)).abs() < 1e-9);
        let right = |depth: f64| {
            cutting
                .iter()
                .filter(|m| (surface - m.to.z - depth).abs() < 1e-9)
                .map(|m| m.to.x)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        // The shallowest level puts the wall 12 x tan 10 in from the
        // square; the foot pass runs round the square itself.
        let shallowest = cutting
            .iter()
            .map(|m| surface - m.to.z)
            .fold(f64::INFINITY, f64::min);
        let tan = 10f64.to_radians().tan();
        let top = right(shallowest);
        assert!((top - (63.0 - 12.0 * tan)).abs() < 1e-6, "{top}");
        assert!((right(12.0) - 63.0).abs() < 1e-6, "{}", right(12.0));
    }

    #[test]
    fn a_v_bit_at_the_angle_cuts_the_usual_profile_and_others_are_refused() {
        let job = job();
        let square = rect(Point::new(10.0, 10.0), Point::new(60.0, 60.0)).expect("rect");
        let params = ProfileParams {
            cut_depth: 6.0,
            wall_angle: 30.0,
            ..ProfileParams::default()
        };
        let v = Tool {
            name: "60 V".into(),
            tool_type: ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 60.0,
            },
            pass_depth: 6.0,
            ..Tool::default()
        };
        let (tapered_cut, _) =
            tapered(std::slice::from_ref(&square), &job, &v, &params, tol()).expect("cuts");
        let upright = ProfileParams {
            wall_angle: 0.0,
            ..params.clone()
        };
        let plain =
            profile(std::slice::from_ref(&square), &job, &v, &upright, tol()).expect("cuts");
        assert_eq!(tapered_cut.moves, plain.moves);
        let wrong = Tool {
            tool_type: ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
            ..v.clone()
        };
        assert!(tapered(std::slice::from_ref(&square), &job, &wrong, &params, tol()).is_err());
        let ball = Tool {
            tool_type: ToolGeometry::BallNose { diameter: 6.0 },
            ..v
        };
        assert!(tapered(&[square], &job, &ball, &params, tol()).is_err());
    }
}
