//! Features cut into panels: notches, arches and rounded corners change a
//! panel's outline; cutouts, holes and hand holds are cut through; pockets
//! and hinge cups are sunk to a depth; shelf pin and slide screw holes are
//! drilled.
//!
//! Positions are panel coordinates: u along the panel's first flat axis
//! (in X, Y, Z order) and v along the second, from the box's corner. `u`
//! and `v` give a feature's middle. Edges are named in furniture space
//! (left, right, front, back, bottom, top); faces too, or `inside` (the
//! face towards the middle of the piece) and `outside`.

use serde::{Deserialize, Serialize};

use crate::assembly::pose::Vec3;
use crate::geom::{Path, Point, SubPath, Tolerance};
use crate::joints::geometry::{ccw, P2};

use super::eval::{Feature, Issue, Panel, TOL};
use super::generate::ring_path;
use super::layout::PanelFrame;

/// What a feature's vector is for.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FeatureRole {
    /// Cut right through (an inside profile).
    Hole,
    /// Cleared from the top to a depth, millimetres.
    Pocket { depth: f64 },
    /// Drilled at the circle's middle with a drill of its size.
    Drill { depth: f64, diameter: f64 },
}

/// A vector a feature makes, flat coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureVector {
    pub path: Path,
    pub role: FeatureRole,
    pub name: String,
}

/// How far a cut runs past the outline so it leaves no sliver.
const MARGIN: f64 = 200.0;

/// Default hinge cup sizes (35 mm cup, 12.5 mm deep, middle 22.5 mm from
/// the hinge edge at a 5 mm boring distance, 80 mm from the door's ends).
pub const CUP_DIAMETER: f64 = 35.0;
pub const CUP_DEPTH: f64 = 12.5;
pub const CUP_FROM_EDGE: f64 = 22.5;
pub const CUP_FROM_END: f64 = 80.0;

/// System 32 shelf pins: 5 mm holes 10 mm deep, rows 37 mm in, 32 mm apart.
pub const PIN_DIAMETER: f64 = 5.0;
pub const PIN_DEPTH: f64 = 10.0;

/// Slide screw pilots: 2.5 mm, 37 mm from the front, then every 96 mm.
pub const SLIDE_PILOT: f64 = 2.5;

/// The hinges a door of `height` takes (up to 900 mm two, 1600 three,
/// 2000 four, else five).
pub fn hinge_count(height: f64) -> u32 {
    match height {
        h if h <= 900.0 => 2,
        h if h <= 1600.0 => 3,
        h if h <= 2000.0 => 4,
        _ => 5,
    }
}

/// The furniture axis and side (high or low) an edge word names.
fn edge_side(word: &str) -> Option<(usize, bool)> {
    Some(match word {
        "left" => (0, false),
        "right" => (0, true),
        "front" => (1, false),
        "back" => (1, true),
        "bottom" => (2, false),
        "top" => (2, true),
        _ => return None,
    })
}

/// The direction (+1 or -1 along the panel's normal) a face word names, or
/// `None` when it is not one of the panel's broad faces.
pub fn face_direction(panel: &Panel, word: &str, middle: Vec3) -> Option<f64> {
    let n = panel.normal;
    let inwards = {
        let d = middle[n] - panel.centre()[n];
        if d < -TOL {
            -1.0
        } else {
            1.0
        }
    };
    match word {
        "inside" => Some(inwards),
        "outside" => Some(-inwards),
        other => {
            let (axis, high) = edge_side(other)?;
            (axis == n).then_some(if high { 1.0 } else { -1.0 })
        }
    }
}

/// The face a feature sinks into or drills (pockets, cups, pins, slide
/// holes), as a direction along the panel's normal; `None` for features
/// cut through.
pub fn feature_face(panel: &Panel, f: &Feature, middle: Vec3) -> Option<f64> {
    let sunk = match f.kind.as_str() {
        "pocket" | "hingeCups" | "shelfPins" | "slideHoles" => true,
        "hole" => f.numbers.contains_key("depth"),
        _ => false,
    };
    if !sunk {
        return None;
    }
    face_direction(panel, f.word("face", "inside"), middle)
}

/// A panel's sizes along its flat axes.
fn extents(panel: &Panel, frame: &PanelFrame) -> [f64; 2] {
    [panel.size(frame.u), panel.size(frame.v)]
}

/// Which flat coordinate (0 u, 1 v) runs along furniture axis `k`.
fn flat_index(frame: &PanelFrame, k: usize) -> Option<usize> {
    if k == frame.u {
        Some(0)
    } else if k == frame.v {
        Some(1)
    } else {
        None
    }
}

/// A rectangle in panel coordinates, flat and counter-clockwise.
fn rect(panel: &Panel, frame: &PanelFrame, u0: f64, u1: f64, v0: f64, v1: f64) -> Vec<P2> {
    ccw(vec![
        frame.flat_uv(panel, u0, v0),
        frame.flat_uv(panel, u1, v0),
        frame.flat_uv(panel, u1, v1),
        frame.flat_uv(panel, u0, v1),
    ])
}

/// A rectangle `w` by `h` about `(u, v)` with corners rounded `r`, as a
/// ring (arcs flattened finely).
fn rounded_rect(
    panel: &Panel,
    frame: &PanelFrame,
    u: f64,
    v: f64,
    w: f64,
    h: f64,
    r: f64,
) -> Vec<P2> {
    let r = r.clamp(0.0, w.min(h) / 2.0);
    if r <= 1e-9 {
        return rect(
            panel,
            frame,
            u - w / 2.0,
            u + w / 2.0,
            v - h / 2.0,
            v + h / 2.0,
        );
    }
    let mut points = Vec::new();
    let corners = [
        (u + w / 2.0 - r, v - h / 2.0 + r, -90.0_f64),
        (u + w / 2.0 - r, v + h / 2.0 - r, 0.0),
        (u - w / 2.0 + r, v + h / 2.0 - r, 90.0),
        (u - w / 2.0 + r, v - h / 2.0 + r, 180.0),
    ];
    let steps = 12;
    for (cu, cv, start) in corners {
        for k in 0..=steps {
            let a = (start + 90.0 * f64::from(k) / f64::from(steps)).to_radians();
            points.push(frame.flat_uv(panel, cu + r * a.cos(), cv + r * a.sin()));
        }
    }
    ccw(points)
}

/// A circle as a closed path of two arcs, flat coordinates.
fn circle(panel: &Panel, frame: &PanelFrame, u: f64, v: f64, r: f64) -> Path {
    let c = frame.flat_uv(panel, u, v);
    let mut sub = SubPath::open(Point::new(c[0] + r, c[1]));
    sub.arc_to(Point::new(c[0] - r, c[1]), 1.0);
    sub.arc_to(Point::new(c[0] + r, c[1]), 1.0);
    sub.closed = true;
    Path {
        subpaths: vec![sub],
    }
}

/// What a panel's features do to it.
#[derive(Debug, Clone, Default)]
pub struct Applied {
    /// Rings cut from the outline.
    pub cuts: Vec<Vec<P2>>,
    /// Vectors cut into the panel.
    pub vectors: Vec<FeatureVector>,
    pub issues: Vec<Issue>,
}

/// The cuts and vectors of `panel`'s features. `free` says which of the
/// panel's sides (per flat index, low then high) no other panel touches,
/// so corners there may be rounded; `tool` is the cutter diameter, so a
/// hole it cannot cut is drilled instead.
pub fn apply(panel: &Panel, frame: &PanelFrame, free: [[bool; 2]; 2], tool: f64) -> Applied {
    let mut out = Applied::default();
    let ext = extents(panel, frame);
    let t = panel.thickness;
    for f in &panel.features {
        let fail = |m: String| Issue::on_panel(m, &panel.key);
        match f.kind.as_str() {
            "notch" => {
                let (Some((ke, he)), Some((ka, ha))) =
                    (edge_side(f.word("edge", "")), edge_side(f.word("at", "")))
                else {
                    out.issues.push(fail(format!(
                        "the notch on {} needs an edge and an end (at)",
                        panel.name
                    )));
                    continue;
                };
                let (Some(ie), Some(ia)) = (flat_index(frame, ke), flat_index(frame, ka)) else {
                    out.issues.push(fail(format!(
                        "the notch on {} names an edge it does not have",
                        panel.name
                    )));
                    continue;
                };
                if ie == ia {
                    out.issues.push(fail(format!(
                        "the notch on {} must be at an end of its edge",
                        panel.name
                    )));
                    continue;
                }
                let (w, h) = (f.num("w", 0.0), f.num("h", 0.0));
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let span = |hi: bool, size: f64, extent: f64| {
                    if hi {
                        (extent - size, extent + MARGIN)
                    } else {
                        (-MARGIN, size)
                    }
                };
                let mut lo = [0.0; 2];
                let mut hi = [0.0; 2];
                (lo[ia], hi[ia]) = span(ha, w, ext[ia]);
                (lo[ie], hi[ie]) = span(he, h, ext[ie]);
                out.cuts
                    .push(rect(panel, frame, lo[0], hi[0], lo[1], hi[1]));
            }
            "arch" => {
                let Some((ke, he)) = edge_side(f.word("edge", "bottom")) else {
                    continue;
                };
                let Some(ie) = flat_index(frame, ke) else {
                    out.issues.push(fail(format!(
                        "the arch on {} names an edge it does not have",
                        panel.name
                    )));
                    continue;
                };
                let ia = 1 - ie;
                let (rise, inset) = (f.num("rise", 0.0), f.num("inset", 0.0));
                let chord = ext[ia] - 2.0 * inset;
                if rise <= 0.0 || chord <= 0.0 {
                    continue;
                }
                // The arc's radius and middle from its chord and rise.
                let radius = (chord * chord / 4.0 + rise * rise) / (2.0 * rise);
                let edge = if he { ext[ie] } else { 0.0 };
                let inward = if he { -1.0 } else { 1.0 };
                let centre_e = edge + inward * (rise - radius);
                let mid = ext[ia] / 2.0;
                let half = (chord / 2.0 / radius).clamp(-1.0, 1.0).asin();
                let steps = 48;
                let mut ring = Vec::with_capacity(steps + 3);
                for k in 0..=steps {
                    let a = -half + 2.0 * half * k as f64 / steps as f64;
                    let along = mid + radius * a.sin();
                    let across = centre_e + inward * radius * a.cos();
                    let mut uv = [0.0; 2];
                    uv[ia] = along;
                    uv[ie] = across;
                    ring.push(frame.flat_uv(panel, uv[0], uv[1]));
                }
                for along in [mid + chord / 2.0, mid - chord / 2.0] {
                    let mut uv = [0.0; 2];
                    uv[ia] = along;
                    uv[ie] = edge - inward * MARGIN;
                    ring.push(frame.flat_uv(panel, uv[0], uv[1]));
                }
                out.cuts.push(ccw(ring));
            }
            "radius" => {
                let r = f.num("r", 0.0);
                if r <= 0.0 {
                    continue;
                }
                let r = r.min(ext[0].min(ext[1]) / 2.0);
                for (cu, hu) in [(0.0, false), (ext[0], true)] {
                    for (cv, hv) in [(0.0, false), (ext[1], true)] {
                        if !(free[0][usize::from(hu)] && free[1][usize::from(hv)]) {
                            continue;
                        }
                        // The corner square less the quarter disc.
                        let su = if hu { -1.0 } else { 1.0 };
                        let sv = if hv { -1.0 } else { 1.0 };
                        let (mu, mv) = (cu + su * r, cv + sv * r);
                        let mut ring =
                            vec![frame.flat_uv(panel, cu - su * MARGIN, cv - sv * MARGIN)];
                        ring.push(frame.flat_uv(panel, cu - su * MARGIN, mv));
                        let steps = 16;
                        for k in 0..=steps {
                            let a = std::f64::consts::FRAC_PI_2 * k as f64 / steps as f64;
                            ring.push(frame.flat_uv(
                                panel,
                                mu - su * r * a.cos(),
                                mv - sv * r * a.sin(),
                            ));
                        }
                        ring.push(frame.flat_uv(panel, mu, cv - sv * MARGIN));
                        out.cuts.push(ccw(ring));
                    }
                }
            }
            "cutout" | "handle" => {
                let (u, v) = (f.num("u", ext[0] / 2.0), f.num("v", ext[1] / 2.0));
                let (w, h) = (f.num("w", 0.0), f.num("h", 0.0));
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let r = if f.kind == "handle" {
                    w.min(h) / 2.0
                } else {
                    f.num("r", 0.0)
                };
                out.vectors.push(FeatureVector {
                    path: ring_path(&rounded_rect(panel, frame, u, v, w, h, r)),
                    role: FeatureRole::Hole,
                    name: if f.kind == "handle" {
                        "hand hold".into()
                    } else {
                        "cutout".into()
                    },
                });
            }
            "pocket" => {
                let (u, v) = (f.num("u", ext[0] / 2.0), f.num("v", ext[1] / 2.0));
                let (w, h) = (f.num("w", 0.0), f.num("h", 0.0));
                let depth = f.num("depth", t / 2.0);
                if w <= 0.0 || h <= 0.0 || depth <= 0.0 || depth >= t {
                    out.issues.push(fail(format!(
                        "the pocket on {} needs a size and a depth less than the panel's thickness",
                        panel.name
                    )));
                    continue;
                }
                out.vectors.push(FeatureVector {
                    path: ring_path(&rounded_rect(panel, frame, u, v, w, h, f.num("r", 0.0))),
                    role: FeatureRole::Pocket { depth },
                    name: "pocket".into(),
                });
            }
            "hole" => {
                let (u, v) = (f.num("u", ext[0] / 2.0), f.num("v", ext[1] / 2.0));
                let d = f.num("diameter", 0.0);
                if d <= 0.0 {
                    continue;
                }
                let depth = f.numbers.get("depth").copied().filter(|x| *x < t);
                let role = match depth {
                    _ if d < tool => FeatureRole::Drill {
                        depth: depth.unwrap_or(t),
                        diameter: d,
                    },
                    Some(depth) => FeatureRole::Pocket { depth },
                    None => FeatureRole::Hole,
                };
                out.vectors.push(FeatureVector {
                    path: circle(panel, frame, u, v, d / 2.0),
                    role,
                    name: format!("hole {} mm", round(d)),
                });
            }
            "hingeCups" => {
                let Some((ke, he)) = edge_side(f.word("edge", "left")) else {
                    continue;
                };
                let Some(ie) = flat_index(frame, ke) else {
                    out.issues.push(fail(format!(
                        "the hinge edge of {} is not one of its edges",
                        panel.name
                    )));
                    continue;
                };
                let ia = 1 - ie;
                let along = ext[ia];
                let count = f
                    .numbers
                    .get("count")
                    .map_or_else(|| hinge_count(along), |c| c.round().max(1.0) as u32);
                let from_end = f.num("end", CUP_FROM_END).min(along / 2.0);
                let cup = f.num("diameter", CUP_DIAMETER);
                let depth = f.num("depth", CUP_DEPTH).min(t - 3.0);
                let across = if he {
                    ext[ie] - f.num("inset", CUP_FROM_EDGE)
                } else {
                    f.num("inset", CUP_FROM_EDGE)
                };
                for k in 0..count {
                    let at = if count == 1 {
                        along / 2.0
                    } else {
                        from_end + (along - 2.0 * from_end) * f64::from(k) / f64::from(count - 1)
                    };
                    let mut uv = [0.0; 2];
                    uv[ia] = at;
                    uv[ie] = across;
                    out.vectors.push(FeatureVector {
                        path: circle(panel, frame, uv[0], uv[1], cup / 2.0),
                        role: FeatureRole::Pocket { depth },
                        name: format!("hinge cup {}", k + 1),
                    });
                }
            }
            "shelfPins" => {
                // Rows run up the panel (along Z, else its longer side),
                // `inset` in from both edges across it.
                let (ir, ia) = match flat_index(frame, 2) {
                    Some(i) => (i, 1 - i),
                    None if ext[0] >= ext[1] => (0, 1),
                    None => (1, 0),
                };
                let rows = f.num("rows", 2.0).round().max(1.0) as u32;
                let inset = f.num("inset", 37.0);
                let pitch = f.num("pitch", 32.0).max(1.0);
                let from = f.num("from", 200.0);
                let to = f.num("to", ext[ir] - 200.0);
                let d = f.num("diameter", PIN_DIAMETER);
                let depth = f.num("depth", PIN_DEPTH).min(t - 6.0).max(1.0);
                let lines: Vec<f64> = if rows == 1 {
                    vec![ext[ia] / 2.0]
                } else {
                    (0..rows)
                        .map(|k| {
                            inset + (ext[ia] - 2.0 * inset) * f64::from(k) / f64::from(rows - 1)
                        })
                        .collect()
                };
                let mut at = from;
                let mut n = 0;
                while at <= to + 1e-9 && n < 400 {
                    for line in &lines {
                        let mut uv = [0.0; 2];
                        uv[ir] = at;
                        uv[ia] = *line;
                        out.vectors.push(FeatureVector {
                            path: circle(panel, frame, uv[0], uv[1], d / 2.0),
                            role: FeatureRole::Drill { depth, diameter: d },
                            name: "shelf pin".into(),
                        });
                    }
                    at += pitch;
                    n += 1;
                }
            }
            "slideHoles" => {
                // A row at height `at` from the bottom, from 37 mm behind
                // the front edge every 96 mm.
                let (Some(iz), Some(iy)) = (flat_index(frame, 2), flat_index(frame, 1)) else {
                    out.issues.push(fail(format!(
                        "slide holes go on a panel standing front to back, not {}",
                        panel.name
                    )));
                    continue;
                };
                let height = f.num("at", ext[iz] / 2.0);
                let first = f.num("from", 37.0);
                let pitch = f.num("pitch", 96.0).max(1.0);
                let d = f.num("diameter", SLIDE_PILOT);
                let depth = f.num("depth", 8.0).min(t - 3.0).max(1.0);
                let mut along = first;
                while along <= ext[iy] - 20.0 + 1e-9 {
                    let mut uv = [0.0; 2];
                    uv[iz] = height;
                    uv[iy] = along;
                    out.vectors.push(FeatureVector {
                        path: circle(panel, frame, uv[0], uv[1], d / 2.0),
                        role: FeatureRole::Drill { depth, diameter: d },
                        name: "slide screw".into(),
                    });
                    along += pitch;
                }
            }
            other => out.issues.push(fail(format!(
                "{} has a feature this version does not know ({other}); it is left out",
                panel.name
            ))),
        }
    }
    out
}

/// `outline` less the `cuts`.
pub fn cut_outline(outline: &Path, cuts: &[Vec<P2>]) -> Path {
    if cuts.is_empty() {
        return outline.clone();
    }
    let cut_paths: Vec<Path> = cuts.iter().map(|r| ring_path(r)).collect();
    match Tolerance::new(0.001)
        .and_then(|tol| crate::geom::subtract(std::slice::from_ref(outline), &cut_paths, tol))
    {
        Ok(mut left) if !left.is_empty() => {
            // The biggest piece left is the panel.
            left.sort_by(|a, b| {
                let area =
                    |p: &Path| crate::geom::path_bbox(p).map_or(0.0, |b| b.width() * b.height());
                area(b).total_cmp(&area(a))
            });
            left.swap_remove(0)
        }
        _ => outline.clone(),
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::eval::{evaluate_template, Panel};
    use crate::furniture::template::builtin_template;
    use crate::joints::geometry::path_rings;
    use std::collections::BTreeMap;

    fn feature(kind: &str, numbers: &[(&str, f64)], words: &[(&str, &str)]) -> Feature {
        Feature {
            kind: kind.into(),
            numbers: numbers
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect(),
            words: words
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    /// A side panel standing on X: 18 thick, 300 deep (Y), 600 tall (Z).
    fn side(features: Vec<Feature>) -> (Panel, PanelFrame) {
        let t = builtin_template("bookcase").expect("bookcase");
        let e = evaluate_template(&t, &BTreeMap::new()).expect("works out");
        let mut p = e.panels[0].clone();
        p.min = [0.0, 0.0, 0.0];
        p.max = [18.0, 300.0, 600.0];
        p.features = features;
        let f = PanelFrame::of(&p, 1.0);
        (p, f)
    }

    fn uv_box(p: &Panel, f: &PanelFrame, ring: &[P2]) -> ([f64; 2], [f64; 2]) {
        // Back to panel coordinates through furniture space.
        let pose = f.pose();
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for q in ring {
            let w = pose.apply([q[0], q[1], 0.0]);
            let uv = [w[f.u] - p.min[f.u], w[f.v] - p.min[f.v]];
            for k in 0..2 {
                lo[k] = lo[k].min(uv[k]);
                hi[k] = hi[k].max(uv[k]);
            }
        }
        (lo, hi)
    }

    #[test]
    fn a_notch_takes_the_corner_off_the_named_edge_end() {
        let (p, f) = side(vec![feature(
            "notch",
            &[("w", 50.0), ("h", 80.0)],
            &[("edge", "bottom"), ("at", "front")],
        )]);
        let a = apply(&p, &f, [[true; 2]; 2], 6.35);
        assert_eq!(a.cuts.len(), 1);
        let rect = ring_path(&crate::furniture::layout::flat_rect(&p, &f, &[[0.0; 2]; 3]));
        let left = cut_outline(&rect, &a.cuts);
        let rings = path_rings(&left);
        // The side's u is Y (depth), v is Z (height): 50 off the front, 80
        // up from the floor; the rest of the outline stays.
        let (lo, hi) = uv_box(&p, &f, &rings[0]);
        assert_eq!((lo, hi), ([0.0, 0.0], [300.0, 600.0]));
        let area: f64 = crate::assembly::triangulate::signed_area2(&rings[0]).abs() / 2.0;
        assert!(
            (area - (300.0 * 600.0 - 50.0 * 80.0)).abs() < 1e-6,
            "{area}"
        );
    }

    #[test]
    fn hinge_cups_sit_off_the_hinge_edge_and_in_from_the_ends() {
        let (p, f) = side(vec![feature("hingeCups", &[], &[("edge", "front")])]);
        let a = apply(&p, &f, [[true; 2]; 2], 6.35);
        // 600 tall: two cups, 80 from the ends, 22.5 from the front edge.
        assert_eq!(a.vectors.len(), 2);
        for (k, v) in a.vectors.iter().enumerate() {
            assert_eq!(v.role, FeatureRole::Pocket { depth: 12.5 });
            let (lo, hi) = uv_box(&p, &f, &path_rings(&v.path)[0]);
            let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
            let want = [22.5, if k == 0 { 80.0 } else { 520.0 }];
            assert!(
                (mid[0] - want[0]).abs() < 1e-6 && (mid[1] - want[1]).abs() < 1e-6,
                "{mid:?}"
            );
            assert!((hi[0] - lo[0] - 35.0).abs() < 0.05);
        }
        assert_eq!(hinge_count(900.0), 2);
        assert_eq!(hinge_count(900.01), 3);
        assert_eq!(hinge_count(2000.0), 4);
        assert_eq!(hinge_count(2000.01), 5);
    }

    #[test]
    fn shelf_pins_drill_two_rows_up_the_side() {
        let (p, f) = side(vec![feature(
            "shelfPins",
            &[("from", 200.0), ("to", 264.0)],
            &[],
        )]);
        let a = apply(&p, &f, [[true; 2]; 2], 6.35);
        // 200, 232, 264 up; rows 37 in from the front and back.
        assert_eq!(a.vectors.len(), 6);
        let (lo, hi) = uv_box(&p, &f, &path_rings(&a.vectors[1].path)[0]);
        assert!(((lo[0] + hi[0]) / 2.0 - 263.0).abs() < 1e-6);
        assert!(((lo[1] + hi[1]) / 2.0 - 200.0).abs() < 1e-6);
        assert_eq!(
            a.vectors[0].role,
            FeatureRole::Drill {
                depth: 10.0,
                diameter: 5.0
            }
        );
    }

    #[test]
    fn a_hole_smaller_than_the_tool_is_drilled_and_a_deep_one_pocketed() {
        let (p, f) = side(vec![
            feature(
                "hole",
                &[("u", 100.0), ("v", 100.0), ("diameter", 5.0)],
                &[],
            ),
            feature(
                "hole",
                &[("u", 100.0), ("v", 200.0), ("diameter", 60.0)],
                &[],
            ),
            feature(
                "hole",
                &[
                    ("u", 100.0),
                    ("v", 300.0),
                    ("diameter", 20.0),
                    ("depth", 6.0),
                ],
                &[],
            ),
        ]);
        let a = apply(&p, &f, [[true; 2]; 2], 6.35);
        let roles: Vec<FeatureRole> = a.vectors.iter().map(|v| v.role).collect();
        assert_eq!(
            roles,
            vec![
                FeatureRole::Drill {
                    depth: 18.0,
                    diameter: 5.0
                },
                FeatureRole::Hole,
                FeatureRole::Pocket { depth: 6.0 }
            ]
        );
    }

    #[test]
    fn rounded_corners_skip_sides_another_panel_touches() {
        let (p, f) = side(vec![feature("radius", &[("r", 20.0)], &[])]);
        // The low u side (front) is joined: only the two back corners round.
        let a = apply(&p, &f, [[false, true], [true, true]], 6.35);
        assert_eq!(a.cuts.len(), 2);
        let rect = ring_path(&crate::furniture::layout::flat_rect(&p, &f, &[[0.0; 2]; 3]));
        let left = cut_outline(&rect, &a.cuts);
        let area = crate::assembly::triangulate::signed_area2(&path_rings(&left)[0]).abs() / 2.0;
        // Each corner loses its square less the quarter circle, drawn in
        // 16 straight steps (16 triangles r^2 sin(pi/32) / 2).
        let quarter = 16.0 * 20.0 * 20.0 * (std::f64::consts::PI / 32.0).sin() / 2.0;
        let corner = 20.0 * 20.0 - quarter;
        assert!(
            (area - (300.0 * 600.0 - 2.0 * corner)).abs() < 1e-4,
            "{area}"
        );
    }

    #[test]
    fn an_arch_lifts_the_edge_between_its_insets() {
        let (p, f) = side(vec![feature(
            "arch",
            &[("rise", 60.0), ("inset", 50.0)],
            &[("edge", "bottom")],
        )]);
        let a = apply(&p, &f, [[true; 2]; 2], 6.35);
        let rect = ring_path(&crate::furniture::layout::flat_rect(&p, &f, &[[0.0; 2]; 3]));
        let left = cut_outline(&rect, &a.cuts);
        let rings = path_rings(&left);
        // The middle of the bottom edge is 60 up; the ends still reach the
        // floor for the 50 mm feet.
        let (lo, hi) = uv_box(&p, &f, &rings[0]);
        assert_eq!((lo, hi), ([0.0, 0.0], [300.0, 600.0]));
        let pose = f.pose();
        let lowest_mid = rings[0]
            .iter()
            .map(|q| pose.apply([q[0], q[1], 0.0]))
            .filter(|w| (w[1] - 150.0).abs() < 3.0)
            .map(|w| w[2])
            .fold(f64::INFINITY, f64::min);
        assert!((lowest_mid - 60.0).abs() < 0.5, "{lowest_mid}");
    }

    #[test]
    fn faces_name_the_inside_outside_or_an_axis() {
        let (p, _) = side(vec![]);
        let middle = [400.0, 150.0, 300.0];
        assert_eq!(face_direction(&p, "inside", middle), Some(1.0));
        assert_eq!(face_direction(&p, "outside", middle), Some(-1.0));
        assert_eq!(face_direction(&p, "left", middle), Some(-1.0));
        assert_eq!(face_direction(&p, "front", middle), None);
    }
}
