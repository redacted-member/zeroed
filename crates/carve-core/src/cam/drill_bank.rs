//! Drill Bank toolpaths: a head of several drills at a fixed pitch drills
//! many holes at once. The bank is a grid of positions (a template), each
//! with a drill number and diameter, one of them the reference drill whose
//! position the machine is sent to. Circles in the selection are matched to
//! the bank's drills within a pitch and a diameter tolerance; each firing
//! lowers the drills that land on circles.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{path_bbox, Path, Point};

use super::common::{Emitter, Heights};
use super::drill::{check_params, drill_hole, DrillParams};
use super::profile::Calculated;
use super::toolpath::BankFiring;

/// One position of a drill bank.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BankDrill {
    /// Drill Number the machine knows the drill by; zero for a position
    /// with no drill fitted.
    pub number: u32,
    /// Diameter in millimetres.
    pub diameter: f64,
}

impl Default for BankDrill {
    fn default() -> Self {
        BankDrill {
            number: 0,
            diameter: 5.0,
        }
    }
}

/// A drill bank template: `horizontal` by `vertical` positions `pitch`
/// apart, row by row from the bottom left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DrillBankTemplate {
    /// Horizontal Drills: positions along X.
    pub horizontal: u32,
    /// Vertical Drills: positions along Y.
    pub vertical: u32,
    /// Pitch between neighbouring positions, in millimetres.
    pub pitch: f64,
    /// Central Drill: column and row of the reference drill.
    pub reference: [u32; 2],
    /// The drills, row by row from the bottom, `horizontal` a row.
    pub drills: Vec<BankDrill>,
}

impl Default for DrillBankTemplate {
    fn default() -> Self {
        // A common line bank: nine drills on the 32 mm system.
        DrillBankTemplate {
            horizontal: 9,
            vertical: 1,
            pitch: 32.0,
            reference: [0, 0],
            drills: (1..=9)
                .map(|number| BankDrill {
                    number,
                    diameter: 5.0,
                })
                .collect(),
        }
    }
}

impl DrillBankTemplate {
    /// Checks the grid, pitch, reference and drills.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming what is wrong.
    pub fn check(&self) -> Result<(), CoreError> {
        if self.horizontal == 0 || self.vertical == 0 || self.horizontal * self.vertical > 400 {
            return Err(CoreError::validation(
                "a drill bank needs 1 to 400 positions",
            ));
        }
        if !(self.pitch.is_finite() && self.pitch > 0.0) {
            return Err(CoreError::validation("the pitch must be more than zero"));
        }
        if self.reference[0] >= self.horizontal || self.reference[1] >= self.vertical {
            return Err(CoreError::validation(
                "the central drill must be one of the bank's positions",
            ));
        }
        if self.drills.len() != (self.horizontal * self.vertical) as usize {
            return Err(CoreError::validation(
                "the drill bank has a drill entry for each position",
            ));
        }
        if self
            .drills
            .iter()
            .any(|d| d.number > 0 && !(d.diameter.is_finite() && d.diameter > 0.0))
        {
            return Err(CoreError::validation("every fitted drill needs a diameter"));
        }
        if self.drills.iter().all(|d| d.number == 0) {
            return Err(CoreError::validation("the drill bank has no drills"));
        }
        let mut numbers: Vec<u32> = self
            .drills
            .iter()
            .map(|d| d.number)
            .filter(|n| *n > 0)
            .collect();
        let fitted = numbers.len();
        numbers.sort_unstable();
        numbers.dedup();
        if numbers.len() != fitted {
            return Err(CoreError::validation(
                "each drill of the bank needs its own number",
            ));
        }
        Ok(())
    }

    /// Of `drills`, those on the horizontal arm (the central drill's row)
    /// and those on the vertical arm (its column), each lowest first. The
    /// central drill is on both.
    pub fn arms(&self, drills: &[u32]) -> (Vec<u32>, Vec<u32>) {
        let h = self.horizontal.max(1) as usize;
        let [column, row] = self.reference.map(|v| v as usize);
        let mut along_x = Vec::new();
        let mut along_y = Vec::new();
        for (index, drill) in self.drills.iter().enumerate() {
            if drill.number == 0 || !drills.contains(&drill.number) {
                continue;
            }
            if index / h == row {
                along_x.push(drill.number);
            }
            if index % h == column {
                along_y.push(drill.number);
            }
        }
        along_x.sort_unstable();
        along_y.sort_unstable();
        (along_x, along_y)
    }

    /// Offset of position (`column`, `row`) from the reference drill.
    fn offset(&self, index: usize) -> Point {
        let h = self.horizontal as usize;
        let (column, row) = (index % h, index / h);
        Point::new(
            (column as f64 - f64::from(self.reference[0])) * self.pitch,
            (row as f64 - f64::from(self.reference[1])) * self.pitch,
        )
    }
}

/// The Drill Bank form's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DrillBankParams {
    /// The bank.
    pub template: DrillBankTemplate,
    /// Pitch Tolerance: how far a circle may sit from a drill's position,
    /// in millimetres.
    pub pitch_tolerance: f64,
    /// Diameter Tolerance: how far a circle's diameter may differ from the
    /// drill's, in millimetres.
    pub diameter_tolerance: f64,
    /// Depths, pecking, retract gap and dwell, as the Drilling form.
    pub drill: DrillParams,
}

impl Default for DrillBankParams {
    fn default() -> Self {
        DrillBankParams {
            template: DrillBankTemplate::default(),
            pitch_tolerance: 0.1,
            diameter_tolerance: 0.1,
            drill: DrillParams::default(),
        }
    }
}

/// A circle to drill: its centre and diameter.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Hole {
    at: Point,
    diameter: f64,
}

/// The closed vectors that are circles (their boxes square within the
/// diameter tolerance), as holes.
fn holes_of(paths: &[Path], tolerance: f64) -> Vec<Hole> {
    let mut out = Vec::new();
    for path in paths {
        for sub in path.subpaths.iter().filter(|s| s.closed) {
            if let Some(bbox) = path_bbox(&sub.clone().into_path()) {
                let (w, h) = (bbox.width(), bbox.height());
                if (w - h).abs() <= tolerance.max(1e-6) * 2.0 {
                    out.push(Hole {
                        at: bbox.centre(),
                        diameter: (w + h) / 2.0,
                    });
                }
            }
        }
    }
    out
}

/// A drill bank toolpath and its firings.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillBankCut {
    /// The moves and warnings.
    pub calculated: Calculated,
    /// Where the reference drill stands for each firing and which drills
    /// go down.
    pub firings: Vec<BankFiring>,
}

/// Calculates a Drill Bank toolpath over the circles in `paths`.
///
/// Each firing places the bank so one of its drills stands on a circle not
/// yet drilled, picking the placement that drills the most circles at once
/// (ties go to the lowest, then leftmost, placement); the drills whose
/// positions fall on undrilled circles of their diameter go down together.
/// The tool moves to the reference drill's position.
///
/// # Errors
/// [`CoreError::Validation`] for a bad template, depths or tolerances, no
/// circles, or no circle that any drill fits.
pub fn drill_bank(
    paths: &[Path],
    job: &JobSetup,
    params: &DrillBankParams,
) -> Result<DrillBankCut, CoreError> {
    params.template.check()?;
    check_params(&params.drill)?;
    let (pitch_tol, dia_tol) = (params.pitch_tolerance, params.diameter_tolerance);
    if !(pitch_tol.is_finite() && pitch_tol >= 0.0 && dia_tol.is_finite() && dia_tol >= 0.0) {
        return Err(CoreError::validation("the tolerances must be zero or more"));
    }
    let holes = holes_of(paths, dia_tol);
    if holes.is_empty() {
        return Err(CoreError::validation(
            "select circles for the drill bank to drill",
        ));
    }
    let template = &params.template;
    let fitted: Vec<(usize, BankDrill)> = template
        .drills
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, d)| d.number > 0)
        .collect();
    let fits =
        |drill: &BankDrill, hole: &Hole| (drill.diameter - hole.diameter).abs() <= dia_tol + 1e-9;
    let mut done = vec![false; holes.len()];
    let mut firings: Vec<(Point, Vec<(u32, usize)>)> = Vec::new();
    loop {
        // Every placement that puts a fitted drill on an undrilled circle.
        let mut best: Option<(Point, Vec<(u32, usize)>)> = None;
        for (h, hole) in holes.iter().enumerate().filter(|(i, _)| !done[*i]) {
            for (index, drill) in &fitted {
                if !fits(drill, hole) {
                    continue;
                }
                let origin = Point::new(
                    hole.at.x - template.offset(*index).x,
                    hole.at.y - template.offset(*index).y,
                );
                let mut hits: Vec<(u32, usize)> = Vec::new();
                for (other, d) in &fitted {
                    let offset = template.offset(*other);
                    let at = Point::new(origin.x + offset.x, origin.y + offset.y);
                    let found = holes.iter().enumerate().find(|(k, c)| {
                        !done[*k] && c.at.distance(at) <= pitch_tol + 1e-9 && fits(d, c)
                    });
                    if let Some((k, _)) = found {
                        if !hits.iter().any(|(_, taken)| *taken == k) {
                            hits.push((d.number, k));
                        }
                    }
                }
                debug_assert!(hits.iter().any(|(_, k)| *k == h));
                let better = match &best {
                    None => true,
                    Some((at, most)) => {
                        hits.len() > most.len()
                            || (hits.len() == most.len()
                                && (origin.y, origin.x) < (at.y - 1e-9, at.x - 1e-9))
                    }
                };
                if better {
                    best = Some((origin, hits));
                }
            }
        }
        let Some((origin, hits)) = best else {
            break;
        };
        for (_, k) in &hits {
            done[*k] = true;
        }
        firings.push((origin, hits));
    }
    if firings.is_empty() {
        return Err(CoreError::validation(
            "no drill of the bank fits the selected circles: check the diameters and the tolerance",
        ));
    }
    let mut warnings = Vec::new();
    let missed = done.iter().filter(|d| !**d).count();
    if missed > 0 {
        warnings.push(crate::warning::Warning::drill_bank_missed(missed));
    }
    let heights = Heights::from_job(job);
    let bottom = heights.z_at(params.drill.cut_depth);
    let top = heights.z_at(params.drill.start_depth);
    let mut out = Emitter::new(heights);
    let mut shots = Vec::with_capacity(firings.len());
    for (origin, hits) in firings {
        drill_hole(&mut out, origin, top, bottom, &params.drill);
        let mut drills: Vec<u32> = hits.iter().map(|(n, _)| *n).collect();
        drills.sort_unstable();
        drills.dedup();
        let (along_x, along_y) = template.arms(&drills);
        shots.push(BankFiring {
            at: origin,
            drills,
            along_x,
            along_y,
        });
    }
    Ok(DrillBankCut {
        calculated: Calculated {
            moves: out.finish(),
            warnings,
        },
        firings: shots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::circle;

    fn holes(at: &[(f64, f64, f64)]) -> Vec<Path> {
        at.iter()
            .map(|(x, y, d)| circle(Point::new(*x, *y), d / 2.0).expect("circle"))
            .collect()
    }

    fn line_bank(count: u32, pitch: f64) -> DrillBankTemplate {
        DrillBankTemplate {
            horizontal: count,
            vertical: 1,
            pitch,
            reference: [0, 0],
            drills: (1..=count)
                .map(|number| BankDrill {
                    number,
                    diameter: 5.0,
                })
                .collect(),
        }
    }

    #[test]
    fn a_row_of_holes_on_the_pitch_is_drilled_in_one_firing() {
        let params = DrillBankParams {
            template: line_bank(5, 32.0),
            ..DrillBankParams::default()
        };
        let row = holes(&[(100.0, 50.0, 5.0), (132.0, 50.0, 5.0), (164.0, 50.0, 5.0)]);
        let cut = drill_bank(&row, &JobSetup::default(), &params).expect("drills");
        assert_eq!(cut.firings.len(), 1);
        assert_eq!(cut.firings[0].drills.len(), 3);
        // The head goes where its reference drill stands.
        let plunges: Vec<Point> = cut
            .calculated
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.xy())
            .collect();
        assert!(plunges.iter().all(|p| p.distance(cut.firings[0].at) < 1e-9));
        assert!(cut.calculated.warnings.is_empty());
    }

    #[test]
    fn circles_off_the_pitch_or_the_diameter_need_firings_of_their_own() {
        let params = DrillBankParams {
            template: line_bank(3, 32.0),
            pitch_tolerance: 0.2,
            diameter_tolerance: 0.1,
            ..DrillBankParams::default()
        };
        // The second hole is 0.1 off the pitch (inside), the third 1 mm off.
        let row = holes(&[(0.0, 0.0, 5.0), (32.1, 0.0, 5.0), (65.0, 0.0, 5.0)]);
        let cut = drill_bank(&row, &JobSetup::default(), &params).expect("drills");
        assert_eq!(cut.firings.len(), 2);
        assert_eq!(cut.firings[0].drills, vec![1, 2]);
        // An 8 mm circle fits no 5 mm drill: it is left and reported.
        let with_big = holes(&[(0.0, 0.0, 5.0), (32.0, 0.0, 8.0)]);
        let cut = drill_bank(&with_big, &JobSetup::default(), &params).expect("drills");
        assert_eq!(cut.firings.len(), 1);
        assert_eq!(cut.calculated.warnings.len(), 1);
        assert!(drill_bank(&holes(&[(0.0, 0.0, 8.0)]), &JobSetup::default(), &params).is_err());
    }

    #[test]
    fn a_grid_bank_uses_its_reference_drill_and_each_drill_number() {
        let mut template = DrillBankTemplate {
            horizontal: 2,
            vertical: 2,
            pitch: 32.0,
            reference: [1, 1],
            drills: vec![
                BankDrill {
                    number: 1,
                    diameter: 5.0,
                },
                BankDrill {
                    number: 0,
                    diameter: 5.0,
                },
                BankDrill {
                    number: 3,
                    diameter: 8.0,
                },
                BankDrill {
                    number: 4,
                    diameter: 5.0,
                },
            ],
        };
        let params = DrillBankParams {
            template: template.clone(),
            ..DrillBankParams::default()
        };
        // Drill 1 at (-32, -32) from the reference, 3 at (-32, 0), 4 at (0, 0).
        let grid = holes(&[(10.0, 10.0, 5.0), (10.0, 42.0, 8.0), (42.0, 42.0, 5.0)]);
        let cut = drill_bank(&grid, &JobSetup::default(), &params).expect("drills");
        assert_eq!(cut.firings.len(), 1);
        assert_eq!(cut.firings[0].drills, vec![1, 3, 4]);
        assert!(cut.firings[0].at.distance(Point::new(42.0, 42.0)) < 1e-9);
        // The central drill's row is the horizontal arm, its column the
        // vertical one; the central drill is on both.
        assert_eq!(cut.firings[0].along_x, vec![3, 4]);
        assert_eq!(cut.firings[0].along_y, vec![4]);
        template.reference = [2, 0];
        assert!(template.check().is_err());
        template.reference = [1, 1];
        template.drills[3].number = 1;
        assert!(template.check().is_err(), "two drills numbered 1");
    }
}
