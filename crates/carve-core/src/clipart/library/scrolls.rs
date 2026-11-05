//! Scrolls and ornaments: C and S scrolls, volutes, flourishes, swashes,
//! banners and acanthus leaves, drawn as tapered strokes along smooth
//! curves, and their reliefs rounded over with Create Shape.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::Point;
use crate::model::field::Field;

use super::art::{self, p, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "scrolls.rs";

/// Points of a spiral round `c` from radius `r0` to `r1`, starting at angle
/// `a0` (radians) and turning `turns` times, counter clockwise when `ccw`.
fn spiral(c: Point, r0: f64, r1: f64, a0: f64, turns: f64, ccw: bool) -> Vec<Point> {
    let n = (turns * 72.0).ceil().max(8.0) as usize;
    let sign = if ccw { 1.0 } else { -1.0 };
    (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let a = a0 + sign * TAU * turns * t;
            let r = r0 + (r1 - r0) * t;
            p(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// `points` turned half a turn about the origin.
fn half_turn(points: &[Point]) -> Vec<Point> {
    points.iter().map(|q| p(-q.x, -q.y)).collect()
}

/// `points` mirrored left for right.
fn mirror_x(points: &[Point]) -> Vec<Point> {
    points.iter().map(|q| p(-q.x, q.y)).collect()
}

/// `points` mirrored top for bottom.
fn mirror_y(points: &[Point]) -> Vec<Point> {
    points.iter().map(|q| p(q.x, -q.y)).collect()
}

/// A smooth tapered stroke through key points.
fn curl(keys: &[Point], wide: f64, narrow: f64) -> Ring {
    art::tapered(&art::smooth_open(keys, 10), wide, narrow)
}

/// The C scroll's lower half, from the middle of the back to the curl.
fn c_half() -> Vec<Point> {
    vec![
        p(-20.0, 0.0),
        p(-18.0, -10.0),
        p(-10.0, -18.0),
        p(0.0, -20.0),
        p(8.0, -17.0),
        p(11.0, -11.0),
        p(8.0, -6.0),
        p(3.5, -7.0),
        p(3.0, -11.0),
    ]
}

/// The S scroll's upper half, from the middle to the curl.
fn s_half() -> Vec<Point> {
    vec![
        p(0.0, 0.0),
        p(5.0, 8.0),
        p(5.0, 17.0),
        p(0.0, 23.0),
        p(-7.0, 24.0),
        p(-11.0, 19.0),
        p(-9.5, 14.0),
        p(-5.5, 13.5),
        p(-4.5, 17.0),
    ]
}

/// Every scroll design: name, tags and outline.
fn designs() -> Vec<art::Design<Result<Vec<Ring>, CoreError>>> {
    vec![
        ("C Scroll", &["scroll", "c scroll", "curl"], || {
            let half = c_half();
            art::union(&[curl(&half, 6.0, 1.2), curl(&mirror_y(&half), 6.0, 1.2)])
        }),
        ("S Scroll", &["scroll", "s scroll", "curl"], || {
            let half = s_half();
            art::union(&[curl(&half, 6.0, 1.2), curl(&half_turn(&half), 6.0, 1.2)])
        }),
        ("Volute", &["scroll", "volute", "spiral", "ionic"], || {
            let mut line = vec![p(-30.0, -14.0), p(-18.0, -12.0)];
            line.extend(spiral(p(0.0, 0.0), 14.0, 2.5, -PI * 0.55, 1.4, true));
            Ok(vec![art::tapered(&line, 6.0, 1.2)])
        }),
        (
            "Double Volute",
            &["scroll", "volute", "ionic", "capital"],
            || {
                let mut right = vec![p(0.0, 10.0), p(12.0, 10.0)];
                right.extend(spiral(p(22.0, 0.0), 10.0, 2.0, PI / 2.0, 1.3, false));
                let left = mirror_x(&right);
                art::union(&[
                    art::tapered(&right, 5.0, 1.0),
                    art::tapered(&left, 5.0, 1.0),
                ])
            },
        ),
        ("Heart Scroll", &["scroll", "heart", "curl"], || {
            let half = vec![
                p(0.0, -26.0),
                p(-8.0, -14.0),
                p(-18.0, -2.0),
                p(-20.0, 10.0),
                p(-13.0, 18.0),
                p(-5.0, 15.0),
                p(-4.0, 8.0),
                p(-9.0, 6.0),
                p(-11.0, 10.0),
            ];
            art::union(&[curl(&half, 5.0, 1.2), curl(&mirror_x(&half), 5.0, 1.2)])
        }),
        (
            "Flourish Divider",
            &["flourish", "divider", "swirl"],
            || {
                let main = vec![
                    p(4.0, 0.0),
                    p(15.0, 4.0),
                    p(28.0, 2.0),
                    p(38.0, -4.0),
                    p(45.0, -2.0),
                    p(45.0, 3.5),
                    p(40.5, 4.5),
                    p(38.5, 1.0),
                ];
                let small = vec![
                    p(14.0, 1.0),
                    p(21.0, -6.0),
                    p(28.0, -7.0),
                    p(30.5, -3.5),
                    p(27.5, -2.0),
                ];
                let mut rings = vec![
                    curl(&main, 3.2, 0.8),
                    curl(&mirror_x(&main), 3.2, 0.8),
                    curl(&small, 2.2, 0.7),
                    curl(&mirror_x(&small), 2.2, 0.7),
                ];
                rings.push(vec![p(-6.0, 0.0), p(0.0, 4.0), p(6.0, 0.0), p(0.0, -4.0)]);
                art::union(&rings)
            },
        ),
        (
            "Swash Underline",
            &["swash", "underline", "flourish"],
            || {
                let keys = vec![
                    p(-50.0, 2.0),
                    p(-30.0, -2.0),
                    p(0.0, -3.0),
                    p(30.0, 0.0),
                    p(46.0, 6.0),
                    p(49.0, 12.0),
                    p(44.0, 14.0),
                    p(41.0, 10.0),
                ];
                Ok(vec![curl(&keys, 3.5, 1.0)])
            },
        ),
        ("Corner Flourish", &["flourish", "corner", "swirl"], || {
            let arm = vec![
                p(2.0, 2.0),
                p(16.0, 3.0),
                p(30.0, 1.0),
                p(40.0, 4.0),
                p(42.0, 10.0),
                p(37.0, 12.0),
                p(35.0, 8.0),
            ];
            let other: Vec<Point> = arm.iter().map(|q| p(q.y, q.x)).collect();
            let inner = vec![
                p(4.0, 4.0),
                p(12.0, 12.0),
                p(18.0, 20.0),
                p(22.0, 18.0),
                p(19.0, 15.0),
            ];
            let inner_other: Vec<Point> = inner.iter().map(|q| p(q.y, q.x)).collect();
            art::union(&[
                curl(&arm, 3.5, 0.8),
                curl(&other, 3.5, 0.8),
                curl(&inner, 2.5, 0.7),
                curl(&inner_other, 2.5, 0.7),
                art::circle(p(2.0, 2.0), 3.0, 32),
            ])
        }),
        ("Scroll Bracket", &["scroll", "bracket", "corbel"], || {
            let top = vec![
                p(0.0, 30.0),
                p(14.0, 29.0),
                p(24.0, 24.0),
                p(27.0, 16.0),
                p(23.0, 11.0),
                p(18.0, 13.0),
                p(19.0, 17.0),
            ];
            let back = vec![
                p(0.0, 30.0),
                p(0.0, 10.0),
                p(2.0, -6.0),
                p(6.0, -18.0),
                p(12.0, -24.0),
                p(16.0, -21.0),
                p(14.0, -17.0),
                p(10.0, -18.0),
            ];
            art::union(&[
                curl(&top, 7.0, 1.4),
                curl(&back, 7.0, 1.4),
                art::rect(p(2.0, 29.0), 8.0, 4.0),
            ])
        }),
        ("Scroll Banner", &["banner", "ribbon", "scroll"], || {
            let mut rings = vec![art::rect(p(0.0, 0.0), 80.0, 18.0)];
            for side in [1.0, -1.0] {
                let roll = spiral(p(side * 42.0, -3.0), 8.0, 2.0, PI / 2.0, 1.1, side < 0.0);
                let mut line = vec![p(side * 30.0, 5.0)];
                line.extend(roll);
                rings.push(art::tapered(&line, 5.0, 1.2));
            }
            art::union(&rings)
        }),
        ("Tendril", &["tendril", "vine", "curl"], || {
            let stem = vec![
                p(-35.0, -10.0),
                p(-15.0, -4.0),
                p(5.0, 2.0),
                p(20.0, 10.0),
                p(26.0, 18.0),
                p(22.0, 23.0),
                p(17.0, 20.0),
                p(19.0, 16.0),
            ];
            let side = vec![
                p(-5.0, -1.0),
                p(0.0, -10.0),
                p(8.0, -14.0),
                p(13.0, -11.0),
                p(10.0, -8.0),
            ];
            let mut rings = vec![curl(&stem, 3.0, 0.8), curl(&side, 2.0, 0.6)];
            let leaf = super::borders::leaf_shape(14.0, 6.0);
            rings.push(art::moved(&leaf, &art::place(2.2, -16.0, -3.0)));
            rings.push(art::moved(&leaf, &art::place(-0.5, 6.0, 3.0)));
            art::union(&rings)
        }),
        (
            "Fleuron",
            &["fleuron", "ornament", "printer's flower"],
            || {
                let heart = super::stars::heart(24.0);
                let leaf = art::moved(&heart, &art::place(PI, 0.0, 10.0));
                let stem_r = vec![
                    p(0.0, -2.0),
                    p(6.0, -8.0),
                    p(14.0, -10.0),
                    p(19.0, -6.0),
                    p(16.0, -2.5),
                    p(13.0, -5.0),
                ];
                art::union(&[
                    leaf,
                    curl(&stem_r, 2.6, 0.7),
                    curl(&mirror_x(&stem_r), 2.6, 0.7),
                ])
            },
        ),
    ]
}

/// An acanthus leaf: a lobed, toothed leaf whose spine curls over at the
/// tip, `len` long. Answers the outline and the spine.
fn acanthus(len: f64) -> (Ring, Vec<Point>) {
    // The spine bends more and more towards the tip.
    let n = 200;
    let mut spine = Vec::with_capacity(n + 1);
    let (mut x, mut y, step) = (0.0, 0.0, len / n as f64);
    for k in 0..=n {
        let t = k as f64 / n as f64;
        spine.push(p(x, y));
        let heading = PI / 2.0 - 2.6 * t.powf(2.4);
        x += step * heading.cos();
        y += step * heading.sin();
    }
    let width = |t: f64| {
        let body = 0.3 * len * (PI * t.powf(0.8)).sin().powf(0.7);
        let lobes = 0.7 + 0.3 * (PI * 4.5 * t).cos().abs().powf(0.4);
        let teeth = 1.0 - 0.08 * (t * 27.0).fract();
        (body * lobes * teeth).max(0.0)
    };
    let normal = |k: usize| {
        let a = spine[k.saturating_sub(1)];
        let b = spine[(k + 1).min(n)];
        let d = a.to(b);
        let l = d.length().max(1e-12);
        (-d.y / l, d.x / l)
    };
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (k, q) in spine.iter().enumerate() {
        let t = k as f64 / n as f64;
        let (nx, ny) = normal(k);
        // The inner side of the curl stays narrower so it does not fold.
        let w_out = width(t);
        let w_in = width(t) * (1.0 - 0.45 * t);
        left.push(p(q.x + nx * w_in, q.y + ny * w_in));
        right.push(p(q.x - nx * w_out, q.y - ny * w_out));
    }
    let mut ring = left;
    ring.extend(right.into_iter().rev());
    (ring, spine)
}

/// A relief of acanthus leaves: a soft body over their outlines, a raised
/// midrib along each spine.
fn acanthus_relief(rings: &[Ring], spines: &[Vec<Point>], height: f64) -> Result<Field, CoreError> {
    let body = art::puffy(&art::union(rings)?, height)?;
    let ribs: Vec<Vec<(Point, f64)>> = spines
        .iter()
        .map(|spine| {
            spine
                .iter()
                .take(spine.len() - 12)
                .map(|q| (*q, -0.3))
                .collect()
        })
        .collect();
    Ok(art::added_onto(&body, &art::tubes(&ribs, 0.9)?))
}

/// Two acanthus leaves leaning apart, and their spines.
fn acanthus_pair(len: f64) -> (Vec<Ring>, Vec<Vec<Point>>) {
    let (ring, spine) = acanthus(len);
    let m = art::place(-0.35, 4.0, 0.0);
    let right = art::moved(&ring, &m);
    let right_spine = art::moved(&spine, &m);
    (
        vec![mirror_x(&right), right],
        vec![mirror_x(&right_spine), right_spine],
    )
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Scrolls and Ornaments/Scrolls",
        source: SOURCE,
    };
    for (name, tags, make) in designs() {
        add.vectors(name, tags, move || Ok(art::paths(&make()?)));
    }
    add.vectors("Acanthus Leaf", &["acanthus", "leaf", "classical"], || {
        Ok(art::paths(&[acanthus(70.0).0]))
    });
    add.vectors(
        "Acanthus Pair",
        &["acanthus", "leaf", "pair", "classical"],
        || Ok(art::paths(&art::union(&acanthus_pair(60.0).0)?)),
    );
    let mut add = Adder {
        out: add.out,
        category: "Scrolls and Ornaments/Scroll Reliefs",
        source: SOURCE,
    };
    for (name, tags, make) in designs() {
        let mut all = vec!["relief"];
        all.extend_from_slice(tags);
        add.relief(&format!("{name} Relief"), &all, move || {
            art::puffy(&make()?, 3.0)
        });
    }
    add.relief(
        "Acanthus Leaf Relief",
        &["acanthus", "leaf", "classical"],
        || {
            let (ring, spine) = acanthus(70.0);
            acanthus_relief(&[ring], &[spine], 5.0)
        },
    );
    add.relief(
        "Acanthus Pair Relief",
        &["acanthus", "leaf", "pair"],
        || {
            let (rings, spines) = acanthus_pair(60.0);
            acanthus_relief(&rings, &spines, 4.5)
        },
    );
}
