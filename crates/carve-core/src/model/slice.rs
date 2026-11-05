//! Slicing the relief model into Z slabs, for models too deep to cut from
//! one board: each slab becomes its own component, dropped to the zero
//! plane, and the slabs added together rebuild the model.
//!
//! Slabs start at the model's zero plane. With one thickness every slab is
//! that thick; with a list, slab `k` takes the `k`th thickness and the last
//! one repeats until the top of the model is reached. From the top, the
//! same thicknesses are taken down from the model's top and the part slab
//! left over sits at the bottom. A number of slices divides the model's
//! height into that many equal slabs. A list of slice heights (the Z
//! values the model is cut at, as the form's Slice Heights list edits them)
//! gives the slab boundaries directly.

use serde::Serialize;

use crate::error::CoreError;

use super::field::Field;

/// Most slabs one slice may make.
pub const MAX_SLABS: usize = 200;

/// One Z band of the model, in millimetres above the zero plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slab {
    /// Position in the stack, 1 for the bottom slab.
    pub index: usize,
    /// Bottom of the band.
    pub bottom: f64,
    /// Top of the band.
    pub top: f64,
}

impl Slab {
    /// Thickness of the band.
    pub fn thickness(&self) -> f64 {
        self.top - self.bottom
    }
}

/// The slabs that cover the model from the zero plane up to `max_height`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an empty list, a thickness that is
/// not a number more than 0, a model with nothing above the zero plane, or
/// more than [`MAX_SLABS`] slabs.
pub fn plan(max_height: f64, thicknesses: &[f64]) -> Result<Vec<Slab>, CoreError> {
    check(max_height, thicknesses)?;
    let mut slabs = Vec::new();
    let mut bottom = 0.0;
    // A top within a micron of a slab boundary does not start another slab.
    while bottom < max_height - 1e-3 {
        if slabs.len() == MAX_SLABS {
            return Err(CoreError::validation(format!(
                "those thicknesses would make more than {MAX_SLABS} slices"
            )));
        }
        let k = slabs.len();
        let thickness = thicknesses[k.min(thicknesses.len() - 1)];
        slabs.push(Slab {
            index: k + 1,
            bottom,
            top: bottom + thickness,
        });
        bottom += thickness;
    }
    Ok(slabs)
}

fn check(max_height: f64, thicknesses: &[f64]) -> Result<(), CoreError> {
    if thicknesses.is_empty() {
        return Err(CoreError::validation("give at least one slice thickness"));
    }
    if thicknesses.iter().any(|t| !t.is_finite() || *t <= 0.0) {
        return Err(CoreError::validation(
            "every slice thickness must be a number more than 0",
        ));
    }
    if !max_height.is_finite() || max_height <= 0.0 {
        return Err(CoreError::validation(
            "there is no model above the zero plane to slice",
        ));
    }
    Ok(())
}

/// The slabs that cover the model from its top at `max_height` down to the
/// zero plane: the first thickness is the top slab, the next one the slab
/// under it, the last one repeating, and the slab that reaches the zero
/// plane keeps what is left. Numbered from the bottom, as [`plan`] does.
///
/// # Errors
/// As [`plan`].
pub fn plan_from_top(max_height: f64, thicknesses: &[f64]) -> Result<Vec<Slab>, CoreError> {
    check(max_height, thicknesses)?;
    let mut bands = Vec::new();
    let mut top = max_height;
    // A bottom within a micron of the zero plane does not start another slab.
    while top > 1e-3 {
        if bands.len() == MAX_SLABS {
            return Err(CoreError::validation(format!(
                "those thicknesses would make more than {MAX_SLABS} slices"
            )));
        }
        let thickness = thicknesses[bands.len().min(thicknesses.len() - 1)];
        let bottom = (top - thickness).max(0.0);
        bands.push((bottom, top));
        top = bottom;
    }
    Ok(bands
        .into_iter()
        .rev()
        .enumerate()
        .map(|(k, (bottom, top))| Slab {
            index: k + 1,
            bottom,
            top,
        })
        .collect())
}

/// `count` slabs of equal thickness from the zero plane to `max_height`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a count of 0 or more than
/// [`MAX_SLABS`], or a model with nothing above the zero plane.
pub fn plan_count(max_height: f64, count: usize) -> Result<Vec<Slab>, CoreError> {
    if count == 0 || count > MAX_SLABS {
        return Err(CoreError::validation(format!(
            "the number of slices must be from 1 to {MAX_SLABS}"
        )));
    }
    check(max_height, &[max_height])?;
    let step = max_height / count as f64;
    Ok((0..count)
        .map(|k| Slab {
            index: k + 1,
            bottom: step * k as f64,
            top: if k + 1 == count {
                max_height
            } else {
                step * (k + 1) as f64
            },
        })
        .collect())
}

/// The slabs between the slice heights `heights` (the Z values the model is
/// cut at) from the zero plane to `max_height`, in any order. Heights at or
/// below the zero plane or at or above the top (within a micron) and
/// repeats are left out, so an empty list gives one slab of the whole model.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a height that is not a number, a
/// model with nothing above the zero plane, or more than [`MAX_SLABS`] slabs.
pub fn plan_heights(max_height: f64, heights: &[f64]) -> Result<Vec<Slab>, CoreError> {
    check(max_height, &[max_height])?;
    if heights.iter().any(|h| !h.is_finite()) {
        return Err(CoreError::validation("every slice height must be a number"));
    }
    let mut cuts: Vec<f64> = heights
        .iter()
        .copied()
        .filter(|h| *h > 1e-3 && *h < max_height - 1e-3)
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);
    if cuts.len() >= MAX_SLABS {
        return Err(CoreError::validation(format!(
            "those heights would make more than {MAX_SLABS} slices"
        )));
    }
    let mut bottom = 0.0;
    let mut slabs = Vec::with_capacity(cuts.len() + 1);
    for top in cuts.into_iter().chain(std::iter::once(max_height)) {
        slabs.push(Slab {
            index: slabs.len() + 1,
            bottom,
            top,
        });
        bottom = top;
    }
    Ok(slabs)
}

/// The part of `field` inside `slab`, dropped so the slab's bottom sits on
/// the zero plane: heights above the top are cut to the slab's thickness,
/// and cells at or below the bottom are emptied. Cropped to the cells that
/// remain, or `None` when the model does not reach the slab.
pub fn slab_of(field: &Field, slab: &Slab) -> Option<Field> {
    let (bottom, top) = (slab.bottom as f32, slab.top as f32);
    let mut out = field.clone();
    for v in &mut out.z {
        *v = if v.is_nan() || *v <= bottom {
            f32::NAN
        } else {
            v.min(top) - bottom
        };
    }
    out.cropped()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn steps() -> Field {
        // Heights 0, 1, ... 9 along X.
        let mut field = Field::empty(10, 1, 1.0, Point::new(0.0, 0.0));
        for (i, v) in field.z.iter_mut().enumerate() {
            *v = i as f32;
        }
        field
    }

    #[test]
    fn one_thickness_covers_the_model_in_equal_slabs() {
        let slabs = plan(9.0, &[4.0]).expect("plan");
        assert_eq!(slabs.len(), 3);
        assert_eq!((slabs[0].bottom, slabs[0].top), (0.0, 4.0));
        assert_eq!((slabs[2].bottom, slabs[2].top), (8.0, 12.0));
        assert_eq!(slabs[2].index, 3);
    }

    #[test]
    fn a_list_of_thicknesses_repeats_its_last_value() {
        let slabs = plan(10.0, &[1.0, 2.0, 3.0]).expect("plan");
        let thick: Vec<f64> = slabs.iter().map(Slab::thickness).collect();
        assert_eq!(thick, vec![1.0, 2.0, 3.0, 3.0, 3.0]);
    }

    #[test]
    fn an_exact_fit_makes_no_empty_slab() {
        assert_eq!(plan(8.0, &[4.0]).expect("plan").len(), 2);
    }

    #[test]
    fn bad_plans_are_refused() {
        assert!(plan(5.0, &[]).is_err());
        assert!(plan(5.0, &[0.0]).is_err());
        assert!(plan(5.0, &[f64::NAN]).is_err());
        assert!(plan(0.0, &[1.0]).is_err());
        assert!(plan(1000.0, &[1.0]).is_err());
    }

    #[test]
    fn slabs_drop_to_the_zero_plane_and_add_back_to_the_model() {
        let field = steps();
        let slabs = plan(9.0, &[4.0]).expect("plan");
        let parts: Vec<Field> = slabs.iter().filter_map(|s| slab_of(&field, s)).collect();
        assert_eq!(parts.len(), 3);
        // The bottom slab holds cells 1..9, capped at 4.
        assert_eq!(parts[0].width, 9);
        assert_eq!(parts[0].max(), Some(4.0));
        // The top slab holds cells 9 only, one high.
        assert_eq!(parts[2].width, 1);
        assert_eq!(parts[2].max(), Some(1.0));
        for i in 0..10 {
            let p = field.centre(i, 0);
            let total: f32 = parts.iter().map(|f| f.at(p)).filter(|v| !v.is_nan()).sum();
            assert_eq!(total, field.z[i], "cell {i}");
        }
    }

    #[test]
    fn a_slab_above_the_model_is_empty() {
        let slab = Slab {
            index: 1,
            bottom: 20.0,
            top: 30.0,
        };
        assert!(slab_of(&steps(), &slab).is_none());
    }

    #[test]
    fn from_top_leaves_the_part_slab_at_the_bottom() {
        let slabs = plan_from_top(9.0, &[4.0]).expect("plan");
        let bands: Vec<(f64, f64)> = slabs.iter().map(|s| (s.bottom, s.top)).collect();
        assert_eq!(bands, vec![(0.0, 1.0), (1.0, 5.0), (5.0, 9.0)]);
        let index: Vec<usize> = slabs.iter().map(|s| s.index).collect();
        assert_eq!(index, vec![1, 2, 3]);
        assert_eq!(plan_from_top(8.0, &[4.0]).expect("exact").len(), 2);
        assert!(plan_from_top(0.0, &[4.0]).is_err());
    }

    #[test]
    fn a_number_of_slices_divides_the_height_evenly() {
        // An example: 3.96 in into 3 slices of 1.32 in.
        let max = 3.96 * 25.4;
        let slabs = plan_count(max, 3).expect("plan");
        assert_eq!(slabs.len(), 3);
        for slab in &slabs {
            assert!((slab.thickness() - 1.32 * 25.4).abs() < 1e-9);
        }
        assert_eq!(slabs[2].top, max);
        assert!(plan_count(max, 0).is_err());
        assert!(plan_count(max, MAX_SLABS + 1).is_err());
        assert!(plan_count(0.0, 2).is_err());
    }

    #[test]
    fn slice_heights_give_the_slab_boundaries() {
        let slabs = plan_heights(10.0, &[7.0, 2.5, 7.0005, 0.0, 12.0]).expect("plans");
        let bands: Vec<(usize, f64, f64)> =
            slabs.iter().map(|s| (s.index, s.bottom, s.top)).collect();
        assert_eq!(bands, vec![(1, 0.0, 2.5), (2, 2.5, 7.0), (3, 7.0, 10.0)]);
        // No heights: the whole model is one slab.
        let whole = plan_heights(10.0, &[]).expect("plans");
        assert_eq!(whole.len(), 1);
        assert_eq!((whole[0].bottom, whole[0].top), (0.0, 10.0));
        assert!(plan_heights(10.0, &[f64::NAN]).is_err());
        assert!(plan_heights(0.0, &[1.0]).is_err());
        let many: Vec<f64> = (1..=MAX_SLABS).map(|k| k as f64 * 0.01).collect();
        assert!(plan_heights(10.0, &many).is_err());
    }
}
