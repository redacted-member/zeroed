//! Leaves and floral: leaf outlines from width curves and lobe formulas,
//! flowers, branches and wreaths, and their reliefs with raised midribs
//! and veins.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::Point;
use crate::model::field::Field;

use super::art::{self, p, round_cap, smoothstep, Ring};
use super::{Adder, Entry};
use crate::clipart::samples;

const SOURCE: &str = "floral.rs";

/// A leaf `len` long from the origin along +Y, half width `w(t)` at
/// fraction `t` of the way from base to tip.
pub(super) fn leaf_outline(len: f64, w: impl Fn(f64) -> f64) -> Ring {
    let n = 120;
    let mut ring = Vec::with_capacity(2 * n);
    for k in 0..=n {
        let t = k as f64 / n as f64;
        ring.push(p(w(t).max(0.0), len * t));
    }
    for k in (1..n).rev() {
        let t = k as f64 / n as f64;
        ring.push(p(-w(t).max(0.0), len * t));
    }
    ring
}

/// A relief of a leaf lying from `base` to `tip`: a rounded body with a
/// raised midrib and paired side veins, `height` high.
///
/// # Errors
/// [`CoreError::Validation`] when the outline encloses no cell.
pub(super) fn veined(
    ring: &[Point],
    base: Point,
    tip: Point,
    height: f64,
) -> Result<Field, CoreError> {
    let body = art::puffy(&[ring.to_vec()], height)?;
    let axis = base.to(tip);
    let len = axis.length().max(1e-9);
    let (ax, ay) = (axis.x / len, axis.y / len);
    let mut out = body.clone();
    for j in 0..body.height {
        for i in 0..body.width {
            let k = j * body.width + i;
            let z = body.z[k];
            if z.is_nan() {
                continue;
            }
            let c = body.centre(i, j);
            let (dx, dy) = (c.x - base.x, c.y - base.y);
            let u = (dx * ax + dy * ay) / len;
            let v = (-dx * ay + dy * ax) / len;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let fade = 1.0 - u;
            let rib = 0.22 * height * (-(v * len / 0.7).powi(2)).exp() * fade.sqrt();
            let phase = 7.0 * (u - 0.9 * v.abs());
            let vein = (TAU * phase).cos().max(0.0).powi(12);
            let groove = 0.12 * height * vein * smoothstep(0.02, 0.06, v.abs());
            out.z[k] = (f64::from(z) + rib - groove).max(0.05) as f32;
        }
    }
    Ok(out)
}

/// A relief of a leaf standing along +Y from the origin, `len` long.
fn veined_up(ring: &[Point], len: f64, height: f64) -> Result<Field, CoreError> {
    veined(ring, p(0.0, 0.0), p(0.0, len), height)
}

/// Every leaf shape: name, tags and outline (standing along +Y, 60 long).
fn leaves() -> Vec<art::Design<Ring>> {
    vec![
        ("Lance Leaf", &["leaf", "lance", "narrow"], || {
            leaf_outline(60.0, |t| 11.0 * (PI * t).sin().powf(0.8) * (1.0 - 0.3 * t))
        }),
        ("Oval Leaf", &["leaf", "oval", "elliptic"], || {
            leaf_outline(60.0, |t| 18.0 * (PI * t).sin().powf(0.6))
        }),
        ("Willow Leaf", &["leaf", "willow", "narrow"], || {
            leaf_outline(60.0, |t| 5.0 * (PI * t).sin().powf(0.7))
        }),
        (
            "Heart Leaf",
            &["leaf", "heart", "cordate", "linden"],
            || {
                leaf_outline(60.0, |t| {
                    let body = 24.0 * (PI * t.powf(0.7)).sin().powf(0.7);
                    body * smoothstep(-0.05, 0.12, t) + 6.0 * (1.0 - smoothstep(0.0, 0.1, t))
                })
            },
        ),
        ("Serrated Leaf", &["leaf", "serrated", "elm"], || {
            leaf_outline(60.0, |t| {
                let tooth = (t * 14.0).fract();
                15.0 * (PI * t).sin().powf(0.7) * (0.9 + 0.1 * tooth)
            })
        }),
        ("Oak Leaf", &["leaf", "oak", "lobed", "autumn"], || {
            leaf_outline(60.0, |t| {
                let lobes = (0.55 + 0.45 * (TAU * 3.5 * t).cos().abs().powf(0.6)).min(1.0);
                20.0 * (PI * t).sin().powf(0.5) * lobes * (1.0 - 0.2 * t) + 1.2
            })
        }),
        (
            "Holly Leaf",
            &["leaf", "holly", "christmas", "spiky"],
            || {
                leaf_outline(60.0, |t| {
                    let spikes = 2.0 * ((t * 5.0).fract() - 0.5).abs();
                    16.0 * (PI * t).sin().powf(0.6) * (0.72 + 0.28 * spikes.powi(3))
                })
            },
        ),
        (
            "Maple Leaf",
            &["leaf", "maple", "palmate", "autumn", "canada"],
            || {
                let lobes = [
                    (PI / 2.0, 1.0),
                    (PI / 2.0 + 0.95, 0.85),
                    (PI / 2.0 - 0.95, 0.85),
                    (PI / 2.0 + 1.95, 0.55),
                    (PI / 2.0 - 1.95, 0.55),
                ];
                let ring = art::polar(720, move |theta| {
                    let reach = lobes
                        .iter()
                        .map(|(at, len)| {
                            let d = ((theta - at + PI).rem_euclid(TAU) - PI).abs();
                            len * (1.0 - d / 0.8).max(0.0)
                        })
                        .fold(0.0, f64::max);
                    let teeth = 1.0 + 0.07 * ((theta * 22.0).sin()).abs();
                    32.0 * (0.3 + 0.7 * reach) * teeth
                });
                art::moved(&ring, &art::place(0.0, 0.0, 10.0))
            },
        ),
        ("Ivy Leaf", &["leaf", "ivy", "palmate", "vine"], || {
            let lobes = [
                (PI / 2.0, 1.0),
                (PI / 2.0 + 1.05, 0.75),
                (PI / 2.0 - 1.05, 0.75),
                (PI / 2.0 + 2.1, 0.5),
                (PI / 2.0 - 2.1, 0.5),
            ];
            let ring = art::polar(720, move |theta| {
                let reach = lobes
                    .iter()
                    .map(|(at, len)| {
                        let d = ((theta - at + PI).rem_euclid(TAU) - PI).abs();
                        len * (1.0 - (d / 0.75).powi(2)).max(0.0).sqrt()
                    })
                    .fold(0.0, f64::max);
                30.0 * (0.3 + 0.7 * reach)
            });
            art::moved(&ring, &art::place(0.0, 0.0, 10.0))
        }),
        ("Ginkgo Leaf", &["leaf", "ginkgo", "fan"], || {
            let mut ring = vec![p(1.5, 0.0), p(1.5, 22.0)];
            for k in 0..=60 {
                let a = PI / 2.0 - 1.0 + 2.0 * k as f64 / 60.0;
                let notch = 1.0 - 0.12 * (-(((a - PI / 2.0) / 0.06).powi(2))).exp();
                let wave = 1.0 + 0.02 * (a * 40.0).sin();
                let r = 40.0 * notch * wave;
                ring.push(p(r * a.cos(), 18.0 + r * a.sin() * 0.9));
            }
            ring.extend([p(-1.5, 22.0), p(-1.5, 0.0)]);
            ring
        }),
        ("Birch Leaf", &["leaf", "birch", "toothed"], || {
            leaf_outline(60.0, |t| {
                let tooth = 1.0 - 0.08 * (t * 18.0).fract();
                21.0 * (PI * t.powf(0.8)).sin().powf(0.7) * tooth * (1.0 - 0.15 * t)
            })
        }),
        ("Round Leaf", &["leaf", "round", "aspen"], || {
            leaf_outline(60.0, |t| 26.0 * (PI * t).sin().powf(0.45) * (1.0 - 0.1 * t))
        }),
    ]
}

/// A fern frond standing along +Y: a stem with pairs of small leaflets
/// that shrink towards the tip.
fn fern(len: f64) -> Result<Vec<Ring>, CoreError> {
    let mut rings = art::stroke(&[p(0.0, -4.0), p(0.0, len)], 1.4)?;
    let pairs = 11;
    for k in 0..pairs {
        let t = k as f64 / pairs as f64;
        let y = 4.0 + (len - 8.0) * t;
        let size = 18.0 * (1.0 - t * 0.85);
        for side in [1.0, -1.0] {
            let leaflet = leaf_outline(size, |u| size * 0.2 * (PI * u).sin().powf(0.7));
            rings.push(art::moved(
                &leaflet,
                &art::place(-side * (PI / 2.0 - 0.45), 0.0, y),
            ));
        }
    }
    art::union(&rings)
}

/// A branch of `pairs` leaf pairs along a gently curving stem, standing
/// along +Y.
fn branch(len: f64, pairs: usize, leaf: f64) -> Result<art::Leaves, CoreError> {
    let stem: Vec<Point> = (0..=60)
        .map(|k| {
            let t = k as f64 / 60.0;
            p(4.0 * (PI * t).sin(), len * t)
        })
        .collect();
    let mut rings = art::stroke(&stem, 1.6)?;
    let mut axes = Vec::new();
    for k in 0..pairs {
        let t = 0.12 + 0.8 * k as f64 / pairs as f64;
        let at = p(4.0 * (PI * t).sin(), len * t);
        for side in [1.0, -1.0] {
            let turn = -side * 0.9;
            let shape = leaf_outline(leaf, |u| {
                leaf * 0.2 * (PI * u).sin().powf(0.8) * (1.0 - 0.3 * u)
            });
            let m = art::place(turn, at.x, at.y);
            axes.push((at, m.apply(p(0.0, leaf))));
            rings.push(art::moved(&shape, &m));
        }
    }
    let tip = leaf_outline(leaf, |u| {
        leaf * 0.2 * (PI * u).sin().powf(0.8) * (1.0 - 0.3 * u)
    });
    let top = p(0.0, len);
    axes.push((top, p(0.0, len + leaf)));
    rings.push(art::moved(&tip, &art::place(0.0, top.x, top.y)));
    Ok((rings, axes))
}

/// A wreath of leaves round a circle of radius `r`, open at the bottom.
fn wreath(r: f64, count: usize, leaf: f64) -> art::Leaves {
    let mut rings = Vec::new();
    let mut axes = Vec::new();
    for side in [1.0, -1.0] {
        for k in 0..count {
            let a = -PI / 2.0 + side * (0.35 + (PI - 0.5) * k as f64 / count as f64);
            let at = p(r * a.cos(), r * a.sin());
            for lean in [0.5, -0.5] {
                let heading = a + side * PI / 2.0 + lean;
                let shape = leaf_outline(leaf, |u| {
                    leaf * 0.2 * (PI * u).sin().powf(0.8) * (1.0 - 0.3 * u)
                });
                let m = art::place(heading - PI / 2.0, at.x, at.y);
                axes.push((at, m.apply(p(0.0, leaf))));
                rings.push(art::moved(&shape, &m));
            }
        }
    }
    (rings, axes)
}

/// A fleur-de-lis standing along +Y, about 65 tall: a tall centre petal,
/// two side petals curling out and down, a band, and a short foot.
pub(super) fn fleur_de_lis() -> Result<Vec<Ring>, CoreError> {
    let centre = art::mirrored(
        &[
            p(0.0, 44.0),
            p(3.0, 36.0),
            p(7.0, 24.0),
            p(9.0, 14.0),
            p(8.0, 6.0),
            p(4.0, 1.0),
            p(0.0, 0.0),
        ],
        10,
    );
    let side_line = art::smooth_open(
        &[
            p(3.0, 4.0),
            p(12.0, 8.0),
            p(20.0, 16.0),
            p(25.0, 26.0),
            p(25.0, 33.0),
            p(21.0, 36.0),
            p(18.0, 32.0),
        ],
        10,
    );
    let foot_line = art::smooth_open(
        &[p(3.0, -5.0), p(9.0, -9.0), p(14.0, -10.0), p(17.5, -8.0)],
        10,
    );
    let mirror = |ring: &Ring| -> Ring { ring.iter().map(|q| p(-q.x, q.y)).collect() };
    let side = art::tapered(&side_line, 11.0, 2.5);
    let foot = art::tapered(&foot_line, 5.0, 1.2);
    let low = art::mirrored(
        &[
            p(0.0, -3.0),
            p(4.0, -7.0),
            p(5.0, -12.0),
            p(2.0, -17.0),
            p(0.0, -19.0),
        ],
        8,
    );
    let band = art::rounded_rect(p(0.0, -2.0), 30.0, 5.0, 1.5);
    art::union(&[centre, mirror(&side), side, mirror(&foot), foot, low, band])
}

fn outlines(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Leaves and Floral/Leaves",
        source: SOURCE,
    };
    for (name, tags, make) in leaves() {
        add.vectors(name, tags, move || Ok(art::paths(&[make()])));
    }
    add.vectors("Fern Frond", &["leaf", "fern", "frond"], || {
        Ok(art::paths(&fern(70.0)?))
    });
    add.vectors(
        "Laurel Branch",
        &["leaf", "laurel", "branch", "olive"],
        || Ok(art::paths(&art::union(&branch(70.0, 5, 16.0)?.0)?)),
    );
    add.vectors(
        "Laurel Wreath",
        &["leaf", "laurel", "wreath", "victory"],
        || Ok(art::paths(&art::union(&wreath(35.0, 8, 14.0).0)?)),
    );
    add.vectors(
        "Three Leaf Clover",
        &["clover", "shamrock", "irish"],
        || Ok(art::paths(&clover(3)?)),
    );
    add.vectors("Four Leaf Clover", &["clover", "lucky", "irish"], || {
        Ok(art::paths(&clover(4)?))
    });
    let mut add = Adder {
        out: add.out,
        category: "Leaves and Floral/Flowers",
        source: SOURCE,
    };
    add.vectors("Daisy", &["flower", "daisy"], || {
        let petal = art::ellipse(p(0.0, 17.0), 3.6, 11.0, 48);
        let mut rings = vec![art::union(&art::around(&[petal], 16, 0.0))?.remove(0)];
        rings.push(art::circle(p(0.0, 0.0), 7.0, 64));
        Ok(art::paths(&rings))
    });
    add.vectors("Sunflower", &["flower", "sunflower"], || {
        let petal = super::borders::leaf_shape(18.0, 7.0);
        let mut petals = art::around(&[art::moved(&petal, &art::place(0.0, 11.0, 0.0))], 20, 0.0);
        petals.extend(art::around(
            &[art::moved(&petal, &art::place(0.0, 10.0, 0.0))],
            20,
            PI / 20.0,
        ));
        petals.push(art::circle(p(0.0, 0.0), 13.0, 96));
        let mut rings = art::union(&petals)?;
        rings.push(art::circle(p(0.0, 0.0), 11.0, 96));
        rings.push(art::circle(p(0.0, 0.0), 6.0, 64));
        Ok(art::paths(&rings))
    });
    add.vectors("Tulip", &["flower", "tulip", "spring"], || {
        Ok(art::paths(&tulip()?))
    });
    add.vectors("Lotus", &["flower", "lotus", "water lily"], || {
        Ok(art::paths(&lotus()?))
    });
    add.vectors("Poppy", &["flower", "poppy", "remembrance"], || {
        let petal = art::circle(p(0.0, 12.0), 13.0, 96);
        let mut rings = art::union(&art::around(&[petal], 4, PI / 4.0))?;
        rings.push(art::circle(p(0.0, 0.0), 5.0, 48));
        Ok(art::paths(&rings))
    });
    add.vectors("Lily", &["flower", "lily"], || {
        let petal = super::borders::leaf_shape(30.0, 11.0);
        let mut rings = art::union(&art::around(&[petal], 6, PI / 2.0))?;
        for k in 0..6 {
            let a = PI / 2.0 + TAU * k as f64 / 6.0 + PI / 6.0;
            rings.push(art::circle(p(12.0 * a.cos(), 12.0 * a.sin()), 1.4, 24));
        }
        Ok(art::paths(&rings))
    });
    add.vectors(
        "Blossom",
        &["flower", "blossom", "cherry", "sakura"],
        || {
            let mut rings = vec![art::polar(480, |t| {
                let u = {
                    let k = (t - PI / 2.0) * 5.0 / TAU;
                    (k - k.round()) * 2.0
                };
                let tip = {
                    let body = (1.0 - u * u).max(0.0).sqrt();
                    body * (1.0 - 0.18 * (-(u / 0.18).powi(2)).exp())
                };
                30.0 * (0.45 + 0.55 * tip)
            })];
            for k in 0..5 {
                let a = PI / 2.0 + TAU * k as f64 / 5.0;
                rings.push(art::circle(p(8.0 * a.cos(), 8.0 * a.sin()), 1.3, 24));
            }
            rings.push(art::circle(p(0.0, 0.0), 3.0, 32));
            Ok(art::paths(&rings))
        },
    );
    add.vectors("Rose", &["flower", "rose", "love"], || {
        Ok(art::paths(&rose_outline()?))
    });
    add.vectors(
        "Fleur de Lis",
        &["fleur de lis", "lily", "heraldry", "french"],
        || Ok(art::paths(&fleur_de_lis()?)),
    );
    add.vectors("Flower Vine", &["flower", "vine", "border"], || {
        let mut rings = Vec::new();
        let stem: Vec<Point> = (0..=240)
            .map(|k| {
                let x = -60.0 + k as f64 * 0.5;
                p(x, 6.0 * (x * TAU / 40.0).sin())
            })
            .collect();
        rings.extend(art::stroke(&stem, 1.4)?);
        for k in 0..3 {
            let x = -40.0 + 40.0 * k as f64;
            let petal = art::ellipse(p(0.0, 4.5), 2.2, 4.0, 32);
            let flower = art::union(&art::around(&[petal], 5, 0.0))?;
            rings.extend(art::moved_all(
                &flower,
                &art::place(0.0, x, 6.0 * (x * TAU / 40.0).sin()),
            ));
            for side in [1.0, -1.0] {
                let leaf = super::borders::leaf_shape(9.0, 4.0);
                rings.push(art::moved(
                    &leaf,
                    &art::place(
                        side * 0.8 + if side < 0.0 { PI } else { 0.0 },
                        x + 20.0,
                        6.0 * ((x + 20.0) * TAU / 40.0).sin(),
                    ),
                ));
            }
        }
        Ok(art::paths(&art::union(&rings)?))
    });
}

/// Clover with `leaves` heart shaped leaflets and a stem.
fn clover(leaves: usize) -> Result<Vec<Ring>, CoreError> {
    let heart = super::stars::heart(22.0);
    let leaflet = art::moved(&heart, &art::place(0.0, 0.0, 12.0));
    let mut rings = art::around(&[leaflet], leaves, 0.0);
    rings.extend(art::stroke(
        &art::smooth_open(&[p(0.0, 0.0), p(3.0, -14.0), p(8.0, -26.0)], 8),
        2.4,
    )?);
    art::union(&rings)
}

fn tulip() -> Result<Vec<Ring>, CoreError> {
    let cup = art::mirrored(
        &[
            p(0.0, 44.0),
            p(4.0, 50.0),
            p(9.0, 46.0),
            p(13.0, 52.0),
            p(15.0, 40.0),
            p(13.0, 28.0),
            p(7.0, 22.0),
            p(0.0, 21.0),
        ],
        8,
    );
    let mut rings = vec![cup];
    rings.extend(art::stroke(&[p(0.0, 24.0), p(0.0, -10.0)], 2.4)?);
    let left = art::moved(
        &super::borders::leaf_shape(30.0, 9.0),
        &art::place(PI / 2.0 + 0.5, 0.0, -8.0),
    );
    let right = art::moved(
        &super::borders::leaf_shape(26.0, 8.0),
        &art::place(PI / 2.0 - 0.45, 0.0, -4.0),
    );
    rings.push(left);
    rings.push(right);
    art::union(&rings)
}

fn lotus() -> Result<Vec<Ring>, CoreError> {
    let mut rings = Vec::new();
    for (k, len) in [(-2, 26.0), (-1, 34.0), (0, 40.0), (1, 34.0), (2, 26.0)] {
        let a = PI / 2.0 + 0.45 * k as f64;
        let petal = leaf_outline(len, |u| len * 0.26 * (PI * u).sin().powf(0.55));
        rings.push(art::moved(&petal, &art::place(a - PI / 2.0, 0.0, 0.0)));
    }
    art::union(&rings)
}

fn rose_outline() -> Result<Vec<Ring>, CoreError> {
    let outer = art::polar(480, |t| 28.0 * (1.0 + 0.06 * (5.0 * t).cos()));
    let mut rings = vec![outer];
    // The petals' edges curl inward as a spiral.
    let spiral: Vec<Point> = (0..=300)
        .map(|k| {
            let t = k as f64 / 300.0;
            let a = TAU * 2.6 * t;
            let r = 3.0 + 20.0 * t * (1.0 + 0.08 * (6.0 * a).cos());
            p(r * a.cos(), r * a.sin())
        })
        .collect();
    rings.extend(art::stroke(&spiral, 1.5)?);
    Ok(rings)
}

fn reliefs(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Leaves and Floral/Leaf Reliefs",
        source: SOURCE,
    };
    add.relief("Veined Leaf", &["leaf", "veins"], || {
        samples::sample("Leaf")
            .map(|c| c.field)
            .ok_or_else(|| CoreError::validation("the leaf sample is missing"))
    });
    for (name, tags, make) in leaves() {
        let mut all = vec!["relief"];
        all.extend_from_slice(tags);
        add.relief(name, &all, move || {
            let ring = make();
            let top = ring.iter().map(|q| q.y).fold(f64::MIN, f64::max);
            veined_up(&ring, top, 4.0)
        });
    }
    add.relief(
        "Laurel Branch Relief",
        &["leaf", "laurel", "branch"],
        || {
            let (rings, axes) = branch(70.0, 5, 16.0)?;
            leaves_on_stem(&rings, &axes)
        },
    );
    add.relief(
        "Laurel Wreath Relief",
        &["leaf", "laurel", "wreath"],
        || {
            let (rings, axes) = wreath(35.0, 8, 14.0);
            leaves_on_stem(&rings, &axes)
        },
    );
    add.relief("Fern Relief", &["leaf", "fern"], || {
        art::dome(&fern(70.0)?, 3.0)
    });
    let mut add = Adder {
        out: add.out,
        category: "Leaves and Floral/Flower Reliefs",
        source: SOURCE,
    };
    add.relief("Daisy Relief", &["flower", "daisy"], || {
        let petal = art::ellipse(p(0.0, 17.0), 3.4, 11.0, 64);
        let petals = art::puffy(&art::union(&art::around(&[petal], 16, 0.0))?, 2.6)?;
        let head = art::field_fn(17.0, 17.0, |x, y| {
            let r = x.hypot(y);
            if r > 8.0 {
                return f64::NAN;
            }
            let dimple = 0.2 * ((x * 2.2).sin() * (y * 2.2).sin()).abs();
            2.6 + 2.2 * (1.0 - (r / 8.0).powi(2)).sqrt() - dimple
        });
        Ok(art::highest(&petals, &head))
    });
    add.relief(
        "Sunflower Relief",
        &["flower", "sunflower", "seeds"],
        || {
            let petals = {
                let petal = super::borders::leaf_shape(18.0, 7.0);
                let mut rings =
                    art::around(&[art::moved(&petal, &art::place(0.0, 11.0, 0.0))], 20, 0.0);
                rings.extend(art::around(
                    &[art::moved(&petal, &art::place(0.0, 10.0, 0.0))],
                    20,
                    PI / 20.0,
                ));
                art::puffy(&art::union(&rings)?, 3.0)?
            };
            let golden = PI * (3.0 - 5.0f64.sqrt());
            let seeds: Vec<(f64, f64)> = (0..220)
                .map(|k| {
                    let r = 0.8 * (k as f64).sqrt();
                    let a = k as f64 * golden;
                    (r * a.cos(), r * a.sin())
                })
                .collect();
            let head = art::field_fn(28.0, 28.0, move |x, y| {
                let r = x.hypot(y);
                if r > 13.5 {
                    return f64::NAN;
                }
                let dome = 3.5 + 2.0 * (1.0 - (r / 13.5).powi(2)).sqrt();
                let near = seeds
                    .iter()
                    .map(|(sx, sy)| (x - sx).hypot(y - sy))
                    .fold(f64::MAX, f64::min);
                dome + 0.35 * (1.0 - (near / 0.5).min(1.0))
            });
            Ok(art::highest(&petals, &head))
        },
    );
    add.relief("Tulip Relief", &["flower", "tulip"], || {
        art::puffy(&tulip()?, 5.0)
    });
    add.relief("Lotus Relief", &["flower", "lotus"], || {
        art::dome(&lotus()?, 5.0)
    });
    add.relief("Rose Relief", &["flower", "rose", "love"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y);
            let edge = 28.0 * (1.0 + 0.06 * (5.0 * y.atan2(x)).cos());
            if r > edge {
                return f64::NAN;
            }
            let theta = y.atan2(x).rem_euclid(TAU);
            // Distance, along the radius, to the nearest turn of the
            // spiral r = 3 + 20 a / (2.6 turns).
            let per_turn = 20.0 / 2.6;
            let base = 3.0 + per_turn * theta / TAU;
            let turns = ((r - base) / per_turn).round();
            let d = (r - (base + turns * per_turn)).abs() / per_turn;
            let dome = 6.0 * (1.0 - (r / edge).powi(2)).max(0.0).sqrt();
            let petal = 1.4 * (1.0 - (2.0 * d).min(1.0)).powf(0.5);
            (dome * 0.8 + petal + 0.6).max(0.3) * round_cap(((edge - r) / 3.0).min(1.0)).max(0.3)
        }))
    });
    add.relief("Fleur de Lis Relief", &["fleur de lis", "heraldry"], || {
        art::puffy(&fleur_de_lis()?, 5.0)
    });
    add.relief("Clover Relief", &["clover", "shamrock"], || {
        art::puffy(&clover(3)?, 4.0)
    });
    add.relief("Poppy Relief", &["flower", "poppy"], || {
        let mut petals: Option<Field> = None;
        for k in 0..4 {
            let a = PI / 4.0 + PI / 2.0 * k as f64;
            let ring = art::circle(p(12.0 * a.cos(), 12.0 * a.sin()), 13.0, 96);
            let f = art::puffy(&[ring], 3.0 + 0.4 * k as f64)?;
            petals = Some(match petals {
                Some(g) => art::highest(&g, &f),
                None => f,
            });
        }
        let centre = art::field_fn(12.0, 12.0, |x, y| {
            let r = x.hypot(y) / 5.5;
            if r > 1.0 {
                f64::NAN
            } else {
                4.0 + 1.5 * (1.0 - r * r).sqrt()
            }
        });
        let petals = petals.ok_or_else(|| CoreError::validation("no petals"))?;
        Ok(art::highest(&petals, &centre))
    });
}

/// Leaves on a stem as one relief: the stem low and rounded, each leaf a
/// veined relief along its own axis.
fn leaves_on_stem(rings: &[Ring], axes: &[(Point, Point)]) -> Result<Field, CoreError> {
    let mut field = art::dome(&art::union(rings)?, 1.5)?;
    // Leaf rings follow the stem's rings; pair them with their axes.
    let leaf_rings: Vec<&Ring> = rings.iter().rev().take(axes.len()).collect();
    for (ring, (base, tip)) in leaf_rings.into_iter().rev().zip(axes) {
        let leaf = veined(ring, *base, *tip, 3.0)?;
        field = art::highest(&field, &leaf);
    }
    Ok(field)
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    outlines(out);
    reliefs(out);
}
