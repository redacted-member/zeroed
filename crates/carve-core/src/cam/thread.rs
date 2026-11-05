//! Thread milling toolpaths: a thread mill follows a helix one pitch per
//! turn round the centre of each selected circle, cutting an internal
//! thread in a hole or an external thread on a boss.
//!
//! Geometry follows the ISO metric and unified thread form: a basic
//! triangle of height `H = sqrt(3) / 2 * P` for pitch `P`. An internal
//! thread is cut to the major diameter, so the cutter's teeth reach
//! `D / 2` from the centre; an external thread is cut to its minor diameter,
//! `D - 2 * 17 / 24 * H`. A positive fit tolerance cuts that much deeper
//! into the part. The cutter's diameter is measured over its teeth.
//!
//! The helix is cut from the start depth down to the thread length, or up
//! from the bottom, turning clockwise seen from above when a right hand
//! thread is cut downwards (and the other way round for going up or a left
//! hand thread). The cutter moves straight out from the hole's centre to
//! the helix (or straight in from clear of a boss) and back the same way.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{bulge_from_sweep, path_bbox, Path, Point};

use super::chain::CutSeg;
use super::common::{order_nearest, Emitter, Heights};
use super::profile::Calculated;
use super::tool::{Tool, ToolGeometry};

/// Height of the basic thread triangle for a pitch of one.
const BASIC_HEIGHT: f64 = 0.866_025_403_784_438_6;

/// Internal (in a hole) or external (on a boss).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadKind {
    /// A female thread inside a hole.
    #[default]
    Internal,
    /// A male thread round a boss.
    External,
}

/// Which way the thread's helix winds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadHand {
    /// A right hand thread, tightened clockwise.
    #[default]
    Right,
    /// A left hand thread.
    Left,
}

/// Whether the helix is cut from the top down or from the bottom up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadCut {
    /// From the start depth down.
    #[default]
    Down,
    /// From the bottom up.
    Up,
}

/// The Thread Milling form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadParams {
    /// Depth the thread starts at below the surface.
    pub start_depth: f64,
    /// Deepest the cutter goes below the start depth.
    pub max_depth: f64,
    /// Distance from one crest to the next.
    pub pitch: f64,
    /// Major (crest) diameter of the thread.
    pub diameter: f64,
    /// How much deeper than the basic form the thread is cut.
    pub fit_tolerance: f64,
    /// Internal or external.
    pub kind: ThreadKind,
    /// Right or left hand.
    pub hand: ThreadHand,
    /// Down from the top or up from the bottom.
    pub cut: ThreadCut,
    /// Thread the circles in the order they were selected; otherwise the
    /// nearest next.
    pub selection_order: bool,
}

impl Default for ThreadParams {
    fn default() -> Self {
        ThreadParams {
            start_depth: 0.0,
            max_depth: 10.0,
            pitch: 1.5,
            diameter: 10.0,
            fit_tolerance: 0.0,
            kind: ThreadKind::Internal,
            hand: ThreadHand::Right,
            cut: ThreadCut::Down,
            selection_order: false,
        }
    }
}

impl ThreadParams {
    /// Minor diameter of the internal thread: the hole it needs.
    pub fn internal_minor(&self) -> f64 {
        self.diameter - 2.0 * 5.0 / 8.0 * BASIC_HEIGHT * self.pitch
    }

    /// Minor diameter of the external thread, where the cutter's teeth
    /// reach on a boss.
    pub fn external_minor(&self) -> f64 {
        self.diameter - 2.0 * 17.0 / 24.0 * BASIC_HEIGHT * self.pitch
    }

    /// Radius of the helix the cutter's axis follows, for a cutter
    /// `cutter` millimetres across its teeth.
    pub fn path_radius(&self, cutter: f64) -> f64 {
        match self.kind {
            ThreadKind::Internal => self.diameter / 2.0 + self.fit_tolerance - cutter / 2.0,
            ThreadKind::External => self.external_minor() / 2.0 - self.fit_tolerance + cutter / 2.0,
        }
    }

    /// True when the helix turns counter clockwise seen from above.
    fn counter_clockwise(&self) -> bool {
        (self.hand == ThreadHand::Right) == (self.cut == ThreadCut::Up)
    }
}

/// Calculates a thread milling toolpath round the centre of every closed
/// vector in `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not a thread mill,
/// sizes or depths that make no sense, a cutter too big for the hole, or
/// when no closed vector is selected.
pub fn thread_mill(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &ThreadParams,
) -> Result<Calculated, CoreError> {
    let (cutter, tool_pitch, tooth_offset, teeth) = match tool.tool_type {
        ToolGeometry::ThreadMill {
            diameter,
            pitch,
            tooth_offset,
            teeth,
            ..
        } => (diameter, pitch, tooth_offset.max(0.0), teeth.max(1)),
        _ => {
            return Err(CoreError::validation(
                "thread milling needs a thread milling tool",
            ))
        }
    };
    if !(params.pitch.is_finite() && params.pitch > 0.0) {
        return Err(CoreError::validation("the pitch must be more than zero"));
    }
    if !(params.diameter.is_finite() && params.diameter > 0.0) {
        return Err(CoreError::validation(
            "the thread diameter must be more than zero",
        ));
    }
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !(params.max_depth.is_finite() && params.max_depth > 0.0) {
        return Err(CoreError::validation(
            "the maximum depth must be more than zero",
        ));
    }
    if !params.fit_tolerance.is_finite() {
        return Err(CoreError::validation("the fit tolerance must be a number"));
    }
    let radius = params.path_radius(cutter);
    let mut warnings = Vec::new();
    if params.kind == ThreadKind::Internal {
        if cutter >= params.internal_minor() - 1e-9 || radius <= 1e-9 {
            return Err(CoreError::validation(
                "the cutter is too big to fit in the hole the thread needs",
            ));
        }
    } else if params.external_minor() <= 0.0 {
        return Err(CoreError::validation(
            "the pitch is too coarse for the thread diameter",
        ));
    }
    let length = thread_length(params.max_depth, tooth_offset);
    if length <= 1e-9 {
        return Err(CoreError::validation(
            "the tool's tooth offset is as deep as the maximum depth, which leaves no thread",
        ));
    }
    let pitch_differs = tool_pitch.is_finite()
        && tool_pitch > 0.0
        && (tool_pitch - params.pitch).abs() > 1e-3 * params.pitch.max(1.0);
    if pitch_differs && teeth > 1 {
        return Err(CoreError::validation(format!(
            "a multi point cutter only cuts its own pitch ({tool_pitch:.3} mm); pick a thread of that pitch or a single point cutter"
        )));
    }
    if pitch_differs {
        warnings.push(crate::warning::Warning::thread_pitch_differs(
            tool_pitch,
            params.pitch,
        ));
    }

    let mut centres: Vec<Point> = Vec::new();
    for path in paths {
        for sub in path.subpaths.iter().filter(|s| s.closed) {
            if let Some(bbox) = path_bbox(&sub.clone().into_path()) {
                centres.push(bbox.centre());
            }
        }
    }
    if centres.is_empty() {
        return Err(CoreError::validation(
            "select circles to thread round their centers",
        ));
    }

    let heights = Heights::from_job(job);
    let centres = if params.selection_order {
        centres
    } else {
        order_nearest(centres, heights.home.xy(), |p| *p, |p| *p)
    };
    let top = heights.z_at(params.start_depth);
    let direction = if params.counter_clockwise() {
        1.0
    } else {
        -1.0
    };
    let helices = helices(params, tooth_offset, teeth, length);
    let mut out = Emitter::new(heights);
    for centre in centres {
        for &(from_depth, to_depth) in &helices {
            let (from_z, to_z) = (heights.z_at(from_depth), heights.z_at(to_depth));
            let turns = (to_depth - from_depth).abs() / params.pitch;
            let start = centre.offset(crate::geom::Vec2::new(radius, 0.0));
            // Holes are entered from the centre; bosses from clear outside.
            let approach = match params.kind {
                ThreadKind::Internal => centre,
                ThreadKind::External => centre.offset(crate::geom::Vec2::new(radius + cutter, 0.0)),
            };
            out.travel_to(approach);
            out.plunge_to(top);
            out.plunge_to(from_z);
            out.cut(
                &CutSeg {
                    from: approach,
                    to: start,
                    bulge: 0.0,
                },
                from_z,
            );
            helix(
                &mut out,
                centre,
                radius,
                direction * turns * std::f64::consts::TAU,
                from_z,
                to_z,
            );
            let end = out.at().xy();
            let leave = match params.kind {
                ThreadKind::Internal => centre,
                ThreadKind::External => {
                    let out_dir = centre
                        .to(end)
                        .normalised()
                        .unwrap_or(crate::geom::Vec2::new(1.0, 0.0));
                    end.offset(out_dir.scaled(cutter))
                }
            };
            out.cut(
                &CutSeg {
                    from: end,
                    to: leave,
                    bulge: 0.0,
                },
                to_z,
            );
        }
        out.retract();
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// How much of the thread a cutter reaches: the maximum depth less the
/// height of its lowest tooth above its bottom, in millimetres.
pub fn thread_length(max_depth: f64, tooth_offset: f64) -> f64 {
    max_depth - tooth_offset.max(0.0)
}

/// The helices each thread is cut in, as depths of the cutter's bottom
/// below the surface at their start and end, in cutting order. A single
/// point cutter spirals the whole thread length in one helix. A multi
/// point cutter's teeth cut `teeth` turns at once, so it goes down to each
/// band of the thread and turns once there, rising or falling one pitch.
fn helices(params: &ThreadParams, tooth_offset: f64, teeth: u32, length: f64) -> Vec<(f64, f64)> {
    let top = params.start_depth + tooth_offset;
    let bottom = params.start_depth + params.max_depth;
    let mut out = if teeth <= 1 {
        vec![(top, bottom)]
    } else {
        let band = f64::from(teeth) * params.pitch;
        let count = ((length / band) - 1e-9).ceil().max(1.0) as usize;
        (0..count)
            .map(|k| {
                // The lowest tooth ends the turn this far down the thread.
                let reach = (band * (k + 1) as f64).min(length);
                let end = params.start_depth + reach + tooth_offset;
                (end - params.pitch, end)
            })
            .collect()
    };
    if params.cut == ThreadCut::Up {
        out.reverse();
        for helix in &mut out {
            *helix = (helix.1, helix.0);
        }
    }
    out
}

/// Cuts a helix round `centre` at `radius` from where the tool is (on the
/// circle at angle zero), turning `sweep` radians (positive counter
/// clockwise) while going from `from_z` to `to_z`, in spans of at most a
/// half turn.
fn helix(out: &mut Emitter, centre: Point, radius: f64, sweep: f64, from_z: f64, to_z: f64) {
    let spans = (sweep.abs() / std::f64::consts::PI).ceil().max(1.0) as usize;
    let step = sweep / spans as f64;
    let at = |angle: f64| {
        Point::new(
            centre.x + radius * angle.cos(),
            centre.y + radius * angle.sin(),
        )
    };
    for k in 0..spans {
        let (a0, a1) = (step * k as f64, step * (k + 1) as f64);
        let z = from_z + (to_z - from_z) * (k + 1) as f64 / spans as f64;
        out.cut(
            &CutSeg {
                from: at(a0),
                to: at(a1),
                bulge: bulge_from_sweep(step),
            },
            z,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::circle;

    fn mill(diameter: f64, pitch: f64) -> Tool {
        Tool {
            name: "Thread mill".to_string(),
            tool_type: ToolGeometry::ThreadMill {
                diameter,
                pitch,
                included_angle: 60.0,
                tooth_offset: 0.0,
                teeth: 1,
            },
            ..Tool::default()
        }
    }

    fn hole() -> Path {
        circle(Point::new(50.0, 40.0), 4.25).expect("circle")
    }

    fn arcs(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind.is_arc()).collect()
    }

    #[test]
    fn an_internal_thread_is_one_helix_a_pitch_per_turn_to_the_major_diameter() {
        let params = ThreadParams {
            max_depth: 6.0,
            ..ThreadParams::default()
        };
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &params).expect("thread");
        assert!(done.warnings.is_empty(), "{:?}", done.warnings);
        let helix = arcs(&done.moves);
        // Four turns, two half turns each.
        assert_eq!(helix.len(), 8);
        for m in &helix {
            let r = m.to.xy().distance(Point::new(50.0, 40.0));
            assert!((r - 2.0).abs() < 1e-9, "{r}");
            // Right hand, cut downwards: clockwise.
            assert!(matches!(m.kind, MoveKind::ArcCw { .. }));
        }
        // Half a pitch down each half turn, ending at the depth.
        assert!((helix[0].to.z + 0.75).abs() < 1e-9);
        assert!((helix[7].to.z + 6.0).abs() < 1e-9);
        assert!(done.moves.iter().all(|m| m.to.z >= -6.0 - 1e-9));
    }

    #[test]
    fn upward_or_left_hand_turn_the_other_way() {
        for (hand, cut) in [
            (ThreadHand::Right, ThreadCut::Up),
            (ThreadHand::Left, ThreadCut::Down),
        ] {
            let params = ThreadParams {
                hand,
                cut,
                ..ThreadParams::default()
            };
            let done = thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &params)
                .expect("thread");
            assert!(arcs(&done.moves)
                .iter()
                .all(|m| matches!(m.kind, MoveKind::ArcCcw { .. })));
        }
        let up = ThreadParams {
            cut: ThreadCut::Up,
            ..ThreadParams::default()
        };
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &up).expect("thread");
        let helix = arcs(&done.moves);
        assert!(helix[0].to.z < helix[helix.len() - 1].to.z, "rises");
    }

    #[test]
    fn an_external_thread_runs_outside_at_the_minor_diameter() {
        let params = ThreadParams {
            kind: ThreadKind::External,
            fit_tolerance: 0.1,
            ..ThreadParams::default()
        };
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &params).expect("thread");
        let want = params.external_minor() / 2.0 - 0.1 + 3.0;
        for m in arcs(&done.moves) {
            let r = m.to.xy().distance(Point::new(50.0, 40.0));
            assert!((r - want).abs() < 1e-9, "{r} {want}");
        }
        // It comes in from clear of the boss, at the helix's height.
        let first_feed = done
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Feed)
            .expect("a feed");
        assert!((first_feed.to.xy().distance(Point::new(50.0, 40.0)) - want).abs() < 1e-9);
    }

    #[test]
    fn a_cutter_too_big_or_the_wrong_tool_is_refused() {
        let params = ThreadParams::default();
        assert!(thread_mill(&[hole()], &JobSetup::default(), &mill(9.0, 1.5), &params).is_err());
        assert!(thread_mill(&[hole()], &JobSetup::default(), &Tool::default(), &params).is_err());
        let open = {
            let mut sub = crate::geom::SubPath::open(Point::ORIGIN);
            sub.line_to(Point::new(10.0, 0.0));
            sub.into_path()
        };
        assert!(thread_mill(&[open], &JobSetup::default(), &mill(6.0, 1.5), &params).is_err());
    }

    #[test]
    fn a_pitch_the_tool_does_not_have_is_warned_about() {
        let params = ThreadParams {
            pitch: 1.25,
            ..ThreadParams::default()
        };
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &params).expect("thread");
        assert_eq!(done.warnings.len(), 1);
    }

    #[test]
    fn a_fraction_of_a_turn_ends_on_the_depth() {
        let params = ThreadParams {
            max_depth: 5.0,
            ..ThreadParams::default()
        };
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &mill(6.0, 1.5), &params).expect("thread");
        let helix = arcs(&done.moves);
        assert!((helix[helix.len() - 1].to.z + 5.0).abs() < 1e-9);
        // 5 / 1.5 turns is 3.33, so 7 half turn spans.
        assert_eq!(helix.len(), 7);
    }

    #[test]
    fn settings_read_with_defaults() {
        let params: ThreadParams =
            serde_json::from_value(serde_json::json!({ "pitch": 2.0, "kind": "external" }))
                .expect("parse");
        assert_eq!(params.pitch, 2.0);
        assert_eq!(params.kind, ThreadKind::External);
        assert_eq!(params.hand, ThreadHand::Right);
    }

    fn multi(teeth: u32, tooth_offset: f64) -> Tool {
        let mut tool = mill(6.0, 1.5);
        tool.tool_type = ToolGeometry::ThreadMill {
            diameter: 6.0,
            pitch: 1.5,
            included_angle: 60.0,
            tooth_offset,
            teeth,
        };
        tool
    }

    /// Lowest Z the cutter's bottom reaches and the full turns cut.
    fn reach_and_turns(moves: &[Move]) -> (f64, f64) {
        let lowest = moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        let mut from = crate::cam::toolpath::Point3::default();
        let mut sweep = 0.0;
        for m in moves {
            if let Some(s) = m.arc_sweep(from) {
                sweep += s.abs();
            }
            from = m.to;
        }
        (lowest, sweep / std::f64::consts::TAU)
    }

    #[test]
    fn a_tooth_offset_shortens_the_thread_above_the_max_depth() {
        assert!((thread_length(10.0, 2.0) - 8.0).abs() < 1e-12);
        let params = ThreadParams::default();
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &multi(1, 2.0), &params).expect("threads");
        let (lowest, turns) = reach_and_turns(&done.moves);
        assert!(
            (lowest + 10.0).abs() < 1e-9,
            "the cutter still goes to the max depth"
        );
        assert!((turns - 8.0 / 1.5).abs() < 1e-6, "{turns}");
        let too_deep = multi(1, 10.0);
        assert!(thread_mill(&[hole()], &JobSetup::default(), &too_deep, &params).is_err());
    }

    #[test]
    fn a_multi_point_cutter_turns_once_per_band_of_teeth() {
        let params = ThreadParams::default();
        // Four teeth cover 6 mm of a 10 mm thread: two bands, one turn each.
        let done =
            thread_mill(&[hole()], &JobSetup::default(), &multi(4, 0.0), &params).expect("threads");
        let (lowest, turns) = reach_and_turns(&done.moves);
        assert!((lowest + 10.0).abs() < 1e-9);
        assert!((turns - 2.0).abs() < 1e-6, "{turns}");
        // A multi point cutter cannot cut another pitch.
        let other = ThreadParams {
            pitch: 1.0,
            diameter: 10.0,
            ..params
        };
        assert!(thread_mill(&[hole()], &JobSetup::default(), &multi(4, 0.0), &other).is_err());
    }

    #[test]
    fn circles_are_threaded_in_selection_order_when_asked() {
        let far = circle(Point::new(90.0, 40.0), 4.25).expect("circle");
        let near = circle(Point::new(10.0, 10.0), 4.25).expect("circle");
        let first_x = |selection_order| {
            let params = ThreadParams {
                selection_order,
                ..ThreadParams::default()
            };
            thread_mill(
                &[far.clone(), near.clone()],
                &JobSetup::default(),
                &mill(6.0, 1.5),
                &params,
            )
            .expect("threads")
            .moves
            .iter()
            .find(|m| m.kind.is_arc())
            .map(|m| m.to.x)
            .expect("an arc")
        };
        assert!(first_x(false) < 30.0);
        assert!(first_x(true) > 70.0);
    }
}
