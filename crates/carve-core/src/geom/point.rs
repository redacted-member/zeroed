//! Points, vectors, affine transforms and the tolerance newtype.
//!
//! All coordinates are millimetres. Angles are radians inside this module and
//! degrees at the public API boundary of the kernel, so every function that
//! takes or returns an angle says which one it means.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// A position in job space, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Distance from the job origin along X, in millimetres.
    pub x: f64,
    /// Distance from the job origin along Y, in millimetres.
    pub y: f64,
}

/// A free vector (a direction with a magnitude), in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    /// X component, in millimetres.
    pub x: f64,
    /// Y component, in millimetres.
    pub y: f64,
}

impl Point {
    /// The job origin, (0, 0).
    pub const ORIGIN: Point = Point { x: 0.0, y: 0.0 };

    /// Builds a point from millimetre coordinates.
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// Vector from `self` to `other`, in millimetres.
    pub fn to(self, other: Point) -> Vec2 {
        Vec2::new(other.x - self.x, other.y - self.y)
    }

    /// Moves the point by a vector, in millimetres.
    pub fn offset(self, by: Vec2) -> Point {
        Point::new(self.x + by.x, self.y + by.y)
    }

    /// Straight line distance to another point, in millimetres.
    pub fn distance(self, other: Point) -> f64 {
        self.distance_squared(other).sqrt()
    }

    /// Squared distance to another point, in square millimetres.
    ///
    /// Cheaper than [`Point::distance`] when only comparisons are needed.
    pub fn distance_squared(self, other: Point) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }

    /// Linear interpolation, `t` = 0 gives `self` and `t` = 1 gives `other`.
    pub fn lerp(self, other: Point, t: f64) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    /// Midpoint of the straight line between two points.
    pub fn midpoint(self, other: Point) -> Point {
        self.lerp(other, 0.5)
    }

    /// True when both coordinates are within `tol` millimetres.
    pub fn approx_eq(self, other: Point, tol: Tolerance) -> bool {
        self.distance_squared(other) <= tol.get() * tol.get()
    }

    /// True when both coordinates are finite numbers.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    /// Builds a vector from millimetre components.
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    /// Unit vector at `angle` radians measured counter clockwise from +X.
    pub fn from_angle(angle: f64) -> Self {
        Vec2::new(angle.cos(), angle.sin())
    }

    /// Length of the vector, in millimetres.
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Squared length, in square millimetres.
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Angle in radians counter clockwise from +X, in the range -pi to pi.
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Dot product, in square millimetres.
    pub fn dot(self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Two dimensional cross product (the Z component of the 3D cross).
    ///
    /// Positive when `other` lies counter clockwise of `self`.
    pub fn cross(self, other: Vec2) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// The vector rotated a quarter turn counter clockwise.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    /// Scales the vector by a factor.
    pub fn scaled(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }

    /// Adds two vectors.
    pub fn plus(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x + other.x, self.y + other.y)
    }

    /// Subtracts `other` from `self`.
    pub fn minus(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x - other.x, self.y - other.y)
    }

    /// Rotates the vector by `angle` radians counter clockwise.
    pub fn rotated(self, angle: f64) -> Vec2 {
        let (s, c) = angle.sin_cos();
        Vec2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    /// Unit vector in the same direction, or `None` when the length is zero.
    pub fn normalised(self) -> Option<Vec2> {
        let len = self.length();
        if len <= f64::MIN_POSITIVE {
            None
        } else {
            Some(self.scaled(1.0 / len))
        }
    }

    /// True when both components are within `tol` millimetres.
    pub fn approx_eq(self, other: Vec2, tol: Tolerance) -> bool {
        self.minus(other).length_squared() <= tol.get() * tol.get()
    }
}

/// A positive distance used to decide "close enough", in millimetres.
///
/// Two defaults are used across the kernel: [`Tolerance::FLATTEN`] (0.01 mm)
/// when turning curves into line segments, and [`Tolerance::EQUALITY`]
/// (1e-6 mm) when deciding whether two coordinates are the same point.
///
/// A tolerance that arrives as JSON goes through the same check as
/// [`Tolerance::new`], so a zero or negative value is refused at the kernel
/// boundary rather than silently flattening every curve to a single chord.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
pub struct Tolerance(f64);

impl<'de> Deserialize<'de> for Tolerance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Tolerance::new(value).map_err(serde::de::Error::custom)
    }
}

impl Tolerance {
    /// Default chord tolerance for flattening curves, 0.01 mm.
    pub const FLATTEN: Tolerance = Tolerance(0.01);
    /// Default tolerance for coordinate equality, 1e-6 mm.
    pub const EQUALITY: Tolerance = Tolerance(1e-6);

    /// Builds a tolerance, rejecting values that are not finite and positive.
    pub fn new(value: f64) -> Result<Self, CoreError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(CoreError::geometry(format!(
                "tolerance must be a positive finite number of millimeters, got {value}"
            )));
        }
        Ok(Tolerance(value))
    }

    /// The tolerance in millimetres.
    pub fn get(self) -> f64 {
        self.0
    }
}

impl Default for Tolerance {
    fn default() -> Self {
        Tolerance::FLATTEN
    }
}

/// A 2D affine transform stored as the six live entries of a 3x3 matrix.
///
/// A point is mapped as `x' = a * x + c * y + e` and `y' = b * x + d * y + f`,
/// the same layout as an SVG or PostScript matrix.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mat3 {
    /// Row 0 column 0.
    pub a: f64,
    /// Row 1 column 0.
    pub b: f64,
    /// Row 0 column 1.
    pub c: f64,
    /// Row 1 column 1.
    pub d: f64,
    /// Row 0 column 2, the X translation in millimetres.
    pub e: f64,
    /// Row 1 column 2, the Y translation in millimetres.
    pub f: f64,
}

impl Default for Mat3 {
    fn default() -> Self {
        Mat3::IDENTITY
    }
}

impl Mat3 {
    /// The transform that changes nothing.
    pub const IDENTITY: Mat3 = Mat3 {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Builds a transform from the six live matrix entries.
    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Mat3 { a, b, c, d, e, f }
    }

    /// Translation by a vector, in millimetres.
    pub fn translate(by: Vec2) -> Self {
        Mat3::new(1.0, 0.0, 0.0, 1.0, by.x, by.y)
    }

    /// Rotation about the origin by `angle` radians counter clockwise.
    pub fn rotate(angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Mat3::new(c, s, -s, c, 0.0, 0.0)
    }

    /// Rotation by `angle` radians counter clockwise about `centre`.
    pub fn rotate_about(angle: f64, centre: Point) -> Self {
        Mat3::translate(Vec2::new(-centre.x, -centre.y))
            .then(&Mat3::rotate(angle))
            .then(&Mat3::translate(Vec2::new(centre.x, centre.y)))
    }

    /// Scale about the origin, independently in X and Y.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Mat3::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// Scale about `centre`, independently in X and Y.
    pub fn scale_about(sx: f64, sy: f64, centre: Point) -> Self {
        Mat3::translate(Vec2::new(-centre.x, -centre.y))
            .then(&Mat3::scale(sx, sy))
            .then(&Mat3::translate(Vec2::new(centre.x, centre.y)))
    }

    /// Reflection across the line through `on` with direction `along`.
    ///
    /// Returns an error when `along` has zero length, which is not a line.
    pub fn mirror_across_line(on: Point, along: Vec2) -> Result<Self, CoreError> {
        let dir = along.normalised().ok_or_else(|| {
            CoreError::geometry("a mirror line needs a direction with non zero length")
        })?;
        let (dx, dy) = (dir.x, dir.y);
        let reflect = Mat3::new(
            dx * dx - dy * dy,
            2.0 * dx * dy,
            2.0 * dx * dy,
            dy * dy - dx * dx,
            0.0,
            0.0,
        );
        Ok(Mat3::translate(Vec2::new(-on.x, -on.y))
            .then(&reflect)
            .then(&Mat3::translate(Vec2::new(on.x, on.y))))
    }

    /// The transform that applies `self` first and then `next`.
    pub fn then(&self, next: &Mat3) -> Mat3 {
        Mat3 {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// Determinant of the linear part. Negative means the transform mirrors.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// The inverse transform, or an error when the transform is singular.
    pub fn invert(&self) -> Result<Mat3, CoreError> {
        let det = self.determinant();
        if det.abs() <= f64::MIN_POSITIVE {
            return Err(CoreError::geometry(
                "transform is singular and cannot be inverted",
            ));
        }
        let inv = 1.0 / det;
        Ok(Mat3 {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        })
    }

    /// Maps a point through the transform.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// Maps a free vector through the linear part only, ignoring translation.
    pub fn apply_vector(&self, v: Vec2) -> Vec2 {
        Vec2::new(self.a * v.x + self.c * v.y, self.b * v.x + self.d * v.y)
    }

    /// True when the transform is a similarity: a rotation, an optional
    /// mirror, a uniform scale and a translation, with no shear.
    ///
    /// Circular arcs stay circular arcs only under a similarity.
    pub fn is_similarity(&self) -> bool {
        let col0 = self.a * self.a + self.b * self.b;
        let col1 = self.c * self.c + self.d * self.d;
        let dot = self.a * self.c + self.b * self.d;
        let scale = col0.max(col1).max(1.0);
        (col0 - col1).abs() <= 1e-12 * scale && dot.abs() <= 1e-12 * scale
    }

    /// Uniform scale factor of a similarity transform.
    ///
    /// For a transform that is not a similarity this returns the geometric
    /// mean of the two axis scales, which is the best single number available.
    pub fn uniform_scale(&self) -> f64 {
        self.determinant().abs().sqrt()
    }

    /// True when the transform reverses orientation (it contains a mirror).
    pub fn is_mirroring(&self) -> bool {
        self.determinant() < 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Tolerance = Tolerance::EQUALITY;

    #[test]
    fn point_distance_is_pythagorean() {
        assert!((Point::new(0.0, 0.0).distance(Point::new(3.0, 4.0)) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn point_lerp_hits_both_ends() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(5.0, 10.0);
        assert!(a.lerp(b, 0.0).approx_eq(a, TOL));
        assert!(a.lerp(b, 1.0).approx_eq(b, TOL));
        assert!(a.midpoint(b).approx_eq(Point::new(3.0, 6.0), TOL));
    }

    #[test]
    fn point_offset_and_to_are_inverses() {
        let a = Point::new(-2.0, 7.0);
        let b = Point::new(4.5, -1.25);
        assert!(a.offset(a.to(b)).approx_eq(b, TOL));
    }

    #[test]
    fn vec_cross_sign_tells_turn_direction() {
        let right = Vec2::new(1.0, 0.0);
        assert!(right.cross(Vec2::new(0.0, 1.0)) > 0.0);
        assert!(right.cross(Vec2::new(0.0, -1.0)) < 0.0);
        assert_eq!(right.cross(Vec2::new(2.0, 0.0)), 0.0);
    }

    #[test]
    fn vec_normalise_rejects_zero() {
        assert!(Vec2::ZERO.normalised().is_none());
        let n = Vec2::new(3.0, 4.0).normalised().expect("non zero");
        assert!((n.length() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn vec_rotation_by_quarter_turn_matches_perp() {
        let v = Vec2::new(2.0, 1.0);
        assert!(v
            .rotated(std::f64::consts::FRAC_PI_2)
            .approx_eq(v.perp(), TOL));
    }

    #[test]
    fn vec_from_angle_round_trips() {
        let a = 0.7;
        assert!((Vec2::from_angle(a).angle() - a).abs() < 1e-12);
    }

    #[test]
    fn tolerance_rejects_zero_and_negative() {
        assert!(Tolerance::new(0.0).is_err());
        assert!(Tolerance::new(-1.0).is_err());
        assert!(Tolerance::new(f64::NAN).is_err());
        assert!((Tolerance::default().get() - 0.01).abs() < 1e-15);
    }

    #[test]
    fn identity_leaves_points_alone() {
        let p = Point::new(3.0, -4.0);
        assert!(Mat3::IDENTITY.apply(p).approx_eq(p, TOL));
    }

    #[test]
    fn translate_moves_by_the_vector() {
        let m = Mat3::translate(Vec2::new(2.0, 3.0));
        assert!(m
            .apply(Point::new(1.0, 1.0))
            .approx_eq(Point::new(3.0, 4.0), TOL));
    }

    #[test]
    fn rotate_about_keeps_the_centre_fixed() {
        let centre = Point::new(5.0, 5.0);
        let m = Mat3::rotate_about(std::f64::consts::FRAC_PI_3, centre);
        assert!(m.apply(centre).approx_eq(centre, TOL));
        let moved = m.apply(Point::new(6.0, 5.0));
        assert!((moved.distance(centre) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn scale_about_keeps_the_centre_fixed() {
        let centre = Point::new(-1.0, 2.0);
        let m = Mat3::scale_about(2.0, 3.0, centre);
        assert!(m.apply(centre).approx_eq(centre, TOL));
        assert!(m
            .apply(Point::new(0.0, 3.0))
            .approx_eq(Point::new(1.0, 5.0), TOL));
    }

    #[test]
    fn mirror_across_x_axis_flips_y() {
        let m = Mat3::mirror_across_line(Point::ORIGIN, Vec2::new(1.0, 0.0)).expect("line");
        assert!(m
            .apply(Point::new(2.0, 3.0))
            .approx_eq(Point::new(2.0, -3.0), TOL));
        assert!(m.is_mirroring());
    }

    #[test]
    fn mirror_needs_a_direction() {
        assert!(Mat3::mirror_across_line(Point::ORIGIN, Vec2::ZERO).is_err());
    }

    #[test]
    fn compose_applies_self_then_next() {
        let move_then_scale = Mat3::translate(Vec2::new(1.0, 0.0)).then(&Mat3::scale(2.0, 2.0));
        assert!(move_then_scale
            .apply(Point::ORIGIN)
            .approx_eq(Point::new(2.0, 0.0), TOL));
    }

    #[test]
    fn invert_undoes_the_transform() {
        let m = Mat3::translate(Vec2::new(3.0, -2.0))
            .then(&Mat3::rotate(0.4))
            .then(&Mat3::scale(2.0, 5.0));
        let back = m.invert().expect("invertible");
        let p = Point::new(7.0, 1.0);
        assert!(back.apply(m.apply(p)).approx_eq(p, TOL));
    }

    #[test]
    fn singular_transform_cannot_be_inverted() {
        assert!(Mat3::scale(0.0, 1.0).invert().is_err());
    }

    #[test]
    fn similarity_detection_matches_expectation() {
        assert!(Mat3::rotate(0.3)
            .then(&Mat3::scale(2.0, 2.0))
            .is_similarity());
        assert!(!Mat3::scale(2.0, 3.0).is_similarity());
        let mirror = Mat3::mirror_across_line(Point::ORIGIN, Vec2::new(1.0, 1.0)).expect("line");
        assert!(mirror.is_similarity());
        assert!((Mat3::scale(3.0, 3.0).uniform_scale() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn apply_vector_ignores_translation() {
        let m = Mat3::translate(Vec2::new(10.0, 10.0));
        assert!(m
            .apply_vector(Vec2::new(1.0, 0.0))
            .approx_eq(Vec2::new(1.0, 0.0), TOL));
    }
}
