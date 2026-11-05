//! Greyscale heightmap export: the model as a PNG where brightness is
//! height, 16 bits per pixel for machining or rendering, or 8 bits for
//! programs that only take ordinary images.
//!
//! One pixel per model cell. The lowest of the model and the zero plane is
//! black, the highest point white, levels spread evenly between; cells with
//! no model are at the zero plane. Row 0 of the image is the top (largest
//! Y) of the model, as images are stored.

use std::io::Cursor;
use std::path::Path as FsPath;

use image::{DynamicImage, ImageBuffer, ImageFormat, Luma};
use serde::Serialize;

use crate::error::CoreError;
use crate::model::field::Field;

/// What a written heightmap covers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeightmapInfo {
    /// Pixels across.
    pub width: usize,
    /// Pixels down.
    pub height: usize,
    /// Height of black, millimetres.
    pub min_z: f64,
    /// Height of white, millimetres.
    pub max_z: f64,
    /// Size of a pixel, millimetres.
    pub pixel_size: f64,
}

/// Bits per pixel of a heightmap: 8 or 16.
fn levels(bits: u8) -> Result<u32, CoreError> {
    match bits {
        8 => Ok(255),
        16 => Ok(65_535),
        _ => Err(CoreError::validation(
            "a heightmap is 8 or 16 bits per pixel",
        )),
    }
}

/// The heightmap of `field` as PNG bytes with `bits` per pixel.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an empty field or a bit depth
/// other than 8 or 16.
pub fn png_bytes(field: &Field, bits: u8) -> Result<(Vec<u8>, HeightmapInfo), CoreError> {
    let top = levels(bits)?;
    if field.width == 0 || field.height == 0 || field.z.len() != field.width * field.height {
        return Err(CoreError::validation("there is no model to export"));
    }
    let min_z = f64::from(field.min().unwrap_or(0.0)).min(0.0);
    let max_z = f64::from(field.max().unwrap_or(0.0)).max(min_z);
    let range = max_z - min_z;
    let (w, h) = (field.width, field.height);
    let level = |i: usize, j: usize| -> u16 {
        let z = field.z[j * w + i];
        let z = if z.is_nan() { 0.0 } else { f64::from(z) };
        if range <= 0.0 {
            return 0;
        }
        (((z - min_z) / range) * f64::from(top))
            .round()
            .clamp(0.0, f64::from(top)) as u16
    };
    let (iw, ih) = (
        u32::try_from(w).map_err(|_| CoreError::validation("the model is too wide"))?,
        u32::try_from(h).map_err(|_| CoreError::validation("the model is too tall"))?,
    );
    let image = if bits == 16 {
        DynamicImage::ImageLuma16(ImageBuffer::from_fn(iw, ih, |x, y| {
            Luma([level(x as usize, h - 1 - y as usize)])
        }))
    } else {
        DynamicImage::ImageLuma8(ImageBuffer::from_fn(iw, ih, |x, y| {
            Luma([level(x as usize, h - 1 - y as usize) as u8])
        }))
    };
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| CoreError::format("PNG", error))?;
    Ok((
        bytes.into_inner(),
        HeightmapInfo {
            width: w,
            height: h,
            min_z,
            max_z,
            pixel_size: field.cell,
        },
    ))
}

/// Writes the heightmap of `field` to `path` as a PNG. See [`png_bytes`].
///
/// # Errors
/// The errors of [`png_bytes`], and [`CoreError::Io`] when the file cannot
/// be written.
pub fn write(field: &Field, bits: u8, path: &FsPath) -> Result<HeightmapInfo, CoreError> {
    let (bytes, info) = png_bytes(field, bits)?;
    std::fs::write(path, bytes).map_err(|error| CoreError::io(path.display(), error))?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn field() -> Field {
        let mut f = Field::empty(3, 2, 0.5, Point::new(0.0, 0.0));
        // Bottom row 0, 1, 2 mm; top row empty, 4 mm, 1 mm.
        f.z = vec![0.0, 1.0, 2.0, f32::NAN, 4.0, 1.0];
        f
    }

    #[test]
    fn sixteen_bit_levels_round_trip() {
        let (bytes, info) = png_bytes(&field(), 16).expect("encodes");
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!((info.min_z, info.max_z), (0.0, 4.0));
        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png).expect("png");
        let grey = image.as_luma16().expect("16 bit grey");
        // Row 0 of the image is the top row of the model.
        assert_eq!(grey.get_pixel(0, 0).0[0], 0);
        assert_eq!(grey.get_pixel(1, 0).0[0], 65_535);
        assert_eq!(grey.get_pixel(2, 0).0[0], 16_384);
        assert_eq!(grey.get_pixel(2, 1).0[0], 32_768);
        // Levels map back to heights within a level.
        let z = |v: u16| info.min_z + f64::from(v) / 65_535.0 * (info.max_z - info.min_z);
        assert!((z(grey.get_pixel(1, 1).0[0]) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn eight_bit_and_negative_heights() {
        let mut f = field();
        f.z[0] = -2.0;
        let (bytes, info) = png_bytes(&f, 8).expect("encodes");
        assert_eq!(info.min_z, -2.0);
        let image = image::load_from_memory(&bytes).expect("png");
        let grey = image.as_luma8().expect("8 bit grey");
        assert_eq!(grey.get_pixel(0, 1).0[0], 0);
        assert_eq!(grey.get_pixel(1, 0).0[0], 255);
        // An empty cell sits at the zero plane, a third of the way up.
        assert_eq!(grey.get_pixel(0, 0).0[0], 85);
    }

    #[test]
    fn bad_requests_are_refused() {
        assert!(png_bytes(&field(), 12).is_err());
        let empty = Field::empty(0, 0, 1.0, Point::new(0.0, 0.0));
        assert!(png_bytes(&empty, 16).is_err());
        let flat = Field::empty(2, 2, 1.0, Point::new(0.0, 0.0));
        let (bytes, _) = png_bytes(&flat, 8).expect("flat is fine");
        let image = image::load_from_memory(&bytes).expect("png");
        assert!(image
            .as_luma8()
            .expect("grey")
            .pixels()
            .all(|p| p.0[0] == 0));
    }
}
