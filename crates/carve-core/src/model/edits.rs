//! Edits to a relief component's heights: smoothing, draft, tilt, fade,
//! mirror, flip, height scaling and emboss.
//!
//! Every edit is a pure function from a component's [`Field`] to a new one
//! on the same grid. Empty cells (NaN) stay empty; no edit adds material
//! where there was none.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;

use super::field::{distance_to, Field};

/// Most smoothing passes one edit may run.
pub const MAX_SMOOTH_ITERATIONS: u32 = 100;

/// Settings for [`smooth`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SmoothParams {
    /// Blur radius in millimetres: each pass averages the cells within this
    /// distance along X, then along Y. At least one cell is always used.
    pub strength: f64,
    /// Number of passes, 1 to [`MAX_SMOOTH_ITERATIONS`].
    pub iterations: u32,
}

impl Default for SmoothParams {
    fn default() -> Self {
        SmoothParams {
            strength: 1.0,
            iterations: 1,
        }
    }
}

/// Settings for [`add_draft`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DraftParams {
    /// Wall angle from vertical in degrees, strictly between 0 and 90.
    pub angle_deg: f64,
}

impl Default for DraftParams {
    fn default() -> Self {
        DraftParams { angle_deg: 10.0 }
    }
}

/// Settings for [`tilt`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TiltParams {
    /// Slope of the added plane in degrees, strictly between -90 and 90.
    pub angle_deg: f64,
    /// Direction the plane rises towards, in degrees anticlockwise from +X
    /// (0 is +X, 90 is +Y).
    pub direction_deg: f64,
    /// Job position (mm) where the added plane is zero.
    pub pivot: Point,
}

impl Default for TiltParams {
    fn default() -> Self {
        TiltParams {
            angle_deg: 5.0,
            direction_deg: 0.0,
            pivot: Point::ORIGIN,
        }
    }
}

/// Settings for [`fade`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FadeParams {
    /// Direction the fade runs towards, in degrees anticlockwise from +X.
    /// Heights are kept in full at the start and fall to zero at the end.
    pub direction_deg: f64,
    /// Fraction (0 to 1) of the component's extent along the direction
    /// where the fade starts; heights before it are kept in full.
    pub start_fraction: f64,
    /// Fraction (0 to 1, above the start) where heights reach zero.
    pub end_fraction: f64,
}

impl Default for FadeParams {
    fn default() -> Self {
        FadeParams {
            direction_deg: 0.0,
            start_fraction: 0.0,
            end_fraction: 1.0,
        }
    }
}

/// Settings for [`mirror`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MirrorParams {
    /// Flip left to right when true, else top to bottom.
    pub horizontal: bool,
}

impl Default for MirrorParams {
    fn default() -> Self {
        MirrorParams { horizontal: true }
    }
}

/// Settings for [`scale_heights`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScaleParams {
    /// Factor every height is multiplied by (unitless).
    pub scale: f64,
    /// Height added after scaling, in millimetres.
    pub base_offset: f64,
}

impl Default for ScaleParams {
    fn default() -> Self {
        ScaleParams {
            scale: 1.0,
            base_offset: 0.0,
        }
    }
}

/// Settings for [`emboss`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmbossParams {
    /// Height in millimetres the highest point is squashed to; above 0.
    pub max_height: f64,
}

impl Default for EmbossParams {
    fn default() -> Self {
        EmbossParams { max_height: 2.0 }
    }
}

fn finite(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CoreError::validation(format!("{what} must be a number")))
    }
}

/// Unit vector for a direction in degrees anticlockwise from +X.
fn unit(direction_deg: f64) -> (f64, f64) {
    let a = direction_deg.to_radians();
    (a.cos(), a.sin())
}

/// A copy of `field` with every non-empty height replaced by `f(i, j, h)`.
fn map_cells(field: &Field, mut f: impl FnMut(usize, usize, f64) -> f64) -> Field {
    let mut out = field.clone();
    for j in 0..field.height {
        for i in 0..field.width {
            let k = j * field.width + i;
            let h = field.z[k];
            if !h.is_nan() {
                out.z[k] = f(i, j, f64::from(h)) as f32;
            }
        }
    }
    out
}

/// Box average of the non-empty samples within `radius` of each sample of
/// one line, written to `out`; empty samples stay empty. `sum` and `count`
/// are prefix buffers one longer than the line.
fn blur_line(line: &[f64], radius: usize, out: &mut [f64], sum: &mut [f64], count: &mut [u32]) {
    let n = line.len();
    sum[0] = 0.0;
    count[0] = 0;
    for (k, &v) in line.iter().enumerate() {
        let solid = !v.is_nan();
        sum[k + 1] = sum[k] + if solid { v } else { 0.0 };
        count[k + 1] = count[k] + u32::from(solid);
    }
    for (k, slot) in out.iter_mut().enumerate().take(n) {
        if line[k].is_nan() {
            *slot = f64::NAN;
            continue;
        }
        let lo = k.saturating_sub(radius);
        let hi = (k + radius + 1).min(n);
        let c = count[hi] - count[lo];
        *slot = (sum[hi] - sum[lo]) / f64::from(c.max(1));
    }
}

/// Smooths a component by box blurring its heights, X then Y, `iterations`
/// times. Only non-empty cells take part: empty cells stay empty and do not
/// pull the edges down.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the strength is not a positive
/// number of millimetres or the iterations are outside 1 to
/// [`MAX_SMOOTH_ITERATIONS`].
pub fn smooth(field: &Field, params: &SmoothParams) -> Result<Field, CoreError> {
    finite(params.strength, "smoothing strength")?;
    if params.strength <= 0.0 {
        return Err(CoreError::validation(
            "smoothing strength must be above 0 mm",
        ));
    }
    if params.iterations == 0 || params.iterations > MAX_SMOOTH_ITERATIONS {
        return Err(CoreError::validation(format!(
            "smoothing passes must be from 1 to {MAX_SMOOTH_ITERATIONS}"
        )));
    }
    let (w, h) = (field.width, field.height);
    let radius = ((params.strength / field.cell).round() as usize).max(1);
    let mut grid: Vec<f64> = field.z.iter().map(|&v| f64::from(v)).collect();
    let longest = w.max(h);
    let mut line = vec![0.0; longest];
    let mut out = vec![0.0; longest];
    let mut sum = vec![0.0; longest + 1];
    let mut count = vec![0u32; longest + 1];
    for _ in 0..params.iterations {
        for j in 0..h {
            let row = &mut grid[j * w..(j + 1) * w];
            line[..w].copy_from_slice(row);
            blur_line(&line[..w], radius, &mut out[..w], &mut sum, &mut count);
            row.copy_from_slice(&out[..w]);
        }
        for i in 0..w {
            for j in 0..h {
                line[j] = grid[j * w + i];
            }
            blur_line(&line[..h], radius, &mut out[..h], &mut sum, &mut count);
            for j in 0..h {
                grid[j * w + i] = out[j];
            }
        }
    }
    let mut result = field.clone();
    for (slot, v) in result.z.iter_mut().zip(&grid) {
        *slot = *v as f32;
    }
    Ok(result)
}

/// Adds draft: the component's walls slope outward at `angle_deg` from
/// vertical. Each cell's height is limited to its distance (mm) from the
/// component's edge divided by `tan(angle)`, where the edge is the border
/// between material and empty cells or the outside of the field.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the angle is not strictly between
/// 0 and 90 degrees.
pub fn add_draft(field: &Field, params: &DraftParams) -> Result<Field, CoreError> {
    let angle = params.angle_deg;
    finite(angle, "draft angle")?;
    if angle <= 0.0 || angle >= 90.0 {
        return Err(CoreError::validation(
            "draft angle must be between 0 and 90 degrees",
        ));
    }
    let (w, h) = (field.width, field.height);
    // One empty cell of padding all round, so the outside counts as empty.
    let (pw, ph) = (w + 2, h + 2);
    let mut seed = vec![true; pw * ph];
    for j in 0..h {
        for i in 0..w {
            seed[(j + 1) * pw + i + 1] = field.z[j * w + i].is_nan();
        }
    }
    let dist = distance_to(&seed, pw, ph);
    let slope = angle.to_radians().tan();
    Ok(map_cells(field, |i, j, z| {
        let to_edge = (dist[(j + 1) * pw + i + 1] - 0.5).max(0.0) * field.cell;
        z.min(to_edge / slope)
    }))
}

/// Tilts a component: adds a plane rising at `angle_deg` towards
/// `direction_deg`, zero at the pivot. Empty cells stay empty.
///
/// # Errors
/// Returns [`CoreError::Validation`] when a value is not a number or the
/// angle is not strictly between -90 and 90 degrees.
pub fn tilt(field: &Field, params: &TiltParams) -> Result<Field, CoreError> {
    finite(params.angle_deg, "tilt angle")?;
    finite(params.direction_deg, "tilt direction")?;
    finite(params.pivot.x, "tilt pivot")?;
    finite(params.pivot.y, "tilt pivot")?;
    if params.angle_deg.abs() >= 90.0 {
        return Err(CoreError::validation(
            "tilt angle must be between -90 and 90 degrees",
        ));
    }
    let rise = params.angle_deg.to_radians().tan();
    let (ux, uy) = unit(params.direction_deg);
    let pivot = params.pivot;
    Ok(map_cells(field, |i, j, z| {
        let p = field.centre(i, j);
        z + rise * ((p.x - pivot.x) * ux + (p.y - pivot.y) * uy)
    }))
}

/// Fades a component: heights are multiplied by a ramp that is 1 up to
/// `start_fraction` of the way across the component along `direction_deg`
/// and falls linearly to 0 at `end_fraction`. The extent is measured
/// between the centres of the outermost non-empty cells.
///
/// # Errors
/// Returns [`CoreError::Validation`] when a value is not a number, a
/// fraction is outside 0 to 1, or the end is not after the start.
pub fn fade(field: &Field, params: &FadeParams) -> Result<Field, CoreError> {
    finite(params.direction_deg, "fade direction")?;
    finite(params.start_fraction, "fade start")?;
    finite(params.end_fraction, "fade end")?;
    let (s, e) = (params.start_fraction, params.end_fraction);
    if !(0.0..=1.0).contains(&s) || !(0.0..=1.0).contains(&e) {
        return Err(CoreError::validation(
            "fade start and end must be from 0 to 1",
        ));
    }
    if e <= s {
        return Err(CoreError::validation("fade end must be after its start"));
    }
    let (ux, uy) = unit(params.direction_deg);
    let along = |i: usize, j: usize| {
        let p = field.centre(i, j);
        p.x * ux + p.y * uy
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for j in 0..field.height {
        for i in 0..field.width {
            if !field.z[j * field.width + i].is_nan() {
                let d = along(i, j);
                lo = lo.min(d);
                hi = hi.max(d);
            }
        }
    }
    let span = hi - lo;
    Ok(map_cells(field, |i, j, z| {
        let t = if span > 0.0 {
            (along(i, j) - lo) / span
        } else {
            0.0
        };
        let factor = ((e - t) / (e - s)).clamp(0.0, 1.0);
        z * factor
    }))
}

/// Mirrors a component about its field's centre: left to right when
/// `horizontal`, else top to bottom. The grid is unchanged.
pub fn mirror(field: &Field, params: &MirrorParams) -> Field {
    let (w, h) = (field.width, field.height);
    let mut out = field.clone();
    for j in 0..h {
        for i in 0..w {
            let (si, sj) = if params.horizontal {
                (w - 1 - i, j)
            } else {
                (i, h - 1 - j)
            };
            out.z[j * w + i] = field.z[sj * w + si];
        }
    }
    out
}

/// Flips a component upside down: each height becomes `max - h`, so the
/// highest point becomes zero and the lowest becomes the old maximum.
pub fn flip_vertical(field: &Field) -> Field {
    match field.max() {
        Some(top) => {
            let top = f64::from(top);
            map_cells(field, |_, _, z| top - z)
        }
        None => field.clone(),
    }
}

/// Scales a component's heights: each becomes `h * scale + base_offset`
/// (mm).
///
/// # Errors
/// Returns [`CoreError::Validation`] when the scale or offset is not a
/// number.
pub fn scale_heights(field: &Field, params: &ScaleParams) -> Result<Field, CoreError> {
    finite(params.scale, "height scale")?;
    finite(params.base_offset, "base height")?;
    Ok(map_cells(field, |_, _, z| {
        z * params.scale + params.base_offset
    }))
}

/// Embosses a component: squashes its heights so the highest becomes
/// `max_height` (mm), keeping the shape of the detail in proportion.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the height is not above 0 mm, or
/// when the component has no height above zero to squash.
pub fn emboss(field: &Field, params: &EmbossParams) -> Result<Field, CoreError> {
    finite(params.max_height, "emboss height")?;
    if params.max_height <= 0.0 {
        return Err(CoreError::validation("emboss height must be above 0 mm"));
    }
    let Some(top) = field.max() else {
        return Ok(field.clone());
    };
    let top = f64::from(top);
    if top <= 0.0 {
        return Err(CoreError::validation(
            "the component has no height above zero to emboss",
        ));
    }
    let k = params.max_height / top;
    Ok(map_cells(field, |_, _, z| z * k))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f64 = 0.5;

    fn solid(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Field {
        let mut field = Field::empty(w, h, CELL, Point::new(10.0, 20.0));
        for j in 0..h {
            for i in 0..w {
                field.z[j * w + i] = f(i, j);
            }
        }
        field
    }

    fn at(field: &Field, i: usize, j: usize) -> f64 {
        f64::from(field.z[j * field.width + i])
    }

    fn mean(field: &Field) -> f64 {
        let vals: Vec<f64> = field
            .z
            .iter()
            .filter(|v| !v.is_nan())
            .map(|&v| f64::from(v))
            .collect();
        vals.iter().sum::<f64>() / vals.len() as f64
    }

    fn dome(n: usize) -> Field {
        let c = (n as f64 - 1.0) / 2.0;
        let r = c;
        solid(n, n, |i, j| {
            let d2 = (i as f64 - c).powi(2) + (j as f64 - c).powi(2);
            if d2 <= r * r {
                (r * r - d2).sqrt() as f32
            } else {
                f32::NAN
            }
        })
    }

    #[test]
    fn smoothing_a_step_lowers_its_top_edge_and_keeps_the_mean() {
        let step = solid(20, 6, |i, _| if i < 10 { 0.0 } else { 10.0 });
        let out = smooth(
            &step,
            &SmoothParams {
                strength: 1.0,
                iterations: 2,
            },
        )
        .expect("smooth");
        assert!(at(&out, 10, 3) < 10.0);
        assert!(at(&out, 9, 3) > 0.0);
        assert!((at(&out, 19, 3) - 10.0).abs() < 1e-5);
        assert!((mean(&out) - mean(&step)).abs() < 0.2);
    }

    #[test]
    fn smoothing_keeps_empty_cells_empty_and_does_not_bleed_them_in() {
        let plateau = solid(12, 12, |i, j| {
            if (3..9).contains(&i) && (3..9).contains(&j) {
                5.0
            } else {
                f32::NAN
            }
        });
        let out = smooth(&plateau, &SmoothParams::default()).expect("smooth");
        for (a, b) in plateau.z.iter().zip(&out.z) {
            assert_eq!(a.is_nan(), b.is_nan());
            if !b.is_nan() {
                assert!((b - 5.0).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn smoothing_rejects_bad_settings() {
        let f = solid(4, 4, |_, _| 1.0);
        for bad in [
            SmoothParams {
                strength: 0.0,
                iterations: 1,
            },
            SmoothParams {
                strength: f64::NAN,
                iterations: 1,
            },
            SmoothParams {
                strength: 1.0,
                iterations: 0,
            },
            SmoothParams {
                strength: 1.0,
                iterations: MAX_SMOOTH_ITERATIONS + 1,
            },
        ] {
            assert!(matches!(smooth(&f, &bad), Err(CoreError::Validation(_))));
        }
    }

    #[test]
    fn draft_of_a_tall_block_slopes_from_the_rim() {
        let block = solid(20, 20, |_, _| 100.0);
        let out = add_draft(&block, &DraftParams { angle_deg: 45.0 }).expect("draft");
        // Rim cell centre is half a cell in from the edge.
        assert!((at(&out, 0, 10) - 0.5 * CELL).abs() < 1e-5);
        assert!((at(&out, 3, 10) - 3.5 * CELL).abs() < 1e-5);
        let steep = add_draft(&block, &DraftParams { angle_deg: 30.0 }).expect("draft");
        let want = 2.5 * CELL / 30f64.to_radians().tan();
        assert!((at(&steep, 10, 2) - want).abs() < 1e-5);
    }

    #[test]
    fn draft_measures_from_empty_cells_and_leaves_low_cells_alone() {
        let mut block = solid(21, 21, |_, _| 1.0);
        block.z[10 * 21 + 10] = f32::NAN;
        block.z[10 * 21 + 15] = 50.0;
        let out = add_draft(&block, &DraftParams { angle_deg: 45.0 }).expect("draft");
        assert!(out.z[10 * 21 + 10].is_nan());
        assert!((at(&out, 11, 10) - 0.5 * CELL).abs() < 1e-5);
        assert!((at(&out, 15, 10) - 4.5 * CELL).abs() < 1e-5);
        assert!((at(&out, 5, 5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn draft_rejects_angles_out_of_range() {
        let f = solid(4, 4, |_, _| 1.0);
        for angle in [0.0, 90.0, -5.0, f64::INFINITY] {
            assert!(matches!(
                add_draft(&f, &DraftParams { angle_deg: angle }),
                Err(CoreError::Validation(_))
            ));
        }
    }

    #[test]
    fn tilt_at_45_degrees_towards_x_adds_the_distance() {
        let flat = solid(10, 10, |_, _| 2.0);
        let pivot = flat.centre(4, 4);
        let out = tilt(
            &flat,
            &TiltParams {
                angle_deg: 45.0,
                direction_deg: 0.0,
                pivot,
            },
        )
        .expect("tilt");
        for i in 0..10 {
            let dx = flat.centre(i, 7).x - pivot.x;
            assert!((at(&out, i, 7) - (2.0 + dx)).abs() < 1e-5);
        }
    }

    #[test]
    fn tilt_towards_y_keeps_empty_cells_and_rejects_steep_angles() {
        let mut flat = solid(6, 6, |_, _| 0.0);
        flat.z[0] = f32::NAN;
        let pivot = flat.centre(0, 0);
        let out = tilt(
            &flat,
            &TiltParams {
                angle_deg: 45.0,
                direction_deg: 90.0,
                pivot,
            },
        )
        .expect("tilt");
        assert!(out.z[0].is_nan());
        assert!((at(&out, 3, 4) - 4.0 * CELL).abs() < 1e-5);
        assert!(matches!(
            tilt(
                &flat,
                &TiltParams {
                    angle_deg: 90.0,
                    ..TiltParams::default()
                }
            ),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn fade_runs_from_full_height_to_zero() {
        let flat = solid(11, 3, |_, _| 4.0);
        let out = fade(&flat, &FadeParams::default()).expect("fade");
        assert!((at(&out, 0, 1) - 4.0).abs() < 1e-6);
        assert!((at(&out, 5, 1) - 2.0).abs() < 1e-6);
        assert!(at(&out, 10, 1).abs() < 1e-6);
    }

    #[test]
    fn fade_holds_before_the_start_and_follows_the_direction() {
        let flat = solid(3, 11, |_, _| 4.0);
        let out = fade(
            &flat,
            &FadeParams {
                direction_deg: 270.0,
                start_fraction: 0.5,
                end_fraction: 1.0,
            },
        )
        .expect("fade");
        // Towards -Y: the top row is the start, the bottom row the end.
        assert!((at(&out, 1, 10) - 4.0).abs() < 1e-5);
        assert!((at(&out, 1, 5) - 4.0).abs() < 1e-5);
        assert!(at(&out, 1, 0).abs() < 1e-5);
        assert!((at(&out, 1, 2) - 1.6).abs() < 1e-5);
    }

    #[test]
    fn fade_rejects_bad_fractions() {
        let f = solid(4, 4, |_, _| 1.0);
        for (s, e) in [(0.5, 0.5), (0.8, 0.2), (-0.1, 1.0), (0.0, 1.5)] {
            let p = FadeParams {
                direction_deg: 0.0,
                start_fraction: s,
                end_fraction: e,
            };
            assert!(matches!(fade(&f, &p), Err(CoreError::Validation(_))));
        }
    }

    #[test]
    fn mirror_twice_returns_the_original() {
        let f = solid(7, 5, |i, j| {
            if i == 1 && j == 1 {
                f32::NAN
            } else {
                (i * 10 + j) as f32
            }
        });
        for horizontal in [true, false] {
            let p = MirrorParams { horizontal };
            assert_eq!(mirror(&mirror(&f, &p), &p), f);
        }
    }

    #[test]
    fn mirror_flips_the_right_axis() {
        let f = solid(7, 5, |i, j| (i * 10 + j) as f32);
        let lr = mirror(&f, &MirrorParams { horizontal: true });
        assert_eq!(at(&lr, 0, 2), at(&f, 6, 2));
        assert_eq!(lr.origin, f.origin);
        let tb = mirror(&f, &MirrorParams { horizontal: false });
        assert_eq!(at(&tb, 3, 0), at(&f, 3, 4));
    }

    #[test]
    fn flip_vertical_turns_a_dome_into_a_dish() {
        let d = dome(21);
        let top = f64::from(d.max().expect("dome"));
        let dish = flip_vertical(&d);
        assert!(at(&dish, 10, 10).abs() < 1e-6);
        assert!(at(&dish, 10, 1) > at(&dish, 10, 5));
        assert!((at(&dish, 10, 0) - top).abs() < 1e-5);
        assert!(dish.z[0].is_nan());
    }

    #[test]
    fn flip_vertical_twice_restores_a_field_whose_lowest_is_zero() {
        let d = solid(5, 5, |i, j| (i + j) as f32);
        assert_eq!(flip_vertical(&flip_vertical(&d)), d);
        let empty = Field::empty(3, 3, 1.0, Point::ORIGIN);
        assert_eq!(flip_vertical(&empty), empty);
    }

    #[test]
    fn scale_heights_scales_then_offsets() {
        let f = solid(3, 3, |i, _| i as f32);
        let out = scale_heights(
            &f,
            &ScaleParams {
                scale: 2.0,
                base_offset: 1.5,
            },
        )
        .expect("scale");
        assert!((at(&out, 0, 0) - 1.5).abs() < 1e-6);
        assert!((at(&out, 2, 1) - 5.5).abs() < 1e-6);
    }

    #[test]
    fn scale_heights_rejects_non_numbers() {
        let f = solid(3, 3, |_, _| 1.0);
        for p in [
            ScaleParams {
                scale: f64::NAN,
                base_offset: 0.0,
            },
            ScaleParams {
                scale: 1.0,
                base_offset: f64::INFINITY,
            },
        ] {
            assert!(matches!(
                scale_heights(&f, &p),
                Err(CoreError::Validation(_))
            ));
        }
    }

    #[test]
    fn emboss_squashes_to_the_height_and_keeps_proportions() {
        let d = dome(15);
        let out = emboss(&d, &EmbossParams { max_height: 1.0 }).expect("emboss");
        assert!((f64::from(out.max().expect("dome")) - 1.0).abs() < 1e-6);
        let ratio = at(&d, 7, 3) / at(&d, 7, 7);
        assert!((at(&out, 7, 3) / at(&out, 7, 7) - ratio).abs() < 1e-5);
        assert!(out.z[0].is_nan());
    }

    #[test]
    fn emboss_rejects_bad_heights_and_flat_components() {
        let d = dome(9);
        assert!(matches!(
            emboss(&d, &EmbossParams { max_height: 0.0 }),
            Err(CoreError::Validation(_))
        ));
        let flat = solid(3, 3, |_, _| 0.0);
        assert!(matches!(
            emboss(&flat, &EmbossParams::default()),
            Err(CoreError::Validation(_))
        ));
    }
}
