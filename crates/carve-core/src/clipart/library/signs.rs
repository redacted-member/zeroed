//! Signs and arrows: arrows of many kinds, sign board shapes and simple
//! symbols, as vectors, and a set of them as reliefs.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::Point;
use crate::model::shape::Profile;

use super::art::{self, p, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "signs.rs";

/// A straight arrow along +X from `-len / 2` to `len / 2`: a shaft `shaft`
/// wide and a head `head` wide and `head_len` long.
fn arrow(len: f64, shaft: f64, head: f64, head_len: f64) -> Ring {
    let (a, b) = (-len / 2.0, len / 2.0);
    let neck = b - head_len;
    vec![
        p(a, -shaft / 2.0),
        p(neck, -shaft / 2.0),
        p(neck, -head / 2.0),
        p(b, 0.0),
        p(neck, head / 2.0),
        p(neck, shaft / 2.0),
        p(a, shaft / 2.0),
    ]
}

/// An arrow head pointing along `heading` (radians) with its tip at `tip`.
fn head_at(tip: Point, heading: f64, width: f64, len: f64) -> Ring {
    let back = p(tip.x - len * heading.cos(), tip.y - len * heading.sin());
    let (nx, ny) = (-heading.sin() * width / 2.0, heading.cos() * width / 2.0);
    vec![
        tip,
        p(back.x + nx, back.y + ny),
        p(back.x - nx, back.y - ny),
    ]
}

/// Points along an arc round `c` of radius `r` from angle `a0` to `a1`.
fn arc(c: Point, r: f64, a0: f64, a1: f64, n: usize) -> Vec<Point> {
    (0..=n)
        .map(|k| {
            let a = a0 + (a1 - a0) * k as f64 / n as f64;
            p(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// A band along an arc with a head at its end, for curved arrows.
fn curved(c: Point, r: f64, a0: f64, a1: f64, width: f64) -> Result<Vec<Ring>, CoreError> {
    let line = arc(c, r, a0, a1, 120);
    let tangent = if a1 > a0 {
        a1 + PI / 2.0
    } else {
        a1 - PI / 2.0
    };
    let end = line[line.len() - 1];
    let tip = p(
        end.x + 0.6 * width * tangent.cos(),
        end.y + 0.6 * width * tangent.sin(),
    );
    let mut rings = art::stroke(&line, width)?;
    rings.push(head_at(tip, tangent, width * 2.6, width * 2.2));
    art::union(&rings)
}

fn arrows() -> Vec<art::Design<Result<Vec<Ring>, CoreError>>> {
    vec![
        ("Block Arrow", &["arrow", "block", "direction"], || {
            Ok(vec![arrow(70.0, 16.0, 36.0, 24.0)])
        }),
        ("Slim Arrow", &["arrow", "slim", "direction"], || {
            Ok(vec![arrow(80.0, 5.0, 20.0, 16.0)])
        }),
        ("Double Arrow", &["arrow", "double", "both ways"], || {
            let mut ring = arrow(80.0, 8.0, 22.0, 16.0);
            ring.drain(0..1);
            ring.pop();
            let mut back: Ring = ring.iter().map(|q| p(-q.x, -q.y)).collect();
            ring.append(&mut back);
            art::union(&[ring])
        }),
        ("Notched Arrow", &["arrow", "notched", "tail"], || {
            let mut ring = arrow(70.0, 14.0, 34.0, 22.0);
            ring.insert(0, p(-27.0, 0.0));
            Ok(vec![ring])
        }),
        ("Chevron", &["chevron", "arrow", "direction"], || {
            Ok(vec![vec![
                p(-15.0, -25.0),
                p(0.0, -25.0),
                p(20.0, 0.0),
                p(0.0, 25.0),
                p(-15.0, 25.0),
                p(5.0, 0.0),
            ]])
        }),
        ("Triple Chevron", &["chevron", "arrow", "fast"], || {
            let one = vec![
                p(-8.0, -20.0),
                p(2.0, -20.0),
                p(16.0, 0.0),
                p(2.0, 20.0),
                p(-8.0, 20.0),
                p(6.0, 0.0),
            ];
            Ok((0..3)
                .map(|k| art::moved(&one, &art::place(0.0, -22.0 + 18.0 * k as f64, 0.0)))
                .collect())
        }),
        ("Curved Arrow", &["arrow", "curved", "turn"], || {
            curved(p(0.0, -20.0), 35.0, PI * 0.85, PI * 0.2, 7.0)
        }),
        ("U Turn Arrow", &["arrow", "u turn", "return"], || {
            let mut rings = art::stroke(&[p(-14.0, -30.0), p(-14.0, 5.0)], 7.0)?;
            rings.extend(curved(p(0.0, 5.0), 14.0, PI, 0.0, 7.0)?);
            art::union(&rings)
        }),
        (
            "Circular Arrow",
            &["arrow", "circular", "refresh", "repeat"],
            || curved(p(0.0, 0.0), 26.0, PI * 0.6, PI * 0.6 - TAU * 0.8, 6.0),
        ),
        ("Bent Arrow", &["arrow", "bent", "turn right"], || {
            let mut rings = art::stroke(&[p(-20.0, -30.0), p(-20.0, 10.0), p(15.0, 10.0)], 8.0)?;
            rings.push(head_at(p(35.0, 10.0), 0.0, 24.0, 20.0));
            art::union(&rings)
        }),
        ("Archery Arrow", &["arrow", "archery", "feathers"], || {
            let mut rings = art::stroke(&[p(-40.0, 0.0), p(32.0, 0.0)], 2.2)?;
            rings.push(head_at(p(45.0, 0.0), 0.0, 12.0, 14.0));
            for k in 0..3 {
                let x = -40.0 + 5.0 * k as f64;
                rings.push(vec![
                    p(x, 1.0),
                    p(x + 7.0, 1.0),
                    p(x + 2.0, 8.0),
                    p(x - 4.0, 8.0),
                ]);
                rings.push(vec![
                    p(x, -1.0),
                    p(x - 4.0, -8.0),
                    p(x + 2.0, -8.0),
                    p(x + 7.0, -1.0),
                ]);
            }
            art::union(&rings)
        }),
        ("Arrow in Circle", &["arrow", "circle", "button"], || {
            let mut rings = art::band(&art::circle(p(0.0, 0.0), 32.0, 360), 4.0)?;
            rings.push(arrow(38.0, 8.0, 22.0, 14.0));
            Ok(rings)
        }),
        ("Four Way Arrows", &["arrows", "move", "cross"], || {
            let one = arrow(30.0, 7.0, 18.0, 12.0);
            let arms: Vec<Ring> = (0..4)
                .map(|k| {
                    art::moved(
                        &one,
                        &art::place(
                            PI / 2.0 * k as f64,
                            15.0 * (PI / 2.0 * k as f64).cos(),
                            15.0 * (PI / 2.0 * k as f64).sin(),
                        ),
                    )
                })
                .collect();
            art::union(&arms)
        }),
        (
            "Rising Arrow",
            &["arrow", "rising", "growth", "trend"],
            || {
                let line = vec![p(-40.0, -25.0), p(-15.0, 0.0), p(0.0, -12.0), p(28.0, 16.0)];
                let mut rings = art::stroke(&line, 6.0)?;
                rings.push(head_at(p(40.0, 28.0), PI / 4.0, 20.0, 18.0));
                art::union(&rings)
            },
        ),
    ]
}

fn boards() -> Vec<art::Design<Result<Vec<Ring>, CoreError>>> {
    vec![
        (
            "Pointer Sign",
            &["sign", "pointer", "direction board"],
            || {
                Ok(vec![vec![
                    p(-45.0, -12.0),
                    p(30.0, -12.0),
                    p(45.0, 0.0),
                    p(30.0, 12.0),
                    p(-45.0, 12.0),
                ]])
            },
        ),
        ("Signpost", &["sign", "signpost", "direction"], || {
            let post = art::rect(p(0.0, -10.0), 6.0, 90.0);
            let right = vec![
                p(-3.0, 18.0),
                p(30.0, 18.0),
                p(40.0, 26.0),
                p(30.0, 34.0),
                p(-3.0, 34.0),
            ];
            let left = vec![
                p(3.0, -2.0),
                p(-30.0, -2.0),
                p(-40.0, 6.0),
                p(-30.0, 14.0),
                p(3.0, 14.0),
            ];
            art::union(&[post, right, left])
        }),
        ("Yield Sign", &["sign", "yield", "triangle"], || {
            let outer = art::polygon(3, 35.0, PI);
            Ok(art::double(&outer, 5.0))
        }),
        ("Warning Sign", &["sign", "warning", "diamond"], || {
            let outer = art::polygon(4, 35.0, 0.0);
            Ok(art::double(&outer, 4.0))
        }),
        ("Stop Sign", &["sign", "stop", "octagon"], || {
            let outer = art::polygon(8, 35.0, PI / 8.0);
            Ok(art::double(&outer, 3.0))
        }),
        (
            "Caution Triangle",
            &["sign", "caution", "exclamation"],
            || {
                let mut rings = art::band(&art::polygon(3, 36.0, 0.0), 5.0)?;
                rings.push(vec![p(-2.5, 8.0), p(2.5, 8.0), p(1.6, -6.0), p(-1.6, -6.0)]);
                rings.push(art::circle(p(0.0, -11.0), 2.4, 32));
                Ok(rings)
            },
        ),
        ("Speech Bubble", &["speech", "bubble", "talk"], || {
            let body = art::rounded_rect(p(0.0, 5.0), 70.0, 40.0, 12.0);
            let tail = vec![p(-18.0, -12.0), p(-26.0, -28.0), p(-6.0, -12.0)];
            art::union(&[body, tail])
        }),
        ("Thought Bubble", &["thought", "bubble", "cloud"], || {
            let cloud = art::union(&[
                art::circle(p(-18.0, 5.0), 14.0, 96),
                art::circle(p(0.0, 14.0), 16.0, 96),
                art::circle(p(18.0, 6.0), 14.0, 96),
                art::circle(p(0.0, -2.0), 16.0, 96),
            ])?;
            let mut rings = cloud;
            rings.push(art::circle(p(-20.0, -18.0), 4.0, 48));
            rings.push(art::circle(p(-28.0, -26.0), 2.5, 32));
            Ok(rings)
        }),
        ("Hanging Sign", &["sign", "hanging", "shop sign"], || {
            let board = art::rounded_rect(p(0.0, -8.0), 80.0, 36.0, 4.0);
            let rings_top = [
                art::circle(p(-28.0, 18.0), 5.0, 48),
                art::circle(p(28.0, 18.0), 5.0, 48),
            ];
            let mut shape = vec![board];
            for r in &rings_top {
                shape.extend(art::band(r, 2.0)?);
            }
            let mut all = art::merge(&[
                vec![shape[0].clone()],
                shape[1..3].to_vec(),
                shape[3..5].to_vec(),
            ])?;
            all.retain(|r| r.len() >= 3);
            Ok(all)
        }),
    ]
}

fn symbols() -> Vec<art::Design<Result<Vec<Ring>, CoreError>>> {
    vec![
        ("Check Mark", &["check", "tick", "done"], || {
            art::stroke(&[p(-25.0, 0.0), p(-8.0, -17.0), p(26.0, 20.0)], 8.0)
        }),
        ("Cross Mark", &["cross", "x", "no"], || {
            let mut rings = art::stroke(&[p(-20.0, -20.0), p(20.0, 20.0)], 8.0)?;
            rings.extend(art::stroke(&[p(-20.0, 20.0), p(20.0, -20.0)], 8.0)?);
            art::union(&rings)
        }),
        ("Plus", &["plus", "cross", "add"], || {
            art::union(&[
                art::rect(p(0.0, 0.0), 50.0, 14.0),
                art::rect(p(0.0, 0.0), 14.0, 50.0),
            ])
        }),
        ("House", &["house", "home", "building"], || {
            let outline = vec![
                p(-25.0, -25.0),
                p(25.0, -25.0),
                p(25.0, 5.0),
                p(32.0, 5.0),
                p(0.0, 30.0),
                p(-32.0, 5.0),
                p(-25.0, 5.0),
            ];
            art::cut(
                &[outline],
                &[
                    art::rect(p(0.0, -15.0), 12.0, 20.0),
                    art::rect(p(-14.0, -4.0), 8.0, 8.0),
                ],
            )
        }),
        ("Crown", &["crown", "king", "queen", "royal"], || {
            let body = vec![
                p(-30.0, -18.0),
                p(30.0, -18.0),
                p(34.0, 16.0),
                p(17.0, 0.0),
                p(0.0, 22.0),
                p(-17.0, 0.0),
                p(-34.0, 16.0),
            ];
            let balls = [
                art::circle(p(-34.0, 18.0), 4.0, 48),
                art::circle(p(0.0, 25.0), 4.5, 48),
                art::circle(p(34.0, 18.0), 4.0, 48),
            ];
            let mut rings = vec![body];
            rings.extend(balls);
            rings.push(art::rect(p(0.0, -22.0), 64.0, 7.0));
            art::union(&rings)
        }),
        ("Key", &["key", "lock", "old key"], || {
            let bow = art::band(&art::circle(p(-28.0, 0.0), 11.0, 120), 5.0)?;
            let shaft = art::rect(p(4.0, 0.0), 54.0, 6.0);
            let bits = vec![
                art::rect(p(24.0, -6.0), 5.0, 8.0),
                art::rect(p(15.0, -5.0), 4.0, 6.0),
            ];
            let mut blade = vec![shaft];
            blade.extend(bits);
            art::merge(&[bow, art::union(&blade)?])
        }),
        (
            "Lightning Bolt",
            &["lightning", "bolt", "electric", "storm"],
            || {
                Ok(vec![vec![
                    p(6.0, 40.0),
                    p(-18.0, 0.0),
                    p(-2.0, 0.0),
                    p(-10.0, -40.0),
                    p(18.0, 6.0),
                    p(2.0, 6.0),
                    p(14.0, 40.0),
                ]])
            },
        ),
        ("Water Drop", &["drop", "water", "rain"], || {
            Ok(vec![art::mirrored(
                &[
                    p(0.0, 35.0),
                    p(8.0, 18.0),
                    p(18.0, 0.0),
                    p(17.0, -12.0),
                    p(9.0, -20.0),
                    p(0.0, -22.0),
                ],
                10,
            )])
        }),
        ("Flame", &["flame", "fire", "campfire"], || {
            Ok(vec![art::smooth(
                &[
                    p(0.0, 40.0),
                    p(8.0, 22.0),
                    p(18.0, 10.0),
                    p(20.0, -8.0),
                    p(12.0, -20.0),
                    p(0.0, -24.0),
                    p(-12.0, -20.0),
                    p(-20.0, -6.0),
                    p(-14.0, 12.0),
                    p(-8.0, 6.0),
                    p(-6.0, 20.0),
                ],
                10,
            )])
        }),
        ("Gear", &["gear", "cog", "machine"], || {
            Ok(gear(12, 30.0, 24.0, 8.0))
        }),
        ("Crescent Moon", &["moon", "crescent", "night"], || {
            art::cut(
                &[art::circle(p(0.0, 0.0), 30.0, 240)],
                &[art::circle(p(12.0, 8.0), 26.0, 240)],
            )
        }),
        ("Sun", &["sun", "sunshine", "summer"], || {
            let mut rings = vec![art::circle(p(0.0, 0.0), 16.0, 160)];
            for k in 0..12 {
                let a = TAU * k as f64 / 12.0;
                rings.push(art::moved(
                    &[p(21.0, -3.5), p(34.0, 0.0), p(21.0, 3.5)],
                    &art::place(a, 0.0, 0.0),
                ));
            }
            Ok(rings)
        }),
        ("Music Note", &["music", "note", "song"], || {
            let head = art::moved(
                &art::ellipse(p(0.0, 0.0), 9.0, 6.5, 72),
                &art::place(0.4, -8.0, -24.0),
            );
            let stem = art::rect(p(0.0, 2.0), 3.6, 52.0);
            let flag = vec![
                p(-1.8, 28.0),
                p(12.0, 18.0),
                p(16.0, 6.0),
                p(12.0, 12.0),
                p(1.8, 18.0),
            ];
            art::union(&[head, stem, flag])
        }),
        ("Infinity", &["infinity", "forever", "love"], || {
            let line: Vec<Point> = (0..=400)
                .map(|k| {
                    let t = TAU * k as f64 / 400.0;
                    let d = 1.0 + t.sin().powi(2);
                    p(34.0 * t.cos() / d, 34.0 * t.sin() * t.cos() / d)
                })
                .collect();
            art::union(&art::stroke(&line, 6.0)?)
        }),
    ]
}

/// A gear with `teeth` teeth between radii `outer` and `root`, and a bore.
fn gear(teeth: usize, outer: f64, root: f64, bore: f64) -> Vec<Ring> {
    let ring = art::polar(teeth * 40, |t| {
        let k = t * teeth as f64 / TAU;
        let f = k - k.floor();
        // A tooth is a trapezoid: rise, top, fall, root.
        let s = if f < 0.15 {
            f / 0.15
        } else if f < 0.45 {
            1.0
        } else if f < 0.6 {
            1.0 - (f - 0.45) / 0.15
        } else {
            0.0
        };
        root + (outer - root) * s
    });
    vec![ring, art::circle(p(0.0, 0.0), bore, 96)]
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    for (category, list) in [
        ("Signs and Arrows/Arrows", arrows()),
        ("Signs and Arrows/Sign Shapes", boards()),
        ("Signs and Arrows/Symbols", symbols()),
    ] {
        let mut add = Adder {
            out: &mut *out,
            category,
            source: SOURCE,
        };
        for (name, tags, make) in list {
            add.vectors(name, tags, move || Ok(art::paths(&make()?)));
        }
    }
    let mut add = Adder {
        out,
        category: "Signs and Arrows/Sign Reliefs",
        source: SOURCE,
    };
    add.relief("Block Arrow Relief", &["arrow", "block", "relief"], || {
        art::shape(
            &[arrow(70.0, 16.0, 36.0, 24.0)],
            Profile::Angular,
            30.0,
            0.0,
        )
    });
    add.relief(
        "Rounded Arrow Relief",
        &["arrow", "rounded", "relief"],
        || art::puffy(&[arrow(70.0, 16.0, 36.0, 24.0)], 5.0),
    );
    add.relief(
        "Curved Arrow Relief",
        &["arrow", "curved", "relief"],
        || art::puffy(&curved(p(0.0, -20.0), 35.0, PI * 0.85, PI * 0.2, 7.0)?, 4.0),
    );
    add.relief("Chevron Relief", &["chevron", "relief"], || {
        art::shape(
            &[vec![
                p(-15.0, -25.0),
                p(0.0, -25.0),
                p(20.0, 0.0),
                p(0.0, 25.0),
                p(-15.0, 25.0),
                p(5.0, 0.0),
            ]],
            Profile::Angular,
            35.0,
            0.0,
        )
    });
    add.relief("Gear Relief", &["gear", "cog", "relief"], || {
        let ring = gear(12, 30.0, 24.0, 8.0);
        let body = art::by_depth(&ring, |d| 1.5 + 2.0 * art::round_cap((d / 1.5).min(1.0)))?;
        Ok(body)
    });
    add.relief("Crown Relief", &["crown", "royal", "relief"], || {
        let body = vec![
            p(-30.0, -18.0),
            p(30.0, -18.0),
            p(34.0, 16.0),
            p(17.0, 0.0),
            p(0.0, 22.0),
            p(-17.0, 0.0),
            p(-34.0, 16.0),
        ];
        let mut rings = vec![body];
        rings.extend([
            art::circle(p(-34.0, 18.0), 4.0, 48),
            art::circle(p(0.0, 25.0), 4.5, 48),
            art::circle(p(34.0, 18.0), 4.0, 48),
        ]);
        rings.push(art::rect(p(0.0, -22.0), 64.0, 7.0));
        art::puffy(&art::union(&rings)?, 5.0)
    });
    add.relief("Sun Relief", &["sun", "sunshine", "relief"], || {
        let mut rings = vec![art::circle(p(0.0, 0.0), 16.0, 160)];
        for k in 0..12 {
            let a = TAU * k as f64 / 12.0;
            rings.push(art::moved(
                &[p(14.0, -4.0), p(34.0, 0.0), p(14.0, 4.0)],
                &art::place(a, 0.0, 0.0),
            ));
        }
        art::shape(&art::union(&rings)?, Profile::Round, 90.0, 6.0)
    });
    add.relief("Water Drop Relief", &["drop", "water", "relief"], || {
        art::puffy(
            &[art::mirrored(
                &[
                    p(0.0, 35.0),
                    p(8.0, 18.0),
                    p(18.0, 0.0),
                    p(17.0, -12.0),
                    p(9.0, -20.0),
                    p(0.0, -22.0),
                ],
                10,
            )],
            6.0,
        )
    });
    add.relief("Flame Relief", &["flame", "fire", "relief"], || {
        art::puffy(
            &[art::smooth(
                &[
                    p(0.0, 40.0),
                    p(8.0, 22.0),
                    p(18.0, 10.0),
                    p(20.0, -8.0),
                    p(12.0, -20.0),
                    p(0.0, -24.0),
                    p(-12.0, -20.0),
                    p(-20.0, -6.0),
                    p(-14.0, 12.0),
                    p(-8.0, 6.0),
                    p(-6.0, 20.0),
                ],
                10,
            )],
            5.0,
        )
    });
    add.relief(
        "Crescent Moon Relief",
        &["moon", "crescent", "relief"],
        || {
            art::puffy(
                &art::cut(
                    &[art::circle(p(0.0, 0.0), 30.0, 240)],
                    &[art::circle(p(12.0, 8.0), 26.0, 240)],
                )?,
                5.0,
            )
        },
    );
    add.relief(
        "Lightning Bolt Relief",
        &["lightning", "bolt", "relief"],
        || {
            art::shape(
                &[vec![
                    p(6.0, 40.0),
                    p(-18.0, 0.0),
                    p(-2.0, 0.0),
                    p(-10.0, -40.0),
                    p(18.0, 6.0),
                    p(2.0, 6.0),
                    p(14.0, 40.0),
                ]],
                Profile::Angular,
                30.0,
                0.0,
            )
        },
    );
}
