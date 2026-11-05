//! Orientate Model: which face of a mesh ends up on top, turns about the
//! three axes, the size it is scaled to, an optional perspective along Z,
//! and where on the job it sits. The result is the mesh in job millimetres,
//! centred on its placement point, standing on Z zero.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;
use crate::io::mesh::Triangle;

use super::vec3::{self, V3};

/// The face of the model, in the file's own axes, that becomes the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopFace {
    /// The face looking along the file's +Z (no change).
    #[default]
    Top,
    /// The face looking along -Z: the model upside down.
    Bottom,
    /// The face looking along -Y.
    Front,
    /// The face looking along +Y (how many Y up files are drawn).
    Back,
    /// The face looking along -X.
    Left,
    /// The face looking along +X.
    Right,
}

impl TopFace {
    /// Turns a file point so that this face looks up.
    pub fn turn(self, v: V3) -> V3 {
        let [x, y, z] = v;
        match self {
            TopFace::Top => [x, y, z],
            TopFace::Bottom => [x, -y, -z],
            TopFace::Front => [x, z, -y],
            TopFace::Back => [x, -z, y],
            TopFace::Left => [z, y, -x],
            TopFace::Right => [-z, y, x],
        }
    }
}

/// The Orientate Model settings. Angles are degrees, lengths millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Orientation {
    /// Which face of the model is on top.
    pub top: TopFace,
    /// Turn about X after the face is chosen, degrees.
    pub rot_x: f64,
    /// Turn about Y after the X turn, degrees.
    pub rot_y: f64,
    /// Turn about Z last, degrees; the quick rotate buttons add to it.
    pub rot_z: f64,
    /// Size along X, Y and Z the turned model is scaled to, millimetres.
    /// `None` keeps the file's numbers, multiplied by `unit_scale`.
    pub size: Option<V3>,
    /// Millimetres per file unit, used when `size` is absent: 1 for files
    /// drawn in millimetres, 25.4 for inches.
    pub unit_scale: f64,
    /// Perspective along Z, 0 for none to 1 for strongest: the lower a
    /// point, the more it is drawn in towards the model's centre.
    pub perspective: f64,
    /// Where the model's centre goes; the job centre when absent.
    pub centre: Option<Point>,
}

impl Default for Orientation {
    fn default() -> Self {
        Orientation {
            top: TopFace::Top,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            size: None,
            unit_scale: 1.0,
            perspective: 0.0,
            centre: None,
        }
    }
}

/// The oriented mesh and its extent.
#[derive(Debug, Clone, PartialEq)]
pub struct Oriented {
    /// Triangles in job millimetres, centred on the placement point in X
    /// and Y, from Z zero up.
    pub triangles: Vec<Triangle>,
    /// Extent along X, Y and Z in millimetres.
    pub size: V3,
    /// Extent of the turned model in the file's units, before scaling.
    pub natural: V3,
    /// The placement point.
    pub centre: Point,
}

/// Turns one file point: the top face, then X, Y and Z turns.
fn turned(v: V3, o: &Orientation) -> V3 {
    let v = o.top.turn(v);
    let v = vec3::rotate_x(v, o.rot_x.to_radians());
    let v = vec3::rotate_y(v, o.rot_y.to_radians());
    vec3::rotate_z(v, o.rot_z.to_radians())
}

/// The extent of `triangles` once turned as `o` says, in file units.
pub fn natural_size(triangles: &[Triangle], o: &Orientation) -> V3 {
    let (lo, hi) = bounds(triangles.iter().flatten().map(|v| turned(*v, o)));
    vec3::sub(hi, lo)
}

fn bounds(points: impl Iterator<Item = V3>) -> (V3, V3) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in points {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v[axis]);
            hi[axis] = hi[axis].max(v[axis]);
        }
    }
    (lo, hi)
}

fn check(o: &Orientation) -> Result<(), CoreError> {
    let angles = [o.rot_x, o.rot_y, o.rot_z];
    if angles.iter().any(|a| !a.is_finite()) {
        return Err(CoreError::validation("the rotation angles must be numbers"));
    }
    if !o.unit_scale.is_finite() || o.unit_scale <= 0.0 {
        return Err(CoreError::validation(
            "the model units must scale by more than zero",
        ));
    }
    if !o.perspective.is_finite() || !(0.0..=1.0).contains(&o.perspective) {
        return Err(CoreError::validation(
            "the perspective must be between 0 and 1",
        ));
    }
    if let Some(size) = o.size {
        if size.iter().any(|s| !s.is_finite() || *s < 0.0) || size[0] <= 0.0 || size[1] <= 0.0 {
            return Err(CoreError::validation(
                "the model size must be more than zero",
            ));
        }
    }
    Ok(())
}

/// Orients `triangles` as `o` says, centring the result on `o.centre` or
/// `job_centre`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for settings out of range or a model
/// that has no size seen from above once turned.
pub fn orient(
    triangles: &[Triangle],
    o: &Orientation,
    job_centre: Point,
) -> Result<Oriented, CoreError> {
    check(o)?;
    let turned_tris: Vec<Triangle> = triangles
        .iter()
        .map(|t| [turned(t[0], o), turned(t[1], o), turned(t[2], o)])
        .collect();
    let (lo, hi) = bounds(turned_tris.iter().flatten().copied());
    let natural = vec3::sub(hi, lo);
    if !natural[0].is_finite() || natural[0] <= 0.0 || natural[1] <= 0.0 {
        return Err(CoreError::validation(
            "the model has no size seen from above; try another face on top",
        ));
    }
    let scale = match o.size {
        None => [o.unit_scale; 3],
        Some(size) => {
            let sx = size[0] / natural[0];
            let sy = size[1] / natural[1];
            // A flat model keeps its (zero) thickness whatever Z says.
            let sz = if natural[2] > 1e-12 {
                size[2] / natural[2]
            } else {
                (sx + sy) / 2.0
            };
            [sx, sy, sz]
        }
    };
    let centre = o.centre.unwrap_or(job_centre);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let height = natural[2] * scale[2];
    let p = o.perspective;
    let place = |v: V3| -> V3 {
        let z = (v[2] - lo[2]) * scale[2];
        // Lower points are drawn in, the top keeps its size.
        let t = if height > 1e-12 { z / height } else { 1.0 };
        let k = 1.0 - 0.5 * p * (1.0 - t);
        [
            centre.x + (v[0] - mid[0]) * scale[0] * k,
            centre.y + (v[1] - mid[1]) * scale[1] * k,
            z,
        ]
    };
    let out: Vec<Triangle> = turned_tris
        .iter()
        .map(|t| [place(t[0]), place(t[1]), place(t[2])])
        .collect();
    Ok(Oriented {
        triangles: out,
        size: [natural[0] * scale[0], natural[1] * scale[1], height],
        natural,
        centre,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slab() -> Vec<Triangle> {
        // A 10 by 20 by 4 box's top and bottom, enough for the bounds.
        vec![
            [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 20.0, 4.0]],
            [[0.0, 0.0, 0.0], [10.0, 20.0, 4.0], [0.0, 20.0, 4.0]],
        ]
    }

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn each_face_on_top_swaps_the_axes() {
        let s = |top| {
            natural_size(
                &slab(),
                &Orientation {
                    top,
                    ..Orientation::default()
                },
            )
        };
        assert!(close(s(TopFace::Top), [10.0, 20.0, 4.0]));
        assert!(close(s(TopFace::Bottom), [10.0, 20.0, 4.0]));
        assert!(close(s(TopFace::Front), [10.0, 4.0, 20.0]));
        assert!(close(s(TopFace::Back), [10.0, 4.0, 20.0]));
        assert!(close(s(TopFace::Left), [4.0, 20.0, 10.0]));
        assert!(close(s(TopFace::Right), [4.0, 20.0, 10.0]));
        // Every face turn keeps a right handed frame.
        for top in [TopFace::Front, TopFace::Back, TopFace::Left, TopFace::Right] {
            let [a, b, c] = [
                top.turn([1.0, 0.0, 0.0]),
                top.turn([0.0, 1.0, 0.0]),
                top.turn([0.0, 0.0, 1.0]),
            ];
            assert!(close(vec3::cross(a, b), c), "{top:?}");
        }
    }

    #[test]
    fn a_quarter_turn_about_z_swaps_width_and_depth() {
        let o = Orientation {
            rot_z: 90.0,
            ..Orientation::default()
        };
        assert!(close(natural_size(&slab(), &o), [20.0, 10.0, 4.0]));
    }

    #[test]
    fn size_scales_each_axis_and_centres_on_the_job() {
        let o = Orientation {
            size: Some([20.0, 40.0, 2.0]),
            ..Orientation::default()
        };
        let out = orient(&slab(), &o, Point::new(100.0, 50.0)).expect("orients");
        assert!(close(out.size, [20.0, 40.0, 2.0]));
        let (lo, hi) = bounds(out.triangles.iter().flatten().copied());
        assert!(close(lo, [90.0, 30.0, 0.0]));
        assert!(close(hi, [110.0, 70.0, 2.0]));
    }

    #[test]
    fn inches_scale_the_file_numbers_by_25_4() {
        let o = Orientation {
            unit_scale: 25.4,
            ..Orientation::default()
        };
        let out = orient(&slab(), &o, Point::ORIGIN).expect("orients");
        assert!(close(out.size, [254.0, 508.0, 101.6]));
    }

    #[test]
    fn perspective_draws_the_bottom_in_and_keeps_the_top() {
        let o = Orientation {
            perspective: 1.0,
            ..Orientation::default()
        };
        let out = orient(&slab(), &o, Point::ORIGIN).expect("orients");
        // The bottom corner (z 0) is pulled half way in, the top one is not.
        assert!(close(out.triangles[0][0], [-2.5, -5.0, 0.0]));
        assert!(close(out.triangles[0][2], [5.0, 10.0, 4.0]));
    }

    #[test]
    fn a_model_seen_edge_on_is_refused() {
        let wall = vec![[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]]];
        assert!(orient(&wall, &Orientation::default(), Point::ORIGIN).is_err());
        let o = Orientation {
            top: TopFace::Front,
            ..Orientation::default()
        };
        assert!(orient(&wall, &o, Point::ORIGIN).is_ok());
        let bad = Orientation {
            perspective: 2.0,
            ..Orientation::default()
        };
        assert!(orient(&slab(), &bad, Point::ORIGIN).is_err());
    }
}
