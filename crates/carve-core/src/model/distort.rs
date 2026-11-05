//! Bending a relief component through a distortion envelope.
//!
//! The result stays on the component's own grid lattice, as a transformed
//! component does, so it bakes like one made in place. Every cell of the
//! result whose centre lies inside the bent envelope is filled by running
//! the envelope's map backwards to the original box and sampling the source
//! there. Heights are never changed; only the footprint bends.

use crate::error::CoreError;
use crate::geom::distort::Envelope;
use crate::geom::Point;

use super::field::Field;
use super::transform::MAX_TRANSFORM_CELLS;

/// Points along each side of the envelope outline used to find which cells
/// lie inside it.
const OUTLINE_POINTS: usize = 128;

/// Normalised positions sampled per side to seed the inverse map.
const SEED_GRID: usize = 32;

/// How far outside `0..=1` a normalised position may be and still count
/// as inside, to keep cells on the edge.
const EDGE_SLACK: f64 = 1e-6;

/// Bends a component's heights through `envelope`.
///
/// The result covers the box round the bent envelope on `field`'s lattice;
/// cells outside the bent envelope, or whose source position has no
/// material, are empty. Material of the source outside the envelope's
/// original box is left behind.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a field with no cells or a bad
/// cell size, and a result larger than [`MAX_TRANSFORM_CELLS`].
pub fn distort_field(field: &Field, envelope: &Envelope) -> Result<Field, CoreError> {
    if field.width == 0
        || field.height == 0
        || field.z.len() != field.width * field.height
        || !(field.cell.is_finite() && field.cell > 0.0)
        || !field.origin.is_finite()
    {
        return Err(CoreError::validation(
            "the component has no heights to distort",
        ));
    }
    let cell = field.cell;
    let bounds = envelope.mapped_bbox(SEED_GRID);
    let i0 = ((bounds.min.x - field.origin.x) / cell).floor();
    let i1 = ((bounds.max.x - field.origin.x) / cell).ceil();
    let j0 = ((bounds.min.y - field.origin.y) / cell).floor();
    let j1 = ((bounds.max.y - field.origin.y) / cell).ceil();
    let width = (i1 - i0 + 1.0).max(1.0);
    let height = (j1 - j0 + 1.0).max(1.0);
    if !(width.is_finite() && height.is_finite()) || width * height > MAX_TRANSFORM_CELLS as f64 {
        return Err(CoreError::validation(
            "the distorted component would be too large",
        ));
    }
    let (width, height) = (width as usize, height as usize);
    let origin = Point::new(field.origin.x + i0 * cell, field.origin.y + j0 * cell);
    let mut out = Field::empty(width, height, cell, origin);

    let outline = envelope.outline(OUTLINE_POINTS);
    let seeds = seed_table(envelope);
    let tol = cell * 1e-4;
    let mut crossings: Vec<f64> = Vec::new();
    for (j, row) in out.z.chunks_exact_mut(width).enumerate() {
        let y = origin.y + j as f64 * cell;
        row_crossings(&outline, y, &mut crossings);
        for span in crossings.chunks_exact(2) {
            let first = ((span[0] - origin.x) / cell).ceil().max(0.0) as usize;
            let last = ((span[1] - origin.x) / cell).floor();
            if last < 0.0 {
                continue;
            }
            let last = (last as usize).min(width - 1);
            let mut guess: Option<(f64, f64)> = None;
            for (i, slot) in row.iter_mut().enumerate().take(last + 1).skip(first) {
                let q = Point::new(origin.x + i as f64 * cell, y);
                let found = guess
                    .and_then(|g| envelope.inverse_uv(q, g, tol))
                    .or_else(|| envelope.inverse_uv(q, nearest_seed(&seeds, q), tol));
                guess = found;
                let Some((u, v)) = found else {
                    continue;
                };
                let inside = -EDGE_SLACK..=1.0 + EDGE_SLACK;
                if !(inside.contains(&u) && inside.contains(&v)) {
                    continue;
                }
                let p = envelope.denormalise(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                *slot = sample(
                    field,
                    (p.x - field.origin.x) / cell,
                    (p.y - field.origin.y) / cell,
                );
            }
        }
    }
    Ok(out)
}

/// Mapped points of a grid of normalised positions, to start the inverse
/// map from the nearest one.
fn seed_table(envelope: &Envelope) -> Vec<(Point, (f64, f64))> {
    let n = SEED_GRID;
    let mut out = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
            out.push((envelope.map_uv(u, v), (u, v)));
        }
    }
    out
}

fn nearest_seed(seeds: &[(Point, (f64, f64))], q: Point) -> (f64, f64) {
    seeds
        .iter()
        .min_by(|a, b| a.0.distance_squared(q).total_cmp(&b.0.distance_squared(q)))
        .map_or((0.5, 0.5), |s| s.1)
}

/// X positions where the closed polygon crosses the line at `y`, sorted,
/// so consecutive pairs bound the inside (even odd rule).
fn row_crossings(polygon: &[Point], y: f64, out: &mut Vec<f64>) {
    out.clear();
    let n = polygon.len();
    for k in 0..n {
        let (a, b) = (polygon[k], polygon[(k + 1) % n]);
        if (a.y <= y) != (b.y <= y) {
            out.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
        }
    }
    out.sort_by(f64::total_cmp);
    if out.len() % 2 == 1 {
        out.pop();
    }
}

/// The source height at `(u, v)` in cell units: bilinear when the four
/// cells around it hold material, else the nearest cell (NaN when empty or
/// more than half a cell outside).
fn sample(field: &Field, u: f64, v: f64) -> f32 {
    let (i, j) = (u.floor(), v.floor());
    let (fx, fy) = (u - i, v - j);
    let (i, j) = (i as i64, j as i64);
    let corners = [
        field.get(i, j),
        field.get(i + 1, j),
        field.get(i, j + 1),
        field.get(i + 1, j + 1),
    ];
    if corners.iter().all(|z| !z.is_nan()) {
        let z = f64::from(corners[0]) * (1.0 - fx) * (1.0 - fy)
            + f64::from(corners[1]) * fx * (1.0 - fy)
            + f64::from(corners[2]) * (1.0 - fx) * fy
            + f64::from(corners[3]) * fx * fy;
        return z as f32;
    }
    let last_i = field.width as f64 - 1.0;
    let last_j = field.height as f64 - 1.0;
    if u < -0.5 || v < -0.5 || u > last_i + 0.5 || v > last_j + 0.5 {
        return f32::NAN;
    }
    field.get(
        u.round().clamp(0.0, last_i) as i64,
        v.round().clamp(0.0, last_j) as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::BBox;

    /// A dome of radius 10 at (50, 50) on a 0.5 mm grid.
    fn dome() -> Field {
        let cell = 0.5;
        let origin = Point::new(40.25, 40.25);
        let mut field = Field::empty(40, 40, cell, origin);
        for j in 0..40 {
            for i in 0..40 {
                let p = field.centre(i, j);
                let r = p.distance(Point::new(50.0, 50.0));
                if r < 10.0 {
                    field.z[j * 40 + i] = (10.0 - r) as f32;
                }
            }
        }
        field
    }

    fn box_of(field: &Field) -> BBox {
        BBox::new(
            Point::new(
                field.origin.x - field.cell / 2.0,
                field.origin.y - field.cell / 2.0,
            ),
            Point::new(
                field.origin.x + (field.width as f64 - 0.5) * field.cell,
                field.origin.y + (field.height as f64 - 0.5) * field.cell,
            ),
        )
    }

    #[test]
    fn the_identity_envelope_keeps_the_heights() {
        let field = dome();
        let envelope = Envelope::identity(box_of(&field), 4);
        let out = distort_field(&field, &envelope)
            .expect("distort")
            .cropped()
            .expect("material");
        for j in 0..field.height {
            for i in 0..field.width {
                let p = field.centre(i, j);
                let (a, b) = (field.get(i as i64, j as i64), out.at(p));
                assert!(
                    (a.is_nan() && b.is_nan()) || (a - b).abs() < 1e-4,
                    "cell {i},{j}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn stretching_the_top_widens_the_footprint() {
        let field = dome();
        let b = box_of(&field);
        let envelope = Envelope::new(
            b,
            vec![b.min, Point::new(b.max.x, b.min.y)],
            vec![
                Point::new(b.min.x - 10.0, b.max.y + 10.0),
                Point::new(b.max.x + 10.0, b.max.y + 10.0),
            ],
            None,
            None,
        )
        .expect("envelope");
        let out = distort_field(&field, &envelope)
            .expect("distort")
            .cropped()
            .expect("material");
        let wide = out.width as f64 * out.cell;
        assert!(wide > 25.0, "footprint {wide} mm across");
        assert!((out.max().unwrap_or(0.0) - 10.0).abs() < 0.5);
        // The peak moves where the envelope sends the dome's centre.
        let peak = envelope.map(Point::new(50.0, 50.0));
        assert!(out.at(peak) > 9.0);
    }

    #[test]
    fn crossings_pair_up_inside_a_square() {
        let square = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
        ];
        let mut xs = Vec::new();
        row_crossings(&square, 2.0, &mut xs);
        assert_eq!(xs, vec![0.0, 4.0]);
        row_crossings(&square, 5.0, &mut xs);
        assert!(xs.is_empty());
    }

    #[test]
    fn an_empty_field_is_refused() {
        let field = Field::empty(0, 0, 1.0, Point::ORIGIN);
        let b = BBox::new(Point::ORIGIN, Point::new(1.0, 1.0));
        assert!(distort_field(&field, &Envelope::identity(b, 2)).is_err());
    }
}
