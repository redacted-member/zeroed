//! Rosettes and medallions: layered petal rosettes from one formula with
//! the petal count, layer count and petal style as parameters, and their
//! outlines as vectors.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;

use super::art::{self, p, round_cap, smoothstep};
use super::{Adder, Entry};
use crate::clipart::samples;

const SOURCE: &str = "rosettes.rs";

/// How a petal's tip is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Petal {
    /// A round tip.
    Round,
    /// A pointed tip.
    Pointed,
    /// A tip with a notch in the middle, like a heart.
    Notched,
    /// A broad petal with three small lobes.
    Lobed,
}

/// The tip shape across a petal, `u` from -1 to 1, as a fraction of the
/// radius the petal reaches.
fn tip(style: Petal, u: f64) -> f64 {
    let u = u.clamp(-1.0, 1.0);
    match style {
        Petal::Round => (1.0 - u * u).max(0.0).sqrt(),
        Petal::Pointed => (1.0 - u.abs()).powf(0.8),
        Petal::Notched => {
            let body = (1.0 - u * u).max(0.0).sqrt();
            body * (1.0 - 0.18 * (-(u / 0.18).powi(2)).exp())
        }
        Petal::Lobed => {
            let body = (1.0 - u * u).max(0.0).powf(0.35);
            body * (0.9 + 0.1 * (3.0 * PI * u).cos())
        }
    }
}

/// Where a point falls in a ring of `n` petals turned by `turn`: the
/// position across the nearest petal, -1 to 1.
fn across(theta: f64, n: usize, turn: f64) -> f64 {
    let k = (theta - turn) * n as f64 / TAU;
    (k - k.round()) * 2.0
}

/// The settings of one rosette.
#[derive(Debug, Clone, Copy)]
pub(super) struct Rosette {
    pub petals: usize,
    pub layers: usize,
    pub style: Petal,
    /// Radius, millimetres.
    pub radius: f64,
    /// How deep each petal is cupped, 0 to 1.
    pub cup: f64,
}

impl Rosette {
    /// The petals' reach at `theta` for layer `layer` (0 the outer one).
    fn edge(&self, layer: usize, theta: f64) -> (f64, f64) {
        let shrink = 1.0 - 0.3 * layer as f64;
        let r = self.radius * shrink;
        let turn = if layer % 2 == 1 {
            PI / self.petals as f64
        } else {
            0.0
        };
        let u = across(theta, self.petals, turn);
        let base = 0.62;
        (r * (base + (1.0 - base) * tip(self.style, u)), u)
    }

    /// Height of the rosette at `(x, y)`, NaN outside it.
    pub fn height(&self, x: f64, y: f64) -> f64 {
        let r = x.hypot(y);
        let theta = y.atan2(x);
        let mut best = f64::NAN;
        let layer_rise = 1.6;
        for layer in 0..self.layers {
            let (edge, u) = self.edge(layer, theta);
            if r > edge {
                continue;
            }
            let t = r / edge;
            let lift = layer as f64 * layer_rise;
            let body = 2.2 * round_cap((1.0 - t) / 0.3);
            let bowl = self.cup * 1.2 * (1.0 - u * u) * (PI * t).sin().powi(2);
            let ridge = 0.5 * (1.0 - u * u).powi(6) * (PI * t).sin();
            let crease = 0.5 * smoothstep(0.75, 1.0, u.abs()) * t;
            let z = lift + body - bowl + ridge - crease;
            if best.is_nan() || z > best {
                best = z;
            }
        }
        let boss_r = self.radius * (0.62 - 0.3 * (self.layers as f64 - 1.0)).max(0.16) * 0.45;
        if r < boss_r {
            let top = (self.layers as f64 - 1.0) * layer_rise + 2.4;
            let boss = top + 1.8 * (1.0 - (r / boss_r).powi(2)).sqrt();
            best = if best.is_nan() { boss } else { best.max(boss) };
        }
        best
    }

    /// The field of the whole rosette.
    pub fn field(&self) -> crate::model::field::Field {
        let size = 2.0 * self.radius + 1.0;
        let me = *self;
        art::field_fn(size, size, move |x, y| me.height(x, y))
    }

    /// The outline of each layer and the centre, as vectors.
    pub fn outlines(&self) -> Vec<art::Ring> {
        // Each layer is drawn small enough to sit inside the notches of the
        // one below, so the loops nest instead of crossing.
        let mut rings = Vec::new();
        let mut scale = 1.0;
        for layer in 0..self.layers {
            let n = self.petals * 48;
            let natural = 1.0 - 0.3 * layer as f64;
            let k = scale / natural;
            rings.push(art::polar(n, |t| self.edge(layer, t).0 * k));
            scale *= 0.62 * 0.92;
        }
        rings.push(art::circle(p(0.0, 0.0), self.radius * scale * 0.55, 96));
        rings
    }
}

const FAMILY: [(&str, usize, usize, Petal, f64); 24] = [
    ("Five Petal Rosette", 5, 1, Petal::Round, 0.4),
    ("Six Petal Rosette", 6, 1, Petal::Round, 0.4),
    ("Eight Petal Rosette", 8, 1, Petal::Round, 0.3),
    ("Twelve Petal Rosette", 12, 1, Petal::Round, 0.2),
    ("Sixteen Petal Rosette", 16, 1, Petal::Round, 0.1),
    ("Pointed Six Rosette", 6, 1, Petal::Pointed, 0.4),
    ("Pointed Eight Rosette", 8, 1, Petal::Pointed, 0.3),
    ("Pointed Twelve Rosette", 12, 1, Petal::Pointed, 0.2),
    ("Notched Five Rosette", 5, 1, Petal::Notched, 0.5),
    ("Notched Eight Rosette", 8, 1, Petal::Notched, 0.4),
    ("Lobed Six Rosette", 6, 1, Petal::Lobed, 0.4),
    ("Lobed Eight Rosette", 8, 1, Petal::Lobed, 0.3),
    ("Double Five Rosette", 5, 2, Petal::Round, 0.5),
    ("Double Six Rosette", 6, 2, Petal::Round, 0.5),
    ("Double Eight Rosette", 8, 2, Petal::Round, 0.4),
    ("Double Twelve Rosette", 12, 2, Petal::Pointed, 0.3),
    ("Double Notched Rosette", 6, 2, Petal::Notched, 0.5),
    ("Double Lobed Rosette", 8, 2, Petal::Lobed, 0.4),
    ("Triple Six Rosette", 6, 3, Petal::Round, 0.5),
    ("Triple Eight Rosette", 8, 3, Petal::Round, 0.5),
    ("Triple Pointed Rosette", 8, 3, Petal::Pointed, 0.4),
    ("Triple Ten Rosette", 10, 3, Petal::Round, 0.4),
    ("Triple Notched Rosette", 5, 3, Petal::Notched, 0.6),
    ("Triple Lobed Rosette", 6, 3, Petal::Lobed, 0.5),
];

fn style_tag(style: Petal) -> &'static str {
    match style {
        Petal::Round => "round petals",
        Petal::Pointed => "pointed petals",
        Petal::Notched => "notched petals",
        Petal::Lobed => "lobed petals",
    }
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Rosettes and Medallions/Rosettes",
        source: SOURCE,
    };
    add.relief("Scalloped Rosette", &["rosette", "flower", "boss"], || {
        samples::sample("Rosette")
            .map(|c| c.field)
            .ok_or_else(|| CoreError::validation("the rosette sample is missing"))
    });
    for (name, petals, layers, style, cup) in FAMILY {
        let rosette = Rosette {
            petals,
            layers,
            style,
            radius: 24.0,
            cup,
        };
        add.relief(name, &["rosette", "flower", style_tag(style)], move || {
            Ok(rosette.field())
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Rosettes and Medallions/Rosette Outlines",
        source: SOURCE,
    };
    for (name, petals, layers, style, cup) in FAMILY.iter().copied().step_by(2) {
        let rosette = Rosette {
            petals,
            layers,
            style,
            radius: 30.0,
            cup,
        };
        let name = name.replace("Rosette", "Outline");
        add.vectors(
            &name,
            &["rosette", "flower", "outline", style_tag(style)],
            move || Ok(art::paths(&rosette.outlines())),
        );
    }
    medallions(add.out);
}

fn medallions(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Rosettes and Medallions/Medallions",
        source: SOURCE,
    };
    add.relief("Beaded Medallion", &["medallion", "beads", "round"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y);
            let theta = y.atan2(x);
            // A ring of beads outside a raised disc with a domed middle.
            let beads = 36.0;
            let k = (theta * beads / TAU).round();
            let a = k * TAU / beads;
            let (bx, by) = (26.5 * a.cos(), 26.5 * a.sin());
            let bead_d = (x - bx).hypot(y - by);
            let bead = if bead_d < 2.0 {
                1.0 + (4.0 - bead_d * bead_d).sqrt()
            } else {
                f64::NAN
            };
            let disc = if r < 24.0 {
                let rim = 1.0 + 2.0 * round_cap((24.0 - r) / 2.0);
                let inner = if r < 21.0 {
                    1.5 + 3.0 * (1.0 - (r / 21.0).powi(2)).sqrt()
                } else {
                    0.0
                };
                rim.max(inner)
            } else {
                f64::NAN
            };
            if bead.is_nan() {
                disc
            } else if disc.is_nan() {
                bead
            } else {
                bead.max(disc)
            }
        }))
    });
    add.relief("Sunburst Medallion", &["medallion", "sun", "rays"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y);
            let theta = y.atan2(x);
            let rays = 24.0;
            let k = (theta * rays / TAU).rem_euclid(rays);
            let u = (k.fract() - 0.5).abs() * 2.0;
            let reach = if (k.floor() as i64).rem_euclid(2) == 0 {
                29.0
            } else {
                23.0
            };
            let edge = 10.0 + (reach - 10.0) * (1.0 - u);
            if r > edge && r > 10.0 {
                return f64::NAN;
            }
            if r <= 10.0 {
                return 3.0 + 2.5 * (1.0 - (r / 10.0).powi(2)).sqrt();
            }
            let t = (r - 10.0) / (reach - 10.0);
            3.0 * (1.0 - t) * (1.0 - u).powf(0.5) + 0.3
        }))
    });
    add.relief(
        "Compass Medallion",
        &["medallion", "compass", "star"],
        || Ok(art::field_fn(60.0, 60.0, |x, y| compass_height(x, y, 29.0))),
    );
    add.relief("Daisy Medallion", &["medallion", "daisy", "flower"], || {
        let daisy = Rosette {
            petals: 16,
            layers: 1,
            style: Petal::Round,
            radius: 20.0,
            cup: 0.3,
        };
        Ok(art::field_fn(60.0, 60.0, move |x, y| {
            let r = x.hypot(y);
            if r > 29.5 {
                return f64::NAN;
            }
            let rim = if r > 25.0 {
                1.0 + 1.5 * round_cap((29.5 - r).min(r - 25.0) / 2.25)
            } else {
                1.0
            };
            let flower = daisy.height(x, y);
            if flower.is_nan() {
                rim
            } else {
                rim.max(1.0 + flower)
            }
        }))
    });
    add.relief(
        "Quatrefoil Medallion",
        &["medallion", "quatrefoil", "gothic"],
        || {
            let lobes: Vec<art::Ring> = (0..4)
                .map(|k| {
                    let a = PI / 2.0 * k as f64;
                    art::circle(p(11.0 * a.cos(), 11.0 * a.sin()), 12.0, 120)
                })
                .collect();
            let shape = art::union(&lobes)?;
            art::dome(&shape, 6.0)
        },
    );
    add.relief("Star Medallion", &["medallion", "star"], || {
        Ok(art::field_fn(60.0, 60.0, |x, y| {
            let r = x.hypot(y);
            if r > 29.5 {
                return f64::NAN;
            }
            let theta = y.atan2(x) - PI / 2.0;
            let u = across(theta, 8, 0.0).abs();
            let spike = 26.0 * (1.0 - u) + 11.0 * u;
            let star = if r < spike {
                1.0 + 3.5 * (1.0 - r / spike) * (1.0 - 0.5 * u)
            } else {
                0.0
            };
            let rim = if r > 27.0 {
                1.0 + 1.2 * round_cap((29.5 - r) / 1.25)
            } else {
                1.0
            };
            rim.max(star)
        }))
    });
}

/// A compass rose: four long and four short faceted points on a disc.
pub(super) fn compass_height(x: f64, y: f64, radius: f64) -> f64 {
    let r = x.hypot(y);
    if r > radius {
        return f64::NAN;
    }
    let theta = y.atan2(x);
    let mut best: f64 = 0.4;
    for (count, reach, width, turn, top) in [
        (4usize, radius * 0.95, 0.55, 0.0, 5.0),
        (4usize, radius * 0.62, 0.5, PI / 4.0, 3.5),
    ] {
        let u = across(theta, count, turn);
        let half = width * (1.0 - r / reach);
        if r < reach && u.abs() < half {
            let side = if u > 0.0 { 1.0 } else { 0.7 };
            let h = top * (1.0 - r / reach).powf(0.6) * (1.0 - u.abs() / half.max(1e-9)) * side;
            best = best.max(0.4 + h);
        }
    }
    let boss = radius * 0.1;
    if r < boss {
        best = best.max(0.4 + 5.2 * (1.0 - (r / boss).powi(2)).sqrt());
    }
    let ring = (r - radius * 0.8).abs();
    if ring < 0.8 {
        best = best.max(0.4 + 1.0 * round_cap((0.8 - ring) / 0.8));
    }
    best
}
