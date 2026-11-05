//! Importing a full 3D model into a rotary job: the oriented mesh is laid
//! along the blank's axis and read as the distance of its surface from the
//! axis all the way round, which is what a rotary job's flat model holds.
//!
//! Meshes here are in the rotary frame `[along, side, up]`: `along` is the
//! job coordinate along the axis, and `side` and `up` are across it with
//! the axis at the origin. A point `around` the blank from the flat layout
//! sits at the angle `around / radius` from straight up, at
//! `(side, up) = (r sin a, r cos a)`, as the 3D view wraps it.

use serde::{Deserialize, Serialize};

use crate::doc::RotaryAxis;
use crate::error::CoreError;
use crate::io::mesh::Triangle;

use super::super::field::Field;
use super::super::shape::Grid;
use super::vec3::{self, V3};

/// How the model sits on the axis and how big it is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RotaryFit {
    /// Where the axis runs through the model, across it from the middle of
    /// its cross section: `[side, up]` in millimetres after scaling.
    pub offset: [f64; 2],
    /// Scale along the axis.
    pub along_scale: f64,
    /// Scale across the axis (the diameter).
    pub radial_scale: f64,
}

impl Default for RotaryFit {
    fn default() -> Self {
        RotaryFit {
            offset: [0.0, 0.0],
            along_scale: 1.0,
            radial_scale: 1.0,
        }
    }
}

/// The cylinder round the model on the axis, millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cylinder {
    pub length: f64,
    pub diameter: f64,
}

fn check(fit: &RotaryFit) -> Result<(), CoreError> {
    let ok = |s: f64| s.is_finite() && s > 0.0;
    if !(ok(fit.along_scale) && ok(fit.radial_scale)) {
        return Err(CoreError::validation(
            "the model size must be more than zero",
        ));
    }
    if !(fit.offset[0].is_finite() && fit.offset[1].is_finite()) {
        return Err(CoreError::validation("the axis offset must be numbers"));
    }
    Ok(())
}

/// Lays an oriented mesh (job millimetres, Z up) along the blank's axis,
/// centred at `along_centre`, scaled and offset as `fit` says. Answers the
/// mesh in the rotary frame and the cylinder round it.
///
/// # Errors
/// [`CoreError::Validation`] for scales that are not above zero, offsets
/// that are not numbers, or an empty mesh.
pub fn lay_on_axis(
    triangles: &[Triangle],
    axis: RotaryAxis,
    fit: &RotaryFit,
    along_centre: f64,
) -> Result<(Vec<Triangle>, Cylinder), CoreError> {
    check(fit)?;
    let frame = |v: V3| -> V3 {
        match axis {
            RotaryAxis::X => [v[0], v[1], v[2]],
            RotaryAxis::Y => [v[1], v[0], v[2]],
        }
    };
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for v in triangles.iter().flatten() {
        let p = frame(*v);
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if triangles.is_empty() || lo[0] > hi[0] {
        return Err(CoreError::validation("the model is empty"));
    }
    let mid = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let place = |v: V3| -> V3 {
        let p = frame(v);
        [
            along_centre + (p[0] - mid[0]) * fit.along_scale,
            (p[1] - mid[1]) * fit.radial_scale - fit.offset[0],
            (p[2] - mid[2]) * fit.radial_scale - fit.offset[1],
        ]
    };
    let out: Vec<Triangle> = triangles
        .iter()
        .map(|t| [place(t[0]), place(t[1]), place(t[2])])
        .collect();
    let reach = out
        .iter()
        .flatten()
        .map(|p| p[1].hypot(p[2]))
        .fold(0.0, f64::max);
    Ok((
        out,
        Cylinder {
            length: (hi[0] - lo[0]) * fit.along_scale,
            diameter: 2.0 * reach,
        },
    ))
}

/// Angle bins round the axis for the ray search.
const ANGLE_BINS: usize = 256;

/// The model as a rotary job's flat heights: for every grid cell, the
/// distance from the axis at which a ray coming in from outside first meets
/// the mesh (`along` and the angle from the cell), NaN where it misses or
/// meets the mesh only beyond the axis. `radius` is the blank's radius,
/// which sets the angle a distance around stands for.
pub fn radial_field(mesh: &[Triangle], grid: &Grid, axis: RotaryAxis, radius: f64) -> Field {
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    if mesh.is_empty() || radius <= 0.0 {
        return field;
    }
    let reach = mesh
        .iter()
        .flatten()
        .map(|p| p[1].hypot(p[2]))
        .fold(0.0, f64::max)
        * 1.01
        + 1.0;
    // Bin the triangles by where they lie along the axis and round it.
    let (a0, a1) = mesh
        .iter()
        .flatten()
        .fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[0]), hi.max(p[0]))
        });
    let along_bins = ((a1 - a0) / grid.cell).ceil().clamp(1.0, 512.0) as usize;
    let along_step = ((a1 - a0) / along_bins as f64).max(1e-9);
    let tau = std::f64::consts::TAU;
    let angle_step = tau / ANGLE_BINS as f64;
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); along_bins * ANGLE_BINS];
    let along_bin =
        |a: f64| (((a - a0) / along_step).floor().max(0.0) as usize).min(along_bins - 1);
    for (k, t) in mesh.iter().enumerate() {
        let (lo, hi) = t.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[0]), hi.max(p[0]))
        });
        let angles: Vec<f64> = t.iter().map(|p| p[1].atan2(p[2]).rem_euclid(tau)).collect();
        let near_axis = t.iter().any(|p| p[1].hypot(p[2]) < 1e-9);
        // The narrowest arc holding the three angles.
        let mut sorted = angles.clone();
        sorted.sort_by(f64::total_cmp);
        let gaps = [
            sorted[1] - sorted[0],
            sorted[2] - sorted[1],
            sorted[0] + tau - sorted[2],
        ];
        let (widest, at) =
            gaps.iter().enumerate().fold(
                (0.0, 0),
                |(w, i), (j, g)| if *g > w { (*g, j) } else { (w, i) },
            );
        let (start, span) = ((sorted[(at + 1) % 3]), tau - widest);
        let every = near_axis || span > std::f64::consts::PI;
        let (b0, count) = if every {
            (0, ANGLE_BINS)
        } else {
            let b0 = (start / angle_step).floor() as usize;
            let b1 = ((start + span) / angle_step).floor() as usize;
            (b0, (b1 - b0 + 1).min(ANGLE_BINS))
        };
        for ab in along_bin(lo)..=along_bin(hi) {
            for s in 0..count {
                bins[ab * ANGLE_BINS + (b0 + s) % ANGLE_BINS].push(k as u32);
            }
        }
    }
    for j in 0..grid.height {
        for i in 0..grid.width {
            let c = field.centre(i, j);
            let (along, around) = match axis {
                RotaryAxis::X => (c.x, c.y),
                RotaryAxis::Y => (c.y, c.x),
            };
            if along < a0 || along > a1 {
                continue;
            }
            let theta = around / radius;
            let (sin, cos) = theta.sin_cos();
            let origin = [along, reach * sin, reach * cos];
            let dir = [0.0, -sin, -cos];
            let bin = along_bin(along) * ANGLE_BINS
                + ((theta.rem_euclid(tau) / angle_step).floor() as usize).min(ANGLE_BINS - 1);
            let mut best = f64::INFINITY;
            for &k in &bins[bin] {
                if let Some(t) = hit(&mesh[k as usize], origin, dir) {
                    if t < best {
                        best = t;
                    }
                }
            }
            if best <= reach {
                field.z[j * grid.width + i] = (reach - best) as f32;
            }
        }
    }
    field
}

/// Where a ray meets a triangle (distance along the unit direction), by
/// the Moller Trumbore test.
fn hit(t: &Triangle, origin: V3, dir: V3) -> Option<f64> {
    let e1 = vec3::sub(t[1], t[0]);
    let e2 = vec3::sub(t[2], t[0]);
    let p = vec3::cross(dir, e2);
    let det = vec3::dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = vec3::sub(origin, t[0]);
    let u = vec3::dot(s, p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = vec3::cross(s, e1);
    let v = vec3::dot(dir, q) * inv;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let d = vec3::dot(e2, q) * inv;
    (d > 0.0).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    /// A closed box `lo..hi` as twelve triangles.
    fn cuboid(lo: V3, hi: V3) -> Vec<Triangle> {
        let c = |x: usize, y: usize, z: usize| -> V3 {
            [
                if x == 0 { lo[0] } else { hi[0] },
                if y == 0 { lo[1] } else { hi[1] },
                if z == 0 { lo[2] } else { hi[2] },
            ]
        };
        let quads = [
            [c(0, 0, 0), c(1, 0, 0), c(1, 1, 0), c(0, 1, 0)],
            [c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)],
            [c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)],
            [c(0, 1, 0), c(1, 1, 0), c(1, 1, 1), c(0, 1, 1)],
            [c(0, 0, 0), c(0, 1, 0), c(0, 1, 1), c(0, 0, 1)],
            [c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)],
        ];
        quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect()
    }

    #[test]
    fn a_box_lies_centred_on_the_axis_in_a_cylinder_round_it() {
        let block = cuboid([0.0, 0.0, 0.0], [40.0, 6.0, 8.0]);
        let (mesh, cyl) =
            lay_on_axis(&block, RotaryAxis::X, &RotaryFit::default(), 100.0).expect("laid");
        assert!((cyl.length - 40.0).abs() < 1e-9);
        assert!(
            (cyl.diameter - 10.0).abs() < 1e-9,
            "corner at (3, 4): {}",
            cyl.diameter
        );
        let along: Vec<f64> = mesh.iter().flatten().map(|p| p[0]).collect();
        assert!(along.iter().all(|a| (80.0..=120.0).contains(a)));
        let fit = RotaryFit {
            offset: [0.0, 4.0],
            along_scale: 0.5,
            radial_scale: 2.0,
        };
        let (_, cyl) = lay_on_axis(&block, RotaryAxis::X, &fit, 0.0).expect("laid");
        assert!((cyl.length - 20.0).abs() < 1e-9);
        // Across: 12 by 16, the axis 4 below the middle: (6, 12) is furthest.
        assert!((cyl.diameter - 2.0 * 180f64.sqrt()).abs() < 1e-9);
        let bad = RotaryFit {
            radial_scale: 0.0,
            ..RotaryFit::default()
        };
        assert!(lay_on_axis(&block, RotaryAxis::X, &bad, 0.0).is_err());
    }

    #[test]
    fn a_square_bar_reads_as_its_distance_from_the_axis_all_round() {
        let block = cuboid([0.0, -5.0, -5.0], [40.0, 5.0, 5.0]);
        let (mesh, _) =
            lay_on_axis(&block, RotaryAxis::X, &RotaryFit::default(), 20.0).expect("laid");
        let radius = 10.0;
        let circumference = std::f64::consts::TAU * radius;
        let grid = Grid {
            width: 60,
            height: (circumference / 0.5).round() as usize,
            cell: 0.5,
            origin: Point::new(-4.75, -circumference / 2.0 + 0.25),
        };
        let field = radial_field(&mesh, &grid, RotaryAxis::X, radius);
        let at = |around: f64| f64::from(field.at(Point::new(20.0, around)));
        // Straight up the flat face is 5 away; a quarter turn round, too;
        // at 45 degrees the edge is 5 root 2 away.
        assert!((at(0.0) - 5.0).abs() < 0.05, "{}", at(0.0));
        assert!((at(circumference / 4.0) - 5.0).abs() < 0.05);
        assert!((at(circumference / 8.0) - 50f64.sqrt()).abs() < 0.1);
        assert!(field.at(Point::new(-2.0, 0.0)).is_nan(), "past the end");
        assert!(field.at(Point::new(45.0, 0.0)).is_nan());
    }

    #[test]
    fn a_blank_along_y_reads_its_around_from_x() {
        let block = cuboid([-3.0, 0.0, -3.0], [3.0, 30.0, 3.0]);
        let (mesh, cyl) =
            lay_on_axis(&block, RotaryAxis::Y, &RotaryFit::default(), 15.0).expect("laid");
        assert!((cyl.length - 30.0).abs() < 1e-9);
        let grid = Grid {
            width: 40,
            height: 40,
            cell: 0.5,
            origin: Point::new(-9.75, 0.25),
        };
        let field = radial_field(&mesh, &grid, RotaryAxis::Y, 10.0);
        assert!((f64::from(field.at(Point::new(0.0, 15.0))) - 3.0).abs() < 0.05);
    }
}
