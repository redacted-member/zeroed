//! The `model.fromBitmap` command: a relief component from a greyscale
//! image file, or from a bitmap placed in the 2D view (`id`), as one undo
//! step. A placed bitmap makes the component exactly under itself, however
//! it was moved, stretched or turned.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use crate::doc::bitmap_cmd::image_to_job;
use crate::doc::{Document, EntityKind};
use crate::error::CoreError;
use crate::geom::{Mat3, Point};
use crate::io;
use crate::kernel::KernelReply;
use crate::model::bitmap_relief::{
    relief_from_bitmap, relief_from_placed, BitmapReliefParams, Placement,
};
use crate::model::{grid_for, Combine, Component};

/// Images larger than this are averaged down first.
const MAX_PIXELS: u64 = 4_000_000;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "model.fromBitmap"
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FromBitmapArgs {
    /// The image file; not needed with `id`.
    #[serde(default)]
    path: Option<PathBuf>,
    /// A bitmap entity in the 2D view to make the component from, in
    /// place of a file; placement fields are then ignored.
    #[serde(default)]
    id: Option<u64>,
    /// Bottom left corner of the image in the job, mm; centred when absent.
    #[serde(default)]
    origin: Option<Point>,
    /// Millimetres per pixel; wins over `width`.
    #[serde(default)]
    pixel_size: Option<f64>,
    /// Width of the image in the job, mm; the image fits the job when
    /// neither this nor `pixelSize` is given.
    #[serde(default)]
    width: Option<f64>,
    #[serde(flatten)]
    params: BitmapReliefParams,
    #[serde(default)]
    combine: Combine,
    #[serde(default)]
    name: Option<String>,
}

/// Runs a bitmap relief command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, the errors
/// of [`io::bitmap::read`] and [`relief_from_bitmap`], and
/// [`CoreError::Validation`] for a relief that covers no cell.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let args: FromBitmapArgs =
        serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
    if let Some(id) = args.id {
        return from_placed(document, id, &args);
    }
    let Some(path) = &args.path else {
        return Err(CoreError::validation(
            "select a bitmap or pick an image file",
        ));
    };
    let full = io::bitmap::read(path)?;
    let (width, height) = (f64::from(full.width), f64::from(full.height));
    if width == 0.0 || height == 0.0 {
        return Err(CoreError::validation("the image has no pixels"));
    }
    let job = document.job_bounds();
    let pixel_size = match (args.pixel_size, args.width) {
        (Some(size), _) => size,
        (None, Some(w)) => w / width,
        (None, None) => (job.width() / width).min(job.height() / height),
    };
    let origin = args.origin.unwrap_or_else(|| {
        let centre = job.centre();
        Point::new(
            centre.x - width * pixel_size / 2.0,
            centre.y - height * pixel_size / 2.0,
        )
    });
    // A downsampled image keeps its size in the job; the blocks the last
    // row and column are cut into hang past the edge by less than a block,
    // so anchor at the top left corner.
    let factor = full.downsample_factor(MAX_PIXELS);
    let bitmap = full.downsample(MAX_PIXELS);
    let block = pixel_size * f64::from(factor);
    let placement = Placement {
        origin: Point::new(
            origin.x,
            origin.y + height * pixel_size - f64::from(bitmap.height) * block,
        ),
        pixel_size: block,
    };
    let grid = grid_for(document.job());
    let field = relief_from_bitmap(&bitmap, placement, &grid, &args.params)?
        .cropped()
        .ok_or_else(|| CoreError::validation("the image leaves every cell empty"))?;
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Bitmap")
        .to_string();
    add(document, &args, field, fallback)
}

/// Adds `field` as the new component, named by the arguments or else
/// `fallback`.
fn add(
    document: &mut Document,
    args: &FromBitmapArgs,
    field: crate::model::field::Field,
    fallback: String,
) -> Result<KernelReply, CoreError> {
    let component_name = args
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(fallback);
    document.begin("Component from Bitmap");
    let id = document.add_component(Component {
        id: 0,
        name: component_name,
        visible: true,
        combine: args.combine,
        height_scale: 1.0,
        level: 0,
        extra: Default::default(),
        field,
    });
    let revision = document.commit();
    KernelReply::ok(&json!({ "id": id, "revision": revision }))
}

/// The component of the bitmap entity `id`, laid exactly under it.
fn from_placed(
    document: &mut Document,
    id: u64,
    args: &FromBitmapArgs,
) -> Result<KernelReply, CoreError> {
    let entity = document.entity(id)?;
    let EntityKind::Bitmap(bitmap) = &entity.kind else {
        return Err(CoreError::validation(
            "select a bitmap to make the component from",
        ));
    };
    let image = document
        .images()
        .get(bitmap.image_id)
        .ok_or_else(|| CoreError::validation("the bitmap's image is missing"))?;
    let full = image.decode()?;
    let to_job = image_to_job(bitmap, full.width, full.height)?;
    // A downsampled image keeps its place: each block covers `factor`
    // pixels, anchored at the top left as for a file.
    let factor = f64::from(full.downsample_factor(MAX_PIXELS));
    let small = full.downsample(MAX_PIXELS);
    let shift = f64::from(full.height) - factor * f64::from(small.height);
    let to_job = Mat3::new(factor, 0.0, 0.0, factor, 0.0, shift).then(&to_job);
    let grid = grid_for(document.job());
    let field = relief_from_placed(&small, to_job, &grid, &args.params)?
        .cropped()
        .ok_or_else(|| CoreError::validation("the image leaves every cell empty"))?;
    let fallback = entity.name.clone().unwrap_or_else(|| "Bitmap".to_string());
    add(document, args, field, fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kernel;
    use serde_json::Value;

    fn write_png(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("ramp.png");
        let mut image = image::GrayImage::new(40, 20);
        for (x, _y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Luma([(x * 255 / 39) as u8]);
        }
        image.save(&path).expect("save png");
        path
    }

    fn result(reply: KernelReply) -> Value {
        match reply {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("json");
                value["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected json"),
        }
    }

    #[test]
    fn from_bitmap_adds_a_component_in_one_undo_step() {
        let dir = std::env::temp_dir().join(format!("carve-bitmap-relief-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = write_png(&dir);
        let mut kernel = Kernel::new();
        let args = json!({
            "path": path,
            "width": 100.0,
            "height": 8.0,
            "baseHeight": 2.0,
            "smoothing": 0,
        })
        .to_string();
        let reply = result(
            kernel
                .apply("model.fromBitmap", &args)
                .expect("from bitmap"),
        );
        let id = reply["id"].as_u64().expect("id");
        let component = kernel.document().component(id).expect("component");
        assert_eq!(component.name, "ramp");
        let max = component.field.max().expect("max");
        let min = component.field.min().expect("min");
        assert!((f64::from(max) - 10.0).abs() < 0.3, "max {max}");
        assert!((f64::from(min) - 2.0).abs() < 0.3, "min {min}");
        let span = component.field.width as f64 * component.field.cell;
        assert!((span - 100.0).abs() < 1.0, "width {span}");
        assert_eq!(
            kernel.document_mut().undo().as_deref(),
            Some("Component from Bitmap")
        );
        assert!(kernel.document().components().is_empty());
        let missing = json!({ "path": dir.join("none.png") }).to_string();
        assert!(kernel.apply("model.fromBitmap", &missing).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_selected_bitmap_becomes_a_component_under_itself() {
        let dir = std::env::temp_dir().join(format!("carve-bitmap-placed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = write_png(&dir);
        let mut kernel = Kernel::new();
        let placed = result(
            kernel
                .apply("io.importBitmap", &json!({ "path": path }).to_string())
                .expect("import"),
        );
        let bitmap = placed["id"].as_u64().expect("bitmap id");
        let EntityKind::Bitmap(entity) = &kernel.document().entity(bitmap).expect("bitmap").kind
        else {
            panic!("not a bitmap");
        };
        let (low, high) = (entity.quad[0], entity.quad[2]);
        let args = json!({ "id": bitmap, "height": 5.0, "smoothing": 0 }).to_string();
        let reply = result(
            kernel
                .apply("model.fromBitmap", &args)
                .expect("from bitmap"),
        );
        let id = reply["id"].as_u64().expect("id");
        let field = &kernel.document().component(id).expect("component").field;
        let cell = field.cell;
        let left = field.origin.x - cell / 2.0;
        let right = left + field.width as f64 * cell;
        let bottom = field.origin.y - cell / 2.0;
        let top = bottom + field.height as f64 * cell;
        assert!((left - low.x).abs() <= cell, "{left} {}", low.x);
        assert!((right - high.x).abs() <= cell, "{right} {}", high.x);
        assert!((bottom - low.y).abs() <= cell && (top - high.y).abs() <= cell);
        // Light is high: the ramp rises to the right.
        let y = (low.y + high.y) / 2.0;
        assert!(field.at(Point::new(high.x - 2.0, y)) > field.at(Point::new(low.x + 2.0, y)));
        let not_bitmap = json!({ "id": id + 1000 }).to_string();
        assert!(kernel.apply("model.fromBitmap", &not_bitmap).is_err());
        assert!(kernel.apply("model.fromBitmap", "{}").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_from_bitmap_is_handled() {
        assert!(handles("model.fromBitmap"));
        assert!(!handles("model.list"));
        let mut doc = Document::new();
        assert!(dispatch(&mut doc, "model.list", "{}").is_err());
    }
}
