//! Silhouettes: animals, nautical, western, holiday and sports motifs,
//! drawn for this library from circles, ellipses, polygons, smooth curves
//! and tapered strokes, welded into single outlines with their holes, and
//! each also as a soft rounded relief.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::Point;

use super::art::{self, p, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "silhouettes.rs";

type Motif = art::Design<Result<Vec<Ring>, CoreError>>;

/// An ellipse centred on `c`, turned by `turn` radians.
fn oval(c: Point, rx: f64, ry: f64, turn: f64) -> Ring {
    art::moved(
        &art::ellipse(p(0.0, 0.0), rx, ry, 96),
        &art::place(turn, c.x, c.y),
    )
}

/// A circle.
fn disc(c: Point, r: f64) -> Ring {
    art::circle(c, r, 96)
}

/// A smooth tapered stroke through key points.
fn limb(keys: &[Point], wide: f64, narrow: f64) -> Ring {
    art::tapered(&art::smooth_open(keys, 10), wide, narrow)
}

/// A straight bar from `a` to `b`, `w` wide, with square ends.
fn bar(a: Point, b: Point, w: f64) -> Ring {
    let d = a.to(b);
    let len = d.length().max(1e-9);
    let (nx, ny) = (-d.y / len * w / 2.0, d.x / len * w / 2.0);
    vec![
        p(a.x + nx, a.y + ny),
        p(a.x - nx, a.y - ny),
        p(b.x - nx, b.y - ny),
        p(b.x + nx, b.y + ny),
    ]
}

/// `shape` less `holes`.
fn with_holes(shape: &[Ring], holes: &[Ring]) -> Result<Vec<Ring>, CoreError> {
    art::cut(shape, holes)
}

fn animals() -> Vec<Motif> {
    vec![
        ("Cat", &["cat", "kitten", "pet"], || {
            art::union(&[
                oval(p(0.0, -12.0), 15.0, 19.0, 0.0),
                disc(p(0.0, 14.0), 10.5),
                vec![p(-10.0, 17.0), p(-9.0, 30.0), p(-2.0, 22.0)],
                vec![p(10.0, 17.0), p(9.0, 30.0), p(2.0, 22.0)],
                limb(
                    &[
                        p(10.0, -28.0),
                        p(22.0, -26.0),
                        p(27.0, -14.0),
                        p(24.0, -4.0),
                    ],
                    5.0,
                    2.5,
                ),
            ])
        }),
        ("Dog Head", &["dog", "puppy", "pet"], || {
            let head = art::union(&[
                disc(p(0.0, 4.0), 16.0),
                oval(p(0.0, -10.0), 10.0, 8.0, 0.0),
                oval(p(-17.0, 4.0), 6.0, 13.0, -0.3),
                oval(p(17.0, 4.0), 6.0, 13.0, 0.3),
            ])?;
            with_holes(
                &head,
                &[
                    disc(p(-6.0, 8.0), 2.2),
                    disc(p(6.0, 8.0), 2.2),
                    oval(p(0.0, -7.0), 3.5, 2.5, 0.0),
                ],
            )
        }),
        ("Bird", &["bird", "robin", "songbird"], || {
            let body = art::union(&[
                oval(p(0.0, 0.0), 18.0, 11.0, 0.25),
                disc(p(15.0, 10.0), 7.5),
                vec![p(21.0, 12.0), p(29.0, 10.0), p(21.0, 8.0)],
                vec![p(-14.0, -2.0), p(-30.0, -6.0), p(-28.0, 2.0)],
                bar(p(-2.0, -9.0), p(-4.0, -18.0), 1.6),
                bar(p(4.0, -9.0), p(4.0, -18.0), 1.6),
            ])?;
            with_holes(&body, &[disc(p(17.0, 12.0), 1.4)])
        }),
        ("Fish", &["fish", "trout", "fishing"], || {
            let body = art::union(&[
                oval(p(0.0, 0.0), 24.0, 11.0, 0.0),
                vec![
                    p(-20.0, 0.0),
                    p(-34.0, 11.0),
                    p(-31.0, 0.0),
                    p(-34.0, -11.0),
                ],
                vec![p(-4.0, 9.0), p(4.0, 16.0), p(8.0, 9.0)],
            ])?;
            with_holes(&body, &[disc(p(14.0, 2.5), 2.0)])
        }),
        ("Whale", &["whale", "sea", "ocean"], || {
            art::union(&[
                art::smooth(
                    &[
                        p(30.0, 2.0),
                        p(24.0, 12.0),
                        p(8.0, 15.0),
                        p(-10.0, 11.0),
                        p(-24.0, 3.0),
                        p(-30.0, -2.0),
                        p(-10.0, -8.0),
                        p(12.0, -9.0),
                        p(28.0, -6.0),
                    ],
                    10,
                ),
                vec![
                    p(-28.0, -1.0),
                    p(-40.0, 8.0),
                    p(-36.0, -1.0),
                    p(-40.0, -8.0),
                ],
                oval(p(4.0, -9.0), 7.0, 2.5, -0.4),
            ])
        }),
        ("Butterfly", &["butterfly", "insect", "garden"], || {
            art::union(&[
                oval(p(-12.0, 9.0), 13.0, 10.0, 0.5),
                oval(p(12.0, 9.0), 13.0, 10.0, -0.5),
                oval(p(-9.0, -9.0), 9.0, 7.0, -0.5),
                oval(p(9.0, -9.0), 9.0, 7.0, 0.5),
                oval(p(0.0, 0.0), 2.4, 16.0, 0.0),
                limb(&[p(-1.0, 14.0), p(-4.0, 22.0), p(-8.0, 26.0)], 1.4, 1.0),
                limb(&[p(1.0, 14.0), p(4.0, 22.0), p(8.0, 26.0)], 1.4, 1.0),
            ])
        }),
        ("Owl", &["owl", "bird", "wise"], || {
            let body = art::union(&[
                art::mirrored(
                    &[
                        p(0.0, 22.0),
                        p(14.0, 24.0),
                        p(18.0, 6.0),
                        p(16.0, -14.0),
                        p(8.0, -24.0),
                        p(0.0, -26.0),
                    ],
                    8,
                ),
                vec![p(-16.0, 20.0), p(-18.0, 32.0), p(-8.0, 22.0)],
                vec![p(16.0, 20.0), p(18.0, 32.0), p(8.0, 22.0)],
            ])?;
            with_holes(
                &body,
                &[
                    disc(p(-7.0, 12.0), 5.0),
                    disc(p(7.0, 12.0), 5.0),
                    vec![p(-2.0, 5.0), p(2.0, 5.0), p(0.0, 0.0)],
                ],
            )
        }),
        ("Rabbit", &["rabbit", "bunny", "easter"], || {
            art::union(&[
                oval(p(-2.0, -12.0), 16.0, 14.0, 0.0),
                disc(p(8.0, 6.0), 9.0),
                oval(p(4.0, 24.0), 3.5, 12.0, 0.2),
                oval(p(12.0, 23.0), 3.5, 11.0, -0.25),
                disc(p(-18.0, -8.0), 4.5),
            ])
        }),
        ("Turtle", &["turtle", "tortoise"], || {
            art::union(&[
                oval(p(0.0, 0.0), 20.0, 13.0, 0.0),
                disc(p(24.0, 3.0), 6.0),
                oval(p(-13.0, -11.0), 4.0, 6.0, 0.5),
                oval(p(13.0, -11.0), 4.0, 6.0, -0.5),
                oval(p(-13.0, 11.0), 4.0, 6.0, -0.5),
                oval(p(13.0, 11.0), 4.0, 6.0, 0.5),
                vec![p(-19.0, 1.5), p(-26.0, 0.0), p(-19.0, -1.5)],
            ])
        }),
        ("Snail", &["snail", "garden", "slow"], || {
            let shell = disc(p(2.0, 8.0), 14.0);
            let body = art::smooth(
                &[
                    p(-24.0, -8.0),
                    p(-18.0, -4.0),
                    p(0.0, -5.0),
                    p(20.0, -6.0),
                    p(28.0, 2.0),
                    p(30.0, -4.0),
                    p(24.0, -10.0),
                    p(-20.0, -11.0),
                ],
                8,
            );
            let horns = [
                limb(&[p(24.0, 0.0), p(27.0, 10.0)], 1.4, 1.0),
                limb(&[p(26.0, 0.0), p(32.0, 8.0)], 1.4, 1.0),
            ];
            let mut all = vec![shell, body];
            all.extend(horns);
            let merged = art::union(&all)?;
            let spiral: Vec<Point> = (0..=160)
                .map(|k| {
                    let t = k as f64 / 160.0;
                    let a = TAU * 2.0 * t;
                    let r = 10.0 * (1.0 - t) + 1.0;
                    p(2.0 + r * a.cos(), 8.0 + r * a.sin())
                })
                .collect();
            with_holes(&merged, &art::stroke(&spiral, 1.4)?)
        }),
        ("Bee", &["bee", "honey", "insect"], || {
            let wings = art::union(&[
                oval(p(-6.0, 12.0), 7.0, 10.0, 0.4),
                oval(p(6.0, 12.0), 7.0, 10.0, -0.4),
            ])?;
            let body = with_holes(
                &[oval(p(0.0, -2.0), 11.0, 15.0, 0.0)],
                &[
                    art::rect(p(0.0, -2.0), 30.0, 3.0),
                    art::rect(p(0.0, -9.0), 30.0, 3.0),
                ],
            )?;
            let mut all = body;
            all.push(disc(p(0.0, 17.0), 6.0));
            let bee = art::union(&all)?;
            // The wings sit behind, apart from the body.
            let wings = with_holes(
                &wings,
                &[oval(p(0.0, -2.0), 12.5, 16.5, 0.0), disc(p(0.0, 17.0), 7.5)],
            )?;
            Ok([bee, wings].concat())
        }),
        ("Elephant", &["elephant", "zoo", "safari"], || {
            art::union(&[
                oval(p(0.0, 0.0), 22.0, 15.0, 0.0),
                disc(p(20.0, 8.0), 11.0),
                oval(p(14.0, 8.0), 8.0, 11.0, 0.2),
                limb(
                    &[p(27.0, 4.0), p(31.0, -8.0), p(30.0, -18.0), p(34.0, -20.0)],
                    7.0,
                    3.5,
                ),
                art::rect(p(-14.0, -16.0), 8.0, 12.0),
                art::rect(p(12.0, -16.0), 8.0, 12.0),
                limb(&[p(-21.0, 2.0), p(-27.0, -4.0)], 2.0, 1.0),
            ])
        }),
        ("Duck", &["duck", "bird", "pond"], || {
            let body = art::union(&[
                oval(p(-2.0, -6.0), 20.0, 11.0, 0.0),
                disc(p(14.0, 10.0), 8.0),
                oval(p(24.0, 8.0), 6.0, 2.8, -0.1),
                vec![p(-18.0, -2.0), p(-28.0, 6.0), p(-22.0, -8.0)],
            ])?;
            with_holes(&body, &[disc(p(16.0, 12.0), 1.4)])
        }),
        ("Bear Head", &["bear", "teddy", "toy"], || {
            let head = art::union(&[
                disc(p(0.0, 0.0), 18.0),
                disc(p(-15.0, 14.0), 7.0),
                disc(p(15.0, 14.0), 7.0),
            ])?;
            with_holes(
                &head,
                &[
                    disc(p(-6.0, 4.0), 2.2),
                    disc(p(6.0, 4.0), 2.2),
                    oval(p(0.0, -6.0), 4.0, 3.0, 0.0),
                ],
            )
        }),
        ("Paw Print", &["paw", "dog", "cat", "pet"], || {
            Ok(vec![
                art::smooth(
                    &[
                        p(0.0, 2.0),
                        p(9.0, -2.0),
                        p(11.0, -10.0),
                        p(4.0, -14.0),
                        p(0.0, -12.0),
                        p(-4.0, -14.0),
                        p(-11.0, -10.0),
                        p(-9.0, -2.0),
                    ],
                    8,
                ),
                oval(p(-13.0, 8.0), 4.0, 5.5, 0.4),
                oval(p(-5.0, 14.0), 4.2, 5.8, 0.1),
                oval(p(5.0, 14.0), 4.2, 5.8, -0.1),
                oval(p(13.0, 8.0), 4.0, 5.5, -0.4),
            ])
        }),
    ]
}

fn nautical() -> Vec<Motif> {
    vec![
        ("Anchor", &["anchor", "boat", "sea"], || {
            let ring = art::band(&disc(p(0.0, 30.0), 5.0), 3.0)?;
            let shaft = bar(p(0.0, 25.0), p(0.0, -26.0), 5.0);
            let stock = art::rect(p(0.0, 18.0), 26.0, 4.0);
            let arc_line: Vec<Point> = (0..=60)
                .map(|k| {
                    let a = PI * 1.05 + PI * 0.9 * k as f64 / 60.0;
                    p(26.0 * a.cos(), -4.0 + 26.0 * a.sin())
                })
                .collect();
            let mut parts = art::stroke(&arc_line, 5.0)?;
            parts.push(shaft);
            parts.push(stock);
            let tip_l = arc_line[0];
            let tip_r = arc_line[arc_line.len() - 1];
            parts.push(vec![
                p(tip_l.x - 5.0, tip_l.y - 1.0),
                p(tip_l.x + 3.0, tip_l.y + 10.0),
                p(tip_l.x + 4.0, tip_l.y - 1.0),
            ]);
            parts.push(vec![
                p(tip_r.x + 5.0, tip_r.y - 1.0),
                p(tip_r.x - 4.0, tip_r.y - 1.0),
                p(tip_r.x - 3.0, tip_r.y + 10.0),
            ]);
            art::merge(&[ring, art::union(&parts)?])
        }),
        ("Ship Wheel", &["wheel", "helm", "ship"], || {
            let rim = art::band(&disc(p(0.0, 0.0), 22.0), 4.0)?;
            let hub = disc(p(0.0, 0.0), 6.0);
            let mut spokes = vec![hub];
            for k in 0..8 {
                let a = TAU * k as f64 / 8.0;
                spokes.push(bar(p(0.0, 0.0), p(33.0 * a.cos(), 33.0 * a.sin()), 3.0));
                spokes.push(disc(p(33.0 * a.cos(), 33.0 * a.sin()), 3.2));
            }
            let wheel = art::merge(&[rim, art::union(&spokes)?])?;
            with_holes(&wheel, &[disc(p(0.0, 0.0), 2.0)])
        }),
        (
            "Compass Rose",
            &["compass", "rose", "north", "navigation"],
            || {
                let star = art::union(&[
                    art::star(4, 34.0, 7.0, 0.0),
                    art::star(4, 22.0, 5.0, PI / 4.0),
                ])?;
                // The ring shows between the points, kept clear of them.
                let ring = art::cut(
                    &art::band(&disc(p(0.0, 0.0), 26.0), 2.0)?,
                    &art::offset(&star, 1.0),
                )?;
                Ok([star, ring].concat())
            },
        ),
        ("Sailboat", &["sailboat", "boat", "sailing"], || {
            Ok(vec![
                vec![
                    p(-30.0, -14.0),
                    p(30.0, -14.0),
                    p(22.0, -24.0),
                    p(-22.0, -24.0),
                ],
                vec![p(1.0, -10.0), p(1.0, 32.0), p(24.0, -10.0)],
                vec![p(-2.0, -10.0), p(-2.0, 26.0), p(-20.0, -10.0)],
            ])
        }),
        ("Lighthouse", &["lighthouse", "coast", "beacon"], || {
            let tower = vec![p(-10.0, -30.0), p(10.0, -30.0), p(6.0, 14.0), p(-6.0, 14.0)];
            let lamp = art::rect(p(0.0, 18.0), 10.0, 8.0);
            let roof = vec![p(-8.0, 22.0), p(8.0, 22.0), p(0.0, 30.0)];
            let gallery = art::rect(p(0.0, 14.0), 18.0, 2.5);
            let body = art::union(&[tower, lamp, roof, gallery])?;
            with_holes(
                &body,
                &[
                    art::rect(p(0.0, 18.0), 6.0, 4.0),
                    art::rect(p(0.0, -8.0), 18.0, 4.0),
                    art::rect(p(0.0, -20.0), 22.0, 4.0),
                ],
            )
        }),
        ("Scallop Shell", &["shell", "scallop", "beach"], || {
            let fan = art::polar(360, |t| {
                let a = (t - PI / 2.0 + PI).rem_euclid(TAU) - PI;
                if a.abs() > 1.05 {
                    return 6.0;
                }
                28.0 * (1.0 + 0.05 * (13.0 * a * PI / 2.1).cos().abs())
            });
            let hinge = art::rect(p(0.0, -4.0), 16.0, 6.0);
            let shell = art::union(&[fan, hinge])?;
            let mut grooves = Vec::new();
            for k in 0..6 {
                let a = PI / 2.0 - 0.75 + 1.5 * k as f64 / 5.0;
                grooves.push(bar(
                    p(3.0 * a.cos(), 3.0 * a.sin()),
                    p(22.0 * a.cos(), 22.0 * a.sin()),
                    1.4,
                ));
            }
            with_holes(&shell, &grooves)
        }),
        ("Starfish", &["starfish", "sea star", "beach"], || {
            Ok(vec![art::smooth(&art::star(5, 30.0, 12.0, 0.0), 6)])
        }),
        ("Life Ring", &["life ring", "buoy", "rescue"], || {
            let ring = art::band(&disc(p(0.0, 0.0), 24.0), 12.0)?;
            let mut cuts = Vec::new();
            for k in 0..4 {
                let a = PI / 4.0 + PI / 2.0 * k as f64;
                cuts.push(bar(
                    p(15.0 * a.cos(), 15.0 * a.sin()),
                    p(33.0 * a.cos(), 33.0 * a.sin()),
                    1.5,
                ));
            }
            with_holes(&ring, &cuts)
        }),
        ("Octopus", &["octopus", "sea", "tentacles"], || {
            let mut parts = vec![art::mirrored(
                &[
                    p(0.0, 34.0),
                    p(12.0, 30.0),
                    p(15.0, 16.0),
                    p(10.0, 4.0),
                    p(0.0, 2.0),
                ],
                8,
            )];
            for k in 0..6 {
                let x0 = -10.0 + 4.0 * k as f64;
                let side = if k < 3 { -1.0 } else { 1.0 };
                let spread = (k as f64 - 2.5) * 5.0;
                parts.push(limb(
                    &[
                        p(x0, 6.0),
                        p(x0 + spread * 0.6, -8.0),
                        p(x0 + spread, -20.0),
                        p(x0 + spread + side * 6.0, -24.0),
                    ],
                    5.0,
                    1.5,
                ));
            }
            let body = art::union(&parts)?;
            with_holes(&body, &[disc(p(-5.0, 20.0), 2.2), disc(p(5.0, 20.0), 2.2)])
        }),
        ("Crab", &["crab", "beach", "sea"], || {
            let mut parts = vec![oval(p(0.0, 0.0), 18.0, 12.0, 0.0)];
            for side in [-1.0, 1.0] {
                parts.push(limb(
                    &[
                        p(side * 14.0, 6.0),
                        p(side * 22.0, 14.0),
                        p(side * 20.0, 22.0),
                    ],
                    3.5,
                    2.5,
                ));
                parts.push(
                    art::cut(
                        &[disc(p(side * 20.0, 26.0), 6.0)],
                        &[vec![
                            p(side * 20.0, 26.0),
                            p(side * 23.0, 36.0),
                            p(side * 17.0, 36.0),
                        ]],
                    )?
                    .remove(0),
                );
                for k in 0..3 {
                    let y = -2.0 - 4.0 * k as f64;
                    parts.push(limb(
                        &[
                            p(side * 15.0, y),
                            p(side * 24.0, y - 3.0),
                            p(side * 28.0, y - 8.0),
                        ],
                        2.2,
                        1.2,
                    ));
                }
            }
            art::union(&parts)
        }),
        ("Wave Crest", &["wave", "surf", "ocean"], || {
            Ok(vec![art::smooth(
                &[
                    p(-35.0, -15.0),
                    p(35.0, -15.0),
                    p(30.0, 0.0),
                    p(20.0, 14.0),
                    p(4.0, 20.0),
                    p(-8.0, 16.0),
                    p(-12.0, 8.0),
                    p(-6.0, 2.0),
                    p(0.0, 6.0),
                    p(-4.0, 10.0),
                    p(2.0, 12.0),
                    p(8.0, 4.0),
                    p(4.0, -4.0),
                    p(-10.0, -6.0),
                    p(-24.0, -8.0),
                ],
                8,
            )])
        }),
        ("Crossed Oars", &["oars", "rowing", "boat"], || {
            let oar = |turn: f64| -> Vec<Ring> {
                vec![
                    art::moved(
                        &art::rect(p(0.0, 0.0), 3.0, 60.0),
                        &art::place(turn, 0.0, 0.0),
                    ),
                    art::moved(
                        &oval(p(0.0, 30.0), 5.5, 11.0, 0.0),
                        &art::place(turn, 0.0, 0.0),
                    ),
                ]
            };
            art::union(&[oar(0.6), oar(-0.6)].concat())
        }),
    ]
}

fn western() -> Vec<Motif> {
    vec![
        ("Horseshoe", &["horseshoe", "lucky", "horse"], || {
            let line: Vec<Point> = (0..=100)
                .map(|k| {
                    let a = -PI * 0.25 + PI * 1.5 * k as f64 / 100.0;
                    p(20.0 * a.cos(), 22.0 * a.sin())
                })
                .collect();
            let shoe = art::union(&art::stroke(&line, 9.0)?)?;
            let nails: Vec<Ring> = (0..8)
                .map(|k| {
                    let a = -PI * 0.15 + PI * 1.3 * k as f64 / 7.0;
                    disc(p(20.0 * a.cos(), 22.0 * a.sin()), 1.2)
                })
                .collect();
            with_holes(&shoe, &nails)
        }),
        ("Cowboy Hat", &["cowboy", "hat", "western"], || {
            art::union(&[
                art::smooth(
                    &[
                        p(-38.0, -4.0),
                        p(-26.0, -10.0),
                        p(0.0, -12.0),
                        p(26.0, -10.0),
                        p(38.0, -4.0),
                        p(34.0, -2.0),
                        p(20.0, -6.0),
                        p(-20.0, -6.0),
                        p(-34.0, -2.0),
                    ],
                    8,
                ),
                art::smooth(
                    &[
                        p(-16.0, -8.0),
                        p(-18.0, 8.0),
                        p(-10.0, 16.0),
                        p(0.0, 12.0),
                        p(10.0, 16.0),
                        p(18.0, 8.0),
                        p(16.0, -8.0),
                    ],
                    8,
                ),
            ])
        }),
        ("Cowboy Boot", &["cowboy", "boot", "western"], || {
            let boot = art::smooth(
                &[
                    p(-10.0, 34.0),
                    p(8.0, 34.0),
                    p(7.0, 6.0),
                    p(12.0, -8.0),
                    p(28.0, -14.0),
                    p(30.0, -22.0),
                    p(-12.0, -22.0),
                    p(-14.0, -12.0),
                    p(-12.0, 10.0),
                ],
                8,
            );
            let heel = art::rect(p(-9.0, -25.0), 8.0, 6.0);
            art::union(&[boot, heel])
        }),
        ("Cactus", &["cactus", "saguaro", "desert"], || {
            art::union(&[
                art::rounded_rect(p(0.0, 0.0), 12.0, 64.0, 6.0),
                limb(
                    &[p(0.0, -4.0), p(-12.0, -4.0), p(-16.0, 4.0), p(-16.0, 16.0)],
                    9.0,
                    9.0,
                ),
                limb(
                    &[p(0.0, 4.0), p(12.0, 4.0), p(16.0, 10.0), p(16.0, 22.0)],
                    9.0,
                    9.0,
                ),
            ])
        }),
        ("Wagon Wheel", &["wagon", "wheel", "western"], || {
            let rim = art::band(&disc(p(0.0, 0.0), 30.0), 5.0)?;
            let mut spokes = vec![disc(p(0.0, 0.0), 6.0)];
            for k in 0..12 {
                let a = TAU * k as f64 / 12.0;
                spokes.push(bar(p(0.0, 0.0), p(29.0 * a.cos(), 29.0 * a.sin()), 2.4));
            }
            let wheel = art::merge(&[rim, art::union(&spokes)?])?;
            with_holes(&wheel, &[disc(p(0.0, 0.0), 2.4)])
        }),
        (
            "Longhorn Skull",
            &["longhorn", "skull", "steer", "western"],
            || {
                let skull = art::mirrored(
                    &[
                        p(0.0, 14.0),
                        p(9.0, 13.0),
                        p(10.0, 2.0),
                        p(7.0, -14.0),
                        p(3.0, -20.0),
                        p(0.0, -21.0),
                    ],
                    8,
                );
                let horns = [
                    limb(
                        &[p(6.0, 10.0), p(20.0, 12.0), p(32.0, 10.0), p(38.0, 16.0)],
                        6.0,
                        1.2,
                    ),
                    limb(
                        &[
                            p(-6.0, 10.0),
                            p(-20.0, 12.0),
                            p(-32.0, 10.0),
                            p(-38.0, 16.0),
                        ],
                        6.0,
                        1.2,
                    ),
                ];
                let mut parts = vec![skull];
                parts.extend(horns);
                let head = art::union(&parts)?;
                with_holes(
                    &head,
                    &[
                        oval(p(-4.5, 4.0), 2.5, 3.5, 0.3),
                        oval(p(4.5, 4.0), 2.5, 3.5, -0.3),
                        disc(p(-2.0, -15.0), 1.2),
                        disc(p(2.0, -15.0), 1.2),
                    ],
                )
            },
        ),
        ("Arrowhead", &["arrowhead", "flint", "stone"], || {
            Ok(vec![vec![
                p(0.0, 32.0),
                p(12.0, 2.0),
                p(11.0, -10.0),
                p(6.0, -10.0),
                p(4.0, -16.0),
                p(8.0, -24.0),
                p(-8.0, -24.0),
                p(-4.0, -16.0),
                p(-6.0, -10.0),
                p(-11.0, -10.0),
                p(-12.0, 2.0),
            ]])
        }),
        ("Feather", &["feather", "quill", "bird"], || {
            let vane = super::floral::leaf_outline(56.0, |t| {
                let notch = if (0.45..0.5).contains(&t) { 0.6 } else { 1.0 };
                11.0 * (std::f64::consts::PI * t).sin().powf(0.6) * notch
            });
            let vane = art::moved(&vane, &art::place(-0.35, 0.0, -18.0));
            let quill = limb(&[p(0.0, -30.0), p(0.0, -18.0), p(-6.0, 16.0)], 2.2, 1.0);
            let feather = art::union(&[vane, quill])?;
            Ok(feather)
        }),
        ("Lasso", &["lasso", "rope", "cowboy"], || {
            let mut rings = art::band(&oval(p(0.0, 6.0), 26.0, 16.0, 0.0), 3.0)?;
            let tail = art::stroke(
                &art::smooth_open(
                    &[
                        p(22.0, -2.0),
                        p(26.0, -14.0),
                        p(18.0, -24.0),
                        p(28.0, -30.0),
                    ],
                    10,
                ),
                3.0,
            )?;
            rings = art::merge(&[rings, tail])?;
            Ok(rings)
        }),
        ("Spur", &["spur", "cowboy", "western"], || {
            let rowel = art::star(8, 12.0, 4.0, 0.0);
            let rowel = art::moved(&rowel, &art::place(0.0, 22.0, 0.0));
            let shank = limb(
                &[p(-24.0, 6.0), p(-8.0, 2.0), p(10.0, 1.0), p(18.0, 0.0)],
                5.0,
                3.0,
            );
            let band = art::band(&oval(p(-24.0, 0.0), 8.0, 12.0, 0.0), 3.0)?;
            art::merge(&[band, art::union(&[shank, rowel])?])
        }),
    ]
}

/// A snowflake with six arms: `branches` pairs of side branches at the
/// given fractions along each arm.
fn snowflake(branches: &[(f64, f64)], tip_disc: bool) -> Result<Vec<Ring>, CoreError> {
    let mut parts = vec![art::polygon(6, 5.0, 0.0)];
    for k in 0..6 {
        let a = PI / 2.0 + TAU * k as f64 / 6.0;
        let dir = |r: f64, turn: f64| p(r * (a + turn).cos(), r * (a + turn).sin());
        parts.push(bar(p(0.0, 0.0), dir(32.0, 0.0), 3.0));
        for &(at, len) in branches {
            let base = dir(32.0 * at, 0.0);
            for side in [-1.0, 1.0] {
                let b = a + side * PI / 3.0;
                parts.push(bar(
                    base,
                    p(base.x + len * b.cos(), base.y + len * b.sin()),
                    2.4,
                ));
            }
        }
        if tip_disc {
            parts.push(disc(dir(32.0, 0.0), 2.6));
        }
    }
    art::union(&parts)
}

fn holidays() -> Vec<Motif> {
    vec![
        ("Christmas Tree", &["christmas", "tree", "pine"], || {
            art::union(&[
                vec![
                    p(0.0, 34.0),
                    p(14.0, 16.0),
                    p(6.0, 16.0),
                    p(20.0, 0.0),
                    p(10.0, 0.0),
                    p(26.0, -18.0),
                    p(-26.0, -18.0),
                    p(-10.0, 0.0),
                    p(-20.0, 0.0),
                    p(-6.0, 16.0),
                    p(-14.0, 16.0),
                ],
                art::rect(p(0.0, -23.0), 8.0, 10.0),
            ])
        }),
        ("Ornament", &["christmas", "ornament", "bauble"], || {
            let ball = art::union(&[disc(p(0.0, -4.0), 22.0), art::rect(p(0.0, 19.0), 10.0, 6.0)])?;
            let hook = art::band(&disc(p(0.0, 25.0), 3.5), 1.6)?;
            let ball = with_holes(
                &ball,
                &[
                    art::rect(p(0.0, 0.0), 50.0, 2.0),
                    art::rect(p(0.0, -8.0), 50.0, 2.0),
                ],
            )?;
            art::merge(&[ball, hook])
        }),
        ("Bell", &["christmas", "bell", "wedding"], || {
            art::union(&[
                art::mirrored(
                    &[
                        p(0.0, 26.0),
                        p(8.0, 24.0),
                        p(12.0, 12.0),
                        p(14.0, -4.0),
                        p(24.0, -14.0),
                        p(0.0, -14.0),
                    ],
                    8,
                ),
                disc(p(0.0, -18.0), 4.0),
                disc(p(0.0, 28.0), 3.0),
            ])
        }),
        ("Candy Cane", &["christmas", "candy cane", "sweet"], || {
            let line: Vec<Point> = [p(6.0, -32.0), p(6.0, 14.0)]
                .into_iter()
                .chain((1..=40).map(|k| {
                    let a = PI * k as f64 / 40.0;
                    p(-4.0 + 10.0 * a.cos(), 14.0 + 10.0 * a.sin())
                }))
                .chain([p(-14.0, 10.0)])
                .collect();
            let cane = art::union(&art::stroke(&line, 7.0)?)?;
            let stripes: Vec<Ring> = (0..6)
                .map(|k| {
                    art::moved(
                        &art::rect(p(0.0, 0.0), 14.0, 2.4),
                        &art::place(0.6, 6.0, -26.0 + 7.0 * k as f64),
                    )
                })
                .collect();
            with_holes(&cane, &stripes)
        }),
        ("Snowflake", &["snowflake", "winter", "christmas"], || {
            snowflake(&[(0.45, 8.0), (0.75, 5.0)], false)
        }),
        ("Snowflake Star", &["snowflake", "winter", "star"], || {
            snowflake(&[(0.6, 10.0)], true)
        }),
        ("Snowflake Fern", &["snowflake", "winter", "frost"], || {
            snowflake(&[(0.3, 5.0), (0.55, 6.0), (0.8, 4.0)], false)
        }),
        (
            "Snowflake Crystal",
            &["snowflake", "winter", "crystal"],
            || snowflake(&[(0.5, 12.0)], false),
        ),
        (
            "Gift Box",
            &["gift", "present", "birthday", "christmas"],
            || {
                let bow = art::union(&[
                    oval(p(-7.0, 25.0), 7.0, 4.0, 0.4),
                    oval(p(7.0, 25.0), 7.0, 4.0, -0.4),
                    disc(p(0.0, 23.0), 3.0),
                ])?;
                let lid = art::rect(p(0.0, 16.0), 50.0, 8.0);
                let box_ = art::rect(p(0.0, -8.0), 44.0, 38.0);
                let body = art::union(&[lid, box_])?;
                let body = with_holes(
                    &body,
                    &[
                        art::rect(p(0.0, -8.0), 5.0, 36.0),
                        art::rect(p(0.0, 11.5), 60.0, 1.5),
                    ],
                )?;
                let bow = with_holes(&bow, &[art::rect(p(0.0, 18.0), 60.0, 6.0)])?;
                Ok([body, bow].concat())
            },
        ),
        ("Stocking", &["christmas", "stocking"], || {
            art::union(&[
                art::smooth(
                    &[
                        p(-10.0, 22.0),
                        p(10.0, 22.0),
                        p(10.0, -4.0),
                        p(20.0, -14.0),
                        p(18.0, -26.0),
                        p(4.0, -28.0),
                        p(-8.0, -18.0),
                        p(-12.0, -4.0),
                    ],
                    8,
                ),
                art::rounded_rect(p(0.0, 26.0), 26.0, 9.0, 3.0),
            ])
        }),
        ("Snowman", &["snowman", "winter", "christmas"], || {
            let body = art::union(&[
                disc(p(0.0, -18.0), 16.0),
                disc(p(0.0, 6.0), 11.0),
                disc(p(0.0, 23.0), 8.0),
                art::rect(p(0.0, 32.0), 14.0, 6.0),
                art::rect(p(0.0, 29.5), 22.0, 2.0),
            ])?;
            with_holes(
                &body,
                &[
                    disc(p(-3.0, 25.0), 1.3),
                    disc(p(3.0, 25.0), 1.3),
                    disc(p(0.0, 8.0), 1.4),
                    disc(p(0.0, 2.0), 1.4),
                ],
            )
        }),
        ("Holly Sprig", &["holly", "christmas", "berries"], || {
            let leaf = super::floral::leaf_outline(30.0, |t| {
                let spikes = 2.0 * ((t * 4.0).fract() - 0.5).abs();
                9.0 * (PI * t).sin().powf(0.6) * (0.72 + 0.28 * spikes.powi(3))
            });
            art::union(&[
                art::moved(&leaf, &art::place(1.0, -2.0, 0.0)),
                art::moved(&leaf, &art::place(-1.0, 2.0, 0.0)),
                disc(p(-4.0, 3.0), 4.0),
                disc(p(4.0, 3.0), 4.0),
                disc(p(0.0, 9.0), 4.0),
            ])
        }),
        (
            "Pumpkin",
            &["pumpkin", "halloween", "autumn", "harvest"],
            || {
                art::union(&[
                    oval(p(-10.0, 0.0), 14.0, 18.0, 0.0),
                    oval(p(10.0, 0.0), 14.0, 18.0, 0.0),
                    oval(p(0.0, 0.0), 14.0, 19.0, 0.0),
                    limb(&[p(0.0, 16.0), p(1.0, 24.0), p(5.0, 28.0)], 4.0, 3.0),
                ])
            },
        ),
        (
            "Jack O Lantern",
            &["pumpkin", "halloween", "jack o lantern"],
            || {
                let body = art::union(&[
                    oval(p(-10.0, 0.0), 14.0, 18.0, 0.0),
                    oval(p(10.0, 0.0), 14.0, 18.0, 0.0),
                    oval(p(0.0, 0.0), 14.0, 19.0, 0.0),
                    limb(&[p(0.0, 16.0), p(1.0, 24.0), p(5.0, 28.0)], 4.0, 3.0),
                ])?;
                with_holes(
                    &body,
                    &[
                        vec![p(-12.0, 4.0), p(-4.0, 4.0), p(-8.0, 11.0)],
                        vec![p(4.0, 4.0), p(12.0, 4.0), p(8.0, 11.0)],
                        vec![
                            p(-14.0, -4.0),
                            p(-8.0, -6.0),
                            p(-5.0, -3.0),
                            p(0.0, -6.0),
                            p(5.0, -3.0),
                            p(8.0, -6.0),
                            p(14.0, -4.0),
                            p(8.0, -11.0),
                            p(-8.0, -11.0),
                        ],
                    ],
                )
            },
        ),
        ("Ghost", &["ghost", "halloween", "spooky"], || {
            let body = art::smooth(
                &[
                    p(0.0, 30.0),
                    p(15.0, 24.0),
                    p(18.0, 4.0),
                    p(20.0, -22.0),
                    p(12.0, -16.0),
                    p(6.0, -24.0),
                    p(0.0, -17.0),
                    p(-6.0, -24.0),
                    p(-12.0, -16.0),
                    p(-20.0, -22.0),
                    p(-18.0, 4.0),
                    p(-15.0, 24.0),
                ],
                8,
            );
            with_holes(
                &[body],
                &[
                    oval(p(-6.0, 12.0), 2.8, 4.0, 0.0),
                    oval(p(6.0, 12.0), 2.8, 4.0, 0.0),
                    oval(p(0.0, 2.0), 3.0, 4.0, 0.0),
                ],
            )
        }),
        ("Bat", &["bat", "halloween", "night"], || {
            let wing = art::smooth(
                &[
                    p(0.0, 4.0),
                    p(14.0, 12.0),
                    p(30.0, 14.0),
                    p(36.0, 4.0),
                    p(30.0, 0.0),
                    p(26.0, -6.0),
                    p(20.0, -2.0),
                    p(14.0, -8.0),
                    p(8.0, -4.0),
                    p(2.0, -6.0),
                ],
                8,
            );
            let left: Ring = wing.iter().map(|q| p(-q.x, q.y)).collect();
            art::union(&[
                wing,
                left,
                oval(p(0.0, 0.0), 5.0, 8.0, 0.0),
                disc(p(0.0, 9.0), 4.5),
                vec![p(-4.0, 11.0), p(-3.0, 17.0), p(-1.0, 12.0)],
                vec![p(4.0, 11.0), p(3.0, 17.0), p(1.0, 12.0)],
            ])
        }),
        ("Witch Hat", &["witch", "hat", "halloween"], || {
            art::union(&[
                oval(p(0.0, -18.0), 34.0, 6.0, 0.0),
                art::smooth(
                    &[
                        p(-16.0, -16.0),
                        p(-8.0, 6.0),
                        p(0.0, 24.0),
                        p(12.0, 34.0),
                        p(6.0, 22.0),
                        p(8.0, 4.0),
                        p(16.0, -16.0),
                    ],
                    8,
                ),
            ])
        }),
        ("Easter Egg", &["easter", "egg", "spring"], || {
            let egg = art::mirrored(
                &[
                    p(0.0, 30.0),
                    p(12.0, 24.0),
                    p(20.0, 6.0),
                    p(18.0, -14.0),
                    p(10.0, -24.0),
                    p(0.0, -26.0),
                ],
                8,
            );
            let zig: Vec<Point> = (0..=12)
                .map(|k| p(-24.0 + 4.0 * k as f64, if k % 2 == 0 { -2.0 } else { 4.0 }))
                .collect();
            let wave: Vec<Point> = (0..=60)
                .map(|k| {
                    let x = -24.0 + 0.8 * k as f64;
                    p(x, 14.0 + 2.0 * (x * 0.6).sin())
                })
                .collect();
            let mut holes = art::stroke(&zig, 2.0)?;
            holes.extend(art::stroke(&wave, 1.6)?);
            holes.extend([
                disc(p(-8.0, -12.0), 2.2),
                disc(p(0.0, -14.0), 2.2),
                disc(p(8.0, -12.0), 2.2),
            ]);
            with_holes(&[egg], &holes)
        }),
        ("Bunny Head", &["easter", "bunny", "rabbit"], || {
            let head = art::union(&[
                disc(p(0.0, -8.0), 16.0),
                oval(p(-7.0, 18.0), 5.0, 16.0, 0.15),
                oval(p(7.0, 18.0), 5.0, 16.0, -0.15),
            ])?;
            with_holes(
                &head,
                &[
                    disc(p(-6.0, -4.0), 2.0),
                    disc(p(6.0, -4.0), 2.0),
                    oval(p(0.0, -11.0), 2.5, 2.0, 0.0),
                ],
            )
        }),
        ("Cross", &["cross", "easter", "church"], || {
            art::union(&[
                art::rect(p(0.0, 0.0), 10.0, 64.0),
                art::rect(p(0.0, 12.0), 40.0, 10.0),
            ])
        }),
    ]
}

fn sports() -> Vec<Motif> {
    vec![
        ("Soccer Ball", &["soccer", "football", "ball"], || {
            let mut holes = vec![art::polygon(5, 8.0, 0.0)];
            for k in 0..5 {
                let a = PI / 2.0 + TAU * k as f64 / 5.0 + PI / 5.0;
                holes.push(art::moved(
                    &art::polygon(5, 6.0, PI / 5.0 + TAU * k as f64 / 5.0),
                    &art::place(0.0, 22.0 * a.cos(), 22.0 * a.sin()),
                ));
            }
            with_holes(&[disc(p(0.0, 0.0), 30.0)], &holes)
        }),
        ("Basketball", &["basketball", "ball", "hoop"], || {
            let mut seams = vec![
                art::rect(p(0.0, 0.0), 64.0, 2.0),
                art::rect(p(0.0, 0.0), 2.0, 64.0),
            ];
            for side in [-1.0, 1.0] {
                let arc: Vec<Point> = (0..=60)
                    .map(|k| {
                        let a = -PI / 2.0 + PI * k as f64 / 60.0;
                        p(side * (40.0 - 26.0 * a.cos()), 26.0 * a.sin())
                    })
                    .collect();
                seams.extend(art::stroke(&arc, 2.0)?);
            }
            with_holes(&[disc(p(0.0, 0.0), 30.0)], &seams)
        }),
        ("Baseball", &["baseball", "ball"], || {
            let mut stitches = Vec::new();
            for side in [-1.0, 1.0] {
                for k in 0..9 {
                    let a = -PI * 0.35 + PI * 0.7 * k as f64 / 8.0;
                    let (cx, cy) = (side * (42.0 - 30.0 * a.cos()), 30.0 * a.sin());
                    stitches.push(art::moved(
                        &art::rect(p(0.0, 0.0), 5.0, 1.4),
                        &art::place(a + side * 0.6, cx, cy),
                    ));
                }
            }
            with_holes(&[disc(p(0.0, 0.0), 30.0)], &stitches)
        }),
        (
            "American Football",
            &["football", "ball", "american"],
            || {
                let ball = art::polar(360, |t| {
                    let c = t.cos();
                    let s = t.sin();
                    // A lens: pointed ends along X.
                    let rx = 34.0;
                    let ry = 18.0;
                    1.0 / ((c / rx).powi(2) + (s / ry).powi(2) * (1.0 + 0.8 * c * c)).sqrt()
                });
                let mut laces = vec![art::rect(p(0.0, 6.0), 22.0, 2.0)];
                for k in 0..5 {
                    laces.push(art::rect(p(-8.0 + 4.0 * k as f64, 6.0), 1.6, 6.0));
                }
                let laces = art::union(&laces)?;
                with_holes(&[ball], &laces)
            },
        ),
        ("Tennis Ball", &["tennis", "ball"], || {
            let mut seams = Vec::new();
            for side in [-1.0, 1.0] {
                let arc: Vec<Point> = (0..=60)
                    .map(|k| {
                        let a = -PI * 0.42 + PI * 0.84 * k as f64 / 60.0;
                        p(side * (36.0 - 26.0 * a.cos()), 26.0 * a.sin())
                    })
                    .collect();
                seams.extend(art::stroke(&arc, 2.2)?);
            }
            with_holes(&[disc(p(0.0, 0.0), 26.0)], &seams)
        }),
        ("Golf Flag", &["golf", "flag", "hole"], || {
            let pole = art::rect(p(0.0, 0.0), 2.4, 60.0);
            let flag = vec![p(1.0, 30.0), p(24.0, 22.0), p(1.0, 14.0)];
            let green = oval(p(0.0, -30.0), 20.0, 5.0, 0.0);
            let shape = art::union(&[pole, flag, green])?;
            with_holes(&shape, &[oval(p(0.0, -30.0), 4.0, 1.6, 0.0)])
        }),
        ("Bowling Pin", &["bowling", "pin", "skittle"], || {
            let pin = art::mirrored(
                &[
                    p(0.0, 34.0),
                    p(5.0, 32.0),
                    p(6.0, 26.0),
                    p(3.5, 18.0),
                    p(4.0, 12.0),
                    p(10.0, 0.0),
                    p(10.0, -16.0),
                    p(6.0, -28.0),
                    p(0.0, -30.0),
                ],
                8,
            );
            with_holes(
                &[pin],
                &[
                    art::rect(p(0.0, 16.0), 12.0, 1.6),
                    art::rect(p(0.0, 12.5), 12.0, 1.6),
                ],
            )
        }),
        ("Trophy", &["trophy", "cup", "winner", "award"], || {
            let cup = art::mirrored(
                &[
                    p(0.0, 30.0),
                    p(16.0, 30.0),
                    p(14.0, 12.0),
                    p(6.0, 2.0),
                    p(0.0, 0.0),
                ],
                1,
            );
            let handles = art::merge(&[
                art::band(&oval(p(-17.0, 20.0), 6.0, 7.0, 0.0), 2.5)?,
                art::band(&oval(p(17.0, 20.0), 6.0, 7.0, 0.0), 2.5)?,
            ])?;
            let stem = art::rect(p(0.0, -8.0), 5.0, 18.0);
            let base = art::rect(p(0.0, -20.0), 26.0, 7.0);
            let body = art::union(&[cup, stem, base])?;
            art::merge(&[body, handles])
        }),
        ("Medal", &["medal", "award", "winner"], || {
            let ribbon = art::union(&[
                vec![p(-12.0, 34.0), p(-4.0, 34.0), p(6.0, 6.0), p(-2.0, 6.0)],
                vec![p(12.0, 34.0), p(4.0, 34.0), p(-6.0, 6.0), p(2.0, 6.0)],
            ])?;
            let medal = with_holes(
                &[disc(p(0.0, -10.0), 17.0)],
                &[art::star(5, 11.0, 4.5, 0.0)
                    .iter()
                    .map(|q| p(q.x, q.y - 10.0))
                    .collect()],
            )?;
            let ribbon = with_holes(&ribbon, &[disc(p(0.0, -10.0), 18.5)])?;
            Ok([medal, ribbon].concat())
        }),
        ("Hockey Sticks", &["hockey", "sticks", "puck"], || {
            let stick = |flip: f64| -> Ring {
                let pts = vec![
                    p(flip * -24.0, 30.0),
                    p(flip * -18.0, 30.0),
                    p(flip * 14.0, -22.0),
                    p(flip * 26.0, -22.0),
                    p(flip * 26.0, -28.0),
                    p(flip * 8.0, -28.0),
                ];
                if flip < 0.0 {
                    pts.into_iter().rev().collect()
                } else {
                    pts
                }
            };
            let sticks = art::union(&[stick(1.0), stick(-1.0)])?;
            Ok([sticks, vec![oval(p(0.0, -34.0), 7.0, 2.5, 0.0)]].concat())
        }),
        (
            "Dumbbell",
            &["dumbbell", "weights", "gym", "fitness"],
            || {
                art::union(&[
                    art::rect(p(0.0, 0.0), 40.0, 4.0),
                    art::rounded_rect(p(-18.0, 0.0), 6.0, 24.0, 1.5),
                    art::rounded_rect(p(-25.0, 0.0), 6.0, 18.0, 1.5),
                    art::rounded_rect(p(18.0, 0.0), 6.0, 24.0, 1.5),
                    art::rounded_rect(p(25.0, 0.0), 6.0, 18.0, 1.5),
                ])
            },
        ),
        ("Tennis Racket", &["tennis", "racket", "racquet"], || {
            let head = art::band(&oval(p(0.0, 12.0), 16.0, 20.0, 0.0), 3.0)?;
            let throat = art::union(&[
                vec![p(-6.0, -6.0), p(6.0, -6.0), p(2.0, -14.0), p(-2.0, -14.0)],
                art::rounded_rect(p(0.0, -26.0), 5.0, 24.0, 2.0),
            ])?;
            art::merge(&[head, throat])
        }),
    ]
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    for (category, reliefs, list) in [
        (
            "Animals/Animal Silhouettes",
            "Animals/Animal Reliefs",
            animals(),
        ),
        (
            "Nautical/Nautical Silhouettes",
            "Nautical/Nautical Reliefs",
            nautical(),
        ),
        (
            "Western/Western Silhouettes",
            "Western/Western Reliefs",
            western(),
        ),
        (
            "Holidays/Holiday Silhouettes",
            "Holidays/Holiday Reliefs",
            holidays(),
        ),
        (
            "Sports/Sports Silhouettes",
            "Sports/Sports Reliefs",
            sports(),
        ),
    ] {
        let mut add = Adder {
            out: &mut *out,
            category,
            source: SOURCE,
        };
        for (name, tags, make) in &list {
            let make = *make;
            add.vectors(name, tags, move || Ok(art::paths(&make()?)));
        }
        let mut add = Adder {
            out: &mut *out,
            category: reliefs,
            source: SOURCE,
        };
        for (name, tags, make) in list {
            let mut all = vec!["relief"];
            all.extend_from_slice(tags);
            add.relief(&format!("{name} Relief"), &all, move || {
                art::puffy(&make()?, 5.0)
            });
        }
    }
    let mut add = Adder {
        out,
        category: "Nautical/Nautical Reliefs",
        source: SOURCE,
    };
    add.relief(
        "Scallop Shell Classic",
        &["shell", "scallop", "sample"],
        || {
            crate::clipart::samples::sample("Shell")
                .map(|c| c.field)
                .ok_or_else(|| CoreError::validation("the shell sample is missing"))
        },
    );
    add.relief(
        "Compass Rose Medallion",
        &["compass", "rose", "navigation"],
        || {
            Ok(art::field_fn(70.0, 70.0, |x, y| {
                super::rosettes::compass_height(x, y, 34.0)
            }))
        },
    );
}
