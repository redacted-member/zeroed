//! Dynamic fade and tilt: component properties set by two anchor points
//! picked in the 3D view. Like the shape height and base height they change
//! how the component bakes, never its stored heights, until the component
//! properties are baked into it.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;

use super::field::Field;

/// Fades a component's heights down from `from` towards `to`: full height
/// at and behind `from`, reduced by `amount` at and beyond `to`, linearly in
/// between along the line from one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fade {
    /// Job position (mm) that keeps its height.
    pub from: Point,
    /// Job position (mm) the heights fade down towards.
    pub to: Point,
    /// How much of the height goes at `to`, 0 (none) to 1 (all of it).
    pub amount: f64,
}

/// Tilts a component: a plane through `pivot` rising at `angle_deg`
/// towards `toward` is added to every height it has.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tilt {
    /// Job position (mm) that stays where it is.
    pub pivot: Point,
    /// Job position (mm) the component rises towards.
    pub toward: Point,
    /// Angle of the plane, degrees above flat, strictly between -90 and 90.
    pub angle_deg: f64,
}

fn unit_between(a: Point, b: Point, what: &str) -> Result<(f64, f64, f64), CoreError> {
    if !(a.is_finite() && b.is_finite()) {
        return Err(CoreError::validation(format!(
            "the {what} anchor points must be numbers"
        )));
    }
    let length = a.distance(b);
    if length < 1e-6 {
        return Err(CoreError::validation(format!(
            "the two {what} anchor points must be apart"
        )));
    }
    Ok(((b.x - a.x) / length, (b.y - a.y) / length, length))
}

impl Fade {
    /// Rejects anchors on top of each other and amounts outside 0 to 1.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the bad value.
    pub fn check(&self) -> Result<(), CoreError> {
        unit_between(self.from, self.to, "fade")?;
        if !(0.0..=1.0).contains(&self.amount) {
            return Err(CoreError::validation(
                "the fade must be from 0 to 100 percent",
            ));
        }
        Ok(())
    }

    /// What the heights at `p` are multiplied by, 1 - amount to 1.
    pub fn factor(&self, p: Point) -> f64 {
        let Ok((ux, uy, length)) = unit_between(self.from, self.to, "fade") else {
            return 1.0;
        };
        let along = ((p.x - self.from.x) * ux + (p.y - self.from.y) * uy) / length;
        1.0 - self.amount * along.clamp(0.0, 1.0)
    }
}

impl Tilt {
    /// Rejects anchors on top of each other and angles of 90 degrees or
    /// more either way.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the bad value.
    pub fn check(&self) -> Result<(), CoreError> {
        unit_between(self.pivot, self.toward, "tilt")?;
        if !(self.angle_deg.is_finite() && self.angle_deg.abs() < 90.0) {
            return Err(CoreError::validation(
                "the tilt angle must be between -90 and 90 degrees",
            ));
        }
        Ok(())
    }

    /// Height the tilt adds at `p` (mm); below zero behind the pivot.
    pub fn rise(&self, p: Point) -> f64 {
        let Ok((ux, uy, _)) = unit_between(self.pivot, self.toward, "tilt") else {
            return 0.0;
        };
        let along = (p.x - self.pivot.x) * ux + (p.y - self.pivot.y) * uy;
        along * self.angle_deg.to_radians().tan()
    }
}

/// A component's height at `p` from its stored height `v`: scaled, raised
/// by the base height, faded, then tilted.
pub fn apply(
    v: f32,
    p: Point,
    height_scale: f64,
    base_height: f64,
    fade: Option<&Fade>,
    tilt: Option<&Tilt>,
) -> f32 {
    let mut h = f64::from(v) * height_scale + base_height;
    if let Some(fade) = fade {
        h *= fade.factor(p);
    }
    if let Some(tilt) = tilt {
        h += tilt.rise(p);
    }
    h as f32
}

/// The field with the dynamic properties applied to every height, as
/// baking the component's properties into it leaves it.
pub fn bake_field(
    field: &Field,
    height_scale: f64,
    base_height: f64,
    fade: Option<&Fade>,
    tilt: Option<&Tilt>,
) -> Field {
    let mut out = field.clone();
    for j in 0..field.height {
        for i in 0..field.width {
            let index = j * field.width + i;
            let v = field.z[index];
            if !v.is_nan() {
                out.z[index] = apply(v, field.centre(i, j), height_scale, base_height, fade, tilt);
            }
        }
    }
    out
}

/// Moves an anchor point the way a component transform moves the
/// component: scaled and turned about `centre`, then offset.
pub fn move_point(
    p: Point,
    centre: Point,
    scale: (f64, f64),
    rotate_deg: f64,
    offset: Point,
) -> Point {
    let (sin, cos) = rotate_deg.to_radians().sin_cos();
    let (x, y) = ((p.x - centre.x) * scale.0, (p.y - centre.y) * scale.1);
    Point::new(
        centre.x + x * cos - y * sin + offset.x,
        centre.y + x * sin + y * cos + offset.y,
    )
}

/// Moves the fade and tilt anchors of `extra` the way
/// [`super::transform::transform`] moves `field` (the heights before the
/// move) with `params`, so both stay where they were on the component.
pub fn follow_transform(
    extra: &mut super::tree::ComponentExtra,
    field: &Field,
    params: &super::transform::TransformParams,
) {
    if extra.fade.is_none() && extra.tilt.is_none() {
        return;
    }
    let centre = params.centre.unwrap_or_else(|| {
        Point::new(
            field.origin.x + (field.width as f64 - 1.0) * field.cell / 2.0,
            field.origin.y + (field.height as f64 - 1.0) * field.cell / 2.0,
        )
    });
    let scale = (params.scale_x, params.scale_y);
    let map = |p: Point| move_point(p, centre, scale, params.rotate_deg, params.offset);
    if let Some(fade) = &mut extra.fade {
        fade.from = map(fade.from);
        fade.to = map(fade.to);
    }
    if let Some(tilt) = &mut extra.tilt {
        tilt.pivot = map(tilt.pivot);
        tilt.toward = map(tilt.toward);
    }
}

/// Moves the fade and tilt anchors of `extra` through the affine `m`.
pub fn follow_affine(extra: &mut super::tree::ComponentExtra, m: &crate::geom::Mat3) {
    if let Some(fade) = &mut extra.fade {
        fade.from = m.apply(fade.from);
        fade.to = m.apply(fade.to);
    }
    if let Some(tilt) = &mut extra.tilt {
        tilt.pivot = m.apply(tilt.pivot);
        tilt.toward = m.apply(tilt.toward);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_keeps_the_first_anchor_and_lowers_towards_the_second() {
        let fade = Fade {
            from: Point::new(0.0, 0.0),
            to: Point::new(10.0, 0.0),
            amount: 0.5,
        };
        fade.check().expect("good");
        assert!((fade.factor(Point::new(-5.0, 3.0)) - 1.0).abs() < 1e-12);
        assert!((fade.factor(Point::new(0.0, 7.0)) - 1.0).abs() < 1e-12);
        assert!((fade.factor(Point::new(5.0, -2.0)) - 0.75).abs() < 1e-12);
        assert!((fade.factor(Point::new(10.0, 0.0)) - 0.5).abs() < 1e-12);
        assert!((fade.factor(Point::new(30.0, 0.0)) - 0.5).abs() < 1e-12);
        let h = apply(4.0, Point::new(10.0, 0.0), 1.0, 2.0, Some(&fade), None);
        assert!((h - 3.0).abs() < 1e-6, "base and shape fade together: {h}");
    }

    #[test]
    fn a_tilt_rises_towards_the_second_anchor_and_sinks_behind_the_pivot() {
        let tilt = Tilt {
            pivot: Point::new(5.0, 5.0),
            toward: Point::new(5.0, 15.0),
            angle_deg: 45.0,
        };
        tilt.check().expect("good");
        assert!(tilt.rise(Point::new(20.0, 5.0)).abs() < 1e-12);
        assert!((tilt.rise(Point::new(0.0, 8.0)) - 3.0).abs() < 1e-9);
        assert!((tilt.rise(Point::new(0.0, 3.0)) + 2.0).abs() < 1e-9);
    }

    #[test]
    fn bad_anchors_and_values_are_refused() {
        let same = Point::new(1.0, 1.0);
        assert!(Fade {
            from: same,
            to: same,
            amount: 0.5
        }
        .check()
        .is_err());
        assert!(Fade {
            from: same,
            to: Point::new(2.0, 1.0),
            amount: 1.5
        }
        .check()
        .is_err());
        assert!(Tilt {
            pivot: same,
            toward: Point::new(2.0, 1.0),
            angle_deg: 90.0
        }
        .check()
        .is_err());
    }

    #[test]
    fn baking_applies_every_property_and_keeps_empty_cells() {
        let mut field = Field::empty(3, 1, 1.0, Point::new(0.0, 0.0));
        field.z = vec![1.0, f32::NAN, 1.0];
        let tilt = Tilt {
            pivot: Point::new(0.0, 0.0),
            toward: Point::new(1.0, 0.0),
            angle_deg: 45.0,
        };
        let baked = bake_field(&field, 2.0, 1.0, None, Some(&tilt));
        assert!((baked.z[0] - 3.0).abs() < 1e-6);
        assert!(baked.z[1].is_nan());
        assert!((baked.z[2] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn anchors_follow_a_transform() {
        let p = move_point(
            Point::new(2.0, 0.0),
            Point::new(0.0, 0.0),
            (2.0, 1.0),
            90.0,
            Point::new(1.0, 1.0),
        );
        assert!(
            (p.x - 1.0).abs() < 1e-9 && (p.y - 5.0).abs() < 1e-9,
            "{p:?}"
        );
    }
}
