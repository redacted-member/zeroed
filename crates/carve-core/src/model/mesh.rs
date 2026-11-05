//! 3D model import: a triangle mesh turned the chosen way up, scaled to a
//! width, centred on a point, sat on the zero plane, and projected down
//! onto the model grid keeping the highest surface over each cell.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;
use crate::io::mesh::Triangle;

use super::field::Field;
use super::shape::Grid;

/// Which axis of the file points up out of the material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Up {
    /// The file's Z axis (most CAD and STL files).
    #[default]
    Z,
    /// The file's Y axis (many OBJ files).
    Y,
    /// The file's X axis.
    X,
}

/// The Import Component form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeshParams {
    /// Which way up the model goes.
    pub up: Up,
    /// Width along X the model is scaled to; the file's own size when absent.
    pub width: Option<f64>,
    /// Where the model's centre goes; the job centre when absent.
    pub centre: Option<Point>,
}

impl Default for MeshParams {
    fn default() -> Self {
        MeshParams {
            up: Up::Z,
            width: None,
            centre: None,
        }
    }
}

fn turned(v: [f64; 3], up: Up) -> [f64; 3] {
    match up {
        Up::Z => v,
        Up::Y => [v[0], -v[2], v[1]],
        Up::X => [v[1], v[2], v[0]],
    }
}

/// Projects `triangles` onto `grid`, with `job_centre` used when the
/// parameters give no centre. Answers the heights, NaN where no triangle is.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a flat or empty mesh or a width
/// that is not positive.
pub fn project(
    triangles: &[Triangle],
    grid: &Grid,
    params: &MeshParams,
    job_centre: Point,
) -> Result<Field, CoreError> {
    let turned: Vec<[[f64; 3]; 3]> = triangles
        .iter()
        .map(|t| {
            [
                turned(t[0], params.up),
                turned(t[1], params.up),
                turned(t[2], params.up),
            ]
        })
        .collect();
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in turned.iter().flatten() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v[axis]);
            hi[axis] = hi[axis].max(v[axis]);
        }
    }
    let span = hi[0] - lo[0];
    let depth = hi[1] - lo[1];
    if !span.is_finite() || span <= 0.0 || !depth.is_finite() || depth <= 0.0 {
        return Err(CoreError::validation(
            "the model has no size seen from above; try another way up",
        ));
    }
    let scale = match params.width {
        Some(width) if width.is_finite() && width > 0.0 => width / span,
        Some(_) => {
            return Err(CoreError::validation(
                "the model width must be more than zero",
            ))
        }
        None => 1.0,
    };
    let centre = params.centre.unwrap_or(job_centre);
    let (mid_x, mid_y) = ((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    let place = |v: [f64; 3]| {
        [
            centre.x + (v[0] - mid_x) * scale,
            centre.y + (v[1] - mid_y) * scale,
            (v[2] - lo[2]) * scale,
        ]
    };
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for t in &turned {
        let (a, b, c) = (place(t[0]), place(t[1]), place(t[2]));
        // Twice the signed area seen from above; skip edge-on triangles.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-12 {
            continue;
        }
        let min_x = a[0].min(b[0]).min(c[0]);
        let max_x = a[0].max(b[0]).max(c[0]);
        let min_y = a[1].min(b[1]).min(c[1]);
        let max_y = a[1].max(b[1]).max(c[1]);
        let i0 = ((min_x - grid.origin.x) / grid.cell).ceil().max(0.0) as usize;
        let j0 = ((min_y - grid.origin.y) / grid.cell).ceil().max(0.0) as usize;
        let i1 = (((max_x - grid.origin.x) / grid.cell).floor()).min(grid.width as f64 - 1.0);
        let j1 = (((max_y - grid.origin.y) / grid.cell).floor()).min(grid.height as f64 - 1.0);
        if i1 < 0.0 || j1 < 0.0 {
            continue;
        }
        for j in j0..=(j1 as usize) {
            for i in i0..=(i1 as usize) {
                let p = grid.origin;
                let (x, y) = (p.x + i as f64 * grid.cell, p.y + j as f64 * grid.cell);
                // Barycentric weights of the cell centre.
                let w_a = ((b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y)) / area;
                let w_b = ((c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y)) / area;
                let w_c = 1.0 - w_a - w_b;
                if w_a < -1e-9 || w_b < -1e-9 || w_c < -1e-9 {
                    continue;
                }
                let z = (w_a * a[2] + w_b * b[2] + w_c * c[2]) as f32;
                let slot = &mut field.z[j * grid.width + i];
                if slot.is_nan() || z > *slot {
                    *slot = z;
                }
            }
        }
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube as twelve triangles.
    fn cube() -> Vec<Triangle> {
        let v = |x: f64, y: f64, z: f64| [x, y, z];
        let quads = [
            [v(0., 0., 0.), v(1., 0., 0.), v(1., 1., 0.), v(0., 1., 0.)],
            [v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)],
            [v(0., 0., 0.), v(1., 0., 0.), v(1., 0., 1.), v(0., 0., 1.)],
            [v(0., 1., 0.), v(1., 1., 0.), v(1., 1., 1.), v(0., 1., 1.)],
            [v(0., 0., 0.), v(0., 1., 0.), v(0., 1., 1.), v(0., 0., 1.)],
            [v(1., 0., 0.), v(1., 1., 0.), v(1., 1., 1.), v(1., 0., 1.)],
        ];
        quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect()
    }

    fn grid() -> Grid {
        Grid {
            width: 100,
            height: 100,
            cell: 0.5,
            origin: Point::new(0.25, 0.25),
        }
    }

    #[test]
    fn a_unit_cube_scaled_to_ten_has_a_flat_top_ten_high() {
        let params = MeshParams {
            width: Some(10.0),
            ..MeshParams::default()
        };
        let field = project(&cube(), &grid(), &params, Point::new(25.0, 25.0)).expect("projects");
        assert_eq!(field.at(Point::new(25.0, 25.0)), 10.0);
        assert_eq!(field.at(Point::new(21.0, 29.0)), 10.0);
        assert!(
            field.at(Point::new(35.0, 25.0)).is_nan(),
            "nothing beyond the cube"
        );
        let cropped = field.cropped().expect("covers cells");
        assert!((cropped.width as f64 * 0.5 - 10.0).abs() <= 1.0);
    }

    #[test]
    fn a_sloped_triangle_is_interpolated() {
        let ramp = vec![[[0.0, 0.0, 0.0], [20.0, 0.0, 10.0], [0.0, 20.0, 0.0]]];
        let params = MeshParams {
            centre: Some(Point::new(10.0, 10.0)),
            ..MeshParams::default()
        };
        let field = project(&ramp, &grid(), &params, Point::ORIGIN).expect("projects");
        let z = f64::from(field.at(Point::new(10.25, 2.25)));
        assert!((z - 5.125).abs() < 0.3, "{z}");
    }

    #[test]
    fn turning_y_up_uses_the_files_y_as_height() {
        let tall = vec![[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 6.0, 4.0]]];
        let z_up = project(
            &tall,
            &grid(),
            &MeshParams::default(),
            Point::new(25.0, 25.0),
        )
        .expect("projects");
        let y_up = project(
            &tall,
            &grid(),
            &MeshParams {
                up: Up::Y,
                ..MeshParams::default()
            },
            Point::new(25.0, 25.0),
        )
        .expect("projects");
        // Peaks are single vertices, which cell centres only come near, so
        // compare the two orientations rather than exact heights.
        let (z, y) = (z_up.max().unwrap_or(0.0), y_up.max().unwrap_or(0.0));
        assert!(z <= 4.0 && y > z + 1.0, "z up {z}, y up {y}");
    }

    #[test]
    fn a_mesh_with_no_size_from_above_is_refused() {
        let wall = vec![[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]]];
        assert!(project(&wall, &grid(), &MeshParams::default(), Point::ORIGIN).is_err());
    }
}
