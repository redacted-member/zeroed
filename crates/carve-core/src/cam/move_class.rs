//! Sorts the moves of a toolpath into the kinds the views draw apart: rapids,
//! cuts, plunges, retracts and joins. A retract lifts the tool straight up; a
//! plunge takes it straight down at a feed rate; a join is a flat feed move
//! from the point where a contour closed to the start of the next one, which
//! offset pockets use instead of lifting between rings.

use super::toolpath::{MoveKind, Point3, Toolpath};

/// How far apart two points may be and still count as the same, in millimetres.
const SAME: f64 = 1e-6;

/// What a move looks like in the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveClass {
    /// Travel at the rapid rate that is not a straight lift.
    Rapid,
    /// A straight cutting move.
    Feed,
    /// A move straight down into the material at a feed rate.
    Plunge,
    /// An arc.
    Arc,
    /// A pause in place.
    Dwell,
    /// A move straight up out of the material.
    Retract,
    /// A flat feed move linking one closed contour to the next.
    Join,
}

fn same_xy(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() <= SAME && (a.y - b.y).abs() <= SAME
}

/// The class of every move of `toolpath`, in order, one per move.
pub fn classify(toolpath: &Toolpath) -> Vec<MoveClass> {
    let mut out = Vec::with_capacity(toolpath.moves.len());
    // Where the contour being cut began, and how many moves it has so far.
    let mut contour_start = toolpath.home;
    let mut contour_moves = 0usize;
    for (from, one) in toolpath.iter_moves() {
        let vertical = same_xy(from, one.to);
        let rising = one.to.z > from.z + SAME;
        let falling = one.to.z < from.z - SAME;
        let class = match one.kind {
            MoveKind::Dwell { .. } => MoveClass::Dwell,
            MoveKind::Rapid | MoveKind::Feed if vertical && rising => MoveClass::Retract,
            MoveKind::Rapid => MoveClass::Rapid,
            MoveKind::Plunge => MoveClass::Plunge,
            MoveKind::Feed if vertical && falling => MoveClass::Plunge,
            MoveKind::Feed
                if !rising
                    && !falling
                    && contour_moves >= 2
                    && same_xy(from, contour_start)
                    && (from.z - contour_start.z).abs() <= SAME =>
            {
                MoveClass::Join
            }
            MoveKind::Feed => MoveClass::Feed,
            MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. } => MoveClass::Arc,
        };
        match class {
            MoveClass::Feed | MoveClass::Arc => contour_moves += 1,
            MoveClass::Dwell => {}
            _ => {
                contour_start = one.to;
                contour_moves = 0;
            }
        }
        out.push(class);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{Move, StrategyKind};

    fn path(moves: Vec<Move>) -> Toolpath {
        let mut toolpath = Toolpath::new(1, "t", Tool::default(), StrategyKind::Profile);
        toolpath.home = Point3::new(0.0, 0.0, 10.0);
        toolpath.moves = moves;
        toolpath
    }

    #[test]
    fn lifts_are_retracts_and_straight_feeds_down_are_plunges() {
        let classes = classify(&path(vec![
            Move::rapid(Point3::new(5.0, 5.0, 10.0)),
            Move::rapid(Point3::new(5.0, 5.0, 2.0)),
            Move::feed(Point3::new(5.0, 5.0, -1.0)),
            Move::feed(Point3::new(20.0, 5.0, -1.0)),
            Move::rapid(Point3::new(20.0, 5.0, 10.0)),
        ]));
        assert_eq!(
            classes,
            vec![
                MoveClass::Rapid,
                MoveClass::Rapid,
                MoveClass::Plunge,
                MoveClass::Feed,
                MoveClass::Retract,
            ]
        );
    }

    #[test]
    fn a_flat_step_from_a_closed_ring_to_the_next_is_a_join() {
        let z = -2.0;
        let classes = classify(&path(vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, z)),
            Move::feed(Point3::new(10.0, 0.0, z)),
            Move::feed(Point3::new(10.0, 10.0, z)),
            Move::feed(Point3::new(0.0, 10.0, z)),
            Move::feed(Point3::new(0.0, 0.0, z)),
            // Back at the start of the ring: step in to the next one.
            Move::feed(Point3::new(2.0, 2.0, z)),
            Move::feed(Point3::new(8.0, 2.0, z)),
            Move::feed(Point3::new(8.0, 8.0, z)),
            Move::feed(Point3::new(2.0, 8.0, z)),
            Move::feed(Point3::new(2.0, 2.0, z)),
            Move::rapid(Point3::new(2.0, 2.0, 5.0)),
        ]));
        assert_eq!(classes[6], MoveClass::Join);
        assert_eq!(
            classes.iter().filter(|c| **c == MoveClass::Join).count(),
            1,
            "{classes:?}"
        );
        assert_eq!(classes[11], MoveClass::Retract);
        assert_eq!(classes[1], MoveClass::Plunge);
    }

    #[test]
    fn a_raster_stepover_is_a_cut() {
        let z = -1.0;
        let classes = classify(&path(vec![
            Move::plunge(Point3::new(0.0, 0.0, z)),
            Move::feed(Point3::new(10.0, 0.0, z)),
            Move::feed(Point3::new(10.0, 1.0, z)),
            Move::feed(Point3::new(0.0, 1.0, z)),
        ]));
        assert!(!classes.contains(&MoveClass::Join));
    }
}
