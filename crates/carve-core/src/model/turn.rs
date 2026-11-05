//! Turn and Spin: reliefs made by revolving a profile vector.
//!
//! Turn is a lathe: the chord from the profile's start to its end is the
//! axis, and the profile's offset from that axis is the radius of the
//! solid at each place along it. The relief is the top half of that solid
//! lying on the model plane. Spin revolves the profile round its left end
//! point in the plane: the profile's X distance from that end is the
//! radius and its Y offset is the height, giving a circular relief. Both
//! can scale the result to an exact height.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{Path, Point, Tolerance};

use super::field::Field;
use super::shape::Grid;
use super::sweep::{open_points, Table};

/// Most samples in a profile's height table.
const MAX_SAMPLES: usize = 100_000;

/// The Turn form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TurnParams {
    /// Height of the base the turned shape stands on.
    pub base_height: f64,
    /// Scale the shape so its highest point (above the base) is `height`.
    pub exact_height: bool,
    /// Height for `exact_height`, millimetres.
    pub height: f64,
}

/// The Spin form's settings. Lengths are millimetres, angles degrees
/// counter clockwise from the +X direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpinParams {
    /// Height of the base the spun shape stands on.
    pub base_height: f64,
    /// Where the sweep round the centre begins, degrees.
    pub start_angle: f64,
    /// Where the sweep round the centre ends, degrees; a full turn when it
    /// is 360 or more past the start.
    pub end_angle: f64,
    /// Scale the shape so its highest point (above the base) is `height`.
    pub exact_height: bool,
    /// Height for `exact_height`, millimetres.
    pub height: f64,
}

impl Default for SpinParams {
    fn default() -> Self {
        SpinParams {
            base_height: 0.0,
            start_angle: 0.0,
            end_angle: 360.0,
            exact_height: false,
            height: 10.0,
        }
    }
}

/// Scales every height above `base` so the highest is `height` above it.
fn scale_to(field: &mut Field, base: f64, height: f64) -> Result<(), CoreError> {
    if !(height.is_finite() && height > 0.0) {
        return Err(CoreError::validation(
            "the exact height must be more than zero",
        ));
    }
    let base = base as f32;
    let top = field
        .z
        .iter()
        .filter(|z| !z.is_nan())
        .fold(f32::NEG_INFINITY, |m, &z| m.max(z - base));
    if top > 1e-9 {
        let factor = height as f32 / top;
        for z in field.z.iter_mut().filter(|z| !z.is_nan()) {
            *z = base + (*z - base) * factor;
        }
    }
    Ok(())
}

/// Table samples for a profile of this length on this grid.
fn samples(length: f64, grid: &Grid) -> usize {
    ((length / (grid.cell * 0.25)).ceil() as usize).clamp(2, MAX_SAMPLES)
}

/// Builds a turned (lathe) relief over the whole grid, NaN where it does
/// not reach.
///
/// The profile's chord is the axis; the height at a cell is
/// `sqrt(h(s)^2 - d^2)` where `s` is its place along the axis, `d` its
/// distance from the axis and `h(s)` the profile's offset from the axis
/// there, all in millimetres.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a closed or empty profile, one
/// whose ends meet (no axis), or a base height that is not a number.
pub fn turn(
    profile: &Path,
    grid: &Grid,
    params: &TurnParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    if !params.base_height.is_finite() {
        return Err(CoreError::validation("the base height must be a number"));
    }
    let points = open_points(profile, "profile", tol)?;
    let start = points[0];
    let chord = start.to(points[points.len() - 1]);
    let length = chord.length();
    let Some(along) = chord.normalised() else {
        return Err(CoreError::validation(
            "the profile's ends must be apart, the line between them is the axis",
        ));
    };
    let pairs: Vec<(f64, f64)> = points
        .iter()
        .map(|p| {
            let v = start.to(*p);
            (v.dot(along).clamp(0.0, length), along.cross(v).abs())
        })
        .collect();
    let table = Table::from_points(&pairs, length, samples(length, grid));

    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for j in 0..grid.height {
        for i in 0..grid.width {
            let v = start.to(field.centre(i, j));
            let s = v.dot(along);
            let d = along.cross(v).abs();
            let h = table.at(s);
            if h.is_nan() || d > h {
                continue;
            }
            let z = params.base_height + (h * h - d * d).max(0.0).sqrt();
            field.z[j * grid.width + i] = z as f32;
        }
    }
    if params.exact_height {
        scale_to(&mut field, params.base_height, params.height)?;
    }
    Ok(field)
}

/// Builds a spun relief over the whole grid, NaN where it does not reach.
///
/// The profile's left end point (whichever end has the smaller X) is the
/// centre. A profile point `(x, y)` gives the height `y - y0` at the radius
/// `|x - x0|` from the centre, where `(x0, y0)` is that end; heights are in
/// millimetres. Only cells whose
/// direction from the centre lies in the start to end angle sweep are set.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a closed or empty profile, one
/// with no reach in X, an end angle not past the start, or values that
/// are not numbers.
pub fn spin(
    profile: &Path,
    grid: &Grid,
    params: &SpinParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    if !params.base_height.is_finite()
        || !params.start_angle.is_finite()
        || !params.end_angle.is_finite()
    {
        return Err(CoreError::validation(
            "the base height and angles must be numbers",
        ));
    }
    let sweep = params.end_angle - params.start_angle;
    if sweep <= 0.0 {
        return Err(CoreError::validation(
            "the end angle must be past the start angle",
        ));
    }
    let points = open_points(profile, "profile", tol)?;
    let (first, last) = (points[0], points[points.len() - 1]);
    let centre = if (last.x, last.y) < (first.x, first.y) {
        last
    } else {
        first
    };
    let pairs: Vec<(f64, f64)> = points
        .iter()
        .map(|p| ((p.x - centre.x).abs(), p.y - centre.y))
        .collect();
    let reach = pairs.iter().map(|p| p.0).fold(0.0, f64::max);
    if reach <= 0.0 {
        return Err(CoreError::validation(
            "the profile must run across in X, that sets the radius",
        ));
    }
    let table = Table::from_points(&pairs, reach, samples(reach, grid));
    let full = sweep >= 360.0;
    let start = params.start_angle.rem_euclid(360.0);

    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for j in 0..grid.height {
        for i in 0..grid.width {
            let c: Point = field.centre(i, j);
            let v = centre.to(c);
            let rho = v.length();
            if !full && rho > 0.0 {
                let angle = v.y.atan2(v.x).to_degrees();
                if (angle - start).rem_euclid(360.0) > sweep {
                    continue;
                }
            }
            let h = table.at(rho);
            if h.is_nan() {
                continue;
            }
            field.z[j * grid.width + i] = (params.base_height + h) as f32;
        }
    }
    if params.exact_height {
        scale_to(&mut field, params.base_height, params.height)?;
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{line, polyline_path, rect};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn grid(cell: f64) -> Grid {
        Grid {
            width: (60.0 / cell) as usize,
            height: (60.0 / cell) as usize,
            cell,
            origin: Point::new(cell / 2.0, cell / 2.0),
        }
    }

    fn path(points: &[(f64, f64)]) -> Path {
        let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        polyline_path(&points, false).expect("path")
    }

    #[test]
    fn turning_a_parallel_profile_makes_a_cylinder_lying_down() {
        let profile = path(&[(10.0, 30.0), (10.0, 35.0), (50.0, 35.0), (50.0, 30.0)]);
        let field = turn(&profile, &grid(0.1), &TurnParams::default(), tol()).expect("turn");
        for x in [15.0, 30.0, 45.0] {
            let crest = f64::from(field.at(Point::new(x, 30.0)));
            assert!((crest - 5.0).abs() < 0.1, "{x}: {crest}");
            let off = f64::from(field.at(Point::new(x, 27.0)));
            assert!((off - 16f64.sqrt()).abs() < 0.1, "{x}: {off}");
        }
        assert!(field.at(Point::new(30.0, 36.0)).is_nan());
        assert!(field.at(Point::new(5.0, 30.0)).is_nan());
    }

    #[test]
    fn turning_a_slanted_profile_makes_a_cone() {
        let profile = path(&[(10.0, 30.0), (30.0, 35.0), (30.0, 30.0)]);
        let field = turn(&profile, &grid(0.1), &TurnParams::default(), tol()).expect("turn");
        // Radius rises 5 over 20: a quarter of the way along the axis.
        for x in [14.0, 20.0, 26.0] {
            let crest = f64::from(field.at(Point::new(x, 30.0)));
            let want = (x - 10.0) / 4.0;
            assert!((crest - want).abs() < 0.1, "{x}: {crest}");
        }
    }

    #[test]
    fn spinning_a_flat_profile_makes_a_disc() {
        let profile = path(&[(30.0, 30.0), (30.0, 33.0), (40.0, 33.0), (40.0, 30.0)]);
        let params = SpinParams {
            base_height: 1.0,
            ..SpinParams::default()
        };
        let field = spin(&profile, &grid(0.2), &params, tol()).expect("spin");
        for p in [(30.0, 30.0), (35.0, 30.0), (30.0, 21.0), (24.0, 36.0)] {
            let z = f64::from(field.at(Point::new(p.0, p.1)));
            assert!((z - 4.0).abs() < 1e-5, "{p:?}: {z}");
        }
        assert!(field.at(Point::new(30.0, 41.0)).is_nan());
    }

    #[test]
    fn spinning_part_way_leaves_the_rest_empty() {
        let profile = path(&[(30.0, 30.0), (30.0, 2.0), (40.0, 2.0)]);
        let params = SpinParams {
            start_angle: 0.0,
            end_angle: 90.0,
            ..SpinParams::default()
        };
        let field = spin(&profile, &grid(0.2), &params, tol()).expect("spin");
        assert!(!field.at(Point::new(35.0, 35.0)).is_nan());
        assert!(field.at(Point::new(25.0, 35.0)).is_nan());
        assert!(field.at(Point::new(35.0, 25.0)).is_nan());
    }

    #[test]
    fn a_spun_slope_is_a_cone() {
        let profile = line(Point::new(30.0, 30.0), Point::new(40.0, 35.0));
        let field = spin(&profile, &grid(0.1), &SpinParams::default(), tol()).expect("spin");
        let centre = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!(centre.abs() < 0.1, "{centre}");
        let half_way = f64::from(field.at(Point::new(30.0, 25.0)));
        assert!((half_way - 2.5).abs() < 0.1, "{half_way}");
    }

    #[test]
    fn closed_or_bad_profiles_are_refused() {
        let closed = rect(Point::new(10.0, 10.0), Point::new(20.0, 20.0)).expect("rect");
        let g = grid(0.5);
        assert!(matches!(
            turn(&closed, &g, &TurnParams::default(), tol()),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            spin(&closed, &g, &SpinParams::default(), tol()),
            Err(CoreError::Validation(_))
        ));
        let backwards = SpinParams {
            start_angle: 90.0,
            end_angle: 10.0,
            ..SpinParams::default()
        };
        let open = line(Point::new(10.0, 10.0), Point::new(20.0, 12.0));
        assert!(spin(&open, &g, &backwards, tol()).is_err());
        assert!(turn(&Path::new(), &g, &TurnParams::default(), tol()).is_err());
    }

    #[test]
    fn spin_turns_round_the_left_end_whichever_end_is_the_start() {
        // Drawn right to left: the centre is still the left end at x 30.
        let profile = path(&[(40.0, 30.0), (40.0, 33.0), (30.0, 33.0), (30.0, 30.0)]);
        let field = spin(&profile, &grid(0.2), &SpinParams::default(), tol()).expect("spin");
        assert!((f64::from(field.at(Point::new(30.0, 30.0))) - 3.0).abs() < 1e-5);
        assert!((f64::from(field.at(Point::new(21.0, 30.0))) - 3.0).abs() < 1e-5);
        assert!(field.at(Point::new(30.0, 41.0)).is_nan());
    }

    #[test]
    fn exact_height_scales_turned_and_spun_shapes() {
        let profile = path(&[(10.0, 30.0), (10.0, 35.0), (50.0, 35.0), (50.0, 30.0)]);
        let params = TurnParams {
            base_height: 1.0,
            exact_height: true,
            height: 2.0,
        };
        let field = turn(&profile, &grid(0.2), &params, tol()).expect("turn");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 3.0).abs() < 1e-4);
        let cone = line(Point::new(30.0, 30.0), Point::new(40.0, 35.0));
        let spun = SpinParams {
            exact_height: true,
            height: 8.0,
            ..SpinParams::default()
        };
        let field = spin(&cone, &grid(0.2), &spun, tol()).expect("spin");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 8.0).abs() < 1e-4);
        let bad = TurnParams {
            height: 0.0,
            ..params
        };
        assert!(turn(&profile, &grid(0.5), &bad, tol()).is_err());
    }
}
