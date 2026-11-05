//! Create Shape: a relief grown from closed vectors. Every cell inside the
//! shape knows how far it is from the edge and how far the edge is from the
//! ridge straight above it (the middle of each stroke, or the hip into a
//! corner): that is the local half width, so a round profile is a true
//! circular section across every stroke of a letter, not one dome over the
//! whole shape.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{flatten_subpath, Mat3, Point, Region, Tolerance};

use super::field::{distance_to, nearest_seed, Field};

/// Cross section of a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    /// Circular section, meeting the base at the edge angle.
    #[default]
    Round,
    /// Straight sides at the edge angle, meeting in a ridge.
    Angular,
    /// Straight up to a flat top `height` above the base. The Flat Plane
    /// tool leaves `height` at zero and lifts the plane by the
    /// component's base height instead.
    Flat,
    /// An S curve from the edge up to the ridge: a cove near the base
    /// easing into a rounded top, shaped by `smooth_bottom` and
    /// `smooth_top`. Across a stroke it rises as high as the stroke is
    /// half wide. See [`smooth_curve`].
    Smooth,
    /// The section an open profile vector draws, from its left end (the
    /// edge) to its right end (the ridge), scaled to each stroke's half
    /// width. See [`ProfileCurve`].
    Custom,
}

/// The Create Shape settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeParams {
    /// Cross section.
    pub profile: Profile,
    /// Angle of the sides at the edge, degrees from flat (round and
    /// angular).
    pub angle: f64,
    /// Height of a flat shape (zero or more), or the cap when `limit` is
    /// on.
    pub height: f64,
    /// Cap the shape's rise at `height`.
    pub limit: bool,
    /// Height of a base baked under the shape. The shape tools leave it at
    /// zero and set the component's dynamic base height instead.
    pub base_height: f64,
    /// Smooth profile only: how rounded the top is, 0 (the sides run
    /// straight into the ridge) to 1 (the top flattens out fully).
    pub smooth_top: f64,
    /// Smooth profile only: how rounded the foot is, 0 (the sides leave
    /// the base at a crisp angle) to 1 (the foot flattens into the base).
    pub smooth_bottom: f64,
    /// Keep inside corners of the outline sharp: near a concave corner the
    /// heights follow the distance to the corner's two edge lines instead
    /// of the corner point, so the valley there is a crisp crease rather
    /// than a rounded hollow.
    pub sharp_corners: bool,
    /// Smooth and custom profiles: vectors nested inside the outer ones
    /// are the top of the shape instead of holes. The profile runs from
    /// the outer vectors up to the inner ones, and inside them the shape
    /// stays at its full height.
    pub blend_inner: bool,
    /// Scale the whole rise (after the limit) so its highest point is
    /// `scale_height`, keeping the profile.
    pub scale: bool,
    /// Highest point of the rise when `scale` is on.
    pub scale_height: f64,
}

impl Default for ShapeParams {
    fn default() -> Self {
        ShapeParams {
            profile: Profile::Round,
            angle: 90.0,
            height: 5.0,
            limit: false,
            base_height: 0.0,
            smooth_top: 0.5,
            smooth_bottom: 0.5,
            sharp_corners: false,
            blend_inner: false,
            scale: false,
            scale_height: 5.0,
        }
    }
}

/// What a shape component was made from, kept on the component so the
/// shape tools can edit it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeSource {
    /// The settings it was last applied with.
    pub params: ShapeParams,
    /// Ids of the closed vectors it was built from; empty for a flat plane
    /// over the whole material.
    pub vectors: Vec<u64>,
    /// Id of the open profile vector of a custom shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_vector: Option<u64>,
    /// Where the component was moved to after it was built: the transform
    /// from the shape as its vectors make it to the component as it lies.
    /// None while it lies on its vectors. A rebuild from the same vectors
    /// goes through it, so the component stays where it was put.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Mat3>,
}

impl ShapeSource {
    /// The transform from the shape its vectors make to the component.
    pub fn placement(&self) -> Mat3 {
        self.placement.unwrap_or(Mat3::IDENTITY)
    }

    /// Every vector id the shape was made from, the profile vector last.
    pub fn all_vectors(&self) -> Vec<u64> {
        let mut ids = self.vectors.clone();
        ids.extend(self.profile_vector);
        ids
    }

    /// Records that the component moved by `m`. With `with_vectors` its
    /// vectors moved by `m` too, so the component still lies where they
    /// put it (`m` then undoes itself round the placement); without, the
    /// component left them by `m`.
    pub fn moved(&mut self, m: &Mat3, with_vectors: bool) {
        let p = self.placement();
        let next = match m.invert() {
            Ok(back) if with_vectors => back.then(&p).then(m),
            _ => p.then(m),
        };
        self.placement = (!near_identity(&next)).then_some(next);
    }

    /// Records that the vectors it was made from moved by `m` while the
    /// component stayed where it is: the placement takes the move back, so
    /// a rebuild from them still lands where the component lies. A move the
    /// placement cannot take back (a collapse to a line) leaves it as it is.
    pub fn vectors_moved(&mut self, m: &Mat3) {
        if let Ok(back) = m.invert() {
            let next = back.then(&self.placement());
            self.placement = (!near_identity(&next)).then_some(next);
        }
    }

    /// The same source made from other vectors: `map` answers each old id's
    /// copy. Ids it has no copy for stay as they are.
    pub fn remapped(&self, map: &dyn Fn(u64) -> Option<u64>) -> ShapeSource {
        ShapeSource {
            params: self.params,
            vectors: self
                .vectors
                .iter()
                .map(|id| map(*id).unwrap_or(*id))
                .collect(),
            profile_vector: self.profile_vector.map(|id| map(id).unwrap_or(id)),
            placement: self.placement,
        }
    }
}

/// True when `m` moves nothing by more than rounding.
fn near_identity(m: &Mat3) -> bool {
    const EPS: f64 = 1e-9;
    let i = Mat3::IDENTITY;
    [
        m.a - i.a,
        m.b - i.b,
        m.c - i.c,
        m.d - i.d,
        m.e - i.e,
        m.f - i.f,
    ]
    .iter()
    .all(|v| v.abs() <= EPS)
}

/// True when `m` keeps lengths and angles (a move, turn or mirror), so a
/// shape built from vectors moved by it is the shape moved by it.
pub fn is_rigid(m: &Mat3) -> bool {
    const EPS: f64 = 1e-9;
    (m.a * m.a + m.b * m.b - 1.0).abs() <= EPS
        && (m.c * m.c + m.d * m.d - 1.0).abs() <= EPS
        && (m.a * m.c + m.b * m.d).abs() <= EPS
}

/// Samples in a [`ProfileCurve`].
const PROFILE_SAMPLES: usize = 512;

/// The section of a custom shape: an open vector read as heights above the
/// straight line joining its two ends, from its left end to its right end.
/// Parts below that line count as zero. Heights and positions are
/// fractions of the line's length, so the section keeps its proportions
/// when it is scaled to a stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileCurve {
    /// Height at evenly spaced fractions 0..=1 of the way along the line.
    heights: Vec<f64>,
}

impl ProfileCurve {
    /// Reads a flattened open vector.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when its ends are on top of each other or
    /// nothing of it lies above the line between them.
    pub fn from_points(points: &[Point]) -> Result<ProfileCurve, CoreError> {
        let (Some(&a), Some(&b)) = (points.first(), points.last()) else {
            return Err(CoreError::validation("the profile vector is empty"));
        };
        let (left, right) = if (a.x, a.y) <= (b.x, b.y) {
            (a, b)
        } else {
            (b, a)
        };
        let length = left.distance(right);
        if length < 1e-6 {
            return Err(CoreError::validation(
                "the profile vector's ends must be apart; use an open vector",
            ));
        }
        let (dx, dy) = ((right.x - left.x) / length, (right.y - left.y) / length);
        // Along the line and height above it, as fractions of its length.
        let local: Vec<(f64, f64)> = points
            .iter()
            .map(|p| {
                let (x, y) = (p.x - left.x, p.y - left.y);
                ((x * dx + y * dy) / length, (-x * dy + y * dx) / length)
            })
            .collect();
        let mut heights = vec![0.0f64; PROFILE_SAMPLES];
        for (k, slot) in heights.iter_mut().enumerate() {
            let u = k as f64 / (PROFILE_SAMPLES - 1) as f64;
            for pair in local.windows(2) {
                let ((u0, v0), (u1, v1)) = (pair[0], pair[1]);
                let (lo, hi) = (u0.min(u1), u0.max(u1));
                if u < lo - 1e-12 || u > hi + 1e-12 {
                    continue;
                }
                let v = if (u1 - u0).abs() < 1e-12 {
                    v0.max(v1)
                } else {
                    v0 + (v1 - v0) * (u - u0) / (u1 - u0)
                };
                *slot = slot.max(v);
            }
        }
        if heights.iter().all(|&h| h <= 1e-9) {
            return Err(CoreError::validation(
                "the profile vector has nothing above the line joining its ends",
            ));
        }
        Ok(ProfileCurve { heights })
    }

    /// Height at `t` of the way from the edge (0) to the ridge (1), as a
    /// fraction of the profile's length; `t` outside 0..=1 is clamped.
    pub fn at(&self, t: f64) -> f64 {
        let x = t.clamp(0.0, 1.0) * (PROFILE_SAMPLES - 1) as f64;
        let k = (x.floor() as usize).min(PROFILE_SAMPLES - 2);
        let f = x - k as f64;
        self.heights[k] * (1.0 - f) + self.heights[k + 1] * f
    }
}

/// The grid a model is built on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Cells across (X).
    pub width: usize,
    /// Cells down (Y).
    pub height: usize,
    /// Cell size in millimetres.
    pub cell: f64,
    /// Job position of the first cell's centre.
    pub origin: Point,
}

/// Which cells of the grid have their centre inside the region, by the
/// even-odd rule, found a row at a time.
pub fn inside_cells(region: &Region, grid: &Grid, tol: Tolerance) -> Vec<bool> {
    inside_polygons(&flatten_region(region, tol), grid, &Window::whole(grid))
}

fn flatten_region(region: &Region, tol: Tolerance) -> Vec<Vec<Point>> {
    region
        .loops
        .iter()
        .map(|sub| flatten_subpath(sub, tol).points)
        .collect()
}

/// The part of a grid a shape is worked out over: the cells from column
/// `i0` and row `j0` on, `width` by `height` of them. Positions are always
/// worked out from the whole grid's own column and row, so a shape comes
/// out the same over a window as over the whole grid.
#[derive(Debug, Clone, Copy)]
struct Window {
    i0: usize,
    j0: usize,
    width: usize,
    height: usize,
}

impl Window {
    fn whole(grid: &Grid) -> Window {
        Window {
            i0: 0,
            j0: 0,
            width: grid.width,
            height: grid.height,
        }
    }

    /// The cells round the polygons, with two outside cells beyond their
    /// box on every side (clipped to the grid). Every cell with its centre
    /// inside is in it, and so is the ring of outside cells round them, so
    /// edge distances inside come out as over the whole grid.
    fn around(polygons: &[Vec<Point>], grid: &Grid) -> Window {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in polygons.iter().flatten() {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return Window::whole(grid);
        }
        let span = |lo: f64, hi: f64, origin: f64, count: usize| -> (usize, usize) {
            let first = ((lo - origin) / grid.cell).floor() - 2.0;
            let last = ((hi - origin) / grid.cell).ceil() + 2.0;
            let top = count as f64 - 1.0;
            let first = first.clamp(0.0, top.max(0.0)) as usize;
            let last = last.clamp(0.0, top.max(0.0)) as usize;
            (first, (last + 1).saturating_sub(first).max(1).min(count))
        };
        let (i0, width) = span(x0, x1, grid.origin.x, grid.width);
        let (j0, height) = span(y0, y1, grid.origin.y, grid.height);
        if grid.width == 0 || grid.height == 0 {
            return Window::whole(grid);
        }
        Window {
            i0,
            j0,
            width,
            height,
        }
    }

    /// The window as a grid of its own, for the steps that only count
    /// cells (its origin is where its first cell's centre is).
    fn grid(&self, grid: &Grid) -> Grid {
        Grid {
            width: self.width,
            height: self.height,
            cell: grid.cell,
            origin: Point::new(
                grid.origin.x + self.i0 as f64 * grid.cell,
                grid.origin.y + self.j0 as f64 * grid.cell,
            ),
        }
    }
}

/// [`inside_cells`] over `window` of `grid`, row by row of the window.
fn inside_polygons(polygons: &[Vec<Point>], grid: &Grid, window: &Window) -> Vec<bool> {
    let ww = window.width;
    let mut inside = vec![false; ww * window.height];
    if ww == 0 {
        return inside;
    }
    let mut crossings: Vec<f64> = Vec::new();
    for wj in 0..window.height {
        let j = window.j0 + wj;
        let y = grid.origin.y + j as f64 * grid.cell;
        crossings.clear();
        for poly in polygons {
            let n = poly.len();
            for k in 0..n {
                let (a, b) = (poly[k], poly[(k + 1) % n]);
                if (a.y > y) != (b.y > y) {
                    crossings.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks_exact(2) {
            let first = ((pair[0] - grid.origin.x) / grid.cell).ceil().max(0.0) as usize;
            let last = ((pair[1] - grid.origin.x) / grid.cell).floor();
            if last < 0.0 {
                continue;
            }
            let last = (last as usize).min(grid.width.saturating_sub(1));
            let last = last.max(first).min(grid.width.saturating_sub(1));
            // Only the window's columns; the rest are left out.
            let (from, to) = (first.max(window.i0), last.min(window.i0 + ww - 1));
            for i in from..=to {
                if i >= first && (grid.origin.x + i as f64 * grid.cell) <= pair[1] {
                    inside[wj * ww + i - window.i0] = true;
                }
            }
        }
    }
    inside
}

/// Whether a point is inside the polygons by the even-odd rule.
fn point_inside(polygons: &[Vec<Point>], p: Point) -> bool {
    let mut inside = false;
    for poly in polygons {
        let n = poly.len();
        for k in 0..n {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                inside = !inside;
            }
        }
    }
    inside
}

/// The S curve of the smooth profile: height as a fraction of the full
/// height at `t`, the fraction of the way from the edge (0) to the ridge
/// (1).
///
/// It is the cubic Hermite curve from (0, 0) to (1, 1) whose end slopes
/// are `1 - bottom` at the edge and `1 - top` at the ridge:
///
/// `f(t) = (3t^2 - 2t^3) + (1 - bottom)(t^3 - 2t^2 + t) + (1 - top)(t^3 - t^2)`
///
/// With both sliders at 0 it is the straight line `f(t) = t`; with both at
/// 1 it is smoothstep, flat at the foot and at the top. In between it
/// blends the two: the foot curves up like a cove (the curve bends upward
/// there) and the top rolls over (it bends downward), and the slope at the
/// middle is `1.5 - (m0 + m1) / 4` with `m0 = 1 - bottom` and
/// `m1 = 1 - top`, steeper the more rounded the ends are.
/// For end slopes in 0..=1 the curve never falls, and `t` outside 0..=1
/// is clamped.
pub fn smooth_curve(t: f64, top: f64, bottom: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let (m0, m1) = (1.0 - bottom.clamp(0.0, 1.0), 1.0 - top.clamp(0.0, 1.0));
    let (t2, t3) = (t * t, t * t * t);
    (3.0 * t2 - 2.0 * t3) + m0 * (t3 - 2.0 * t2 + t) + m1 * (t3 - t2)
}

/// Inside corners turning less than this (degrees) are left rounded, so a
/// concave curve flattened into many short spans does not grow creases.
const SHARP_CORNER_MIN_TURN: f64 = 5.0;

/// Makes the edge distance chiselled at the outline's concave corners.
///
/// At a concave corner `v` with edges running off along unit directions
/// `a` and `b`, the cells beyond the ends of both edges (the wedge between
/// the edges' normals at `v`) are nearest the corner point, so plain edge
/// distance makes circles round it and a rounded valley. There the
/// distance becomes the larger of the distances to the two edges' lines,
/// which is the edge distance a mitred inward offset of the outline gives:
/// its level lines keep the corner square and the two sides meet along the
/// corner's bisector in a crease. On the wedge's sides this equals the
/// distance to the corner point, so the heights stay continuous. Only the
/// part of the wedge joined to the corner through inside cells is touched,
/// and a cell only ever gets lower.
fn sharpen_inside_corners(
    polygons: &[Vec<Point>],
    grid: &Grid,
    window: &Window,
    inside: &[bool],
    to_edge: &mut [f64],
) {
    let (w, h) = (window.width, window.height);
    let deepest = to_edge.iter().copied().fold(0.0f64, f64::max);
    let min_turn = SHARP_CORNER_MIN_TURN.to_radians();
    let mut stamp = vec![0u32; w * h];
    let mut queue: Vec<usize> = Vec::new();
    let mut corner = 0u32;
    for poly in polygons {
        let n = poly.len();
        if n < 3 {
            continue;
        }
        for k in 0..n {
            let v = poly[k];
            let prev = (1..n)
                .map(|s| poly[(k + n - s) % n])
                .find(|p| p.distance(v) > 1e-9);
            let next = (1..n)
                .map(|s| poly[(k + s) % n])
                .find(|p| p.distance(v) > 1e-9);
            let (Some(prev), Some(next)) = (prev, next) else {
                continue;
            };
            let (la, lb) = (prev.distance(v), next.distance(v));
            let a = ((prev.x - v.x) / la, (prev.y - v.y) / la);
            let b = ((next.x - v.x) / lb, (next.y - v.y) / lb);
            let opening = (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos();
            if std::f64::consts::PI - opening < min_turn {
                continue;
            }
            // The corner is concave when the narrow side of it is outside.
            let (mx, my) = (a.0 + b.0, a.1 + b.1);
            let ml = (mx * mx + my * my).sqrt();
            let eps = (la.min(lb) * 1e-3).max(1e-9);
            let probe = Point::new(v.x + eps * mx / ml, v.y + eps * my / ml);
            if point_inside(polygons, probe) {
                continue;
            }
            corner += 1;
            let chiselled = |i: usize, j: usize| -> Option<f64> {
                let (px, py) = (
                    grid.origin.x + (window.i0 + i) as f64 * grid.cell - v.x,
                    grid.origin.y + (window.j0 + j) as f64 * grid.cell - v.y,
                );
                if px * a.0 + py * a.1 > 0.0 || px * b.0 + py * b.1 > 0.0 {
                    return None;
                }
                let d = (px * a.1 - py * a.0).abs().max((px * b.1 - py * b.0).abs());
                (d < deepest).then_some(d)
            };
            // Seed from the cells round the corner, then flood the wedge.
            queue.clear();
            let ci = ((v.x - grid.origin.x) / grid.cell).floor() as i64;
            let cj = ((v.y - grid.origin.y) / grid.cell).floor() as i64;
            for dj in -1..=2 {
                for di in -1..=2 {
                    let (i, j) = (ci + di, cj + dj);
                    if i >= 0 && j >= 0 && (i as usize) < grid.width && (j as usize) < grid.height {
                        // Cells off the window are all outside the shape.
                        let (i, j) = (i as usize, j as usize);
                        if i >= window.i0
                            && j >= window.j0
                            && i - window.i0 < w
                            && j - window.j0 < h
                        {
                            queue.push((j - window.j0) * w + i - window.i0);
                        }
                    }
                }
            }
            while let Some(index) = queue.pop() {
                if stamp[index] == corner || !inside[index] {
                    continue;
                }
                stamp[index] = corner;
                let (i, j) = (index % w, index / w);
                let Some(d) = chiselled(i, j) else {
                    continue;
                };
                to_edge[index] = to_edge[index].min(d);
                if i > 0 {
                    queue.push(index - 1);
                }
                if i + 1 < w {
                    queue.push(index + 1);
                }
                if j > 0 {
                    queue.push(index - w);
                }
                if j + 1 < h {
                    queue.push(index + w);
                }
            }
        }
    }
}

/// Builds the heights of a shape over the whole grid, NaN outside it. A
/// custom profile needs [`create_shape_with`].
///
/// # Errors
/// As [`create_shape_with`].
pub fn create_shape(
    region: &Region,
    grid: &Grid,
    params: &ShapeParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    create_shape_with(region, grid, params, None, tol)
}

fn check_params(params: &ShapeParams, custom: Option<&ProfileCurve>) -> Result<(), CoreError> {
    let angled = matches!(params.profile, Profile::Round | Profile::Angular);
    if angled && !(params.angle > 0.0 && params.angle <= 90.0) {
        return Err(CoreError::validation(
            "the shape angle must be more than 0 and at most 90 degrees",
        ));
    }
    if params.profile == Profile::Angular && params.angle >= 90.0 && !params.limit {
        return Err(CoreError::validation(
            "an angular shape at 90 degrees has upright sides and no top: turn on Limit to Height or use a smaller angle",
        ));
    }
    if params.limit
        && params.profile != Profile::Flat
        && (!params.height.is_finite() || params.height <= 0.0)
    {
        return Err(CoreError::validation(
            "the shape height must be more than zero",
        ));
    }
    if params.scale
        && params.profile != Profile::Flat
        && (!params.scale_height.is_finite() || params.scale_height <= 0.0)
    {
        return Err(CoreError::validation(
            "the height to scale to must be more than zero",
        ));
    }
    if params.profile == Profile::Flat && !(params.height.is_finite() && params.height >= 0.0) {
        return Err(CoreError::validation(
            "the height of a flat shape must be zero or more",
        ));
    }
    if !params.base_height.is_finite() {
        return Err(CoreError::validation("the base height must be a number"));
    }
    if params.profile == Profile::Smooth
        && !((0.0..=1.0).contains(&params.smooth_top)
            && (0.0..=1.0).contains(&params.smooth_bottom))
    {
        return Err(CoreError::validation(
            "the top and bottom smoothing must be between 0 and 1",
        ));
    }
    if params.profile == Profile::Custom && custom.is_none() {
        return Err(CoreError::validation(
            "a custom shape needs an open vector for its profile",
        ));
    }
    Ok(())
}

/// Depth of each polygon: how many of the others it lies inside.
fn nesting_depths(polygons: &[Vec<Point>]) -> Vec<usize> {
    polygons
        .iter()
        .enumerate()
        .map(|(k, poly)| {
            let Some(&probe) = poly.first() else {
                return 0;
            };
            polygons
                .iter()
                .enumerate()
                .filter(|(other, outer)| {
                    *other != k
                        && outer.len() > 2
                        && point_inside(std::slice::from_ref(*outer), probe)
                })
                .count()
        })
        .collect()
}

/// Distance of every cell (mm) to the nearest cell outside `inside`, less
/// half a cell to land on the boundary between them. The grid is ringed
/// with one outside cell so a shape running off the grid still has an edge
/// there.
fn edge_distance(inside: &[bool], grid: &Grid) -> Vec<f64> {
    edge_distance_from(inside, grid).0
}

/// [`edge_distance`] with, for every cell, the outside cell nearest it
/// (column, row; the ring just around the grid is -1 or the size).
fn edge_distance_from(inside: &[bool], grid: &Grid) -> (Vec<f64>, Vec<(i64, i64)>) {
    let (w, h) = (grid.width, grid.height);
    let (pw, ph) = (w + 2, h + 2);
    let mut outside = vec![true; pw * ph];
    for j in 0..h {
        for i in 0..w {
            outside[(j + 1) * pw + i + 1] = !inside[j * w + i];
        }
    }
    let (padded, nearest) = nearest_seed(&outside, pw, ph);
    let mut to_edge = vec![0.0; w * h];
    let mut from = vec![(0i64, 0i64); w * h];
    for j in 0..h {
        for i in 0..w {
            let at = (j + 1) * pw + i + 1;
            to_edge[j * w + i] = ((padded[at] - 0.5).max(0.0)) * grid.cell;
            let seed = nearest[at];
            from[j * w + i] = ((seed % pw) as i64 - 1, (seed / pw) as i64 - 1);
        }
    }
    (to_edge, from)
}

/// The eight neighbours of a cell.
const AROUND: [(i64, i64); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
];

/// Whether a cell sits on the shape's medial axis next to `other`: the two
/// take their distance from edge points well apart (more than three cells,
/// both at least six cells in)
/// and from clearly different directions (more than 40 degrees). This
/// finds the ridge running into every corner, which the highest cells
/// alone miss along a diagonal.
fn splits_from(
    cell: (i64, i64),
    near: (i64, i64),
    other: (i64, i64),
    other_near: (i64, i64),
) -> bool {
    let (gx, gy) = (
        (near.0 - other_near.0) as f64,
        (near.1 - other_near.1) as f64,
    );
    if gx * gx + gy * gy <= 9.0 {
        return false;
    }
    let a = ((cell.0 - near.0) as f64, (cell.1 - near.1) as f64);
    let b = (
        (other.0 - other_near.0) as f64,
        (other.1 - other_near.1) as f64,
    );
    let (la, lb) = (
        (a.0 * a.0 + a.1 * a.1).sqrt(),
        (b.0 * b.0 + b.1 * b.1).sqrt(),
    );
    if la < 6.0 || lb < 6.0 {
        return false;
    }
    (a.0 * b.0 + a.1 * b.1) / (la * lb) < 40f64.to_radians().cos()
}

/// The shape's ridge: the inside cells no nearer the edge than any
/// neighbour, and the medial axis cells between two edges (the hip into
/// each corner).
fn ridge_cells(inside: &[bool], to_edge: &[f64], from: &[(i64, i64)], grid: &Grid) -> Vec<bool> {
    let (w, h) = (grid.width, grid.height);
    let mut ridge = vec![false; w * h];
    for j in 0..h {
        for i in 0..w {
            let index = j * w + i;
            if !inside[index] {
                continue;
            }
            let here = to_edge[index];
            let mut top = true;
            let mut split = false;
            for (di, dj) in AROUND {
                let (x, y) = (i as i64 + di, j as i64 + dj);
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    let other = y as usize * w + x as usize;
                    if !inside[other] {
                        continue;
                    }
                    if to_edge[other] > here + 1e-9 {
                        top = false;
                    }
                    if !split {
                        split = splits_from((i as i64, j as i64), from[index], (x, y), from[other]);
                    }
                }
            }
            ridge[index] = top || split;
        }
    }
    ridge
}

/// The neighbour a cell climbs to: the steepest rise in edge distance, if
/// any neighbour is further from the edge.
fn uphill(index: usize, inside: &[bool], to_edge: &[f64], grid: &Grid) -> Option<usize> {
    let (w, h) = (grid.width, grid.height);
    let (i, j) = ((index % w) as i64, (index / w) as i64);
    let here = to_edge[index];
    let mut best: Option<(f64, usize)> = None;
    for (di, dj) in AROUND {
        let (x, y) = (i + di, j + dj);
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            continue;
        }
        let other = y as usize * w + x as usize;
        let rise = to_edge[other] - here;
        if !inside[other] || rise <= 1e-9 {
            continue;
        }
        let slope = rise / ((di * di + dj * dj) as f64).sqrt();
        if best.is_none_or(|(steepest, _)| slope > steepest) {
            best = Some((slope, other));
        }
    }
    best.map(|(_, other)| other)
}

/// The half width (mm) of the stroke through every inside cell: the edge
/// distance of the ridge cell reached by climbing straight up from it, so
/// each section runs from its own edge to the crest or hip above it.
fn stroke_spans(inside: &[bool], to_edge: &[f64], ridge: &[bool], grid: &Grid) -> Vec<f64> {
    let n = inside.len();
    let mut span = vec![f64::NAN; n];
    for index in 0..n {
        if inside[index] && ridge[index] {
            span[index] = to_edge[index];
        }
    }
    let mut path = Vec::new();
    for start in 0..n {
        if !inside[start] || !span[start].is_nan() {
            continue;
        }
        path.clear();
        let mut at = start;
        let top = loop {
            if !span[at].is_nan() {
                break span[at];
            }
            path.push(at);
            match uphill(at, inside, to_edge, grid) {
                Some(next) => at = next,
                None => break to_edge[at],
            }
        };
        for &cell in &path {
            span[cell] = top;
        }
    }
    span
}

/// Where each inside cell is across the shape: `(t, span)` with `t` from 0
/// at the edge to 1 at the top and `span` the distance (mm) the profile
/// covers there.
struct Across {
    inside: Vec<bool>,
    to_edge: Vec<f64>,
    t: Vec<f64>,
    span: Vec<f64>,
}

/// Every stroke on its own: from the edge to the ridge in the middle of
/// the stroke, over the stroke's half width.
fn across_strokes(
    polygons: &[Vec<Point>],
    grid: &Grid,
    window: &Window,
    params: &ShapeParams,
) -> Across {
    let inside = inside_polygons(polygons, grid, window);
    let sub = window.grid(grid);
    let (mut to_edge, from) = edge_distance_from(&inside, &sub);
    if params.sharp_corners && params.profile != Profile::Flat {
        sharpen_inside_corners(polygons, grid, window, &inside, &mut to_edge);
    }
    let ridge = ridge_cells(&inside, &to_edge, &from, &sub);
    let spans = stroke_spans(&inside, &to_edge, &ridge, &sub);
    let n = inside.len();
    let (mut t, mut span) = (vec![0.0; n], vec![0.0; n]);
    for index in 0..n {
        if inside[index] {
            let d = to_edge[index];
            let half = spans[index].max(d).max(1e-9);
            t[index] = d / half;
            span[index] = half;
        }
    }
    Across {
        inside,
        to_edge,
        t,
        span,
    }
}

/// Blend To Inner Vectors: the outer vectors are the edge and the vectors
/// nested one deep inside them are the top. Between the two the profile
/// runs by how far a cell is from each; inside the inner vectors it stays
/// at the top. The span is the mean distance between the two, so the
/// whole shape rises to one height.
fn across_to_inner(
    polygons: &[Vec<Point>],
    grid: &Grid,
    window: &Window,
    params: &ShapeParams,
) -> Option<Across> {
    let depths = nesting_depths(polygons);
    let outer: Vec<Vec<Point>> = polygons
        .iter()
        .zip(&depths)
        .filter(|(_, d)| **d == 0)
        .map(|(p, _)| p.clone())
        .collect();
    let inner: Vec<Vec<Point>> = polygons
        .iter()
        .zip(&depths)
        .filter(|(_, d)| **d == 1)
        .map(|(p, _)| p.clone())
        .collect();
    if inner.is_empty() || outer.is_empty() {
        return None;
    }
    let inside = inside_polygons(&outer, grid, window);
    let top = inside_polygons(&inner, grid, window);
    let mut to_edge = edge_distance(&inside, &window.grid(grid));
    if params.sharp_corners {
        sharpen_inside_corners(&outer, grid, window, &inside, &mut to_edge);
    }
    let to_top: Vec<f64> = distance_to(&top, window.width, window.height)
        .into_iter()
        .map(|d| d * grid.cell)
        .collect();
    let n = inside.len();
    let mut t = vec![0.0; n];
    let (mut sum, mut count) = (0.0, 0usize);
    for index in 0..n {
        if !inside[index] {
            continue;
        }
        if top[index] {
            t[index] = 1.0;
            continue;
        }
        let (d, e) = (to_edge[index], to_top[index]);
        t[index] = d / (d + e).max(1e-9);
        sum += d + e;
        count += 1;
    }
    let mean = if count == 0 {
        grid.cell
    } else {
        sum / count as f64
    };
    Some(Across {
        inside,
        to_edge,
        t,
        span: vec![mean; n],
    })
}

/// Builds the heights of a shape over the whole grid, NaN outside it.
/// `custom` is the section of a [`Profile::Custom`] shape.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an angle, height or smoothing
/// setting that makes no sense, a custom shape without its profile, or
/// when the vectors enclose no cell of the grid.
pub fn create_shape_with(
    region: &Region,
    grid: &Grid,
    params: &ShapeParams,
    custom: Option<&ProfileCurve>,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    shape_over(region, grid, params, custom, tol, false)
}

/// [`create_shape_with`], worked out over the whole grid when `whole` is
/// set and otherwise only over the cells round the vectors (the rest of
/// the grid stays empty), which comes out the same.
fn shape_over(
    region: &Region,
    grid: &Grid,
    params: &ShapeParams,
    custom: Option<&ProfileCurve>,
    tol: Tolerance,
    whole: bool,
) -> Result<Field, CoreError> {
    check_params(params, custom)?;
    let polygons = flatten_region(region, tol);
    let window = if whole {
        Window::whole(grid)
    } else {
        Window::around(&polygons, grid)
    };
    let blends = params.blend_inner && matches!(params.profile, Profile::Smooth | Profile::Custom);
    let across = match blends
        .then(|| across_to_inner(&polygons, grid, &window, params))
        .flatten()
    {
        Some(across) => across,
        None => across_strokes(&polygons, grid, &window, params),
    };
    if !across.inside.iter().any(|&c| c) {
        return Err(CoreError::validation(
            "the vectors are too small for the model resolution, or not closed",
        ));
    }

    let slope = params.angle.to_radians().tan();
    let sine = params.angle.to_radians().sin();
    let mut rises = vec![f64::NAN; window.width * window.height];
    let mut highest = 0.0f64;
    for (index, slot) in rises.iter_mut().enumerate() {
        if !across.inside[index] {
            continue;
        }
        let d = across.to_edge[index];
        let (t, span) = (across.t[index], across.span[index]);
        let rise = match params.profile {
            Profile::Flat => params.height,
            Profile::Angular => d * slope,
            Profile::Round => {
                // Circle meeting the base at the edge angle: radius half /
                // sin(angle), centred below the ridge.
                let half = span;
                let radius = half / sine;
                let below = half * params.angle.to_radians().cos() / sine;
                let across = half - d.min(half);
                (radius * radius - across * across).max(0.0).sqrt() - below
            }
            Profile::Smooth => span * smooth_curve(t, params.smooth_top, params.smooth_bottom),
            Profile::Custom => custom.map_or(0.0, |c| span * c.at(t)),
        };
        let rise = if params.limit && params.profile != Profile::Flat {
            rise.min(params.height)
        } else {
            rise
        }
        .max(0.0);
        highest = highest.max(rise);
        *slot = rise;
    }
    let factor = if params.scale && params.profile != Profile::Flat && highest > 1e-12 {
        params.scale_height / highest
    } else {
        1.0
    };
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for (wj, row) in rises.chunks_exact(window.width.max(1)).enumerate() {
        let start = (window.j0 + wj) * grid.width + window.i0;
        for (slot, rise) in field.z[start..start + row.len()].iter_mut().zip(row) {
            if !rise.is_nan() {
                *slot = (params.base_height + rise * factor) as f32;
            }
        }
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, polyline_path, rect};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn grid(cell: f64) -> Grid {
        Grid {
            width: (60.0 / cell) as usize,
            height: (60.0 / cell) as usize,
            cell,
            origin: Point::new(cell / 2.0, cell / 2.0),
        }
    }

    fn disc() -> Region {
        Region::from_paths(
            &[circle(Point::new(30.0, 30.0), 10.0).expect("circle")],
            tol(),
        )
    }

    /// Working a shape out only round its vectors gives the very same
    /// heights as over the whole grid: every profile, sharp corners and
    /// blending to inner vectors, shapes inside the grid, touching its
    /// edges and running off it.
    #[test]
    fn a_shape_over_its_window_matches_the_whole_grid() {
        let star = |c: Point, n: usize, outer: f64, inner: f64| {
            let points: Vec<Point> = (0..2 * n)
                .map(|k| {
                    let a = std::f64::consts::PI * k as f64 / n as f64 + 0.1;
                    let r = if k % 2 == 0 { outer } else { inner };
                    Point::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            polyline_path(&points, true).expect("star")
        };
        let regions = [
            vec![circle(Point::new(30.0, 30.0), 10.0).expect("circle")],
            vec![star(Point::new(25.0, 32.0), 6, 14.0, 6.0)],
            // A frame: blending runs from the outer to the inner outline.
            vec![
                rect(Point::new(8.0, 10.0), Point::new(50.0, 44.0)).expect("rect"),
                rect(Point::new(20.0, 20.0), Point::new(38.0, 32.0)).expect("rect"),
            ],
            // Touching the grid's corner, and running off its far side.
            vec![rect(Point::new(0.0, 0.0), Point::new(12.0, 9.0)).expect("rect")],
            vec![star(Point::new(55.0, 50.0), 5, 12.0, 5.0)],
        ];
        let curve = profile(&[(0.0, 0.0), (5.0, 4.0), (10.0, 5.0)]);
        let profiles = [
            Profile::Round,
            Profile::Angular,
            Profile::Flat,
            Profile::Smooth,
            Profile::Custom,
        ];
        let mut compared = 0;
        for paths in &regions {
            let region = Region::from_paths(paths, tol());
            for profile in profiles {
                for (sharp, blend) in [(false, false), (true, false), (false, true), (true, true)] {
                    let params = ShapeParams {
                        profile,
                        angle: 50.0,
                        height: 4.0,
                        sharp_corners: sharp,
                        blend_inner: blend,
                        ..ShapeParams::default()
                    };
                    let custom = Some(&curve);
                    for cell in [0.13, 0.25] {
                        let grid = grid(cell);
                        let whole = shape_over(&region, &grid, &params, custom, tol(), true);
                        let window = shape_over(&region, &grid, &params, custom, tol(), false);
                        let (whole, window) = match (whole, window) {
                            (Ok(a), Ok(b)) => (a, b),
                            (Err(a), Err(b)) => {
                                assert_eq!(a.to_string(), b.to_string());
                                continue;
                            }
                            (a, b) => panic!("{a:?} against {b:?}"),
                        };
                        assert_eq!((whole.width, whole.height), (window.width, window.height));
                        let same = whole
                            .z
                            .iter()
                            .zip(&window.z)
                            .all(|(a, b)| a.to_bits() == b.to_bits());
                        assert!(same, "{profile:?} sharp {sharp} blend {blend} cell {cell}");
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 150, "{compared}");
    }

    #[test]
    fn a_round_disc_at_ninety_degrees_is_a_hemisphere() {
        let field =
            create_shape(&disc(), &grid(0.1), &ShapeParams::default(), tol()).expect("shape");
        let centre = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((centre - 10.0).abs() < 0.15, "{centre}");
        // Five millimetres in from the edge the sphere is sqrt(100 - 25) high.
        let five_in = f64::from(field.at(Point::new(35.0, 30.0)));
        assert!((five_in - 75f64.sqrt()).abs() < 0.2, "{five_in}");
        assert!(field.at(Point::new(45.0, 30.0)).is_nan(), "empty outside");
    }

    // Review 033 row 15: at the default 90 degrees an angular shape rose
    // tan(90 degrees) times the distance in, about 1.6e16 mm per mm.
    #[test]
    fn an_angular_shape_at_ninety_degrees_needs_a_height_limit() {
        let params = ShapeParams {
            profile: Profile::Angular,
            ..ShapeParams::default()
        };
        let refused = create_shape(&disc(), &grid(0.1), &params, tol());
        assert!(refused.is_err(), "{:?}", refused.map(|f| f.max()));
        let limited = ShapeParams {
            limit: true,
            height: 3.0,
            ..params
        };
        let field = create_shape(&disc(), &grid(0.1), &limited, tol()).expect("shape");
        assert_eq!(field.max(), Some(3.0));
        assert_eq!(f64::from(field.at(Point::new(39.5, 30.0))), 3.0);
        let below = ShapeParams {
            angle: 89.0,
            ..params
        };
        let field = create_shape(&disc(), &grid(0.1), &below, tol()).expect("shape");
        assert!(field.max().is_some_and(|m| m.is_finite() && m < 1e3));
    }

    #[test]
    fn an_angular_profile_rises_at_its_angle() {
        let params = ShapeParams {
            profile: Profile::Angular,
            angle: 45.0,
            ..ShapeParams::default()
        };
        let field = create_shape(&disc(), &grid(0.1), &params, tol()).expect("shape");
        let two_in = f64::from(field.at(Point::new(38.0, 30.0)));
        assert!((two_in - 2.0).abs() < 0.1, "{two_in}");
    }

    #[test]
    fn a_flat_shape_is_base_plus_height_and_a_limit_caps_the_rest() {
        let flat = ShapeParams {
            profile: Profile::Flat,
            height: 3.0,
            base_height: 1.0,
            ..ShapeParams::default()
        };
        let field = create_shape(&disc(), &grid(0.2), &flat, tol()).expect("shape");
        assert_eq!(field.max(), Some(4.0));
        assert_eq!(field.min(), Some(4.0));
        let plane = ShapeParams {
            height: 0.0,
            base_height: 0.0,
            ..flat
        };
        let field = create_shape(&disc(), &grid(0.2), &plane, tol()).expect("a zero plane");
        assert_eq!(field.max(), Some(0.0));
        let capped = ShapeParams {
            limit: true,
            height: 4.0,
            ..ShapeParams::default()
        };
        let field = create_shape(&disc(), &grid(0.2), &capped, tol()).expect("shape");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn scale_sets_the_top_and_keeps_the_profile() {
        let plain =
            create_shape(&disc(), &grid(0.2), &ShapeParams::default(), tol()).expect("shape");
        let scaled = ShapeParams {
            scale: true,
            scale_height: 2.5,
            ..ShapeParams::default()
        };
        let field = create_shape(&disc(), &grid(0.2), &scaled, tol()).expect("shape");
        let top = f64::from(field.max().unwrap_or(0.0));
        assert!((top - 2.5).abs() < 1e-5, "{top}");
        // Every height is the plain one times the same factor.
        let factor = top / f64::from(plain.max().unwrap_or(1.0));
        for p in [Point::new(35.0, 30.0), Point::new(28.0, 33.0)] {
            let want = f64::from(plain.at(p)) * factor;
            assert!((f64::from(field.at(p)) - want).abs() < 1e-4);
        }
        // After a limit the scale stretches the capped shape.
        let both = ShapeParams {
            limit: true,
            height: 4.0,
            ..scaled
        };
        let field = create_shape(&disc(), &grid(0.2), &both, tol()).expect("shape");
        assert!((f64::from(field.max().unwrap_or(0.0)) - 2.5).abs() < 1e-5);
        let flat_top = f64::from(field.at(Point::new(31.0, 30.0)));
        assert!((flat_top - 2.5).abs() < 1e-5, "the capped top stays flat");
        let bad = ShapeParams {
            scale_height: 0.0,
            ..scaled
        };
        assert!(create_shape(&disc(), &grid(0.5), &bad, tol()).is_err());
    }

    #[test]
    fn a_long_bar_is_rounded_across_its_width_not_its_length() {
        let bar = Region::from_paths(
            &[rect(Point::new(5.0, 25.0), Point::new(55.0, 35.0)).expect("rect")],
            tol(),
        );
        let field = create_shape(&bar, &grid(0.1), &ShapeParams::default(), tol()).expect("shape");
        // Half width 5: the crest is 5 high all along, not 25.
        for x in [15.0, 30.0, 45.0] {
            let crest = f64::from(field.at(Point::new(x, 30.0)));
            assert!((crest - 5.0).abs() < 0.2, "{x}: {crest}");
        }
    }

    #[test]
    fn a_round_rectangle_rises_along_its_corner_hips_and_no_higher() {
        // 50 by 30: every point is at most 15 high, and the hip from a
        // corner climbs steadily to the long crest without a hump.
        let block = Region::from_paths(
            &[rect(Point::new(5.0, 15.0), Point::new(55.0, 45.0)).expect("rect")],
            tol(),
        );
        let field =
            create_shape(&block, &grid(0.2), &ShapeParams::default(), tol()).expect("shape");
        let top = f64::from(field.max().unwrap_or(0.0));
        assert!(top <= 15.2, "no higher than half the width: {top}");
        let mut last = 0.0;
        for k in 1..=15 {
            let along = f64::from(k);
            let z = f64::from(field.at(Point::new(5.0 + along, 15.0 + along)));
            assert!(
                z + 0.05 >= last,
                "the hip climbs at {along}: {z} after {last}"
            );
            assert!(
                z <= along + 0.3,
                "a hip point is no higher than its edge distance: {z}"
            );
            last = z;
        }
        let crest = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((crest - 15.0).abs() < 0.3, "{crest}");
        // Five in from the end, on the middle line: a section 15 wide.
        let end = f64::from(field.at(Point::new(10.0, 30.0)));
        assert!((end - 125f64.sqrt()).abs() < 0.3, "{end}");
    }

    #[test]
    fn a_shape_touching_the_grid_edge_is_rounded_to_that_edge() {
        // Runs from y 20 to past the top of the 60 mm grid: 40 mm of it lies
        // on the grid, so its edge there is the grid edge.
        let bar = Region::from_paths(
            &[rect(Point::new(5.0, 50.0), Point::new(55.0, 70.0)).expect("rect")],
            tol(),
        );
        let field = create_shape(&bar, &grid(0.2), &ShapeParams::default(), tol()).expect("shape");
        let top = f64::from(field.max().unwrap_or(0.0));
        assert!(top <= 5.3, "half of the 10 mm on the grid, not 20: {top}");
    }

    #[test]
    fn a_hole_stays_empty() {
        let ring = Region::from_paths(
            &[
                circle(Point::new(30.0, 30.0), 15.0).expect("circle"),
                circle(Point::new(30.0, 30.0), 5.0).expect("circle"),
            ],
            tol(),
        );
        let field = create_shape(&ring, &grid(0.2), &ShapeParams::default(), tol()).expect("shape");
        assert!(field.at(Point::new(30.0, 30.0)).is_nan());
        let crest = f64::from(field.at(Point::new(40.0, 30.0)));
        assert!((crest - 5.0).abs() < 0.3, "ring crest {crest}");
    }

    #[test]
    fn bad_settings_are_refused() {
        let steep = ShapeParams {
            angle: 0.0,
            ..ShapeParams::default()
        };
        assert!(create_shape(&disc(), &grid(0.5), &steep, tol()).is_err());
        let capped = ShapeParams {
            limit: true,
            height: 0.0,
            ..ShapeParams::default()
        };
        assert!(create_shape(&disc(), &grid(0.5), &capped, tol()).is_err());
        let flat = ShapeParams {
            profile: Profile::Flat,
            limit: true,
            height: 0.0,
            ..ShapeParams::default()
        };
        assert!(
            create_shape(&disc(), &grid(0.5), &flat, tol()).is_ok(),
            "no limit on a plane"
        );
        let sunk = ShapeParams {
            profile: Profile::Flat,
            height: -1.0,
            ..ShapeParams::default()
        };
        assert!(create_shape(&disc(), &grid(0.5), &sunk, tol()).is_err());
    }

    #[test]
    fn the_smooth_curve_rises_from_zero_to_one_without_falling() {
        for (top, bottom) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (0.0, 1.0), (1.0, 0.2)] {
            assert!(smooth_curve(0.0, top, bottom).abs() < 1e-12);
            assert!((smooth_curve(1.0, top, bottom) - 1.0).abs() < 1e-12);
            let mut last = 0.0;
            for k in 1..=200 {
                let v = smooth_curve(k as f64 / 200.0, top, bottom);
                assert!(v >= last - 1e-12, "{top} {bottom} falls at {k}");
                last = v;
            }
        }
        // Both sliders at 0 is a straight line.
        assert!((smooth_curve(0.3, 0.0, 0.0) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn the_smooth_sliders_shape_the_ends_and_the_middle() {
        let slope = |t: f64, top: f64, bottom: f64| {
            (smooth_curve(t + 1e-6, top, bottom) - smooth_curve(t - 1e-6, top, bottom)) / 2e-6
        };
        assert!((slope(0.5, 0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((slope(0.5, 1.0, 1.0) - 1.5).abs() < 1e-6);
        assert!(slope(0.5, 0.5, 0.5) > slope(0.5, 0.0, 0.5));
        assert!(slope(0.5, 0.5, 0.5) > slope(0.5, 0.5, 0.0));
        // Fully rounded ends are flat; sharp ones keep the straight slope.
        assert!(slope(1e-5, 0.5, 1.0) < 1e-3);
        assert!((slope(1e-5, 0.5, 0.0) - 1.0).abs() < 1e-3);
        assert!(slope(1.0 - 1e-5, 1.0, 0.5) < 1e-3);
        // A cove at the foot, a rounded top: below the line, then above it.
        assert!(smooth_curve(0.2, 0.5, 0.5) < 0.2);
        assert!(smooth_curve(0.8, 0.5, 0.5) > 0.8);
    }

    #[test]
    fn a_smooth_shape_rises_as_high_as_its_stroke_is_half_wide() {
        let bar = Region::from_paths(
            &[rect(Point::new(5.0, 25.0), Point::new(55.0, 35.0)).expect("rect")],
            tol(),
        );
        let params = ShapeParams {
            profile: Profile::Smooth,
            base_height: 1.0,
            ..ShapeParams::default()
        };
        let field = create_shape(&bar, &grid(0.1), &params, tol()).expect("shape");
        let crest = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((crest - 6.0).abs() < 0.1, "{crest}");
        // Scaled to 3: a quarter of the way in, the S curve is below a
        // quarter of the height.
        let scaled = ShapeParams {
            scale: true,
            scale_height: 3.0,
            ..params
        };
        let field = create_shape(&bar, &grid(0.1), &scaled, tol()).expect("shape");
        let crest = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((crest - 4.0).abs() < 0.02, "{crest}");
        let quarter = f64::from(field.at(Point::new(30.0, 26.25)));
        assert!(quarter > 1.0 && quarter < 1.0 + 0.25 * 3.0, "{quarter}");
        let foot = f64::from(field.at(Point::new(30.0, 25.05)));
        assert!(foot < 1.1, "{foot}");
        let bad = ShapeParams {
            smooth_top: 1.5,
            ..params
        };
        assert!(create_shape(&bar, &grid(0.5), &bad, tol()).is_err());
    }

    fn profile(points: &[(f64, f64)]) -> ProfileCurve {
        let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        ProfileCurve::from_points(&points).expect("profile")
    }

    #[test]
    fn a_profile_is_read_above_the_line_between_its_ends() {
        // A step: a leg straight up, then flat, drawn right to left.
        let step = profile(&[(10.0, 0.0), (10.0, 0.0), (0.0, 5.0), (0.0, 0.0)]);
        assert!((step.at(0.0) - 0.5).abs() < 1e-9, "the leg makes a wall");
        assert!((step.at(0.5) - 0.25).abs() < 1e-3);
        assert!(step.at(1.0).abs() < 1e-9);
        // Parts below the line count as zero.
        let dip = profile(&[(0.0, 0.0), (3.0, -3.0), (6.0, 3.0), (10.0, 0.0)]);
        assert!(dip.at(0.3).abs() < 1e-12);
        assert!((dip.at(0.6) - 0.3).abs() < 5e-3);
        let tilted = profile(&[(0.0, 0.0), (5.0, 5.0 + 2.0), (10.0, 10.0)]);
        assert!(
            (tilted.at(0.6) - 0.1).abs() < 5e-3,
            "measured square to the line"
        );
        let flat = [
            Point::new(0.0, 0.0),
            Point::new(10.0, -1.0),
            Point::new(20.0, 0.0),
        ];
        assert!(ProfileCurve::from_points(&flat).is_err());
        let closed = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(0.0, 0.0),
        ];
        assert!(ProfileCurve::from_points(&closed).is_err());
    }

    #[test]
    fn a_custom_shape_scales_its_profile_to_each_stroke() {
        let bar = Region::from_paths(
            &[rect(Point::new(5.0, 25.0), Point::new(55.0, 35.0)).expect("rect")],
            tol(),
        );
        // A quarter circle from the edge to the ridge: a round top.
        // Up a quarter circle to the ridge, then a leg down to the line.
        let mut points: Vec<(f64, f64)> = (0..=90)
            .map(|k| {
                let a = f64::from(k).to_radians();
                (20.0 - 20.0 * a.cos(), 20.0 * a.sin())
            })
            .collect();
        points.push((20.0, 0.0));
        let curve = profile(&points);
        let params = ShapeParams {
            profile: Profile::Custom,
            ..ShapeParams::default()
        };
        let field =
            create_shape_with(&bar, &grid(0.1), &params, Some(&curve), tol()).expect("shape");
        let crest = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((crest - 5.0).abs() < 0.1, "{crest}");
        let round = create_shape(&bar, &grid(0.1), &ShapeParams::default(), tol()).expect("shape");
        let p = Point::new(30.0, 27.0);
        assert!((f64::from(field.at(p)) - f64::from(round.at(p))).abs() < 0.15);
        assert!(
            create_shape(&bar, &grid(0.5), &params, tol()).is_err(),
            "needs a profile"
        );
    }

    #[test]
    fn blending_to_inner_vectors_rises_from_the_outer_to_the_inner_ones() {
        let rings = Region::from_paths(
            &[
                circle(Point::new(30.0, 30.0), 20.0).expect("circle"),
                circle(Point::new(30.0, 30.0), 10.0).expect("circle"),
            ],
            tol(),
        );
        let params = ShapeParams {
            profile: Profile::Smooth,
            blend_inner: true,
            ..ShapeParams::default()
        };
        let field = create_shape(&rings, &grid(0.2), &params, tol()).expect("shape");
        // The inner circle is the top, 10 mm (the ring's width) high all over.
        let middle = f64::from(field.at(Point::new(30.0, 30.0)));
        assert!((middle - 10.0).abs() < 0.3, "{middle}");
        let rim = f64::from(field.at(Point::new(30.0, 40.5)));
        assert!((rim - 10.0).abs() < 0.5, "{rim}");
        let halfway = f64::from(field.at(Point::new(45.0, 30.0)));
        assert!((halfway - 5.0).abs() < 0.3, "{halfway}");
        assert!(f64::from(field.at(Point::new(49.8, 30.0))) < 0.3);
        // Without the option the inner circle is a hole.
        let plain = ShapeParams {
            blend_inner: false,
            ..params
        };
        let field = create_shape(&rings, &grid(0.2), &plain, tol()).expect("shape");
        assert!(field.at(Point::new(30.0, 30.0)).is_nan());
        // Round and angular shapes never blend.
        let round = ShapeParams {
            blend_inner: true,
            ..ShapeParams::default()
        };
        let field = create_shape(&rings, &grid(0.2), &round, tol()).expect("shape");
        assert!(field.at(Point::new(30.0, 30.0)).is_nan());
    }

    #[test]
    fn the_placement_follows_moves_with_and_without_the_vectors() {
        use crate::geom::{Mat3, Vec2};
        let mut source = ShapeSource {
            params: ShapeParams::default(),
            vectors: vec![1],
            profile_vector: Some(2),
            placement: None,
        };
        assert_eq!(source.all_vectors(), vec![1, 2]);
        let right = Mat3::translate(Vec2::new(10.0, 0.0));
        source.moved(&right, true);
        assert_eq!(source.placement, None, "still on its vectors");
        source.moved(&right, false);
        assert_eq!(source.placement(), right);
        // Turned together with its vectors, it stays 10 mm to their right
        // in their own turned frame.
        let turn = Mat3::rotate_about(std::f64::consts::FRAC_PI_2, Point::ORIGIN);
        source.moved(&turn, true);
        let p = source.placement().apply(turn.apply(Point::new(0.0, 0.0)));
        assert!(
            (p.x - 0.0).abs() < 1e-9 && (p.y - 10.0).abs() < 1e-9,
            "{p:?}"
        );
        // Its vectors moved on their own and then the component by the
        // same move: it lies where it did relative to them.
        let before = source.placement;
        let up = Mat3::translate(Vec2::new(0.0, 25.0));
        source.vectors_moved(&up);
        assert_ne!(source.placement, before);
        source.moved(&up, false);
        let (a, b) = (source.placement(), before.unwrap_or(Mat3::IDENTITY));
        assert!(
            [
                a.a - b.a,
                a.b - b.b,
                a.c - b.c,
                a.d - b.d,
                a.e - b.e,
                a.f - b.f
            ]
            .iter()
            .all(|v| v.abs() < 1e-9),
            "{a:?} {b:?}"
        );
        let copy = source.remapped(&|id| (id == 2).then_some(7));
        assert_eq!(copy.all_vectors(), vec![1, 7]);
        assert!(is_rigid(&turn) && !is_rigid(&Mat3::scale(1.0, 0.5)));
        let text = serde_json::to_string(&ShapeSource {
            placement: None,
            ..copy
        })
        .expect("json");
        assert!(!text.contains("placement"), "{text}");
    }

    #[test]
    fn old_shape_settings_still_parse() {
        let params: ShapeParams =
            serde_json::from_str(r#"{"profile":"round","angle":45.0,"height":2.0,"limit":true}"#)
                .expect("parses");
        assert_eq!(params.smooth_top, 0.5);
        assert_eq!(params.smooth_bottom, 0.5);
        assert!(!params.sharp_corners);
        assert!(!params.blend_inner && !params.scale);
        let smooth: ShapeParams = serde_json::from_str(
            r#"{"profile":"smooth","smoothTop":0.2,"smoothBottom":0.9,"sharpCorners":true}"#,
        )
        .expect("parses");
        assert_eq!(smooth.profile, Profile::Smooth);
        assert!(smooth.sharp_corners);
    }

    fn ell() -> Region {
        // Arms 10 wide; the inside corner is at (30, 30).
        let points = [
            Point::new(20.0, 20.0),
            Point::new(40.0, 20.0),
            Point::new(40.0, 30.0),
            Point::new(30.0, 30.0),
            Point::new(30.0, 40.0),
            Point::new(20.0, 40.0),
        ];
        Region::from_paths(&[polyline_path(&points, true).expect("path")], tol())
    }

    #[test]
    fn sharp_corners_crease_the_inside_corner() {
        let params = ShapeParams {
            profile: Profile::Angular,
            angle: 45.0,
            sharp_corners: true,
            ..ShapeParams::default()
        };
        let field = create_shape(&ell(), &grid(0.1), &params, tol()).expect("shape");
        for d in [1.0, 2.0, 3.0] {
            // On the bisector, and on each straight arm, at the same
            // distance from the edge lines.
            let bisector = f64::from(field.at(Point::new(30.0 - d, 30.0 - d)));
            let arm_x = f64::from(field.at(Point::new(35.0, 30.0 - d)));
            let arm_y = f64::from(field.at(Point::new(30.0 - d, 35.0)));
            assert!((bisector - arm_x).abs() < 0.02, "{d}: {bisector} {arm_x}");
            assert!((bisector - arm_y).abs() < 0.02, "{d}: {bisector} {arm_y}");
        }
        // The crease: either side of the bisector the height rises away
        // from it at the full slope, in a V, not a rounded trough.
        let on = f64::from(field.at(Point::new(28.0, 28.0)));
        let off = f64::from(field.at(Point::new(27.5, 28.5)));
        assert!((off - on - 0.5).abs() < 0.05, "{on} {off}");
        // Without the option the corner is rounded and stands higher.
        let round = ShapeParams {
            sharp_corners: false,
            ..params
        };
        let field = create_shape(&ell(), &grid(0.1), &round, tol()).expect("shape");
        let bisector = f64::from(field.at(Point::new(28.0, 28.0)));
        assert!(bisector > 2.6, "{bisector}");
        // Away from the corner nothing changes.
        let arm = f64::from(field.at(Point::new(38.0, 25.0)));
        assert!((arm - 2.0).abs() < 0.1, "{arm}");
    }

    #[test]
    fn sharp_corners_leave_convex_shapes_alone() {
        let plain = ShapeParams::default();
        let sharp = ShapeParams {
            sharp_corners: true,
            ..plain
        };
        let a = create_shape(&disc(), &grid(0.2), &plain, tol()).expect("shape");
        let b = create_shape(&disc(), &grid(0.2), &sharp, tol()).expect("shape");
        assert_eq!(a, b);
    }
}
