//! Operations that turn several components, or part of one, into a new
//! field: baking a set of components into one ("Bake components") and
//! cutting away everything below a height ("Replace below").
//!
//! Both answer a cropped [`Field`] ready to become a component's heights,
//! or `None` when nothing is left.

use crate::geom::Point;

use super::field::Field;
use super::shape::Grid;
use super::{bake_levels, Component, Level};

/// Bakes `components` into one field, as the model would bake them alone:
/// in the given order, bottom first, each meeting what is below with its
/// own combine mode and height scale, and level by level with the levels'
/// combine modes, exactly as [`bake_levels`] does. Hidden components, and
/// components on hidden levels, are left out as the bake leaves them out.
///
/// The bake runs on the first component's lattice (its cell size and cell
/// positions) over the union of the components' boxes, so the first
/// component's cells come through unchanged and the rest are resampled to
/// the nearest cell. Heights are millimetres above the zero plane with the
/// height scales applied, so the merged component should add at scale 1.
/// Cells no baked component reaches are empty, and the result is cropped
/// to the cells in use.
///
/// Answers `None` when there are no components or none of them bakes.
pub fn merge(components: &[&Component], levels: &[Level]) -> Option<Field> {
    let first = components.first()?;
    let cell = first.field.cell;
    if !(cell.is_finite() && cell > 0.0) {
        return None;
    }
    let lattice = first.field.origin;
    let first_level = levels.first().map_or(0, |l| l.id);
    let bakes = |c: &Component| {
        if !c.visible {
            return false;
        }
        if levels.is_empty() {
            return true;
        }
        let level = if levels.iter().any(|l| l.id == c.level) {
            c.level
        } else {
            first_level
        };
        levels.iter().any(|l| l.id == level && l.visible)
    };
    let baking: Vec<&Component> = components.iter().copied().filter(|c| bakes(c)).collect();

    // The union of the boxes the baking components' cells cover.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &baking {
        let f = &c.field;
        if f.width == 0 || f.height == 0 {
            continue;
        }
        x0 = x0.min(f.origin.x - f.cell / 2.0);
        y0 = y0.min(f.origin.y - f.cell / 2.0);
        x1 = x1.max(f.origin.x + (f.width as f64 - 0.5) * f.cell);
        y1 = y1.max(f.origin.y + (f.height as f64 - 0.5) * f.cell);
    }
    if x0 > x1 || y0 > y1 {
        return None;
    }
    let i0 = ((x0 - lattice.x) / cell).floor();
    let j0 = ((y0 - lattice.y) / cell).floor();
    let i1 = ((x1 - lattice.x) / cell).ceil();
    let j1 = ((y1 - lattice.y) / cell).ceil();
    let grid = Grid {
        width: (i1 - i0) as usize + 1,
        height: (j1 - j0) as usize + 1,
        cell,
        origin: Point::new(lattice.x + i0 * cell, lattice.y + j0 * cell),
    };
    // A level's mirror mode and base height show on the model only: a bake
    // takes the components as they are.
    let plain: Vec<Level> = levels
        .iter()
        .map(|l| Level {
            mirror: None,
            base_height: 0.0,
            ..l.clone()
        })
        .collect();
    let mut out = bake_levels(&plain, baking.iter().copied(), &grid);
    // The bake starts from the zero plane everywhere; keep only the cells
    // some component reached, as the bake itself decides.
    for j in 0..grid.height {
        for i in 0..grid.width {
            let p = Point::new(
                grid.origin.x + i as f64 * cell,
                grid.origin.y + j as f64 * cell,
            );
            if !baking.iter().any(|c| !c.field.at(p).is_nan()) {
                out.z[j * grid.width + i] = f32::NAN;
            }
        }
    }
    out.cropped()
}

/// "Replace below": removes every cell of `field` lower than `height`
/// (millimetres), leaving those cells empty, and keeps the rest at their
/// own absolute heights. The result is cropped to the cells left.
///
/// This is the plain clip; the kernel owner may offer "keep base" variants
/// on top of it, such as lowering what is left by `height` so it stands on
/// the zero plane, or filling the removed cells at `height` instead of
/// emptying them. A NaN `height` removes nothing.
///
/// Answers `None` when every cell is removed or the field was empty.
pub fn flatten_below(field: &Field, height: f32) -> Option<Field> {
    let mut out = field.clone();
    for v in &mut out.z {
        if *v < height {
            *v = f32::NAN;
        }
    }
    out.cropped()
}

/// What Replace Below puts where the model is lower than its height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplaceWith {
    /// Nothing: those cells become empty.
    #[default]
    Transparent,
    /// A flat plane at the height.
    Plane,
}

/// Replace Below on a component's stored heights: the component shows
/// `z * scale + base` (millimetres) at each cell, and every cell showing
/// lower than `height` is emptied or raised to it; with `lower` what is
/// left then drops by `height`. The answer is stored heights again for the
/// same `scale` and `base`, cropped to the cells left, or `None` when
/// nothing is left or `scale` is zero.
pub fn replace_below(
    field: &Field,
    scale: f64,
    base: f64,
    height: f64,
    with: ReplaceWith,
    lower: bool,
) -> Option<Field> {
    if scale == 0.0 || !scale.is_finite() || !height.is_finite() {
        return None;
    }
    let mut out = field.clone();
    for v in &mut out.z {
        if v.is_nan() {
            continue;
        }
        let mut shown = f64::from(*v) * scale + base;
        if shown < height {
            match with {
                ReplaceWith::Transparent => {
                    *v = f32::NAN;
                    continue;
                }
                ReplaceWith::Plane => shown = height,
            }
        }
        if lower {
            shown -= height;
        }
        *v = ((shown - base) / scale) as f32;
    }
    out.cropped()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Combine;

    fn component(id: u64, x0: f64, x1: f64, height: f32, combine: Combine) -> Component {
        // Cells of 1 mm centred on half millimetres, two rows.
        let width = (x1 - x0) as usize;
        let mut field = Field::empty(width, 2, 1.0, Point::new(x0 + 0.5, 0.5));
        field.z.fill(height);
        Component {
            id,
            name: format!("c{id}"),
            visible: true,
            combine,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        }
    }

    fn level(id: u64, combine: Combine) -> Level {
        Level {
            id,
            name: format!("Level {id}"),
            visible: true,
            combine,
            groups: Vec::new(),
            clip: Vec::new(),
            clip_shape: Vec::new(),
            color: None,
            material: None,
            mirror: None,
            base_height: 0.0,
        }
    }

    #[test]
    fn merging_bakes_in_order_with_each_combine_mode() {
        let base = component(1, 0.0, 6.0, 4.0, Combine::Add);
        let top = component(2, 4.0, 8.0, 1.0, Combine::Subtract);
        let merged = merge(&[&base, &top], &[Level::first()]).expect("merged");
        assert_eq!((merged.width, merged.height), (8, 2));
        assert_eq!(merged.origin, Point::new(0.5, 0.5));
        assert_eq!(merged.at(Point::new(1.5, 0.5)), 4.0);
        assert_eq!(merged.at(Point::new(4.5, 0.5)), 3.0);
        assert_eq!(merged.at(Point::new(7.5, 1.5)), -1.0);
        // The other order: the subtracting one alone, then the base added.
        let merged = merge(&[&top, &base], &[Level::first()]).expect("merged");
        assert_eq!(merged.at(Point::new(4.5, 0.5)), 3.0);
        assert_eq!(merged.at(Point::new(7.5, 0.5)), -1.0);
    }

    #[test]
    fn merging_leaves_gaps_empty_and_honours_scale_and_visibility() {
        let mut left = component(1, 0.0, 2.0, 2.0, Combine::Add);
        left.height_scale = 1.5;
        let right = component(2, 6.0, 8.0, 1.0, Combine::Add);
        let mut hidden = component(3, 0.0, 8.0, 9.0, Combine::Add);
        hidden.visible = false;
        let merged = merge(&[&left, &hidden, &right], &[]).expect("merged");
        assert_eq!(merged.width, 8);
        assert_eq!(merged.at(Point::new(0.5, 0.5)), 3.0);
        assert!(merged.at(Point::new(3.5, 0.5)).is_nan(), "gap stays empty");
        assert_eq!(merged.at(Point::new(6.5, 0.5)), 1.0);
        assert!(merge(&[&hidden], &[]).is_none());
        assert!(merge(&[], &[]).is_none());
    }

    #[test]
    fn merging_follows_the_levels() {
        let base = component(1, 0.0, 6.0, 4.0, Combine::Add);
        let mut bump = component(2, 2.0, 4.0, 1.0, Combine::Add);
        bump.level = 2;
        let levels = [level(1, Combine::Add), level(2, Combine::Subtract)];
        let merged = merge(&[&base, &bump], &levels).expect("merged");
        assert_eq!(merged.at(Point::new(2.5, 0.5)), 3.0);
        assert_eq!(merged.at(Point::new(0.5, 0.5)), 4.0);
        let mut hidden = levels.clone();
        hidden[1].visible = false;
        let merged = merge(&[&base, &bump], &hidden).expect("merged");
        assert_eq!(merged.at(Point::new(2.5, 0.5)), 4.0);
    }

    #[test]
    fn merging_keeps_the_first_components_lattice() {
        let mut a = component(1, 0.0, 2.0, 1.0, Combine::Add);
        a.field.origin = Point::new(0.75, 0.75);
        let b = component(2, 4.0, 6.0, 2.0, Combine::Add);
        let merged = merge(&[&a, &b], &[]).expect("merged");
        let offset = (merged.origin.x - 0.75) / merged.cell;
        assert!((offset - offset.round()).abs() < 1e-9, "{offset}");
        assert_eq!(merged.at(Point::new(0.75, 0.75)), 1.0);
        assert_eq!(merged.at(Point::new(4.75, 0.75)), 2.0);
    }

    #[test]
    fn replace_below_clips_and_keeps_absolute_heights() {
        let mut field = Field::empty(4, 1, 1.0, Point::new(0.5, 0.5));
        field.z = vec![1.0, 3.0, f32::NAN, 5.0];
        let clipped = flatten_below(&field, 2.0).expect("some left");
        assert_eq!((clipped.width, clipped.height), (3, 1));
        assert_eq!(clipped.origin, Point::new(1.5, 0.5));
        assert_eq!(clipped.z[0], 3.0);
        assert!(clipped.z[1].is_nan());
        assert_eq!(clipped.z[2], 5.0);
        // A cell exactly at the height stays.
        assert_eq!(flatten_below(&field, 3.0).map(|f| f.width), Some(3));
        assert!(flatten_below(&field, 6.0).is_none());
        assert_eq!(flatten_below(&field, f32::NAN), field.cropped());
    }

    #[test]
    fn replace_below_empties_or_plants_a_plane_and_can_lower() {
        let mut field = Field::empty(4, 1, 1.0, Point::new(0.5, 0.5));
        field.z = vec![1.0, 3.0, f32::NAN, 5.0];
        let plain =
            replace_below(&field, 1.0, 0.0, 2.0, ReplaceWith::Transparent, false).expect("left");
        assert_eq!(plain, flatten_below(&field, 2.0).expect("left"));
        let plane = replace_below(&field, 1.0, 0.0, 2.0, ReplaceWith::Plane, false).expect("left");
        assert_eq!(plane.width, 4);
        assert_eq!(plane.z[0], 2.0, "raised to the plane");
        assert!(plane.z[2].is_nan(), "empty cells stay empty");
        let lowered = replace_below(&field, 1.0, 0.0, 2.0, ReplaceWith::Plane, true).expect("left");
        assert_eq!(lowered.z[0], 0.0);
        assert_eq!(lowered.z[3], 3.0);
        // Shown heights count the height scale and the base: 2 z + 1.
        let scaled =
            replace_below(&field, 2.0, 1.0, 4.0, ReplaceWith::Transparent, true).expect("left");
        assert_eq!(scaled.width, 3, "the 1 mm cell showed 3 and went");
        assert_eq!(scaled.z[0] * 2.0 + 1.0, 3.0, "7 shown drops to 3");
        assert!(replace_below(&field, 1.0, 0.0, 9.0, ReplaceWith::Transparent, false).is_none());
        assert!(replace_below(&field, 0.0, 0.0, 1.0, ReplaceWith::Plane, false).is_none());
    }
}
