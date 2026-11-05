//! Bitmap tracing: closed vectors round the dark (or light) areas of a raster
//! image, as Trace Bitmap makes them.
//!
//! The image is thresholded to a mask, then marching squares walks the 2x2
//! pixel windows and follows the pixel edges between inside and outside
//! pixels, so a block of pixels is outlined exactly along its edges. Where
//! a window holds two inside pixels on one diagonal and two outside pixels on
//! the other (a saddle), the inside pixels are joined: inside areas are
//! eight connected and outside areas four connected, the same everywhere in
//! the image. Loops keep the inside on their left, which makes outer loops
//! counter clockwise and holes clockwise once rows are flipped so Y goes up.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::io::bitmap::Bitmap;

use super::flatten::fit_cubics;
use super::path::{Path, Polyline, SubPath, Winding};
use super::point::{Point, Tolerance};
use super::simplify::simplify_polyline;

/// Alpha below this counts as transparent, which is always outside.
const OPAQUE_FROM: u8 = 128;

/// A turn sharper than this between neighbouring simplified edges, in
/// radians, is kept as a corner when fitting curves (60 degrees).
const CORNER_TURN: f64 = std::f64::consts::FRAC_PI_3;

/// How the traced outlines are rebuilt after simplification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    /// Straight lines between the simplified points.
    Lines,
    /// Smooth cubic Beziers between sharp corners, lines where two corners
    /// are neighbours.
    #[default]
    Curves,
}

/// Settings for [`trace`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TraceParams {
    /// Luma level from 0 to 255. Pixels darker than this are inside (or,
    /// with `invert`, pixels at least this light). Default 128.
    pub threshold: u8,
    /// Trace the light areas instead of the dark ones. Default false.
    pub invert: bool,
    /// Loops enclosing fewer than this many pixels are dropped, outer loops
    /// and holes alike. Default 4.
    pub noise: f64,
    /// How outlines are rebuilt. Default curves.
    pub corner_fit: FitMode,
    /// How far the result may stray from the pixel outline, in pixels.
    /// Larger values give fewer nodes and smoother shapes. Default 1.
    pub smoothness: f64,
}

impl Default for TraceParams {
    fn default() -> Self {
        TraceParams {
            threshold: 128,
            invert: false,
            noise: 4.0,
            corner_fit: FitMode::Curves,
            smoothness: 1.0,
        }
    }
}

/// Where the image sits in the job.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Job position of the image's bottom left corner, in millimetres.
    pub origin: Point,
    /// Size of one pixel, in millimetres per pixel.
    pub pixel_size: f64,
}

/// Traces the inside areas of a bitmap into closed vectors.
///
/// A pixel is inside when it is opaque (alpha of at least 128, or no alpha
/// channel) and its luma is below `threshold`, or at least `threshold` when
/// `invert` is set. Each outer loop comes back as one path, counter
/// clockwise, with the holes directly inside it as further subpaths,
/// clockwise; an island inside a hole is a path of its own. Loops are
/// simplified to `smoothness` pixels and then, for [`FitMode::Curves`],
/// rebuilt as cubic Beziers within the same tolerance. Coordinates are job
/// millimetres: pixel column `x` and row `y` (counted from the top) cover
/// `origin.x + x * pixel_size` to `origin.x + (x + 1) * pixel_size` across
/// and `origin.y + (height - 1 - y) * pixel_size` upward. Output order is
/// deterministic.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the bitmap's channels do not match
/// its size, `noise` is negative or not finite, `smoothness` or
/// `pixel_size` is not a positive finite number, or `origin` is not finite.
pub fn trace(
    bitmap: &Bitmap,
    params: &TraceParams,
    placement: Placement,
) -> Result<Vec<Path>, CoreError> {
    bitmap.check()?;
    validate(params, placement)?;
    let tol = Tolerance::new(params.smoothness * placement.pixel_size)
        .map_err(|_| CoreError::validation("smoothness is too small for the pixel size"))?;

    let mask = Mask::new(bitmap, params);
    let loops = mask.loops();

    let mut outers: Vec<RawLoop> = Vec::new();
    let mut holes: Vec<RawLoop> = Vec::new();
    for corners in loops {
        let area = shoelace(&corners);
        if area.abs() < params.noise || area == 0.0 {
            continue;
        }
        let raw = RawLoop { corners, area };
        if area > 0.0 {
            outers.push(raw);
        } else {
            holes.push(raw);
        }
    }

    let mut groups: Vec<Vec<SubPath>> = Vec::with_capacity(outers.len());
    for outer in &outers {
        groups.push(vec![rebuild(outer, params, placement, tol)?]);
    }
    for hole in &holes {
        let Some(owner) = owner_of(hole, &outers) else {
            continue;
        };
        let sub = rebuild(hole, params, placement, tol)?;
        groups[owner].push(sub);
    }
    Ok(groups.into_iter().map(Path::from_subpaths).collect())
}

/// Checks the numeric settings.
fn validate(params: &TraceParams, placement: Placement) -> Result<(), CoreError> {
    if !params.noise.is_finite() || params.noise < 0.0 {
        return Err(CoreError::validation(
            "noise must be a number of pixels, zero or more",
        ));
    }
    if !params.smoothness.is_finite() || params.smoothness <= 0.0 {
        return Err(CoreError::validation(
            "smoothness must be a positive number of pixels",
        ));
    }
    if !placement.pixel_size.is_finite() || placement.pixel_size <= 0.0 {
        return Err(CoreError::validation(
            "the pixel size must be a positive number of millimeters",
        ));
    }
    if !placement.origin.x.is_finite() || !placement.origin.y.is_finite() {
        return Err(CoreError::validation(
            "the image position must be a point in millimeters",
        ));
    }
    Ok(())
}

/// One traced loop before simplification: its corners in pixel units with
/// Y up, and its signed area in square pixels.
struct RawLoop {
    corners: Vec<(i64, i64)>,
    area: f64,
}

/// Signed area of a loop of pixel corners, in square pixels, positive
/// counter clockwise.
fn shoelace(corners: &[(i64, i64)]) -> f64 {
    let n = corners.len();
    let mut sum: i64 = 0;
    for i in 0..n {
        let (ax, ay) = corners[i];
        let (bx, by) = corners[(i + 1) % n];
        sum += ax * by - bx * ay;
    }
    sum as f64 * 0.5
}

/// The outer loop directly round a hole: the smallest one containing the
/// centre of an outside pixel just inside the hole's first edge.
fn owner_of(hole: &RawLoop, outers: &[RawLoop]) -> Option<usize> {
    let (&(x0, y0), &(x1, y1)) = (hole.corners.first()?, hole.corners.get(1)?);
    let (dx, dy) = ((x1 - x0).signum() as f64, (y1 - y0).signum() as f64);
    // Right of the edge is outside, since holes keep the inside on the left.
    let px = x0 as f64 + 0.5 * dx + 0.5 * dy;
    let py = y0 as f64 + 0.5 * dy - 0.5 * dx;
    let mut best: Option<(usize, f64)> = None;
    for (index, outer) in outers.iter().enumerate() {
        if outer.area <= hole.area.abs() {
            continue;
        }
        if best.is_some_and(|(_, area)| outer.area >= area) {
            continue;
        }
        if contains(&outer.corners, px, py) {
            best = Some((index, outer.area));
        }
    }
    best.map(|(index, _)| index)
}

/// Even odd test of a point against a loop of pixel corners. The point is a
/// pixel centre, so it never lies on the loop.
fn contains(corners: &[(i64, i64)], px: f64, py: f64) -> bool {
    let n = corners.len();
    let mut inside = false;
    for i in 0..n {
        let (ax, ay) = corners[i];
        let (bx, by) = corners[(i + 1) % n];
        let (ax, ay, bx, by) = (ax as f64, ay as f64, bx as f64, by as f64);
        if (ay > py) != (by > py) {
            let x = ax + (py - ay) * (bx - ax) / (by - ay);
            if x > px {
                inside = !inside;
            }
        }
    }
    inside
}

/// Simplifies a raw loop, places it in the job and rebuilds it as lines or
/// curves, wound counter clockwise for an outer loop and clockwise for a
/// hole.
fn rebuild(
    raw: &RawLoop,
    params: &TraceParams,
    placement: Placement,
    tol: Tolerance,
) -> Result<SubPath, CoreError> {
    let place = |&(x, y): &(i64, i64)| {
        Point::new(
            placement.origin.x + x as f64 * placement.pixel_size,
            placement.origin.y + y as f64 * placement.pixel_size,
        )
    };
    let mut full = Polyline::new(raw.corners.iter().map(place).collect(), true);
    let want = if raw.area > 0.0 {
        Winding::Ccw
    } else {
        Winding::Cw
    };
    full.orient(want);
    let mut simple = simplify_polyline(&full, tol);
    if simple.points.len() < 3 {
        simple = full.clone();
    }
    match params.corner_fit {
        FitMode::Lines => simple
            .to_subpath()
            .ok_or_else(|| CoreError::geometry("a traced loop lost all its points")),
        FitMode::Curves => fit_loop(&full.points, &simple.points, tol),
    }
}

/// Rebuilds a closed loop as cubic Beziers between its sharp corners.
///
/// `dense` is the pixel outline and `kept` the points simplification kept
/// from it, in the same order. Corners are found on `kept`; between two
/// corners the curve is fitted to the midpoints of the pixel edges, which
/// follow the true outline more closely than the stair step corners do.
/// Neighbouring corners are joined with a straight line.
fn fit_loop(dense: &[Point], kept: &[Point], tol: Tolerance) -> Result<SubPath, CoreError> {
    let n = dense.len();
    let lost = || CoreError::geometry("a traced loop lost all its points");
    let first = *kept.first().ok_or_else(lost)?;
    let mut at = dense.iter().position(|p| *p == first).ok_or_else(lost)?;
    let m = kept.len();
    let mut corners: Vec<usize> = Vec::new();
    for i in 0..m {
        let steps = dense
            .iter()
            .cycle()
            .skip(at)
            .take(n)
            .position(|p| *p == kept[i])
            .ok_or_else(lost)?;
        at = (at + steps) % n;
        let prev = kept[(i + m - 1) % m];
        let next = kept[(i + 1) % m];
        let a = prev.to(kept[i]);
        let b = kept[i].to(next);
        let turn = (a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y);
        if turn.abs() > CORNER_TURN {
            corners.push(at);
        }
    }
    let midpoint = |i: usize| {
        let (a, b) = (dense[i % n], dense[(i + 1) % n]);
        Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
    };

    if corners.is_empty() {
        let mut ring: Vec<Point> = (0..n).map(midpoint).collect();
        ring.push(midpoint(0));
        let mut sub = fit_cubics(&ring, tol)?;
        sub.closed = true;
        return Ok(sub);
    }

    let mut sub = SubPath::open(dense[corners[0]]);
    let mut run: Vec<Point> = Vec::with_capacity(n + 2);
    for (k, &from) in corners.iter().enumerate() {
        let to = corners[(k + 1) % corners.len()];
        let len = match (to + n - from) % n {
            0 => n,
            len => len,
        };
        if len == 1 {
            sub.line_to(dense[to]);
            continue;
        }
        run.clear();
        run.push(dense[from]);
        run.extend((from..from + len).map(midpoint));
        run.push(dense[to]);
        let piece = fit_cubics(&run, tol)?;
        sub.segments.extend(piece.segments);
    }
    sub.closed = true;
    Ok(sub)
}

/// Unit steps along the pixel grid, Y up: +X, +Y, -X, -Y. A left turn adds
/// one, a right turn subtracts one.
const STEPS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The thresholded image, rows flipped so row 0 is the bottom.
struct Mask {
    width: usize,
    height: usize,
    inside: Vec<bool>,
}

impl Mask {
    fn new(bitmap: &Bitmap, params: &TraceParams) -> Mask {
        let width = bitmap.width as usize;
        let height = bitmap.height as usize;
        let mut inside = vec![false; width * height];
        for row in 0..height {
            let src = (height - 1 - row) * width;
            for col in 0..width {
                let at = src + col;
                let opaque = bitmap
                    .alpha
                    .as_ref()
                    .is_none_or(|alpha| alpha[at] >= OPAQUE_FROM);
                let luma = bitmap.luma[at];
                let dark = if params.invert {
                    luma >= params.threshold
                } else {
                    luma < params.threshold
                };
                inside[row * width + col] = opaque && dark;
            }
        }
        Mask {
            width,
            height,
            inside,
        }
    }

    /// True for an inside pixel; everything beyond the image is outside.
    fn at(&self, col: i64, row: i64) -> bool {
        if col < 0 || row < 0 || col >= self.width as i64 || row >= self.height as i64 {
            return false;
        }
        self.inside[row as usize * self.width + col as usize]
    }

    /// Every boundary loop as its corner points, in pixel units with Y up,
    /// the inside on the left.
    fn loops(&self) -> Vec<Vec<(i64, i64)>> {
        let cols = self.width + 1;
        let rows = self.height + 1;
        // One bit per direction for the boundary edges leaving each grid
        // corner.
        let mut out = vec![0u8; cols * rows];
        let vertex = |x: i64, y: i64| y as usize * cols + x as usize;
        for row in 0..self.height as i64 {
            for col in 0..self.width as i64 {
                if !self.at(col, row) {
                    continue;
                }
                if !self.at(col, row - 1) {
                    out[vertex(col, row)] |= 1 << 0;
                }
                if !self.at(col + 1, row) {
                    out[vertex(col + 1, row)] |= 1 << 1;
                }
                if !self.at(col, row + 1) {
                    out[vertex(col + 1, row + 1)] |= 1 << 2;
                }
                if !self.at(col - 1, row) {
                    out[vertex(col, row + 1)] |= 1 << 3;
                }
            }
        }

        let mut loops = Vec::new();
        for start_v in 0..out.len() {
            while out[start_v] != 0 {
                let start_d = out[start_v].trailing_zeros() as usize;
                let (sx, sy) = ((start_v % cols) as i64, (start_v / cols) as i64);
                let mut corners = Vec::new();
                let (mut x, mut y, mut d) = (sx, sy, start_d);
                loop {
                    out[vertex(x, y)] &= !(1u8 << d);
                    x += STEPS[d].0;
                    y += STEPS[d].1;
                    let here = vertex(x, y);
                    let available = |dir: usize| {
                        out[here] & (1u8 << dir) != 0 || (here == start_v && dir == start_d)
                    };
                    // Right first joins the inside pixels at a saddle.
                    let next = [(d + 3) % 4, d, (d + 1) % 4]
                        .into_iter()
                        .find(|&dir| available(dir));
                    let Some(next) = next else {
                        break;
                    };
                    if next != d {
                        corners.push((x, y));
                    }
                    d = next;
                    if here == start_v && d == start_d {
                        break;
                    }
                }
                if corners.len() >= 4 {
                    loops.push(corners);
                }
            }
        }
        loops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::{path_bbox, subpath_signed_area};
    use crate::geom::path::Segment;

    /// A white opaque bitmap with the given pixels (column, row from the top)
    /// painted black.
    fn bitmap(width: u32, height: u32, dark: impl Fn(u32, u32) -> bool) -> Bitmap {
        let mut luma = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                luma.push(if dark(x, y) { 0 } else { 255 });
            }
        }
        Bitmap::new(width, height, luma, None).expect("bitmap")
    }

    fn placement() -> Placement {
        Placement {
            origin: Point::new(100.0, 200.0),
            pixel_size: 0.5,
        }
    }

    fn lines() -> TraceParams {
        TraceParams {
            corner_fit: FitMode::Lines,
            ..TraceParams::default()
        }
    }

    #[test]
    fn a_rectangle_traces_to_its_outline() {
        // 20 by 10 pixels, columns 5..25 and rows 4..14 from the top of a
        // 30 by 20 image.
        let image = bitmap(30, 20, |x, y| (5..25).contains(&x) && (4..14).contains(&y));
        for fit in [FitMode::Lines, FitMode::Curves] {
            let params = TraceParams {
                corner_fit: fit,
                ..TraceParams::default()
            };
            let paths = trace(&image, &params, placement()).expect("traces");
            assert_eq!(paths.len(), 1);
            let path = &paths[0];
            assert_eq!(path.subpaths.len(), 1);
            let sub = &path.subpaths[0];
            assert!(sub.closed);
            assert_eq!(sub.segments.len(), 4);
            assert!(sub
                .segments
                .iter()
                .all(|s| matches!(s, Segment::Line { .. })));
            let area = subpath_signed_area(sub);
            assert!((area - 50.0).abs() < 1e-9, "area {area}");
            let bbox = path_bbox(path).expect("bbox");
            assert!((bbox.min.x - 102.5).abs() < 1e-9);
            assert!((bbox.max.x - 112.5).abs() < 1e-9);
            // Rows 4..14 from the top of 20 are rows 6..16 from the bottom.
            assert!((bbox.min.y - 203.0).abs() < 1e-9);
            assert!((bbox.max.y - 208.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_ring_is_one_path_with_a_hole() {
        let image = bitmap(16, 16, |x, y| {
            let outer = (2..14).contains(&x) && (2..14).contains(&y);
            let hole = (6..10).contains(&x) && (6..10).contains(&y);
            outer && !hole
        });
        let paths = trace(&image, &lines(), placement()).expect("traces");
        assert_eq!(paths.len(), 1);
        let subs = &paths[0].subpaths;
        assert_eq!(subs.len(), 2);
        let outer = subpath_signed_area(&subs[0]);
        let hole = subpath_signed_area(&subs[1]);
        assert!((outer - 36.0).abs() < 1e-9, "outer {outer}");
        assert!((hole + 4.0).abs() < 1e-9, "hole {hole}");

        let curved = trace(&image, &TraceParams::default(), placement()).expect("traces");
        assert_eq!(curved.len(), 1);
        let subs = &curved[0].subpaths;
        assert_eq!(subs.len(), 2);
        assert!(subpath_signed_area(&subs[0]) > 0.0);
        assert!(subpath_signed_area(&subs[1]) < 0.0);
    }

    #[test]
    fn an_island_in_a_hole_is_its_own_path() {
        let image = bitmap(20, 20, |x, y| {
            let outer = (1..19).contains(&x) && (1..19).contains(&y);
            let hole = (4..16).contains(&x) && (4..16).contains(&y);
            let island = (8..12).contains(&x) && (8..12).contains(&y);
            (outer && !hole) || island
        });
        let paths = trace(&image, &lines(), placement()).expect("traces");
        assert_eq!(paths.len(), 2);
        let counts: Vec<usize> = paths.iter().map(|p| p.subpaths.len()).collect();
        assert!(counts.contains(&2) && counts.contains(&1), "{counts:?}");
    }

    #[test]
    fn threshold_and_invert_pick_the_inside() {
        // Left half mid grey, right half white.
        let mut luma = Vec::new();
        for _ in 0..8 {
            luma.extend_from_slice(&[100; 4]);
            luma.extend_from_slice(&[255; 4]);
        }
        let image = Bitmap::new(8, 8, luma, None).expect("bitmap");
        let traced = |threshold: u8, invert: bool| {
            let params = TraceParams {
                threshold,
                invert,
                ..lines()
            };
            let paths = trace(&image, &params, placement()).expect("traces");
            paths
                .iter()
                .flat_map(|p| &p.subpaths)
                .map(subpath_signed_area)
                .sum::<f64>()
        };
        assert!((traced(128, false) - 8.0).abs() < 1e-9);
        assert_eq!(traced(100, false), 0.0);
        assert!((traced(101, false) - 8.0).abs() < 1e-9);
        assert!((traced(128, true) - 8.0).abs() < 1e-9);
        assert!((traced(100, true) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn transparent_pixels_are_outside() {
        let luma = vec![0; 36];
        let alpha: Vec<u8> = (0..36).map(|i| if (i % 6) < 3 { 255 } else { 0 }).collect();
        let image = Bitmap::new(6, 6, luma, Some(alpha)).expect("bitmap");
        let paths = trace(&image, &lines(), placement()).expect("traces");
        assert_eq!(paths.len(), 1);
        let bbox = path_bbox(&paths[0]).expect("bbox");
        assert!((bbox.max.x - bbox.min.x - 1.5).abs() < 1e-9);
        assert!((bbox.max.y - bbox.min.y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn noise_drops_specks() {
        let image = bitmap(20, 20, |x, y| {
            (x == 2 && y == 2) || ((10..15).contains(&x) && (10..15).contains(&y))
        });
        let paths = trace(&image, &lines(), placement()).expect("traces");
        assert_eq!(paths.len(), 1);
        let keep = TraceParams {
            noise: 0.0,
            ..lines()
        };
        let paths = trace(&image, &keep, placement()).expect("traces");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn diagonal_pixels_join() {
        let image = bitmap(6, 6, |x, y| {
            ((1..3).contains(&x) && (1..3).contains(&y))
                || ((3..5).contains(&x) && (3..5).contains(&y))
        });
        let params = TraceParams {
            noise: 0.0,
            ..lines()
        };
        let paths = trace(&image, &params, placement()).expect("traces");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths.len(), 1);
    }

    #[test]
    fn a_disc_fits_curves() {
        let image = bitmap(40, 40, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 20.0, y as f64 + 0.5 - 20.0);
            dx * dx + dy * dy < 15.0 * 15.0
        });
        let paths = trace(&image, &TraceParams::default(), placement()).expect("traces");
        assert_eq!(paths.len(), 1);
        let sub = &paths[0].subpaths[0];
        assert!(sub.closed);
        assert!(sub
            .segments
            .iter()
            .any(|s| matches!(s, Segment::Cubic { .. })));
        assert!(sub.end().approx_eq(sub.start, Tolerance::EQUALITY));
        let area = subpath_signed_area(sub);
        let expected = std::f64::consts::PI * 7.5 * 7.5;
        assert!((area - expected).abs() < expected * 0.03, "area {area}");
    }

    #[test]
    fn bad_settings_are_refused() {
        let image = bitmap(4, 4, |_, _| true);
        let bad = TraceParams {
            smoothness: 0.0,
            ..TraceParams::default()
        };
        assert!(trace(&image, &bad, placement()).is_err());
        let bad_place = Placement {
            origin: Point::new(0.0, 0.0),
            pixel_size: -1.0,
        };
        assert!(trace(&image, &TraceParams::default(), bad_place).is_err());
    }

    #[test]
    fn params_use_camel_case() {
        let params: TraceParams =
            serde_json::from_str(r#"{"cornerFit":"lines","threshold":90}"#).expect("parses");
        assert_eq!(params.corner_fit, FitMode::Lines);
        assert_eq!(params.threshold, 90);
        assert_eq!(params.noise, 4.0);
        let placement: Placement =
            serde_json::from_str(r#"{"origin":{"x":1,"y":2},"pixelSize":0.25}"#).expect("parses");
        assert_eq!(placement.pixel_size, 0.25);
    }
}
