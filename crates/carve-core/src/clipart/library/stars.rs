//! Hearts and stars, as vectors and as reliefs.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::model::shape::Profile;

use super::art::{self, p, Ring};
use super::{Adder, Entry};
use crate::clipart::samples;

const SOURCE: &str = "stars.rs";

/// The classic heart curve, `size` millimetres tall, centred.
pub(super) fn heart(size: f64) -> Ring {
    let ring: Ring = (0..240)
        .map(|k| {
            let t = TAU * k as f64 / 240.0;
            let x = 16.0 * t.sin().powi(3);
            let y =
                13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            p(x, y)
        })
        .collect();
    art::fit(&[ring], size).remove(0)
}

/// A heart stretched by `sx`, `sy`.
fn heart_stretched(size: f64, sx: f64, sy: f64) -> Ring {
    art::moved(&heart(size), &art::scaled(sx, sy))
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    stars_2d(out);
    stars_3d(out);
    hearts(out);
}

fn stars_2d(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Hearts and Stars/Stars",
        source: SOURCE,
    };
    let names = [
        (4, "Four"),
        (5, "Five"),
        (6, "Six"),
        (7, "Seven"),
        (8, "Eight"),
        (10, "Ten"),
        (12, "Twelve"),
    ];
    for (n, word) in names {
        add.vectors(
            &format!("{word} Point Star"),
            &["star", "points"],
            move || {
                let inner = if n == 5 { 0.38 } else { 0.5 };
                Ok(art::paths(&[art::star(n, 30.0, 30.0 * inner, 0.0)]))
            },
        );
        add.vectors(
            &format!("{word} Point Slim Star"),
            &["star", "points", "slim"],
            move || Ok(art::paths(&[art::star(n, 30.0, 30.0 * 0.28, 0.0)])),
        );
    }
    add.vectors("Outlined Star", &["star", "outline", "double"], || {
        let outer = art::star(5, 30.0, 12.0, 0.0);
        let inner = art::star(5, 24.0, 9.6, 0.0);
        Ok(art::paths(&[outer, inner]))
    });
    add.vectors(
        "Nautical Star",
        &["star", "nautical", "facets", "two tone"],
        || {
            // The star, and inside it every other half point as its own facet,
            // set in from the edges so the loops never touch.
            let (ro, ri) = (30.0, 12.0);
            let mut rings = vec![art::star(5, ro, ri, 0.0)];
            for k in 0..5 {
                let t = PI / 2.0 + TAU * k as f64 / 5.0;
                let tip = p(ro * t.cos(), ro * t.sin());
                let notch = p(ri * (t + PI / 5.0).cos(), ri * (t + PI / 5.0).sin());
                rings.extend(art::offset(&[vec![p(0.0, 0.0), tip, notch]], -1.2));
            }
            Ok(art::paths(&rings))
        },
    );
    add.vectors("Starburst", &["star", "burst", "rays", "sun"], || {
        Ok(art::paths(&[art::star(16, 30.0, 20.0, 0.0)]))
    });
    add.vectors("Sunburst", &["star", "burst", "rays", "sun"], || {
        Ok(art::paths(&[
            art::star(24, 30.0, 23.0, 0.0),
            art::circle(p(0.0, 0.0), 15.0, 120),
        ]))
    });
    add.vectors("Star in Circle", &["star", "circle", "badge"], || {
        Ok(art::paths(&[
            art::circle(p(0.0, 0.0), 30.0, 180),
            art::star(5, 27.0, 10.5, 0.0),
        ]))
    });
    add.vectors(
        "Sheriff Star",
        &["star", "badge", "sheriff", "western"],
        || {
            let mut rings = vec![art::star(6, 28.0, 17.0, 0.0)];
            for k in 0..6 {
                let t = PI / 2.0 + TAU * k as f64 / 6.0;
                rings.push(art::circle(p(30.0 * t.cos(), 30.0 * t.sin()), 3.5, 48));
            }
            let shape = art::union(&rings)?;
            let mut all = shape;
            all.push(art::circle(p(0.0, 0.0), 11.0, 96));
            Ok(art::paths(&all))
        },
    );
    add.vectors("Star Cluster", &["star", "stars", "group"], || {
        let big = art::star(5, 20.0, 8.0, 0.0);
        let a = art::moved(&art::star(5, 10.0, 4.0, 0.2), &art::place(0.0, 20.0, 18.0));
        let b = art::moved(&art::star(5, 8.0, 3.2, -0.3), &art::place(0.0, -21.0, 16.0));
        let c = art::moved(&art::star(5, 6.0, 2.4, 0.1), &art::place(0.0, 18.0, -18.0));
        Ok(art::paths(&[big, a, b, c]))
    });
}

fn stars_3d(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Hearts and Stars/Stars",
        source: SOURCE,
    };
    add.relief("Star Dome", &["star", "dome", "rounded"], || {
        samples::sample("Star Dome")
            .map(|c| c.field)
            .ok_or_else(|| CoreError::validation("the star sample is missing"))
    });
    for (n, word) in [(4, "Four"), (5, "Five"), (6, "Six"), (8, "Eight")] {
        add.relief(
            &format!("Faceted {word} Point Star"),
            &["star", "faceted", "angular"],
            move || {
                let inner = if n == 5 { 0.4 } else { 0.5 };
                art::shape(
                    &[art::star(n, 25.0, 25.0 * inner, 0.0)],
                    Profile::Angular,
                    25.0,
                    0.0,
                )
            },
        );
    }
    add.relief("Rounded Star", &["star", "rounded", "puffy"], || {
        art::puffy(&[art::star(5, 25.0, 11.0, 0.0)], 6.0)
    });
    add.relief(
        "Nautical Star Relief",
        &["star", "nautical", "faceted", "two tone"],
        || {
            let (ro, ri) = (25.0, 10.0);
            Ok(art::field_fn(52.0, 52.0, move |x, y| {
                let r = x.hypot(y);
                let theta = (y.atan2(x) - PI / 2.0).rem_euclid(TAU);
                let step = TAU / 5.0;
                let local = theta.rem_euclid(step);
                let u = if local > step / 2.0 {
                    local - step
                } else {
                    local
                };
                // Distance from the spike's axis to its edge at radius r.
                let tip = p(0.0, ro);
                let notch = p(ri * (step / 2.0).sin(), ri * (step / 2.0).cos());
                let (px, py) = (r * u.abs().sin(), r * u.cos());
                let cross = (notch.x - tip.x) * (py - tip.y) - (notch.y - tip.y) * (px - tip.x);
                if cross > 0.0 {
                    return f64::NAN;
                }
                let span = ((notch.x - tip.x).powi(2) + (notch.y - tip.y).powi(2)).sqrt();
                let d = -cross / span;
                let h = 1.0 + (d * 0.6).min(6.0);
                if u > 0.0 {
                    h
                } else {
                    h * 0.7 + 0.3
                }
            }))
        },
    );
    add.relief(
        "Starburst Relief",
        &["star", "burst", "sun", "rays"],
        || {
            art::shape(
                &[art::star(16, 25.0, 15.0, 0.0)],
                Profile::Angular,
                25.0,
                0.0,
            )
        },
    );
}

fn hearts(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Hearts and Stars/Hearts",
        source: SOURCE,
    };
    add.vectors("Heart", &["heart", "love", "valentine"], || {
        Ok(art::paths(&[heart(50.0)]))
    });
    add.vectors("Tall Heart", &["heart", "love"], || {
        Ok(art::paths(&[heart_stretched(50.0, 0.8, 1.1)]))
    });
    add.vectors("Wide Heart", &["heart", "love"], || {
        Ok(art::paths(&[heart_stretched(50.0, 1.2, 0.9)]))
    });
    add.vectors("Heart Outline", &["heart", "outline", "double"], || {
        Ok(art::paths(&art::band(&heart(50.0), 4.0)?))
    });
    add.vectors("Two Hearts", &["heart", "pair", "wedding", "love"], || {
        let a = art::moved(&heart(40.0), &art::place(0.25, -10.0, 2.0));
        let b = art::moved(&heart(34.0), &art::place(-0.25, 13.0, -3.0));
        let both = art::merge(&[art::band(&a, 3.0)?, art::band(&b, 3.0)?])?;
        Ok(art::paths(&both))
    });
    add.vectors("Heart of Hearts", &["heart", "ring", "love"], || {
        let big = art::resample(&heart(60.0), 18);
        let mut rings = Vec::new();
        for at in big {
            rings.push(art::moved(&heart(7.0), &art::place(0.0, at.x, at.y)));
        }
        Ok(art::paths(&rings))
    });
    add.vectors(
        "Heart with Arrow",
        &["heart", "arrow", "cupid", "valentine"],
        || {
            let h = heart(44.0);
            let shaft = art::stroke(&[p(-32.0, -18.0), p(28.0, 16.0)], 2.0)?;
            let head = vec![p(34.0, 19.4), p(24.0, 18.5), p(28.5, 10.5)];
            let feather = art::moved(
                &art::rect(p(0.0, 0.0), 9.0, 5.0),
                &art::place(0.52, -30.0, -17.0),
            );
            let arrow = art::union(&[shaft, vec![head, feather]].concat())?;
            let all = art::union(&[vec![h], arrow].concat())?;
            Ok(art::paths(&all))
        },
    );
    add.relief("Puffy Heart", &["heart", "rounded", "love"], || {
        art::puffy(&[heart(50.0)], 7.0)
    });
    add.relief("Faceted Heart", &["heart", "angular", "faceted"], || {
        art::shape(&[heart(50.0)], Profile::Angular, 25.0, 0.0)
    });
    add.relief("Smooth Heart", &["heart", "smooth", "love"], || {
        art::shape(&[heart(50.0)], Profile::Smooth, 90.0, 6.0)
    });
    add.relief("Framed Heart", &["heart", "frame", "border"], || {
        let h = heart(50.0);
        art::by_depth(&[h], |d| {
            if d < 3.0 {
                2.0 * art::round_cap(d.min(3.0 - d).max(0.0) / 1.5) + 1.0
            } else {
                1.0 + 3.0 * art::round_cap(((d - 3.0) / 8.0).min(1.0))
            }
        })
    });
    add.relief("Double Heart", &["heart", "pair", "love"], || {
        let a = art::moved(&heart(40.0), &art::place(0.25, -10.0, 2.0));
        let b = art::moved(&heart(34.0), &art::place(-0.25, 13.0, -3.0));
        let fa = art::puffy(&[a], 6.0)?;
        let fb = art::puffy(&[b], 5.0)?;
        Ok(art::highest(&fa, &fb))
    });
}
