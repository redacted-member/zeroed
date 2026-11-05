//! Trial runs of a job, posted before the real file: an air cut, with every
//! height raised so the whole job runs clear above the material, and a
//! scrap cut, which cuts no deeper than a shallow pass so the job can be
//! run on a scrap board first. Both are labelled in the file name and in
//! the program header so they are never taken for the real program.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::toolpath::Toolpath;

/// A trial version of the job to post instead of the real one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrialCut {
    /// Every Z raised by one amount, so that the lowest point of the job
    /// sits `height` millimetres above the material surface.
    Air {
        /// Clearance of the lowest point above the material, millimetres.
        height: f64,
    },
    /// Every Z kept within `depth` millimetres below the material surface;
    /// a cut shallower than that is left as it is.
    Scrap {
        /// Deepest cut below the material surface, millimetres.
        depth: f64,
    },
}

impl TrialCut {
    /// Written before the file name in the program header.
    pub fn label(&self) -> &'static str {
        match self {
            TrialCut::Air { .. } => "AIR CUT",
            TrialCut::Scrap { .. } => "SCRAP CUT",
        }
    }

    /// Added to the file name, before the extension.
    pub fn suffix(&self) -> &'static str {
        match self {
            TrialCut::Air { .. } => "_AIR_CUT",
            TrialCut::Scrap { .. } => "_SCRAP_CUT",
        }
    }

    /// Checks the height or depth is a positive length.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for zero, a negative or a non
    /// finite value.
    pub fn validate(&self) -> Result<(), CoreError> {
        let (value, what) = match self {
            TrialCut::Air { height } => (*height, "the air cut height"),
            TrialCut::Scrap { depth } => (*depth, "the scrap cut depth"),
        };
        if value.is_finite() && value > 0.0 {
            Ok(())
        } else {
            Err(CoreError::validation(format!(
                "{what} must be more than zero"
            )))
        }
    }

    /// The toolpaths as the trial posts them. `top` is the job Z of the
    /// material surface, millimetres. The air cut raises every move, the
    /// safe Z and the home position of every toolpath by the same amount,
    /// so the job keeps its shape; the scrap cut lifts every point deeper
    /// than its depth up to it.
    pub fn apply(&self, toolpaths: &[Toolpath], top: f64) -> Vec<Toolpath> {
        let mut out = toolpaths.to_vec();
        match *self {
            TrialCut::Air { height } => {
                let lowest = toolpaths
                    .iter()
                    .flat_map(|t| t.moves.iter().map(|m| m.to.z))
                    .fold(f64::INFINITY, f64::min);
                let lift = if lowest.is_finite() {
                    (top + height - lowest).max(0.0)
                } else {
                    0.0
                };
                for toolpath in &mut out {
                    for one in &mut toolpath.moves {
                        one.to.z += lift;
                    }
                    toolpath.safe_z += lift;
                    toolpath.home.z += lift;
                    toolpath.recompute_bounds();
                }
            }
            TrialCut::Scrap { depth } => {
                let floor = top - depth;
                for toolpath in &mut out {
                    for one in &mut toolpath.moves {
                        one.to.z = one.to.z.max(floor);
                    }
                    toolpath.recompute_bounds();
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{Move, Point3, StrategyKind};

    fn job(top: f64) -> Vec<Toolpath> {
        let mut first = Toolpath::new(1, "pocket", Tool::default(), StrategyKind::Pocket);
        first.safe_z = top + 5.0;
        first.home = Point3::new(0.0, 0.0, top + 20.0);
        first.moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, top + 5.0)),
            Move::plunge(Point3::new(10.0, 10.0, top - 3.0)),
            Move::feed(Point3::new(30.0, 10.0, top - 6.0)),
            Move::rapid(Point3::new(30.0, 10.0, top + 5.0)),
        ];
        let mut second = first.clone();
        second.id = 2;
        second.moves[1].to.z = top - 0.2;
        second.moves[2].to.z = top - 0.3;
        vec![first, second]
    }

    fn heights(toolpaths: &[Toolpath]) -> Vec<f64> {
        toolpaths
            .iter()
            .flat_map(|t| t.moves.iter().map(|m| m.to.z))
            .collect()
    }

    fn close(got: &[f64], expected: &[f64]) {
        assert_eq!(got.len(), expected.len(), "{got:?} {expected:?}");
        for (g, e) in got.iter().zip(expected) {
            assert!((g - e).abs() < 1e-9, "{got:?} {expected:?}");
        }
    }

    #[test]
    fn an_air_cut_lifts_the_lowest_point_its_height_above_the_material() {
        // Z zero on top, and on the bed of 18 mm material (surface at 18).
        // The deepest point is 6 below the surface, so everything goes up
        // 25 + 6 = 31.
        for top in [0.0, 18.0] {
            let toolpaths = job(top);
            let air = TrialCut::Air { height: 25.0 }.apply(&toolpaths, top);
            let expected: Vec<f64> = heights(&toolpaths).iter().map(|z| z + 31.0).collect();
            close(&heights(&air), &expected);
            assert!((air[0].safe_z - (top + 36.0)).abs() < 1e-9);
            assert!((air[1].home.z - (top + 51.0)).abs() < 1e-9);
            let bounds = air[0].bounds.expect("bounds");
            assert!((bounds.min.z - (top + 25.0)).abs() < 1e-9);
            // X and Y stay where they were.
            assert_eq!(air[0].moves[2].to.x, 30.0);
        }
        // An inch height: 1 in is 25.4 mm above the surface.
        let air = TrialCut::Air { height: 25.4 }.apply(&job(0.0), 0.0);
        let lowest = heights(&air).into_iter().fold(f64::INFINITY, f64::min);
        assert!((lowest - 25.4).abs() < 1e-9);
    }

    #[test]
    fn a_scrap_cut_goes_no_deeper_than_its_depth() {
        let top = 18.0;
        let toolpaths = job(top);
        let scrap = TrialCut::Scrap { depth: 0.5 }.apply(&toolpaths, top);
        // The deep pocket stops 0.5 below the surface; the cut that was
        // already shallower (0.2 and 0.3 deep) is left as it was; the
        // rapids above the surface are untouched.
        close(
            &heights(&scrap),
            &[23.0, 17.5, 17.5, 23.0, 23.0, 17.8, 17.7, 23.0],
        );
        assert_eq!(scrap[0].safe_z, toolpaths[0].safe_z);
        assert_eq!(scrap[0].home, toolpaths[0].home);
        let bounds = scrap[0].bounds.expect("bounds");
        assert!((bounds.min.z - 17.5).abs() < 1e-9);
        // An inch depth of 0.02 in is 0.508 mm.
        let scrap = TrialCut::Scrap { depth: 0.508 }.apply(&job(0.0), 0.0);
        assert!((scrap[0].moves[1].to.z + 0.508).abs() < 1e-9);
        // At the edge: a depth just shallower than the real cut clamps it,
        // and one at the real depth leaves it.
        let scrap = TrialCut::Scrap { depth: 5.99 }.apply(&job(0.0), 0.0);
        assert!((scrap[0].moves[2].to.z + 5.99).abs() < 1e-9);
        assert!((scrap[0].moves[1].to.z + 3.0).abs() < 1e-9);
        let scrap = TrialCut::Scrap { depth: 6.0 }.apply(&job(0.0), 0.0);
        close(&heights(&scrap), &heights(&job(0.0)));
    }

    #[test]
    fn a_trial_needs_a_positive_length_and_says_what_it_is() {
        assert!(TrialCut::Air { height: 0.0 }.validate().is_err());
        assert!(TrialCut::Scrap { depth: -0.5 }.validate().is_err());
        assert!(TrialCut::Scrap { depth: f64::NAN }.validate().is_err());
        assert!(TrialCut::Scrap { depth: 0.01 }.validate().is_ok());
        assert_eq!(TrialCut::Air { height: 1.0 }.label(), "AIR CUT");
        assert_eq!(TrialCut::Scrap { depth: 1.0 }.label(), "SCRAP CUT");
        assert_eq!(TrialCut::Air { height: 1.0 }.suffix(), "_AIR_CUT");
        assert_eq!(TrialCut::Scrap { depth: 1.0 }.suffix(), "_SCRAP_CUT");
    }
}
