//! Frames and plaques: sign blanks as outlines and double outlines, and as
//! reliefs with edges molded by the profile library.

use std::f64::consts::PI;

use crate::error::CoreError;
use crate::geom::Point;

use super::art::{self, p, Ring};
use super::profiles;
use super::{Adder, Entry};

const SOURCE: &str = "frames.rs";

/// How a corner of a plaque is cut.
#[derive(Debug, Clone, Copy)]
pub(super) enum Corner {
    Round,
    Chamfer,
    Notch,
    Step,
    Ogee,
}

/// A rectangle `w` by `h` centred on the origin whose corners are cut
/// `r` deep in the given style.
pub(super) fn cornered(w: f64, h: f64, r: f64, corner: Corner) -> Ring {
    // One corner in local coordinates: the corner at the origin, the curve
    // running from (0, -r) on the right edge to (-r, 0) on the top edge.
    let local: Vec<Point> = match corner {
        Corner::Round => (0..=12)
            .map(|k| {
                let a = PI / 2.0 * k as f64 / 12.0;
                p(-r + r * a.cos(), -r + r * a.sin())
            })
            .collect(),
        Corner::Chamfer => vec![p(0.0, -r), p(-r, 0.0)],
        Corner::Notch => (0..=12)
            .map(|k| {
                let a = -PI / 2.0 - PI / 2.0 * k as f64 / 12.0;
                p(r * a.cos(), r * a.sin())
            })
            .collect(),
        Corner::Step => vec![p(0.0, -r), p(-r, -r), p(-r, 0.0)],
        Corner::Ogee => (0..=16)
            .map(|k| {
                let t = k as f64 / 16.0;
                p(-r * t, -r + r * (0.5 - 0.5 * (PI * t).cos()))
            })
            .collect(),
    };
    let corners = [
        (w / 2.0, -h / 2.0),
        (w / 2.0, h / 2.0),
        (-w / 2.0, h / 2.0),
        (-w / 2.0, -h / 2.0),
    ];
    let mut ring = Vec::new();
    for (k, (cx, cy)) in corners.iter().enumerate() {
        // Turn the local corner so it fits this corner; the top right one
        // is the local frame itself.
        let turn = PI / 2.0 * (k as f64 - 1.0);
        let m = art::place(turn, *cx, *cy);
        ring.extend(art::moved(&local, &m));
    }
    ring
}

/// Points along the quadratic curve from `a` to `b` pulled towards `c`.
fn quad(a: Point, c: Point, b: Point, n: usize) -> Vec<Point> {
    (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let u = 1.0 - t;
            p(
                u * u * a.x + 2.0 * u * t * c.x + t * t * b.x,
                u * u * a.y + 2.0 * u * t * c.y + t * t * b.y,
            )
        })
        .collect()
}

/// Every plaque outline: name, tags and outline rings.
pub(super) fn outlines() -> Vec<art::Design<Result<Vec<Ring>, CoreError>>> {
    vec![
        ("Rectangle", &["rectangle", "sign"], || {
            Ok(vec![art::rect(p(0.0, 0.0), 100.0, 60.0)])
        }),
        (
            "Rounded Rectangle",
            &["rectangle", "rounded", "sign"],
            || Ok(vec![cornered(100.0, 60.0, 8.0, Corner::Round)]),
        ),
        (
            "Chamfered Rectangle",
            &["rectangle", "chamfer", "sign"],
            || Ok(vec![cornered(100.0, 60.0, 8.0, Corner::Chamfer)]),
        ),
        (
            "Notched Rectangle",
            &["rectangle", "notched", "sign"],
            || Ok(vec![cornered(100.0, 60.0, 10.0, Corner::Notch)]),
        ),
        (
            "Stepped Rectangle",
            &["rectangle", "stepped", "sign"],
            || Ok(vec![cornered(100.0, 60.0, 7.0, Corner::Step)]),
        ),
        ("Ogee Corner", &["rectangle", "ogee", "sign"], || {
            Ok(vec![cornered(100.0, 60.0, 12.0, Corner::Ogee)])
        }),
        ("Oval", &["oval", "ellipse", "sign"], || {
            Ok(vec![art::ellipse(p(0.0, 0.0), 50.0, 30.0, 200)])
        }),
        ("Circle", &["circle", "round", "sign"], || {
            Ok(vec![art::circle(p(0.0, 0.0), 35.0, 200)])
        }),
        ("Octagon", &["octagon", "sign"], || {
            Ok(vec![art::polygon(8, 36.0, PI / 8.0)])
        }),
        ("Arch Top", &["arch", "sign"], || {
            let mut ring = vec![p(-40.0, -35.0), p(40.0, -35.0), p(40.0, 5.0)];
            ring.extend((1..40).map(|k| {
                let a = PI * k as f64 / 40.0;
                p(40.0 * a.cos(), 5.0 + 40.0 * a.sin())
            }));
            ring.push(p(-40.0, 5.0));
            Ok(vec![ring])
        }),
        ("Tombstone", &["tombstone", "rounded top", "sign"], || {
            let mut ring = cornered(70.0, 90.0, 25.0, Corner::Round);
            // Square the bottom corners again.
            ring.retain(|q| q.y > -45.0 + 25.0 - 1e-9);
            ring.insert(0, p(35.0, -45.0));
            ring.push(p(-35.0, -45.0));
            Ok(vec![ring])
        }),
        (
            "Gothic Arch",
            &["arch", "gothic", "pointed", "sign"],
            || {
                // An equilateral arch: each side an arc centred on the other
                // side's springing point.
                let (half, r, spring): (f64, f64, f64) = (35.0, 70.0, 10.0);
                let top = (half / r).acos();
                let mut ring = vec![p(-half, -40.0), p(half, -40.0)];
                let right: Vec<Point> = (0..24)
                    .map(|k| {
                        let a = top * k as f64 / 24.0;
                        p(-half + r * a.cos(), spring + r * a.sin())
                    })
                    .collect();
                ring.extend(right.iter().copied());
                ring.push(p(0.0, spring + r * top.sin()));
                ring.extend(right.iter().rev().map(|q| p(-q.x, q.y)));
                Ok(vec![ring])
            },
        ),
        ("Shield", &["shield", "crest", "heraldry"], || {
            Ok(vec![art::mirrored(
                &[
                    p(0.0, 38.0),
                    p(20.0, 40.0),
                    p(38.0, 42.0),
                    p(37.0, 10.0),
                    p(28.0, -20.0),
                    p(12.0, -38.0),
                    p(0.0, -46.0),
                ],
                8,
            )])
        }),
        ("Scalloped Oval", &["oval", "scallop", "sign"], || {
            Ok(vec![(0..480)
                .map(|k| {
                    let t = 2.0 * PI * k as f64 / 480.0;
                    let bump = 1.0 + 0.05 * (16.0 * t).cos().abs();
                    p(48.0 * bump * t.cos(), 30.0 * bump * t.sin())
                })
                .collect()])
        }),
        ("Ticket", &["ticket", "notched", "sign"], || {
            let body = cornered(100.0, 60.0, 9.0, Corner::Notch);
            let bites = vec![
                art::circle(p(50.0, 0.0), 7.0, 48),
                art::circle(p(-50.0, 0.0), 7.0, 48),
            ];
            art::cut(&[body], &bites)
        }),
        ("Bracket", &["bracket", "scroll", "sign"], || {
            let mut ring = vec![p(-40.0, -30.0), p(40.0, -30.0)];
            ring.extend(
                quad(p(40.0, -30.0), p(42.0, -4.0), p(56.0, 0.0), 12)
                    .into_iter()
                    .skip(1),
            );
            ring.extend(
                quad(p(56.0, 0.0), p(42.0, 4.0), p(40.0, 30.0), 12)
                    .into_iter()
                    .skip(1),
            );
            ring.push(p(-40.0, 30.0));
            ring.extend(
                quad(p(-40.0, 30.0), p(-42.0, 4.0), p(-56.0, 0.0), 12)
                    .into_iter()
                    .skip(1),
            );
            ring.extend(
                quad(p(-56.0, 0.0), p(-42.0, -4.0), p(-40.0, -30.0), 12)
                    .into_iter()
                    .skip(1),
            );
            ring.pop();
            Ok(vec![ring])
        }),
        ("Banner", &["banner", "ribbon", "swallowtail"], || {
            Ok(vec![vec![
                p(-55.0, -14.0),
                p(55.0, -14.0),
                p(45.0, 0.0),
                p(55.0, 14.0),
                p(-55.0, 14.0),
                p(-45.0, 0.0),
            ]])
        }),
        ("Cloud", &["cloud", "bubble"], || {
            let circles = vec![
                art::circle(p(-30.0, -5.0), 18.0, 96),
                art::circle(p(-8.0, 10.0), 22.0, 96),
                art::circle(p(18.0, 8.0), 20.0, 96),
                art::circle(p(35.0, -6.0), 15.0, 96),
                art::rect(p(2.0, -12.0), 66.0, 22.0),
            ];
            art::union(&circles)
        }),
        ("Keystone", &["keystone", "trapezoid"], || {
            Ok(vec![vec![
                p(-35.0, -35.0),
                p(35.0, -35.0),
                p(48.0, 35.0),
                p(-48.0, 35.0),
            ]])
        }),
        ("Label Tag", &["tag", "label", "gift"], || {
            Ok(vec![
                vec![
                    p(-50.0, -25.0),
                    p(35.0, -25.0),
                    p(55.0, 0.0),
                    p(35.0, 25.0),
                    p(-50.0, 25.0),
                ],
                art::circle(p(38.0, 0.0), 4.0, 48),
            ])
        }),
    ]
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Frames and Plaques/Plaque Outlines",
        source: SOURCE,
    };
    for (name, tags, make) in outlines() {
        let mut all = vec!["plaque", "outline"];
        all.extend_from_slice(tags);
        add.vectors(&format!("{name} Plaque"), &all, move || {
            Ok(art::paths(&make()?))
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Frames and Plaques/Frame Outlines",
        source: SOURCE,
    };
    for (name, tags, make) in outlines() {
        let mut all = vec!["frame", "double line", "border"];
        all.extend_from_slice(tags);
        add.vectors(&format!("{name} Frame"), &all, move || {
            let rings = make()?;
            let mut out = rings.clone();
            out.extend(art::offset(&rings, -6.0));
            Ok(art::paths(&out))
        });
    }
    reliefs(add.out);
}

/// A relief of `rings` whose edge follows the named profile scaled to
/// `width` across and `rise` high, flat on top beyond it, standing on a
/// base `base` high.
fn molded(
    rings: &[Ring],
    profile: &str,
    width: f64,
    rise: f64,
    base: f64,
) -> Result<crate::model::field::Field, CoreError> {
    let pts = profiles::named(profile);
    let (pw, ph) = (
        profiles::width(&pts),
        pts.iter().map(|q| q.y).fold(0.0, f64::max).max(1e-9),
    );
    art::by_depth(rings, move |d| {
        let x = (d / width).min(1.0) * pw;
        base + rise * profiles::height_at(&pts, x) / ph
    })
}

/// A frame: the band `width` wide inside `rings`, its section the named
/// profile rising from the outer edge inward; empty inside the band.
fn framed(
    rings: &[Ring],
    profile: &str,
    width: f64,
    rise: f64,
) -> Result<crate::model::field::Field, CoreError> {
    let pts = profiles::named(profile);
    let (pw, ph) = (
        profiles::width(&pts),
        pts.iter().map(|q| q.y).fold(0.0, f64::max).max(1e-9),
    );
    art::by_depth(rings, move |d| {
        if d > width {
            return f64::NAN;
        }
        1.0 + rise * profiles::height_at(&pts, d / width * pw) / ph
    })
}

fn reliefs(out: &mut Vec<Entry>) {
    let shapes = outlines();
    let find = |name: &str| {
        shapes
            .iter()
            .find(|(n, ..)| *n == name)
            .map(|(_, _, make)| *make)
    };
    let mut add = Adder {
        out,
        category: "Frames and Plaques/Plaques",
        source: SOURCE,
    };
    let plaques = [
        ("Rounded Rectangle", "Ogee", "Rounded Plaque with Ogee Edge"),
        (
            "Rectangle",
            "Cove with Fillet",
            "Rectangle Plaque with Cove Edge",
        ),
        (
            "Notched Rectangle",
            "Round Over",
            "Notched Plaque with Round Over",
        ),
        ("Oval", "Roman Ogee", "Oval Plaque with Roman Ogee"),
        ("Circle", "Classic Crown", "Round Plaque with Crown Edge"),
        ("Arch Top", "Chamfer", "Arch Plaque with Chamfer"),
        (
            "Tombstone",
            "Thumbnail",
            "Tombstone Plaque with Thumbnail Edge",
        ),
        ("Shield", "Bullnose", "Shield Plaque"),
        ("Scalloped Oval", "Cove", "Scalloped Plaque"),
        ("Ogee Corner", "Step and Cove", "Ogee Corner Plaque"),
        ("Octagon", "Shallow Bevel", "Octagon Plaque with Bevel"),
        ("Banner", "Round Over", "Banner Plaque"),
        ("Bracket", "Cyma Recta", "Bracket Plaque"),
        ("Gothic Arch", "Cove and Bead", "Gothic Plaque"),
        ("Keystone", "Chamfer", "Keystone Plaque"),
    ];
    for (shape, profile, name) in plaques {
        let Some(make) = find(shape) else { continue };
        add.relief(name, &["plaque", "sign", "molded edge"], move || {
            molded(&make()?, profile, 8.0, 3.0, 1.0)
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Frames and Plaques/Frames",
        source: SOURCE,
    };
    let frames = [
        ("Rectangle", "Picture Frame Ogee", "Ogee Picture Frame"),
        (
            "Rounded Rectangle",
            "Picture Frame Cove",
            "Rounded Cove Frame",
        ),
        ("Oval", "Picture Frame Scoop", "Oval Scoop Frame"),
        ("Circle", "Picture Frame Bolection", "Round Bolection Frame"),
        ("Arch Top", "Colonial Casing", "Arch Casing Frame"),
        ("Octagon", "Reeded", "Reeded Octagon Frame"),
        ("Notched Rectangle", "Beaded Ogee", "Notched Beaded Frame"),
        ("Shield", "Astragal", "Shield Frame"),
        ("Scalloped Oval", "Torus", "Scalloped Torus Frame"),
        ("Chamfered Rectangle", "Fluted Casing", "Fluted Frame"),
    ];
    for (shape, profile, name) in frames {
        let Some(make) = find(shape) else { continue };
        add.relief(name, &["frame", "picture frame", "molded"], move || {
            framed(&make()?, profile, 12.0, 5.0)
        });
    }
}
