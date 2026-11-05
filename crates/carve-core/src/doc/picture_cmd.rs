//! Picture tools for placed bitmaps: Edit Picture (contrast, brightness,
//! gamma, invert, greyscale and a faded border) and Crop Bitmap (clear the
//! picture outside selected closed vectors and trim it to what is left).
//!
//! Both write a new PNG into the image store and point the bitmap entity at
//! it, as one undo step; the stored image the entity used before is left
//! alone, so undo simply points back at it.
//!
//! Commands:
//! - `bitmap.editPicture {id?, contrast, brightness, gamma, invert,
//!   greyscale, border, fade}` edits the given or the selected bitmap.
//! - `bitmap.crop {id?, boundaries?}` crops the given or the selected bitmap
//!   to the given or the selected closed vectors.

use std::io::Cursor;

use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::{json, Value};

use super::bitmap_cmd::{image_to_job, stored_image};
use super::{BitmapRef, Document, EntityKind};
use crate::error::CoreError;
use crate::geom::{flatten_subpath, path_is_closed, Mat3, Point, Tolerance};
use crate::kernel::KernelReply;

/// Distance crop boundaries are flattened to, in millimetres.
const CROP_TOLERANCE_MM: f64 = 0.01;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(name, "bitmap.editPicture" | "bitmap.crop")
}

/// The border Edit Picture fades the picture into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PictureBorder {
    /// No border.
    #[default]
    None,
    /// Fades in from the four edges.
    Rectangle,
    /// Fades in from the ellipse that fits the picture.
    Oval,
}

/// Edit Picture settings. The defaults leave the picture as it is.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PictureEdit {
    /// -100 (flat grey) to 100 (strongest), 0 leaves the contrast.
    pub contrast: f64,
    /// -100 (black) to 100 (white), 0 leaves the brightness.
    pub brightness: f64,
    /// Gamma correction, 0.1 to 10; above 1 lightens the mid tones.
    pub gamma: f64,
    /// Swaps light and dark.
    pub invert: bool,
    /// Turns the picture black and white (grey levels).
    pub greyscale: bool,
    /// The border shape.
    pub border: PictureBorder,
    /// Width of the border fade as a percentage of the picture's smaller
    /// side, 0 to 50.
    pub fade: f64,
}

impl Default for PictureEdit {
    fn default() -> Self {
        PictureEdit {
            contrast: 0.0,
            brightness: 0.0,
            gamma: 1.0,
            invert: false,
            greyscale: false,
            border: PictureBorder::None,
            fade: 10.0,
        }
    }
}

impl PictureEdit {
    fn check(&self) -> Result<(), CoreError> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.contrast, -100.0, 100.0) {
            return Err(CoreError::validation("contrast must be from -100 to 100"));
        }
        if !within(self.brightness, -100.0, 100.0) {
            return Err(CoreError::validation("brightness must be from -100 to 100"));
        }
        if !within(self.gamma, 0.1, 10.0) {
            return Err(CoreError::validation("gamma must be from 0.1 to 10"));
        }
        if !within(self.fade, 0.0, 50.0) {
            return Err(CoreError::validation(
                "the border fade must be from 0 to 50 %",
            ));
        }
        Ok(())
    }

    /// Maps one channel value, 0 to 1, through contrast, brightness, gamma
    /// and invert.
    fn tone(&self, value: f64) -> f64 {
        let factor = if self.contrast >= 0.0 {
            1.0 + self.contrast / 100.0 * 4.0
        } else {
            1.0 + self.contrast / 100.0
        };
        let mut v = (value - 0.5) * factor + 0.5 + self.brightness / 100.0;
        v = v.clamp(0.0, 1.0).powf(1.0 / self.gamma);
        if self.invert {
            v = 1.0 - v;
        }
        v
    }

    /// How much of the picture shows at pixel centre (x, y) of a w by h
    /// picture, 0 (all border) to 1.
    fn border_weight(&self, x: f64, y: f64, w: f64, h: f64) -> f64 {
        let fade = self.fade / 100.0 * w.min(h);
        match self.border {
            PictureBorder::None => 1.0,
            PictureBorder::Rectangle => {
                if fade <= 0.0 {
                    return 1.0;
                }
                let edge = x.min(w - x).min(y).min(h - y);
                smooth(edge / fade)
            }
            PictureBorder::Oval => {
                let (rx, ry) = (w / 2.0, h / 2.0);
                let (dx, dy) = ((x - rx) / rx, (y - ry) / ry);
                let r = (dx * dx + dy * dy).sqrt();
                // Distance inside the ellipse, measured along the smaller radius.
                let inside = (1.0 - r) * rx.min(ry);
                if fade <= 0.0 {
                    return if inside >= 0.0 { 1.0 } else { 0.0 };
                }
                smooth(inside / fade)
            }
        }
    }
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Applies Edit Picture to an image. The border fades to white.
pub fn edit_picture(image: &RgbaImage, edit: &PictureEdit) -> RgbaImage {
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        *slot = to_u8(edit.tone(i as f64 / 255.0));
    }
    ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
        let Rgba([r, g, b, a]) = *image.get_pixel(x, y);
        let (mut r, mut g, mut b) = (r, g, b);
        if edit.greyscale {
            let luma = (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b))
                .round()
                .clamp(0.0, 255.0) as u8;
            (r, g, b) = (luma, luma, luma);
        }
        let (r, g, b) = (lut[r as usize], lut[g as usize], lut[b as usize]);
        let weight = edit.border_weight(f64::from(x) + 0.5, f64::from(y) + 0.5, w, h);
        let blend = |c: u8| to_u8((f64::from(c) / 255.0) * weight + (1.0 - weight));
        Rgba([blend(r), blend(g), blend(b), a])
    })
}

fn to_u8(v: f64) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// The pixel rectangle a crop keeps: left, top, width, height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    /// First kept column.
    pub x: u32,
    /// First kept row, from the top.
    pub y: u32,
    /// Kept columns.
    pub width: u32,
    /// Kept rows.
    pub height: u32,
}

/// Clears the pixels whose centres lie outside `boundaries` and trims the
/// image to what is left. Each boundary is one selected vector's closed
/// polylines in image space (pixels, origin bottom left, Y up). A pixel is
/// kept when it lies inside any of the vectors: the vectors join (union),
/// while within one vector the rings are even-odd, so a hole in a vector is
/// cleared too.
///
/// # Errors
/// [`CoreError::Validation`] when no pixel lies inside the boundaries.
pub fn crop_to_boundaries(
    image: &RgbaImage,
    boundaries: &[Vec<Vec<Point>>],
) -> Result<(RgbaImage, CropRect), CoreError> {
    let (width, height) = image.dimensions();
    let mut mask = vec![false; width as usize * height as usize];
    let mut crossings: Vec<f64> = Vec::new();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for row in 0..height {
        // Image space y of this row's pixel centres.
        let y = f64::from(height - row) - 0.5;
        for vector in boundaries {
            crossings.clear();
            for ring in vector {
                let n = ring.len();
                for i in 0..n {
                    let a = ring[i];
                    let b = ring[(i + 1) % n];
                    if (a.y > y) != (b.y > y) {
                        crossings.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                    }
                }
            }
            crossings.sort_by(|p, q| p.total_cmp(q));
            for span in crossings.chunks_exact(2) {
                // Pixel centres col + 0.5 inside [span0, span1).
                let first = (span[0] - 0.5).ceil().max(0.0);
                let last = (span[1] - 0.5).ceil().min(f64::from(width));
                if last <= first {
                    continue;
                }
                let (first, last) = (first as u32, last as u32);
                for col in first..last {
                    mask[row as usize * width as usize + col as usize] = true;
                }
                min_x = min_x.min(first);
                max_x = max_x.max(last - 1);
                min_y = min_y.min(row);
                max_y = max_y.max(row);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return Err(CoreError::validation(
            "the selected vectors do not cover any of the bitmap",
        ));
    }
    let rect = CropRect {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
    };
    let cropped = ImageBuffer::from_fn(rect.width, rect.height, |x, y| {
        let (sx, sy) = (x + rect.x, y + rect.y);
        let mut pixel = *image.get_pixel(sx, sy);
        if !mask[sy as usize * width as usize + sx as usize] {
            pixel = Rgba([pixel[0], pixel[1], pixel[2], 0]);
        }
        pixel
    });
    Ok((cropped, rect))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditArgs {
    #[serde(default)]
    id: Option<u64>,
    #[serde(flatten)]
    edit: PictureEditArgs,
}

/// `PictureEdit` with every field optional on the wire.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PictureEditArgs {
    contrast: Option<f64>,
    brightness: Option<f64>,
    gamma: Option<f64>,
    invert: Option<bool>,
    greyscale: Option<bool>,
    border: Option<PictureBorder>,
    fade: Option<f64>,
}

impl PictureEditArgs {
    fn resolve(&self) -> PictureEdit {
        let d = PictureEdit::default();
        PictureEdit {
            contrast: self.contrast.unwrap_or(d.contrast),
            brightness: self.brightness.unwrap_or(d.brightness),
            gamma: self.gamma.unwrap_or(d.gamma),
            invert: self.invert.unwrap_or(d.invert),
            greyscale: self.greyscale.unwrap_or(d.greyscale),
            border: self.border.unwrap_or(d.border),
            fade: self.fade.unwrap_or(d.fade),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CropArgs {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    boundaries: Option<Vec<u64>>,
}

fn parse<T: for<'de> Deserialize<'de> + Default>(name: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(trimmed).map_err(|error| CoreError::invalid_args(name, error))
}

/// Runs a picture command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for another name, [`CoreError::InvalidArgs`]
/// for arguments that do not parse, [`CoreError::Validation`] for bad
/// values, a missing bitmap or boundary, and the image codec's errors.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "bitmap.editPicture" => {
            let args: EditArgs = parse(name, args)?;
            let edit = args.edit.resolve();
            edit.check()?;
            let id = bitmap_id(document, args.id)?;
            let (bitmap, image) = bitmap_image(document, id)?;
            let edited = edit_picture(&image, &edit);
            let image_id = store_png(document, &edited)?;
            in_step(document, "Edit Picture", |doc| {
                let mut entity = doc.entity(id)?.clone();
                entity.kind = EntityKind::Bitmap(BitmapRef { image_id, ..bitmap });
                doc.replace_entity(id, entity)?;
                Ok(json!({ "id": id, "imageId": image_id }))
            })
        }
        "bitmap.crop" => {
            let args: CropArgs = parse(name, args)?;
            let id = bitmap_id(document, args.id)?;
            let boundary_ids: Vec<u64> = match args.boundaries {
                Some(ids) if !ids.is_empty() => ids,
                _ => document
                    .selection()
                    .iter()
                    .copied()
                    .filter(|s| *s != id)
                    .collect(),
            };
            let (bitmap, image) = bitmap_image(document, id)?;
            let to_job = image_to_job(&bitmap, image.width(), image.height())?;
            let to_image = to_job.invert()?;
            let rings = boundary_rings(document, &boundary_ids, &to_image)?;
            let (cropped, rect) = crop_to_boundaries(&image, &rings)?;
            let quad = cropped_quad(&to_job, image.height(), rect);
            let image_id = store_png(document, &cropped)?;
            in_step(document, "Crop Bitmap", |doc| {
                let mut entity = doc.entity(id)?.clone();
                entity.kind = EntityKind::Bitmap(BitmapRef {
                    image_id,
                    quad,
                    ..bitmap
                });
                doc.replace_entity(id, entity)?;
                doc.set_selection(&[id]);
                Ok(json!({ "id": id, "imageId": image_id }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The bitmap a command works on: the one given, or the first selected.
fn bitmap_id(document: &Document, given: Option<u64>) -> Result<u64, CoreError> {
    if let Some(id) = given {
        return Ok(id);
    }
    document
        .selection()
        .iter()
        .copied()
        .find(|id| {
            document
                .entity(*id)
                .is_ok_and(|e| matches!(e.kind, EntityKind::Bitmap(_)))
        })
        .ok_or_else(|| CoreError::validation("select a bitmap first"))
}

fn bitmap_image(document: &Document, id: u64) -> Result<(BitmapRef, RgbaImage), CoreError> {
    let EntityKind::Bitmap(bitmap) = &document.entity(id)?.kind else {
        return Err(CoreError::validation("select a bitmap first"));
    };
    let stored = document
        .images()
        .get(bitmap.image_id)
        .ok_or_else(|| CoreError::validation("the bitmap's image is missing"))?;
    let decoded = image::load_from_memory(&stored.bytes)
        .map_err(|error| CoreError::format("image", error))?;
    Ok((bitmap.clone(), decoded.to_rgba8()))
}

fn store_png(document: &mut Document, image: &RgbaImage) -> Result<u64, CoreError> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|error| CoreError::format("PNG", error))?;
    let stored = stored_image(bytes, Some("png"))?;
    Ok(document.images_mut().insert(stored))
}

/// The closed vectors of `ids`, flattened and taken into image space.
fn boundary_rings(
    document: &Document,
    ids: &[u64],
    to_image: &Mat3,
) -> Result<Vec<Vec<Vec<Point>>>, CoreError> {
    let tol = Tolerance::new(CROP_TOLERANCE_MM)?;
    let mut rings = Vec::new();
    for id in ids {
        let mut vector: Vec<Vec<Point>> = Vec::new();
        for (_, path) in document.paths_of(*id) {
            if !path_is_closed(&path) {
                continue;
            }
            for sub in &path.subpaths {
                let flat = flatten_subpath(sub, tol);
                if flat.points.len() >= 3 {
                    vector.push(flat.points.iter().map(|p| to_image.apply(*p)).collect());
                }
            }
        }
        if !vector.is_empty() {
            rings.push(vector);
        }
    }
    if rings.is_empty() {
        return Err(CoreError::validation(
            "select the bitmap, then the closed vectors to crop it to",
        ));
    }
    Ok(rings)
}

/// The job corners (lower left, lower right, upper right, upper left) of the
/// part of an image `height` pixels high that `rect` keeps.
fn cropped_quad(to_job: &Mat3, height: u32, rect: CropRect) -> Vec<Point> {
    let left = f64::from(rect.x);
    let right = f64::from(rect.x + rect.width);
    let bottom = f64::from(height - rect.y - rect.height);
    let top = f64::from(height - rect.y);
    [
        Point::new(left, bottom),
        Point::new(right, bottom),
        Point::new(right, top),
        Point::new(left, top),
    ]
    .into_iter()
    .map(|p| to_job.apply(p))
    .collect()
}

/// Runs `action` as one undo step called `label`, rolling back on failure.
fn in_step<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(mut value) => {
            let revision = document.commit();
            value["revision"] = json!(revision);
            KernelReply::ok(&value)
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::entity::Entity;
    use crate::geom::{circle, rect};

    fn grey(width: u32, height: u32, level: u8) -> RgbaImage {
        ImageBuffer::from_pixel(width, height, Rgba([level, level, level, 255]))
    }

    /// A document with a 100 by 50 pixel bitmap placed over 0..100, 0..50 mm.
    fn with_bitmap(image: &RgbaImage) -> (Document, u64) {
        let mut doc = Document::new();
        let image_id = store_png(&mut doc, image).expect("png");
        doc.begin("Place");
        let (w, h) = (f64::from(image.width()), f64::from(image.height()));
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Bitmap(BitmapRef {
                image_id,
                quad: vec![
                    Point::new(0.0, 0.0),
                    Point::new(w, 0.0),
                    Point::new(w, h),
                    Point::new(0.0, h),
                ],
                opacity: 1.0,
            }),
        });
        doc.commit();
        (doc, id)
    }

    fn add_path(doc: &mut Document, path: crate::geom::Path) -> u64 {
        doc.begin("Draw");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        id
    }

    #[test]
    fn default_settings_leave_the_picture_alone() {
        let image = grey(4, 4, 100);
        assert_eq!(edit_picture(&image, &PictureEdit::default()), image);
    }

    #[test]
    fn brightness_contrast_gamma_and_invert_move_the_tones() {
        let tone = |edit: PictureEdit, level: u8| {
            edit_picture(&grey(1, 1, level), &edit).get_pixel(0, 0)[0]
        };
        let base = PictureEdit::default();
        assert!(
            tone(
                PictureEdit {
                    brightness: 50.0,
                    ..base
                },
                100
            ) > 100
        );
        assert!(
            tone(
                PictureEdit {
                    contrast: 50.0,
                    ..base
                },
                200
            ) > 200
        );
        assert!(
            tone(
                PictureEdit {
                    contrast: 50.0,
                    ..base
                },
                50
            ) < 50
        );
        assert_eq!(
            tone(
                PictureEdit {
                    contrast: -100.0,
                    ..base
                },
                30
            ),
            128
        );
        assert!(tone(PictureEdit { gamma: 2.0, ..base }, 100) > 100);
        assert_eq!(
            tone(
                PictureEdit {
                    invert: true,
                    ..base
                },
                0
            ),
            255
        );
    }

    #[test]
    fn greyscale_uses_the_luminance() {
        let image: RgbaImage = ImageBuffer::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
        let edit = PictureEdit {
            greyscale: true,
            ..PictureEdit::default()
        };
        let pixel = *edit_picture(&image, &edit).get_pixel(0, 0);
        assert_eq!(pixel, Rgba([76, 76, 76, 255]));
    }

    #[test]
    fn a_border_fades_the_edges_to_white() {
        let image = grey(100, 100, 0);
        for border in [PictureBorder::Rectangle, PictureBorder::Oval] {
            let edit = PictureEdit {
                border,
                fade: 20.0,
                ..PictureEdit::default()
            };
            let out = edit_picture(&image, &edit);
            assert_eq!(out.get_pixel(0, 0)[0], 255, "{border:?} corner");
            assert_eq!(out.get_pixel(50, 50)[0], 0, "{border:?} centre");
        }
    }

    #[test]
    fn edit_picture_swaps_the_image_in_one_undo_step() {
        let (mut doc, id) = with_bitmap(&grey(10, 10, 100));
        doc.set_selection(&[id]);
        let before = match &doc.entity(id).expect("entity").kind {
            EntityKind::Bitmap(b) => b.image_id,
            _ => panic!("bitmap"),
        };
        dispatch(&mut doc, "bitmap.editPicture", r#"{"invert":true}"#).expect("edit");
        let EntityKind::Bitmap(after) = &doc.entity(id).expect("entity").kind else {
            panic!("bitmap");
        };
        assert_ne!(after.image_id, before);
        let (_, image) = bitmap_image(&doc, id).expect("image");
        assert_eq!(image.get_pixel(0, 0)[0], 155);
        doc.undo().expect("undo");
        let EntityKind::Bitmap(undone) = &doc.entity(id).expect("entity").kind else {
            panic!("bitmap");
        };
        assert_eq!(undone.image_id, before);
    }

    #[test]
    fn bad_settings_are_refused() {
        let (mut doc, id) = with_bitmap(&grey(2, 2, 0));
        let args = json!({ "id": id, "gamma": 0.0 }).to_string();
        let error = dispatch(&mut doc, "bitmap.editPicture", &args).expect_err("gamma");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn crop_keeps_what_lies_inside_and_trims_the_picture() {
        let (mut doc, id) = with_bitmap(&grey(100, 50, 0));
        let square = add_path(
            &mut doc,
            rect(Point::new(20.0, 10.0), Point::new(60.0, 40.0)).expect("rect"),
        );
        doc.set_selection(&[id, square]);
        dispatch(&mut doc, "bitmap.crop", "").expect("crop");
        let (bitmap, image) = bitmap_image(&doc, id).expect("image");
        assert_eq!(image.dimensions(), (40, 30));
        assert_eq!(image.get_pixel(0, 0)[3], 255);
        assert_eq!(bitmap.quad[0], Point::new(20.0, 10.0));
        assert_eq!(bitmap.quad[2], Point::new(60.0, 40.0));
        assert_eq!(doc.selection(), &[id]);
    }

    #[test]
    fn crop_clears_the_outside_of_a_round_boundary() {
        let (mut doc, id) = with_bitmap(&grey(100, 50, 0));
        let round = add_path(
            &mut doc,
            circle(Point::new(50.0, 25.0), 20.0).expect("circle"),
        );
        let args = json!({ "id": id, "boundaries": [round] }).to_string();
        dispatch(&mut doc, "bitmap.crop", &args).expect("crop");
        let (_, image) = bitmap_image(&doc, id).expect("image");
        assert_eq!(image.dimensions(), (40, 40));
        assert_eq!(image.get_pixel(0, 0)[3], 0, "corner is cleared");
        assert_eq!(image.get_pixel(20, 20)[3], 255, "centre is kept");
    }

    #[test]
    fn crop_keeps_what_lies_inside_any_of_several_vectors() {
        let (mut doc, id) = with_bitmap(&grey(100, 50, 0));
        // Two overlapping squares: their overlap stays, as does each one.
        let a = add_path(
            &mut doc,
            rect(Point::new(10.0, 10.0), Point::new(50.0, 40.0)).expect("rect"),
        );
        let b = add_path(
            &mut doc,
            rect(Point::new(30.0, 10.0), Point::new(70.0, 40.0)).expect("rect"),
        );
        let args = json!({ "id": id, "boundaries": [a, b] }).to_string();
        dispatch(&mut doc, "bitmap.crop", &args).expect("crop");
        let (_, image) = bitmap_image(&doc, id).expect("image");
        assert_eq!(image.dimensions(), (60, 30));
        // Image columns 20..40 are the overlap (job x 30..50).
        assert_eq!(image.get_pixel(30, 15)[3], 255, "the overlap is kept");
        assert_eq!(image.get_pixel(5, 15)[3], 255, "the first square is kept");
        assert_eq!(image.get_pixel(55, 15)[3], 255, "the second square is kept");
    }

    #[test]
    fn crop_clears_a_hole_inside_one_vector() {
        let (mut doc, id) = with_bitmap(&grey(100, 50, 0));
        let mut ring = rect(Point::new(10.0, 5.0), Point::new(50.0, 45.0)).expect("rect");
        let hole = rect(Point::new(20.0, 15.0), Point::new(40.0, 35.0)).expect("rect");
        ring.subpaths.extend(hole.subpaths);
        let ring = add_path(&mut doc, ring);
        let args = json!({ "id": id, "boundaries": [ring] }).to_string();
        dispatch(&mut doc, "bitmap.crop", &args).expect("crop");
        let (_, image) = bitmap_image(&doc, id).expect("image");
        assert_eq!(image.dimensions(), (40, 40));
        assert_eq!(image.get_pixel(20, 20)[3], 0, "the hole is cleared");
        assert_eq!(image.get_pixel(5, 20)[3], 255, "the ring is kept");
    }

    #[test]
    fn crop_needs_a_boundary_over_the_picture() {
        let (mut doc, id) = with_bitmap(&grey(10, 10, 0));
        doc.set_selection(&[id]);
        let error = dispatch(&mut doc, "bitmap.crop", "").expect_err("no boundary");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
        let away = add_path(
            &mut doc,
            rect(Point::new(200.0, 200.0), Point::new(210.0, 210.0)).expect("rect"),
        );
        let args = json!({ "id": id, "boundaries": [away] }).to_string();
        let error = dispatch(&mut doc, "bitmap.crop", &args).expect_err("outside");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }
}
