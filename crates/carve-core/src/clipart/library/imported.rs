//! Items made from outside art that is free to ship: reliefs rendered from
//! CC0 3D models and scans, texture tiles from CC0 displacement maps, and
//! silhouettes from CC0 and CC BY vector sets.
//!
//! The files under `assets/clipart/imported/` hold the converted art, not
//! the originals: reliefs and tiles as 16 bit grayscale PNG height maps (0
//! is no material, a value `v` above 0 is `(v - 1) * step` millimetres),
//! vectors as SVG. Every item records where it came from, who made it, its license
//! and the day it was downloaded (see [`Origin`]); `README.md` next to
//! the files explains how each set was converted.
//!
//! A relief is stored a little coarser than it is cut and brought to the
//! library's [`art::CELL`] with a smooth cubic resample when built, which
//! keeps the repository small without steps a cutter could follow.

use crate::error::CoreError;
use crate::geom::{flatten_path, BBox, Path};
use crate::io::svg;
use crate::model::field::Field;

use super::art::{self, Ring};
use super::Entry;

mod items;

const SOURCE: &str = "imported.rs";

/// Where an item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    /// The item's own page at its source.
    pub url: &'static str,
    /// Who made it, as the source credits them.
    pub author: &'static str,
    /// SPDX style license name: `CC0-1.0`, `CC-BY-3.0`, `CC-BY-4.0` or
    /// `PDM` (the Public Domain Mark).
    pub license: &'static str,
    /// The day it was downloaded, `YYYY-MM-DD`.
    pub downloaded: &'static str,
}

impl Origin {
    /// Whether the license asks for the author to be credited.
    pub fn needs_credit(&self) -> bool {
        self.license.starts_with("CC-BY")
    }
}

/// What an imported file becomes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Form {
    /// A relief stored with `cell` millimetre pixels, each level of the
    /// map `step` millimetres high.
    Relief { step: f64, cell: f64 },
    /// A tile that repeats without a seam, as [`Form::Relief`].
    Tile { step: f64, cell: f64 },
    /// Closed shapes whose box's longer side is `size` millimetres.
    Shape { size: f64 },
}

/// One imported item.
#[derive(Debug)]
pub struct Item {
    /// Category, `/` between levels.
    pub category: &'static str,
    /// Name shown under the thumbnail.
    pub name: &'static str,
    /// Search words.
    pub tags: &'static [&'static str],
    /// What the file becomes.
    pub form: Form,
    /// The converted file's bytes.
    pub data: &'static [u8],
    /// Where it came from.
    pub origin: Origin,
}

/// Adds every imported item to the catalogue.
pub(super) fn entries(out: &mut Vec<Entry>) {
    for item in items::ITEMS {
        let mut entry = match item.form {
            Form::Relief { step, cell } => {
                Entry::relief(item.category, item.name, item.tags, SOURCE, move || {
                    relief(item.data, step, cell, false)
                })
            }
            Form::Tile { step, cell } => {
                Entry::tile(item.category, item.name, item.tags, SOURCE, move || {
                    relief(item.data, step, cell, true)
                })
            }
            Form::Shape { size } => {
                Entry::vectors(item.category, item.name, item.tags, SOURCE, move || {
                    shapes(item.data, size)
                })
            }
        };
        entry.origin = Some(&item.origin);
        out.push(entry);
    }
}

/// Every imported item.
pub fn items() -> &'static [Item] {
    items::ITEMS
}

/// Heights from a 16 bit PNG height map: rows from the lowest Y up, NaN
/// where the map holds 0.
fn heights(data: &[u8], step: f64) -> Result<(usize, usize, Vec<f32>), CoreError> {
    let image = image::load_from_memory_with_format(data, image::ImageFormat::Png)
        .map_err(|e| CoreError::validation(format!("the height map cannot be read: {e}")))?
        .into_luma16();
    let (w, h) = (image.width() as usize, image.height() as usize);
    let raw = image.into_raw();
    let mut z = vec![f32::NAN; w * h];
    for j in 0..h {
        let row = &raw[(h - 1 - j) * w..(h - j) * w];
        for (i, &v) in row.iter().enumerate() {
            if v > 0 {
                z[j * w + i] = ((f64::from(v) - 1.0) * step) as f32;
            }
        }
    }
    Ok((w, h, z))
}

/// Catmull-Rom weights for a sample `t` of the way from the second of four
/// points to the third.
fn weights(t: f64) -> [f64; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// The stored height map brought to the library's cell size. A relief's
/// empty cells stay empty (a cell is material when most of the stored
/// cells round it are); a tile reads across its edges from the opposite
/// ones so it still repeats.
fn relief(data: &[u8], step: f64, cell: f64, tile: bool) -> Result<Field, CoreError> {
    use rayon::prelude::*;
    let (w, h, z) = heights(data, step)?;
    let k = cell / art::CELL;
    let (nw, nh) = (
        ((w as f64) * k).round().max(1.0) as usize,
        ((h as f64) * k).round().max(1.0) as usize,
    );
    let at = |i: i64, j: i64| -> f32 {
        let (i, j) = if tile {
            (i.rem_euclid(w as i64), j.rem_euclid(h as i64))
        } else {
            (i.clamp(0, w as i64 - 1), j.clamp(0, h as i64 - 1))
        };
        z[j as usize * w + i as usize]
    };
    let mut out = vec![f32::NAN; nw * nh];
    out.par_chunks_mut(nw).enumerate().for_each(|(j, row)| {
        let v = (j as f64 + 0.5) * (h as f64 / nh as f64) - 0.5;
        let j0 = v.floor() as i64;
        let wy = weights(v - j0 as f64);
        for (i, cell_z) in row.iter_mut().enumerate() {
            let u = (i as f64 + 0.5) * (w as f64 / nw as f64) - 0.5;
            let i0 = u.floor() as i64;
            let wx = weights(u - i0 as f64);
            let (mut sum, mut inside) = (0.0f64, 0.0f64);
            for (b, wyb) in wy.iter().enumerate() {
                for (a, wxa) in wx.iter().enumerate() {
                    let s = at(i0 - 1 + a as i64, j0 - 1 + b as i64);
                    let wt = wxa * wyb;
                    if s.is_nan() {
                        continue;
                    }
                    sum += wt * f64::from(s);
                    // Only the middle four samples decide whether the cell
                    // holds material, as a bilinear read would.
                    if (1..=2).contains(&a) && (1..=2).contains(&b) {
                        let fx = if a == 1 {
                            1.0 - (u - i0 as f64)
                        } else {
                            u - i0 as f64
                        };
                        let fy = if b == 1 {
                            1.0 - (v - j0 as f64)
                        } else {
                            v - j0 as f64
                        };
                        inside += fx * fy;
                    }
                }
            }
            if inside > 0.5 && sum > 0.0 {
                *cell_z = sum as f32;
            } else if tile {
                *cell_z = sum.max(0.0) as f32;
            }
        }
    });
    let origin = if tile {
        art::p(art::CELL / 2.0, art::CELL / 2.0)
    } else {
        art::p(
            -(((nw - 1) / 2) as f64) * art::CELL,
            -(((nh - 1) / 2) as f64) * art::CELL,
        )
    };
    let mut field = Field::empty(nw, nh, art::CELL, origin);
    field.z = out;
    if tile {
        return Ok(field);
    }
    field
        .cropped()
        .ok_or_else(|| CoreError::validation("the height map holds no material"))
}

/// Closed shapes from SVG text, filled as the file fills them, welded into
/// outlines that do not overlap and scaled so the longer side is `size`.
fn shapes(data: &[u8], size: f64) -> Result<Vec<Path>, CoreError> {
    let text =
        std::str::from_utf8(data).map_err(|_| CoreError::validation("the drawing is not text"))?;
    let (first, _) = svg::import(text, 1.0)?;
    let bbox = first
        .iter()
        .flat_map(|p| flatten_path(&p.path, art::tol()))
        .filter_map(|l| BBox::of_points(l.points))
        .reduce(BBox::union)
        .ok_or_else(|| CoreError::validation("the drawing is empty"))?;
    let k = size / bbox.width().max(bbox.height());
    let (read, _) = svg::import(text, k)?;
    let mut rings: Vec<Ring> = Vec::new();
    for imported in &read {
        for line in flatten_path(&imported.path, art::tol()) {
            let mut pts = line.points;
            if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 1e-9 {
                pts.pop();
            }
            if pts.len() >= 3 {
                rings.push(pts);
            }
        }
    }
    let even_odd = text.contains("evenodd");
    if even_odd {
        rings = by_parity(&rings);
    }
    let welded = art::merge(&[rings])?;
    // Specks too small to cut are left out.
    let kept: Vec<Ring> = welded
        .into_iter()
        .filter(|r| art::signed_area2(r).abs() / 2.0 >= MIN_AREA)
        .collect();
    let kept = art::centred(&kept);
    kept.iter()
        .map(|ring| {
            lean(ring).ok_or_else(|| CoreError::validation("an outline cannot be fitted cleanly"))
        })
        .collect()
}

/// The ring as one clean closed path of lines and arcs with at most
/// [`MAX_SEGMENTS`], its points thinned a little more each try until the
/// fit is short enough and valid.
fn lean(ring: &Ring) -> Option<Path> {
    use crate::geom::contain::validate_set;
    use crate::geom::Tolerance;
    let check = Tolerance::new(0.001).ok()?;
    for thin in [0.0, 0.02, 0.04, 0.08, 0.12, 0.2] {
        let pts = if thin > 0.0 {
            thinned(ring, thin)
        } else {
            ring.clone()
        };
        if pts.len() < 3 {
            return None;
        }
        let Some(path) = art::paths(&[pts]).into_iter().next() else {
            continue;
        };
        let segments: usize = path.subpaths.iter().map(|s| s.segments.len()).sum();
        if segments <= MAX_SEGMENTS && validate_set(std::slice::from_ref(&path), check).is_empty() {
            return Some(path);
        }
    }
    None
}

/// Most segments a loop may have.
const MAX_SEGMENTS: usize = 380;

/// Douglas-Peucker thinning of a closed ring to within `tol` millimetres.
fn thinned(ring: &Ring, tol: f64) -> Ring {
    fn dp(pts: &[crate::geom::Point], tol: f64, out: &mut Vec<crate::geom::Point>) {
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        let mut far = (0.0, 0);
        for (k, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
            let d = if len > 1e-12 {
                ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
            } else {
                p.distance(a)
            };
            if d > far.0 {
                far = (d, k);
            }
        }
        if far.0 > tol {
            dp(&pts[..=far.1], tol, out);
            out.pop();
            dp(&pts[far.1..], tol, out);
        } else {
            out.push(a);
            out.push(b);
        }
    }
    // Split at the point farthest from the first so both halves are open.
    let n = ring.len();
    let far = (1..n)
        .max_by(|&i, &j| {
            ring[0]
                .distance(ring[i])
                .total_cmp(&ring[0].distance(ring[j]))
        })
        .unwrap_or(0);
    let first: Vec<_> = ring[..=far].to_vec();
    let mut second: Vec<_> = ring[far..].to_vec();
    second.push(ring[0]);
    let mut out = Vec::new();
    dp(&first, tol, &mut out);
    out.pop();
    let mut rest = Vec::new();
    dp(&second, tol, &mut rest);
    rest.pop();
    out.extend(rest);
    out
}

/// Smallest loop kept, square millimetres.
const MIN_AREA: f64 = 0.4;

/// Rings turned so the even-odd fill reads the same under the nonzero
/// rule: a ring inside an even number of others runs counter clockwise,
/// one inside an odd number clockwise.
fn by_parity(rings: &[Ring]) -> Vec<Ring> {
    rings
        .iter()
        .enumerate()
        .map(|(k, ring)| {
            let probe = ring[0];
            let depth = rings
                .iter()
                .enumerate()
                .filter(|(m, other)| *m != k && contains(other, probe))
                .count();
            let ccw = art::ccw(ring);
            if depth % 2 == 0 {
                ccw
            } else {
                ccw.into_iter().rev().collect()
            }
        })
        .collect()
}

/// Whether `p` lies inside the closed ring, by crossings.
fn contains(ring: &[crate::geom::Point], p: crate::geom::Point) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x > p.x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_imported_file_is_listed_once() {
        // Building each item is covered by the library's own tests; this
        // one keeps the folder and the table in step.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/clipart/imported");
        let mut on_disk = Vec::new();
        let mut stack = vec![dir];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).expect("reads the folder").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("png" | "svg")
                ) {
                    on_disk.push(std::fs::read(&path).expect("reads the file").len());
                }
            }
        }
        assert_eq!(on_disk.len(), items().len());
        let mut sizes: Vec<usize> = items().iter().map(|i| i.data.len()).collect();
        sizes.sort_unstable();
        on_disk.sort_unstable();
        assert_eq!(sizes, on_disk);
    }

    #[test]
    fn no_shipped_item_needs_a_credit_the_app_does_not_show() {
        assert!(items().iter().all(|i| !i.origin.needs_credit()));
    }
}
