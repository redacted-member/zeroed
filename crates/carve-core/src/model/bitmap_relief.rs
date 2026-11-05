//! Relief components from greyscale images: grey levels become heights.
//!
//! The image is laid on the job at a placement (bottom left corner and
//! millimetres per pixel). Each cell of the model grid whose centre falls on
//! the image takes the image's luma there, bilinearly between pixel
//! centres, scaled so black is the base height and white is the base plus
//! the Z range (or the other way round when inverted). Transparent pixels
//! leave cells empty. Smoothing blurs the grey levels first, which takes
//! the stair steps out of an 8 bit image.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{Mat3, Point};
use crate::io::bitmap::Bitmap;

use super::field::Field;
use super::shape::Grid;
use super::transform::MAX_TRANSFORM_CELLS;

/// Most smoothing passes a relief may ask for.
pub const MAX_SMOOTHING: u32 = 20;

/// Alpha below this counts as transparent.
const OPAQUE: u8 = 128;

/// Settings for [`relief_from_bitmap`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BitmapReliefParams {
    /// Height difference between black and white, in millimetres.
    pub height: f64,
    /// Dark is high instead of light.
    pub invert: bool,
    /// Blur passes over the grey levels before sampling, each a 3 by 3 box.
    pub smoothing: u32,
    /// Height of the lowest grey level, in millimetres.
    pub base_height: f64,
}

impl Default for BitmapReliefParams {
    fn default() -> Self {
        BitmapReliefParams {
            height: 5.0,
            invert: false,
            smoothing: 1,
            base_height: 0.0,
        }
    }
}

/// Where an image lies in the job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Job position of the image's bottom left corner, in millimetres.
    pub origin: Point,
    /// Size of one pixel, in millimetres.
    pub pixel_size: f64,
}

/// Builds the heightfield of a greyscale image on the lattice of `grid`,
/// covering the cells whose centres fall on the image.
///
/// # Errors
/// Returns [`CoreError::Validation`] for settings that are not finite, a
/// negative height or base, too much smoothing, an image with no pixels,
/// a pixel size that is not above zero, an image that misses every cell
/// and a result larger than [`MAX_TRANSFORM_CELLS`].
pub fn relief_from_bitmap(
    bitmap: &Bitmap,
    placement: Placement,
    grid: &Grid,
    params: &BitmapReliefParams,
) -> Result<Field, CoreError> {
    validate(bitmap, placement, params)?;
    let (w, h) = (bitmap.width as usize, bitmap.height as usize);
    let size = placement.pixel_size;
    let cell = grid.cell;
    let (x0, y0) = (placement.origin.x, placement.origin.y);
    let (x1, y1) = (x0 + w as f64 * size, y0 + h as f64 * size);
    let i0 = ((x0 - grid.origin.x) / cell).ceil();
    let i1 = ((x1 - grid.origin.x) / cell).floor();
    let j0 = ((y0 - grid.origin.y) / cell).ceil();
    let j1 = ((y1 - grid.origin.y) / cell).floor();
    if i1 < i0 || j1 < j0 {
        return Err(CoreError::validation(
            "the image is smaller than one cell of the model",
        ));
    }
    let (width, height) = (i1 - i0 + 1.0, j1 - j0 + 1.0);
    if width * height > MAX_TRANSFORM_CELLS as f64 {
        return Err(CoreError::validation(
            "the image would make too large a component",
        ));
    }
    let (width, height) = (width as usize, height as usize);
    let origin = Point::new(grid.origin.x + i0 * cell, grid.origin.y + j0 * cell);

    let grey = smoothed(bitmap, params.smoothing);
    let mut out = Field::empty(width, height, cell, origin);
    let scale = params.height / 255.0;
    for (j, row) in out.z.chunks_exact_mut(width).enumerate() {
        let y = origin.y + j as f64 * cell;
        // Pixel rows run from the top of the picture.
        let fy = ((y1 - y) / size - 0.5).clamp(0.0, h as f64 - 1.0);
        for (i, slot) in row.iter_mut().enumerate() {
            let x = origin.x + i as f64 * cell;
            let fx = ((x - x0) / size - 0.5).clamp(0.0, w as f64 - 1.0);
            if let Some(alpha) = &bitmap.alpha {
                let nearest = fy.round() as usize * w + fx.round() as usize;
                if alpha[nearest] < OPAQUE {
                    continue;
                }
            }
            let level = bilinear(&grey, w, h, fx, fy);
            let level = if params.invert { 255.0 - level } else { level };
            *slot = (params.base_height + level * scale) as f32;
        }
    }
    Ok(out)
}

/// As [`relief_from_bitmap`] for an image laid on the job by any affine
/// map `to_job` from image space (pixels, origin at the bottom left, Y up)
/// to job millimetres, as a placed bitmap that was stretched or rotated is.
/// A square, upright placement gives exactly what [`relief_from_bitmap`]
/// gives.
///
/// # Errors
/// As [`relief_from_bitmap`], and [`CoreError::Validation`] when `to_job`
/// cannot be inverted.
pub fn relief_from_placed(
    bitmap: &Bitmap,
    to_job: Mat3,
    grid: &Grid,
    params: &BitmapReliefParams,
) -> Result<Field, CoreError> {
    let upright = to_job.b.abs() < 1e-12 && to_job.c.abs() < 1e-12 && to_job.a > 0.0;
    if upright && (to_job.a - to_job.d).abs() <= 1e-9 * to_job.a {
        let placement = Placement {
            origin: Point::new(to_job.e, to_job.f),
            pixel_size: to_job.a,
        };
        return relief_from_bitmap(bitmap, placement, grid, params);
    }
    let unit = Placement {
        origin: Point::new(0.0, 0.0),
        pixel_size: 1.0,
    };
    validate(bitmap, unit, params)?;
    let to_image = to_job.invert()?;
    let (w, h) = (f64::from(bitmap.width), f64::from(bitmap.height));
    let corners =
        [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| to_job.apply(Point::new(x, y)));
    let fold = |pick: fn(&Point) -> f64, f: fn(f64, f64) -> f64, start: f64| {
        corners.iter().map(pick).fold(start, f)
    };
    let (lo_x, hi_x) = (
        fold(|p| p.x, f64::min, f64::MAX),
        fold(|p| p.x, f64::max, f64::MIN),
    );
    let (lo_y, hi_y) = (
        fold(|p| p.y, f64::min, f64::MAX),
        fold(|p| p.y, f64::max, f64::MIN),
    );
    let cell = grid.cell;
    let i0 = ((lo_x - grid.origin.x) / cell).ceil();
    let i1 = ((hi_x - grid.origin.x) / cell).floor();
    let j0 = ((lo_y - grid.origin.y) / cell).ceil();
    let j1 = ((hi_y - grid.origin.y) / cell).floor();
    if !(i0.is_finite() && j0.is_finite() && i1.is_finite() && j1.is_finite()) || i1 < i0 || j1 < j0
    {
        return Err(CoreError::validation(
            "the image is smaller than one cell of the model",
        ));
    }
    let (width, height) = (i1 - i0 + 1.0, j1 - j0 + 1.0);
    if width * height > MAX_TRANSFORM_CELLS as f64 {
        return Err(CoreError::validation(
            "the image would make too large a component",
        ));
    }
    let (width, height) = (width as usize, height as usize);
    let origin = Point::new(grid.origin.x + i0 * cell, grid.origin.y + j0 * cell);
    let (wi, hi) = (bitmap.width as usize, bitmap.height as usize);
    let grey = smoothed(bitmap, params.smoothing);
    let mut out = Field::empty(width, height, cell, origin);
    let scale = params.height / 255.0;
    let edge = 1e-9;
    for (j, row) in out.z.chunks_exact_mut(width).enumerate() {
        let y = origin.y + j as f64 * cell;
        for (i, slot) in row.iter_mut().enumerate() {
            let p = to_image.apply(Point::new(origin.x + i as f64 * cell, y));
            if p.x < -edge || p.y < -edge || p.x > w + edge || p.y > h + edge {
                continue;
            }
            let fx = (p.x - 0.5).clamp(0.0, w - 1.0);
            // Pixel rows run from the top of the picture.
            let fy = (h - p.y - 0.5).clamp(0.0, h - 1.0);
            if let Some(alpha) = &bitmap.alpha {
                let nearest = fy.round() as usize * wi + fx.round() as usize;
                if alpha[nearest] < OPAQUE {
                    continue;
                }
            }
            let level = bilinear(&grey, wi, hi, fx, fy);
            let level = if params.invert { 255.0 - level } else { level };
            *slot = (params.base_height + level * scale) as f32;
        }
    }
    Ok(out)
}

fn validate(
    bitmap: &Bitmap,
    placement: Placement,
    params: &BitmapReliefParams,
) -> Result<(), CoreError> {
    if bitmap.width == 0 || bitmap.height == 0 || bitmap.check().is_err() {
        return Err(CoreError::validation("the image has no pixels"));
    }
    if !(placement.origin.is_finite()
        && placement.pixel_size.is_finite()
        && placement.pixel_size > 0.0)
    {
        return Err(CoreError::validation(
            "the image needs a position and a pixel size above zero",
        ));
    }
    if !(params.height.is_finite() && params.height >= 0.0) {
        return Err(CoreError::validation(
            "the height must be zero or more millimeters",
        ));
    }
    if !(params.base_height.is_finite() && params.base_height >= 0.0) {
        return Err(CoreError::validation(
            "the base height must be zero or more millimeters",
        ));
    }
    if params.smoothing > MAX_SMOOTHING {
        return Err(CoreError::validation(format!(
            "smoothing goes up to {MAX_SMOOTHING}"
        )));
    }
    Ok(())
}

/// Grey levels as numbers, blurred by `passes` 3 by 3 box passes (edges
/// average the pixels they have).
fn smoothed(bitmap: &Bitmap, passes: u32) -> Vec<f32> {
    let (w, h) = (bitmap.width as usize, bitmap.height as usize);
    let mut grey: Vec<f32> = bitmap.luma.iter().map(|&v| f32::from(v)).collect();
    let mut next = vec![0.0f32; grey.len()];
    for _ in 0..passes {
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        sum += grey[yy * w + xx];
                        count += 1.0;
                    }
                }
                next[y * w + x] = sum / count;
            }
        }
        std::mem::swap(&mut grey, &mut next);
    }
    grey
}

fn bilinear(grey: &[f32], w: usize, h: usize, fx: f64, fy: f64) -> f64 {
    let (px, py) = (fx.floor() as usize, fy.floor() as usize);
    let (tx, ty) = (fx - px as f64, fy - py as f64);
    let (qx, qy) = ((px + 1).min(w - 1), (py + 1).min(h - 1));
    let at = |x: usize, y: usize| f64::from(grey[y * w + x]);
    at(px, py) * (1.0 - tx) * (1.0 - ty)
        + at(qx, py) * tx * (1.0 - ty)
        + at(px, qy) * (1.0 - tx) * ty
        + at(qx, qy) * tx * ty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            width: 200,
            height: 200,
            cell: 0.5,
            origin: Point::new(0.25, 0.25),
        }
    }

    /// Left half black, right half white, 10 by 4 pixels.
    fn halves(alpha: Option<Vec<u8>>) -> Bitmap {
        let mut luma = Vec::new();
        for _ in 0..4 {
            luma.extend([0u8; 5]);
            luma.extend([255u8; 5]);
        }
        Bitmap::new(10, 4, luma, alpha).expect("bitmap")
    }

    fn placement() -> Placement {
        Placement {
            origin: Point::new(10.0, 10.0),
            pixel_size: 2.0,
        }
    }

    #[test]
    fn light_is_high_and_the_field_covers_the_image() {
        let params = BitmapReliefParams {
            smoothing: 0,
            base_height: 1.0,
            ..BitmapReliefParams::default()
        };
        let field =
            relief_from_bitmap(&halves(None), placement(), &grid(), &params).expect("relief");
        assert_eq!((field.width, field.height), (40, 16));
        assert!((field.origin.x - 10.25).abs() < 1e-12);
        assert!((field.at(Point::new(12.0, 14.0)) - 1.0).abs() < 1e-6);
        assert!((field.at(Point::new(28.0, 14.0)) - 6.0).abs() < 1e-6);
    }

    #[test]
    fn invert_makes_dark_high() {
        let params = BitmapReliefParams {
            invert: true,
            smoothing: 0,
            ..BitmapReliefParams::default()
        };
        let field =
            relief_from_bitmap(&halves(None), placement(), &grid(), &params).expect("relief");
        assert!((field.at(Point::new(12.0, 14.0)) - 5.0).abs() < 1e-6);
        assert!(field.at(Point::new(28.0, 14.0)).abs() < 1e-6);
    }

    #[test]
    fn smoothing_softens_the_step() {
        let sharp = BitmapReliefParams {
            smoothing: 0,
            ..BitmapReliefParams::default()
        };
        let soft = BitmapReliefParams {
            smoothing: 2,
            ..BitmapReliefParams::default()
        };
        let a = relief_from_bitmap(&halves(None), placement(), &grid(), &sharp).expect("a");
        let b = relief_from_bitmap(&halves(None), placement(), &grid(), &soft).expect("b");
        // Just left of the step: sharp is still black, smoothed has risen.
        let p = Point::new(18.5, 14.0);
        assert!(a.at(p) < 0.01);
        assert!(b.at(p) > 0.5 && b.at(p) < 4.5);
    }

    #[test]
    fn transparent_pixels_leave_holes() {
        let mut alpha = vec![255u8; 40];
        alpha[0] = 0;
        let params = BitmapReliefParams::default();
        let field = relief_from_bitmap(&halves(Some(alpha)), placement(), &grid(), &params)
            .expect("relief");
        // Pixel 0 is the top left one.
        assert!(field.at(Point::new(10.75, 17.75)).is_nan());
        assert!(!field.at(Point::new(14.0, 17.75)).is_nan());
    }

    #[test]
    fn bad_settings_are_refused() {
        let bitmap = halves(None);
        let bad = [
            BitmapReliefParams {
                height: -1.0,
                ..BitmapReliefParams::default()
            },
            BitmapReliefParams {
                base_height: f64::NAN,
                ..BitmapReliefParams::default()
            },
            BitmapReliefParams {
                smoothing: MAX_SMOOTHING + 1,
                ..BitmapReliefParams::default()
            },
        ];
        for params in bad {
            assert!(relief_from_bitmap(&bitmap, placement(), &grid(), &params).is_err());
        }
        let tiny = Placement {
            origin: Point::new(10.1, 10.1),
            pixel_size: 0.001,
        };
        assert!(
            relief_from_bitmap(&bitmap, tiny, &grid(), &BitmapReliefParams::default()).is_err()
        );
    }

    #[test]
    fn a_placed_bitmap_can_be_stretched_or_turned() {
        let params = BitmapReliefParams {
            smoothing: 0,
            ..BitmapReliefParams::default()
        };
        // Upright and square: the same as a placement.
        let square = Mat3 {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            e: 10.0,
            f: 10.0,
        };
        let placed = relief_from_placed(&halves(None), square, &grid(), &params).expect("square");
        let plain =
            relief_from_bitmap(&halves(None), placement(), &grid(), &params).expect("plain");
        assert_eq!(placed, plain);
        // Turned a quarter anticlockwise: the image's X runs up the job.
        let turned = Mat3 {
            a: 0.0,
            b: 2.0,
            c: -2.0,
            d: 0.0,
            e: 30.0,
            f: 10.0,
        };
        let field = relief_from_placed(&halves(None), turned, &grid(), &params).expect("turned");
        assert!(field.at(Point::new(26.0, 14.0)).abs() < 1e-6, "black low");
        assert!(
            (field.at(Point::new(26.0, 28.0)) - 5.0).abs() < 1e-6,
            "white high"
        );
        assert!((field.origin.x - 22.25).abs() < 1e-9);
        // Stretched wide: twice as wide as high per pixel.
        let wide = Mat3 {
            a: 4.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            e: 0.0,
            f: 0.0,
        };
        let field = relief_from_placed(&halves(None), wide, &grid(), &params).expect("wide");
        assert!((field.at(Point::new(30.0, 4.0)) - 5.0).abs() < 1e-6);
        assert!(field.at(Point::new(10.0, 4.0)).abs() < 1e-6);
    }
}
