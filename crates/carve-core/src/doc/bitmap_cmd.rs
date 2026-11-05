//! Placed bitmaps: File > Import Bitmap, the pixels the 2D view draws, the
//! fade of a bitmap and tracing a placed bitmap into vectors.
//!
//! Commands:
//! - `io.importBitmap {path}` stores the image and places it fitted inside
//!   the job, centred, as a selected bitmap entity on the active layer.
//! - `bitmaps.pixels {imageId}` answers the stored file as a blob with the
//!   header `{imageId, width, height, mime}` and one `u8` section `bytes`.
//! - `bitmap.setOpacity {ids, opacity}` fades bitmaps, 0 to 1, one undo step.
//! - `bitmap.traceImage {id, params, colours?, region?}` traces a placed
//!   bitmap; the vectors land exactly over it, following its quad through
//!   moves, scales and rotations. With `colours {count, trace}` the image is
//!   reduced to `count` colours and the areas of the `trace` palette indices
//!   are traced together instead of thresholding. `region [minX, minY, maxX,
//!   maxY]` in job mm limits the trace to the pixels under that box. With
//!   `preview` it answers `{paths}` and changes nothing (the Preview
//!   button).
//! - `bitmap.tracePalette {id, colours}` answers `{colour, colours: [{rgb,
//!   pixels}]}`: whether the image is in colour and its palette reduced to
//!   `colours` (2 to 16), most pixels first.
//! - `bitmap.traceIndices {id, colours}` answers a blob with the header
//!   `{width, height}` and one `u8` section `indices`: each pixel's palette
//!   index, row by row from the top, 255 where transparent, so the view can
//!   show the picture in its reduced colours.
//! - `bitmap.colourAt {id, colours, x, y}` answers `{index}`, the palette
//!   index under a job point, or null off the image.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use super::images::{self, StoredImage};
use super::{BitmapRef, Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{self, transform_path, Mat3, Point};
use crate::kernel::KernelReply;

/// Largest image the store takes, in pixels.
pub const MAX_IMPORT_PIXELS: u64 = 100_000_000;

/// Largest image tracing works on; bigger ones are merged in blocks first.
const TRACE_MAX_PIXELS: u64 = 4_000_000;

/// True when `name` is one of the commands this module answers to.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "io.importBitmap"
            | "bitmaps.pixels"
            | "bitmap.setOpacity"
            | "bitmap.traceImage"
            | "bitmap.tracePalette"
            | "bitmap.traceIndices"
            | "bitmap.colourAt"
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportArgs {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PixelsArgs {
    image_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpacityArgs {
    ids: Vec<u64>,
    opacity: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TraceImageArgs {
    id: u64,
    #[serde(default)]
    params: geom::trace::TraceParams,
    #[serde(default)]
    colours: Option<ColourPick>,
    #[serde(default)]
    region: Option<[f64; 4]>,
    /// Preview: answer the traced paths without making anything.
    #[serde(default)]
    preview: bool,
}

/// The colours a colour trace works with.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ColourPick {
    /// How many colours the image is reduced to, 2 to 16.
    count: usize,
    /// Palette indices traced together.
    trace: Vec<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaletteArgs {
    id: u64,
    #[serde(default = "default_colours")]
    colours: usize,
}

fn default_colours() -> usize {
    crate::io::colours::BASE_COLOURS
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ColourAtArgs {
    id: u64,
    #[serde(default = "default_colours")]
    colours: usize,
    x: f64,
    y: f64,
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() { "{}" } else { trimmed };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(name, error))
}

/// Runs one of this module's commands.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name this module does not answer to,
/// and whatever the command itself reports.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "io.importBitmap" => {
            let args: ImportArgs = parse(name, args)?;
            let image = read_image(&args.path)?;
            let stem = args
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned());
            import(document, image, stem)
        }
        "bitmaps.pixels" => {
            let args: PixelsArgs = parse(name, args)?;
            let image = document.images().get(args.image_id).ok_or_else(|| {
                CoreError::validation(format!("there is no image {}", args.image_id))
            })?;
            Ok(KernelReply::Bytes(pixels_blob(args.image_id, image)?))
        }
        "bitmap.setOpacity" => {
            let args: OpacityArgs = parse(name, args)?;
            if !(args.opacity.is_finite() && (0.0..=1.0).contains(&args.opacity)) {
                return Err(CoreError::validation("the fade must be from 0 to 1"));
            }
            in_step(document, "Fade Bitmap", |doc| {
                let mut changed = Vec::new();
                for id in args.ids {
                    let mut entity = doc.entity(id)?.clone();
                    if let EntityKind::Bitmap(bitmap) = &mut entity.kind {
                        bitmap.opacity = args.opacity;
                        doc.replace_entity(id, entity)?;
                        changed.push(id);
                    }
                }
                if changed.is_empty() {
                    return Err(CoreError::validation("select a bitmap to fade"));
                }
                Ok(json!({ "ids": changed }))
            })
        }
        "bitmap.tracePalette" => {
            let args: PaletteArgs = parse(name, args)?;
            KernelReply::ok(&trace_palette(document, &args)?)
        }
        "bitmap.traceIndices" => {
            let args: PaletteArgs = parse(name, args)?;
            Ok(KernelReply::Bytes(trace_indices(document, &args)?))
        }
        "bitmap.colourAt" => {
            let args: ColourAtArgs = parse(name, args)?;
            KernelReply::ok(&colour_at(document, &args)?)
        }
        "bitmap.traceImage" => {
            let args: TraceImageArgs = parse(name, args)?;
            let paths = trace_entity(
                document,
                args.id,
                &args.params,
                args.colours.as_ref(),
                args.region,
            )?;
            if args.preview {
                return KernelReply::ok(&json!({ "paths": paths }));
            }
            let layer = document.active_layer();
            let sheet = document.active_sheet();
            in_step(document, "Trace Bitmap", |doc| {
                let ids: Vec<u64> = paths
                    .into_iter()
                    .map(|path| {
                        doc.insert_entity(Entity {
                            rotation: 0.0,
                            id: 0,
                            layer,
                            sheet,
                            revision: 0,
                            name: None,
                            parent: None,
                            kind: EntityKind::Path { path },
                        })
                    })
                    .collect();
                doc.set_selection(&ids);
                Ok(json!({ "ids": ids }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
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

/// Reads an image file into the form the store keeps, checking it decodes.
///
/// # Errors
/// [`CoreError::Io`] when the file cannot be read, [`CoreError::Unsupported`]
/// for a format other than PNG, JPEG, BMP or WebP, and the decoder's errors.
pub fn read_image(path: &std::path::Path) -> Result<StoredImage, CoreError> {
    let bytes = std::fs::read(path).map_err(|error| CoreError::io(path.display(), error))?;
    let ext = path.extension().and_then(|e| e.to_str());
    stored_image(bytes, ext)
}

/// Checks encoded image bytes and wraps them for the store.
///
/// # Errors
/// [`CoreError::Unsupported`] for a format the store does not keep,
/// [`CoreError::Validation`] for an empty or oversized image, and the
/// decoder's errors.
pub fn stored_image(bytes: Vec<u8>, ext: Option<&str>) -> Result<StoredImage, CoreError> {
    let format = images::sniff(&bytes)
        .or_else(|| ext.and_then(images::format_of))
        .ok_or_else(|| {
            CoreError::unsupported("only PNG, JPEG, BMP, WebP, GIF and TIFF images can be imported")
        })?;
    let (bytes, format) = if format == "tif" {
        (tiff_as_png(&bytes)?, "png")
    } else {
        (bytes, format)
    };
    let decoded = crate::io::bitmap::decode(&bytes, Some(format))?;
    let pixels = u64::from(decoded.width) * u64::from(decoded.height);
    if pixels == 0 {
        return Err(CoreError::validation("the image has no pixels"));
    }
    if pixels > MAX_IMPORT_PIXELS {
        return Err(CoreError::validation(
            "the image is too large; images up to 100 megapixels can be imported",
        ));
    }
    Ok(StoredImage {
        width: decoded.width,
        height: decoded.height,
        format: format.to_string(),
        bytes: Arc::new(bytes),
    })
}

/// Re-encodes a TIFF as a PNG of the same pixels, since the view cannot
/// show TIFF.
fn tiff_as_png(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff)
        .map_err(|error| CoreError::format("TIFF", error))?;
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels > MAX_IMPORT_PIXELS {
        return Err(CoreError::validation(
            "the image is too large; images up to 100 megapixels can be imported",
        ));
    }
    // PNG takes 8 and 16 bit grey and colour; anything else becomes RGBA.
    let image = match image.color() {
        image::ColorType::L8
        | image::ColorType::La8
        | image::ColorType::Rgb8
        | image::ColorType::Rgba8
        | image::ColorType::L16
        | image::ColorType::La16
        | image::ColorType::Rgb16
        | image::ColorType::Rgba16 => image,
        _ => image::DynamicImage::ImageRgba8(image.to_rgba8()),
    };
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|error| CoreError::format("PNG", error))?;
    Ok(out.into_inner())
}

/// Places `image` as large as fits inside the job, centred, as a selected
/// bitmap entity on the active layer and sheet. One undo step.
fn import(
    document: &mut Document,
    image: StoredImage,
    name: Option<String>,
) -> Result<KernelReply, CoreError> {
    let job = document.job_bounds();
    let (width, height) = (f64::from(image.width), f64::from(image.height));
    let pixel = (job.width() / width).min(job.height() / height);
    let (w, h) = (width * pixel, height * pixel);
    let centre = job.centre();
    let (x0, y0) = (centre.x - w / 2.0, centre.y - h / 2.0);
    let quad = vec![
        Point::new(x0, y0),
        Point::new(x0 + w, y0),
        Point::new(x0 + w, y0 + h),
        Point::new(x0, y0 + h),
    ];
    let (pixels_wide, pixels_high) = (image.width, image.height);
    let image_id = document.images_mut().insert(image);
    let layer = document.active_layer();
    let sheet = document.active_sheet();
    in_step(document, "Import Bitmap", |doc| {
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: 0,
            name,
            parent: None,
            kind: EntityKind::Bitmap(BitmapRef {
                image_id,
                quad,
                opacity: 1.0,
            }),
        });
        doc.set_selection(&[id]);
        Ok(json!({
            "id": id,
            "imageId": image_id,
            "width": pixels_wide,
            "height": pixels_high,
        }))
    })
}

/// The blob `bitmaps.pixels` answers.
fn pixels_blob(image_id: u64, image: &StoredImage) -> Result<Vec<u8>, CoreError> {
    let count = image.bytes.len();
    let header = serde_json::to_vec(&json!({
        "imageId": image_id,
        "width": image.width,
        "height": image.height,
        "mime": image.mime(),
        "sections": [{ "name": "bytes", "dtype": "u8", "offset": 0, "count": count }],
    }))?;
    let mut out = Vec::with_capacity(4 + header.len() + count);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&image.bytes);
    Ok(out)
}

/// True when entity `id` is a bitmap whose picture covers `at`.
pub fn bitmap_contains(document: &Document, id: u64, at: Point) -> bool {
    let Ok(entity) = document.entity(id) else {
        return false;
    };
    let EntityKind::Bitmap(bitmap) = &entity.kind else {
        return false;
    };
    let quad = &bitmap.quad;
    let mut inside = false;
    for (i, a) in quad.iter().enumerate() {
        let b = quad[(i + 1) % quad.len()];
        if (a.y > at.y) != (b.y > at.y) {
            let x = a.x + (at.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if at.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The transform from image space (pixels, origin at the bottom left, Y up)
/// to the job, taken from a bitmap's corner quad.
///
/// # Errors
/// [`CoreError::Validation`] when the quad has fewer than four corners.
pub fn image_to_job(bitmap: &BitmapRef, width: u32, height: u32) -> Result<Mat3, CoreError> {
    let [p0, p1, _, p3] = bitmap.quad[..] else {
        return Err(CoreError::validation("the bitmap has no placement"));
    };
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    Ok(Mat3::new(
        (p1.x - p0.x) / w,
        (p1.y - p0.y) / w,
        (p3.x - p0.x) / h,
        (p3.y - p0.y) / h,
        p0.x,
        p0.y,
    ))
}

/// The placement and stored image of bitmap entity `id`.
fn placed_image(document: &Document, id: u64) -> Result<(&BitmapRef, &StoredImage), CoreError> {
    let EntityKind::Bitmap(bitmap) = &document.entity(id)?.kind else {
        return Err(CoreError::validation("select a bitmap to trace"));
    };
    let image = document
        .images()
        .get(bitmap.image_id)
        .ok_or_else(|| CoreError::validation("the bitmap's image is missing"))?;
    Ok((bitmap, image))
}

/// The palette reply of `bitmap.tracePalette`.
fn trace_palette(document: &Document, args: &PaletteArgs) -> Result<Value, CoreError> {
    let (_, image) = placed_image(document, args.id)?;
    let colour = crate::io::colours::decode(&image.bytes)?;
    let palette = colour.palette(args.colours);
    Ok(json!({ "colour": colour.is_colour(), "colours": palette.colours }))
}

/// The blob of `bitmap.traceIndices`.
fn trace_indices(document: &Document, args: &PaletteArgs) -> Result<Vec<u8>, CoreError> {
    let (_, image) = placed_image(document, args.id)?;
    let colour = crate::io::colours::decode(&image.bytes)?;
    let indices = colour.indices(&colour.palette(args.colours));
    let header = serde_json::to_vec(&json!({
        "width": colour.width,
        "height": colour.height,
        "sections": [{ "name": "indices", "dtype": "u8", "offset": 0, "count": indices.len() }],
    }))?;
    let mut out = Vec::with_capacity(4 + header.len() + indices.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&indices);
    Ok(out)
}

/// The palette index under a job point, for `bitmap.colourAt`.
fn colour_at(document: &Document, args: &ColourAtArgs) -> Result<Value, CoreError> {
    let (bitmap, image) = placed_image(document, args.id)?;
    let colour = crate::io::colours::decode(&image.bytes)?;
    let palette = colour.palette(args.colours);
    let to_image = image_to_job(bitmap, colour.width, colour.height)?.invert()?;
    let p = to_image.apply(Point::new(args.x, args.y));
    let col = p.x.floor() as i64;
    let row = i64::from(colour.height) - 1 - p.y.floor() as i64;
    Ok(json!({ "index": colour.index_at(&palette, col, row) }))
}

/// Makes every pixel outside the job box `region` transparent, which the
/// tracer never traces. A rotated bitmap keeps the pixels under the box's
/// footprint in image space.
fn clip_to_region(
    full: &mut crate::io::bitmap::Bitmap,
    bitmap: &BitmapRef,
    region: [f64; 4],
) -> Result<(), CoreError> {
    let [x0, y0, x1, y1] = region;
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return Err(CoreError::validation(
            "the trace area must be a box in millimeters",
        ));
    }
    let to_image = image_to_job(bitmap, full.width, full.height)?.invert()?;
    let corners =
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| to_image.apply(Point::new(x, y)));
    let min_x = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let (w, h) = (full.width as usize, full.height as usize);
    let alpha = full.alpha.get_or_insert_with(|| vec![255; w * h]);
    for row in 0..h {
        // Pixel row `row` from the top spans y (up) from h - 1 - row to h - row.
        let y = (h - 1 - row) as f64 + 0.5;
        let inside_row = y >= min_y && y <= max_y;
        for col in 0..w {
            let x = col as f64 + 0.5;
            if !(inside_row && x >= min_x && x <= max_x) {
                alpha[row * w + col] = 0;
            }
        }
    }
    Ok(())
}

/// Traces the image of bitmap entity `id` into vectors over the bitmap.
fn trace_entity(
    document: &Document,
    id: u64,
    params: &geom::trace::TraceParams,
    colours: Option<&ColourPick>,
    region: Option<[f64; 4]>,
) -> Result<Vec<geom::Path>, CoreError> {
    let (bitmap, image) = placed_image(document, id)?;
    let mut params = *params;
    let mut full = match colours {
        Some(pick) => {
            if pick.trace.is_empty() {
                return Err(CoreError::validation("pick a color to trace"));
            }
            let colour = crate::io::colours::decode(&image.bytes)?;
            let palette = colour.palette(pick.count);
            params.threshold = 128;
            params.invert = false;
            colour.mask(&palette, &pick.trace)
        }
        None => image.decode()?,
    };
    if let Some(region) = region {
        clip_to_region(&mut full, bitmap, region)?;
    }
    let factor = full.downsample_factor(TRACE_MAX_PIXELS);
    let small = full.downsample(TRACE_MAX_PIXELS);
    params.noise /= f64::from(factor * factor);
    params.smoothness = (params.smoothness / f64::from(factor)).max(0.05);
    // Blocks at the bottom may hang past the image by less than one block;
    // anchor at the top edge, as the file trace does.
    let shift = f64::from(small.height * factor) - f64::from(full.height);
    let placement = geom::trace::Placement {
        origin: Point::new(0.0, -shift),
        pixel_size: f64::from(factor),
    };
    let paths = geom::trace::trace(&small, &params, placement)?;
    if paths.is_empty() {
        return Err(CoreError::validation(
            "nothing in the image passes the threshold; try another threshold or invert",
        ));
    }
    let to_job = image_to_job(bitmap, full.width, full.height)?;
    Ok(paths.iter().map(|p| transform_path(p, &to_job)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma, Rgb};
    use std::io::Cursor;

    fn png_bytes(width: u32, height: u32, dark: impl Fn(u32, u32) -> bool) -> Vec<u8> {
        let buffer = ImageBuffer::from_fn(width, height, |x, y| {
            Luma([if dark(x, y) { 0u8 } else { 255u8 }])
        });
        let mut bytes = Vec::new();
        buffer
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("png");
        bytes
    }

    fn run(doc: &mut Document, name: &str, args: Value) -> Value {
        match dispatch(doc, name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                let envelope: Value = serde_json::from_str(&text).expect("json");
                envelope["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected JSON"),
        }
    }

    fn import_png(doc: &mut Document, bytes: Vec<u8>) -> Value {
        let image = stored_image(bytes, Some("png")).expect("image");
        match import(doc, image, Some("picture".into())).expect("import") {
            KernelReply::Json(text) => {
                serde_json::from_str::<Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected JSON"),
        }
    }

    #[test]
    fn an_imported_bitmap_fits_the_job_centred_and_is_selected() {
        let mut doc = Document::new();
        let reply = import_png(&mut doc, png_bytes(200, 100, |_, _| false));
        let id = reply["id"].as_u64().expect("id");
        assert_eq!(doc.selection(), &[id]);
        let entity = doc.entity(id).expect("entity");
        let EntityKind::Bitmap(bitmap) = &entity.kind else {
            panic!("not a bitmap");
        };
        let job = doc.job_bounds();
        let bbox = entity.bbox().expect("bbox");
        assert!(
            (bbox.width() - job.width()).abs() < 1e-9
                || (bbox.height() - job.height()).abs() < 1e-9
        );
        assert!((bbox.width() / bbox.height() - 2.0).abs() < 1e-9);
        assert!((bbox.centre().x - job.centre().x).abs() < 1e-9);
        assert_eq!(bitmap.opacity, 1.0);
        assert_eq!(entity.name.as_deref(), Some("picture"));
        assert_eq!(doc.images().len(), 1);
    }

    #[test]
    fn the_pixels_come_back_as_the_stored_file() {
        let mut doc = Document::new();
        let bytes = png_bytes(4, 3, |_, _| true);
        let image_id = import_png(&mut doc, bytes.clone())["imageId"]
            .as_u64()
            .expect("id");
        let reply = dispatch(
            &mut doc,
            "bitmaps.pixels",
            &json!({ "imageId": image_id }).to_string(),
        )
        .expect("pixels");
        let KernelReply::Bytes(blob) = reply else {
            panic!("expected bytes");
        };
        let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        let header: Value = serde_json::from_slice(&blob[4..4 + len]).expect("header");
        assert_eq!(header["mime"], "image/png");
        assert_eq!(header["width"], 4);
        assert_eq!(&blob[4 + len..], &bytes[..]);
        assert!(dispatch(&mut doc, "bitmaps.pixels", r#"{"imageId":99}"#).is_err());
    }

    #[test]
    fn images_that_are_not_pictures_are_refused() {
        assert!(matches!(
            stored_image(b"PSD....".to_vec(), Some("psd")),
            Err(CoreError::Unsupported(_))
        ));
        assert!(matches!(
            stored_image(b"GIF89a....".to_vec(), Some("gif")),
            Err(CoreError::Format { .. })
        ));
        let mut bytes = Vec::new();
        ImageBuffer::from_pixel(2, 2, Rgb([1u8, 2, 3]))
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .expect("jpeg");
        let image = stored_image(bytes, None).expect("jpeg sniffed");
        assert_eq!(image.format, "jpg");
    }

    #[test]
    fn gif_is_kept_as_it_is_and_tiff_becomes_png() {
        let pixels = ImageBuffer::from_fn(3, 2, |x, _| Rgb([if x == 0 { 0u8 } else { 255 }, 9, 9]));
        let mut gif = Vec::new();
        pixels
            .write_to(&mut Cursor::new(&mut gif), image::ImageFormat::Gif)
            .expect("gif");
        let image = stored_image(gif.clone(), Some("gif")).expect("gif image");
        assert_eq!((image.format.as_str(), image.mime()), ("gif", "image/gif"));
        assert_eq!(&image.bytes[..], &gif[..]);
        let first = image.decode().expect("decodes");
        assert!(first.luma[0] < first.luma[1]);
        let mut tiff = Vec::new();
        pixels
            .write_to(&mut Cursor::new(&mut tiff), image::ImageFormat::Tiff)
            .expect("tiff");
        let image = stored_image(tiff, Some("tif")).expect("tiff image");
        assert_eq!(image.format, "png");
        assert_eq!((image.width, image.height), (3, 2));
        let back = image.decode().expect("decodes");
        assert_eq!(back.luma[..3], [7, 61, 61]);
    }

    #[test]
    fn fading_a_bitmap_is_one_undo_step() {
        let mut doc = Document::new();
        let id = import_png(&mut doc, png_bytes(2, 2, |_, _| false))["id"]
            .as_u64()
            .expect("id");
        run(
            &mut doc,
            "bitmap.setOpacity",
            json!({ "ids": [id], "opacity": 0.25 }),
        );
        let opacity = |doc: &Document| match &doc.entity(id).expect("entity").kind {
            EntityKind::Bitmap(bitmap) => bitmap.opacity,
            _ => f64::NAN,
        };
        assert_eq!(opacity(&doc), 0.25);
        doc.undo().expect("undo");
        assert_eq!(opacity(&doc), 1.0);
        assert!(dispatch(
            &mut doc,
            "bitmap.setOpacity",
            &json!({ "ids": [id], "opacity": 2 }).to_string()
        )
        .is_err());
    }

    #[test]
    fn a_traced_bitmap_lands_over_the_bitmap_after_moving_and_rotating_it() {
        let mut doc = Document::new();
        // A 20 x 10 image, dark in its left half.
        let id = import_png(&mut doc, png_bytes(20, 10, |x, _| x < 10))["id"]
            .as_u64()
            .expect("id");
        // Place it at 100,50 with 1 mm pixels, then turn it a quarter turn.
        let mut entity = doc.entity(id).expect("entity").clone();
        if let EntityKind::Bitmap(bitmap) = &mut entity.kind {
            bitmap.quad = vec![
                Point::new(100.0, 50.0),
                Point::new(100.0, 70.0),
                Point::new(90.0, 70.0),
                Point::new(90.0, 50.0),
            ];
        }
        doc.replace_entity(id, entity).expect("replace");
        let reply = run(
            &mut doc,
            "bitmap.traceImage",
            json!({ "id": id, "params": { "cornerFit": "lines" } }),
        );
        let ids = reply["ids"].as_array().expect("ids");
        assert_eq!(ids.len(), 1);
        let traced = doc.entity(ids[0].as_u64().expect("id")).expect("traced");
        let bbox = traced.bbox().expect("bbox");
        // The dark half runs along the bitmap's first 10 mm of its turned X axis.
        assert!((bbox.min.x - 90.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.max.x - 100.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.min.y - 50.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.max.y - 60.0).abs() < 1e-6, "{bbox:?}");
        assert_eq!(doc.selection(), &[ids[0].as_u64().expect("id")]);
        // Preview answers the same outline and makes nothing.
        let count = doc.entities().len();
        let revision = doc.revision();
        let preview = run(
            &mut doc,
            "bitmap.traceImage",
            json!({ "id": id, "params": { "cornerFit": "lines" }, "preview": true }),
        );
        assert_eq!(preview["paths"].as_array().expect("paths").len(), 1);
        assert_eq!(doc.entities().len(), count);
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn placed_images_survive_save_and_open_and_undone_ones_are_dropped() {
        let mut doc = Document::new();
        let bytes = png_bytes(3, 2, |x, _| x == 0);
        let reply = import_png(&mut doc, bytes.clone());
        let image_id = reply["imageId"].as_u64().expect("image");
        import_png(&mut doc, png_bytes(5, 5, |_, _| false));
        doc.undo().expect("undo the second import");
        let mut path = std::env::temp_dir();
        path.push(format!("carve-bitmaps-{}.carve", std::process::id()));
        crate::io::native::save(&doc, &path).expect("save");
        let opened = crate::io::native::open(&path, 0).expect("open");
        std::fs::remove_file(&path).ok();
        assert_eq!(opened.images().len(), 1);
        let image = opened.images().get(image_id).expect("image kept");
        assert_eq!(&image.bytes[..], &bytes[..]);
        assert_eq!((image.width, image.height), (3, 2));
        // A new image after opening gets a fresh id.
        let mut opened = opened;
        let next = import_png(&mut opened, png_bytes(1, 1, |_, _| true));
        assert!(next["imageId"].as_u64().expect("id") > image_id);
    }

    #[test]
    fn a_bitmap_is_picked_anywhere_over_its_picture() {
        let mut doc = Document::new();
        let id = import_png(&mut doc, png_bytes(2, 2, |_, _| false))["id"]
            .as_u64()
            .expect("id");
        let centre = doc.job_bounds().centre();
        assert!(bitmap_contains(&doc, id, centre));
        assert!(!bitmap_contains(&doc, id, Point::new(-1000.0, 0.0)));
        let args = super::super::probe::HitTestArgs::Point {
            x: centre.x,
            y: centre.y,
            tolerance: 0.5,
        };
        assert_eq!(super::super::probe::hit_test(&doc, &args), vec![id]);
    }

    #[test]
    fn tracing_needs_a_bitmap() {
        let mut doc = Document::new();
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: geom::rect(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).expect("rect"),
            },
        });
        assert!(dispatch(
            &mut doc,
            "bitmap.traceImage",
            &json!({ "id": id }).to_string()
        )
        .is_err());
    }

    /// A 30 x 10 colour image placed with 1 mm pixels, bottom left at the
    /// origin: red, blue and green thirds from left to right.
    fn colour_bitmap(doc: &mut Document) -> u64 {
        let buffer = ImageBuffer::from_fn(30, 10, |x, _| {
            Rgb(match x {
                0..=9 => [220u8, 20, 20],
                10..=19 => [20, 20, 220],
                _ => [20, 200, 20],
            })
        });
        let mut bytes = Vec::new();
        buffer
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("png");
        let id = import_png(doc, bytes)["id"].as_u64().expect("id");
        let mut entity = doc.entity(id).expect("entity").clone();
        if let EntityKind::Bitmap(bitmap) = &mut entity.kind {
            bitmap.quad = vec![
                Point::new(0.0, 0.0),
                Point::new(30.0, 0.0),
                Point::new(30.0, 10.0),
                Point::new(0.0, 10.0),
            ];
        }
        doc.replace_entity(id, entity).expect("placed");
        id
    }

    #[test]
    fn a_colour_trace_outlines_the_picked_colours() {
        let mut doc = Document::new();
        let id = colour_bitmap(&mut doc);
        let palette = run(
            &mut doc,
            "bitmap.tracePalette",
            json!({ "id": id, "colours": 16 }),
        );
        assert_eq!(palette["colour"], true);
        assert_eq!(palette["colours"].as_array().expect("colours").len(), 3);
        let blue = run(
            &mut doc,
            "bitmap.colourAt",
            json!({ "id": id, "colours": 16, "x": 15.0, "y": 5.0 }),
        )["index"]
            .as_u64()
            .expect("blue index");
        let off = run(
            &mut doc,
            "bitmap.colourAt",
            json!({ "id": id, "colours": 16, "x": 45.0, "y": 5.0 }),
        );
        assert!(off["index"].is_null());
        let KernelReply::Bytes(blob) = dispatch(
            &mut doc,
            "bitmap.traceIndices",
            &json!({ "id": id, "colours": 16 }).to_string(),
        )
        .expect("indices") else {
            panic!("expected bytes");
        };
        let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        let indices = &blob[4 + len..];
        assert_eq!(indices.len(), 300);
        assert_eq!(u64::from(indices[15]), blue);
        let reply = run(
            &mut doc,
            "bitmap.traceImage",
            json!({ "id": id, "params": { "noise": 0 }, "colours": { "count": 16, "trace": [blue] } }),
        );
        let ids = reply["ids"].as_array().expect("ids");
        assert_eq!(ids.len(), 1);
        let bbox = doc
            .entity(ids[0].as_u64().expect("id"))
            .expect("traced")
            .bbox()
            .expect("bbox");
        assert!(
            (bbox.min.x - 10.0).abs() < 0.6 && (bbox.max.x - 20.0).abs() < 0.6,
            "{bbox:?}"
        );
        // Linking red to blue traces them as one area.
        let red = run(
            &mut doc,
            "bitmap.colourAt",
            json!({ "id": id, "colours": 16, "x": 5.0, "y": 5.0 }),
        )["index"]
            .as_u64()
            .expect("red index");
        let reply = run(
            &mut doc,
            "bitmap.traceImage",
            json!({ "id": id, "params": { "noise": 0 }, "colours": { "count": 16, "trace": [red, blue] } }),
        );
        let ids = reply["ids"].as_array().expect("ids");
        assert_eq!(ids.len(), 1);
        let bbox = doc
            .entity(ids[0].as_u64().expect("id"))
            .expect("traced")
            .bbox()
            .expect("bbox");
        assert!(
            (bbox.max.x - 20.0).abs() < 0.6 && bbox.min.x < 0.6,
            "{bbox:?}"
        );
        assert!(dispatch(
            &mut doc,
            "bitmap.traceImage",
            &json!({ "id": id, "colours": { "count": 16, "trace": [] } }).to_string()
        )
        .is_err());
    }

    #[test]
    fn a_trace_area_limits_the_trace_to_the_pixels_under_it() {
        let mut doc = Document::new();
        let id = colour_bitmap(&mut doc);
        let red = run(
            &mut doc,
            "bitmap.colourAt",
            json!({ "id": id, "colours": 16, "x": 5.0, "y": 5.0 }),
        )["index"]
            .as_u64()
            .expect("red");
        let reply = run(
            &mut doc,
            "bitmap.traceImage",
            json!({
                "id": id,
                "params": { "noise": 0 },
                "colours": { "count": 16, "trace": [red] },
                "region": [0.0, 0.0, 5.0, 5.0],
            }),
        );
        let ids = reply["ids"].as_array().expect("ids");
        let bbox = doc
            .entity(ids[0].as_u64().expect("id"))
            .expect("traced")
            .bbox()
            .expect("bbox");
        assert!(bbox.max.x < 5.6 && bbox.max.y < 5.6, "{bbox:?}");
    }

    #[test]
    fn a_placed_bitmap_is_not_exported_as_a_vector() {
        let mut doc = Document::new();
        import_png(&mut doc, png_bytes(20, 10, |_, _| false));
        let path =
            std::env::temp_dir().join(format!("carve-bitmap-export-{}.svg", std::process::id()));
        let error = crate::io::vectors_cmd::dispatch(
            &mut doc,
            None,
            "io.exportVectors",
            &json!({ "path": path.to_string_lossy() }).to_string(),
        )
        .expect_err("a bitmap alone has no vectors");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
        assert!(!path.exists());
    }
}
