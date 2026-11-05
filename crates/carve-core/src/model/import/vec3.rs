//! Small 3D vector helpers for mesh import.

/// A point or direction in millimetres (or file units before scaling).
pub type V3 = [f64; 3];

/// `a - b`.
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a + b * k`.
pub fn add_scaled(a: V3, b: V3, k: f64) -> V3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// Dot product.
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Cross product.
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Length.
pub fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// `a` scaled to length one, or `a` itself when it has no length.
pub fn unit(a: V3) -> V3 {
    let l = length(a);
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        a
    }
}

/// Turns `v` about X by `a` radians (right handed).
pub fn rotate_x(v: V3, a: f64) -> V3 {
    if a == 0.0 {
        return v;
    }
    let (s, c) = a.sin_cos();
    [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c]
}

/// Turns `v` about Y by `a` radians (right handed).
pub fn rotate_y(v: V3, a: f64) -> V3 {
    if a == 0.0 {
        return v;
    }
    let (s, c) = a.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
}

/// Turns `v` about Z by `a` radians (right handed).
pub fn rotate_z(v: V3, a: f64) -> V3 {
    if a == 0.0 {
        return v;
    }
    let (s, c) = a.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
}

/// The normal of a triangle, not normalised (twice its area long).
pub fn normal(t: &[V3; 3]) -> V3 {
    cross(sub(t[1], t[0]), sub(t[2], t[0]))
}
