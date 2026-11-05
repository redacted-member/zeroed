//! Geometric shapes and Celtic knots. Knots are closed curves carrying a
//! height at every point: as vectors each strand breaks where it passes
//! under another, as reliefs they are round tubes that rise over and dip
//! under at the crossings.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::Point;
use crate::model::field::Field;

use super::art::{self, p, round_cap, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "geometric.rs";

/// One closed strand of a knot: points with the height of the strand.
type Strand = Vec<(Point, f64)>;

/// A closed curve sampled every `step` millimetres or so, from a function
/// of `t` in 0..1 answering the point and its height.
fn sample(f: impl Fn(f64) -> (Point, f64), step: f64) -> Strand {
    // Measure the length first so samples come out evenly spaced.
    let rough = 2000;
    let mut length = 0.0;
    let mut last = f(0.0).0;
    for k in 1..=rough {
        let q = f(k as f64 / rough as f64).0;
        length += last.distance(q);
        last = q;
    }
    let n = ((length / step).ceil() as usize).max(24);
    (0..n).map(|k| f(k as f64 / n as f64)).collect()
}

/// A torus knot wound `p` times round and `q` times through, `size` across.
fn torus_knot(pw: usize, qw: usize, size: f64) -> Strand {
    let (pw, qw) = (pw as f64, qw as f64);
    let k = size / 6.0;
    sample(
        |t| {
            let a = TAU * t;
            let r = 2.0 + (qw * a).cos();
            (
                p(k * r * (pw * a).cos(), k * r * (pw * a).sin()),
                -(qw * a).sin(),
            )
        },
        0.25,
    )
}

/// Every knot: name, tags and strands.
fn knots() -> Vec<art::Design<Vec<Strand>>> {
    vec![
        ("Trefoil Knot", &["knot", "celtic", "trefoil"], || {
            vec![torus_knot(2, 3, 60.0)]
        }),
        ("Cinquefoil Knot", &["knot", "celtic", "five"], || {
            vec![torus_knot(2, 5, 60.0)]
        }),
        ("Septafoil Knot", &["knot", "celtic", "seven"], || {
            vec![torus_knot(2, 7, 60.0)]
        }),
        ("Star Knot", &["knot", "celtic", "star"], || {
            vec![torus_knot(3, 5, 60.0)]
        }),
        (
            "Figure Eight Knot",
            &["knot", "celtic", "figure eight"],
            || {
                vec![sample(
                    |t| {
                        let a = TAU * t;
                        let r = 2.0 + (2.0 * a).cos();
                        (
                            p(10.0 * r * (3.0 * a).cos(), 10.0 * r * (3.0 * a).sin()),
                            (4.0 * a).sin(),
                        )
                    },
                    0.25,
                )]
            },
        ),
        (
            "Triquetra",
            &["knot", "celtic", "triquetra", "trinity"],
            || {
                let lobes = sample(
                    |t| {
                        let a = TAU * t;
                        let x = 11.0 * (a.sin() + 2.0 * (2.0 * a).sin());
                        let y = 11.0 * (a.cos() - 2.0 * (2.0 * a).cos());
                        (p(x, y), -(3.0 * a).sin())
                    },
                    0.25,
                );
                let ring = sample(
                    |t| {
                        let a = TAU * t;
                        (p(19.0 * a.cos(), 19.0 * a.sin()), (6.0 * a + 0.9).sin())
                    },
                    0.25,
                );
                vec![lobes, ring]
            },
        ),
        (
            "Two Rings",
            &["rings", "linked", "wedding", "celtic"],
            || {
                let ring = |cx: f64, sign: f64| {
                    sample(
                        move |t| {
                            let a = TAU * t;
                            (p(cx + 16.0 * a.cos(), 16.0 * a.sin()), sign * a.sin())
                        },
                        0.25,
                    )
                };
                vec![ring(-9.0, 1.0), ring(9.0, -1.0)]
            },
        ),
        (
            "Three Rings",
            &["rings", "borromean", "linked", "celtic"],
            || {
                (0..3)
                    .map(|k| {
                        let at = PI / 2.0 + TAU * k as f64 / 3.0;
                        let (cx, cy) = (10.0 * at.cos(), 10.0 * at.sin());
                        sample(
                            move |t| {
                                let a = TAU * t;
                                (p(cx + 16.0 * a.cos(), cy + 16.0 * a.sin()), (a - at).sin())
                            },
                            0.25,
                        )
                    })
                    .collect()
            },
        ),
        ("Solomon Knot", &["knot", "solomon", "celtic"], || {
            let loop_of = |vertical: bool, sign: f64| {
                sample(
                    move |t| {
                        let a = TAU * t;
                        let (c, s) = (a.cos(), a.sin());
                        let (x, y) = (
                            28.0 * c.signum() * c.abs().powf(0.6),
                            11.0 * s.signum() * s.abs().powf(0.6),
                        );
                        let pt = if vertical { p(y, x) } else { p(x, y) };
                        (pt, sign * (2.0 * a).sin())
                    },
                    0.25,
                )
            };
            vec![loop_of(false, 1.0), loop_of(true, -1.0)]
        }),
        ("Celtic Ring", &["knot", "celtic", "ring", "circle"], || {
            (0..2)
                .map(|k| {
                    let sign = if k == 0 { 1.0 } else { -1.0 };
                    sample(
                        move |t| {
                            let a = TAU * t;
                            let r = 25.0 + sign * 5.0 * (8.0 * a).sin();
                            (p(r * a.cos(), r * a.sin()), sign * (8.0 * a).cos())
                        },
                        0.25,
                    )
                })
                .collect()
        }),
        (
            "Interlaced Pentagram",
            &["pentagram", "star", "interlaced"],
            || {
                let corners: Vec<Point> = (0..5)
                    .map(|k| {
                        let a = PI / 2.0 + TAU * (2 * k) as f64 / 5.0;
                        p(30.0 * a.cos(), 30.0 * a.sin())
                    })
                    .collect();
                vec![polygon_strand(&corners, 0.382, 0.618)]
            },
        ),
        (
            "Interlaced Hexagram",
            &["hexagram", "star", "interlaced"],
            || {
                let tri = |turn: f64| -> Vec<Point> {
                    (0..3)
                        .map(|k| {
                            let a = PI / 2.0 + turn + TAU * k as f64 / 3.0;
                            p(30.0 * a.cos(), 30.0 * a.sin())
                        })
                        .collect()
                };
                vec![
                    polygon_strand(&tri(0.0), 1.0 / 3.0, 2.0 / 3.0),
                    polygon_strand(&tri(PI), 1.0 / 3.0, 2.0 / 3.0),
                ]
            },
        ),
    ]
}

/// A closed polygon strand whose height is up at fraction `up` of every side
/// and down at fraction `down`, so its crossings alternate.
fn polygon_strand(corners: &[Point], up: f64, down: f64) -> Strand {
    let n = corners.len();
    let mut out = Vec::new();
    for k in 0..n {
        let (a, b) = (corners[k], corners[(k + 1) % n]);
        let steps = (a.distance(b) / 0.25).ceil() as usize;
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            let z = (PI * (t - up) / (down - up)).cos();
            out.push((a.lerp(b, t), z));
        }
    }
    out
}

/// A knot drawn as bands `width` wide, each strand broken where it passes
/// under another so the crossings read as over and under.
fn knot_outline(strands: &[Strand], width: f64) -> Result<Vec<Ring>, CoreError> {
    let reach = width * 1.2;
    let all: Vec<(usize, usize, Point, f64)> = strands
        .iter()
        .enumerate()
        .flat_map(|(s, strand)| {
            strand
                .iter()
                .enumerate()
                .map(move |(i, (q, z))| (s, i, *q, *z))
        })
        .collect();
    let mut rings = Vec::new();
    for (s, strand) in strands.iter().enumerate() {
        let n = strand.len();
        let near = (reach * 3.0 / 0.25) as usize;
        let under: Vec<bool> = strand
            .iter()
            .enumerate()
            .map(|(i, (q, z))| {
                all.iter().any(|(s2, j, q2, z2)| {
                    if *s2 == s {
                        let gap = i.abs_diff(*j).min(n - i.abs_diff(*j));
                        if gap < near {
                            return false;
                        }
                    }
                    z2 > z && q.distance(*q2) < reach
                })
            })
            .collect();
        match under.iter().position(|u| *u) {
            None => {
                let ring: Ring = strand.iter().map(|(q, _)| *q).collect();
                rings.extend(art::band(&ring, width)?);
            }
            Some(first) => {
                let mut run: Vec<Point> = Vec::new();
                for k in 0..=n {
                    let i = (first + k) % n;
                    if under[i] || k == n {
                        if run.len() >= 2 {
                            rings.extend(art::stroke(&run, width)?);
                        }
                        run.clear();
                    } else {
                        run.push(strand[i].0);
                    }
                }
            }
        }
    }
    art::union(&rings)
}

/// A knot as round tubes whose axis rises and falls with the strands'
/// heights, standing on nothing: the higher tube wins at a crossing.
fn knot_relief(strands: &[Strand], radius: f64) -> Result<Field, CoreError> {
    let lines: Vec<Vec<(Point, f64)>> = strands
        .iter()
        .map(|s| {
            let mut line: Vec<(Point, f64)> = s
                .iter()
                .map(|(q, z)| (*q, radius * 0.2 + radius * 0.9 * (z + 1.0) / 2.0))
                .collect();
            line.push(line[0]);
            line
        })
        .collect();
    art::tubes(&lines, radius)
}

/// Rings for a mandala of `petals` petals: an outer circle, a ring of petal
/// shapes, a band of dots, an inner rosette and a centre, all apart.
fn mandala(petals: usize, pointed: bool, dots: usize) -> Vec<Ring> {
    let mut rings = vec![art::circle(p(0.0, 0.0), 40.0, 360)];
    let len = 13.0;
    for k in 0..petals {
        let a = TAU * k as f64 / petals as f64;
        let w = (TAU * 26.0 / petals as f64 * 0.42).min(8.0);
        let shape = super::floral::leaf_outline(len, move |u| {
            let body = (PI * u).sin();
            w / 2.0
                * if pointed {
                    body.powf(0.8)
                } else {
                    body.powf(0.4)
                }
        });
        rings.push(art::moved(
            &shape,
            &art::place(a - PI / 2.0, 24.0 * a.cos(), 24.0 * a.sin()),
        ));
    }
    rings.push(art::circle(p(0.0, 0.0), 22.0, 240));
    for k in 0..dots {
        let a = TAU * (k as f64 + 0.5) / dots as f64;
        rings.push(art::circle(p(18.5 * a.cos(), 18.5 * a.sin()), 1.3, 24));
    }
    let inner = super::rosettes::Rosette {
        petals: petals / 2,
        layers: 1,
        style: if pointed {
            super::rosettes::Petal::Pointed
        } else {
            super::rosettes::Petal::Round
        },
        radius: 15.0,
        cup: 0.3,
    };
    rings.extend(inner.outlines());
    rings
}

fn outlines(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Geometric and Celtic Knots/Celtic Knots",
        source: SOURCE,
    };
    for (name, tags, make) in knots() {
        add.vectors(name, tags, move || {
            Ok(art::paths(&knot_outline(&make(), 4.0)?))
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Geometric and Celtic Knots/Knot Reliefs",
        source: SOURCE,
    };
    for (name, tags, make) in knots() {
        let mut all = vec!["relief"];
        all.extend_from_slice(tags);
        add.relief(&format!("{name} Relief"), &all, move || {
            knot_relief(&make(), 2.2)
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Geometric and Celtic Knots/Polygons",
        source: SOURCE,
    };
    for (sides, name) in [
        (3, "Triangle"),
        (4, "Square"),
        (5, "Pentagon"),
        (6, "Hexagon"),
        (7, "Heptagon"),
        (8, "Octagon"),
        (9, "Nonagon"),
        (10, "Decagon"),
        (12, "Dodecagon"),
    ] {
        add.vectors(name, &["polygon", "geometric"], move || {
            let turn = if sides % 2 == 0 {
                PI / sides as f64
            } else {
                0.0
            };
            Ok(art::paths(&[art::polygon(sides, 30.0, turn)]))
        });
    }
    add.vectors("Square Frame", &["square", "frame", "geometric"], || {
        Ok(art::paths(&art::band(
            &art::rect(p(0.0, 0.0), 50.0, 50.0),
            5.0,
        )?))
    });
    add.vectors("Circle Ring", &["circle", "ring", "washer"], || {
        Ok(art::paths(&[
            art::circle(p(0.0, 0.0), 30.0, 360),
            art::circle(p(0.0, 0.0), 22.0, 300),
        ]))
    });
    let mut add = Adder {
        out: add.out,
        category: "Geometric and Celtic Knots/Mandalas and Spirals",
        source: SOURCE,
    };
    for (name, petals, pointed, dots) in [
        ("Lotus Mandala", 12, true, 24),
        ("Daisy Mandala", 16, false, 32),
        ("Star Mandala", 8, true, 16),
        ("Sun Mandala", 20, true, 40),
    ] {
        add.vectors(name, &["mandala", "circle", "pattern"], move || {
            Ok(art::paths(&mandala(petals, pointed, dots)))
        });
    }
    add.vectors("Spiral", &["spiral", "archimedean"], || {
        let line: Vec<Point> = (0..=900)
            .map(|k| {
                let t = k as f64 / 900.0;
                let a = TAU * 4.0 * t;
                let r = 3.0 + 27.0 * t;
                p(r * a.cos(), r * a.sin())
            })
            .collect();
        Ok(art::paths(&art::stroke(&line, 2.5)?))
    });
    add.vectors("Triskele", &["triskele", "spiral", "celtic"], || {
        let mut rings = Vec::new();
        for k in 0..3 {
            let turn = TAU * k as f64 / 3.0;
            let arm: Vec<Point> = (0..=240)
                .map(|s| {
                    let t = s as f64 / 240.0;
                    let a = 1.8 * TAU * t;
                    let r = 12.0 * (1.0 - t) + 1.5;
                    let c = p(14.0 * turn.cos(), 14.0 * turn.sin());
                    p(
                        c.x + r * (a + turn + PI).cos(),
                        c.y + r * (a + turn + PI).sin(),
                    )
                })
                .collect();
            let mut line = vec![p(0.0, 0.0)];
            line.extend(arm);
            rings.push(art::tapered(&line, 4.0, 1.4));
        }
        art::union(&rings).map(|r| art::paths(&r))
    });
    let mut add = Adder {
        out: add.out,
        category: "Geometric and Celtic Knots/Patterns",
        source: SOURCE,
    };
    add.vectors("Honeycomb", &["pattern", "hexagon", "honeycomb"], || {
        let mut rings = Vec::new();
        for row in 0..5 {
            for col in 0..6 {
                let x = col as f64 * 13.0 + if row % 2 == 1 { 6.5 } else { 0.0 };
                let y = row as f64 * 11.26;
                rings.push(art::moved(
                    &art::polygon(6, 7.0, 0.0),
                    &art::place(0.0, x, y),
                ));
            }
        }
        Ok(art::paths(&art::fit(&rings, 80.0)))
    });
    add.vectors(
        "Diamond Lattice",
        &["pattern", "diamond", "lattice"],
        || {
            let mut rings = Vec::new();
            for row in 0..6 {
                for col in 0..6 {
                    let (x, y) = (
                        col as f64 * 14.0 + (row % 2) as f64 * 7.0,
                        row as f64 * 10.0,
                    );
                    rings.push(vec![
                        p(x - 6.0, y),
                        p(x, y + 4.5),
                        p(x + 6.0, y),
                        p(x, y - 4.5),
                    ]);
                }
            }
            Ok(art::paths(&rings))
        },
    );
    add.vectors("Basket Weave", &["pattern", "basket", "weave"], || {
        let mut rings = Vec::new();
        for row in 0..4 {
            for col in 0..4 {
                let (x, y) = (col as f64 * 20.0, row as f64 * 20.0);
                for k in 0..3 {
                    let off = -6.0 + 6.0 * k as f64;
                    let r = if (row + col) % 2 == 0 {
                        art::rect(p(x, y + off), 18.0, 4.8)
                    } else {
                        art::rect(p(x + off, y), 4.8, 18.0)
                    };
                    rings.push(r);
                }
            }
        }
        Ok(art::paths(&rings))
    });
    add.vectors("Brick Pattern", &["pattern", "brick", "wall"], || {
        let mut rings = Vec::new();
        for row in 0..6 {
            for col in 0..4 {
                let x = col as f64 * 22.0 + if row % 2 == 1 { 11.0 } else { 0.0 };
                rings.push(art::rect(p(x, row as f64 * 11.0), 20.0, 9.0));
            }
        }
        Ok(art::paths(&rings))
    });
    add.vectors("Fish Scale Pattern", &["pattern", "scales", "fish"], || {
        // Each scale shows below the row above it, with a small gap so the
        // loops never share an edge.
        let centre = |row: i64, col: i64| {
            p(
                col as f64 * 16.0 + if row % 2 == 1 { 8.0 } else { 0.0 },
                row as f64 * 10.0,
            )
        };
        let mut rings = Vec::new();
        for row in 0..4 {
            for col in 0..5 {
                let c = centre(row, col);
                let above: Vec<Ring> = [(row + 1, col - 1), (row + 1, col), (row + 1, col + 1)]
                    .iter()
                    .map(|&(r, k)| art::circle(centre(r, k), 9.0, 96))
                    .collect();
                rings.extend(art::cut(&[art::circle(c, 7.6, 96)], &above)?);
            }
        }
        Ok(art::paths(&rings))
    });
    add.vectors("Chevron Pattern", &["pattern", "chevron", "zigzag"], || {
        let mut rings = Vec::new();
        for row in 0..5 {
            let y = row as f64 * 10.0;
            let line: Vec<Point> = (0..=8)
                .map(|k| p(k as f64 * 10.0, y + if k % 2 == 0 { 0.0 } else { 5.0 }))
                .collect();
            rings.extend(art::stroke(&line, 3.0)?);
        }
        Ok(art::paths(&rings))
    });
    add.vectors("Circle Grid", &["pattern", "dots", "circles"], || {
        let mut rings = Vec::new();
        for row in 0..7 {
            for col in 0..7 {
                rings.push(art::circle(
                    p(col as f64 * 10.0, row as f64 * 10.0),
                    3.5,
                    48,
                ));
            }
        }
        Ok(art::paths(&rings))
    });
}

fn tiles(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Geometric and Celtic Knots/Tiles",
        source: SOURCE,
    };
    let edge = |x: f64, y: f64| (25.0 - x.abs()).min(25.0 - y.abs());
    add.relief(
        "Faceted Square Tile",
        &["tile", "pyramid", "faceted"],
        move || {
            Ok(art::field_fn(50.0, 50.0, move |x, y| {
                let d = edge(x, y);
                if d < 0.0 {
                    return f64::NAN;
                }
                if d < 3.0 {
                    1.0 + 1.5 * round_cap(d.min(3.0 - d).max(0.0) / 1.5)
                } else {
                    1.0 + 0.4 * (d - 3.0)
                }
            }))
        },
    );
    add.relief("Hex Dome Tile", &["tile", "hexagon", "dome"], || {
        Ok(art::field_fn(50.0, 50.0, |x, y| {
            let hex = |x: f64, y: f64| {
                let (ax, ay) = (x.abs(), y.abs());
                (ax * 0.866 + ay * 0.5).max(ay)
            };
            let d = hex(x, y);
            if d > 23.0 {
                return f64::NAN;
            }
            1.0 + 5.0 * (1.0 - (d / 23.0).powi(2)).max(0.0).sqrt()
        }))
    });
    add.relief(
        "Eight Point Star Tile",
        &["tile", "star", "islamic"],
        move || {
            Ok(art::field_fn(50.0, 50.0, move |x, y| {
                if edge(x, y) < 0.0 {
                    return f64::NAN;
                }
                let r = x.hypot(y);
                let a = y.atan2(x);
                let u = {
                    let k = a * 8.0 / TAU;
                    (k - k.round()).abs() * 2.0
                };
                let star = 22.0 * (1.0 - u) + 12.0 * u;
                let base = 1.0;
                if r < star {
                    base + 3.0 * (1.0 - r / star).powf(0.7) * (1.0 - 0.35 * u)
                } else {
                    base
                }
            }))
        },
    );
    add.relief(
        "Quatrefoil Tile",
        &["tile", "quatrefoil", "gothic"],
        move || {
            let lobes: Vec<Ring> = (0..4)
                .map(|k| {
                    let a = PI / 2.0 * k as f64;
                    art::circle(p(8.5 * a.cos(), 8.5 * a.sin()), 10.0, 120)
                })
                .collect();
            let foil = art::puffy(&art::union(&lobes)?, 4.0)?;
            let plate = art::field_fn(
                50.0,
                50.0,
                move |x, y| if edge(x, y) < 0.0 { f64::NAN } else { 1.0 },
            );
            Ok(art::highest(&plate, &art::raised(foil, 1.0)))
        },
    );
    add.relief("Sunburst Tile", &["tile", "sunburst", "rays"], move || {
        Ok(art::field_fn(50.0, 50.0, move |x, y| {
            if edge(x, y) < 0.0 {
                return f64::NAN;
            }
            let a = y.atan2(x);
            let k = a * 32.0 / TAU;
            let u = (k - k.round()).abs() * 2.0;
            let r = x.hypot(y);
            1.0 + 1.5 * (1.0 - u * u) * (r / 30.0).min(1.0)
                + 2.5 * (1.0 - (r / 8.0).powi(2)).max(0.0).sqrt()
        }))
    });
    add.relief(
        "Maltese Cross Tile",
        &["tile", "cross", "maltese"],
        move || {
            let arm: Ring = vec![p(0.0, 0.0), p(20.0, -9.0), p(16.0, 0.0), p(20.0, 9.0)];
            let cross = art::union(&art::around(&[arm], 4, 0.0))?;
            let raised_cross = art::raised(
                art::shape(&cross, crate::model::shape::Profile::Angular, 30.0, 0.0)?,
                1.0,
            );
            let plate = art::field_fn(
                50.0,
                50.0,
                move |x, y| if edge(x, y) < 0.0 { f64::NAN } else { 1.0 },
            );
            Ok(art::highest(&plate, &raised_cross))
        },
    );
    add.relief(
        "Concentric Squares Tile",
        &["tile", "squares", "steps"],
        move || {
            Ok(art::field_fn(50.0, 50.0, move |x, y| {
                let d = edge(x, y);
                if d < 0.0 {
                    return f64::NAN;
                }
                let ring = (d / 5.0).fract();
                1.0 + 1.2 * (PI * ring).sin().powf(0.6) + 0.2 * (d / 5.0).floor()
            }))
        },
    );
    add.relief("Ripple Tile", &["tile", "ripple", "water"], move || {
        Ok(art::field_fn(50.0, 50.0, move |x, y| {
            if edge(x, y) < 0.0 {
                return f64::NAN;
            }
            let r = x.hypot(y);
            2.0 + 0.8 * (r * TAU / 6.0).cos() * (-r / 30.0).exp()
        }))
    });
    add.relief(
        "Pinwheel Tile",
        &["tile", "pinwheel", "spiral"],
        move || {
            Ok(art::field_fn(50.0, 50.0, move |x, y| {
                if edge(x, y) < 0.0 {
                    return f64::NAN;
                }
                let r = x.hypot(y);
                let a = y.atan2(x) + r / 12.0;
                let blade = (4.0 * a).cos().max(0.0);
                1.0 + 2.5
                    * blade.powf(0.7)
                    * (r / 24.0).min(1.0)
                    * (1.0 - (r / 34.0).min(1.0)).powf(0.3)
            }))
        },
    );
    add.relief(
        "Celtic Square Tile",
        &["tile", "celtic", "knot"],
        move || {
            let knot = knot_relief(&[torus_knot(2, 3, 40.0)], 2.0)?;
            let plate = art::field_fn(
                50.0,
                50.0,
                move |x, y| if edge(x, y) < 0.0 { f64::NAN } else { 1.0 },
            );
            Ok(art::highest(&plate, &art::raised(knot, 1.0)))
        },
    );
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    outlines(out);
    tiles(out);
}
