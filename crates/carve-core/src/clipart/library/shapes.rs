//! Shapes: domes, pyramids, beads, cones and dishes, made from formulas and
//! from Create Shape on plain outlines.

use std::f64::consts::{PI, TAU};

use crate::model::shape::Profile;

use super::art::{self, p, round_cap};
use super::{Adder, Entry};

const SOURCE: &str = "shapes.rs";

pub(super) fn entries(out: &mut Vec<Entry>) {
    domes(out);
    pyramids(out);
    beads(out);
    dishes(out);
}

fn domes(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Shapes/Domes",
        source: SOURCE,
    };
    add.relief("Round Dome", &["dome", "circle", "round"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let r = x.hypot(y) / 24.0;
            if r > 1.0 {
                f64::NAN
            } else {
                8.0 * (1.0 - r * r).sqrt()
            }
        }))
    });
    add.relief("Low Dome", &["dome", "circle", "shallow"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let r = x.hypot(y) / 24.0;
            if r > 1.0 {
                f64::NAN
            } else {
                3.0 * (1.0 - r * r)
            }
        }))
    });
    add.relief("Hemisphere", &["dome", "ball", "half sphere"], || {
        Ok(art::field_fn(40.0, 40.0, |x, y| {
            let r2 = 19.0 * 19.0 - x * x - y * y;
            if r2 < 0.0 {
                f64::NAN
            } else {
                r2.sqrt()
            }
        }))
    });
    add.relief("Oval Dome", &["dome", "ellipse", "oval"], || {
        Ok(art::field_fn(64.0, 40.0, |x, y| {
            let r = (x / 31.0).hypot(y / 19.0);
            if r > 1.0 {
                f64::NAN
            } else {
                6.0 * (1.0 - r * r).sqrt()
            }
        }))
    });
    add.relief("Pillow", &["dome", "square", "cushion"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let (u, v) = (x / 24.0, y / 24.0);
            if u.abs() > 1.0 || v.abs() > 1.0 {
                f64::NAN
            } else {
                6.0 * ((1.0 - u.powi(4)) * (1.0 - v.powi(4))).sqrt()
            }
        }))
    });
    for (name, sides, turn) in [
        ("Square Dome", 4, PI / 4.0),
        ("Hexagon Dome", 6, 0.0),
        ("Octagon Dome", 8, PI / 8.0),
    ] {
        add.relief(name, &["dome", "polygon"], move || {
            art::dome(&[art::polygon(sides, 25.0, turn)], 7.0)
        });
    }
    add.relief("Flat Top Dome", &["dome", "plateau", "button"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let r = x.hypot(y);
            if r > 24.0 {
                f64::NAN
            } else {
                5.0 * round_cap((24.0 - r) / 8.0)
            }
        }))
    });
}

fn pyramids(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Shapes/Pyramids",
        source: SOURCE,
    };
    for (name, sides, turn) in [
        ("Triangle Pyramid", 3, 0.0),
        ("Square Pyramid", 4, PI / 4.0),
        ("Pentagon Pyramid", 5, 0.0),
        ("Hexagon Pyramid", 6, 0.0),
        ("Octagon Pyramid", 8, PI / 8.0),
    ] {
        add.relief(name, &["pyramid", "polygon", "faceted"], move || {
            art::shape(
                &[art::polygon(sides, 25.0, turn)],
                Profile::Angular,
                30.0,
                0.0,
            )
        });
    }
    add.relief("Ridge", &["pyramid", "roof", "rectangle"], || {
        art::shape(
            &[art::rect(p(0.0, 0.0), 60.0, 30.0)],
            Profile::Angular,
            30.0,
            0.0,
        )
    });
    add.relief("Stepped Pyramid", &["pyramid", "ziggurat", "steps"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let d = x.abs().max(y.abs());
            if d > 24.0 {
                f64::NAN
            } else {
                (((24.0 - d) / 4.0).floor() + 1.0).min(5.0) * 1.5
            }
        }))
    });
    add.relief(
        "Faceted Gem",
        &["gem", "jewel", "faceted", "diamond"],
        || {
            let r = 22.0;
            Ok(art::field_fn(48.0, 48.0, move |x, y| {
                let theta = y.atan2(x);
                let step = TAU / 8.0;
                let local = (theta.rem_euclid(step) - step / 2.0).abs();
                let edge = r * (step / 2.0).cos() / local.cos();
                let d = x.hypot(y);
                if d > edge {
                    return f64::NAN;
                }
                let t = d / edge;
                // A flat table in the middle, crown facets round it.
                let crown = 6.0 * (1.0 - t) / 0.55;
                crown.min(6.0)
            }))
        },
    );
}

fn beads(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Shapes/Beads and Cones",
        source: SOURCE,
    };
    add.relief("Cone", &["cone", "point"], || {
        Ok(art::field_fn(40.0, 40.0, |x, y| {
            let r = x.hypot(y) / 19.5;
            if r > 1.0 {
                f64::NAN
            } else {
                8.0 * (1.0 - r)
            }
        }))
    });
    add.relief("Bead", &["bead", "ball", "sphere"], || {
        Ok(art::field_fn(20.0, 20.0, |x, y| {
            let r2 = 9.5 * 9.5 - x * x - y * y;
            if r2 < 0.0 {
                f64::NAN
            } else {
                r2.sqrt()
            }
        }))
    });
    add.relief("Egg", &["egg", "oval", "bead"], || {
        Ok(art::field_fn(30.0, 42.0, |x, y| {
            let ry = 20.0;
            let v = y / ry;
            if v.abs() > 1.0 {
                return f64::NAN;
            }
            let rx = 14.0 * (1.0 - v * v).sqrt() * (1.0 - 0.18 * v);
            let u = x / rx.max(1e-9);
            if u.abs() > 1.0 {
                f64::NAN
            } else {
                9.0 * ((1.0 - u * u) * (1.0 - v * v)).sqrt()
            }
        }))
    });
    add.relief("Ring", &["torus", "ring", "donut"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let d = x.hypot(y) - 17.0;
            let r2 = 7.0 * 7.0 - d * d;
            if r2 < 0.0 {
                f64::NAN
            } else {
                r2.sqrt()
            }
        }))
    });
    add.relief("Half Round Bar", &["bar", "cylinder", "rod"], || {
        Ok(art::field_fn(80.0, 12.0, |x, y| {
            let r2 = 5.75 * 5.75 - y * y;
            if r2 < 0.0 || x.abs() > 39.5 {
                f64::NAN
            } else {
                r2.sqrt()
            }
        }))
    });
    add.relief("Button", &["button", "holes", "sewing"], || {
        Ok(art::field_fn(40.0, 40.0, |x, y| {
            let r = x.hypot(y);
            if r > 19.5 {
                return f64::NAN;
            }
            for (hx, hy) in [(4.0, 4.0), (-4.0, 4.0), (4.0, -4.0), (-4.0, -4.0)] {
                if (x - hx).hypot(y - hy) < 2.0 {
                    return f64::NAN;
                }
            }
            if r < 15.0 {
                2.0 + 0.4 * (r / 15.0).powi(2)
            } else {
                let t = (r - 15.0) / 4.5;
                1.0 + 1.4 + 1.6 * (PI * t).sin()
            }
        }))
    });
    add.relief("Teardrop", &["drop", "teardrop", "bead"], || {
        let ring = art::mirrored(
            &[
                p(0.0, 22.0),
                p(4.0, 12.0),
                p(10.0, 0.0),
                p(11.0, -8.0),
                p(7.0, -15.0),
                p(0.0, -17.0),
            ],
            8,
        );
        art::dome(&[ring], 7.0)
    });
    add.relief("Lens", &["lens", "vesica", "almond"], || {
        Ok(art::field_fn(56.0, 26.0, |x, y| {
            let r = 30.0;
            let c = r - 12.0;
            let d1 = x.hypot(y - c);
            let d2 = x.hypot(y + c);
            if d1 > r || d2 > r {
                return f64::NAN;
            }
            let t = ((r - d1).min(r - d2) / 12.0).clamp(0.0, 1.0);
            6.0 * round_cap(t)
        }))
    });
}

fn dishes(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Shapes/Dishes",
        source: SOURCE,
    };
    add.relief("Round Dish", &["dish", "bowl", "concave"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y);
            if r > 29.5 {
                return f64::NAN;
            }
            let lip = 6.0 * round_cap((29.5 - r) / 3.0);
            let bowl = 1.5 + 4.5 * (r / 26.0).powi(2);
            if r < 26.0 {
                bowl.min(lip)
            } else {
                lip
            }
        }))
    });
    add.relief("Shallow Bowl", &["dish", "bowl", "concave"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y) / 29.5;
            if r > 1.0 {
                f64::NAN
            } else {
                1.0 + 3.0 * r * r * round_cap((1.0 - r) * 12.0)
            }
        }))
    });
    add.relief("Square Dish", &["dish", "tray", "square"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let d = x.abs().max(y.abs());
            if d > 29.5 {
                return f64::NAN;
            }
            let lip = 5.0 * round_cap((29.5 - d) / 3.0);
            if d < 26.0 {
                (1.0 + 4.0 * (d / 26.0).powi(3)).min(lip)
            } else {
                lip
            }
        }))
    });
    add.relief("Oval Platter", &["dish", "platter", "oval"], || {
        Ok(art::field_fn(80.0, 50.0, |x, y| {
            let r = (x / 39.5).hypot(y / 24.5);
            if r > 1.0 {
                return f64::NAN;
            }
            let lip = 5.0 * round_cap((1.0 - r) * 8.0);
            (1.0 + 4.0 * r.powi(4)).min(lip)
        }))
    });
    add.relief("Scalloped Dish", &["dish", "scallop", "bowl"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let theta = y.atan2(x);
            let edge = 27.0 + 2.5 * (12.0 * theta).cos().abs();
            let r = x.hypot(y);
            if r > edge {
                return f64::NAN;
            }
            let lip = 5.0 * round_cap((edge - r) / 3.0);
            (1.2 + 3.8 * (r / edge).powi(2)).min(lip)
        }))
    });
}
