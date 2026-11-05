//! Drilling toolpaths: a hole at the centre of each closed vector, cut
//! straight down in one go or in pecks, with an optional dwell at the bottom
//! of each pass.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{path_bbox, Path, Point};

use super::common::{order_nearest, Emitter, Heights};
use super::profile::Calculated;

/// Most clearance kept above the bottom of the previous peck when the drill
/// rapids back down for the next one, in millimetres.
const PECK_RETURN_GAP: f64 = 0.5;

/// Retract Gap (R) of a toolpath saved without one, in millimetres: the
/// fixed gap such toolpaths were cut with. The form starts new ones at 0.
pub const DEFAULT_RETRACT_GAP: f64 = 1.0;

/// The Drilling form's settings. Depths are millimetres below the surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DrillParams {
    /// Depth the hole starts at.
    pub start_depth: f64,
    /// Depth of the hole.
    pub cut_depth: f64,
    /// Depth of each peck (the tool's pass depth); zero drills in one plunge.
    pub peck_depth: f64,
    /// Between pecks, lift to the Retract Gap above the start depth.
    /// Otherwise the drill lifts the Retract Gap above the previous pass.
    pub full_retract: bool,
    /// Retract Gap (R) in millimetres: how far above the start depth, or
    /// above the previous pass, the drill lifts between pecks.
    pub retract_gap: f64,
    /// Seconds to wait at the bottom of each drill pass.
    pub dwell: f64,
    /// Cut the vectors in the order they were selected instead of the
    /// nearest next (Use Vector Selection Order).
    pub selection_order: bool,
}

impl Default for DrillParams {
    fn default() -> Self {
        DrillParams {
            start_depth: 0.0,
            cut_depth: 10.0,
            peck_depth: 0.0,
            full_retract: true,
            retract_gap: DEFAULT_RETRACT_GAP,
            dwell: 0.0,
            selection_order: false,
        }
    }
}

/// Calculates a drilling toolpath with a hole at the centre of every closed
/// vector in `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for bad depths or when there are no
/// closed vectors to drill.
pub fn drill(
    paths: &[Path],
    job: &JobSetup,
    params: &DrillParams,
) -> Result<Calculated, CoreError> {
    check_params(params)?;
    let mut warnings = Vec::new();
    let mut holes: Vec<Point> = Vec::new();
    for path in paths {
        for sub in &path.subpaths {
            if !sub.closed {
                continue;
            }
            if let Some(bbox) = path_bbox(&sub.clone().into_path()) {
                holes.push(bbox.centre());
            }
        }
    }
    if paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .any(|s| !s.closed)
    {
        warnings.push(crate::warning::Warning::drill_open_vectors());
    }
    if holes.is_empty() {
        return Err(CoreError::validation(
            "select closed vectors, such as circles, to drill at their centers",
        ));
    }
    let heights = Heights::from_job(job);
    let holes = if params.selection_order {
        holes
    } else {
        order_nearest(holes, heights.home.xy(), |p| *p, |p| *p)
    };
    let bottom = heights.z_at(params.cut_depth);
    let top = heights.z_at(params.start_depth);
    let mut out = Emitter::new(heights);
    for hole in holes {
        drill_hole(&mut out, hole, top, bottom, params);
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// Drills one hole at `hole` from job Z `top` down to `bottom`, in one
/// plunge or in pecks with `params`' retract gap and dwell, and lifts out.
pub(crate) fn drill_hole(
    out: &mut Emitter,
    hole: Point,
    top: f64,
    bottom: f64,
    params: &DrillParams,
) {
    out.travel_to(hole);
    out.plunge_to(top);
    if params.peck_depth <= 0.0 {
        out.plunge_to(bottom);
    } else {
        let mut reached = top;
        while reached > bottom + 1e-9 {
            let next = (reached - params.peck_depth).max(bottom);
            out.plunge_to(next);
            out.dwell(params.dwell);
            reached = next;
            if reached > bottom + 1e-9 {
                // Clear the chip, then come back down near the bottom.
                let lift = if params.full_retract { top } else { reached } + params.retract_gap;
                out.plunge_to(lift);
                out.rapid_down_to(reached + params.retract_gap.min(PECK_RETURN_GAP));
            }
        }
    }
    if params.peck_depth <= 0.0 {
        out.dwell(params.dwell);
    }
    out.retract();
}

/// Checks the depths and gaps of drilling settings.
///
/// # Errors
/// [`CoreError::Validation`] naming the setting that is wrong.
pub(crate) fn check_params(params: &DrillParams) -> Result<(), CoreError> {
    if !params.cut_depth.is_finite() || params.cut_depth <= params.start_depth {
        return Err(CoreError::validation(
            "the cut depth must be deeper than the start depth",
        ));
    }
    if !params.peck_depth.is_finite() || params.peck_depth < 0.0 {
        return Err(CoreError::validation("the peck depth must be zero or more"));
    }
    if !params.retract_gap.is_finite() || params.retract_gap < 0.0 {
        return Err(CoreError::validation(
            "the retract gap must be zero or more",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::{circle, rect};

    fn holes() -> Vec<Path> {
        vec![
            circle(Point::new(20.0, 20.0), 3.0).expect("circle"),
            circle(Point::new(60.0, 20.0), 3.0).expect("circle"),
        ]
    }

    #[test]
    fn each_closed_vector_gets_one_hole_at_its_centre() {
        let done = drill(&holes(), &JobSetup::default(), &DrillParams::default()).expect("drill");
        let bottoms: Vec<Point> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && (m.to.z + 10.0).abs() < 1e-9)
            .map(|m| m.to.xy())
            .collect();
        assert_eq!(
            bottoms,
            vec![Point::new(20.0, 20.0), Point::new(60.0, 20.0)]
        );
    }

    #[test]
    fn pecks_go_down_in_steps_and_lift_between_them() {
        let params = DrillParams {
            peck_depth: 4.0,
            ..DrillParams::default()
        };
        let one = [circle(Point::new(20.0, 20.0), 3.0).expect("circle")];
        let done = drill(&one, &JobSetup::default(), &params).expect("drill");
        let depths: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.z)
            .filter(|z| *z < 0.0)
            .collect();
        assert!(
            depths.contains(&-4.0) && depths.contains(&-8.0) && depths.contains(&-10.0),
            "{depths:?}"
        );
        let lifts = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && (m.to.z - DEFAULT_RETRACT_GAP).abs() < 1e-9)
            .count();
        assert_eq!(
            lifts, 2,
            "a lift to the gap above the start after each peck but the last"
        );
    }

    fn feed_lifts(moves: &[crate::cam::toolpath::Move]) -> Vec<f64> {
        moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| m.to.z)
            .collect()
    }

    #[test]
    fn the_retract_gap_is_measured_from_the_start_or_the_previous_pass() {
        let one = [circle(Point::new(20.0, 20.0), 3.0).expect("circle")];
        let above_start = DrillParams {
            start_depth: 2.0,
            cut_depth: 10.0,
            peck_depth: 4.0,
            retract_gap: 3.0,
            ..DrillParams::default()
        };
        let done = drill(&one, &JobSetup::default(), &above_start).expect("drill");
        assert_eq!(feed_lifts(&done.moves), vec![1.0]);
        let above_pass = DrillParams {
            full_retract: false,
            ..above_start
        };
        let done = drill(&one, &JobSetup::default(), &above_pass).expect("drill");
        assert_eq!(feed_lifts(&done.moves), vec![-3.0]);
        // Back down near the bottom of the last pass before the next peck.
        assert!(done
            .moves
            .iter()
            .any(|m| m.kind == MoveKind::Rapid && (m.to.z + 5.5).abs() < 1e-9));
        let bad = DrillParams {
            retract_gap: -1.0,
            ..above_start
        };
        assert!(drill(&one, &JobSetup::default(), &bad).is_err());
    }

    #[test]
    fn a_dwell_follows_each_drill_pass() {
        let params = DrillParams {
            peck_depth: 4.0,
            dwell: 0.5,
            ..DrillParams::default()
        };
        let one = [circle(Point::new(20.0, 20.0), 3.0).expect("circle")];
        let done = drill(&one, &JobSetup::default(), &params).expect("drill");
        let dwells: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| matches!(m.kind, MoveKind::Dwell { .. }))
            .map(|m| m.to.z)
            .collect();
        assert_eq!(dwells, vec![-4.0, -8.0, -10.0]);
    }

    #[test]
    fn a_dwell_is_added_at_the_bottom() {
        let params = DrillParams {
            dwell: 0.5,
            ..DrillParams::default()
        };
        let done = drill(&holes(), &JobSetup::default(), &params).expect("drill");
        let dwells = done
            .moves
            .iter()
            .filter(|m| matches!(m.kind, MoveKind::Dwell { .. }) && (m.to.z + 10.0).abs() < 1e-9)
            .count();
        assert_eq!(dwells, 2);
    }

    #[test]
    fn open_vectors_alone_cannot_be_drilled() {
        let mut sub = crate::geom::SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 0.0));
        assert!(drill(
            &[sub.into_path()],
            &JobSetup::default(),
            &DrillParams::default()
        )
        .is_err());
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let done = drill(&[square], &JobSetup::default(), &DrillParams::default()).expect("drill");
        assert!(done.moves.iter().any(|m| m.to.xy() == Point::new(5.0, 5.0)));
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
    fn use_vector_selection_order_drills_in_the_order_selected() {
        let holes = vec![
            circle(Point::new(100.0, 20.0), 3.0).expect("circle"),
            circle(Point::new(20.0, 20.0), 3.0).expect("circle"),
            circle(Point::new(60.0, 20.0), 3.0).expect("circle"),
        ];
        let by_x = |x: f64, _y: f64| (x / 40.0) as usize;
        let nearest = drill(&holes, &JobSetup::default(), &DrillParams::default()).expect("drill");
        assert_eq!(plunge_order(&nearest.moves, by_x), vec![0, 1, 2]);
        let params = DrillParams {
            selection_order: true,
            ..DrillParams::default()
        };
        let picked = drill(&holes, &JobSetup::default(), &params).expect("drill");
        assert_eq!(plunge_order(&picked.moves, by_x), vec![2, 0, 1]);
    }
}
