//! Cross sections of the relief model: the heights along a straight line
//! across the job, drawn as a vector so the profile can be checked, copied
//! or used as a sweep section.

use crate::error::CoreError;
use crate::geom::{Path, Point, Polyline};

use super::field::Field;

/// Most samples a section takes, whatever the line's length.
pub const MAX_SAMPLES: usize = 20_000;

/// Height under a job position, blended from the four nearest cells. Empty
/// cells are left out of the blend; `None` when all four are empty or
/// outside the field.
pub fn sample(field: &Field, p: Point) -> Option<f64> {
    let fx = (p.x - field.origin.x) / field.cell;
    let fy = (p.y - field.origin.y) / field.cell;
    if !fx.is_finite() || !fy.is_finite() {
        return None;
    }
    let (i, j) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - i, fy - j);
    let (i, j) = (i as i64, j as i64);
    let corners = [
        (field.get(i, j), (1.0 - tx) * (1.0 - ty)),
        (field.get(i + 1, j), tx * (1.0 - ty)),
        (field.get(i, j + 1), (1.0 - tx) * ty),
        (field.get(i + 1, j + 1), tx * ty),
    ];
    let (mut sum, mut weight) = (0.0, 0.0);
    for (z, w) in corners {
        if !z.is_nan() && w > 0.0 {
            sum += f64::from(z) * w;
            weight += w;
        }
    }
    if weight > 1e-12 {
        Some(sum / weight)
    } else {
        // A point right on an empty cell's neighbour: fall back to nearest.
        let z = field.at(p);
        (!z.is_nan()).then_some(f64::from(z))
    }
}

/// The model's profile from `from` to `to`: points of (distance along the
/// line, height), in millimetres, about two samples per cell. Where the
/// field has nothing the height is 0, the model's zero plane.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the two points are not numbers or
/// are closer together than a cell.
pub fn profile(field: &Field, from: Point, to: Point) -> Result<Vec<Point>, CoreError> {
    if !from.is_finite() || !to.is_finite() {
        return Err(CoreError::validation(
            "the section's end points must be numbers",
        ));
    }
    let length = from.distance(to);
    if length < field.cell {
        return Err(CoreError::validation(
            "click two points further apart to take a cross section",
        ));
    }
    let steps = ((length / (field.cell / 2.0)).ceil() as usize).clamp(2, MAX_SAMPLES);
    let mut out = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let height = sample(field, from.lerp(to, t)).unwrap_or(0.0);
        out.push(Point::new(length * t, height));
    }
    Ok(out)
}

/// Keeps only the points a polyline needs to stay within `tol` of the
/// original (Douglas and Peucker), always keeping both ends.
pub fn simplify(points: &[Point], tol: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (points[a], points[b]);
        let along = pa.to(pb);
        let len = along.length();
        let (mut worst, mut at) = (0.0, a);
        for (k, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = if len < 1e-12 {
                p.distance(pa)
            } else {
                (along.cross(pa.to(*p)) / len).abs()
            };
            if d > worst {
                worst = d;
                at = k;
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// The profile as an open vector: distance along the section runs along X
/// from `origin`, height times `scale` along Y. Straight runs are merged
/// to within a micron.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a scale that is not a number more
/// than 0, or a profile of fewer than two points.
pub fn section_path(profile: &[Point], origin: Point, scale: f64) -> Result<Path, CoreError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(CoreError::validation(
            "the height scale must be a number more than 0",
        ));
    }
    if !origin.is_finite() {
        return Err(CoreError::validation("the position must be a number"));
    }
    let placed: Vec<Point> = profile
        .iter()
        .map(|p| Point::new(origin.x + p.x, origin.y + p.y * scale))
        .collect();
    let points = simplify(&placed, 0.001);
    Polyline::new(points, false)
        .to_subpath()
        .filter(|s| !s.is_empty())
        .map(|s| s.into_path())
        .ok_or_else(|| CoreError::validation("the section has no length"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ramp rising 1 mm per mm along X, 20 by 5 cells of 1 mm.
    fn ramp() -> Field {
        let mut field = Field::empty(20, 5, 1.0, Point::new(0.0, 0.0));
        for j in 0..5 {
            for i in 0..20 {
                field.z[j * 20 + i] = i as f32;
            }
        }
        field
    }

    #[test]
    fn samples_blend_between_cells() {
        let field = ramp();
        let z = sample(&field, Point::new(3.25, 2.0)).expect("inside");
        assert!((z - 3.25).abs() < 1e-9);
        assert!(sample(&field, Point::new(-5.0, -5.0)).is_none());
    }

    #[test]
    fn empty_cells_are_left_out_of_the_blend() {
        let mut field = ramp();
        field.z[2 * 20 + 4] = f32::NAN;
        let z = sample(&field, Point::new(3.5, 2.0)).expect("one side set");
        assert!((z - 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_profile_follows_the_heights_along_the_line() {
        let field = ramp();
        let points = profile(&field, Point::new(2.0, 2.0), Point::new(12.0, 2.0)).expect("ok");
        assert_eq!(points.first().map(|p| p.x), Some(0.0));
        let last = points.last().copied().expect("points");
        assert!((last.x - 10.0).abs() < 1e-9);
        assert!((last.y - 12.0).abs() < 1e-9);
        for p in &points {
            assert!((p.y - (p.x + 2.0)).abs() < 1e-6, "{p:?}");
        }
    }

    #[test]
    fn outside_the_field_reads_as_the_zero_plane() {
        let field = ramp();
        let points = profile(&field, Point::new(-10.0, 2.0), Point::new(5.0, 2.0)).expect("ok");
        assert_eq!(points[0].y, 0.0);
    }

    #[test]
    fn short_or_bad_lines_are_refused() {
        let field = ramp();
        assert!(profile(&field, Point::new(1.0, 1.0), Point::new(1.2, 1.0)).is_err());
        assert!(profile(&field, Point::new(f64::NAN, 1.0), Point::new(5.0, 1.0)).is_err());
    }

    #[test]
    fn straight_runs_collapse_to_their_ends() {
        let field = ramp();
        let points = profile(&field, Point::new(2.0, 2.0), Point::new(12.0, 2.0)).expect("ok");
        let path = section_path(&points, Point::new(100.0, 50.0), 2.0).expect("path");
        let nodes = path.subpaths[0].nodes();
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].approx_eq(Point::new(100.0, 54.0), Default::default()));
        assert!(nodes[1].approx_eq(Point::new(110.0, 74.0), Default::default()));
        assert!(!path.subpaths[0].closed);
    }

    #[test]
    fn simplify_keeps_corners() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(2.0, 2.0),
        ];
        let kept = simplify(&points, 1e-6);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[1], Point::new(2.0, 0.0));
    }

    #[test]
    fn a_bad_scale_is_refused() {
        let points = [Point::new(0.0, 0.0), Point::new(1.0, 1.0)];
        assert!(section_path(&points, Point::ORIGIN, 0.0).is_err());
        assert!(section_path(&points, Point::ORIGIN, f64::NAN).is_err());
    }
}
