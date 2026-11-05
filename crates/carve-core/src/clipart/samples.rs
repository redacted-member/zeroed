//! The first reliefs of the library, made here from formulas: a rosette, a
//! leaf, a star dome and a shell. The library (see `library`) lists them in
//! their categories.
//!
//! Each is a relief on a 0.1 mm grid, a few centimetres across.

use crate::geom::Point;
use crate::model::field::Field;
use std::f64::consts::{PI, TAU};

use super::clip_file::Clip;

/// Cell size of every sample, in millimetres.
pub const SAMPLE_CELL: f64 = 0.1;

/// Names of the samples, in the order the library lists them.
pub const SAMPLE_NAMES: [&str; 4] = ["Rosette", "Leaf", "Star Dome", "Shell"];

/// Builds the named sample, or `None` for a name that is not a sample.
pub fn sample(name: &str) -> Option<Clip> {
    let field = match name {
        "Rosette" => rosette(),
        "Leaf" => leaf(),
        "Star Dome" => star_dome(),
        "Shell" => shell(),
        _ => return None,
    };
    Some(Clip {
        name: name.to_string(),
        field,
    })
}

/// A field of `width` by `height` millimetres on the sample grid, centred
/// on the origin, with heights from `height_at(x, y)` (NaN for none).
fn build(width: f64, height: f64, height_at: impl Fn(f64, f64) -> f64) -> Field {
    let w = (width / SAMPLE_CELL).round() as usize;
    let h = (height / SAMPLE_CELL).round() as usize;
    let origin = Point::new(
        -(w as f64 - 1.0) * SAMPLE_CELL / 2.0,
        -(h as f64 - 1.0) * SAMPLE_CELL / 2.0,
    );
    let mut field = Field::empty(w, h, SAMPLE_CELL, origin);
    for j in 0..h {
        for i in 0..w {
            let p = field.centre(i, j);
            let z = height_at(p.x, p.y);
            if z.is_finite() && z >= 0.0 {
                field.z[j * w + i] = z as f32;
            }
        }
    }
    field.cropped().unwrap_or(field)
}

/// Eight scalloped petals round a domed boss, 48 mm across.
fn rosette() -> Field {
    let outer = 23.0;
    build(48.0, 48.0, |x, y| {
        let r = x.hypot(y);
        let theta = y.atan2(x);
        let petal = (4.0 * theta).cos().abs();
        let edge = outer * (0.82 + 0.18 * petal.powf(0.6));
        if r > edge {
            return f64::NAN;
        }
        let t = r / edge;
        let body = 2.2 * (1.0 - t * t).sqrt();
        let ridge = 1.4 * petal.powi(6) * (PI * t).sin();
        let boss = 2.5 * (1.0 - (r / 6.0).powi(2)).max(0.0).sqrt();
        body + ridge + boss
    })
}

/// A pointed leaf with a raised midrib and veins, 60 by 30 mm.
fn leaf() -> Field {
    let (half_len, half_width) = (29.0, 14.0);
    build(60.0, 30.0, |x, y| {
        let u = x / half_len;
        if u.abs() >= 1.0 {
            return f64::NAN;
        }
        let w = half_width * (1.0 - u * u).powf(0.9) * (1.0 - 0.25 * u);
        let v = y / w;
        if v.abs() >= 1.0 {
            return f64::NAN;
        }
        let body = 2.0 * (1.0 - v * v).sqrt() * (1.0 - u * u).powf(0.3);
        let midrib = 0.9 * (-(y / 0.6).powi(2)).exp() * (1.0 - u.abs());
        let phase = 7.0 * (u + 0.45 * v.abs());
        let vein = 0.35 * (TAU * phase).cos().max(0.0).powi(8) * (1.0 - v.abs());
        body + midrib + vein
    })
}

/// A five pointed star rising to a rounded peak, 40 mm across.
fn star_dome() -> Field {
    let (outer, inner) = (19.5, 8.0);
    let corners: Vec<(f64, f64)> = (0..10)
        .map(|k| {
            let angle = PI / 2.0 + k as f64 * PI / 5.0;
            let r = if k % 2 == 0 { outer } else { inner };
            (r * angle.cos(), r * angle.sin())
        })
        .collect();
    // The distance from the centre to the outline is at most the inner
    // radius times the cosine of half a step.
    let peak = inner * (PI / 10.0).cos();
    build(40.0, 40.0, move |x, y| {
        if !inside(&corners, x, y) {
            return f64::NAN;
        }
        let d = distance_to_outline(&corners, x, y);
        let t = (d / peak).min(1.0);
        5.0 * (1.0 - (1.0 - t).powi(2))
    })
}

/// A scallop shell: a fan of ribs over a dome with a scalloped rim and a
/// hinge at the bottom, 64 by 40 mm.
fn shell() -> Field {
    let radius = 36.0;
    let hinge = Point::new(0.0, -18.5);
    let spread = 1.0;
    build(64.0, 40.0, move |x, y| {
        let (dx, dy) = (x - hinge.x, y - hinge.y);
        let r = dx.hypot(dy);
        let phi = dx.atan2(dy);
        let ears = (0.0..5.0).contains(&dy) && dx.abs() < 11.0 - dy;
        if phi.abs() > spread && !ears {
            return f64::NAN;
        }
        let ribs = 13.0;
        let rib = (ribs * phi / (2.0 * spread) * TAU).cos();
        let rim = radius * (0.95 + 0.05 * rib);
        if r > rim && !ears {
            return f64::NAN;
        }
        if ears && (phi.abs() > spread || r < 4.0) {
            return 0.8;
        }
        let t = (r / rim).min(1.0);
        let fan = 1.0 - (phi / spread).powi(2);
        let dome = 4.5 * (t * (1.0 - t)).max(0.0).sqrt() * 2.0 * fan.max(0.0).sqrt();
        let ridge = 0.6 * (0.5 + 0.5 * rib) * t;
        0.8 + dome + ridge
    })
}

/// Even-odd point in polygon.
fn inside(corners: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut odd = false;
    let n = corners.len();
    for k in 0..n {
        let (x0, y0) = corners[k];
        let (x1, y1) = corners[(k + 1) % n];
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) * (x1 - x0) / (y1 - y0) {
            odd = !odd;
        }
    }
    odd
}

/// Shortest distance from a point to the polygon's edges.
fn distance_to_outline(corners: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let n = corners.len();
    (0..n)
        .map(|k| {
            let (x0, y0) = corners[k];
            let (x1, y1) = corners[(k + 1) % n];
            let (ex, ey) = (x1 - x0, y1 - y0);
            let len2 = ex * ex + ey * ey;
            let t = (((x - x0) * ex + (y - y0) * ey) / len2).clamp(0.0, 1.0);
            (x - (x0 + t * ex)).hypot(y - (y0 + t * ey))
        })
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_has_material_and_sensible_heights() {
        for name in SAMPLE_NAMES {
            let clip = sample(name).expect("a sample");
            let field = &clip.field;
            assert!(field.width > 40 && field.height > 40, "{name} is too small");
            let max = field.max().expect("has material");
            assert!(max > 1.0 && max < 10.0, "{name} peaks at {max}");
            assert!(field.min().expect("has material") >= 0.0);
        }
        assert!(sample("Teapot").is_none());
    }
}
