//! A furniture design in the document and everything it makes there:
//! sheets per material with the panels nested on them, the panel vectors
//! on a layer named after the design, the joints, the toolpaths that cut
//! them and the Assembly View pieces placed where the panels go.
//!
//! Regenerating reuses the ids of the vectors, joints, toolpaths and
//! sheets of panels and connections that still exist, so mates, notes and
//! edits on other objects survive a change.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::assembly::pieces::{Anchor, Piece};
use crate::assembly::pose::Pose;
use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::nest::{nest, NestParams, NestPart};
use crate::geom::{path_bbox, transform_path, Path, Tolerance};
use crate::joints::geometry::{inside_rings, path_rings, P2};
use crate::joints::{Joint, JointSet, Role};

use super::connect::{JointChoice, Style};
use super::eval::{Issue, Panel};
use super::features::FeatureRole;
use super::generate::{generate, joint_of, Generated, Inputs, Joined, Placement};
use super::layout;
use super::template::{GrainDef, Template};

/// What a design made for one panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Made {
    /// The panel's outline vector.
    pub vector: u64,
    /// Its sheet.
    pub sheet: u64,
    /// Its part number on the sheet, for the Assembly View.
    pub piece: u32,
    /// The joints it takes part in.
    pub joints: Vec<u64>,
    /// Where it was placed: turn (radians) and shift from flat coordinates
    /// to the sheet's job coordinates.
    #[serde(default)]
    pub place: [f64; 3],
    /// The vectors of its features (holes, pockets, drilled holes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<u64>,
    /// The outline's first two points as last made (x0, y0, x1, y1), to
    /// tell a move by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<[f64; 4]>,
    /// Moved on its sheet by hand: it keeps that place when the design is
    /// made again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
}

/// The first two points of a vector's outline.
fn first_points(doc: &Document, id: u64) -> Option<[f64; 4]> {
    let EntityKind::Path { path } = &doc.entity(id).ok()?.kind else {
        return None;
    };
    let sub = path.subpaths.first()?;
    let end = sub.segments.first()?.end();
    Some([sub.start.x, sub.start.y, end.x, end.y])
}

/// Where a panel the maker moved (or turned) on its sheet by hand now is,
/// read from its outline's first two points against those last made;
/// `None` when it has not moved or its outline was changed some other way.
fn hand_place(doc: &Document, made: &Made) -> Option<Placement> {
    let p = made.mark?;
    let c = first_points(doc, made.vector)?;
    if (0..4).all(|k| (c[k] - p[k]).abs() < 1e-6) {
        return None;
    }
    let (pd, cd) = ([p[2] - p[0], p[3] - p[1]], [c[2] - c[0], c[3] - c[1]]);
    let (lp, lc) = (pd[0].hypot(pd[1]), cd[0].hypot(cd[1]));
    if lp < 1e-6 || (lp - lc).abs() > 1e-4 {
        return None;
    }
    let a = cd[1].atan2(cd[0]) - pd[1].atan2(pd[0]);
    let (s, co) = a.sin_cos();
    let d = [made.place[1] - p[0], made.place[2] - p[1]];
    Some(Placement {
        turn: made.place[0] + a,
        shift: [co * d[0] - s * d[1] + c[0], s * d[0] + co * d[1] + c[1]],
    })
}

/// A sheet a design made: the material and its number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignSheet {
    pub material: String,
    pub index: u32,
    pub id: u64,
}

/// A furniture design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FurnitureDesign {
    pub id: u64,
    pub name: String,
    /// A full copy of the template.
    pub template: Template,
    /// The values, parameter by parameter.
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub style: Style,
    /// Joints picked per class.
    #[serde(default)]
    pub class_joints: BTreeMap<String, JointChoice>,
    /// Joints picked per connection.
    #[serde(default)]
    pub connection_joints: BTreeMap<String, JointChoice>,
    /// What was made, by panel key.
    #[serde(default)]
    pub made: BTreeMap<String, Made>,
    /// The sheets made.
    #[serde(default)]
    pub sheets: Vec<DesignSheet>,
    /// The layer of the panel vectors.
    #[serde(default)]
    pub layer: u64,
    /// Joint ids by connection key.
    #[serde(default)]
    pub joints: BTreeMap<String, u64>,
    /// Toolpath ids by `<sheet>|<name>`.
    #[serde(default)]
    pub toolpaths: BTreeMap<String, u64>,
    /// Where the design's furniture space origin sits in the Assembly View.
    #[serde(default)]
    pub origin: [f64; 3],
    /// Models and vectors on its panels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<super::attach::Attachment>,
    /// The carving toolpaths Make Carving Toolpaths made, by sheet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub carving: BTreeMap<u64, Vec<u64>>,
    /// What it made on the job's other side (a double sided job): joint
    /// pockets in faces turned away from their other panels.
    #[serde(default, skip_serializing_if = "BackCuts::is_empty")]
    pub back: BackCuts,
}

/// Vectors and toolpaths a design made on the job's other side.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackCuts {
    pub vectors: Vec<u64>,
    pub toolpaths: Vec<u64>,
}

impl BackCuts {
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty() && self.toolpaths.is_empty()
    }
}

/// Deletes what `back` made on the job's other side.
fn clear_back(doc: &mut Document, back: &BackCuts) -> Result<(), CoreError> {
    if back.is_empty() || !doc.job_setup().is_double_sided() {
        return Ok(());
    }
    let here = doc.active_side();
    doc.set_active_side(here.other())?;
    let toolpaths: Vec<u64> = back
        .toolpaths
        .iter()
        .copied()
        .filter(|id| doc.toolpath(*id).is_ok())
        .collect();
    if !toolpaths.is_empty() {
        doc.delete_toolpaths(&toolpaths)?;
    }
    let vectors: Vec<u64> = back
        .vectors
        .iter()
        .copied()
        .filter(|id| doc.entity(*id).is_ok())
        .collect();
    doc.delete_entities(&vectors);
    doc.set_active_side(here)
}

/// A pocket to cut from the job's other side: this side's sheet, the
/// toolpath's name and settings, and the vectors' paths as on this side.
struct BackCut {
    sheet: u64,
    name: String,
    params: Value,
    paths: Vec<Path>,
}

/// Makes the pockets of `cuts` on the job's other side: each path flipped
/// to where it lies seen from there, on the sheet named as its own, with a
/// pocket toolpath made with that sheet on show. Answers what it made.
fn make_back(
    doc: &mut Document,
    design_name: &str,
    cuts: Vec<BackCut>,
    tool: &crate::cam::tool::Tool,
    issues: &mut Vec<Issue>,
) -> Result<BackCuts, CoreError> {
    let mut made = BackCuts::default();
    if cuts.is_empty() {
        return Ok(made);
    }
    let job = *doc.job();
    let flip = {
        let (e, f) = job.flip_xy(0.0, 0.0);
        let (ax, ay) = job.flip_xy(1.0, 0.0);
        let (cx, cy) = job.flip_xy(0.0, 1.0);
        crate::geom::Mat3::new(ax - e, ay - f, cx - e, cy - f, e, f)
    };
    let mut named = Vec::with_capacity(cuts.len());
    for cut in cuts {
        let sheet_name = doc.sheet(cut.sheet)?.name.clone();
        let material = doc.sheet_material(cut.sheet)?;
        named.push((sheet_name, material, cut));
    }
    let here = doc.active_side();
    doc.set_active_side(here.other())?;
    let showing = doc.active_sheet();
    let layer = match doc.layers().iter().find(|l| l.name == design_name) {
        Some(l) => l.id,
        None => doc.add_layer(Some(design_name.to_string()), None),
    };
    for (sheet_name, material, cut) in named {
        let sheet = super::attach::sheet_across(doc, &sheet_name, material)?;
        let mut ids = Vec::with_capacity(cut.paths.len());
        for path in &cut.paths {
            let id = doc.insert_entity(Entity {
                id: 0,
                layer,
                sheet,
                revision: 0,
                name: Some(format!("{} (other side)", cut.name)),
                parent: None,
                rotation: 0.0,
                kind: EntityKind::Path {
                    path: transform_path(path, &flip),
                },
            });
            ids.push(id);
        }
        made.vectors.extend(&ids);
        if doc.active_sheet() != sheet {
            doc.set_active_sheet(sheet)?;
        }
        let name = format!("{} (other side)", cut.name);
        match crate::joints::commands::job_toolpath(
            doc,
            sheet,
            name.clone(),
            crate::cam::compute::Strategy::Pocket,
            cut.params,
            ids,
            tool,
        ) {
            Ok(toolpath) => made.toolpaths.push(doc.add_toolpath(toolpath)),
            Err(error) => issues.push(Issue::new(format!(
                "the toolpath {name} could not be made: {error}"
            ))),
        }
    }
    if doc.active_sheet() != showing && doc.sheet(showing).is_ok() {
        doc.set_active_sheet(showing)?;
    }
    doc.set_active_side(here)?;
    Ok(made)
}

impl FurnitureDesign {
    /// The inputs that generate it.
    pub fn inputs(&self) -> Inputs<'_> {
        Inputs {
            template: &self.template,
            values: &self.values,
            style: self.style,
            classes: &self.class_joints,
            picked: &self.connection_joints,
            faces: super::attach::faces(self),
            double_sided: false,
        }
    }

    /// The inputs as `doc`'s job makes them (one sided or both).
    pub fn inputs_in(&self, doc: &Document) -> Inputs<'_> {
        Inputs {
            double_sided: doc.job_setup().is_double_sided(),
            ..self.inputs()
        }
    }
}

/// A design's summary, as every editing command answers it.
///
/// ```text
/// { id, name, template, style, panels, connections,
///   sheets: [{ material, label, thickness, count }],
///   size: [width, depth, height], issues: [{ message, panel?, connection?, suggest? }] }
/// ```
pub fn summary(gen: &Generated, design: &FurnitureDesign, issues: &[Issue]) -> Value {
    let (min, max) = layout::bounds(&gen.eval.panels);
    let sheets: Vec<Value> = gen
        .eval
        .materials
        .values()
        .map(|m| {
            let count = design.sheets.iter().filter(|s| s.material == m.key).count();
            json!({ "material": m.key, "label": m.label, "thickness": m.thickness, "count": count })
        })
        .collect();
    json!({
        "id": design.id,
        "name": design.name,
        "template": design.template.id,
        "style": design.style,
        "panels": gen.eval.panels.len(),
        "connections": gen.conns.len(),
        "sheets": sheets,
        "size": [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
        "issues": issues,
    })
}

/// An end mill of the joints' tool diameter, for the design's toolpaths.
pub fn end_mill(diameter: f64) -> crate::cam::tool::Tool {
    let mut tool = crate::cam::tool::Tool {
        name: format!("End Mill ({} mm)", (diameter * 100.0).round() / 100.0),
        tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
        ..crate::cam::tool::Tool::default()
    };
    tool.set_stepover(diameter * 0.4);
    tool
}

/// A twist drill of `diameter`, for the design's drilled holes.
pub fn drill_bit(diameter: f64) -> crate::cam::tool::Tool {
    crate::cam::tool::Tool {
        name: format!("Drill ({} mm)", (diameter * 100.0).round() / 100.0),
        tool_type: crate::cam::tool::ToolGeometry::Drill {
            diameter,
            point_angle: 118.0,
        },
        ..crate::cam::tool::Tool::default()
    }
}

/// Area and centroid of `outer` less `holes`.
fn area_centre(outer: &[P2], holes: &[Vec<P2>]) -> (f64, P2) {
    let ring_sum = |ring: &[P2]| {
        let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for k in 0..ring.len() {
            let (p, q) = (ring[k], ring[(k + 1) % ring.len()]);
            let cross = p[0] * q[1] - q[0] * p[1];
            a += cross;
            cx += (p[0] + q[0]) * cross;
            cy += (p[1] + q[1]) * cross;
        }
        (a / 2.0, cx / 6.0, cy / 6.0)
    };
    let (mut area, mut mx, mut my) = ring_sum(outer);
    if area < 0.0 {
        (area, mx, my) = (-area, -mx, -my);
    }
    for h in holes {
        let (mut a, mut x, mut y) = ring_sum(h);
        if a < 0.0 {
            (a, x, y) = (-a, -x, -y);
        }
        area -= a;
        mx -= x;
        my -= y;
    }
    if area.abs() < 1e-9 {
        return (0.0, outer.first().copied().unwrap_or([0.0, 0.0]));
    }
    (area, [mx / area, my / area])
}

/// The flat axis the panel's grain runs along (0 X, 1 Y), if any.
fn grain_axis(panel: &Panel, outline: &Path) -> Option<usize> {
    let b = path_bbox(outline)?;
    let (w, h) = (b.width(), b.height());
    match panel.grain {
        GrainDef::None => None,
        GrainDef::Long => Some(usize::from(h > w)),
        GrainDef::Short => Some(usize::from(h <= w)),
    }
}

/// Where each panel goes: (sheet index of its material, placement), the
/// panels of each material nested on sheets of the job's size.
fn nest_panels(
    doc: &Document,
    gen: &Generated,
    joined: &Joined,
    tool: f64,
    pins: &BTreeMap<usize, (u32, Placement)>,
    issues: &mut Vec<Issue>,
) -> Result<Vec<(u32, Placement)>, CoreError> {
    let job = doc.job_setup();
    let bounds = job.material_bounds();
    let mut out = vec![
        (
            0u32,
            Placement {
                turn: 0.0,
                shift: [0.0, 0.0]
            }
        );
        gen.eval.panels.len()
    ];
    let tol = Tolerance::new(0.05)?;
    for key in gen.eval.materials.keys() {
        let members: Vec<usize> = (0..gen.eval.panels.len())
            .filter(|i| gen.eval.panels[*i].material == *key && !pins.contains_key(i))
            .collect();
        let pinned: Vec<usize> = pins
            .keys()
            .copied()
            .filter(|i| gen.eval.panels[*i].material == *key)
            .collect();
        for i in &pinned {
            out[*i] = pins[i];
        }
        if members.is_empty() {
            continue;
        }
        let mut pre = BTreeMap::new();
        let mut parts = Vec::new();
        let mut grained = false;
        for i in &members {
            let outline = &joined.outlines[*i];
            let turn = match grain_axis(&gen.eval.panels[*i], outline) {
                Some(1) => {
                    grained = true;
                    std::f64::consts::FRAC_PI_2
                }
                Some(_) => {
                    grained = true;
                    0.0
                }
                None => 0.0,
            };
            pre.insert(*i, turn);
            let turned = Placement {
                turn,
                shift: [0.0, 0.0],
            };
            parts.push(NestPart {
                id: *i as u64,
                paths: vec![transform_path(outline, &turned.matrix())],
                quantity: 1,
                filler: false,
                no_mirror: true,
            });
        }
        let params = NestParams {
            tool_diameter: tool,
            gap: 2.0,
            border: 10.0,
            rotation_step_deg: if grained { 180.0 } else { 90.0 },
            sheet_width: job.width,
            sheet_height: job.height,
            ..NestParams::default()
        };
        let result = nest(&parts, &params, tol)?;
        let placed = |p: &crate::geom::nest::NestPlacement, first: u32| {
            let i = p.part_id as usize;
            (
                p.sheet as u32 + first,
                Placement {
                    turn: pre.get(&i).copied().unwrap_or(0.0) + p.rotation_deg.to_radians(),
                    shift: [
                        p.translation.x + bounds.min.x,
                        p.translation.y + bounds.min.y,
                    ],
                },
            )
        };
        for p in &result.placements {
            out[p.part_id as usize] = placed(p, 0);
        }
        // Panels moved by hand keep their place: a nested panel that would
        // lie over one goes on a sheet after the others.
        if !pinned.is_empty() {
            let spacing = tool + params.gap;
            let footprint = |i: usize, place: &Placement| {
                path_bbox(&transform_path(&joined.outlines[i], &place.matrix()))
            };
            let clash: Vec<usize> = result
                .placements
                .iter()
                .map(|p| p.part_id as usize)
                .filter(|i| {
                    let (sheet, place) = out[*i];
                    let Some(b) = footprint(*i, &place) else {
                        return false;
                    };
                    pinned.iter().any(|k| {
                        let (ks, kp) = out[*k];
                        ks == sheet
                            && footprint(*k, &kp).is_some_and(|o| {
                                b.min.x < o.max.x + spacing
                                    && o.min.x < b.max.x + spacing
                                    && b.min.y < o.max.y + spacing
                                    && o.min.y < b.max.y + spacing
                            })
                    })
                })
                .collect();
            if !clash.is_empty() {
                let first = members
                    .iter()
                    .chain(&pinned)
                    .filter(|i| !clash.contains(i))
                    .map(|i| out[*i].0 + 1)
                    .max()
                    .unwrap_or(0);
                let again: Vec<NestPart> = parts
                    .iter()
                    .filter(|p| clash.contains(&(p.id as usize)))
                    .cloned()
                    .collect();
                let moved = nest(&again, &params, tol)?;
                for p in &moved.placements {
                    let i = p.part_id as usize;
                    out[i] = placed(p, first);
                    issues.push(Issue::on_panel(
                        format!(
                            "{} goes on sheet {} to clear the panels moved by hand",
                            gen.eval.panels[i].name,
                            out[i].0 + 1
                        ),
                        &gen.eval.panels[i].key,
                    ));
                }
            }
        }
        let mut beside = bounds.max.x + 100.0;
        for u in &result.unplaced {
            let i = u.part_id as usize;
            let panel = &gen.eval.panels[i];
            issues.push(Issue::on_panel(
                format!(
                    "{} does not fit on a {} x {} mm sheet; it is drawn beside the first sheet",
                    panel.name, job.width, job.height
                ),
                &panel.key,
            ));
            let turn = pre.get(&i).copied().unwrap_or(0.0);
            let b = path_bbox(&transform_path(
                &joined.outlines[i],
                &Placement {
                    turn,
                    shift: [0.0, 0.0],
                }
                .matrix(),
            ));
            let (x0, y0, w) = b.map_or((0.0, 0.0, 0.0), |b| (b.min.x, b.min.y, b.width()));
            out[i] = (
                0,
                Placement {
                    turn,
                    shift: [beside - x0, bounds.min.y - y0],
                },
            );
            beside += w + 50.0;
        }
    }
    Ok(out)
}

/// Makes (or remakes) everything `design` makes in `doc`, inside the
/// caller's undo step, and stores the design in the assembly. Answers the
/// summary.
///
/// # Errors
/// [`CoreError::Validation`] when the template does not work out, and
/// what the document refuses.
/// Takes in what the maker did by hand since the design was last made,
/// against the design as it was made: panels moved or turned on their
/// sheets (they keep that place from now on) and models and vectors put on
/// the panels (attached). Call it before changing the design's values or
/// joints; [`regenerate`] calls it too, and a second call finds nothing new.
pub fn take_in(doc: &Document, design: &mut FurnitureDesign) {
    for m in design.made.values_mut() {
        if let Some(place) = hand_place(doc, m) {
            m.place = [place.turn, place.shift[0], place.shift[1]];
            if let Ok(e) = doc.entity(m.vector) {
                m.sheet = e.sheet;
            }
            m.pinned = true;
            // Taken in: the next look finds it where it now is.
            m.mark = first_points(doc, m.vector);
        }
    }
    if !design.made.is_empty() {
        let settings = &doc.assembly().joint_set.settings;
        if let Ok(old) = super::generate::lay_out(
            &design.inputs_in(doc),
            &BTreeMap::new(),
            settings.tool_diameter,
        ) {
            super::attach::capture(doc, design, &old);
        }
    }
}

pub fn regenerate(doc: &mut Document, design: &mut FurnitureDesign) -> Result<Value, CoreError> {
    let settings = doc.assembly().joint_set.settings.clone();
    take_in(doc, design);
    let (gen, joined) = generate(&design.inputs_in(doc), &settings)?;
    design.values = gen.eval.values.clone();
    let mut issues = gen.issues.clone();
    let panels = &gen.eval.panels;

    // The design's joints out of the set: its vectors go back to the
    // outlines they had before any joint.
    let mut set: JointSet = doc.assembly().joint_set.clone();
    let mine: BTreeSet<u64> = design.joints.values().copied().collect();
    let old: BTreeMap<u64, Joint> = set
        .joints
        .iter()
        .filter(|j| mine.contains(&j.id))
        .map(|j| (j.id, j.clone()))
        .collect();
    set.joints.retain(|j| !mine.contains(&j.id));
    crate::joints::rebuild_skipping(doc, &mut set)?;

    // Sheets, one set per material, the panels nested on them.
    let mut pins: BTreeMap<usize, (u32, Placement)> = BTreeMap::new();
    let bounds = doc.job_setup().material_bounds();
    for (i, panel) in panels.iter().enumerate() {
        let Some(m) = design.made.get(&panel.key).filter(|m| m.pinned) else {
            continue;
        };
        let Some(index) = design
            .sheets
            .iter()
            .find(|s| s.id == m.sheet && s.material == panel.material)
            .map(|s| s.index)
        else {
            continue;
        };
        let place = Placement {
            turn: m.place[0],
            shift: [m.place[1], m.place[2]],
        };
        let on_sheet = path_bbox(&transform_path(&joined.outlines[i], &place.matrix()))
            .is_some_and(|b| {
                b.min.x >= bounds.min.x - 1e-6
                    && b.min.y >= bounds.min.y - 1e-6
                    && b.max.x <= bounds.max.x + 1e-6
                    && b.max.y <= bounds.max.y + 1e-6
            });
        if on_sheet {
            pins.insert(i, (index, place));
        } else {
            issues.push(Issue::on_panel(
                format!(
                    "{} no longer fits on its sheet where it was moved by hand; it is nested again",
                    panel.name
                ),
                &panel.key,
            ));
        }
    }
    let places = nest_panels(
        doc,
        &gen,
        &joined,
        settings.tool_diameter,
        &pins,
        &mut issues,
    )?;
    let template_material = doc.sheet_material(doc.active_sheet())?;
    let mut sheets: Vec<DesignSheet> = Vec::new();
    for (key, m) in &gen.eval.materials {
        let count = panels
            .iter()
            .zip(&places)
            .filter(|(p, _)| p.material == *key)
            .map(|(_, (s, _))| *s + 1)
            .max()
            .unwrap_or(0);
        for index in 0..count {
            let name = format!("{} {} {}", design.name, m.label, index + 1);
            let reuse = design
                .sheets
                .iter()
                .find(|s| s.material == *key && s.index == index && doc.sheet(s.id).is_ok())
                .map(|s| s.id);
            let id = match reuse {
                Some(id) => {
                    if doc.sheet(id)?.name != name {
                        doc.rename_sheet(id, name)?;
                    }
                    id
                }
                None => doc.add_sheet(Some(name)),
            };
            let mut material = template_material;
            material.thickness = m.thickness;
            if doc.sheet_material(id)? != material {
                doc.set_sheet_material(id, material)?;
            }
            sheets.push(DesignSheet {
                material: key.clone(),
                index,
                id,
            });
        }
    }
    let sheet_of = |i: usize| {
        sheets
            .iter()
            .find(|s| s.material == panels[i].material && s.index == places[i].0)
            .map_or(0, |s| s.id)
    };

    // The layer.
    let layer = if doc.layer(design.layer).is_ok() {
        if doc.layer(design.layer)?.name != design.name {
            doc.update_layer(design.layer, Some(design.name.clone()), None, None, None)?;
        }
        design.layer
    } else {
        doc.add_layer(Some(design.name.clone()), None)
    };

    // The panel vectors, as they are before the joints.
    let mut vectors = Vec::with_capacity(panels.len());
    for (i, panel) in panels.iter().enumerate() {
        let place = places[i].1;
        let entity = Entity {
            id: 0,
            layer,
            sheet: sheet_of(i),
            revision: 0,
            name: Some(panel.name.clone()),
            parent: None,
            rotation: 0.0,
            kind: EntityKind::Path {
                path: transform_path(&gen.outlines[i], &place.matrix()),
            },
        };
        let reuse = design
            .made
            .get(&panel.key)
            .map(|m| m.vector)
            .filter(|id| doc.entity(*id).is_ok());
        let id = match reuse {
            Some(id) => {
                doc.replace_entity(id, entity)?;
                id
            }
            None => doc.insert_entity(entity),
        };
        vectors.push(id);
    }
    // Feature vectors, reusing the ids each panel's had.
    let mut feature_ids: Vec<Vec<(u64, FeatureRole)>> = Vec::with_capacity(panels.len());
    let mut dropped: Vec<u64> = Vec::new();
    for (i, panel) in panels.iter().enumerate() {
        let place = places[i].1;
        let old_ids = design
            .made
            .get(&panel.key)
            .map(|m| m.features.clone())
            .unwrap_or_default();
        let mut ids = Vec::new();
        for (k, f) in gen.features[i].iter().enumerate() {
            let entity = Entity {
                id: 0,
                layer,
                sheet: sheet_of(i),
                revision: 0,
                name: Some(format!("{} {}", panel.name, f.name)),
                parent: None,
                rotation: 0.0,
                kind: EntityKind::Path {
                    path: transform_path(&f.path, &place.matrix()),
                },
            };
            let id = match old_ids.get(k).copied().filter(|id| doc.entity(*id).is_ok()) {
                Some(id) => {
                    doc.replace_entity(id, entity)?;
                    id
                }
                None => doc.insert_entity(entity),
            };
            ids.push((id, f.role));
        }
        dropped.extend(old_ids.iter().skip(gen.features[i].len()));
        feature_ids.push(ids);
    }
    // Vectors of panels that are gone.
    let gone: Vec<u64> = design
        .made
        .iter()
        .filter(|(key, _)| !panels.iter().any(|p| p.key == **key))
        .flat_map(|(_, m)| std::iter::once(m.vector).chain(m.features.iter().copied()))
        .chain(dropped)
        .filter(|id| doc.entity(*id).is_ok())
        .collect();
    doc.delete_entities(&gen_unique(gone));

    // The joints, reusing the ids (and so the vectors) of connections
    // that still have one.
    let place_list: Vec<Placement> = places.iter().map(|(_, p)| *p).collect();
    let mut joint_ids: BTreeMap<String, u64> = BTreeMap::new();
    let mut matched: BTreeMap<u64, bool> = BTreeMap::new();
    for spec in &gen.specs {
        let key = &gen.conns[spec.conn].key;
        let reuse = design.joints.get(key).and_then(|id| old.get(id));
        let id = reuse.map_or_else(|| set.take_id(), |j| j.id);
        let mut joint = joint_of(spec, &vectors, &place_list, id);
        if let Some(previous) = reuse {
            joint.vectors = previous.vectors.clone();
        }
        set.joints.push(joint);
        joint_ids.insert(key.clone(), id);
        matched.insert(id, spec.matched);
    }
    for (id, joint) in &old {
        if !joint_ids.values().any(|j| j == id) {
            crate::joints::delete_made(doc, joint);
        }
    }
    let outcomes = crate::joints::rebuild_skipping(doc, &mut set)?;
    for (joint, outcome) in set.joints.iter_mut().zip(&outcomes) {
        joint.resolved = outcome.resolved.clone();
        let keep = matched.get(&joint.id).copied().unwrap_or(true);
        joint.b_to_a = if keep { outcome.b_to_a } else { None };
        if let Some(broken) = &outcome.broken {
            if matched.contains_key(&joint.id) {
                issues.push(Issue::new(format!(
                    "{} {} could not be made: {broken}",
                    joint.kind.title(),
                    joint.id
                )));
            }
        }
    }

    // Models and vectors on the panels go where their panels went.
    super::attach::follow(doc, design, &gen, &places, &sheet_of, &mut issues)?;

    // Toolpaths, one set per sheet.
    let tool = end_mill(settings.tool_diameter);
    let made_joints: Vec<&Joint> = set
        .joints
        .iter()
        .filter(|j| matched.contains_key(&j.id))
        .collect();
    let mut jobs = crate::joints::commands::jobs_for(
        doc,
        made_joints.iter().copied(),
        &vectors,
        &settings,
        &design.name,
    );
    // Pockets in faces turned away from the other panel (a double sided
    // job): cut from the job's other side instead.
    let mut back_ids: BTreeSet<u64> = BTreeSet::new();
    for key in &gen.back {
        let Some(joint) = joint_ids
            .get(key)
            .and_then(|id| set.joints.iter().find(|j| j.id == *id))
        else {
            continue;
        };
        let Some(spec) = gen.specs.iter().find(|s| gen.conns[s.conn].key == *key) else {
            continue;
        };
        let Some(b_box) = doc.bbox_of(vectors[spec.b]) else {
            continue;
        };
        let b_sheet = sheet_of(spec.b);
        for v in &joint.vectors {
            if !matches!(v.role, Role::Pocket { .. }) {
                continue;
            }
            let on_b = doc.entity(v.id).is_ok_and(|e| e.sheet == b_sheet)
                && doc.bbox_of(v.id).is_some_and(|b| {
                    let m = [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0];
                    m[0] >= b_box.min.x
                        && m[0] <= b_box.max.x
                        && m[1] >= b_box.min.y
                        && m[1] <= b_box.max.y
                });
            if on_b {
                back_ids.insert(v.id);
            }
        }
    }
    let mut back_cuts: Vec<BackCut> = Vec::new();
    if !back_ids.is_empty() {
        for ((sheet, _, name), (strategy, params, ids)) in jobs.iter_mut() {
            if *strategy != crate::cam::compute::Strategy::Pocket {
                continue;
            }
            let moved: Vec<u64> = ids
                .iter()
                .copied()
                .filter(|id| back_ids.contains(id))
                .collect();
            if moved.is_empty() {
                continue;
            }
            ids.retain(|id| !back_ids.contains(id));
            let paths: Vec<Path> = moved
                .iter()
                .filter_map(|id| match doc.entity(*id).map(|e| &e.kind) {
                    Ok(EntityKind::Path { path }) => Some(path.clone()),
                    _ => None,
                })
                .collect();
            back_cuts.push(BackCut {
                sheet: *sheet,
                name: name.clone(),
                params: params.clone(),
                paths,
            });
        }
        jobs.retain(|_, (_, _, ids)| !ids.is_empty());
    }
    let mut drills: BTreeMap<String, f64> = BTreeMap::new();
    for (i, ids) in feature_ids.iter().enumerate() {
        let sheet = sheet_of(i);
        let through = panels[i].thickness;
        for (id, role) in ids {
            use crate::cam::compute::Strategy;
            let (order, name, strategy, params) = match role {
                FeatureRole::Pocket { depth } => (
                    1,
                    format!(
                        "{} pockets {} mm",
                        design.name,
                        (depth * 100.0).round() / 100.0
                    ),
                    Strategy::Pocket,
                    json!({ "cutDepth": depth }),
                ),
                FeatureRole::Hole => (
                    2,
                    format!("{} holes", design.name),
                    Strategy::Profile,
                    json!({ "side": "inside", "cutDepth": through }),
                ),
                FeatureRole::Drill { depth, diameter } => {
                    let name = format!(
                        "{} drill {} mm, {} mm deep",
                        design.name,
                        (diameter * 100.0).round() / 100.0,
                        (depth * 100.0).round() / 100.0
                    );
                    drills.insert(name.clone(), *diameter);
                    (0, name, Strategy::Drill, json!({ "cutDepth": depth }))
                }
            };
            jobs.entry((sheet, order, name))
                .or_insert_with(|| (strategy, params, Vec::new()))
                .2
                .push(*id);
        }
    }
    let mut toolpaths: BTreeMap<String, u64> = BTreeMap::new();
    // Each toolpath is made with its sheet on show: the job setup is the
    // sheet on show's material, and the depths and the through cut follow
    // it.
    let showing = doc.active_sheet();
    for (sheet, name, strategy, params, ids) in apart(doc, jobs) {
        if doc.active_sheet() != sheet && doc.sheet(sheet).is_ok() {
            doc.set_active_sheet(sheet)?;
        }
        let key = format!("{sheet}|{name}");
        let drill;
        let cutter = match drills.get(&name) {
            Some(d) => {
                drill = drill_bit(*d);
                &drill
            }
            None => &tool,
        };
        let toolpath = match crate::joints::commands::job_toolpath(
            doc,
            sheet,
            name.clone(),
            strategy,
            params,
            ids,
            cutter,
        ) {
            Ok(toolpath) => toolpath,
            Err(error) => {
                issues.push(Issue::new(format!(
                    "the toolpath {name} could not be made: {error}"
                )));
                continue;
            }
        };
        let id = match design
            .toolpaths
            .get(&key)
            .copied()
            .filter(|id| doc.toolpath(*id).is_ok())
        {
            Some(id) => {
                doc.replace_toolpath(id, toolpath)?;
                id
            }
            None => doc.add_toolpath(toolpath),
        };
        toolpaths.insert(key, id);
    }
    if doc.active_sheet() != showing && doc.sheet(showing).is_ok() {
        doc.set_active_sheet(showing)?;
    }
    let stale: Vec<u64> = design
        .toolpaths
        .values()
        .copied()
        .filter(|id| !toolpaths.values().any(|t| t == id) && doc.toolpath(*id).is_ok())
        .collect();
    if !stale.is_empty() {
        doc.delete_toolpaths(&stale)?;
    }
    clear_back(doc, &design.back)?;
    design.back = make_back(doc, &design.name, back_cuts, &tool, &mut issues)?;

    // Sheets no longer used.
    for s in &design.sheets {
        if !sheets.iter().any(|n| n.id == s.id) && doc.sheet(s.id).is_ok() && doc.sheets().len() > 1
        {
            doc.delete_sheet(s.id)?;
        }
    }

    // Pieces: each panel where its box puts it.
    let feature_holes: Vec<u64> = feature_ids
        .iter()
        .flatten()
        .filter(|(_, role)| *role == FeatureRole::Hole)
        .map(|(id, _)| *id)
        .collect();
    let holes: Vec<Vec<P2>> = set
        .joints
        .iter()
        .filter(|j| matched.contains_key(&j.id))
        .flat_map(|j| j.vectors.iter())
        .filter(|v| matches!(v.role, Role::Hole))
        .map(|v| v.id)
        .chain(feature_holes)
        .filter_map(|id| match &doc.entity(id).ok()?.kind {
            EntityKind::Path { path } => path_rings(path).into_iter().next(),
            _ => None,
        })
        .collect();
    let mut infos: Vec<(usize, u64, [f64; 4], Anchor)> = Vec::new();
    for (i, v) in vectors.iter().enumerate() {
        let EntityKind::Path { path } = &doc.entity(*v)?.kind else {
            continue;
        };
        let rings = path_rings(path);
        let Some(outer) = rings.first() else {
            continue;
        };
        let Some(b) = path_bbox(path) else {
            continue;
        };
        let inner: Vec<Vec<P2>> = holes
            .iter()
            .filter(|h| h.first().is_some_and(|p| inside_rings(&rings, *p)))
            .cloned()
            .collect();
        let (area, centre) = area_centre(outer, &inner);
        infos.push((
            i,
            sheet_of(i),
            [b.min.x, b.min.y, b.max.x, b.max.y],
            Anchor { centre, area },
        ));
    }
    let mut numbers: BTreeMap<usize, u32> = BTreeMap::new();
    for s in &sheets {
        let mut on: Vec<&(usize, u64, [f64; 4], Anchor)> =
            infos.iter().filter(|x| x.1 == s.id).collect();
        on.sort_by(|a, b| a.2[1].total_cmp(&b.2[1]).then(a.2[0].total_cmp(&b.2[0])));
        for (n, x) in on.iter().enumerate() {
            numbers.insert(x.0, n as u32 + 1);
        }
    }
    let mut assembly = doc.assembly().clone();
    if design.origin == [0.0; 3] {
        let taken: f64 = assembly
            .designs
            .iter()
            .filter(|d| d.id != design.id)
            .map(|d| {
                d.values
                    .get("width")
                    .and_then(Value::as_f64)
                    .unwrap_or(1000.0)
                    + 300.0
            })
            .sum();
        design.origin = [taken, 0.0, 0.0];
    }
    let my_sheets: Vec<u64> = sheets.iter().map(|s| s.id).collect();
    let old_sheets: Vec<u64> = design.sheets.iter().map(|s| s.id).collect();
    assembly.pieces.retain(|p| {
        p.component.is_some() || !(my_sheets.contains(&p.sheet) || old_sheets.contains(&p.sheet))
    });
    let mut made = BTreeMap::new();
    for (i, sheet, b, anchor) in &infos {
        let panel = &panels[*i];
        let place = place_list[*i];
        let centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let flat = place.back(centre);
        let frame = gen.frames[*i].pose();
        let p = frame.apply([flat[0], flat[1], panel.thickness / 2.0]);
        let r = frame.then_local(&place.pose().inverse());
        let rotation = Pose {
            r: r.r,
            t: [0.0; 3],
        }
        .euler_angles();
        let part = numbers.get(i).copied().unwrap_or(1);
        assembly.pieces.push(Piece {
            sheet: *sheet,
            part,
            position: [
                p[0] + design.origin[0],
                p[1] + design.origin[1],
                p[2] + design.origin[2],
            ],
            rotation,
            anchor: Some(*anchor),
            component: None,
            hidden: false,
            grounded: false,
        });
        let joints: Vec<u64> = gen
            .specs
            .iter()
            .filter(|s| s.a == *i || s.b == *i)
            .filter_map(|s| joint_ids.get(&gen.conns[s.conn].key).copied())
            .collect();
        made.insert(
            panel.key.clone(),
            Made {
                vector: vectors[*i],
                sheet: *sheet,
                piece: part,
                joints,
                place: [place.turn, place.shift[0], place.shift[1]],
                features: feature_ids[*i].iter().map(|(id, _)| *id).collect(),
                mark: first_points(doc, vectors[*i]),
                pinned: pins.contains_key(i),
            },
        );
    }
    design.made = made;
    design.sheets = sheets;
    design.layer = layer;
    design.joints = joint_ids;
    design.toolpaths = toolpaths;
    assembly.joint_set = set;
    match assembly.designs.iter_mut().find(|d| d.id == design.id) {
        Some(slot) => *slot = design.clone(),
        None => assembly.designs.push(design.clone()),
    }
    doc.set_assembly(assembly);
    issues.extend(super::models::face_model_issues(doc, design, &gen));
    Ok(summary(&gen, design, &issues))
}

/// Toolpaths to make: (sheet, order, name) to (strategy, settings, vectors).
type Jobs = BTreeMap<(u64, u8, String), (crate::cam::compute::Strategy, Value, Vec<u64>)>;

/// A toolpath to make: sheet, name, strategy, settings, vectors.
type Job = (u64, String, crate::cam::compute::Strategy, Value, Vec<u64>);

/// The jobs as a list, each pocket job split so that no two of its
/// vectors overlap (two rabbets meeting at a corner of a panel): a pocket
/// is cut from vectors that lie apart.
fn apart(doc: &Document, jobs: Jobs) -> Vec<Job> {
    let mut out = Vec::new();
    for ((sheet, _, name), (strategy, params, ids)) in jobs {
        if strategy != crate::cam::compute::Strategy::Pocket {
            out.push((sheet, name, strategy, params, ids));
            continue;
        }
        let mut groups: Vec<Vec<(u64, crate::geom::BBox)>> = Vec::new();
        for id in ids {
            let Some(b) = doc.bbox_of(id) else {
                continue;
            };
            let meets = |o: &crate::geom::BBox| {
                b.min.x < o.max.x - 1e-6
                    && o.min.x < b.max.x - 1e-6
                    && b.min.y < o.max.y - 1e-6
                    && o.min.y < b.max.y - 1e-6
            };
            match groups.iter_mut().find(|g| !g.iter().any(|(_, o)| meets(o))) {
                Some(g) => g.push((id, b)),
                None => groups.push(vec![(id, b)]),
            }
        }
        for (k, g) in groups.into_iter().enumerate() {
            let label = if k == 0 {
                name.clone()
            } else {
                format!("{name} ({})", k + 1)
            };
            out.push((
                sheet,
                label,
                strategy,
                params.clone(),
                g.into_iter().map(|(id, _)| id).collect(),
            ));
        }
    }
    out
}

fn gen_unique(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Removes everything `design` made: its joints, toolpaths and pieces, and
/// its sheets and vectors unless `keep_vectors` (then the panels stay as
/// plain vectors, joints taken off).
///
/// # Errors
/// What the document refuses.
pub fn remove(
    doc: &mut Document,
    design: &FurnitureDesign,
    keep_vectors: bool,
) -> Result<(), CoreError> {
    let mut set = doc.assembly().joint_set.clone();
    let mine: BTreeSet<u64> = design.joints.values().copied().collect();
    for joint in set.joints.iter().filter(|j| mine.contains(&j.id)) {
        if !keep_vectors {
            crate::joints::delete_made(doc, joint);
        }
    }
    set.joints.retain(|j| !mine.contains(&j.id));
    crate::joints::rebuild_skipping(doc, &mut set)?;
    clear_back(doc, &design.back)?;
    let toolpaths: Vec<u64> = design
        .toolpaths
        .values()
        .copied()
        .filter(|id| doc.toolpath(*id).is_ok())
        .collect();
    if !toolpaths.is_empty() {
        doc.delete_toolpaths(&toolpaths)?;
    }
    let sheets: Vec<u64> = design.sheets.iter().map(|s| s.id).collect();
    if !keep_vectors {
        let vectors: Vec<u64> = design
            .made
            .values()
            .flat_map(|m| std::iter::once(m.vector).chain(m.features.iter().copied()))
            .filter(|id| doc.entity(*id).is_ok())
            .collect();
        doc.delete_entities(&vectors);
        for id in &sheets {
            let empty = !doc.entities().values().any(|e| e.sheet == *id);
            if empty && doc.sheet(*id).is_ok() && doc.sheets().len() > 1 {
                doc.delete_sheet(*id)?;
            }
        }
    }
    let mut assembly = doc.assembly().clone();
    assembly.joint_set = set;
    assembly
        .pieces
        .retain(|p| p.component.is_some() || !sheets.contains(&p.sheet));
    assembly.designs.retain(|d| d.id != design.id);
    doc.set_assembly(assembly);
    Ok(())
}
