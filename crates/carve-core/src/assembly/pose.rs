//! Rigid transforms in assembly space: a rotation matrix and a translation,
//! in millimetres.

/// A 3D vector or point in millimetres.
pub type Vec3 = [f64; 3];

/// A rigid transform: `p' = r * p + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Rotation, row major.
    pub r: [[f64; 3]; 3],
    /// Translation in millimetres.
    pub t: Vec3,
}

impl Default for Pose {
    fn default() -> Self {
        Pose::IDENTITY
    }
}

impl Pose {
    /// The transform that changes nothing.
    pub const IDENTITY: Pose = Pose {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    /// A pure translation.
    pub fn translation(t: Vec3) -> Pose {
        Pose {
            t,
            ..Pose::IDENTITY
        }
    }

    /// A frame: its columns are the images of the X, Y and Z axes and
    /// `origin` the image of the origin.
    pub fn frame(origin: Vec3, x: Vec3, y: Vec3, z: Vec3) -> Pose {
        Pose {
            r: [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]],
            t: origin,
        }
    }

    /// A rotation of `degrees` about the unit `axis` through the origin.
    pub fn rotation(axis: Vec3, degrees: f64) -> Pose {
        let a = normalized(axis);
        let (s, c) = degrees.to_radians().sin_cos();
        let k = 1.0 - c;
        let [x, y, z] = a;
        Pose {
            r: [
                [c + x * x * k, x * y * k - z * s, x * z * k + y * s],
                [y * x * k + z * s, c + y * y * k, y * z * k - x * s],
                [z * x * k - y * s, z * y * k + x * s, c + z * z * k],
            ],
            t: [0.0; 3],
        }
    }

    /// A rotation of `degrees` about the line through `point` along `axis`.
    pub fn rotation_about(point: Vec3, axis: Vec3, degrees: f64) -> Pose {
        Pose::translation(point)
            .then_local(&Pose::rotation(axis, degrees))
            .then_local(&Pose::translation(scale(point, -1.0)))
    }

    /// Rotations about X, then Y, then Z (fixed axes), in degrees.
    pub fn euler(degrees: Vec3) -> Pose {
        Pose::rotation([0.0, 0.0, 1.0], degrees[2])
            .then_local(&Pose::rotation([0.0, 1.0, 0.0], degrees[1]))
            .then_local(&Pose::rotation([1.0, 0.0, 0.0], degrees[0]))
    }

    /// The turns about X, then Y, then Z (fixed axes), in degrees, that
    /// [`Pose::euler`] makes this pose's rotation from. Y stays within -90
    /// to 90; when it is at either end the X turn is taken as zero.
    pub fn euler_angles(&self) -> Vec3 {
        let r = &self.r;
        let sin_y = (-r[2][0]).clamp(-1.0, 1.0);
        let y = sin_y.asin();
        let (x, z) = if y.cos() > 1e-7 {
            (r[2][1].atan2(r[2][2]), r[1][0].atan2(r[0][0]))
        } else {
            (0.0, (-r[0][1]).atan2(r[1][1]))
        };
        let tidy = |radians: f64| {
            let degrees = (radians.to_degrees() * 1e9).round() / 1e9;
            if degrees == 0.0 {
                0.0
            } else {
                degrees
            }
        };
        [tidy(x), tidy(y), tidy(z)]
    }

    /// `self * other`: applies `other` first, then `self`.
    pub fn then_local(&self, other: &Pose) -> Pose {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.r[i][k] * other.r[k][j]).sum();
            }
        }
        Pose {
            r,
            t: add(self.apply_vector(other.t), self.t),
        }
    }

    /// The inverse transform.
    pub fn inverse(&self) -> Pose {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = self.r[j][i];
            }
        }
        let back = Pose { r, t: [0.0; 3] };
        Pose {
            r,
            t: scale(back.apply_vector(self.t), -1.0),
        }
    }

    /// Transforms a point.
    pub fn apply(&self, p: Vec3) -> Vec3 {
        add(self.apply_vector(p), self.t)
    }

    /// Transforms a direction (no translation).
    pub fn apply_vector(&self, v: Vec3) -> Vec3 {
        [
            self.r[0][0] * v[0] + self.r[0][1] * v[1] + self.r[0][2] * v[2],
            self.r[1][0] * v[0] + self.r[1][1] * v[1] + self.r[1][2] * v[2],
            self.r[2][0] * v[0] + self.r[2][1] * v[1] + self.r[2][2] * v[2],
        ]
    }

    /// The transform as a column major 4x4 matrix, as three.js takes it.
    pub fn to_matrix(&self) -> [f64; 16] {
        [
            self.r[0][0],
            self.r[1][0],
            self.r[2][0],
            0.0,
            self.r[0][1],
            self.r[1][1],
            self.r[2][1],
            0.0,
            self.r[0][2],
            self.r[1][2],
            self.r[2][2],
            0.0,
            self.t[0],
            self.t[1],
            self.t[2],
            1.0,
        ]
    }
}

/// `a + b`.
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a - b`.
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `v * k`.
pub fn scale(v: Vec3, k: f64) -> Vec3 {
    [v[0] * k, v[1] * k, v[2] * k]
}

/// The cross product.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The dot product.
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v` scaled to unit length; the Z axis for a zero vector.
pub fn normalized(v: Vec3) -> Vec3 {
    let len = dot(v, v).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        scale(v, 1.0 / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn rotation_about_z_turns_x_into_y() {
        let p = Pose::rotation([0.0, 0.0, 1.0], 90.0).apply([1.0, 0.0, 0.0]);
        assert!(close(p, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn inverse_undoes_the_pose() {
        let pose = Pose::translation([5.0, -2.0, 7.0]).then_local(&Pose::euler([10.0, 20.0, 30.0]));
        let p = [3.0, 4.0, 5.0];
        assert!(close(pose.inverse().apply(pose.apply(p)), p));
    }

    #[test]
    fn euler_angles_give_back_the_rotation() {
        for angles in [
            [10.0, 20.0, 30.0],
            [-120.0, 45.0, 170.0],
            [0.0, 90.0, 30.0],
            [25.0, -90.0, 0.0],
            [90.0, 0.0, 180.0],
        ] {
            let pose = Pose::euler(angles);
            let back = Pose::euler(pose.euler_angles());
            for i in 0..3 {
                for j in 0..3 {
                    assert!((pose.r[i][j] - back.r[i][j]).abs() < 1e-9, "{angles:?}");
                }
            }
        }
    }

    #[test]
    fn rotation_about_a_line_keeps_points_on_it() {
        let hinge = Pose::rotation_about([10.0, 0.0, 0.0], [0.0, 1.0, 0.0], 90.0);
        assert!(close(hinge.apply([10.0, 5.0, 0.0]), [10.0, 5.0, 0.0]));
        assert!(close(hinge.apply([20.0, 0.0, 0.0]), [10.0, 0.0, -10.0]));
    }
}
