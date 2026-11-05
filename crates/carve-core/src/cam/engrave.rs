//! Quick Engrave toolpaths: an engraving or drag tool runs along the
//! vectors themselves (no offset), and can also fill closed shapes with
//! back and forth lines. Depth is reached in a set number of equal passes.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{Path, Region, Tolerance};

use super::chain::Chain;
use super::common::{order_nearest_either_way, Emitter, Heights};
use super::pocket::raster_lines;
use super::profile::Calculated;
use super::tool::ToolGeometry;
use super::toolpath::{StrategyKind, Toolpath};

/// Outline only, or outline plus a raster fill of closed shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngraveMode {
    /// Along the vectors.
    #[default]
    Outline,
    /// Along the vectors and back and forth inside closed ones.
    Fill,
}

/// How a fill covers the inside of closed shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillPattern {
    /// Back and forth lines at the angle.
    #[default]
    Raster,
    /// Lines at the angle, then again at right angles to it.
    CrossHatch,
    /// Rings a stepover apart, following the outline inwards.
    Offset,
}

/// The Quick Engrave form's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngraveParams {
    /// Final depth below the surface, in millimetres.
    pub depth: f64,
    /// Equal passes taken to reach it.
    pub passes: usize,
    /// Outline or fill.
    pub mode: EngraveMode,
    /// Distance between fill lines, in millimetres.
    pub stepover: f64,
    /// Fill line angle in degrees from the X axis.
    pub angle: f64,
    /// Raster, cross hatch or offset fill.
    pub pattern: FillPattern,
    /// A sprung nose cone holds the depth: every pass runs at the full
    /// depth instead of stepping down to it.
    pub nose_cone: bool,
}

impl Default for EngraveParams {
    fn default() -> Self {
        EngraveParams {
            depth: 0.5,
            passes: 1,
            mode: EngraveMode::Outline,
            stepover: 0.3,
            angle: 0.0,
            pattern: FillPattern::Raster,
            nose_cone: false,
        }
    }
}

/// Calculates a Quick Engrave toolpath on `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a depth, pass count or stepover
/// that makes no sense, or when there is nothing to engrave.
pub fn engrave(
    paths: &[Path],
    job: &JobSetup,
    params: &EngraveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    if !params.depth.is_finite() || params.depth <= 0.0 {
        return Err(CoreError::validation(
            "the engraving depth must be more than zero",
        ));
    }
    if params.passes == 0 || params.passes > 100 {
        return Err(CoreError::validation(
            "the number of passes must be from 1 to 100",
        ));
    }
    let mut chains: Vec<Chain> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .map(|sub| Chain::from_subpath(sub, tol))
        .filter(|c| !c.is_empty())
        .collect();
    if params.mode == EngraveMode::Fill {
        if !params.stepover.is_finite() || params.stepover <= 0.0 {
            return Err(CoreError::validation(
                "the fill stepover must be more than zero",
            ));
        }
        let region = Region::from_paths(paths, tol);
        match params.pattern {
            FillPattern::Raster => {
                chains.extend(raster_lines(&region, params.stepover, params.angle, tol));
            }
            FillPattern::CrossHatch => {
                chains.extend(raster_lines(&region, params.stepover, params.angle, tol));
                chains.extend(raster_lines(
                    &region,
                    params.stepover,
                    params.angle + 90.0,
                    tol,
                ));
            }
            FillPattern::Offset => {
                let mut ring = region.offset(params.stepover, tol);
                let mut count = 0;
                while !ring.is_empty() && count < 10_000 {
                    chains.extend(
                        ring.loops
                            .iter()
                            .map(|sub| Chain::from_subpath(sub, tol))
                            .filter(|c| !c.is_empty()),
                    );
                    ring = ring.offset(params.stepover, tol);
                    count += 1;
                }
            }
        }
    }
    if chains.is_empty() {
        return Err(CoreError::validation("select the vectors to engrave"));
    }
    let heights = Heights::from_job(job);
    let home = heights.home.xy();
    // Open cuts can be entered from either end; loops start where they start.
    let chains = order_nearest_either_way(
        chains,
        home,
        |c| c.start().unwrap_or(home),
        |c| c.end().unwrap_or(home),
        |c| if c.closed { c } else { c.reversed() },
    );
    let mut out = Emitter::new(heights);
    for pass in 1..=params.passes {
        let z = if params.nose_cone {
            heights.z_at(params.depth)
        } else {
            heights.z_at(params.depth * pass as f64 / params.passes as f64)
        };
        for chain in &chains {
            let Some(begin) = chain.start() else {
                continue;
            };
            out.travel_to(begin);
            out.plunge_to(z);
            out.cut_flat(&chain.segs, z);
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

/// Depth a diamond drag point of `angle` degrees leaves when its mark is
/// `line_width` millimetres wide: the half width over the tangent of the
/// half angle, so a 90 degree point marks half as deep as it is wide.
pub fn diamond_mark_depth(line_width: f64, angle: f64) -> Option<f64> {
    let half = (angle.clamp(1.0, 179.0) / 2.0).to_radians();
    let depth = line_width / 2.0 / half.tan();
    (depth.is_finite() && depth > 0.0).then_some(depth)
}

/// The Quick Engrave toolpath as the preview cuts it. A diamond drag tool
/// is pressed into the surface by the Pressure (Z), which the output keeps,
/// but the mark it leaves is only as deep as its line width and point angle
/// allow, so the preview scales every cut below the surface to that depth.
/// With the nose cone the depth is real and the toolpath is left alone, as
/// is every other tool, strategy and a toolpath projected onto the model.
pub fn preview_view(toolpath: &Toolpath, surface_z: f64) -> Option<Toolpath> {
    if toolpath.kind != StrategyKind::QuickEngrave {
        return None;
    }
    let ToolGeometry::DiamondDrag { diameter, angle } = toolpath.tool.tool_type else {
        return None;
    };
    let record = toolpath.params_json.get("params")?;
    if record.get("projectToModel").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    let params: EngraveParams = serde_json::from_value(record.clone()).ok()?;
    if params.nose_cone || params.depth <= 0.0 {
        return None;
    }
    let mark = diamond_mark_depth(diameter, angle)?;
    let ratio = mark / params.depth;
    let mut out = toolpath.clone();
    for one in &mut out.moves {
        if one.to.z < surface_z {
            one.to.z = surface_z - (surface_z - one.to.z) * ratio;
        }
    }
    out.recompute_bounds();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::{rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn square() -> Path {
        rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")
    }

    fn cuts(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind == MoveKind::Feed).collect()
    }

    #[test]
    fn an_outline_follows_the_vector_itself_in_equal_passes() {
        let params = EngraveParams {
            depth: 1.0,
            passes: 2,
            ..EngraveParams::default()
        };
        let done = engrave(&[square()], &JobSetup::default(), &params, tol()).expect("engraves");
        let feeds = cuts(&done.moves);
        assert!(feeds
            .iter()
            .all(|m| (m.to.z + 0.5).abs() < 1e-9 || (m.to.z + 1.0).abs() < 1e-9));
        assert!(feeds.iter().any(|m| (m.to.z + 0.5).abs() < 1e-9));
        assert!(feeds
            .iter()
            .all(|m| m.to.x == 0.0 || m.to.x == 10.0 || m.to.y == 0.0 || m.to.y == 10.0));
    }

    #[test]
    fn a_fill_adds_lines_inside_closed_shapes() {
        let params = EngraveParams {
            mode: EngraveMode::Fill,
            stepover: 1.0,
            ..EngraveParams::default()
        };
        let done = engrave(&[square()], &JobSetup::default(), &params, tol()).expect("engraves");
        let inside = cuts(&done.moves)
            .iter()
            .filter(|m| m.to.y > 0.4 && m.to.y < 9.6)
            .count();
        assert!(inside >= 9, "fill lines {inside}");
    }

    #[test]
    fn bad_settings_are_refused() {
        let zero = EngraveParams {
            depth: 0.0,
            ..EngraveParams::default()
        };
        assert!(engrave(&[square()], &JobSetup::default(), &zero, tol()).is_err());
        let none = EngraveParams {
            passes: 0,
            ..EngraveParams::default()
        };
        assert!(engrave(&[square()], &JobSetup::default(), &none, tol()).is_err());
        assert!(engrave(&[], &JobSetup::default(), &EngraveParams::default(), tol()).is_err());
    }

    fn fill(pattern: FillPattern) -> Vec<Move> {
        let params = EngraveParams {
            mode: EngraveMode::Fill,
            stepover: 1.0,
            pattern,
            ..EngraveParams::default()
        };
        engrave(&[square()], &JobSetup::default(), &params, tol())
            .expect("engrave")
            .moves
    }

    fn lines_along(moves: &[Move], x: bool) -> usize {
        moves
            .windows(2)
            .filter(|w| {
                w[1].kind == MoveKind::Feed
                    && if x {
                        (w[0].to.y - w[1].to.y).abs() < 1e-9 && (w[0].to.x - w[1].to.x).abs() > 5.0
                    } else {
                        (w[0].to.x - w[1].to.x).abs() < 1e-9 && (w[0].to.y - w[1].to.y).abs() > 5.0
                    }
            })
            .count()
    }

    #[test]
    fn a_cross_hatch_fills_both_ways_and_offset_fills_with_rings() {
        let raster = fill(FillPattern::Raster);
        assert!(lines_along(&raster, true) >= 8);
        assert_eq!(lines_along(&raster, false), 2, "only the outline's sides");
        let cross = fill(FillPattern::CrossHatch);
        assert!(lines_along(&cross, true) >= 8 && lines_along(&cross, false) >= 8);
        let offset = fill(FillPattern::Offset);
        // Rings at 1, 2, 3 and 4 mm in: each has sides both ways.
        let inner = offset
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && (m.to.x - 4.0).abs() < 1e-6)
            .count();
        assert!(inner >= 1, "the ring 4 mm in");
        assert!(lines_along(&offset, false) >= 6);
    }

    #[test]
    fn a_nose_cone_runs_every_pass_at_full_depth() {
        let params = EngraveParams {
            depth: 0.3,
            passes: 3,
            nose_cone: true,
            ..EngraveParams::default()
        };
        let moves = engrave(&[square()], &JobSetup::default(), &params, tol())
            .expect("engrave")
            .moves;
        let plunges: Vec<f64> = moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| m.to.z)
            .collect();
        assert_eq!(plunges.len(), 3);
        assert!(
            plunges.iter().all(|z| (z + 0.3).abs() < 1e-9),
            "{plunges:?}"
        );
    }

    fn drag_toolpath(nose_cone: bool, tool: ToolGeometry) -> Toolpath {
        let params = EngraveParams {
            depth: 0.4,
            passes: 2,
            nose_cone,
            ..EngraveParams::default()
        };
        let done = engrave(&[square()], &JobSetup::default(), &params, tol()).expect("engrave");
        let mut tp = Toolpath::new(
            1,
            "drag",
            crate::cam::tool::Tool {
                name: "d".into(),
                tool_type: tool,
                ..crate::cam::tool::Tool::default()
            },
            StrategyKind::QuickEngrave,
        );
        tp.moves = done.moves;
        tp.params_json = serde_json::json!({ "strategy": "quickEngrave", "params": params });
        tp
    }

    fn lowest(tp: &Toolpath) -> f64 {
        tp.moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min)
    }

    #[test]
    fn a_diamond_drag_previews_the_depth_of_its_line_width() {
        assert!((diamond_mark_depth(0.25, 90.0).expect("depth") - 0.125).abs() < 1e-9);
        assert!(diamond_mark_depth(0.25, 60.0).expect("depth") > 0.2);
        let drag = ToolGeometry::DiamondDrag {
            diameter: 0.25,
            angle: 90.0,
        };
        let tp = drag_toolpath(false, drag);
        assert!(
            (lowest(&tp) + 0.4).abs() < 1e-9,
            "the output keeps the pressure"
        );
        let seen = preview_view(&tp, 0.0).expect("a drag preview");
        assert!((lowest(&seen) + 0.125).abs() < 1e-9, "{}", lowest(&seen));
        // The first of two passes previews half as deep, rapids stay put.
        let first = seen
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| m.to.z)
            .fold(f64::MIN, f64::max);
        assert!((first + 0.0625).abs() < 1e-9, "{first}");
        assert_eq!(
            seen.moves
                .iter()
                .filter(|m| m.kind == MoveKind::Rapid)
                .count(),
            tp.moves
                .iter()
                .filter(|m| m.kind == MoveKind::Rapid)
                .count()
        );
    }

    #[test]
    fn a_nose_cone_or_a_cutting_tool_previews_the_real_depth() {
        let drag = ToolGeometry::DiamondDrag {
            diameter: 0.25,
            angle: 90.0,
        };
        assert!(preview_view(&drag_toolpath(true, drag), 0.0).is_none());
        let bit = ToolGeometry::VBit {
            diameter: 6.0,
            included_angle: 90.0,
        };
        assert!(preview_view(&drag_toolpath(false, bit), 0.0).is_none());
    }
}
