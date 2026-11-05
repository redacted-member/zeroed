//! Moving, scaling and rotating a relief component in XY.
//!
//! The result stays on the component's own grid: the same cell size, with
//! cell centres on the same lattice (the origin moves by whole cells), so
//! a transformed component bakes exactly like one made in place. Each new
//! cell is filled by mapping its centre back into the source and sampling
//! there. Heights are never scaled; only the footprint changes.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;

use super::field::Field;

/// Most cells a transformed component may have, so a large scale cannot
/// ask for more memory than a machine has.
pub const MAX_TRANSFORM_CELLS: usize = 64_000_000;

/// Fractions of a cell closer than this to a whole cell count as whole, so
/// moves by whole cells and quarter turns sample exactly.
const SNAP: f64 = 1e-9;

/// Settings for [`transform`]. The default changes nothing.
///
/// A source point `p` goes to `centre + rotate(scale(p - centre)) + offset`:
/// scaled about the centre first, then turned about it, then moved.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TransformParams {
    /// Move in millimetres, applied last.
    pub offset: Point,
    /// Stretch along X (unitless, more than zero).
    pub scale_x: f64,
    /// Stretch along Y (unitless, more than zero).
    pub scale_y: f64,
    /// Turn in degrees, counter clockwise.
    pub rotate_deg: f64,
    /// Job position (mm) scaling and turning happen about; the centre of
    /// the component's box when absent.
    pub centre: Option<Point>,
}

impl TransformParams {
    /// The same change as an affine matrix of job space, for `field`
    /// (whose box middle is the centre when none is given).
    pub fn matrix(&self, field: &Field) -> crate::geom::Mat3 {
        use crate::geom::{Mat3, Vec2};
        let centre = self.centre.unwrap_or_else(|| {
            Point::new(
                field.origin.x + (field.width as f64 - 1.0) * field.cell / 2.0,
                field.origin.y + (field.height as f64 - 1.0) * field.cell / 2.0,
            )
        });
        Mat3::scale_about(self.scale_x, self.scale_y, centre)
            .then(&Mat3::rotate_about(self.rotate_deg.to_radians(), centre))
            .then(&Mat3::translate(Vec2::new(self.offset.x, self.offset.y)))
    }
}

impl Default for TransformParams {
    fn default() -> Self {
        TransformParams {
            offset: Point::ORIGIN,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_deg: 0.0,
            centre: None,
        }
    }
}

/// Moves, scales and turns a component's heights in XY.
///
/// The result shares `field`'s cell size and grid lattice and covers the
/// transformed box of the source cells: every cell whose centre falls
/// inside it (half open, so a box whose edges fall on cell centres, as a
/// quarter turn of an odd by even field does, keeps its length in cells).
/// Each cell samples the source at its centre mapped back:
/// bilinearly between the four nearest source cells when all of them hold
/// material, and from the nearest source cell when any of them is empty.
/// The nearest fallback keeps edges where they belong instead of eating a
/// cell of material away all round, and never spreads material into empty
/// cells beyond half a source cell. A sample landing exactly on a source
/// cell centre takes that cell's height, so the identity, moves by whole
/// cells and quarter turns about a cell centre or corner are exact.
///
/// # Errors
/// Returns [`CoreError::Validation`] for values that are not finite, scales
/// that are not above zero, a field with no cells or a bad cell size, and a
/// result larger than [`MAX_TRANSFORM_CELLS`].
pub fn transform(field: &Field, params: &TransformParams) -> Result<Field, CoreError> {
    validate(field, params)?;
    let cell = field.cell;
    let (w, h) = (field.width as f64, field.height as f64);
    let centre = params.centre.unwrap_or_else(|| {
        Point::new(
            field.origin.x + (w - 1.0) * cell / 2.0,
            field.origin.y + (h - 1.0) * cell / 2.0,
        )
    });
    let (sin, cos) = params.rotate_deg.to_radians().sin_cos();
    let (sx, sy) = (params.scale_x, params.scale_y);
    let forward = |p: Point| {
        let (x, y) = ((p.x - centre.x) * sx, (p.y - centre.y) * sy);
        Point::new(
            centre.x + x * cos - y * sin + params.offset.x,
            centre.y + x * sin + y * cos + params.offset.y,
        )
    };

    // The transformed box of the source cells' outer edges.
    let low = Point::new(field.origin.x - cell / 2.0, field.origin.y - cell / 2.0);
    let high = Point::new(low.x + w * cell, low.y + h * cell);
    let corners = [
        forward(low),
        forward(Point::new(high.x, low.y)),
        forward(high),
        forward(Point::new(low.x, high.y)),
    ];
    let (mut min, mut max) = (corners[0], corners[0]);
    for c in &corners[1..] {
        min = Point::new(min.x.min(c.x), min.y.min(c.y));
        max = Point::new(max.x.max(c.x), max.y.max(c.y));
    }
    // Lattice cells whose centres are inside the box, in source indices.
    // The box is half open, so a box whose edges fall on cell centres gets
    // as many cells as it is long rather than one more.
    let i0 = ((min.x - field.origin.x) / cell - SNAP).ceil();
    let i1 = ((max.x - field.origin.x) / cell - SNAP).ceil() - 1.0;
    let j0 = ((min.y - field.origin.y) / cell - SNAP).ceil();
    let j1 = ((max.y - field.origin.y) / cell - SNAP).ceil() - 1.0;
    let width = (i1 - i0 + 1.0).max(1.0);
    let height = (j1 - j0 + 1.0).max(1.0);
    if width * height > MAX_TRANSFORM_CELLS as f64 {
        return Err(CoreError::validation(
            "the transformed component would be too large",
        ));
    }
    let (width, height) = (width as usize, height as usize);
    let origin = Point::new(field.origin.x + i0 * cell, field.origin.y + j0 * cell);
    let mut out = Field::empty(width, height, cell, origin);

    // Inverse map, straight to source cell indices: undo the move, turn
    // back, unscale.
    for (j, row) in out.z.chunks_exact_mut(width).enumerate() {
        let qy = origin.y + j as f64 * cell - params.offset.y - centre.y;
        for (i, slot) in row.iter_mut().enumerate() {
            let qx = origin.x + i as f64 * cell - params.offset.x - centre.x;
            let x = (qx * cos + qy * sin) / sx + centre.x;
            let y = (-qx * sin + qy * cos) / sy + centre.y;
            *slot = sample(
                field,
                (x - field.origin.x) / cell,
                (y - field.origin.y) / cell,
            );
        }
    }
    Ok(out)
}

/// Maps a component's heights through any affine transform of job space
/// (moves, scales, turns, mirrors and skews alike), sampling as
/// [`transform`] does. Heights are not changed.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a transform that is not made of
/// numbers or flattens everything to a line, and a result larger than
/// [`MAX_TRANSFORM_CELLS`].
pub fn transform_affine(field: &Field, m: &crate::geom::Mat3) -> Result<Field, CoreError> {
    let entries = [m.a, m.b, m.c, m.d, m.e, m.f];
    if entries.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation("the transform must be numbers"));
    }
    let inverse = m
        .invert()
        .map_err(|_| CoreError::validation("the transform flattens the component"))?;
    let cell = field.cell;
    let (w, h) = (field.width as f64, field.height as f64);
    let low = Point::new(field.origin.x - cell / 2.0, field.origin.y - cell / 2.0);
    let high = Point::new(low.x + w * cell, low.y + h * cell);
    let corners = [
        m.apply(low),
        m.apply(Point::new(high.x, low.y)),
        m.apply(high),
        m.apply(Point::new(low.x, high.y)),
    ];
    let (mut min, mut max) = (corners[0], corners[0]);
    for c in &corners[1..] {
        min = Point::new(min.x.min(c.x), min.y.min(c.y));
        max = Point::new(max.x.max(c.x), max.y.max(c.y));
    }
    let i0 = ((min.x - field.origin.x) / cell - SNAP).ceil();
    let i1 = ((max.x - field.origin.x) / cell - SNAP).ceil() - 1.0;
    let j0 = ((min.y - field.origin.y) / cell - SNAP).ceil();
    let j1 = ((max.y - field.origin.y) / cell - SNAP).ceil() - 1.0;
    let width = (i1 - i0 + 1.0).max(1.0);
    let height = (j1 - j0 + 1.0).max(1.0);
    if width * height > MAX_TRANSFORM_CELLS as f64 {
        return Err(CoreError::validation(
            "the transformed component would be too large",
        ));
    }
    let (width, height) = (width as usize, height as usize);
    let origin = Point::new(field.origin.x + i0 * cell, field.origin.y + j0 * cell);
    let mut out = Field::empty(width, height, cell, origin);
    for (j, row) in out.z.chunks_exact_mut(width).enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            let p = inverse.apply(Point::new(
                origin.x + i as f64 * cell,
                origin.y + j as f64 * cell,
            ));
            *slot = sample(
                field,
                (p.x - field.origin.x) / cell,
                (p.y - field.origin.y) / cell,
            );
        }
    }
    Ok(out)
}

/// Rejects settings and fields the transform cannot use.
fn validate(field: &Field, params: &TransformParams) -> Result<(), CoreError> {
    if !(params.offset.is_finite()
        && params.rotate_deg.is_finite()
        && params.centre.is_none_or(Point::is_finite))
    {
        return Err(CoreError::validation(
            "the move, turn and center must be numbers",
        ));
    }
    let good_scale = |s: f64| s.is_finite() && s > 0.0;
    if !(good_scale(params.scale_x) && good_scale(params.scale_y)) {
        return Err(CoreError::validation("the scale must be more than zero"));
    }
    if field.width == 0
        || field.height == 0
        || field.z.len() != field.width * field.height
        || !(field.cell.is_finite() && field.cell > 0.0)
        || !field.origin.is_finite()
    {
        return Err(CoreError::validation(
            "the component has no heights to transform",
        ));
    }
    Ok(())
}

/// Splits a position in cell units into a whole cell and a fraction,
/// snapping fractions within [`SNAP`] of a whole cell onto it.
fn split(u: f64) -> (i64, f64) {
    let whole = u.floor();
    let frac = u - whole;
    if frac < SNAP {
        (whole as i64, 0.0)
    } else if frac > 1.0 - SNAP {
        (whole as i64 + 1, 0.0)
    } else {
        (whole as i64, frac)
    }
}

/// The height of the source cell nearest `(u, v)` in cell units. A point
/// on the outer edge of the source, half a cell out from the last centre,
/// takes the edge cell's height; anything further out is empty.
fn nearest(field: &Field, u: f64, v: f64) -> f32 {
    let pick = |x: f64, n: usize| {
        let last = n as f64 - 1.0;
        (x >= -0.5 - SNAP && x <= last + 0.5 + SNAP).then(|| x.round().clamp(0.0, last) as i64)
    };
    match (pick(u, field.width), pick(v, field.height)) {
        (Some(i), Some(j)) => field.get(i, j),
        _ => f32::NAN,
    }
}

/// The source height at `(u, v)` in cell units: bilinear when every cell
/// with a share is full, else the nearest cell (NaN when that is empty or
/// outside).
fn sample(field: &Field, u: f64, v: f64) -> f32 {
    let (i, fx) = split(u);
    let (j, fy) = split(v);
    let mut sum = 0.0f64;
    for (di, wx) in [(0, 1.0 - fx), (1, fx)] {
        if wx == 0.0 {
            continue;
        }
        for (dj, wy) in [(0, 1.0 - fy), (1, fy)] {
            if wy == 0.0 {
                continue;
            }
            let z = field.get(i + di, j + dj);
            if z.is_nan() {
                return nearest(field, u, v);
            }
            sum += f64::from(z) * wx * wy;
        }
    }
    sum as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field whose cell `(i, j)` holds `10 i + j`.
    fn ramp(width: usize, height: usize, cell: f64, origin: Point) -> Field {
        let mut field = Field::empty(width, height, cell, origin);
        for j in 0..height {
            for i in 0..width {
                field.z[j * width + i] = (10 * i + j) as f32;
            }
        }
        field
    }

    #[test]
    fn the_default_changes_nothing() {
        let field = ramp(7, 4, 0.25, Point::new(3.125, -1.375));
        let done = transform(&field, &TransformParams::default()).expect("transform");
        assert_eq!(done, field);
    }

    #[test]
    fn a_move_by_whole_cells_is_exact() {
        let field = ramp(5, 3, 0.25, Point::new(0.125, 0.125));
        let params = TransformParams {
            offset: Point::new(1.0, -0.75),
            ..TransformParams::default()
        };
        let done = transform(&field, &params).expect("transform");
        assert_eq!((done.width, done.height), (5, 3));
        assert!((done.origin.x - 1.125).abs() < 1e-12);
        assert!((done.origin.y + 0.625).abs() < 1e-12);
        assert_eq!(done.z, field.z);
    }

    #[test]
    fn a_quarter_turn_moves_cells_round_the_centre() {
        // 3 by 5 cells: the box centre is a cell centre, cell (1, 2).
        let field = ramp(3, 5, 1.0, Point::new(0.0, 0.0));
        let params = TransformParams {
            rotate_deg: 90.0,
            ..TransformParams::default()
        };
        let done = transform(&field, &params).expect("transform");
        assert_eq!((done.width, done.height), (5, 3));
        assert_eq!(done.origin, Point::new(-1.0, 1.0));
        // Counter clockwise: the source cell at (1 + a, 2 + b) lands on the
        // job position (1 - b, 2 + a).
        for j in 0..5i64 {
            for i in 0..3i64 {
                let (a, b) = (i - 1, j - 2);
                let at = Point::new((1 - b) as f64, (2 + a) as f64);
                assert_eq!(done.at(at), field.get(i, j), "cell {i},{j}");
            }
        }
    }

    #[test]
    fn scaling_by_two_doubles_the_box() {
        let field = ramp(4, 6, 0.5, Point::new(0.25, 0.25));
        let params = TransformParams {
            scale_x: 2.0,
            scale_y: 2.0,
            ..TransformParams::default()
        };
        let done = transform(&field, &params).expect("transform");
        assert_eq!((done.width, done.height), (8, 12));
        let centre = |f: &Field| {
            Point::new(
                f.origin.x + (f.width - 1) as f64 * f.cell / 2.0,
                f.origin.y + (f.height - 1) as f64 * f.cell / 2.0,
            )
        };
        assert!(centre(&done).distance(centre(&field)) < 1e-12);
        assert!(done.z.iter().all(|z| !z.is_nan()));
        // Heights stay the same range; only the footprint grows.
        assert_eq!(done.min(), field.min());
        assert_eq!(done.max(), field.max());

        let stretched = transform(
            &field,
            &TransformParams {
                scale_x: 2.0,
                scale_y: 0.5,
                ..TransformParams::default()
            },
        )
        .expect("transform");
        assert_eq!((stretched.width, stretched.height), (8, 3));
        assert!(stretched.z.iter().all(|z| !z.is_nan()));
    }

    #[test]
    fn a_quarter_turn_of_an_odd_by_even_field_keeps_every_cell() {
        let field = ramp(4, 3, 1.0, Point::ORIGIN);
        let done = transform(
            &field,
            &TransformParams {
                rotate_deg: 90.0,
                ..TransformParams::default()
            },
        )
        .expect("transform");
        assert_eq!((done.width, done.height), (3, 4));
        assert!(done.z.iter().all(|z| !z.is_nan()), "{:?}", done.z);
    }

    #[test]
    fn empty_cells_stay_empty() {
        let mut field = Field::empty(12, 12, 1.0, Point::new(0.5, 0.5));
        for j in 0..12 {
            for i in 0..12 {
                if !((4..8).contains(&i) && (4..8).contains(&j)) {
                    field.z[j * 12 + i] = 1.0;
                }
            }
        }
        for params in [
            TransformParams {
                rotate_deg: 30.0,
                ..TransformParams::default()
            },
            TransformParams {
                offset: Point::new(2.5, -1.25),
                scale_x: 1.5,
                ..TransformParams::default()
            },
        ] {
            let done = transform(&field, &params).expect("transform");
            // The hole's middle stays empty wherever it went.
            let hole = Point::new(6.0 + params.offset.x, 6.0 + params.offset.y);
            assert!(done.at(hole).is_nan(), "{params:?}");
            // Material is never invented or blended with emptiness.
            assert!(done.z.iter().all(|z| z.is_nan() || *z == 1.0));
            let full = done.z.iter().filter(|z| !z.is_nan()).count() as f64;
            let want = (144.0 - 16.0) * params.scale_x * params.scale_y;
            assert!((full - want).abs() / want < 0.15, "{full} vs {want}");
            // Corners of the rotated box are outside the source.
            assert!(done.z[0].is_nan() || params.rotate_deg == 0.0);
        }
    }

    #[test]
    fn bad_settings_are_refused() {
        let field = ramp(3, 3, 1.0, Point::ORIGIN);
        for params in [
            TransformParams {
                scale_x: 0.0,
                ..TransformParams::default()
            },
            TransformParams {
                scale_y: -1.0,
                ..TransformParams::default()
            },
            TransformParams {
                rotate_deg: f64::NAN,
                ..TransformParams::default()
            },
            TransformParams {
                centre: Some(Point::new(f64::INFINITY, 0.0)),
                ..TransformParams::default()
            },
            TransformParams {
                scale_x: 1e9,
                ..TransformParams::default()
            },
        ] {
            assert!(transform(&field, &params).is_err(), "{params:?}");
        }
    }

    #[test]
    fn settings_parse_with_defaults() {
        let params: TransformParams =
            serde_json::from_str(r#"{"rotateDeg": 45.0, "scaleX": 2.0}"#).expect("parse");
        assert_eq!(params.rotate_deg, 45.0);
        assert_eq!(params.scale_x, 2.0);
        assert_eq!(params.scale_y, 1.0);
        assert_eq!(params.centre, None);
        let json = serde_json::to_string(&TransformParams::default()).expect("json");
        assert!(json.contains("\"scaleY\"") && json.contains("\"rotateDeg\""));
    }

    #[test]
    fn an_affine_mirror_flips_the_heights_about_its_line() {
        // Heights rising to the right, mirrored left to right about x = 10.
        let mut f = Field::empty(10, 1, 1.0, Point::new(0.5, 0.5));
        for i in 0..10 {
            f.z[i] = i as f32;
        }
        let mirror = crate::geom::Mat3::new(-1.0, 0.0, 0.0, 1.0, 20.0, 0.0);
        let out = transform_affine(&f, &mirror).expect("mirrored");
        assert_eq!(out.width, 10);
        assert_eq!(out.at(Point::new(10.5, 0.5)), 9.0);
        assert_eq!(out.at(Point::new(19.5, 0.5)), 0.0);
        let flat = crate::geom::Mat3::new(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(transform_affine(&f, &flat).is_err());
    }
}
