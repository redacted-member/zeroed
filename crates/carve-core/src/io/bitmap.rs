//! Raster images read in for tracing: PNG, JPEG, BMP, WebP, GIF and TIFF decoded to a
//! greyscale bitmap with an optional alpha channel.
//!
//! Colour is reduced to luma with the Rec. 709 weights applied to the stored
//! (gamma encoded) sRGB values, `Y = 0.2126 R + 0.7152 G + 0.0722 B`, rounded
//! to the nearest 8 bit level. Images with more than 8 bits per channel are
//! reduced to 8 bits first. Rows are stored top first, as the file stores
//! them, so row 0 is the top of the picture.

use std::path::Path as FsPath;

use image::{DynamicImage, ImageFormat};

use crate::error::CoreError;

/// A greyscale raster image, 8 bits per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Luma per pixel, 0 black to 255 white, row by row from the top left.
    /// Holds exactly `width * height` values.
    pub luma: Vec<u8>,
    /// Opacity per pixel, 0 fully transparent to 255 opaque, in the same
    /// order as `luma`. `None` when the image has no alpha channel, which
    /// means every pixel is opaque.
    pub alpha: Option<Vec<u8>>,
}

impl Bitmap {
    /// Builds a bitmap from its parts, checking that the channel lengths
    /// match the size.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when a channel does not hold
    /// `width * height` values.
    pub fn new(
        width: u32,
        height: u32,
        luma: Vec<u8>,
        alpha: Option<Vec<u8>>,
    ) -> Result<Bitmap, CoreError> {
        let bitmap = Bitmap {
            width,
            height,
            luma,
            alpha,
        };
        bitmap.check()?;
        Ok(bitmap)
    }

    /// Number of pixels, `width * height`.
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Checks that the channel lengths match the size.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when they do not.
    pub fn check(&self) -> Result<(), CoreError> {
        let count = self.pixel_count();
        if self.luma.len() != count {
            return Err(CoreError::validation(format!(
                "a {}x{} bitmap needs {count} luma values, got {}",
                self.width,
                self.height,
                self.luma.len()
            )));
        }
        if let Some(alpha) = &self.alpha {
            if alpha.len() != count {
                return Err(CoreError::validation(format!(
                    "a {}x{} bitmap needs {count} alpha values, got {}",
                    self.width,
                    self.height,
                    alpha.len()
                )));
            }
        }
        Ok(())
    }

    /// The whole number of source pixels along each side that
    /// [`Bitmap::downsample`] merges into one, so the result holds at most
    /// `max_pixels` pixels. 1 when the image is already small enough.
    /// A `max_pixels` of 0 is treated as 1.
    pub fn downsample_factor(&self, max_pixels: u64) -> u32 {
        let max = max_pixels.max(1);
        let (w, h) = (u64::from(self.width), u64::from(self.height));
        let mut k: u64 = 1;
        while w.div_ceil(k) * h.div_ceil(k) > max {
            k += 1;
        }
        u32::try_from(k).unwrap_or(u32::MAX)
    }

    /// A smaller copy holding at most `max_pixels` pixels, for images too big
    /// to trace at full resolution.
    ///
    /// Each output pixel is the mean of a square block of
    /// [`Bitmap::downsample_factor`] source pixels on a side (blocks cut off
    /// by the right and bottom edges average the pixels they have), so the
    /// pixel size in millimetres grows by that factor. Luma is weighted by
    /// alpha, so transparent pixels do not darken or lighten their block.
    /// Returns a clone when the image is already small enough.
    pub fn downsample(&self, max_pixels: u64) -> Bitmap {
        let k = self.downsample_factor(max_pixels);
        if k <= 1 || self.check().is_err() {
            return self.clone();
        }
        let k = k as usize;
        let (w, h) = (self.width as usize, self.height as usize);
        let (nw, nh) = (w.div_ceil(k), h.div_ceil(k));
        let mut luma = Vec::with_capacity(nw * nh);
        let mut alpha = self.alpha.as_ref().map(|_| Vec::with_capacity(nw * nh));
        for by in 0..nh {
            let y_end = ((by + 1) * k).min(h);
            for bx in 0..nw {
                let x_end = ((bx + 1) * k).min(w);
                let mut luma_sum: u64 = 0;
                let mut weight_sum: u64 = 0;
                let mut alpha_sum: u64 = 0;
                let mut count: u64 = 0;
                for y in by * k..y_end {
                    let row = y * w;
                    for x in bx * k..x_end {
                        let at = row + x;
                        let a = self.alpha.as_ref().map_or(255, |a| u64::from(a[at]));
                        luma_sum += u64::from(self.luma[at]) * a;
                        weight_sum += a;
                        alpha_sum += a;
                        count += 1;
                    }
                }
                let mean_luma = (luma_sum + weight_sum / 2)
                    .checked_div(weight_sum)
                    .unwrap_or(255);
                luma.push(mean_luma.min(255) as u8);
                if let Some(out) = alpha.as_mut() {
                    out.push(((alpha_sum + count / 2) / count.max(1)).min(255) as u8);
                }
            }
        }
        Bitmap {
            width: nw as u32,
            height: nh as u32,
            luma,
            alpha,
        }
    }
}

/// Reads and decodes an image file, choosing the decoder by the file's
/// extension and falling back to its contents.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be read, and the errors of
/// [`decode`] when it cannot be decoded.
pub fn read(path: &FsPath) -> Result<Bitmap, CoreError> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path.display(), e))?;
    let ext = path.extension().and_then(|e| e.to_str());
    decode(&bytes, ext)
}

/// Decodes a PNG, JPEG, BMP, WebP, GIF or TIFF image held in memory. A GIF
/// is read from its first frame.
///
/// `ext_hint` is the file extension without the dot (`"png"`, `"jpg"`,
/// `"jpeg"`, `"bmp"`, `"webp"`, `"gif"`, `"tif"` or `"tiff"`, any case).
/// When it is missing or not one of those, the format is recognised from
/// the bytes.
///
/// # Errors
/// Returns [`CoreError::Unsupported`] when the bytes are not one of the
/// six formats, and [`CoreError::Format`] when they are but cannot be
/// decoded.
pub fn decode(bytes: &[u8], ext_hint: Option<&str>) -> Result<Bitmap, CoreError> {
    let hinted = ext_hint.and_then(format_of_extension);
    let format = match hinted {
        Some(format) => format,
        None => image::guess_format(bytes)
            .ok()
            .filter(|f| {
                matches!(
                    f,
                    ImageFormat::Png
                        | ImageFormat::Jpeg
                        | ImageFormat::Bmp
                        | ImageFormat::WebP
                        | ImageFormat::Gif
                        | ImageFormat::Tiff
                )
            })
            .ok_or_else(|| {
                CoreError::unsupported(
                    "only PNG, JPEG, BMP, WebP, GIF and TIFF images can be traced",
                )
            })?,
    };
    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| CoreError::format(format_label(format), e))?;
    Ok(from_image(&image))
}

/// The decoder an extension names, if it is one this build reads.
fn format_of_extension(ext: &str) -> Option<ImageFormat> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "bmp" => Some(ImageFormat::Bmp),
        "webp" => Some(ImageFormat::WebP),
        "gif" => Some(ImageFormat::Gif),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

/// The name of a format in an error message.
fn format_label(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Bmp => "BMP",
        ImageFormat::WebP => "WebP",
        ImageFormat::Gif => "GIF",
        ImageFormat::Tiff => "TIFF",
        _ => "image",
    }
}

/// Rec. 709 luma of an 8 bit sRGB triple, rounded.
fn luma_709(r: u8, g: u8, b: u8) -> u8 {
    let sum = 2126 * u32::from(r) + 7152 * u32::from(g) + 722 * u32::from(b);
    ((sum + 5000) / 10000).min(255) as u8
}

/// Converts a decoded image to a bitmap, keeping alpha only when the image
/// has an alpha channel.
fn from_image(image: &DynamicImage) -> Bitmap {
    let has_alpha = image.color().has_alpha();
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let count = width as usize * height as usize;
    let mut luma = Vec::with_capacity(count);
    let mut alpha = if has_alpha {
        Some(Vec::with_capacity(count))
    } else {
        None
    };
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        luma.push(luma_709(r, g, b));
        if let Some(out) = alpha.as_mut() {
            out.push(a);
        }
    }
    Bitmap {
        width,
        height,
        luma,
        alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma, Rgba};
    use std::io::Cursor;

    fn encode(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).expect("encodes");
        bytes.into_inner()
    }

    #[test]
    fn a_png_round_trips() {
        let buffer: ImageBuffer<Luma<u8>, Vec<u8>> =
            ImageBuffer::from_fn(4, 3, |x, y| Luma([(x * 60 + y * 10) as u8]));
        let bytes = encode(DynamicImage::ImageLuma8(buffer.clone()), ImageFormat::Png);
        let bitmap = decode(&bytes, Some("PNG")).expect("decodes");
        assert_eq!((bitmap.width, bitmap.height), (4, 3));
        assert_eq!(bitmap.luma, buffer.into_raw());
        assert!(bitmap.alpha.is_none());

        let sniffed = decode(&bytes, None).expect("recognised from the bytes");
        assert_eq!(sniffed, bitmap);
    }

    #[test]
    fn colour_and_alpha_are_converted() {
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_vec(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0])
                .expect("buffer");
        let bytes = encode(DynamicImage::ImageRgba8(buffer), ImageFormat::Png);
        let bitmap = decode(&bytes, Some("png")).expect("decodes");
        assert_eq!(bitmap.luma, vec![54, 182, 18]);
        assert_eq!(bitmap.alpha, Some(vec![255, 128, 0]));
    }

    #[test]
    fn a_bmp_decodes() {
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(2, 2, Rgba([10, 10, 10, 255]));
        let rgb = DynamicImage::ImageRgba8(buffer).to_rgb8();
        let bytes = encode(DynamicImage::ImageRgb8(rgb), ImageFormat::Bmp);
        let bitmap = decode(&bytes, Some("bmp")).expect("decodes");
        assert_eq!(bitmap.luma, vec![10; 4]);
    }

    #[test]
    fn a_jpeg_decodes() {
        let buffer: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_pixel(8, 8, Luma([200]));
        let bytes = encode(DynamicImage::ImageLuma8(buffer), ImageFormat::Jpeg);
        let bitmap = decode(&bytes, Some("jpg")).expect("decodes");
        assert_eq!((bitmap.width, bitmap.height), (8, 8));
        assert!(bitmap.luma.iter().all(|v| v.abs_diff(200) <= 2));
    }

    #[test]
    fn a_webp_decodes() {
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(3, 2, |x, y| {
            Rgba([(x * 80) as u8, 0, 0, if y == 0 { 255 } else { 0 }])
        });
        let bytes = encode(DynamicImage::ImageRgba8(buffer), ImageFormat::WebP);
        let bitmap = decode(&bytes, Some("WEBP")).expect("decodes");
        assert_eq!((bitmap.width, bitmap.height), (3, 2));
        assert_eq!(bitmap.luma[..3], [0, 17, 34]);
        assert_eq!(bitmap.alpha, Some(vec![255, 255, 255, 0, 0, 0]));
        assert_eq!(decode(&bytes, None).expect("sniffed"), bitmap);
    }

    #[test]
    fn garbage_is_refused() {
        let error = decode(b"not an image", None).expect_err("unknown");
        assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
        let error = decode(b"not an image", Some("png")).expect_err("broken");
        assert!(matches!(error, CoreError::Format { .. }), "{error}");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let error = read(FsPath::new("no/such/picture.png")).expect_err("missing");
        assert!(matches!(error, CoreError::Io { .. }), "{error}");
    }

    #[test]
    fn downsampling_averages_blocks() {
        let bitmap =
            Bitmap::new(4, 2, vec![0, 100, 200, 200, 0, 100, 200, 200], None).expect("bitmap");
        assert_eq!(bitmap.downsample(8), bitmap);
        assert_eq!(bitmap.downsample_factor(2), 2);
        let small = bitmap.downsample(2);
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.luma, vec![50, 200]);

        let odd = Bitmap::new(3, 3, vec![0; 9], Some(vec![255; 9])).expect("bitmap");
        let small = odd.downsample(4);
        assert_eq!((small.width, small.height), (2, 2));
        assert_eq!(small.alpha, Some(vec![255; 4]));
    }

    #[test]
    fn mismatched_channels_are_refused() {
        assert!(Bitmap::new(2, 2, vec![0; 3], None).is_err());
        assert!(Bitmap::new(2, 2, vec![0; 4], Some(vec![0; 5])).is_err());
    }
}
