//! Named explode steps of the Assembly View: each moves some pieces a set
//! distance along a direction, and the Explode slider plays them in order
//! (the first step's pieces move out, then the second's, and so on). They
//! are kept with the job; with none, the slider moves each piece out of
//! its joints as Verify Fit finds them.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::pieces::{Anchor, PieceRef};
use super::pose::{self, Vec3};

/// One explode step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplodeStep {
    /// Id, from the assembly's id counter; 0 on a step not yet kept.
    #[serde(default)]
    pub id: u64,
    /// Its name in the list.
    pub name: String,
    /// The pieces it moves.
    pub pieces: Vec<PieceRef>,
    /// The anchor of each piece's part, in the order of `pieces` (none for a
    /// component or raw model piece, or one saved before anchors were
    /// kept): what keeps the step on the same physical part when a
    /// toolpath edit renumbers the parts, as mates do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Option<Anchor>>,
    /// Which way, a unit vector in assembly space.
    pub direction: Vec3,
    /// How far, mm.
    pub distance: f64,
}

/// Checks and tidies steps before they are kept: every step moves at
/// least one piece (each once) a finite distance along a direction that
/// is not zero (made unit length); an empty name becomes `Step n`.
///
/// # Errors
/// [`CoreError::Validation`] naming the step that is wrong.
pub fn tidy_steps(steps: Vec<ExplodeStep>) -> Result<Vec<ExplodeStep>, CoreError> {
    steps
        .into_iter()
        .enumerate()
        .map(|(i, mut step)| {
            let length = pose::dot(step.direction, step.direction).sqrt();
            let finite = step.direction.iter().all(|v| v.is_finite()) && step.distance.is_finite();
            if !finite || length < 1e-9 {
                return Err(CoreError::validation(format!(
                    "explode step {} needs a direction and a distance",
                    i + 1
                )));
            }
            if step.anchors.len() != step.pieces.len() {
                step.anchors = vec![None; step.pieces.len()];
            }
            let mut kept: Vec<(PieceRef, Option<Anchor>)> = Vec::new();
            for (piece, anchor) in step.pieces.iter().zip(&step.anchors) {
                if !kept.iter().any(|(p, _)| p == piece) {
                    kept.push((*piece, *anchor));
                }
            }
            (step.pieces, step.anchors) = kept.into_iter().unzip();
            if step.pieces.is_empty() {
                return Err(CoreError::validation(format!(
                    "explode step {} moves no piece",
                    i + 1
                )));
            }
            step.direction = pose::scale(step.direction, 1.0 / length);
            step.name = step.name.trim().to_string();
            if step.name.is_empty() {
                step.name = format!("Step {}", i + 1);
            }
            Ok(step)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(sheet: u64) -> PieceRef {
        PieceRef {
            sheet,
            part: 1,
            component: None,
        }
    }

    #[test]
    fn steps_are_tidied_and_bad_ones_refused() {
        let steps = tidy_steps(vec![ExplodeStep {
            id: 0,
            name: "  ".into(),
            pieces: vec![piece(1), piece(1), piece(2)],
            anchors: Vec::new(),
            direction: [0.0, 0.0, 2.0],
            distance: 40.0,
        }])
        .expect("tidy");
        assert_eq!(steps[0].name, "Step 1");
        assert_eq!(steps[0].pieces, vec![piece(1), piece(2)]);
        assert_eq!(steps[0].anchors, vec![None, None]);
        assert_eq!(steps[0].direction, [0.0, 0.0, 1.0]);
        let none = ExplodeStep {
            pieces: Vec::new(),
            ..steps[0].clone()
        };
        assert!(tidy_steps(vec![none]).is_err());
        let still = ExplodeStep {
            direction: [0.0; 3],
            ..steps[0].clone()
        };
        assert!(tidy_steps(vec![still]).is_err());
    }
}
