//! Molding and edge profiles: open 2D sections for the Moulding toolpath
//! and Two Rail Sweep, built from the classic elements (fillets, coves,
//! round overs, ogees, beads) laid one after another. Frames and plaques
//! use the same profiles for their molded edges.
//!
//! A profile runs from its outer foot at the origin to the right and up,
//! X always increasing, so it reads as a height over its width.

use std::f64::consts::PI;

use crate::geom::Point;

use super::art::{self, p};
use super::{Adder, Entry};

const SOURCE: &str = "profiles.rs";

/// Points per curved element.
const STEPS: usize = 24;

/// One element of a profile, sizes in millimetres.
#[derive(Debug, Clone, Copy)]
pub(super) enum Part {
    /// Level run.
    Flat(f64),
    /// Straight rise.
    Step(f64),
    /// Straight slope, width then rise.
    Chamfer(f64, f64),
    /// Concave quarter ellipse, width then rise.
    Cove(f64, f64),
    /// Convex quarter ellipse, width then rise.
    Round(f64, f64),
    /// Concave then convex S curve, width then rise.
    Ogee(f64, f64),
    /// Convex then concave S curve, width then rise.
    Reverse(f64, f64),
    /// A half round bump of this radius, back to the same level.
    Bead(f64),
    /// A half round groove of this radius, back to the same level.
    Flute(f64),
}

/// The points of a profile built from `parts`.
pub(super) fn build(parts: &[Part]) -> Vec<Point> {
    let mut pts = vec![p(0.0, 0.0)];
    let mut at = p(0.0, 0.0);
    let curve = |pts: &mut Vec<Point>, at: &mut Point, f: &dyn Fn(f64) -> (f64, f64)| {
        let base = *at;
        for k in 1..=STEPS {
            let (dx, dy) = f(k as f64 / STEPS as f64);
            pts.push(p(base.x + dx, base.y + dy));
        }
        *at = *pts.last().unwrap_or(&base);
    };
    for part in parts {
        match *part {
            Part::Flat(w) => {
                at = p(at.x + w, at.y);
                pts.push(at);
            }
            Part::Step(h) => {
                // A hair of width keeps X increasing.
                at = p(at.x + 0.01, at.y + h);
                pts.push(at);
            }
            Part::Chamfer(w, h) => {
                at = p(at.x + w, at.y + h);
                pts.push(at);
            }
            Part::Cove(w, h) => curve(&mut pts, &mut at, &|t| {
                let a = t * PI / 2.0;
                (w * a.sin(), h * (1.0 - a.cos()))
            }),
            Part::Round(w, h) => curve(&mut pts, &mut at, &|t| {
                let a = t * PI / 2.0;
                (w * (1.0 - a.cos()), h * a.sin())
            }),
            Part::Ogee(w, h) => curve(&mut pts, &mut at, &|t| {
                (w * t, h * (0.5 - 0.5 * (PI * t).cos()))
            }),
            Part::Reverse(w, h) => curve(&mut pts, &mut at, &|t| {
                let s = if t < 0.5 {
                    0.5 * (1.0 - (1.0 - 2.0 * t).powi(2)).sqrt()
                } else {
                    1.0 - 0.5 * (1.0 - (2.0 * t - 1.0).powi(2)).sqrt()
                };
                (w * t, h * s)
            }),
            Part::Bead(r) => curve(&mut pts, &mut at, &|t| {
                let a = PI * (1.0 - t);
                (r + r * a.cos(), r * a.sin())
            }),
            Part::Flute(r) => curve(&mut pts, &mut at, &|t| {
                let a = PI * (1.0 - t);
                (r + r * a.cos(), -0.6 * r * a.sin())
            }),
        }
    }
    // Keep everything at or above the foot.
    let low = pts.iter().map(|q| q.y).fold(0.0, f64::min);
    if low < 0.0 {
        for q in &mut pts {
            q.y -= low;
        }
    }
    pts
}

/// The height of a profile at `x`, read between its points; beyond its
/// ends it holds its end heights.
pub(super) fn height_at(pts: &[Point], x: f64) -> f64 {
    if x <= pts[0].x {
        return pts[0].y;
    }
    for pair in pts.windows(2) {
        if x <= pair[1].x {
            let span = (pair[1].x - pair[0].x).max(1e-12);
            let t = (x - pair[0].x) / span;
            return pair[0].y + t * (pair[1].y - pair[0].y);
        }
    }
    pts[pts.len() - 1].y
}

/// The width a profile covers.
pub(super) fn width(pts: &[Point]) -> f64 {
    pts[pts.len() - 1].x - pts[0].x
}

use Part::{Bead, Chamfer, Cove, Flat, Flute, Ogee, Reverse, Round, Step};

/// Every profile: name, subcategory, tags and parts.
pub(super) fn all() -> Vec<(
    &'static str,
    &'static str,
    &'static [&'static str],
    Vec<Part>,
)> {
    vec![
        (
            "Cove",
            "Router Edges",
            &["cove", "concave"],
            vec![Cove(12.0, 12.0)],
        ),
        (
            "Cove with Fillet",
            "Router Edges",
            &["cove", "fillet"],
            vec![Flat(2.0), Cove(10.0, 10.0), Flat(3.0)],
        ),
        (
            "Round Over",
            "Router Edges",
            &["round over", "convex"],
            vec![Round(12.0, 12.0)],
        ),
        (
            "Round Over with Step",
            "Router Edges",
            &["round over", "step"],
            vec![Step(3.0), Round(10.0, 10.0), Flat(3.0)],
        ),
        (
            "Chamfer",
            "Router Edges",
            &["chamfer", "bevel"],
            vec![Chamfer(12.0, 12.0)],
        ),
        (
            "Steep Chamfer",
            "Router Edges",
            &["chamfer", "bevel"],
            vec![Chamfer(6.0, 14.0), Flat(4.0)],
        ),
        (
            "Shallow Bevel",
            "Router Edges",
            &["chamfer", "bevel", "raised panel"],
            vec![Chamfer(20.0, 6.0), Flat(3.0)],
        ),
        (
            "Ogee",
            "Router Edges",
            &["ogee", "cyma recta"],
            vec![Ogee(14.0, 12.0)],
        ),
        (
            "Roman Ogee",
            "Router Edges",
            &["ogee", "roman"],
            vec![Step(2.0), Reverse(14.0, 12.0), Flat(2.0)],
        ),
        (
            "Bead",
            "Router Edges",
            &["bead"],
            vec![Step(4.0), Bead(5.0), Flat(3.0)],
        ),
        (
            "Bullnose",
            "Router Edges",
            &["bullnose", "half round"],
            vec![Round(8.0, 8.0), Flat(2.0)],
        ),
        (
            "Thumbnail",
            "Router Edges",
            &["thumbnail", "table edge"],
            vec![Step(2.0), Round(16.0, 6.0), Flat(3.0)],
        ),
        (
            "Cavetto",
            "Classical",
            &["cavetto", "cove"],
            vec![Step(2.0), Cove(8.0, 14.0), Flat(3.0)],
        ),
        (
            "Ovolo",
            "Classical",
            &["ovolo", "quarter round"],
            vec![Flat(2.0), Round(10.0, 10.0), Step(2.0), Flat(3.0)],
        ),
        (
            "Cyma Recta",
            "Classical",
            &["ogee", "cyma recta"],
            vec![Flat(2.0), Ogee(14.0, 14.0), Flat(2.0)],
        ),
        (
            "Cyma Reversa",
            "Classical",
            &["ogee", "cyma reversa"],
            vec![Flat(2.0), Reverse(14.0, 14.0), Flat(2.0)],
        ),
        (
            "Torus",
            "Classical",
            &["torus", "bead"],
            vec![Bead(8.0), Flat(3.0)],
        ),
        (
            "Scotia",
            "Classical",
            &["scotia", "cove"],
            vec![
                Step(3.0),
                Flat(1.0),
                Cove(6.0, 4.0),
                Cove(6.0, 8.0),
                Flat(2.0),
            ],
        ),
        (
            "Astragal",
            "Classical",
            &["astragal", "bead", "fillet"],
            vec![
                Step(2.0),
                Flat(2.0),
                Bead(4.0),
                Flat(2.0),
                Step(2.0),
                Flat(2.0),
            ],
        ),
        (
            "Fillet and Fascia",
            "Classical",
            &["fillet", "fascia", "step"],
            vec![Step(4.0), Flat(4.0), Step(4.0), Flat(6.0)],
        ),
        (
            "Reeded",
            "Decorative",
            &["reed", "beads"],
            vec![Step(2.0), Bead(3.0), Bead(3.0), Bead(3.0), Flat(2.0)],
        ),
        (
            "Fluted",
            "Decorative",
            &["flute", "grooves"],
            vec![
                Step(6.0),
                Flute(3.0),
                Flat(1.0),
                Flute(3.0),
                Flat(1.0),
                Flute(3.0),
                Flat(2.0),
            ],
        ),
        (
            "Cove and Bead",
            "Decorative",
            &["cove", "bead"],
            vec![Cove(8.0, 8.0), Flat(1.0), Bead(4.0), Flat(2.0)],
        ),
        (
            "Ogee and Bead",
            "Decorative",
            &["ogee", "bead"],
            vec![Ogee(10.0, 9.0), Flat(1.0), Bead(3.5), Flat(2.0)],
        ),
        (
            "Double Round",
            "Decorative",
            &["round over", "double"],
            vec![Round(6.0, 6.0), Flat(2.0), Round(6.0, 6.0), Flat(2.0)],
        ),
        (
            "Triple Step",
            "Decorative",
            &["steps", "fillet"],
            vec![
                Step(3.0),
                Flat(4.0),
                Step(3.0),
                Flat(4.0),
                Step(3.0),
                Flat(4.0),
            ],
        ),
        (
            "Step and Cove",
            "Decorative",
            &["step", "cove"],
            vec![Step(4.0), Flat(2.0), Cove(8.0, 8.0), Flat(3.0)],
        ),
        (
            "Beaded Ogee",
            "Decorative",
            &["ogee", "bead"],
            vec![Bead(3.0), Ogee(12.0, 10.0), Flat(3.0)],
        ),
        (
            "Classic Crown",
            "Crown and Casing",
            &["crown", "cornice"],
            vec![
                Flat(2.0),
                Cove(8.0, 10.0),
                Flat(1.5),
                Step(2.0),
                Ogee(10.0, 10.0),
                Flat(2.0),
            ],
        ),
        (
            "Cove Crown",
            "Crown and Casing",
            &["crown", "cove"],
            vec![
                Step(3.0),
                Flat(1.0),
                Cove(16.0, 18.0),
                Flat(1.0),
                Step(2.0),
                Flat(2.0),
            ],
        ),
        (
            "Colonial Crown",
            "Crown and Casing",
            &["crown", "colonial"],
            vec![
                Bead(2.5),
                Ogee(10.0, 12.0),
                Flat(1.0),
                Cove(6.0, 6.0),
                Flat(2.0),
            ],
        ),
        (
            "Colonial Casing",
            "Crown and Casing",
            &["casing", "colonial", "trim"],
            vec![
                Step(3.0),
                Round(4.0, 3.0),
                Flat(6.0),
                Ogee(8.0, 6.0),
                Flat(3.0),
            ],
        ),
        (
            "Ranch Casing",
            "Crown and Casing",
            &["casing", "ranch", "trim"],
            vec![Round(3.0, 4.0), Chamfer(20.0, 4.0), Round(4.0, 3.0)],
        ),
        (
            "Fluted Casing",
            "Crown and Casing",
            &["casing", "flute", "trim"],
            vec![
                Step(8.0),
                Flat(2.0),
                Flute(3.0),
                Flat(2.0),
                Flute(3.0),
                Flat(2.0),
                Flute(3.0),
                Flat(2.0),
            ],
        ),
        (
            "Base Molding",
            "Crown and Casing",
            &["base", "skirting"],
            vec![Step(14.0), Flat(3.0), Ogee(6.0, 5.0), Flat(3.0)],
        ),
        (
            "Chair Rail",
            "Crown and Casing",
            &["chair rail", "rail"],
            vec![
                Round(4.0, 4.0),
                Flat(2.0),
                Bead(4.0),
                Flat(2.0),
                Ogee(6.0, 4.0),
                Flat(2.0),
            ],
        ),
        (
            "Picture Frame Ogee",
            "Picture Frame",
            &["picture frame", "ogee"],
            vec![Step(4.0), Flat(3.0), Ogee(14.0, 10.0), Bead(2.5), Flat(4.0)],
        ),
        (
            "Picture Frame Cove",
            "Picture Frame",
            &["picture frame", "cove"],
            vec![
                Step(3.0),
                Bead(2.0),
                Cove(14.0, 12.0),
                Flat(2.0),
                Step(2.0),
                Flat(3.0),
            ],
        ),
        (
            "Picture Frame Scoop",
            "Picture Frame",
            &["picture frame", "scoop"],
            vec![
                Round(6.0, 8.0),
                Flat(1.0),
                Flute(8.0),
                Flat(1.0),
                Chamfer(3.0, 2.0),
            ],
        ),
        (
            "Picture Frame Bolection",
            "Picture Frame",
            &["picture frame", "bolection"],
            vec![
                Step(2.0),
                Round(8.0, 12.0),
                Flat(2.0),
                Cove(8.0, 4.0),
                Flat(3.0),
            ],
        ),
        (
            "Table Edge",
            "Picture Frame",
            &["table edge", "nosing"],
            vec![Round(5.0, 6.0), Flat(3.0), Cove(5.0, 4.0), Flat(3.0)],
        ),
        (
            "Stair Nosing",
            "Picture Frame",
            &["nosing", "stair"],
            vec![Round(10.0, 10.0), Flat(12.0)],
        ),
    ]
}

/// The profile of the given name, or a plain round over.
pub(super) fn named(name: &str) -> Vec<Point> {
    all()
        .into_iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, _, _, parts)| build(&parts))
        .unwrap_or_else(|| build(&[Round(10.0, 10.0)]))
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    for (name, sub, tags, parts) in all() {
        let category: &'static str = match sub {
            "Router Edges" => "Molding and Edge Profiles/Router Edges",
            "Classical" => "Molding and Edge Profiles/Classical",
            "Decorative" => "Molding and Edge Profiles/Decorative",
            "Crown and Casing" => "Molding and Edge Profiles/Crown and Casing",
            _ => "Molding and Edge Profiles/Picture Frame",
        };
        let mut add = Adder {
            out,
            category,
            source: SOURCE,
        };
        let mut all_tags = vec!["profile", "molding"];
        all_tags.extend_from_slice(tags);
        add.line_art(&format!("{name} Profile"), &all_tags, move || {
            Ok(vec![art::open_path(&build(&parts))])
        });
    }
}
