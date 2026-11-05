//! Borders and corners: repeating strips (rope, beads, waves, keys,
//! scallops and more) as vectors and as reliefs, and corner pieces.
//!
//! A strip runs along X, centred on the origin, a whole number of repeats
//! long so copies placed end to end continue the pattern.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::model::field::Field;

use super::art::{self, p, round_cap, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "borders.rs";

/// Repeats in a strip.
const REPEATS: usize = 5;

/// `count` copies of `unit`, `pitch` apart along X, centred.
fn row(unit: &[Ring], pitch: f64, count: usize) -> Vec<Ring> {
    let start = -pitch * (count as f64 - 1.0) / 2.0;
    (0..count)
        .flat_map(|k| art::moved_all(unit, &art::place(0.0, start + pitch * k as f64, 0.0)))
        .collect()
}

/// A leaf shape `len` long and `wide` across, pointing along +X from the
/// origin.
pub(super) fn leaf_shape(len: f64, wide: f64) -> Ring {
    let n = 40;
    let mut ring = Vec::with_capacity(2 * n);
    for k in 0..=n {
        let t = k as f64 / n as f64;
        ring.push(p(len * t, wide / 2.0 * (PI * t).sin() * (1.0 - 0.35 * t)));
    }
    for k in (1..n).rev() {
        let t = k as f64 / n as f64;
        ring.push(p(len * t, -wide / 2.0 * (PI * t).sin() * (1.0 - 0.35 * t)));
    }
    ring
}

/// The Greek key (meander) center line for one repeat of width 20 and
/// height 16, starting at the lower left.
fn key_unit() -> Vec<(f64, f64)> {
    vec![
        (0.0, 0.0),
        (0.0, 16.0),
        (16.0, 16.0),
        (16.0, 4.0),
        (8.0, 4.0),
        (8.0, 10.0),
        (12.0, 10.0),
    ]
}

/// The meander as one line, `repeats` long, centred.
fn meander(repeats: usize) -> Vec<art::Ring> {
    // Each repeat is a spiral hook; a base line joins them.
    let pitch = 20.0;
    let mut rings = Vec::new();
    let start = -pitch * repeats as f64 / 2.0;
    for k in 0..repeats {
        let x0 = start + pitch * k as f64;
        let pts: Vec<_> = key_unit()
            .into_iter()
            .map(|(x, y)| p(x0 + x, y - 8.0))
            .collect();
        rings.push(pts);
    }
    rings
}

fn strips(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Borders and Corners/Border Strips",
        source: SOURCE,
    };
    add.vectors("Bead Border", &["border", "beads", "dots"], || {
        Ok(art::paths(&row(
            &[art::circle(p(0.0, 0.0), 4.0, 48)],
            10.0,
            REPEATS * 2,
        )))
    });
    add.vectors(
        "Bead and Reel Border",
        &["border", "bead and reel", "classical"],
        || {
            let unit = vec![
                art::ellipse(p(0.0, 0.0), 6.0, 3.5, 48),
                art::circle(p(9.0, 0.0), 2.0, 32),
            ];
            Ok(art::paths(&row(&unit, 18.0, REPEATS)))
        },
    );
    add.vectors("Scallop Border", &["border", "scallops", "arcs"], || {
        let pitch = 16.0;
        let count = REPEATS * 2;
        let x0 = -pitch * count as f64 / 2.0;
        let mut ring = vec![p(x0, -4.0)];
        for k in 0..count {
            for s in 0..=16 {
                let a = PI - PI * s as f64 / 16.0;
                ring.push(p(
                    x0 + pitch * k as f64 + pitch / 2.0 + pitch / 2.0 * a.cos(),
                    4.0 + 6.0 * a.sin(),
                ));
            }
        }
        ring.push(p(-x0, -4.0));
        Ok(art::paths(&[ring]))
    });
    add.vectors(
        "Dentil Border",
        &["border", "dentil", "blocks", "classical"],
        || {
            Ok(art::paths(&row(
                &[art::rect(p(0.0, 0.0), 8.0, 12.0)],
                12.0,
                REPEATS * 2,
            )))
        },
    );
    add.vectors("Diamond Border", &["border", "diamonds", "lozenge"], || {
        let unit = vec![vec![p(-7.0, 0.0), p(0.0, 6.0), p(7.0, 0.0), p(0.0, -6.0)]];
        Ok(art::paths(&row(&unit, 16.0, REPEATS * 2)))
    });
    add.vectors("Zigzag Border", &["border", "zigzag", "chevron"], || {
        let pitch = 16.0;
        let count = REPEATS * 2;
        let x0 = -pitch * count as f64 / 2.0;
        let mut line = Vec::new();
        for k in 0..=count * 2 {
            let y = if k % 2 == 0 { -5.0 } else { 5.0 };
            line.push(p(x0 + pitch / 2.0 * k as f64, y));
        }
        Ok(art::paths(&art::stroke(&line, 3.0)?))
    });
    add.vectors(
        "Wave Border",
        &["border", "wave", "running scroll", "vitruvian"],
        || Ok(art::paths(&wave_shape(2.0)?)),
    );
    add.vectors(
        "Greek Key Border",
        &["border", "greek key", "meander", "classical"],
        || {
            let mut rings = Vec::new();
            for line in meander(REPEATS) {
                rings.extend(art::stroke(&line, 2.5)?);
            }
            let base = vec![p(-50.0, -8.0), p(50.0, -8.0)];
            rings.extend(art::stroke(&base, 2.5)?);
            Ok(art::paths(&art::union(&rings)?))
        },
    );
    add.vectors(
        "Rope Border",
        &["border", "rope", "twist", "nautical"],
        || {
            let pitch = 10.0;
            let count = REPEATS * 2;
            let unit = vec![art::moved(
                &art::ellipse(p(0.0, 0.0), 8.0, 3.2, 48),
                &art::place(PI / 4.0, 0.0, 0.0),
            )];
            Ok(art::paths(&row(&unit, pitch, count)))
        },
    );
    add.vectors("Chain Border", &["border", "chain", "links"], || {
        let mut links = Vec::new();
        for k in 0..REPEATS {
            let x = -40.0 + 20.0 * k as f64;
            links.push(art::band(
                &art::rounded_rect(p(x, 0.0), 24.0, 12.0, 6.0),
                2.5,
            )?);
        }
        Ok(art::paths(&art::merge(&links)?))
    });
    add.vectors("Heart Border", &["border", "hearts", "love"], || {
        let heart = super::stars::heart(10.0);
        Ok(art::paths(&row(
            &[art::moved(&heart, &art::place(-PI / 2.0, 0.0, 0.0))],
            14.0,
            REPEATS * 2,
        )))
    });
    add.vectors("Star Border", &["border", "stars"], || {
        Ok(art::paths(&row(
            &[art::star(5, 6.0, 2.4, 0.0)],
            14.0,
            REPEATS * 2,
        )))
    });
    add.vectors(
        "Leaf Garland Border",
        &["border", "leaves", "garland", "vine"],
        || {
            let mut rings = Vec::new();
            let mut stem = Vec::new();
            for k in 0..=100 {
                let x = -50.0 + k as f64;
                stem.push(p(x, 2.0 * (x * TAU / 20.0).sin()));
            }
            rings.extend(art::stroke(&stem, 1.2)?);
            for k in 0..REPEATS {
                let x = -40.0 + 20.0 * k as f64;
                rings.push(art::moved(
                    &leaf_shape(10.0, 5.0),
                    &art::place(PI / 4.0, x, 2.0),
                ));
                rings.push(art::moved(
                    &leaf_shape(10.0, 5.0),
                    &art::place(PI / 4.0 + PI, x + 10.0, -2.0),
                ));
            }
            Ok(art::paths(&art::union(&rings)?))
        },
    );
    add.vectors(
        "Egg and Dart Border",
        &["border", "egg and dart", "classical"],
        || {
            let mut rings = Vec::new();
            for k in 0..REPEATS {
                let x = -40.0 + 20.0 * k as f64;
                rings.push(art::ellipse(p(x, 1.0), 6.0, 8.0, 48));
                rings.push(art::ellipse(p(x, 1.0), 3.8, 5.8, 48));
                rings.push(vec![
                    p(x + 10.0, 9.0),
                    p(x + 11.5, 0.0),
                    p(x + 10.0, -9.0),
                    p(x + 8.5, 0.0),
                ]);
            }
            Ok(art::paths(&rings))
        },
    );
    add.vectors("Double Line Border", &["border", "lines", "dots"], || {
        let mut rings = vec![
            art::rect(p(0.0, 6.0), 100.0, 1.5),
            art::rect(p(0.0, -6.0), 100.0, 1.5),
        ];
        rings.extend(row(&[art::circle(p(0.0, 0.0), 1.8, 24)], 8.0, 12));
        Ok(art::paths(&rings))
    });
    add.vectors(
        "Braid Border",
        &["border", "braid", "plait", "celtic"],
        || {
            let mut rings = Vec::new();
            for phase in [0.0, PI] {
                let line: Vec<_> = (0..=200)
                    .map(|k| {
                        let x = -50.0 + k as f64 * 0.5;
                        p(x, 5.0 * (x * TAU / 20.0 + phase).sin())
                    })
                    .collect();
                rings.extend(art::stroke(&line, 3.0)?);
            }
            Ok(art::paths(&art::union(&rings)?))
        },
    );
}

/// The running scroll as one line per wave: a run along the base that
/// rises into a curl. Neighbouring waves meet on the base line.
fn wave_lines() -> Vec<Vec<crate::geom::Point>> {
    let pitch = 20.0;
    let start = -pitch * REPEATS as f64 / 2.0;
    (0..REPEATS)
        .map(|k| {
            let x0 = start + pitch * k as f64;
            let mut line = vec![p(x0 - pitch / 2.0, -7.0)];
            for s in 0..=60 {
                let t = s as f64 / 60.0;
                let a = -PI / 2.0 + TAU * 1.2 * t;
                let r = 7.0 * (1.0 - 0.62 * t);
                line.push(p(x0 + pitch / 2.0 + r * a.cos(), r * a.sin()));
            }
            line
        })
        .collect()
}

/// The running scroll's outline, `width` wide.
fn wave_shape(width: f64) -> Result<Vec<Ring>, CoreError> {
    let mut rings = Vec::new();
    for line in wave_lines() {
        rings.extend(art::stroke(&line, width)?);
    }
    art::union(&rings)
}

/// A rope: two strands twisting along X, `len` long and `wide` across.
pub(super) fn rope(len: f64, wide: f64, pitch: f64) -> Field {
    let r = wide / 2.0;
    art::field_fn(len, wide + 0.5, move |x, y| {
        if y.abs() > r {
            return f64::NAN;
        }
        let env = (r * r - y * y).sqrt();
        let phase = ((x + y) / pitch).rem_euclid(1.0);
        let strand = (PI * phase).sin().powf(0.7);
        env * (0.65 + 0.35 * strand)
    })
}

fn relief_strips(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Borders and Corners/Border Reliefs",
        source: SOURCE,
    };
    add.relief("Rope Relief", &["border", "rope", "twist"], || {
        Ok(rope(80.0, 12.0, 8.0))
    });
    add.relief("Fine Rope Relief", &["border", "rope", "twist"], || {
        Ok(rope(80.0, 7.0, 5.0))
    });
    add.relief("Bead Relief", &["border", "beads"], || {
        Ok(art::field_fn(80.0, 10.0, |x, y| {
            let k = (x / 10.0).round() * 10.0;
            let d2 = (x - k).powi(2) + y * y;
            if d2 > 4.6 * 4.6 || k.abs() > 35.0 {
                f64::NAN
            } else {
                (4.6 * 4.6 - d2).sqrt()
            }
        }))
    });
    add.relief(
        "Bead and Reel Relief",
        &["border", "bead and reel", "classical"],
        || {
            Ok(art::field_fn(80.0, 10.0, |x, y| {
                let k = (x / 18.0).round() * 18.0;
                let u = x - k;
                if k.abs() > 35.0 {
                    return f64::NAN;
                }
                let bead = {
                    let d = (u / 6.0).hypot(y / 3.8);
                    if d < 1.0 {
                        3.8 * (1.0 - d * d).sqrt()
                    } else {
                        f64::NAN
                    }
                };
                let reel = {
                    let v = (u.abs() - 9.0) / 2.2;
                    let d = v.hypot(y / 2.4);
                    if d < 1.0 {
                        2.4 * (1.0 - d * d).sqrt()
                    } else {
                        f64::NAN
                    }
                };
                match (bead.is_nan(), reel.is_nan()) {
                    (true, true) => f64::NAN,
                    (false, true) => bead,
                    (true, false) => reel,
                    (false, false) => bead.max(reel),
                }
            }))
        },
    );
    add.relief(
        "Egg and Dart Relief",
        &["border", "egg and dart", "classical"],
        || {
            Ok(art::field_fn(80.0, 22.0, |x, y| {
                let k = (x / 20.0).round() * 20.0;
                let u = x - k;
                if x.abs() > 39.5 {
                    return f64::NAN;
                }
                let rim = if y.abs() > 9.5 && y.abs() < 10.8 {
                    1.2
                } else {
                    f64::NAN
                };
                let egg = {
                    let d = (u / 6.5).hypot((y - 0.5) / 8.5);
                    if d < 1.0 {
                        let shell = if d > 0.78 { 1.6 } else { 0.8 };
                        shell + 2.4 * (1.0 - d * d).sqrt() * if d < 0.72 { 1.0 } else { 0.4 }
                    } else {
                        f64::NAN
                    }
                };
                let dart = {
                    let v = (u.abs() - 10.0).abs();
                    let reach = 8.5 * (1.0 - v / 1.8);
                    if v < 1.8 && y.abs() < reach {
                        2.2 * (1.0 - y.abs() / 9.0)
                    } else {
                        f64::NAN
                    }
                };
                [rim, egg, dart]
                    .into_iter()
                    .filter(|v| !v.is_nan())
                    .fold(f64::NAN, |a, b| if a.is_nan() { b } else { a.max(b) })
            }))
        },
    );
    add.relief("Dentil Relief", &["border", "dentil", "blocks"], || {
        Ok(art::field_fn(80.0, 14.0, |x, y| {
            let u = x - (x / 12.0).round() * 12.0;
            if u.abs() > 4.0 || y.abs() > 6.5 || x.abs() > 38.0 {
                return f64::NAN;
            }
            let edge = (4.0 - u.abs()).min(6.5 - y.abs());
            1.0 + 2.5 * round_cap((edge / 0.8).min(1.0))
        }))
    });
    add.relief("Scallop Relief", &["border", "scallops"], || {
        Ok(art::field_fn(80.0, 14.0, |x, y| {
            let k = (x / 12.0).round() * 12.0;
            let d = (x - k).hypot(y + 6.0);
            if d > 12.0 || y < -6.0 || x.abs() > 40.0 {
                return f64::NAN;
            }
            let t = d / 12.0;
            3.0 * round_cap((1.0 - t) / 0.3) * (0.7 + 0.3 * t)
        }))
    });
    add.relief("Twist Relief", &["border", "twist", "barley"], || {
        Ok(art::field_fn(80.0, 10.0, |x, y| {
            if y.abs() > 4.8 {
                return f64::NAN;
            }
            let env = (4.8 * 4.8 - y * y).sqrt();
            let groove = 0.5 + 0.5 * ((x * 0.8 + y * 0.8) * TAU / 8.0).cos();
            env * (0.8 + 0.2 * groove)
        }))
    });
    add.relief("Wave Relief", &["border", "wave", "running scroll"], || {
        let lines: Vec<Vec<(crate::geom::Point, f64)>> = wave_lines()
            .into_iter()
            .map(|l| l.into_iter().map(|q| (q, 0.0)).collect())
            .collect();
        art::tubes(&lines, 1.6)
    });
    add.relief("Braid Relief", &["border", "braid", "plait"], || {
        // Two strands, each rising where it crosses over the other.
        let mut lines = Vec::new();
        for phase in [0.0, PI] {
            let line: Vec<_> = (0..=320)
                .map(|k| {
                    let x = -40.0 + k as f64 * 0.25;
                    let a = x * TAU / 20.0 + phase;
                    (p(x, 5.0 * a.sin()), 1.2 * a.cos())
                })
                .collect();
            lines.push(line);
        }
        art::tubes(&lines, 2.0).map(|f| art::raised(f, 1.2))
    });
    add.relief(
        "Greek Key Relief",
        &["border", "greek key", "meander"],
        || {
            let mut lines: Vec<Vec<(crate::geom::Point, f64)>> = meander(4)
                .into_iter()
                .map(|l| l.into_iter().map(|q| (q, 0.0)).collect())
                .collect();
            lines.push(vec![(p(-40.0, -8.0), 0.0), (p(40.0, -8.0), 0.0)]);
            art::tubes(&lines, 1.4)
        },
    );
    add.relief(
        "Leaf Garland Relief",
        &["border", "leaves", "garland"],
        || {
            let mut field: Option<Field> = None;
            let stem: Vec<_> = (0..=320)
                .map(|k| {
                    let x = -40.0 + k as f64 * 0.25;
                    (p(x, 2.0 * (x * TAU / 20.0).sin()), 0.0)
                })
                .collect();
            let mut add_to = |f: Field| {
                field = Some(match field.take() {
                    Some(g) => art::highest(&g, &f),
                    None => f,
                });
            };
            add_to(art::tubes(&[stem], 0.8)?);
            for k in 0..4 {
                let x = -30.0 + 20.0 * k as f64;
                for (turn, at) in [(PI / 4.0, p(x, 2.0)), (PI / 4.0 + PI, p(x + 10.0, -2.0))] {
                    let m = art::place(turn, at.x, at.y);
                    let leaf = art::moved(&leaf_shape(10.0, 5.0), &m);
                    add_to(super::floral::veined(
                        &leaf,
                        at,
                        m.apply(p(10.0, 0.0)),
                        2.0,
                    )?);
                }
            }
            field.ok_or_else(|| CoreError::validation("the garland is empty"))
        },
    );
}

fn corners(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Borders and Corners/Corners",
        source: SOURCE,
    };
    add.vectors(
        "Quarter Circle Corner",
        &["corner", "quarter circle"],
        || {
            let mut ring = vec![p(0.0, 0.0), p(40.0, 0.0)];
            ring.extend((1..24).map(|k| {
                let a = PI / 2.0 * k as f64 / 24.0;
                p(40.0 * a.cos(), 40.0 * a.sin())
            }));
            ring.push(p(0.0, 40.0));
            Ok(art::paths(&[ring]))
        },
    );
    add.vectors("Fan Corner", &["corner", "fan", "sunburst"], || {
        let mut rings = Vec::new();
        for k in 0..6 {
            let a0 = PI / 2.0 * k as f64 / 6.0 + 0.03;
            let a1 = PI / 2.0 * (k + 1) as f64 / 6.0 - 0.03;
            let mut blade = vec![p(4.0 * (a0 + a1).cos() / 2.0, 4.0 * (a0 + a1).sin() / 2.0)];
            for s in 0..=8 {
                let a = a0 + (a1 - a0) * s as f64 / 8.0;
                blade.push(p(40.0 * a.cos(), 40.0 * a.sin()));
            }
            rings.push(blade);
        }
        rings.push(art::circle(p(0.0, 0.0), 6.0, 48));
        Ok(art::paths(&art::union(&rings)?))
    });
    add.vectors("Notched Corner", &["corner", "notch", "bracket"], || {
        let square = art::rect(p(20.0, 20.0), 40.0, 40.0);
        Ok(art::paths(&art::cut(
            &[square],
            &[art::circle(p(40.0, 40.0), 32.0, 180)],
        )?))
    });
    add.vectors("Scroll Corner", &["corner", "scroll", "curl"], || {
        let mut rings = Vec::new();
        for flip in [false, true] {
            let spiral: Vec<_> = (0..=120)
                .map(|k| {
                    let t = k as f64 / 120.0;
                    let a = -PI / 2.0 + TAU * 1.25 * t;
                    let r = 10.0 * (1.0 - 0.75 * t);
                    let (x, y) = (32.0 + r * a.cos(), 14.0 + r * a.sin());
                    if flip {
                        p(y, x)
                    } else {
                        p(x, y)
                    }
                })
                .collect();
            let arm: Vec<_> = if flip {
                vec![p(4.0, 0.0), p(4.0, 32.0)]
            } else {
                vec![p(0.0, 4.0), p(32.0, 4.0)]
            };
            rings.extend(art::stroke(&arm, 3.0)?);
            rings.extend(art::stroke(&spiral, 3.0)?);
        }
        rings.push(art::circle(p(4.0, 4.0), 4.0, 32));
        Ok(art::paths(&art::union(&rings)?))
    });
    add.vectors("Leaf Corner", &["corner", "leaves", "floral"], || {
        let mut rings = vec![art::circle(p(0.0, 0.0), 4.0, 32)];
        for (k, len) in [(0, 30.0), (1, 22.0), (2, 30.0)] {
            let a = PI / 4.0 * k as f64;
            rings.push(art::moved(
                &leaf_shape(len, len * 0.4),
                &art::place(a, 2.0 * a.cos(), 2.0 * a.sin()),
            ));
        }
        Ok(art::paths(&art::union(&rings)?))
    });
    add.vectors(
        "Greek Key Corner",
        &["corner", "greek key", "meander"],
        || {
            let line = vec![
                p(0.0, 40.0),
                p(0.0, 0.0),
                p(40.0, 0.0),
                p(40.0, 8.0),
                p(8.0, 8.0),
                p(8.0, 30.0),
                p(24.0, 30.0),
                p(24.0, 16.0),
                p(16.0, 16.0),
                p(16.0, 22.0),
            ];
            Ok(art::paths(&art::stroke(&line, 2.5)?))
        },
    );
    add.vectors("Rope Corner", &["corner", "rope", "nautical"], || {
        let mut rings = Vec::new();
        let path: Vec<_> = (0..=60)
            .map(|k| {
                let a = PI / 2.0 * k as f64 / 60.0;
                p(40.0 - 30.0 * a.sin(), 40.0 - 30.0 * a.cos())
            })
            .collect();
        let resampled = art::smooth_open(&path, 1);
        for w in resampled.windows(2).step_by(6) {
            let mid = w[0].midpoint(w[1]);
            let heading = (w[1].y - w[0].y).atan2(w[1].x - w[0].x);
            rings.push(art::moved(
                &art::ellipse(p(0.0, 0.0), 3.4, 1.4, 24),
                &art::place(heading + PI / 4.0, mid.x, mid.y),
            ));
        }
        Ok(art::paths(&rings))
    });
    add.vectors("Heart Corner", &["corner", "heart", "love"], || {
        let heart = super::stars::heart(14.0);
        let mut rings = vec![art::moved(&heart, &art::place(-PI / 4.0, 10.0, 10.0))];
        for k in 1..4 {
            let d = 10.0 + 10.0 * k as f64;
            rings.push(art::circle(p(d, 3.0), 1.6, 24));
            rings.push(art::circle(p(3.0, d), 1.6, 24));
        }
        Ok(art::paths(&rings))
    });
    add.relief("Fan Corner Relief", &["corner", "fan", "sunburst"], || {
        Ok(art::field_fn(84.0, 84.0, |x, y| {
            let (u, v) = (x + 41.0, y + 41.0);
            let r = u.hypot(v);
            let a = v.atan2(u);
            if !(0.0..=PI / 2.0).contains(&a) || r > 40.0 {
                return f64::NAN;
            }
            let blade = ((a / (PI / 2.0) * 6.0).fract() - 0.5).abs() * 2.0;
            let rise = 3.0 * (1.0 - blade.powi(2)).sqrt() * round_cap((40.0 - r) / 6.0);
            if r < 6.0 {
                2.0 + 2.0 * (1.0 - (r / 6.0).powi(2)).sqrt()
            } else {
                0.8 + rise
            }
        }))
    });
    add.relief("Quarter Rosette Corner", &["corner", "rosette"], || {
        let rosette = super::rosettes::Rosette {
            petals: 12,
            layers: 2,
            style: super::rosettes::Petal::Round,
            radius: 40.0,
            cup: 0.4,
        };
        Ok(art::field_fn(84.0, 84.0, move |x, y| {
            let (u, v) = (x + 41.0, y + 41.0);
            if u < 0.0 || v < 0.0 {
                return f64::NAN;
            }
            rosette.height(u, v)
        }))
    });
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    strips(out);
    relief_strips(out);
    corners(out);
}
