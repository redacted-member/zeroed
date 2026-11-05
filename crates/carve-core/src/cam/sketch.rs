//! Sketch Carving and Laser Sketch: toolpaths that pick out the key lines
//! of a bitmap or of the 3D model, the way a pencil sketch does, and carve
//! them with a V shaped tool or burn them with a laser.
//!
//! Both start from a tone grid, darkness from 0 (white) to 1 (black) on
//! square cells in job space. A bitmap gives its grey levels; the model is
//! lit from the upper left so its shape becomes shading. The sketch lines
//! are where the grid is darker than its surroundings: the tone blurred a
//! little minus the tone blurred more (a difference of Gaussians). The
//! line thickness, 0 to 100, sets how wide both blurs are, so a higher
//! value gives heavier lines and less detail. The inked cells, limited to
//! the boundary, are traced into closed vectors.
//!
//! Sketch Carving V-Grooves those vectors, from the start depth and flat at
//! the flat depth when one is set. Laser Sketch hatch fills them at the
//! surface with the laser's power.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::doc::bitmap_cmd::image_to_job;
use crate::doc::{Document, EntityKind, JobSetup};
use crate::error::CoreError;
use crate::geom::trace::{trace as trace_bitmap, FitMode, Placement, TraceParams};
use crate::geom::{flatten_subpath, path_bbox, Mat3, Path, Point, Region, Tolerance};
use crate::io::bitmap::Bitmap;
use crate::model::field::Field;

use super::common::{order_nearest, Heights};
use super::laser::{speed_of, tool_power_speed, Beam};
use super::pocket::raster_lines;
use super::profile::Calculated;
use super::tool::{Tool, ToolGeometry};
use super::vgroove::{vgroove, VGrooveParams, FLATS_NEED_A_POCKET};

/// Most cells along the longer side of a bitmap's tone grid.
pub const MAX_SIDE: usize = 800;

/// How much darker than its surroundings a cell must be to be a line.
const INK_LEVEL: f32 = 0.035;

/// Where the sketch comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SketchSource {
    /// An image file placed on the job.
    #[default]
    Bitmap,
    /// The composite 3D model.
    Model,
}

/// What limits the sketch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SketchBoundary {
    /// The edge of the bitmap, or of the model's components.
    #[default]
    Source,
    /// The selected closed vectors.
    Vector,
    /// The outline of the components on one model level.
    Level,
}

/// How Laser Sketch travels between the areas it fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SketchFill {
    /// Every line crosses the whole sketch, unpowered over the gaps.
    Together,
    /// Each area is filled on its own, travelling between areas.
    #[default]
    Regionally,
    /// As regionally, with every travel kept to the areas' own lines.
    RegionallyWithinVectors,
}

/// Settings both sketch toolpaths share. Lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SketchTrace {
    /// Bitmap or model.
    pub source: SketchSource,
    /// A bitmap placed in the job (entity id); when absent the image file
    /// below is used.
    pub bitmap: Option<u64>,
    /// The image file, for a bitmap that is not placed in the job.
    pub path: PathBuf,
    /// Lower left corner of the bitmap on the job.
    pub origin: Point,
    /// Width of the bitmap on the job.
    pub width: f64,
    /// Height of the bitmap; zero keeps the image's proportions.
    pub height: f64,
    /// The machining limit.
    pub boundary: SketchBoundary,
    /// The model level whose components limit the sketch, for
    /// [`SketchBoundary::Level`].
    pub level: Option<u64>,
    /// How far outside the boundary (whichever it is) the sketch may go.
    pub boundary_offset: f64,
    /// 0 (fine) to 100 (heavy lines, less detail).
    pub line_thickness: f64,
}

impl Default for SketchTrace {
    fn default() -> Self {
        SketchTrace {
            source: SketchSource::Bitmap,
            bitmap: None,
            path: PathBuf::new(),
            origin: Point::new(0.0, 0.0),
            width: 100.0,
            height: 0.0,
            boundary: SketchBoundary::Source,
            level: None,
            boundary_offset: 0.0,
            line_thickness: 50.0,
        }
    }
}

/// Settings of Sketch Carving.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SketchCarveParams {
    /// Source, boundary and line thickness.
    #[serde(flatten)]
    pub trace: SketchTrace,
    /// Depth below the surface the carving starts at.
    pub start_depth: f64,
    /// Deepest the tool goes below the start depth; wider lines are flat
    /// bottomed there.
    pub flat_depth: Option<f64>,
}

/// Settings of Laser Sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaserSketchParams {
    /// Source, boundary and line thickness.
    #[serde(flatten)]
    pub trace: SketchTrace,
    /// Power in percent; the tool's when absent.
    pub power: Option<f64>,
    /// Speed in millimetres per minute; the tool's when absent.
    pub speed: Option<f64>,
    /// Unpowered run past each end of a hatch line.
    pub overscan: f64,
    /// Distance between hatch lines.
    pub stepover: f64,
    /// Hatch angle in degrees from the X axis.
    pub angle: f64,
    /// Hatch again at right angles.
    pub cross_hatch: bool,
    /// Together or area by area.
    pub fill: SketchFill,
}

impl Default for LaserSketchParams {
    fn default() -> Self {
        LaserSketchParams {
            trace: SketchTrace::default(),
            power: None,
            speed: None,
            overscan: 0.0,
            stepover: 0.1,
            angle: 0.0,
            cross_hatch: false,
            fill: SketchFill::Regionally,
        }
    }
}

/// Darkness on square cells. Row 0 is the bottom row.
#[derive(Debug, Clone, PartialEq)]
pub struct ToneGrid {
    /// Cells across.
    pub cols: usize,
    /// Cells up.
    pub rows: usize,
    /// Cell size in millimetres.
    pub cell: f64,
    /// Job position of the grid's lower left corner.
    pub origin: Point,
    /// Darkness per cell, 0 to 1, row by row from the bottom.
    pub dark: Vec<f32>,
    /// Cells the source covers; the rest are never inked.
    pub inside: Vec<bool>,
}

impl ToneGrid {
    fn centre(&self, i: usize, j: usize) -> Point {
        Point::new(
            self.origin.x + (i as f64 + 0.5) * self.cell,
            self.origin.y + (j as f64 + 0.5) * self.cell,
        )
    }
}

/// Whether a request's parameters sketch the model, so the toolpath goes
/// out of date when the model changes.
pub fn uses_model(params: &Value) -> bool {
    params.get("source").and_then(Value::as_str) == Some("model")
}

/// The tone of a bitmap placed with its lower left corner at `origin`,
/// `width` wide and `height` high (zero keeps its proportions).
///
/// # Errors
/// [`CoreError::Validation`] for an empty image or a size that is not
/// positive.
pub fn bitmap_tone(
    image: &Bitmap,
    origin: Point,
    width: f64,
    height: f64,
) -> Result<ToneGrid, CoreError> {
    image.check()?;
    if image.width == 0 || image.height == 0 {
        return Err(CoreError::validation("the image has no pixels"));
    }
    let height = if height > 0.0 {
        height
    } else {
        width * f64::from(image.height) / f64::from(image.width)
    };
    if !(width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0) {
        return Err(CoreError::validation(
            "the bitmap size must be greater than zero",
        ));
    }
    if !(origin.x.is_finite() && origin.y.is_finite()) {
        return Err(CoreError::validation("the bitmap position is not a number"));
    }
    let (iw, ih) = (image.width as usize, image.height as usize);
    let longest = iw.max(ih);
    let scale = (MAX_SIDE as f64 / longest as f64).min(1.0);
    let cols = ((iw as f64 * scale).round() as usize).max(1);
    let cell = width / cols as f64;
    let rows = ((height / cell).round() as usize).max(1);
    let mut dark = vec![0.0f32; cols * rows];
    for j in 0..rows {
        // Image rows count from the top.
        let v0 = 1.0 - (j + 1) as f64 / rows as f64;
        let v1 = 1.0 - j as f64 / rows as f64;
        let y0 = ((v0 * ih as f64).floor() as usize).min(ih - 1);
        let y1 = ((v1 * ih as f64).ceil() as usize).clamp(y0 + 1, ih);
        for i in 0..cols {
            let x0 = ((i as f64 / cols as f64 * iw as f64).floor() as usize).min(iw - 1);
            let x1 = (((i + 1) as f64 / cols as f64 * iw as f64).ceil() as usize).clamp(x0 + 1, iw);
            let mut sum = 0.0f64;
            let mut n = 0.0f64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let k = y * iw + x;
                    let opaque = image
                        .alpha
                        .as_ref()
                        .map_or(1.0, |a| f64::from(a[k]) / 255.0);
                    let luma = f64::from(image.luma[k]) / 255.0;
                    // Transparent pixels show white.
                    sum += 1.0 - (luma * opaque + (1.0 - opaque));
                    n += 1.0;
                }
            }
            dark[j * cols + i] = (sum / n.max(1.0)) as f32;
        }
    }
    Ok(ToneGrid {
        cols,
        rows,
        cell,
        origin,
        dark,
        inside: vec![true; cols * rows],
    })
}

/// The tone of an image placed by `to_job`, the transform from its pixels
/// (origin bottom left, Y up) to the job, which may rotate or skew it. The
/// grid covers the box round the placed image; cells off the image are
/// outside the source.
///
/// # Errors
/// [`CoreError::Validation`] for an empty image or a placement that has no
/// area.
pub fn placed_tone(image: &Bitmap, to_job: &Mat3) -> Result<ToneGrid, CoreError> {
    image.check()?;
    if image.width == 0 || image.height == 0 {
        return Err(CoreError::validation("the image has no pixels"));
    }
    let limit = (MAX_SIDE * MAX_SIDE) as u64;
    let factor = f64::from(image.downsample_factor(limit));
    let small = image.downsample(limit);
    let (fw, fh) = (f64::from(image.width), f64::from(image.height));
    let corners = [
        to_job.apply(Point::new(0.0, 0.0)),
        to_job.apply(Point::new(fw, 0.0)),
        to_job.apply(Point::new(fw, fh)),
        to_job.apply(Point::new(0.0, fh)),
    ];
    let (mut lo, mut hi) = (corners[0], corners[0]);
    for c in &corners[1..] {
        lo = Point::new(lo.x.min(c.x), lo.y.min(c.y));
        hi = Point::new(hi.x.max(c.x), hi.y.max(c.y));
    }
    let back = to_job.invert()?;
    let (bw, bh) = (hi.x - lo.x, hi.y - lo.y);
    let side = f64::from(small.width.max(small.height));
    let cell = bw.max(bh) / side;
    if !(cell.is_finite() && cell > 0.0) {
        return Err(CoreError::validation("the bitmap has no area"));
    }
    let cols = ((bw / cell).ceil() as usize).max(1);
    let rows = ((bh / cell).ceil() as usize).max(1);
    let (sw, sh) = (small.width as usize, small.height as usize);
    let mut dark = vec![0.0f32; cols * rows];
    let mut inside = vec![false; cols * rows];
    for j in 0..rows {
        for i in 0..cols {
            let at = Point::new(
                lo.x + (i as f64 + 0.5) * cell,
                lo.y + (j as f64 + 0.5) * cell,
            );
            let p = back.apply(at);
            if !(p.x >= 0.0 && p.x < fw && p.y >= 0.0 && p.y < fh) {
                continue;
            }
            // Downsampled blocks are anchored at the image's top edge.
            let sx = ((p.x / factor) as usize).min(sw - 1);
            let sy = (((fh - p.y) / factor) as usize).min(sh - 1);
            let k = sy * sw + sx;
            let opaque = small
                .alpha
                .as_ref()
                .map_or(1.0, |a| f64::from(a[k]) / 255.0);
            let luma = f64::from(small.luma[k]) / 255.0;
            dark[j * cols + i] = (1.0 - (luma * opaque + (1.0 - opaque))) as f32;
            inside[j * cols + i] = true;
        }
    }
    Ok(ToneGrid {
        cols,
        rows,
        cell,
        origin: lo,
        dark,
        inside,
    })
}

/// The image and placement of bitmap entity `id`, with its quad outline.
pub(crate) fn placed_bitmap(
    doc: &Document,
    id: u64,
) -> Result<(ToneGrid, Option<Path>), CoreError> {
    let entity = doc.entity(id)?;
    let EntityKind::Bitmap(bitmap) = &entity.kind else {
        return Err(CoreError::validation("select a bitmap to sketch"));
    };
    let image = doc
        .images()
        .get(bitmap.image_id)
        .ok_or_else(|| CoreError::validation("the bitmap's image is missing"))?
        .decode()?;
    let to_job = image_to_job(bitmap, image.width, image.height)?;
    Ok((placed_tone(&image, &to_job)?, entity.path()))
}

/// The tone of the model: its surface lit from the upper left, flat areas
/// white and slopes facing away from the light dark. Cells with no model
/// are white and outside the source, except a rim of two cells round the
/// model so its silhouette can be drawn.
///
/// # Errors
/// [`CoreError::Validation`] when the model is empty.
pub fn model_tone(field: &Field) -> Result<ToneGrid, CoreError> {
    let (cols, rows) = (field.width, field.height);
    let covered: Vec<bool> = field.z.iter().map(|h| !h.is_nan() && *h != 0.0).collect();
    if cols < 2 || rows < 2 || !covered.iter().any(|c| *c) {
        return Err(CoreError::validation(
            "there is no model to sketch: create a shape in the Modeling tab first",
        ));
    }
    let h = |i: usize, j: usize| -> f64 {
        let z = field.z[j * cols + i];
        if z.is_nan() {
            0.0
        } else {
            f64::from(z)
        }
    };
    let cell = field.cell;
    let light = {
        let (x, y, z) = (-1.0f64, 1.0f64, 1.0f64);
        let n = (x * x + y * y + z * z).sqrt();
        (x / n, y / n, z / n)
    };
    let mut dark = vec![0.0f32; cols * rows];
    for j in 0..rows {
        for i in 0..cols {
            let (il, ir) = (i.saturating_sub(1), (i + 1).min(cols - 1));
            let (jd, ju) = (j.saturating_sub(1), (j + 1).min(rows - 1));
            let dx = (h(ir, j) - h(il, j)) / (cell * (ir - il).max(1) as f64);
            let dy = (h(i, ju) - h(i, jd)) / (cell * (ju - jd).max(1) as f64);
            let len = (dx * dx + dy * dy + 1.0).sqrt();
            let lit = (-dx * light.0 - dy * light.1 + light.2) / len;
            let brightness = (lit / light.2).clamp(0.0, 1.0);
            dark[j * cols + i] = (1.0 - brightness) as f32;
        }
    }
    let mut inside = vec![false; cols * rows];
    const RIM: usize = 2;
    for j in 0..rows {
        for i in 0..cols {
            if !covered[j * cols + i] {
                continue;
            }
            for jj in j.saturating_sub(RIM)..=(j + RIM).min(rows - 1) {
                for ii in i.saturating_sub(RIM)..=(i + RIM).min(cols - 1) {
                    inside[jj * cols + ii] = true;
                }
            }
        }
    }
    Ok(ToneGrid {
        cols,
        rows,
        cell,
        origin: Point::new(field.origin.x - cell / 2.0, field.origin.y - cell / 2.0),
        dark,
        inside,
    })
}

/// A Gaussian blur of `src` with `sigma` in cells, edges clamped.
fn blur(src: &[f32], cols: usize, rows: usize, sigma: f64) -> Vec<f32> {
    let radius = (3.0 * sigma).ceil().max(1.0) as usize;
    let kernel: Vec<f32> = {
        let raw: Vec<f64> = (0..=2 * radius)
            .map(|k| {
                let d = k as f64 - radius as f64;
                (-(d * d) / (2.0 * sigma * sigma)).exp()
            })
            .collect();
        let total: f64 = raw.iter().sum();
        raw.iter().map(|w| (w / total) as f32).collect()
    };
    let mut across = vec![0.0f32; src.len()];
    for j in 0..rows {
        let row = &src[j * cols..(j + 1) * cols];
        for i in 0..cols {
            let mut acc = 0.0f32;
            for (k, w) in kernel.iter().enumerate() {
                let x = (i + k).saturating_sub(radius).min(cols - 1);
                acc += w * row[x];
            }
            across[j * cols + i] = acc;
        }
    }
    let mut out = vec![0.0f32; src.len()];
    for j in 0..rows {
        for i in 0..cols {
            let mut acc = 0.0f32;
            for (k, w) in kernel.iter().enumerate() {
                let y = (j + k).saturating_sub(radius).min(rows - 1);
                acc += w * across[y * cols + i];
            }
            out[j * cols + i] = acc;
        }
    }
    out
}

/// The cells that are sketch lines: darker than their surroundings by the
/// ink level, after blurring by an amount the line thickness sets.
pub fn ink(tone: &ToneGrid, line_thickness: f64) -> Vec<bool> {
    let t = (line_thickness / 100.0).clamp(0.0, 1.0);
    let sigma = 0.6 + 2.4 * t;
    let near = blur(&tone.dark, tone.cols, tone.rows, sigma);
    let wide = blur(&tone.dark, tone.cols, tone.rows, sigma * 2.5);
    near.iter()
        .zip(&wide)
        .zip(&tone.inside)
        .map(|((a, b), inside)| *inside && a - b > INK_LEVEL)
        .collect()
}

/// Marks the cells whose centres are inside the closed loops of `region`
/// (even-odd), a row at a time.
fn region_mask(tone: &ToneGrid, region: &Region, tol: Tolerance) -> Vec<bool> {
    let polys: Vec<Vec<Point>> = region
        .loops
        .iter()
        .map(|sub| flatten_subpath(sub, tol).points)
        .collect();
    let mut mask = vec![false; tone.cols * tone.rows];
    let mut xs: Vec<f64> = Vec::new();
    for j in 0..tone.rows {
        let y = tone.centre(0, j).y;
        xs.clear();
        for poly in &polys {
            let n = poly.len();
            for k in 0..n {
                let (a, b) = (poly[k], poly[(k + 1) % n]);
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks(2) {
            let [x0, x1] = pair else { continue };
            let i0 = ((x0 - tone.origin.x) / tone.cell - 0.5).ceil().max(0.0) as usize;
            let i1 = ((x1 - tone.origin.x) / tone.cell - 0.5).floor();
            if i1 < 0.0 {
                continue;
            }
            let i1 = (i1 as usize).min(tone.cols.saturating_sub(1));
            for i in i0..=i1 {
                if i < tone.cols {
                    mask[j * tone.cols + i] = true;
                }
            }
        }
    }
    mask
}

/// The tone grid a sketch works on.
fn tone_of(doc: &Document, trace: &SketchTrace) -> Result<(ToneGrid, Option<Path>), CoreError> {
    match (trace.source, trace.bitmap) {
        (SketchSource::Bitmap, Some(id)) => placed_bitmap(doc, id),
        (SketchSource::Bitmap, None) => {
            if trace.path.as_os_str().is_empty() {
                return Err(CoreError::validation(
                    "choose the bitmap to sketch before calculating",
                ));
            }
            let image = crate::io::bitmap::read(&trace.path)?;
            Ok((
                bitmap_tone(&image, trace.origin, trace.width, trace.height)?,
                None,
            ))
        }
        (SketchSource::Model, _) => {
            if doc.components().is_empty() {
                return Err(CoreError::validation(
                    "there is no model to sketch: create a shape in the Modeling tab first",
                ));
            }
            Ok((model_tone(&doc.composite())?, None))
        }
    }
}

/// The sketch lines of a tone grid as closed vectors, limited to the
/// boundary vectors when the trace asks for them.
///
/// # Errors
/// [`CoreError::Validation`] for a line thickness outside 0 to 100, a
/// negative boundary offset, a vector boundary with no closed vectors, or
/// no lines found.
pub fn outlines_of(
    tone: &ToneGrid,
    trace: &SketchTrace,
    boundary: &[Path],
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    if !(trace.line_thickness.is_finite() && (0.0..=100.0).contains(&trace.line_thickness)) {
        return Err(CoreError::validation("the line thickness must be 0 to 100"));
    }
    if !(trace.boundary_offset.is_finite() && trace.boundary_offset >= 0.0) {
        return Err(CoreError::validation(
            "the boundary offset cannot be negative",
        ));
    }
    let mut inked = if trace.boundary == SketchBoundary::Source && trace.boundary_offset > 0.0 {
        // The source's own edge, grown outward by the offset.
        let cells = (trace.boundary_offset / tone.cell).round() as usize;
        let grown = ToneGrid {
            inside: grow_mask(&tone.inside, tone.cols, tone.rows, cells),
            ..tone.clone()
        };
        ink(&grown, trace.line_thickness)
    } else {
        ink(tone, trace.line_thickness)
    };
    if matches!(
        trace.boundary,
        SketchBoundary::Vector | SketchBoundary::Level
    ) {
        let region = Region::from_paths(boundary, tol);
        if region.is_empty() {
            return Err(CoreError::validation(match trace.boundary {
                SketchBoundary::Level => {
                    "the selected level has no components to limit the sketch: pick another level"
                }
                _ => "select a closed vector for the boundary, or use the bitmap or model boundary",
            }));
        }
        let region = if trace.boundary_offset > 0.0 {
            region.offset(-trace.boundary_offset, tol)
        } else {
            region
        };
        let within = region_mask(tone, &region, tol);
        for (cell, keep) in inked.iter_mut().zip(within) {
            *cell &= keep;
        }
    }
    // Rows of the traced bitmap count from the top.
    let (cols, rows) = (tone.cols, tone.rows);
    let mut luma = vec![255u8; cols * rows];
    for j in 0..rows {
        for i in 0..cols {
            if inked[j * cols + i] {
                luma[(rows - 1 - j) * cols + i] = 0;
            }
        }
    }
    let bitmap = Bitmap::new(cols as u32, rows as u32, luma, None)?;
    let t = trace.line_thickness / 100.0;
    let params = TraceParams {
        threshold: 128,
        invert: false,
        noise: 2.0 + 6.0 * t,
        corner_fit: FitMode::Lines,
        smoothness: 0.6,
    };
    let placement = Placement {
        origin: tone.origin,
        pixel_size: tone.cell,
    };
    let paths = trace_bitmap(&bitmap, &params, placement)?;
    if paths.is_empty() {
        return Err(CoreError::validation(
            "the sketch found no lines: change the line thickness, the boundary or the source",
        ));
    }
    Ok(paths)
}

/// `mask` with every set cell spread `cells` cells in each direction (a
/// square reach, first along the rows and then down the columns).
fn grow_mask(mask: &[bool], cols: usize, rows: usize, cells: usize) -> Vec<bool> {
    if cells == 0 {
        return mask.to_vec();
    }
    let mut across = vec![false; mask.len()];
    for j in 0..rows {
        let row = &mask[j * cols..(j + 1) * cols];
        let mut last: Option<usize> = None;
        for i in 0..cols {
            if row[i] {
                last = Some(i);
            }
            // Nearest set cell at or before i, within reach.
            if last.is_some_and(|l| i - l <= cells) {
                across[j * cols + i] = true;
            }
        }
        let mut next: Option<usize> = None;
        for i in (0..cols).rev() {
            if row[i] {
                next = Some(i);
            }
            if next.is_some_and(|n| n - i <= cells) {
                across[j * cols + i] = true;
            }
        }
    }
    let mut grown = vec![false; mask.len()];
    for i in 0..cols {
        let mut last: Option<usize> = None;
        for j in 0..rows {
            if across[j * cols + i] {
                last = Some(j);
            }
            if last.is_some_and(|l| j - l <= cells) {
                grown[j * cols + i] = true;
            }
        }
        let mut next: Option<usize> = None;
        for j in (0..rows).rev() {
            if across[j * cols + i] {
                next = Some(j);
            }
            if next.is_some_and(|n| n - j <= cells) {
                grown[j * cols + i] = true;
            }
        }
    }
    grown
}

/// The outline of the visible components on a model level, as closed
/// vectors, for a Selected Level boundary.
///
/// # Errors
/// [`CoreError::Validation`] when no level is given or the level is unknown.
pub fn level_boundary(
    doc: &Document,
    level: Option<u64>,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    let Some(level) = level else {
        return Err(CoreError::validation(
            "pick the level whose components limit the sketch",
        ));
    };
    let levels = doc.levels();
    if !levels.iter().any(|l| l.id == level) {
        return Err(CoreError::validation(format!("no level with id {level}")));
    }
    // Components from files before levels sit on the first level.
    let first = levels.first().map(|l| l.id);
    let ids: Vec<u64> = doc
        .components()
        .into_iter()
        .filter(|c| c.level == level || (c.level == 0 && first == Some(level)))
        .map(|c| c.id)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut field = doc.composite_of(&ids);
    // A level that subtracts still covers the cells it changes.
    for z in &mut field.z {
        if !z.is_nan() {
            *z = z.abs();
        }
    }
    crate::model::contour::boundary(&field, 1e-4, 0.0, tol)
}

/// The vectors that limit the sketch: the selected ones, or for a level
/// boundary the outline of that level's components.
fn limit_paths(
    doc: &Document,
    trace: &SketchTrace,
    boundary: Vec<Path>,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    match trace.boundary {
        SketchBoundary::Level => level_boundary(doc, trace.level, tol),
        _ => Ok(boundary),
    }
}

/// The boundary vectors without the placed bitmap's own outline, which comes
/// along when the bitmap is part of the selection.
fn without(boundary: &[Path], quad: Option<Path>) -> Vec<Path> {
    boundary
        .iter()
        .filter(|p| quad.as_ref() != Some(*p))
        .cloned()
        .collect()
}

/// The sketch lines for a request, the vectors the form previews.
///
/// # Errors
/// As [`outlines_of`], and when the bitmap cannot be read or there is no
/// model.
pub fn outlines(
    doc: &Document,
    trace: &SketchTrace,
    boundary: &[Path],
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    let (tone, quad) = tone_of(doc, trace)?;
    let limit = limit_paths(doc, trace, without(boundary, quad), tol)?;
    outlines_of(&tone, trace, &limit, tol)
}

/// The V shaped tool Sketch Carving cuts with: V-bits and engraving tools
/// as they are; a tapered ball nose as a V of its taper; a ball nose as a
/// 90 degree V, with the depth kept to its radius.
fn carving_tool(tool: &Tool) -> Result<(Tool, Option<f64>), CoreError> {
    let mut shaped = tool.clone();
    let limit = match tool.tool_type {
        ToolGeometry::VBit { .. } | ToolGeometry::EngravingTool { .. } => None,
        ToolGeometry::TaperedBallNose {
            diameter,
            taper_angle,
            ..
        } => {
            shaped.tool_type = ToolGeometry::EngravingTool {
                diameter,
                included_angle: (2.0 * taper_angle).clamp(1.0, 179.0),
                flat_diameter: 0.0,
            };
            None
        }
        ToolGeometry::BallNose { diameter } => {
            shaped.tool_type = ToolGeometry::EngravingTool {
                diameter,
                included_angle: 90.0,
                flat_diameter: 0.0,
            };
            Some(diameter / 2.0)
        }
        _ => return Err(CoreError::validation(
            "Sketch Carving needs a V-bit, an engraving tool, a ball nose or a tapered ball nose",
        )),
    };
    Ok((shaped, limit))
}

/// Carves the sketch lines of a tone grid.
///
/// # Errors
/// As [`outlines_of`], for a tool Sketch Carving cannot use, and whatever
/// the V-Groove reports.
pub fn sketch_carve_tone(
    tone: &ToneGrid,
    boundary: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &SketchCarveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (shaped, limit) = carving_tool(tool)?;
    let paths = outlines_of(tone, &params.trace, boundary, tol)?;
    let flat_depth = match (params.flat_depth, limit) {
        (Some(flat), Some(limit)) => Some(flat.min(limit)),
        (flat, limit) => flat.or(limit),
    };
    let vparams = VGrooveParams {
        start_depth: params.start_depth,
        flat_depth,
        max_pass_depth: (tool.pass_depth > 0.0).then_some(tool.pass_depth),
        clearance_tool: None,
        ..VGrooveParams::default()
    };
    let mut done = vgroove(&paths, job, &shaped, &vparams, tol)?;
    // Flat bottomed lines are what a flat depth asks for here.
    done.warnings.retain(|w| !w.is(FLATS_NEED_A_POCKET));
    Ok(done)
}

/// Calculates Sketch Carving.
///
/// # Errors
/// As [`sketch_carve_tone`], and when the bitmap cannot be read or there
/// is no model.
pub fn sketch_carve(
    doc: &Document,
    boundary: &[Path],
    tool: &Tool,
    params: &SketchCarveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (tone, quad) = tone_of(doc, &params.trace)?;
    let boundary = limit_paths(doc, &params.trace, without(boundary, quad), tol)?;
    sketch_carve_tone(&tone, &boundary, doc.job(), tool, params, tol)
}

fn check_percent(name: &str, value: f64) -> Result<f64, CoreError> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(value)
    } else {
        Err(CoreError::validation(format!(
            "{name} must be 0 to 100 percent"
        )))
    }
}

/// Burns the sketch lines of a tone grid as hatching.
///
/// # Errors
/// As [`outlines_of`], for a tool that is not a laser, a power outside 0 to
/// 100 percent, a stepover under 0.01 mm or a negative overscan.
pub fn laser_sketch_tone(
    tone: &ToneGrid,
    boundary: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &LaserSketchParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (tool_power, _) = tool_power_speed(tool)?;
    let power = check_percent("the power", params.power.unwrap_or(tool_power))?;
    speed_of(tool, params.speed)?;
    if !(params.stepover.is_finite() && params.stepover >= 0.01) {
        return Err(CoreError::validation(
            "the stepover must be at least 0.01 mm",
        ));
    }
    if !(params.overscan.is_finite() && params.overscan >= 0.0) {
        return Err(CoreError::validation("the overscan cannot be negative"));
    }
    if !params.angle.is_finite() {
        return Err(CoreError::validation("the hatch angle is not a number"));
    }
    let paths = outlines_of(tone, &params.trace, boundary, tol)?;
    let mut angles = vec![params.angle];
    if params.cross_hatch {
        angles.push(params.angle + 90.0);
    }
    let heights = Heights::from_job(job);
    let mut beam = Beam::new(heights);
    let mut burnt = false;
    match params.fill {
        SketchFill::Together => {
            let region = Region::from_paths(&paths, tol);
            for angle in &angles {
                let lines = raster_lines(&region, params.stepover, *angle, tol);
                for (k, line) in lines.iter().enumerate() {
                    let (Some(a), Some(b)) = (line.start(), line.end()) else {
                        continue;
                    };
                    if k > 0 {
                        // Keep moving, unpowered, to where the next line starts.
                        let dir = a.to(b);
                        let length = dir.length();
                        let lead = if length > 1e-9 && params.overscan > 0.0 {
                            a.offset(dir.scaled(-params.overscan / length))
                        } else {
                            a
                        };
                        beam.burn_to(lead, 0.0);
                    }
                    beam.burn_line(a, b, power, params.overscan);
                    burnt = true;
                }
            }
        }
        SketchFill::Regionally | SketchFill::RegionallyWithinVectors => {
            let from = heights.home.xy();
            let areas: Vec<(Region, Point)> = paths
                .iter()
                .filter_map(|p| {
                    let region = Region::from_paths(std::slice::from_ref(p), tol);
                    let start = path_bbox(p)?.min;
                    (!region.is_empty()).then_some((region, start))
                })
                .collect();
            let areas = order_nearest(areas, from, |a| a.1, |a| a.1);
            for (region, _) in &areas {
                for angle in &angles {
                    for line in raster_lines(region, params.stepover, *angle, tol) {
                        if let (Some(a), Some(b)) = (line.start(), line.end()) {
                            beam.burn_line(a, b, power, params.overscan);
                            burnt = true;
                        }
                    }
                }
            }
        }
    }
    if !burnt {
        return Err(CoreError::validation(
            "the sketch lines are narrower than the stepover: use a smaller stepover or thicker lines",
        ));
    }
    Ok(Calculated {
        moves: beam.finish(),
        warnings: Vec::new(),
    })
}

/// Calculates Laser Sketch.
///
/// # Errors
/// As [`laser_sketch_tone`], and when the bitmap cannot be read or there is
/// no model.
pub fn laser_sketch(
    doc: &Document,
    boundary: &[Path],
    tool: &Tool,
    params: &LaserSketchParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (tone, quad) = tone_of(doc, &params.trace)?;
    let boundary = limit_paths(doc, &params.trace, without(boundary, quad), tol)?;
    laser_sketch_tone(&tone, &boundary, doc.job(), tool, params, tol)
}

/// `cam.sketchPreview`: `{ params, ids }` answers `{ paths }`, the sketch
/// lines the settings give, for the form to draw before calculating.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and the
/// errors of [`outlines`].
pub fn preview_command(doc: &Document, args: &str) -> Result<Value, CoreError> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        params: SketchTrace,
        #[serde(default)]
        ids: Vec<u64>,
    }
    let args: Args =
        serde_json::from_str(args).map_err(|e| CoreError::invalid_args("cam.sketchPreview", e))?;
    let mut boundary = Vec::new();
    for id in &args.ids {
        doc.entity(*id)?;
        boundary.extend(doc.paths_of(*id).into_iter().map(|(_, p)| p));
    }
    let tol = Tolerance::new(super::compute::CUT_TOLERANCE)?;
    let paths = outlines(doc, &args.params, &boundary, tol)?;
    Ok(json!({ "paths": paths }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::{rect, BBox};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tol")
    }

    /// A white image with a black disc in the middle.
    fn disc_image(size: u32) -> Bitmap {
        let mut luma = vec![255u8; (size * size) as usize];
        let c = f64::from(size) / 2.0;
        let r = f64::from(size) / 4.0;
        for y in 0..size {
            for x in 0..size {
                let (dx, dy) = (f64::from(x) + 0.5 - c, f64::from(y) + 0.5 - c);
                if dx * dx + dy * dy < r * r {
                    luma[(y * size + x) as usize] = 0;
                }
            }
        }
        Bitmap::new(size, size, luma, None).expect("bitmap")
    }

    fn vbit() -> Tool {
        Tool {
            name: "V 60".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter: 6.0,
                included_angle: 60.0,
            },
            ..Tool::default()
        }
    }

    fn laser() -> Tool {
        Tool {
            name: "Laser".to_string(),
            tool_type: ToolGeometry::LaserTool {
                power: 40.0,
                speed: 1500.0,
                kerf: 0.1,
                max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
            },
            ..Tool::default()
        }
    }

    fn trace_of(width: f64) -> SketchTrace {
        SketchTrace {
            width,
            ..SketchTrace::default()
        }
    }

    #[test]
    fn a_bitmap_becomes_darkness_on_square_cells() {
        let tone = bitmap_tone(&disc_image(40), Point::new(10.0, 5.0), 20.0, 0.0).expect("tone");
        assert_eq!((tone.cols, tone.rows), (40, 40));
        assert!((tone.cell - 0.5).abs() < 1e-12);
        // The middle is black, the corners white.
        assert!(tone.dark[20 * 40 + 20] > 0.99);
        assert!(tone.dark[0] < 0.01);
        assert!(bitmap_tone(&disc_image(4), Point::new(0.0, 0.0), 0.0, 0.0).is_err());
    }

    #[test]
    fn a_placed_bitmap_is_sampled_through_its_transform() {
        // 40 pixels shown 20 mm wide at (100, 50), turned a quarter turn.
        let image = disc_image(40);
        let to_job = Mat3::new(0.0, 0.5, -0.5, 0.0, 100.0, 50.0);
        let tone = placed_tone(&image, &to_job).expect("tone");
        assert_eq!((tone.cols, tone.rows), (40, 40));
        assert!((tone.origin.x - 80.0).abs() < 1e-9 && (tone.origin.y - 50.0).abs() < 1e-9);
        assert!(tone.dark[20 * 40 + 20] > 0.99);
        assert!(tone.dark[0] < 0.01);
        assert!(tone.inside.iter().all(|c| *c));
        let flat = Mat3::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(placed_tone(&image, &flat).is_err());
    }

    #[test]
    fn large_images_are_sampled_down_to_the_grid_limit() {
        let image = Bitmap::new(1600, 400, vec![128; 1600 * 400], None).expect("bitmap");
        let tone = bitmap_tone(&image, Point::new(0.0, 0.0), 160.0, 0.0).expect("tone");
        assert_eq!((tone.cols, tone.rows), (MAX_SIDE, MAX_SIDE / 4));
        assert!(tone.dark.iter().all(|d| (d - 0.498).abs() < 0.01));
    }

    #[test]
    fn lines_follow_the_edges_of_dark_areas_not_their_middles() {
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let inked = ink(&tone, 30.0);
        let at = |i: usize, j: usize| inked[j * 80 + i];
        // Just inside the disc's edge (radius 20 round 40,40) is a line.
        assert!(at(40 + 18, 40));
        // Its middle and the white paper are not.
        assert!(!at(40, 40));
        assert!(!at(3, 3));
    }

    #[test]
    fn thicker_lines_ink_more_cells() {
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let count = |t: f64| ink(&tone, t).iter().filter(|c| **c).count();
        assert!(count(90.0) > count(10.0));
    }

    #[test]
    fn a_blank_image_has_no_lines() {
        let image = Bitmap::new(50, 50, vec![255; 2500], None).expect("bitmap");
        let tone = bitmap_tone(&image, Point::new(0.0, 0.0), 50.0, 0.0).expect("tone");
        assert!(outlines_of(&tone, &trace_of(50.0), &[], tol()).is_err());
    }

    #[test]
    fn the_sketch_traces_a_ring_round_the_disc() {
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let paths = outlines_of(&tone, &trace_of(80.0), &[], tol()).expect("paths");
        // One ring: an outer loop with a hole.
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths.len(), 2);
        let bbox = path_bbox(&paths[0]).expect("bbox");
        assert!(bbox.min.x > 15.0 && bbox.max.x < 65.0, "{bbox:?}");
    }

    #[test]
    fn a_vector_boundary_limits_the_lines() {
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let half = rect(Point::new(0.0, 0.0), Point::new(40.0, 80.0)).expect("rect");
        let trace = SketchTrace {
            boundary: SketchBoundary::Vector,
            ..trace_of(80.0)
        };
        let paths = outlines_of(&tone, &trace, std::slice::from_ref(&half), tol()).expect("paths");
        for path in &paths {
            assert!(path_bbox(path).expect("bbox").max.x <= 40.5);
        }
        assert!(outlines_of(&tone, &trace, &[], tol()).is_err());
        let wider = SketchTrace {
            boundary_offset: 10.0,
            ..trace
        };
        let grown = outlines_of(&tone, &wider, &[half], tol()).expect("paths");
        let right = grown
            .iter()
            .filter_map(path_bbox)
            .map(|b| b.max.x)
            .fold(0.0, f64::max);
        assert!(right > 45.0, "{right}");
    }

    #[test]
    fn the_boundary_offset_grows_the_sources_own_edge() {
        let mut tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        // A source that only covers the left half, cells 1 mm wide.
        for j in 0..tone.rows {
            for i in 0..tone.cols {
                tone.inside[j * tone.cols + i] = i < 40;
            }
        }
        let right = |offset: f64| {
            let trace = SketchTrace {
                boundary_offset: offset,
                ..trace_of(80.0)
            };
            outlines_of(&tone, &trace, &[], tol())
                .expect("paths")
                .iter()
                .filter_map(path_bbox)
                .map(|b| b.max.x)
                .fold(0.0, f64::max)
        };
        assert!(right(0.0) <= 40.5, "{}", right(0.0));
        assert!(right(10.0) > 45.0, "{}", right(10.0));
    }

    #[test]
    fn grown_masks_reach_a_square_around_each_cell() {
        let mut mask = vec![false; 49];
        mask[3 * 7 + 3] = true;
        let grown = grow_mask(&mask, 7, 7, 2);
        assert_eq!(grown.iter().filter(|c| **c).count(), 25);
        assert!(grown[8] && grown[40] && !grown[0]);
        assert_eq!(grow_mask(&mask, 7, 7, 0), mask);
    }

    #[test]
    fn a_selected_level_limits_the_sketch_to_its_components() {
        let mut doc = Document::new();
        let level = doc.active_level();
        let grid = crate::model::grid_for(doc.job());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        let (x0, y0) = (grid.origin.x, grid.origin.y);
        for j in 0..grid.height {
            for i in 0..grid.width {
                let (x, y) = (x0 + i as f64 * grid.cell, y0 + j as f64 * grid.cell);
                if (20.0..60.0).contains(&x) && (30.0..50.0).contains(&y) {
                    field.z[j * grid.width + i] = 3.0;
                }
            }
        }
        doc.begin("Model");
        doc.add_component(crate::model::Component {
            id: 0,
            name: "Block".to_string(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level,
            extra: Default::default(),
            field,
        });
        let empty = doc.add_level(None, crate::model::Combine::Add);
        doc.commit();
        let paths = level_boundary(&doc, Some(level), tol()).expect("outline");
        let bbox = paths
            .iter()
            .filter_map(path_bbox)
            .reduce(BBox::union)
            .expect("bbox");
        assert!(
            (bbox.min.x - 20.0).abs() < 2.0 && (bbox.max.x - 60.0).abs() < 2.0,
            "{bbox:?}"
        );
        assert!(
            (bbox.min.y - 30.0).abs() < 2.0 && (bbox.max.y - 50.0).abs() < 2.0,
            "{bbox:?}"
        );
        assert!(level_boundary(&doc, Some(empty), tol())
            .expect("empty")
            .is_empty());
        assert!(level_boundary(&doc, None, tol()).is_err());
        assert!(level_boundary(&doc, Some(999), tol()).is_err());
        // An empty level cannot limit anything.
        let tone = bitmap_tone(&disc_image(40), Point::new(0.0, 0.0), 40.0, 0.0).expect("tone");
        let trace = SketchTrace {
            boundary: SketchBoundary::Level,
            level: Some(empty),
            ..trace_of(40.0)
        };
        let error = outlines_of(&tone, &trace, &[], tol()).expect_err("refused");
        assert!(error.to_string().contains("level"), "{error}");
    }

    #[test]
    fn bad_trace_settings_are_refused() {
        let tone = bitmap_tone(&disc_image(40), Point::new(0.0, 0.0), 40.0, 0.0).expect("tone");
        let thick = SketchTrace {
            line_thickness: 120.0,
            ..trace_of(40.0)
        };
        assert!(outlines_of(&tone, &thick, &[], tol()).is_err());
        let offset = SketchTrace {
            boundary_offset: -1.0,
            ..trace_of(40.0)
        };
        assert!(outlines_of(&tone, &offset, &[], tol()).is_err());
    }

    #[test]
    fn sketch_carving_v_carves_the_lines_below_the_start_depth() {
        let job = JobSetup::default();
        let surface = Heights::from_job(&job).surface;
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let params = SketchCarveParams {
            trace: trace_of(80.0),
            start_depth: 0.5,
            flat_depth: Some(1.0),
        };
        let done = sketch_carve_tone(&tone, &[], &job, &vbit(), &params, tol()).expect("carve");
        let cuts: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| m.to.z)
            .collect();
        assert!(!cuts.is_empty());
        let deepest = cuts.iter().cloned().fold(f64::MAX, f64::min);
        assert!(deepest >= surface - 1.5 - 1e-6, "{deepest}");
        assert!(cuts.iter().all(|z| *z <= surface - 0.5 + 1e-6));
        assert!(done.warnings.iter().all(|w| !w.is(FLATS_NEED_A_POCKET)));
    }

    #[test]
    fn sketch_carving_takes_ball_noses_but_not_end_mills() {
        let job = JobSetup::default();
        let surface = Heights::from_job(&job).surface;
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let params = SketchCarveParams {
            trace: trace_of(80.0),
            ..SketchCarveParams::default()
        };
        let ball = Tool {
            tool_type: ToolGeometry::BallNose { diameter: 1.0 },
            ..Tool::default()
        };
        let done = sketch_carve_tone(&tone, &[], &job, &ball, &params, tol()).expect("ball");
        // A ball nose goes no deeper than its radius.
        assert!(done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .all(|m| m.to.z >= surface - 0.5 - 1e-6));
        let mill = Tool {
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        assert!(sketch_carve_tone(&tone, &[], &job, &mill, &params, tol()).is_err());
    }

    #[test]
    fn laser_sketch_burns_hatching_at_the_surface_with_the_power() {
        let job = JobSetup::default();
        let surface = Heights::from_job(&job).surface;
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let params = LaserSketchParams {
            trace: trace_of(80.0),
            power: Some(55.0),
            stepover: 0.5,
            ..LaserSketchParams::default()
        };
        let done = laser_sketch_tone(&tone, &[], &job, &laser(), &params, tol()).expect("burn");
        let burns: Vec<_> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        assert!(!burns.is_empty());
        assert!(burns.iter().all(|m| (m.to.z - surface).abs() < 1e-9));
        assert!(burns
            .iter()
            .all(|m| m.power == Some(55.0) || m.power == Some(0.0)));

        let cross = LaserSketchParams {
            cross_hatch: true,
            ..params.clone()
        };
        let crossed = laser_sketch_tone(&tone, &[], &job, &laser(), &cross, tol()).expect("x");
        assert!(crossed.moves.len() > done.moves.len());
        assert!(laser_sketch_tone(&tone, &[], &job, &vbit(), &params, tol()).is_err());
        let fine = LaserSketchParams {
            stepover: 0.001,
            ..params
        };
        assert!(laser_sketch_tone(&tone, &[], &job, &laser(), &fine, tol()).is_err());
    }

    #[test]
    fn filling_together_keeps_moving_unpowered_between_lines() {
        let job = JobSetup::default();
        let tone = bitmap_tone(&disc_image(80), Point::new(0.0, 0.0), 80.0, 0.0).expect("tone");
        let base = LaserSketchParams {
            trace: trace_of(80.0),
            stepover: 0.5,
            ..LaserSketchParams::default()
        };
        let rapids = |fill: SketchFill| {
            let params = LaserSketchParams {
                fill,
                ..base.clone()
            };
            laser_sketch_tone(&tone, &[], &job, &laser(), &params, tol())
                .expect("burn")
                .moves
                .iter()
                .filter(|m| m.kind == MoveKind::Rapid)
                .count()
        };
        assert!(rapids(SketchFill::Together) < rapids(SketchFill::Regionally));
        assert!(rapids(SketchFill::Together) <= 3);
    }

    #[test]
    fn a_model_is_shaded_and_sketched_round_its_edges() {
        let mut field = Field {
            width: 60,
            height: 60,
            cell: 0.5,
            origin: Point::new(0.25, 0.25),
            z: vec![f32::NAN; 3600],
        };
        // A raised square with sloped sides.
        for j in 15..45 {
            for i in 15..45 {
                let edge = (i - 15).min(44 - i).min(j - 15).min(44 - j) as f32;
                field.z[j * 60 + i] = (1.0 + edge).min(5.0);
            }
        }
        let tone = model_tone(&field).expect("tone");
        assert_eq!((tone.cols, tone.rows), (60, 60));
        assert!(tone.origin.x.abs() < 1e-12);
        // Outside the rim round the model is not part of the source.
        assert!(!tone.inside[0]);
        assert!(tone.inside[30 * 60 + 30]);
        // A slope facing away from the light is darker than the flat top.
        assert!(tone.dark[30 * 60 + 43] > tone.dark[30 * 60 + 30]);
        let paths = outlines_of(&tone, &trace_of(30.0), &[], tol()).expect("paths");
        assert!(!paths.is_empty());
        let empty = Field {
            z: vec![f32::NAN; 3600],
            ..field
        };
        assert!(model_tone(&empty).is_err());
    }

    #[test]
    fn params_read_from_the_forms_json() {
        let params: SketchCarveParams = serde_json::from_value(json!({
            "source": "model",
            "lineThickness": 20.0,
            "boundary": "vector",
            "boundaryOffset": 2.0,
            "level": 3,
            "startDepth": 0.2,
            "flatDepth": null,
        }))
        .expect("reads");
        assert_eq!(params.trace.source, SketchSource::Model);
        assert_eq!(params.trace.boundary, SketchBoundary::Vector);
        assert_eq!(params.trace.level, Some(3));
        assert!((params.start_depth - 0.2).abs() < 1e-12);
        assert!(uses_model(&json!({ "source": "model" })));
        assert!(!uses_model(&json!({ "source": "bitmap" })));
        let laser: LaserSketchParams =
            serde_json::from_value(json!({ "fill": "together", "crossHatch": true }))
                .expect("reads");
        assert_eq!(laser.fill, SketchFill::Together);
        assert!(laser.cross_hatch);
    }
}
