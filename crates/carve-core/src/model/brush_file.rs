//! Sculpting brushes kept as files, for the custom brush of the Sculpt
//! form: a relief clip (`.carveclip`) or a picture whose grey levels are
//! the brush heights (light is high, transparent is outside the brush).
//!
//! Create Sculpting Brush writes a component into the brushes folder as a
//! clip, cropped and reduced to at most [`MAX_BRUSH_SIDE`] cells on its
//! longer side so it stamps quickly. Brush files are read once and kept
//! while the same file stays chosen ([`BrushCache`]).

use std::path::{Path as FsPath, PathBuf};

use crate::clipart::clip_file::{self, Clip};
use crate::error::CoreError;
use crate::geom::Point;
use crate::io::bitmap::{self, Bitmap};

use super::custom_brush::BrushShape;
use super::field::Field;

/// Extension of a brush written by Create Sculpting Brush, without the dot.
pub const BRUSH_EXTENSION: &str = "carveclip";
/// Most cells a saved brush or a picture brush has on its longer side.
pub const MAX_BRUSH_SIDE: usize = 256;
/// Alpha below this is outside a picture brush.
const OPAQUE: u8 = 128;
/// Picture files a brush can be read from.
const PICTURES: [&str; 8] = ["png", "jpg", "jpeg", "bmp", "webp", "gif", "tif", "tiff"];

/// The brushes folder: `CarveStudio/brushes` under the platform data
/// directory; unit tests get a folder under the temporary directory.
pub fn brush_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("carve-brushes-{}", std::process::id()));
    }
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("CarveStudio")
        .join("brushes")
}

fn extension(path: &FsPath) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The heights of a brush file.
///
/// # Errors
/// [`CoreError::Unsupported`] for a file that is neither a clip nor a
/// picture, and the errors of reading it.
pub fn read_brush(path: &FsPath) -> Result<Field, CoreError> {
    let ext = extension(path);
    if ext == BRUSH_EXTENSION {
        return Ok(clip_file::read(path)?.field);
    }
    if PICTURES.contains(&ext.as_str()) {
        let picture = bitmap::read(path)?;
        let limit = (MAX_BRUSH_SIDE * MAX_BRUSH_SIDE) as u64;
        return picture_field(&picture.downsample(limit));
    }
    Err(CoreError::unsupported(
        "a brush is a clip file or a picture (PNG, JPEG, BMP, WebP, GIF or TIFF)",
    ))
}

/// A picture's grey levels as heights from 0 (black) to 1 (white), one
/// cell a pixel, the top row of the picture at the top of the field.
///
/// # Errors
/// [`CoreError::Validation`] for a picture with no pixels.
pub fn picture_field(picture: &Bitmap) -> Result<Field, CoreError> {
    picture.check()?;
    let (w, h) = (picture.width as usize, picture.height as usize);
    if w == 0 || h == 0 {
        return Err(CoreError::validation("the picture has no pixels"));
    }
    let mut field = Field::empty(w, h, 1.0, Point::new(0.0, 0.0));
    for (j, row) in field.z.chunks_exact_mut(w).enumerate() {
        let source = (h - 1 - j) * w;
        for (i, slot) in row.iter_mut().enumerate() {
            let at = source + i;
            let seen = picture.alpha.as_ref().is_none_or(|a| a[at] >= OPAQUE);
            if seen {
                *slot = f32::from(picture.luma[at]) / 255.0;
            }
        }
    }
    Ok(field)
}

/// `field` cropped to its material and reduced (block averages) to at most
/// [`MAX_BRUSH_SIDE`] cells on its longer side.
///
/// # Errors
/// [`CoreError::Validation`] for a field with no heights.
pub fn brush_from_field(field: &Field) -> Result<Field, CoreError> {
    let cropped = field
        .cropped()
        .ok_or_else(|| CoreError::validation("the component has no heights to make a brush"))?;
    let k = cropped.width.max(cropped.height).div_ceil(MAX_BRUSH_SIDE);
    if k <= 1 {
        return Ok(cropped);
    }
    let (w, h) = (cropped.width.div_ceil(k), cropped.height.div_ceil(k));
    let mut out = Field::empty(w, h, cropped.cell * k as f64, cropped.origin);
    for (j, row) in out.z.chunks_exact_mut(w).enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            let (mut sum, mut count) = (0.0_f64, 0_u32);
            for y in j * k..((j + 1) * k).min(cropped.height) {
                for x in i * k..((i + 1) * k).min(cropped.width) {
                    let v = cropped.z[y * cropped.width + x];
                    if !v.is_nan() {
                        sum += f64::from(v);
                        count += 1;
                    }
                }
            }
            if count > 0 {
                *slot = (sum / f64::from(count)) as f32;
            }
        }
    }
    Ok(out)
}

/// A free file name for a brush called `name` in `dir`.
fn free_path(dir: &FsPath, name: &str) -> PathBuf {
    let base: String = name
        .trim()
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let base = if base.is_empty() {
        "Brush".to_string()
    } else {
        base
    };
    let mut path = dir.join(format!("{base}.{BRUSH_EXTENSION}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base} {n}.{BRUSH_EXTENSION}"));
        n += 1;
    }
    path
}

/// Writes `field` as a brush called `name` into `dir`, made first if need
/// be, under a name no other file there has. Answers the path.
///
/// # Errors
/// The errors of [`brush_from_field`] and of writing the file.
pub fn save_brush(field: &Field, name: &str, dir: &FsPath) -> Result<PathBuf, CoreError> {
    let brush = brush_from_field(field)?;
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir.display(), e))?;
    let path = free_path(dir, name);
    clip_file::write(
        &Clip {
            name: name.trim().to_string(),
            field: brush,
        },
        &path,
    )?;
    Ok(path)
}

/// The brush file read last, kept while sculpting goes on with it.
#[derive(Debug, Default)]
pub struct BrushCache {
    loaded: Option<(PathBuf, BrushShape)>,
}

impl BrushCache {
    /// The brush in `path`, read unless it is the one read last.
    ///
    /// # Errors
    /// The errors of [`read_brush`] and [`BrushShape::from_field`].
    pub fn get(&mut self, path: &FsPath) -> Result<&BrushShape, CoreError> {
        let fresh = !matches!(&self.loaded, Some((kept, _)) if kept == path);
        if fresh {
            let shape = BrushShape::from_field(&read_brush(path)?)?;
            self.loaded = Some((path.to_path_buf(), shape));
        }
        match &self.loaded {
            Some((_, shape)) => Ok(shape),
            None => Err(CoreError::validation("no brush is loaded")),
        }
    }

    /// Forgets the kept brush, so the next use reads the file again.
    pub fn clear(&mut self) {
        self.loaded = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = brush_dir().join(label);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("makes the folder");
        dir
    }

    fn ramp(width: usize, height: usize) -> Field {
        let mut field = Field::empty(width, height, 0.5, Point::new(10.0, 20.0));
        for (index, v) in field.z.iter_mut().enumerate() {
            *v = (index % width) as f32;
        }
        field
    }

    #[test]
    fn a_picture_is_grey_levels_with_the_top_row_up() {
        let picture =
            Bitmap::new(2, 2, vec![0, 255, 51, 102], Some(vec![255, 0, 255, 255])).expect("bitmap");
        let field = picture_field(&picture).expect("field");
        // Bottom row of the field is the picture's last row.
        assert!((field.get(0, 0) - 0.2).abs() < 1e-6);
        assert!((field.get(1, 0) - 0.4).abs() < 1e-6);
        assert_eq!(field.get(0, 1), 0.0);
        assert!(field.get(1, 1).is_nan(), "transparent is outside");
    }

    #[test]
    fn a_large_component_is_reduced_for_a_brush() {
        let field = ramp(600, 300);
        let brush = brush_from_field(&field).expect("brush");
        assert_eq!((brush.width, brush.height), (200, 100));
        assert!((brush.cell - 1.5).abs() < 1e-12);
        assert!(
            (brush.get(0, 0) - 1.0).abs() < 1e-6,
            "average of 0, 1 and 2"
        );
        let small = brush_from_field(&ramp(10, 5)).expect("small");
        assert_eq!(small.width, 10);
    }

    #[test]
    fn saved_brushes_read_back_and_never_overwrite() {
        let dir = temp_dir("save");
        let first = save_brush(&ramp(8, 4), "Leaf/1", &dir).expect("saves");
        let second = save_brush(&ramp(8, 4), "Leaf/1", &dir).expect("saves again");
        assert_eq!(
            first.file_name().and_then(|n| n.to_str()),
            Some("Leaf_1.carveclip")
        );
        assert_eq!(
            second.file_name().and_then(|n| n.to_str()),
            Some("Leaf_1 2.carveclip")
        );
        let read = read_brush(&first).expect("reads");
        assert_eq!((read.width, read.height), (8, 4));
        let mut cache = BrushCache::default();
        assert!(cache.get(&first).is_ok());
        assert!(cache.get(&dir.join("missing.carveclip")).is_err());
        assert!(read_brush(&dir.join("brush.txt")).is_err());
    }

    #[test]
    fn a_picture_file_is_a_brush() {
        let dir = temp_dir("picture");
        let path = dir.join("dot.png");
        image::GrayImage::from_fn(4, 4, |x, y| image::Luma([if x == y { 255 } else { 0 }]))
            .save(&path)
            .expect("writes a png");
        let field = read_brush(&path).expect("reads");
        assert_eq!((field.width, field.height), (4, 4));
        assert_eq!(field.get(0, 3), 1.0, "the picture's top left pixel");
    }
}
