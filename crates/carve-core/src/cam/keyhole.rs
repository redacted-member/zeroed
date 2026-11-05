//! Keyhole toolpaths: hanging slots cut into the back of a sign with a
//! keyhole cutter. At the centre of each selected vector the tool plunges to
//! the slot depth, runs along the slot, comes back along the same line and
//! lifts out where it went in.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{path_bbox, Path, Point};

use super::chain::CutSeg;
use super::common::{order_nearest, Emitter, Heights};
use super::profile::Calculated;

/// Which way the slot runs from its entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SlotDirection {
    /// Vertical, from the entry point towards +Y.
    #[default]
    BottomToTop,
    /// Vertical, from the entry point towards -Y.
    TopToBottom,
    /// Horizontal, from the entry point towards +X.
    LeftToRight,
    /// Horizontal, from the entry point towards -X.
    RightToLeft,
}

impl SlotDirection {
    /// Unit step along the slot.
    fn step(self) -> (f64, f64) {
        match self {
            SlotDirection::BottomToTop => (0.0, 1.0),
            SlotDirection::TopToBottom => (0.0, -1.0),
            SlotDirection::LeftToRight => (1.0, 0.0),
            SlotDirection::RightToLeft => (-1.0, 0.0),
        }
    }
}

/// The Keyhole Toolpath settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyholeParams {
    /// Slot orientation.
    pub direction: SlotDirection,
    /// Depth of the slot below the material surface.
    pub depth: f64,
    /// How far the tool travels from the entry point along the slot.
    pub length: f64,
}

impl Default for KeyholeParams {
    fn default() -> Self {
        KeyholeParams {
            direction: SlotDirection::BottomToTop,
            depth: 6.35,
            length: 25.4,
        }
    }
}

/// The entry points: the centre of each vector's bounding box.
pub fn entry_points(paths: &[Path]) -> Vec<Point> {
    paths
        .iter()
        .filter_map(path_bbox)
        .map(|bbox| bbox.centre())
        .collect()
}

/// Calculates a keyhole toolpath with one slot per vector in `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a depth or length that is not more
/// than zero, or when there are no vectors to take entry points from.
pub fn keyhole(
    paths: &[Path],
    job: &JobSetup,
    params: &KeyholeParams,
) -> Result<Calculated, CoreError> {
    if !(params.depth.is_finite() && params.depth > 0.0) {
        return Err(CoreError::validation(
            "the slot depth must be more than zero",
        ));
    }
    if !(params.length.is_finite() && params.length > 0.0) {
        return Err(CoreError::validation(
            "the slot length must be more than zero",
        ));
    }
    let entries = entry_points(paths);
    if entries.is_empty() {
        return Err(CoreError::validation(
            "select one or more vectors: their centers are the entry points of the slots",
        ));
    }
    let heights = Heights::from_job(job);
    let entries = order_nearest(entries, heights.home.xy(), |p| *p, |p| *p);
    let bottom = heights.z_at(params.depth);
    let (dx, dy) = params.direction.step();
    let mut out = Emitter::new(heights);
    for entry in entries {
        let end = Point::new(entry.x + dx * params.length, entry.y + dy * params.length);
        out.travel_to(entry);
        out.plunge_to(bottom);
        out.cut(
            &CutSeg {
                from: entry,
                to: end,
                bulge: 0.0,
            },
            bottom,
        );
        out.cut(
            &CutSeg {
                from: end,
                to: entry,
                bulge: 0.0,
            },
            bottom,
        );
        out.retract();
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::circle;

    fn entries() -> Vec<Path> {
        vec![
            circle(Point::new(20.0, 30.0), 6.0).expect("circle"),
            circle(Point::new(80.0, 30.0), 6.0).expect("circle"),
        ]
    }

    fn params(direction: SlotDirection) -> KeyholeParams {
        KeyholeParams {
            direction,
            depth: 5.0,
            length: 12.0,
        }
    }

    #[test]
    fn each_slot_plunges_runs_out_comes_back_and_lifts_at_the_entry() {
        let job = JobSetup::default();
        let done = keyhole(&entries(), &job, &params(SlotDirection::BottomToTop)).expect("keyhole");
        let surface = Heights::from_job(&job).surface;
        let bottom = surface - 5.0;
        let plunges: Vec<_> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && (m.to.z - bottom).abs() < 1e-9)
            .map(|m| m.to.xy())
            .collect();
        assert_eq!(
            plunges,
            vec![Point::new(20.0, 30.0), Point::new(80.0, 30.0)]
        );
        let feeds: Vec<_> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| (m.to.xy(), m.to.z))
            .collect();
        assert_eq!(
            feeds,
            vec![
                (Point::new(20.0, 42.0), bottom),
                (Point::new(20.0, 30.0), bottom),
                (Point::new(80.0, 42.0), bottom),
                (Point::new(80.0, 30.0), bottom),
            ]
        );
        // After each return the tool lifts straight up where it went in.
        for (i, m) in done.moves.iter().enumerate() {
            if m.kind == MoveKind::Feed && m.to.y == 30.0 {
                let lift = &done.moves[i + 1];
                assert_eq!(lift.kind, MoveKind::Rapid);
                assert_eq!(lift.to.xy(), m.to.xy());
                assert!(lift.to.z > surface);
            }
        }
        let last = done.moves.last().expect("moves");
        assert_eq!(last.to, Heights::from_job(&job).home);
    }

    #[test]
    fn the_direction_sets_where_the_slot_runs() {
        let one = [circle(Point::new(50.0, 50.0), 5.0).expect("circle")];
        let cases = [
            (SlotDirection::BottomToTop, Point::new(50.0, 62.0)),
            (SlotDirection::TopToBottom, Point::new(50.0, 38.0)),
            (SlotDirection::LeftToRight, Point::new(62.0, 50.0)),
            (SlotDirection::RightToLeft, Point::new(38.0, 50.0)),
        ];
        for (direction, end) in cases {
            let done = keyhole(&one, &JobSetup::default(), &params(direction)).expect("keyhole");
            let first = done
                .moves
                .iter()
                .find(|m| m.kind == MoveKind::Feed)
                .expect("a feed");
            assert_eq!(first.to.xy(), end, "{direction:?}");
        }
    }

    #[test]
    fn nothing_selected_or_bad_sizes_are_refused() {
        let job = JobSetup::default();
        assert!(keyhole(&[], &job, &KeyholeParams::default()).is_err());
        let zero_depth = KeyholeParams {
            depth: 0.0,
            ..KeyholeParams::default()
        };
        assert!(keyhole(&entries(), &job, &zero_depth).is_err());
        let zero_length = KeyholeParams {
            length: 0.0,
            ..KeyholeParams::default()
        };
        assert!(keyhole(&entries(), &job, &zero_length).is_err());
    }

    #[test]
    fn params_read_from_the_form_json() {
        let params: KeyholeParams = serde_json::from_value(serde_json::json!({
            "direction": "rightToLeft", "depth": 4.0, "length": 20.0
        }))
        .expect("params");
        assert_eq!(params.direction, SlotDirection::RightToLeft);
        assert_eq!(params.length, 20.0);
    }
}
