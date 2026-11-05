//! Custom brush sculpting: Deposit and Remove with the shape of another
//! model instead of the round brush, leaving an impression as if that
//! model were pushed into (or pulled out of) the surface.
//!
//! The brush model's heights are scaled to 0..1 and its larger side to the
//! brush diameter. Dabs are laid along the stroke as for the round brush;
//! within one stroke overlapping dabs meet at their highest, so a slow
//! stroke does not pile up. The stroke's impression, `strength * diameter
//! / 4` mm deep at the brush's highest point, then either adds onto the
//! surface (merge off) or merges with the earlier strokes, keeping the
//! higher (Deposit) or lower (Remove) of the surface and the component as
//! it was when sculpting began plus the impression (merge on).

use serde::Deserialize;

use crate::error::CoreError;
use crate::geom::Point;

use super::field::Field;
use super::sculpt::{dab_centres, grown, trimmed, Brush, BrushParams, Lattice, MAX_DABS};

/// Impression depth at full strength, as a fraction of the diameter.
const DEPTH_PER_DIAMETER: f64 = 0.25;

/// How a custom brush is held.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomBrush {
    /// Turn of the brush, degrees counter clockwise.
    pub angle: f64,
    /// Turn the brush with the stroke's direction (plus `angle`).
    pub dynamic: bool,
    /// Merge each stroke with the earlier ones instead of adding onto them.
    pub merge: bool,
}

impl Default for CustomBrush {
    fn default() -> Self {
        CustomBrush {
            angle: 0.0,
            dynamic: false,
            merge: true,
        }
    }
}

/// A model made ready as a brush: heights from 0 to 1, empty outside.
#[derive(Debug, Clone, PartialEq)]
pub struct BrushShape {
    heights: Field,
}

impl BrushShape {
    /// The brush for a model's heights.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a model with no heights.
    pub fn from_field(field: &Field) -> Result<BrushShape, CoreError> {
        let (Some(min), Some(max)) = (field.min(), field.max()) else {
            return Err(CoreError::validation("the brush model has no heights"));
        };
        let span = f64::from(max - min);
        let mut heights = field.clone();
        for v in &mut heights.z {
            if !v.is_nan() {
                *v = if span > 1e-9 {
                    ((f64::from(*v) - f64::from(min)) / span) as f32
                } else {
                    1.0
                };
            }
        }
        Ok(BrushShape { heights })
    }

    /// Width and height of the model, mm.
    fn size(&self) -> (f64, f64) {
        (
            self.heights.width as f64 * self.heights.cell,
            self.heights.height as f64 * self.heights.cell,
        )
    }

    /// The 0..1 height at `local` mm from the brush middle, in the brush's
    /// own (unscaled) frame; NaN outside it.
    fn sample(&self, local: Point) -> f32 {
        let (w, h) = self.size();
        let f = &self.heights;
        let i = ((local.x + w / 2.0) / f.cell - 0.5).round() as i64;
        let j = ((local.y + h / 2.0) / f.cell - 0.5).round() as i64;
        f.get(i, j)
    }
}

/// One Deposit or Remove stroke with a custom brush; see the module note.
/// `start` is the component as it was when sculpting began, which merged
/// strokes build on.
///
/// # Errors
/// [`CoreError::Validation`] for another brush than Deposit or Remove, bad
/// settings or points, or a stroke needing more than [`MAX_DABS`] dabs.
pub fn custom_stroke(
    field: &Field,
    start: &Field,
    points: &[Point],
    params: &BrushParams,
    shape: &BrushShape,
    custom: &CustomBrush,
) -> Result<Field, CoreError> {
    params.validate()?;
    if !matches!(params.brush, Brush::Deposit | Brush::Remove) {
        return Err(CoreError::validation(
            "a custom brush works with Deposit and Remove only",
        ));
    }
    if points.iter().any(|p| !p.is_finite()) || !custom.angle.is_finite() {
        return Err(CoreError::validation("stroke points must be numbers"));
    }
    if points.is_empty() {
        return Ok(field.clone());
    }
    let spacing = params.diameter / 4.0;
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if length / spacing >= MAX_DABS as f64 {
        return Err(CoreError::validation(
            "the stroke is too long for the brush size; use a larger brush or a shorter stroke",
        ));
    }
    let dabs = dab_centres(points, spacing);
    let (w, h) = shape.size();
    let scale = params.diameter / w.max(h).max(1e-9);
    // The brush turned any way fits in this circle.
    let radius = params.diameter / 2.0 * std::f64::consts::SQRT_2;
    let grow = params.brush == Brush::Deposit && params.grow;
    let (mut out, keep) = if grow {
        grown(field, &dabs, radius)
    } else {
        (field.clone(), None)
    };
    let lat = Lattice::of(&out);
    let depth = params.strength * params.diameter * DEPTH_PER_DIAMETER;

    // The stroke's impression: the deepest any dab reaches at each cell.
    let mut impression = vec![f32::NAN; out.z.len()];
    for (n, &c) in dabs.iter().enumerate() {
        let mut turn = custom.angle.to_radians();
        if custom.dynamic {
            let (a, b) = if n > 0 {
                (dabs[n - 1], c)
            } else if let Some(&next) = dabs.get(1) {
                (c, next)
            } else {
                (c, c)
            };
            if a.distance(b) > 0.0 {
                turn += (b.y - a.y).atan2(b.x - a.x);
            }
        }
        let (sin, cos) = (-turn).sin_cos();
        let Some((i0, j0, i1, j1)) = lat.span(c, radius) else {
            continue;
        };
        for j in j0..=j1 {
            let dy = lat.origin.y + j as f64 * lat.cell - c.y;
            for i in i0..=i1 {
                let dx = lat.origin.x + i as f64 * lat.cell - c.x;
                let local =
                    Point::new((dx * cos - dy * sin) / scale, (dx * sin + dy * cos) / scale);
                let b = shape.sample(local);
                if b.is_nan() {
                    continue;
                }
                let slot = &mut impression[j * lat.width + i];
                let deep = (f64::from(b) * depth) as f32;
                if slot.is_nan() || deep > *slot {
                    *slot = deep;
                }
            }
        }
    }

    let up = params.brush == Brush::Deposit;
    for (index, deep) in impression.iter().enumerate() {
        if deep.is_nan() {
            continue;
        }
        let at = Point::new(
            lat.origin.x + (index % lat.width) as f64 * lat.cell,
            lat.origin.y + (index / lat.width) as f64 * lat.cell,
        );
        let slot = &mut out.z[index];
        let now = if slot.is_nan() {
            if !grow {
                continue;
            }
            0.0
        } else {
            f64::from(*slot)
        };
        let deep = f64::from(*deep);
        let mut new = if custom.merge {
            let before = f64::from(start.at(at));
            let base = if before.is_nan() { now } else { before };
            if up {
                now.max(base + deep)
            } else {
                now.min(base - deep)
            }
        } else if up {
            now + deep
        } else {
            now - deep
        };
        if let Some(limit) = params.limit {
            new = if up {
                now.max(new.min(limit))
            } else {
                now.min(new.max(limit))
            };
        }
        *slot = new as f32;
    }
    Ok(match keep {
        Some(rect) => trimmed(&out, rect),
        None => out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: usize, height: usize, z: f32) -> Field {
        let mut f = Field::empty(width, height, 1.0, Point::new(0.5, 0.5));
        f.z.fill(z);
        f
    }

    /// A 4 by 2 mm bar brush: twice as wide as it is tall.
    fn bar() -> BrushShape {
        let mut f = Field::empty(4, 2, 1.0, Point::new(0.5, 0.5));
        f.z.fill(3.0);
        BrushShape::from_field(&f).expect("a brush")
    }

    fn params(brush: Brush) -> BrushParams {
        BrushParams {
            brush,
            diameter: 8.0,
            strength: 1.0,
            falloff: 0.0,
            limit: None,
            grow: false,
            mode: crate::model::sculpt::SculptMode::Normal,
        }
    }

    #[test]
    fn the_brush_shape_scales_heights_to_one() {
        let mut f = Field::empty(3, 1, 1.0, Point::new(0.5, 0.5));
        f.z = vec![2.0, 4.0, f32::NAN];
        let shape = BrushShape::from_field(&f).expect("a brush");
        assert_eq!(shape.heights.z[0], 0.0);
        assert_eq!(shape.heights.z[1], 1.0);
        assert!(shape.heights.z[2].is_nan());
        assert!(BrushShape::from_field(&Field::empty(2, 2, 1.0, Point::ORIGIN)).is_err());
    }

    #[test]
    fn a_dab_leaves_the_brush_shape_and_turns_with_the_angle() {
        let surface = flat(20, 20, 1.0);
        let c = Point::new(10.0, 10.0);
        let custom = CustomBrush::default();
        let out = custom_stroke(
            &surface,
            &surface,
            &[c],
            &params(Brush::Deposit),
            &bar(),
            &custom,
        )
        .expect("stroke");
        // 8 mm across, 4 mm tall, 2 mm deep (a quarter of the diameter).
        assert_eq!(out.at(Point::new(13.5, 10.5)), 3.0);
        assert_eq!(out.at(Point::new(10.5, 13.5)), 1.0, "outside the bar");
        let turned = CustomBrush {
            angle: 90.0,
            ..custom
        };
        let out = custom_stroke(
            &surface,
            &surface,
            &[c],
            &params(Brush::Deposit),
            &bar(),
            &turned,
        )
        .expect("stroke");
        assert_eq!(out.at(Point::new(10.5, 13.5)), 3.0, "the bar stands up");
        assert_eq!(out.at(Point::new(13.5, 10.5)), 1.0);
        let removed = custom_stroke(
            &surface,
            &surface,
            &[c],
            &params(Brush::Remove),
            &bar(),
            &custom,
        )
        .expect("stroke");
        assert_eq!(removed.at(Point::new(10.5, 10.5)), -1.0);
    }

    #[test]
    fn merged_strokes_do_not_pile_up_and_added_ones_do() {
        let surface = flat(20, 20, 1.0);
        let stroke = [Point::new(6.0, 10.0), Point::new(14.0, 10.0)];
        let merge = CustomBrush::default();
        let once = custom_stroke(
            &surface,
            &surface,
            &stroke,
            &params(Brush::Deposit),
            &bar(),
            &merge,
        )
        .expect("stroke");
        assert_eq!(
            once.at(Point::new(10.5, 10.5)),
            3.0,
            "dabs of one stroke meet at the top"
        );
        let twice = custom_stroke(
            &once,
            &surface,
            &stroke,
            &params(Brush::Deposit),
            &bar(),
            &merge,
        )
        .expect("stroke");
        assert_eq!(twice, once, "merged with the earlier stroke");
        let add = CustomBrush {
            merge: false,
            ..merge
        };
        let added = custom_stroke(
            &once,
            &surface,
            &stroke,
            &params(Brush::Deposit),
            &bar(),
            &add,
        )
        .expect("stroke");
        assert_eq!(added.at(Point::new(10.5, 10.5)), 5.0, "added onto it");
    }

    #[test]
    fn dynamic_rotation_follows_the_stroke() {
        let surface = flat(30, 30, 0.0);
        let upward = [Point::new(15.0, 11.0), Point::new(15.0, 19.0)];
        let dynamic = CustomBrush {
            dynamic: true,
            ..CustomBrush::default()
        };
        let out = custom_stroke(
            &surface,
            &surface,
            &upward,
            &params(Brush::Deposit),
            &bar(),
            &dynamic,
        )
        .expect("stroke");
        // Along the stroke the bar lies lengthways, so it is narrow across.
        assert_eq!(out.at(Point::new(18.5, 15.5)), 0.0);
        assert_eq!(out.at(Point::new(15.5, 15.5)), 2.0);
    }

    #[test]
    fn only_deposit_and_remove_take_a_custom_brush() {
        let surface = flat(4, 4, 0.0);
        let custom = CustomBrush::default();
        for brush in [Brush::Smooth, Brush::Smudge, Brush::Erase, Brush::Restore] {
            assert!(custom_stroke(
                &surface,
                &surface,
                &[Point::new(2.0, 2.0)],
                &params(brush),
                &bar(),
                &custom
            )
            .is_err());
        }
    }
}
