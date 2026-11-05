//! Cutter shapes: how high the bottom of a tool sits above its tip at a
//! distance from its axis.

/// The shape of a cutter's end, lengths in millimetres, angles in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cutter {
    /// Flat bottomed end mill, also used for thread mills seen from above.
    Flat {
        /// Cutting diameter.
        diameter: f64,
    },
    /// Twist drill with a conical point: the hole's full width stops the
    /// point's length above the depth the tip reaches.
    Drill {
        /// Drilled hole diameter.
        diameter: f64,
        /// Full angle across the point (118 on most twist drills).
        point_angle: f64,
    },
    /// Ball nose end mill.
    Ball {
        /// Cutting diameter.
        diameter: f64,
    },
    /// V bit with a sharp point.
    Vee {
        /// Widest diameter of the cone.
        diameter: f64,
        /// Angle between the two flanks.
        included_angle: f64,
    },
    /// V bit with a flat tip.
    Engraving {
        /// Widest diameter of the cone.
        diameter: f64,
        /// Angle between the two flanks.
        included_angle: f64,
        /// Diameter of the flat at the tip.
        flat_diameter: f64,
    },
    /// End mill with rounded corners.
    Bull {
        /// Cutting diameter.
        diameter: f64,
        /// Radius of the corners.
        corner_radius: f64,
    },
    /// Ball tip on a tapered body.
    TaperedBall {
        /// Widest diameter of the body.
        diameter: f64,
        /// Radius of the ball at the tip.
        tip_radius: f64,
        /// Angle of each flank from the tool axis.
        taper_angle: f64,
    },
}

impl Cutter {
    /// Radius of the widest part of the cutter.
    pub fn radius(&self) -> f64 {
        let diameter = match *self {
            Cutter::Flat { diameter }
            | Cutter::Drill { diameter, .. }
            | Cutter::Ball { diameter }
            | Cutter::Vee { diameter, .. }
            | Cutter::Engraving { diameter, .. }
            | Cutter::Bull { diameter, .. }
            | Cutter::TaperedBall { diameter, .. } => diameter,
        };
        (diameter / 2.0).max(0.0)
    }

    /// Steepest rise over run of the cutter's bottom, sampled across its
    /// radius; a flat end mill has none, a ball nose gets steep at its rim.
    pub fn steepest(&self) -> f64 {
        let r = self.radius();
        let n = 64;
        let mut worst: f64 = 0.0;
        for k in 0..n {
            let (r0, r1) = (r * k as f64 / n as f64, r * (k + 1) as f64 / n as f64);
            if let (Some(h0), Some(h1)) = (self.height_at(r0), self.height_at(r1)) {
                worst = worst.max((h1 - h0) / (r1 - r0).max(1e-12));
            }
        }
        worst
    }

    /// Height of the cutter's bottom above its tip at distance `r` from the
    /// axis, or `None` beyond the cutter.
    pub fn height_at(&self, r: f64) -> Option<f64> {
        let radius = self.radius();
        if r > radius + 1e-9 || r < 0.0 {
            return None;
        }
        let h = match *self {
            Cutter::Flat { .. } => 0.0,
            Cutter::Ball { .. } => radius - (radius * radius - r * r).max(0.0).sqrt(),
            Cutter::Vee { included_angle, .. } => r / half_tan(included_angle),
            Cutter::Drill { point_angle, .. } => r / half_tan(point_angle),
            Cutter::Engraving {
                included_angle,
                flat_diameter,
                ..
            } => (r - flat_diameter / 2.0).max(0.0) / half_tan(included_angle),
            Cutter::Bull { corner_radius, .. } => {
                let corner = corner_radius.clamp(0.0, radius);
                let flat = radius - corner;
                if r <= flat {
                    0.0
                } else {
                    let d = r - flat;
                    corner - (corner * corner - d * d).max(0.0).sqrt()
                }
            }
            Cutter::TaperedBall {
                tip_radius,
                taper_angle,
                ..
            } => {
                // The flank is tangent to the ball where the ball's normal
                // is square to the flank.
                let a = taper_angle.to_radians();
                let touch_r = tip_radius * a.cos();
                if r <= touch_r {
                    tip_radius - (tip_radius * tip_radius - r * r).max(0.0).sqrt()
                } else {
                    let touch_h = tip_radius - tip_radius * a.sin();
                    let t = a.tan().max(1e-9);
                    touch_h + (r - touch_r) / t
                }
            }
        };
        Some(h)
    }
}

fn half_tan(included_angle: f64) -> f64 {
    (included_angle.to_radians() / 2.0).tan().max(1e-9)
}

#[cfg(test)]
mod tests {
    use super::Cutter;

    #[test]
    fn a_drill_point_leaves_the_wall_short_of_the_tip() {
        let drill = Cutter::Drill {
            diameter: 6.0,
            point_angle: 118.0,
        };
        let point = 3.0 / 59.0f64.to_radians().tan();
        assert!((drill.height_at(3.0).expect("on the drill") - point).abs() < 1e-9);
        assert_eq!(drill.height_at(0.0), Some(0.0));
        assert!(drill.height_at(3.1).is_none());
    }
    #[test]
    fn each_cutter_bottom_rises_from_its_tip_as_its_shape_says() {
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
        let flat = Cutter::Flat { diameter: 6.0 };
        assert!(close(flat.height_at(2.9), 0.0));
        assert!(flat.height_at(3.001).is_none() && flat.steepest() == 0.0);
        // A 60 degree V rises 1 / tan(30) per millimetre out.
        let vee = Cutter::Vee {
            diameter: 12.0,
            included_angle: 60.0,
        };
        assert!(close(vee.height_at(1.0), 1.0 / 30f64.to_radians().tan()));
        assert!(close(vee.height_at(6.0), 6.0 / 30f64.to_radians().tan()));
        // A ball's bottom is its sphere: r out is r - sqrt(r^2 - x^2) up.
        let ball = Cutter::Ball { diameter: 6.0 };
        assert!(close(ball.height_at(0.0), 0.0));
        assert!(close(ball.height_at(1.5), 3.0 - (9.0f64 - 2.25).sqrt()));
        assert!(close(ball.height_at(3.0), 3.0));
        // An engraver is flat across its tip, then a V.
        let engraver = Cutter::Engraving {
            diameter: 6.0,
            included_angle: 90.0,
            flat_diameter: 1.0,
        };
        assert!(close(engraver.height_at(0.5), 0.0));
        assert!(close(engraver.height_at(1.5), 1.0));
        // A bull nose is flat to the corner, then a quarter round.
        let bull = Cutter::Bull {
            diameter: 6.0,
            corner_radius: 1.0,
        };
        assert!(close(bull.height_at(2.0), 0.0));
        assert!(close(bull.height_at(2.5), 1.0 - 0.75f64.sqrt()));
        assert!(close(bull.height_at(3.0), 1.0));
    }
}
