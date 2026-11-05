//! What a design takes to build: the cut list (panels grouped by size,
//! with their edge banding), the hardware (from its joints, features and
//! template) and an estimate (sheets, material used, cutting time).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::doc::{Document, EntityKind};
use crate::geom::path_bbox;
use crate::joints::{JointKind, JointParams};

use super::design::FurnitureDesign;
use super::eval::TOL;
use super::generate::{Generated, Placement};
use super::layout;
use super::template::GrainDef;

fn round(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn grain_word(g: GrainDef) -> &'static str {
    match g {
        GrainDef::Long => "long",
        GrainDef::Short => "short",
        GrainDef::None => "none",
    }
}

/// The furniture direction word of side `side` (0 low, 1 high) of axis `k`.
fn side_word(k: usize, side: usize) -> &'static str {
    match (k, side) {
        (0, 0) => "left",
        (0, _) => "right",
        (1, 0) => "front",
        (1, _) => "back",
        (_, 0) => "bottom",
        _ => "top",
    }
}

/// The cut list: `{ rows: [{ names, panels, quantity, length, width,
/// thickness, material, materialLabel, grain, banding, bandingLength }],
/// panels }`. Length and width are the finished outline's sizes (length
/// the longer), millimetres; banding names the edges to band (by default
/// the free front edges, and every free edge of a door or front), and
/// bandingLength their total per panel.
pub fn cut_list(doc: &Document, design: &FurnitureDesign, gen: &Generated) -> Value {
    let panels = &gen.eval.panels;
    let (lo, _) = layout::bounds(panels);
    // (material, length, width, grain, banding) -> the group.
    type Key = (String, i64, i64, &'static str, Vec<&'static str>);
    type Group = (Vec<String>, Vec<String>, f64, f64, f64);
    let mut groups: BTreeMap<Key, Group> = BTreeMap::new();
    for (i, panel) in panels.iter().enumerate() {
        let frame = &gen.frames[i];
        let (mut w, mut h) = (panel.size(frame.u), panel.size(frame.v));
        if let Some(made) = design.made.get(&panel.key) {
            if let Ok(EntityKind::Path { path }) = doc.entity(made.vector).map(|e| &e.kind) {
                let place = Placement {
                    turn: made.place[0],
                    shift: [made.place[1], made.place[2]],
                };
                let back = place.matrix().invert().ok();
                if let Some(b) =
                    back.and_then(|m| path_bbox(&crate::geom::transform_path(path, &m)))
                {
                    w = b.width();
                    h = b.height();
                }
            }
        }
        let free = layout::free_sides(panels, i, frame);
        let front_panel = panel.normal == 1 && panel.min[1] <= lo[1] + TOL;
        let mut banding = Vec::new();
        let mut band_length = 0.0;
        for (fi, k) in [frame.u, frame.v].into_iter().enumerate() {
            for (side, open) in free[fi].iter().enumerate() {
                if !open {
                    continue;
                }
                let facing_front = k == 1 && side == 0;
                if facing_front || front_panel {
                    banding.push(side_word(k, side));
                    band_length += if fi == 0 { h } else { w };
                }
            }
        }
        banding.sort_unstable();
        let (length, width) = if w >= h { (w, h) } else { (h, w) };
        let key = (
            panel.material.clone(),
            (length * 10.0).round() as i64,
            (width * 10.0).round() as i64,
            grain_word(panel.grain),
            banding,
        );
        let entry = groups
            .entry(key)
            .or_insert_with(|| (Vec::new(), Vec::new(), 0.0, length, width));
        entry.0.push(panel.name.clone());
        entry.1.push(panel.key.clone());
        entry.2 = band_length;
    }
    let rows: Vec<Value> = groups
        .into_iter()
        .map(
            |((material, _, _, grain, banding), (names, keys, band, length, width))| {
                let m = gen.eval.materials.get(&material);
                json!({
                    "names": names,
                    "panels": keys,
                    "quantity": names.len(),
                    "length": round(length),
                    "width": round(width),
                    "thickness": m.map_or(0.0, |m| m.thickness),
                    "material": material,
                    "materialLabel": m.map_or_else(String::new, |m| m.label.clone()),
                    "grain": grain,
                    "banding": banding,
                    "bandingLength": round(band),
                })
            },
        )
        .collect();
    json!({ "rows": rows, "panels": panels.len() })
}

/// The hardware row of a made joint, if it takes any: (name, count, note).
fn joint_hardware(kind: JointKind, p: &JointParams) -> Option<(String, f64, String)> {
    let count = f64::from(p.count.unwrap_or(1));
    let mm = |v: Option<f64>| round(v.unwrap_or(0.0));
    Some(match kind {
        JointKind::Dowel => (
            format!("Dowel {} x {} mm", mm(p.diameter), mm(p.length)),
            count,
            "glued; drill the edge holes at the marks".into(),
        ),
        JointKind::CaptiveNut => (
            format!("Bolt {} mm with square nut", mm(p.diameter)),
            count,
            "knock-down".into(),
        ),
        JointKind::CamLock => (
            format!("Cam lock fitting {} mm", mm(p.diameter)),
            count,
            "cam, bolt and its insert; drill the 8 mm edge bores at the marks".into(),
        ),
        JointKind::Screws => (
            format!("Wood screw {} mm", mm(p.diameter)),
            count,
            "countersunk; drill at the marks".into(),
        ),
        _ => return None,
    })
}

/// The hardware: `{ rows: [{ name, count, note }] }` from the design's
/// joints, its hinge cups and its template's rows, the same name added up.
pub fn hardware(doc: &Document, design: &FurnitureDesign, gen: &Generated) -> Value {
    let mut rows: Vec<(String, f64, String)> = Vec::new();
    let mut add =
        |name: String, count: f64, note: String| match rows.iter_mut().find(|r| r.0 == name) {
            Some(row) => row.1 += count,
            None => rows.push((name, count, note)),
        };
    let mine: Vec<u64> = design.joints.values().copied().collect();
    for joint in doc
        .assembly()
        .joint_set
        .joints
        .iter()
        .filter(|j| mine.contains(&j.id))
    {
        if let Some((name, count, note)) = joint_hardware(joint.kind, &joint.resolved) {
            add(name, count, note);
        }
    }
    let cups: usize = gen
        .features
        .iter()
        .flatten()
        .filter(|f| f.name.starts_with("hinge cup"))
        .count();
    let template_has_hinges = gen
        .eval
        .hardware
        .iter()
        .any(|h| h.name.to_lowercase().contains("hinge"));
    if cups > 0 && !template_has_hinges {
        add(
            "Concealed hinge 35 mm".into(),
            cups as f64,
            "one per hinge cup".into(),
        );
    }
    for h in &gen.eval.hardware {
        add(h.name.clone(), h.count, h.note.clone());
    }
    let rows: Vec<Value> = rows
        .into_iter()
        .filter(|r| r.1 > 0.0)
        .map(|(name, count, note)| json!({ "name": name, "count": count, "note": note }))
        .collect();
    json!({ "rows": rows })
}

/// The estimate: `{ sheets: [{ material, label, thickness, count, area,
/// used, waste }], seconds, toolpaths }`: sheets per material with their
/// area and the panels' area on them (square millimetres) and the share
/// wasted, and the cutting time of the design's toolpaths in seconds.
pub fn estimate(doc: &Document, design: &FurnitureDesign, gen: &Generated) -> Value {
    let job = doc.job_setup();
    let sheet_area = job.width * job.height;
    let mut sheets = Vec::new();
    for (key, m) in &gen.eval.materials {
        let count = design.sheets.iter().filter(|s| s.material == *key).count();
        let used: f64 = gen
            .eval
            .panels
            .iter()
            .filter(|p| p.material == *key)
            .filter_map(|p| design.made.get(&p.key))
            .filter_map(|made| match doc.entity(made.vector).map(|e| &e.kind) {
                Ok(EntityKind::Path { path }) => Some(
                    crate::joints::geometry::path_rings(path)
                        .iter()
                        .map(|r| crate::assembly::triangulate::signed_area2(r) / 2.0)
                        .sum::<f64>()
                        .abs(),
                ),
                _ => None,
            })
            .sum();
        let area = sheet_area * count as f64;
        sheets.push(json!({
            "material": key,
            "label": m.label,
            "thickness": m.thickness,
            "count": count,
            "area": round(area),
            "used": round(used),
            "waste": if area > 0.0 { ((1.0 - used / area) * 1000.0).round() / 1000.0 } else { 0.0 },
        }));
    }
    let toolpaths: Vec<u64> = design.toolpaths.values().copied().collect();
    let seconds: f64 = toolpaths
        .iter()
        .filter_map(|id| doc.toolpath(*id).ok())
        .map(|t| t.estimated_seconds)
        .sum();
    json!({ "sheets": sheets, "seconds": seconds.round(), "toolpaths": toolpaths.len() })
}
