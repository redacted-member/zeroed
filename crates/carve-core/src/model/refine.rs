//! Whole model and whole component refinements: smoothing with or without
//! keeping the component's outline, embossing (squashing heights while
//! keeping the fine detail), a draft skirt round the model and a ball
//! offset of the model, plus the flat zero plane.
//!
//! Every function is pure: a [`Field`] in, a new field out. Heights are
//! millimetres; empty cells are NaN.

use crate::error::CoreError;

use super::edits::{smooth, SmoothParams};
use super::field::Field;
use super::shape::Grid;

/// Widest blur, in cells, the smoothing amount reaches at 100 %.
pub const MAX_SMOOTH_CELLS: f64 = 8.0;
/// Blur passes of one smoothing; three box passes are close to a Gaussian.
const SMOOTH_PASSES: u32 = 3;
/// Heights below this (mm) are empty when a blur spreads into the background.
const SPREAD_EPSILON: f32 = 1e-3;
/// Most cells a ball offset visits round each cell; wider balls are
/// sampled on a coarser lattice so an offset stays interactive.
pub const MAX_BALL_SAMPLES: usize = 1600;

fn finite(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CoreError::validation(format!("{what} must be a number")))
    }
}

/// A flat component at height zero covering the whole model grid.
pub fn zero_plane(grid: &Grid) -> Field {
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    field.z.fill(0.0);
    field
}

/// Smooths a component by `amount` percent (0 to 100). With
/// `preserve_transparency` the blur stays inside the component's cells,
/// so its outline is kept; without it the edges blur into the background
/// and the component may grow a soft skirt.
///
/// # Errors
/// [`CoreError::Validation`] when the amount is not from 0 to 100.
pub fn smooth_amount(
    field: &Field,
    amount: f64,
    preserve_transparency: bool,
) -> Result<Field, CoreError> {
    finite(amount, "the smoothing amount")?;
    if !(0.0..=100.0).contains(&amount) {
        return Err(CoreError::validation(
            "the smoothing amount must be from 0 to 100 %",
        ));
    }
    let cells = (amount / 100.0 * MAX_SMOOTH_CELLS).round();
    if cells < 1.0 {
        return Ok(field.clone());
    }
    if preserve_transparency {
        return smooth(
            field,
            &SmoothParams {
                strength: cells * field.cell,
                iterations: SMOOTH_PASSES,
            },
        );
    }
    // Pad by how far the blur can reach, fill the background with zero so
    // the edges fall into it, blur, and keep what rose above nothing.
    let pad = cells as usize * SMOOTH_PASSES as usize;
    let (w, h) = (field.width + 2 * pad, field.height + 2 * pad);
    let origin = crate::geom::Point::new(
        field.origin.x - pad as f64 * field.cell,
        field.origin.y - pad as f64 * field.cell,
    );
    let mut padded = Field::empty(w, h, field.cell, origin);
    padded.z.fill(0.0);
    for j in 0..field.height {
        for i in 0..field.width {
            let v = field.z[j * field.width + i];
            if !v.is_nan() {
                padded.z[(j + pad) * w + i + pad] = v;
            }
        }
    }
    let mut blurred = smooth(
        &padded,
        &SmoothParams {
            strength: cells * field.cell,
            iterations: SMOOTH_PASSES,
        },
    )?;
    for (index, v) in blurred.z.iter_mut().enumerate() {
        let (i, j) = (index % w, index / w);
        let was_solid = i >= pad
            && j >= pad
            && i < pad + field.width
            && j < pad + field.height
            && !field.z[(j - pad) * field.width + (i - pad)].is_nan();
        if !was_solid && v.abs() < SPREAD_EPSILON {
            *v = f32::NAN;
        }
    }
    Ok(blurred.cropped().unwrap_or(blurred))
}

/// Embosses a component: its heights are squashed to `scale` (0 to 1] of
/// what they were while the fine detail keeps more of its depth, the way
/// a coin keeps the texture of a full relief. The broad form (a heavy
/// blur) scales by `scale`, the detail on top of it by `sqrt(scale)`;
/// `detail_smoothing` (0 to 100 %) softens the noise the stronger detail
/// brings out.
///
/// # Errors
/// [`CoreError::Validation`] for a scale outside (0, 1] or a smoothing
/// outside 0 to 100.
pub fn emboss_detail(field: &Field, scale: f64, detail_smoothing: f64) -> Result<Field, CoreError> {
    finite(scale, "the emboss height")?;
    finite(detail_smoothing, "the detail smoothing")?;
    if scale <= 0.0 || scale > 1.0 {
        return Err(CoreError::validation(
            "the emboss height must be above 0 and at most 100 %",
        ));
    }
    if !(0.0..=100.0).contains(&detail_smoothing) {
        return Err(CoreError::validation(
            "the detail smoothing must be from 0 to 100 %",
        ));
    }
    if field.width == 0 || field.height == 0 {
        return Ok(field.clone());
    }
    // The broad form: a blur about a twentieth of the component across.
    let across = field.width.max(field.height) as f64;
    let broad_cells = (across / 20.0).clamp(2.0, 64.0);
    let base = smooth(
        field,
        &SmoothParams {
            strength: broad_cells * field.cell,
            iterations: SMOOTH_PASSES,
        },
    )?;
    let mut detail = field.clone();
    for (d, b) in detail.z.iter_mut().zip(&base.z) {
        if !d.is_nan() {
            *d -= *b;
        }
    }
    let detail_cells = (detail_smoothing / 100.0 * 4.0).round();
    if detail_cells >= 1.0 {
        detail = smooth(
            &detail,
            &SmoothParams {
                strength: detail_cells * field.cell,
                iterations: 2,
            },
        )?;
    }
    let (s, g) = (scale as f32, scale.sqrt() as f32);
    let floor = field.min().map_or(0.0, |m| m.min(0.0) * s);
    let mut out = field.clone();
    for ((o, b), d) in out.z.iter_mut().zip(&base.z).zip(&detail.z) {
        if !o.is_nan() {
            *o = (b * s + d * g).max(floor);
        }
    }
    Ok(out)
}

/// Adds a draft skirt: every cell rises to at least the height of a cone
/// of wall angle `angle_deg` (from vertical) standing on each higher cell,
/// so no wall is steeper than the angle and the model can be pulled from
/// a mould. Heights only ever rise. Empty cells count as not there.
///
/// The cone is traced with a 5 by 5 chamfer in two raster passes, within
/// about 3 % of the exact distance.
///
/// # Errors
/// [`CoreError::Validation`] when the angle is not strictly between 0 and
/// 90 degrees.
pub fn draft_skirt(field: &Field, angle_deg: f64) -> Result<Field, CoreError> {
    finite(angle_deg, "the draft angle")?;
    if angle_deg <= 0.0 || angle_deg >= 90.0 {
        return Err(CoreError::validation(
            "the draft angle must be between 0 and 90 degrees",
        ));
    }
    // Height lost per cell away from a wall's top.
    let drop = field.cell / angle_deg.to_radians().tan();
    let (w, h) = (field.width as i64, field.height as i64);
    let mut z: Vec<f64> = field
        .z
        .iter()
        .map(|v| {
            if v.is_nan() {
                f64::NEG_INFINITY
            } else {
                f64::from(*v)
            }
        })
        .collect();
    // Neighbours already visited by a forward raster pass, with their
    // distances in cells; the backward pass uses the mirror images.
    const FORWARD: [(i64, i64); 8] = [
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
        (-2, -1),
        (-1, -2),
        (1, -2),
        (2, -1),
    ];
    let steps: Vec<(i64, i64, f64)> = FORWARD
        .iter()
        .map(|&(di, dj)| (di, dj, ((di * di + dj * dj) as f64).sqrt() * drop))
        .collect();
    let relax = |i: i64, j: i64, sign: i64, z: &mut Vec<f64>| {
        let here = (j * w + i) as usize;
        let mut best = z[here];
        for &(di, dj, fall) in &steps {
            let (ni, nj) = (i + sign * di, j + sign * dj);
            if ni < 0 || nj < 0 || ni >= w || nj >= h {
                continue;
            }
            let v = z[(nj * w + ni) as usize] - fall;
            if v > best {
                best = v;
            }
        }
        z[here] = best;
    };
    for j in 0..h {
        for i in 0..w {
            relax(i, j, 1, &mut z);
        }
    }
    for j in (0..h).rev() {
        for i in (0..w).rev() {
            relax(i, j, -1, &mut z);
        }
    }
    let mut out = field.clone();
    for (o, v) in out.z.iter_mut().zip(&z) {
        *o = if v.is_finite() { *v as f32 } else { f32::NAN };
    }
    Ok(out)
}

/// Offsets a model by `distance` mm the way a ball of that radius rolled
/// over it would: outwards (up and sideways) for a positive distance,
/// inwards for a negative one. With `clip_to_zero`, nothing ends below the
/// zero plane. Empty cells stay empty and do not take part.
///
/// # Errors
/// [`CoreError::Validation`] when the distance is zero or not a number.
pub fn ball_offset(field: &Field, distance: f64, clip_to_zero: bool) -> Result<Field, CoreError> {
    finite(distance, "the offset distance")?;
    if distance == 0.0 {
        return Err(CoreError::validation(
            "the offset distance must not be zero",
        ));
    }
    let r = distance.abs();
    let reach = (r / field.cell).floor() as i64;
    // Sample the ball on a lattice coarse enough to stay within the budget.
    let full = ((2 * reach + 1) * (2 * reach + 1)) as usize;
    let stride = ((full as f64 / MAX_BALL_SAMPLES as f64).sqrt().ceil() as i64).max(1);
    let mut ball: Vec<(i64, i64, f32)> = Vec::new();
    let mut di = -reach;
    while di <= reach {
        let mut dj = -reach;
        while dj <= reach {
            let d2 = ((di * di + dj * dj) as f64) * field.cell * field.cell;
            if d2 <= r * r {
                ball.push((di, dj, (r * r - d2).sqrt() as f32));
            }
            dj += stride;
        }
        di += stride;
    }
    // Always include the cell itself and the four nearest, so thin walls
    // are never skipped by a coarse lattice.
    for (di, dj) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        let d2 = ((di * di + dj * dj) as f64) * field.cell * field.cell;
        if d2 <= r * r && !ball.iter().any(|b| b.0 == di && b.1 == dj) {
            ball.push((di, dj, (r * r - d2).sqrt() as f32));
        }
    }
    let (w, h) = (field.width as i64, field.height as i64);
    let outwards = distance > 0.0;
    let mut out = field.clone();
    for j in 0..h {
        for i in 0..w {
            let here = (j * w + i) as usize;
            if field.z[here].is_nan() {
                continue;
            }
            let mut best = if outwards {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            };
            for &(di, dj, rise) in &ball {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || ni >= w || nj >= h {
                    continue;
                }
                let v = field.z[(nj * w + ni) as usize];
                if v.is_nan() {
                    continue;
                }
                if outwards {
                    best = best.max(v + rise);
                } else {
                    best = best.min(v - rise);
                }
            }
            if clip_to_zero {
                best = best.max(0.0);
            }
            out.z[here] = best;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn solid(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Field {
        let mut field = Field::empty(w, h, 1.0, Point::new(0.5, 0.5));
        for j in 0..h {
            for i in 0..w {
                field.z[j * w + i] = f(i, j);
            }
        }
        field
    }

    fn at(field: &Field, i: usize, j: usize) -> f32 {
        field.z[j * field.width + i]
    }

    #[test]
    fn the_zero_plane_covers_the_grid_at_zero() {
        let grid = Grid {
            width: 4,
            height: 3,
            cell: 2.0,
            origin: Point::new(1.0, 1.0),
        };
        let plane = zero_plane(&grid);
        assert_eq!(plane.z.len(), 12);
        assert!(plane.z.iter().all(|v| *v == 0.0));
        assert_eq!(plane.origin, grid.origin);
    }

    #[test]
    fn smoothing_nothing_changes_nothing_and_more_smooths_more() {
        let step = solid(30, 5, |i, _| if i < 15 { 0.0 } else { 10.0 });
        assert_eq!(smooth_amount(&step, 0.0, true).expect("ok"), step);
        let light = smooth_amount(&step, 25.0, true).expect("ok");
        let heavy = smooth_amount(&step, 100.0, true).expect("ok");
        let jump = |f: &Field| at(f, 15, 2) - at(f, 14, 2);
        assert!(jump(&light) < 10.0);
        assert!(jump(&heavy) < jump(&light));
        assert!(smooth_amount(&step, 120.0, true).is_err());
    }

    #[test]
    fn preserving_transparency_keeps_the_outline() {
        let mut block = Field::empty(30, 30, 1.0, Point::new(0.5, 0.5));
        for j in 10..20 {
            for i in 10..20 {
                block.z[j * 30 + i] = 5.0;
            }
        }
        let kept = smooth_amount(&block, 60.0, true).expect("ok");
        let cells = |f: &Field| f.z.iter().filter(|v| !v.is_nan()).count();
        assert_eq!(cells(&kept), 100);
        assert_eq!(at(&kept, 12, 12), 5.0, "a flat top stays flat inside");
        let spread = smooth_amount(&block, 60.0, false).expect("ok");
        assert!(cells(&spread) > 100, "edges blur into the background");
        assert!(spread.max().expect("cells") < 5.0 + 1e-4);
        let edge = spread.at(Point::new(10.5, 15.5));
        assert!(edge < 5.0 && edge > 0.0, "the wall softens: {edge}");
    }

    #[test]
    fn emboss_squashes_the_form_but_keeps_more_of_the_detail() {
        // A broad dome with fine ripples on it.
        let n = 80;
        let c = (n as f64 - 1.0) / 2.0;
        let dome = solid(n, n, |i, j| {
            let d2 = (i as f64 - c).powi(2) + (j as f64 - c).powi(2);
            let form = (40.0 - d2 / 40.0).max(0.0);
            let ripple = if (i / 2) % 2 == 0 { 0.5 } else { -0.5 };
            (form + ripple) as f32
        });
        let out = emboss_detail(&dome, 0.1, 0.0).expect("ok");
        let top = out.max().expect("cells");
        assert!(top < 8.0, "squashed to about a tenth: {top}");
        let ripple = |f: &Field| (at(f, 40, 40) - at(f, 42, 40)).abs();
        let kept = ripple(&out) / ripple(&dome);
        assert!(kept > 0.2, "detail keeps more than the scale: {kept}");
        let softened = emboss_detail(&dome, 0.1, 100.0).expect("ok");
        assert!(ripple(&softened) < ripple(&out));
        assert!(emboss_detail(&dome, 0.0, 0.0).is_err());
        assert!(emboss_detail(&dome, 1.5, 0.0).is_err());
    }

    #[test]
    fn a_draft_skirt_slopes_every_wall() {
        // A 10 mm high block standing on a zero plane.
        let block = solid(41, 41, |i, j| {
            if (15..26).contains(&i) && (15..26).contains(&j) {
                10.0
            } else {
                0.0
            }
        });
        let out = draft_skirt(&block, 45.0).expect("ok");
        assert_eq!(at(&out, 20, 20), 10.0, "the top is kept");
        // 45 degrees: one millimetre down per millimetre out.
        assert!(
            (at(&out, 12, 20) - 7.0).abs() < 0.05,
            "{}",
            at(&out, 12, 20)
        );
        assert!(
            (at(&out, 5, 20) - 0.0).abs() < 1e-6,
            "far off the skirt meets zero"
        );
        let diag = at(&out, 12, 12);
        let exact = 10.0 - (18.0f32).sqrt() * 1.0;
        assert!((diag - exact).abs() < 0.3, "{diag} vs {exact}");
        assert!(
            out.z.iter().zip(&block.z).all(|(a, b)| a >= b),
            "heights only rise"
        );
        assert!(draft_skirt(&block, 0.0).is_err());
        assert!(draft_skirt(&block, 90.0).is_err());
    }

    #[test]
    fn a_ball_offset_grows_or_shrinks_the_model() {
        let plane = solid(21, 21, |i, j| if i == 10 && j == 10 { 5.0 } else { 0.0 });
        let grown = ball_offset(&plane, 2.0, false).expect("ok");
        assert!((at(&grown, 10, 10) - 7.0).abs() < 1e-5);
        assert!(
            (at(&grown, 0, 0) - 2.0).abs() < 1e-5,
            "flat areas rise by the distance"
        );
        // Two cells off the spike, the ball touching its top sits at 5.
        assert!((at(&grown, 12, 10) - 5.0).abs() < 1e-5);
        let shrunk = ball_offset(&plane, -2.0, false).expect("ok");
        assert!((at(&shrunk, 0, 0) + 2.0).abs() < 1e-5);
        assert!(
            shrunk.max().expect("cells") <= 0.0,
            "a one cell spike vanishes"
        );
        let clipped = ball_offset(&plane, -2.0, true).expect("ok");
        assert!(clipped.min().expect("cells") >= 0.0);
        assert!(ball_offset(&plane, 0.0, false).is_err());
    }

    #[test]
    fn a_wide_ball_is_sampled_within_budget() {
        let plane = solid(50, 50, |_, _| 1.0);
        let grown = ball_offset(&plane, 200.0, false).expect("ok");
        assert!((at(&grown, 25, 25) - 201.0).abs() < 1e-3);
    }
}
