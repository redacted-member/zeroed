//! Thumbnails for the clipart library: reliefs drawn shaded, vectors drawn
//! as outlines, each fitted into a square PNG with a clear background.

use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};

use crate::error::CoreError;
use crate::geom::{flatten_path, BBox, Path, Point, Tolerance};
use crate::model::field::Field;

/// Default thumbnail size in pixels.
pub const THUMBNAIL_SIZE: u32 = 256;
/// Largest thumbnail size a caller may ask for.
pub const MAX_THUMBNAIL_SIZE: u32 = 1024;

/// Surface colour of a shaded relief.
const RELIEF_RGB: [f64; 3] = [214.0, 186.0, 146.0];
/// Stroke colour of a vector outline.
const STROKE_RGB: [u8; 3] = [40, 44, 52];

/// Encodes `image` as PNG bytes.
///
/// # Errors
/// [`CoreError::Validation`] if the encoder fails.
pub fn png(image: &RgbaImage) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| {
            CoreError::validation(format!("could not encode the thumbnail: {error}"))
        })?;
    Ok(bytes.into_inner())
}

/// The relief shaded with light from the upper left, fitted into `size`
/// pixels along its longer side. Empty cells are transparent.
pub fn relief(field: &Field, size: u32) -> RgbaImage {
    let (w, h) = (field.width, field.height);
    if w == 0 || h == 0 {
        return RgbaImage::new(size, size);
    }
    let shade = shade_cells(field);
    let per_cell = f64::from(size) / w.max(h) as f64;
    let out_w = ((w as f64 * per_cell).round() as u32).clamp(1, size);
    let out_h = ((h as f64 * per_cell).round() as u32).clamp(1, size);
    let mut image = RgbaImage::new(size, size);
    let (left, top) = ((size - out_w) / 2, (size - out_h) / 2);
    for py in 0..out_h {
        for px in 0..out_w {
            let u = (f64::from(px) + 0.5) / per_cell - 0.5;
            let v = (h as f64 - 1.0) - ((f64::from(py) + 0.5) / per_cell - 0.5);
            let s = sample(&shade, w, h, u, v);
            if s.is_nan() {
                continue;
            }
            let rgb = RELIEF_RGB.map(|c| (c * s).clamp(0.0, 255.0) as u8);
            image.put_pixel(left + px, top + py, Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    image
}

/// Brightness of each cell (NaN where empty), from its surface normal.
fn shade_cells(field: &Field) -> Vec<f64> {
    let (w, h) = (field.width, field.height);
    let lo = f64::from(field.min().unwrap_or(0.0));
    let hi = f64::from(field.max().unwrap_or(0.0));
    let extent = w.max(h) as f64 * field.cell;
    let range = (hi - lo).max(1e-6);
    // Low reliefs are exaggerated so their shape reads at thumbnail size.
    let exaggerate = (extent * 0.12 / range).clamp(1.0, 25.0);
    let light = {
        let (x, y, z) = (-1.0f64, 1.0f64, 1.6f64);
        let n = (x * x + y * y + z * z).sqrt();
        [x / n, y / n, z / n]
    };
    let mut out = vec![f64::NAN; w * h];
    for j in 0..h {
        for i in 0..w {
            let here = field.get(i as i64, j as i64);
            if here.is_nan() {
                continue;
            }
            let at = |di: i64, dj: i64| {
                let v = field.get(i as i64 + di, j as i64 + dj);
                // An empty neighbour is taken as the zero plane, so outlines
                // show as a slope.
                if v.is_nan() {
                    0.0
                } else {
                    f64::from(v)
                }
            };
            let dx = (at(1, 0) - at(-1, 0)) / (2.0 * field.cell) * exaggerate;
            let dy = (at(0, 1) - at(0, -1)) / (2.0 * field.cell) * exaggerate;
            let n = (dx * dx + dy * dy + 1.0).sqrt();
            let lit = (-dx * light[0] - dy * light[1] + light[2]) / n;
            out[j * w + i] = 0.35 + 0.65 * lit.max(0.0);
        }
    }
    out
}

/// Bilinear sample of per-cell values, nearest when a neighbour is empty.
fn sample(values: &[f64], w: usize, h: usize, u: f64, v: f64) -> f64 {
    let u = u.clamp(0.0, w as f64 - 1.0);
    let v = v.clamp(0.0, h as f64 - 1.0);
    let (i0, j0) = (u.floor() as usize, v.floor() as usize);
    let (i1, j1) = ((i0 + 1).min(w - 1), (j0 + 1).min(h - 1));
    let (fu, fv) = (u - i0 as f64, v - j0 as f64);
    let get = |i: usize, j: usize| values[j * w + i];
    let (a, b, c, d) = (get(i0, j0), get(i1, j0), get(i0, j1), get(i1, j1));
    if a.is_nan() || b.is_nan() || c.is_nan() || d.is_nan() {
        let i = if fu < 0.5 { i0 } else { i1 };
        let j = if fv < 0.5 { j0 } else { j1 };
        return get(i, j);
    }
    let top = a + (b - a) * fu;
    let bottom = c + (d - c) * fu;
    top + (bottom - top) * fv
}

/// The outlines of `paths` fitted into `size` pixels with a small margin.
pub fn vectors(paths: &[Path], size: u32) -> RgbaImage {
    let mut image = RgbaImage::new(size, size);
    let tol = Tolerance::new(0.05).unwrap_or(Tolerance::EQUALITY);
    // A closed loop is drawn back to its first point.
    let polylines: Vec<Vec<Point>> = paths
        .iter()
        .flat_map(|p| flatten_path(p, tol))
        .map(|p| {
            let mut points = p.points;
            if p.closed && points.len() > 2 {
                points.push(points[0]);
            }
            points
        })
        .collect();
    let Some(bbox) = polylines
        .iter()
        .filter_map(|p| BBox::of_points(p.iter().copied()))
        .reduce(BBox::union)
    else {
        return image;
    };
    let margin = f64::from(size) * 0.06;
    let usable = f64::from(size) - 2.0 * margin;
    let span = bbox.width().max(bbox.height()).max(1e-9);
    let k = usable / span;
    let centre = bbox.centre();
    let half = f64::from(size) / 2.0;
    let to_px = |p: Point| (half + (p.x - centre.x) * k, half - (p.y - centre.y) * k);
    let line = f64::from(size) / 128.0;
    for points in &polylines {
        for pair in points.windows(2) {
            let (a, b) = (to_px(pair[0]), to_px(pair[1]));
            stroke(&mut image, a, b, line);
        }
    }
    image
}

/// Draws a line of `width` pixels from `a` to `b` by stamping discs.
fn stroke(image: &mut RgbaImage, a: (f64, f64), b: (f64, f64), width: f64) {
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    let steps = (length / 0.5).ceil().max(1.0) as usize;
    let radius = width / 2.0;
    let reach = radius.ceil() as i64 + 1;
    let (iw, ih) = (i64::from(image.width()), i64::from(image.height()));
    for s in 0..=steps {
        let t = s as f64 / steps as f64;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        let (cx, cy) = (x.floor() as i64, y.floor() as i64);
        for py in (cy - reach)..=(cy + reach) {
            for px in (cx - reach)..=(cx + reach) {
                if px < 0 || py < 0 || px >= iw || py >= ih {
                    continue;
                }
                let d = (px as f64 + 0.5 - x).hypot(py as f64 + 0.5 - y);
                let cover = (radius + 0.5 - d).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let pixel = image.get_pixel_mut(px as u32, py as u32);
                let alpha = ((cover * 255.0) as u8).max(pixel.0[3]);
                *pixel = Rgba([STROKE_RGB[0], STROKE_RGB[1], STROKE_RGB[2], alpha]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::SubPath;

    #[test]
    fn a_relief_thumbnail_is_square_and_clear_outside() {
        let mut field = Field::empty(40, 20, 0.5, Point::ORIGIN);
        for j in 0..20 {
            for i in 0..40 {
                field.z[j * 40 + i] = (i as f32 * 0.1).sin() + 1.0;
            }
        }
        let image = relief(&field, THUMBNAIL_SIZE);
        assert_eq!(image.dimensions(), (256, 256));
        // A wide field leaves the top and bottom clear.
        assert_eq!(image.get_pixel(128, 2).0[3], 0);
        assert_eq!(image.get_pixel(128, 128).0[3], 255);
        let bytes = png(&image).expect("encodes");
        assert_eq!(&bytes[1..4], b"PNG");
        let back = image::load_from_memory(&bytes).expect("decodes");
        assert_eq!((back.width(), back.height()), (256, 256));
    }

    #[test]
    fn a_vector_thumbnail_draws_the_outline() {
        let mut sub = SubPath::new(Point::new(0.0, 0.0), Vec::new(), true);
        sub.line_to(Point::new(10.0, 0.0));
        sub.line_to(Point::new(10.0, 10.0));
        let image = vectors(&[Path::from_subpath(sub)], 64);
        assert_eq!(image.dimensions(), (64, 64));
        let inked = image.pixels().filter(|p| p.0[3] > 0).count();
        assert!(inked > 50, "only {inked} pixels drawn");
        // The middle of the triangle's inside stays clear.
        assert_eq!(image.get_pixel(40, 30).0[3], 0);
    }
}
