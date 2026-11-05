//! Models and vectors on furniture panels. A component or vector the maker
//! puts over a panel on its sheet is attached to that panel: its middle as
//! a share of the panel (along the panel's u and v), the face it is carved
//! into and its turn against the panel. When the design is made again it
//! goes where the panel went, keeping its size (or stretched with the
//! panel when asked); one that no longer fits is kept and said, never
//! deleted.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::doc::{Document, EntityKind};
use crate::error::CoreError;
use crate::geom::{BBox, Mat3, Point, Vec2};

use super::design::FurnitureDesign;
use super::eval::{Issue, Panel};
use super::generate::{Generated, Placement};

/// What is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachKind {
    Component,
    Vector,
}

/// A model or vector on a panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// The panel's key.
    pub panel: String,
    pub kind: AttachKind,
    /// The component or vector.
    pub id: u64,
    /// The face it is on: +1 or -1 along the panel's normal.
    pub face: f64,
    /// Its middle as a share of the panel along u and v (0 to 1).
    pub at: [f64; 2],
    /// The panel's size along u and v when it was last placed.
    pub panel_size: [f64; 2],
    /// The turn (radians) of the panel's flat frame on the sheet when it
    /// was last placed.
    pub turn: f64,
    /// Whether the panel's flat frame ran against v (it is turned over).
    pub mirror: bool,
    /// Stretched with the panel instead of keeping its size.
    #[serde(default)]
    pub stretch: bool,
    /// Carved from the job's other side (a double sided job whose joints
    /// need this panel's other face up).
    #[serde(default)]
    pub other_side: bool,
}

/// The box of a component's heights, job millimetres.
pub fn component_box(doc: &Document, id: u64) -> Option<BBox> {
    let f = &doc.component(id).ok()?.field;
    let half = f.cell / 2.0;
    Some(BBox::new(
        Point::new(f.origin.x - half, f.origin.y - half),
        Point::new(
            f.origin.x + (f.width as f64 - 0.5) * f.cell,
            f.origin.y + (f.height as f64 - 0.5) * f.cell,
        ),
    ))
}

fn object_box(doc: &Document, kind: AttachKind, id: u64) -> Option<BBox> {
    match kind {
        AttachKind::Component => component_box(doc, id),
        AttachKind::Vector => doc.bbox_of(id),
    }
}

fn middle(b: &BBox) -> [f64; 2] {
    [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0]
}

fn place_of(m: [f64; 3]) -> Placement {
    Placement {
        turn: m[0],
        shift: [m[1], m[2]],
    }
}

/// The vectors a design owns: its panels, their features and the vectors
/// its joints made.
pub fn owned_vectors(doc: &Document, design: &FurnitureDesign) -> BTreeSet<u64> {
    let mine: Vec<u64> = design.joints.values().copied().collect();
    design
        .made
        .values()
        .flat_map(|m| std::iter::once(m.vector).chain(m.features.iter().copied()))
        .chain(
            doc.assembly()
                .joint_set
                .joints
                .iter()
                .filter(|j| mine.contains(&j.id))
                .flat_map(|j| j.vectors.iter().map(|v| v.id)),
        )
        .collect()
}

/// Every vector some joint made or works on (other designs' and the Joint
/// Creator's too), which is never the maker's drawing on a panel.
fn joint_vectors(doc: &Document) -> BTreeSet<u64> {
    doc.assembly()
        .joint_set
        .joints
        .iter()
        .flat_map(|j| {
            j.vectors
                .iter()
                .map(|v| v.id)
                .chain([j.a.vector, j.b.vector()])
        })
        .collect()
}

/// The panel of `design` whose outline holds `point` on `sheet`, if any.
pub fn panel_under(
    doc: &Document,
    design: &FurnitureDesign,
    sheet: u64,
    point: [f64; 2],
) -> Option<String> {
    design
        .made
        .iter()
        .filter(|(_, m)| m.sheet == sheet)
        .find(|(_, m)| match doc.entity(m.vector).map(|e| &e.kind) {
            Ok(EntityKind::Path { path }) => crate::joints::geometry::inside_rings(
                &crate::joints::geometry::path_rings(path),
                point,
            ),
            _ => false,
        })
        .map(|(key, _)| key.clone())
}

/// A panel's flat point back to panel coordinates (u, v) from its box
/// corner, through the frame it was made with.
fn panel_uv(gen_panel: &Panel, frame: &super::layout::PanelFrame, flat: [f64; 2]) -> [f64; 2] {
    let w = frame.pose().apply([flat[0], flat[1], 0.0]);
    [
        w[frame.u] - gen_panel.min[frame.u],
        w[frame.v] - gen_panel.min[frame.v],
    ]
}

/// Brings the design's attachments up to date before it is made again:
/// drops those whose object is gone and attaches every component and
/// vector the maker put over one of its panels since, as the panels lie
/// now (`old`, the design as last made).
pub fn capture(doc: &Document, design: &mut FurnitureDesign, old: &Generated) {
    // A model carved from the other side lives with that side's content.
    design.attachments.retain(|a| match a.kind {
        AttachKind::Component => a.other_side || doc.component(a.id).is_ok(),
        AttachKind::Vector => doc.entity(a.id).is_ok(),
    });
    let sheets: Vec<u64> = design.sheets.iter().map(|s| s.id).collect();
    let taken: BTreeSet<(bool, u64)> = design
        .attachments
        .iter()
        .map(|a| (a.kind == AttachKind::Component, a.id))
        .collect();
    let mut found: Vec<(AttachKind, u64, u64, BBox)> = Vec::new();
    for c in doc.all_components() {
        let sheet = doc.sheet_of_component(c);
        if !sheets.contains(&sheet) || taken.contains(&(true, c.id)) {
            continue;
        }
        if let Some(b) = component_box(doc, c.id) {
            found.push((AttachKind::Component, c.id, sheet, b));
        }
    }
    let owned = owned_vectors(doc, design);
    let joints = joint_vectors(doc);
    for (id, e) in doc.entities() {
        if e.parent.is_some()
            || !sheets.contains(&e.sheet)
            || owned.contains(id)
            || joints.contains(id)
            || taken.contains(&(false, *id))
        {
            continue;
        }
        if let Some(b) = doc.bbox_of(*id) {
            found.push((AttachKind::Vector, *id, e.sheet, b));
        }
    }
    for (kind, id, sheet, b) in found {
        let mid = middle(&b);
        let Some(key) = panel_under(doc, design, sheet, mid) else {
            continue;
        };
        let Some(i) = old.eval.panels.iter().position(|p| p.key == key) else {
            continue;
        };
        let made = &design.made[&key];
        let panel = &old.eval.panels[i];
        let frame = &old.frames[i];
        let place = place_of(made.place);
        let uv = panel_uv(panel, frame, place.back(mid));
        let size = [panel.size(frame.u), panel.size(frame.v)];
        design.attachments.push(Attachment {
            panel: key,
            kind,
            id,
            face: frame.up,
            at: [uv[0] / size[0], uv[1] / size[1]],
            panel_size: size,
            turn: made.place[0],
            mirror: frame.mirror,
            stretch: false,
            other_side: false,
        });
    }
}

/// The face each panel's models need up, by panel key.
pub fn faces(design: &FurnitureDesign) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for a in design
        .attachments
        .iter()
        .filter(|a| a.kind == AttachKind::Component)
    {
        out.insert(a.panel.clone(), a.face);
    }
    out
}

/// The move from where an attachment lies to where it goes.
struct Move {
    m: Mat3,
    sheet: u64,
    fits: bool,
}

/// Moves every attachment to where its panel now lies (`places` and the
/// new layout `gen`), inside the caller's step; issues for those that no
/// longer fit their panel, whose panel is gone, or whose face is now cut
/// from below.
///
/// # Errors
/// What moving a component reports.
pub fn follow(
    doc: &mut Document,
    design: &mut FurnitureDesign,
    gen: &Generated,
    places: &[(u32, Placement)],
    sheet_of: &dyn Fn(usize) -> u64,
    issues: &mut Vec<Issue>,
) -> Result<(), CoreError> {
    let double = doc.job_setup().is_double_sided();
    let mut kept = Vec::with_capacity(design.attachments.len());
    for mut a in std::mem::take(&mut design.attachments) {
        let Some(i) = gen.eval.panels.iter().position(|p| p.key == a.panel) else {
            issues.push(Issue::on_panel(
                format!(
                    "the panel {} is gone, so what was on it stays where it was",
                    a.panel
                ),
                &a.panel,
            ));
            continue;
        };
        let panel = &gen.eval.panels[i];
        let frame = &gen.frames[i];
        let place = places[i].1;
        let size = [panel.size(frame.u), panel.size(frame.v)];
        let flipped = frame.up != a.face;
        if a.kind == AttachKind::Component && (a.other_side || (flipped && double)) {
            // Carved from the job's other side: where the panel's face lies
            // seen from there.
            let uv = [a.at[0] * size[0], a.at[1] * size[1]];
            let to = place.apply(frame.flat_uv(panel, uv[0], uv[1]));
            a.id = across(doc, &a, to, place.turn, sheet_of(i), flipped)?;
            a.turn = place.turn;
            a.mirror = frame.mirror;
            a.panel_size = size;
            a.other_side = flipped;
            kept.push(a);
            continue;
        }
        let Some(b) = object_box(doc, a.kind, a.id) else {
            continue;
        };
        let from = middle(&b);
        let uv = [a.at[0] * size[0], a.at[1] * size[1]];
        let flat = frame.flat_uv(panel, uv[0], uv[1]);
        let to = place.apply(flat);
        // Turn with the panel; stretched, scale along its flat axes.
        let turn = place.turn - a.turn;
        let (su, sv) = if a.stretch {
            (size[0] / a.panel_size[0], size[1] / a.panel_size[1])
        } else {
            (1.0, 1.0)
        };
        let back = Mat3::translate(Vec2::new(-from[0], -from[1])).then(&Mat3::rotate(-a.turn));
        let scaled = back.then(&Mat3::scale(su, sv));
        let m = scaled
            .then(&Mat3::rotate(a.turn + turn))
            .then(&Mat3::translate(Vec2::new(to[0], to[1])));
        if flipped && a.kind == AttachKind::Component && !double {
            issues.push(Issue::on_panel(
                format!(
                    "a model on {} is on the face cut from below; switch the joints that need pockets on its other face",
                    panel.name
                ),
                &panel.key,
            ));
        }
        let mv = Move {
            m,
            sheet: sheet_of(i),
            fits: true,
        };
        let moved = apply_move(doc, &a, &mv)?;
        // Kept on the panel: its box within the panel's outline box.
        if let (Some(nb), Some(pb)) = (
            object_box(doc, a.kind, a.id),
            doc.bbox_of(design.made.get(&panel.key).map_or(0, |m| m.vector)),
        ) {
            let inside = nb.min.x >= pb.min.x - 0.5
                && nb.min.y >= pb.min.y - 0.5
                && nb.max.x <= pb.max.x + 0.5
                && nb.max.y <= pb.max.y + 0.5;
            if !inside && mv.fits {
                issues.push(Issue::on_panel(
                    format!(
                        "a {} on {} no longer fits it; it is kept where the panel's share puts it",
                        if a.kind == AttachKind::Component {
                            "model"
                        } else {
                            "vector"
                        },
                        panel.name
                    ),
                    &panel.key,
                ));
            }
        }
        let _ = moved;
        a.turn = place.turn;
        a.mirror = frame.mirror;
        a.panel_size = size;
        a.other_side = flipped && double && a.kind == AttachKind::Component;
        kept.push(a);
    }
    design.attachments = kept;
    Ok(())
}

/// The other side's sheet named as this side's `sheet` (made there, with
/// its material, when it has none).
pub(super) fn sheet_across(
    doc: &mut Document,
    name: &str,
    material: crate::doc::SheetMaterial,
) -> Result<u64, CoreError> {
    if let Some(s) = doc.sheets().iter().find(|s| s.name == name) {
        return Ok(s.id);
    }
    let id = doc.add_sheet(Some(name.to_string()));
    doc.set_sheet_material(id, material)?;
    Ok(id)
}

/// Puts a model carved from the job's other side (`under`) where its
/// panel's face lies, `to` in this side's job space and the panel turned
/// `turn`; or brings it back to this side when its face is up again.
/// Answers the component's id (a model moved across sides is a new
/// component there).
fn across(
    doc: &mut Document,
    a: &Attachment,
    to: [f64; 2],
    turn: f64,
    sheet: u64,
    under: bool,
) -> Result<u64, CoreError> {
    let here = doc.active_side();
    let job = *doc.job();
    let flip = {
        let (e, f) = job.flip_xy(0.0, 0.0);
        let (ax, ay) = job.flip_xy(1.0, 0.0);
        let (cx, cy) = job.flip_xy(0.0, 1.0);
        Mat3::new(ax - e, ay - f, cx - e, cy - f, e, f)
    };
    let name = doc.sheet(sheet)?.name.clone();
    let material = doc.sheet_material(sheet)?;
    let delta = turn - a.turn;
    // Where it lies now, on whichever side holds it.
    if a.other_side {
        doc.set_active_side(here.other())?;
    }
    let component = doc.component(a.id)?.clone();
    let b = component_box(doc, a.id).ok_or_else(|| CoreError::validation("the model is empty"))?;
    let from = middle(&b);
    let (target, m) = if a.other_side {
        // From the other side's space: back to this side's, turned with the
        // panel and placed, then across again if it stays under.
        let (fx, fy) = job.flip_xy(from[0], from[1]);
        let moved = Mat3::translate(Vec2::new(-fx, -fy))
            .then(&Mat3::rotate(delta))
            .then(&Mat3::translate(Vec2::new(to[0], to[1])));
        let m = flip.then(&moved);
        if under {
            (here.other(), m.then(&flip))
        } else {
            (here, m)
        }
    } else {
        let moved = Mat3::translate(Vec2::new(-from[0], -from[1]))
            .then(&Mat3::rotate(delta))
            .then(&Mat3::translate(Vec2::new(to[0], to[1])));
        (here.other(), moved.then(&flip))
    };
    let mut copy = crate::model::component_cmd::copy_of(doc, &component, &m, &BTreeMap::new())?;
    copy.extra.shape = None;
    copy.extra.texture = None;
    copy.extra.group = 0;
    let now_side = doc.active_side();
    if now_side == target {
        doc.update_component(a.id, |c| {
            c.field = copy.field.clone();
            c.extra = copy.extra.clone();
        })?;
        let sheet_there = sheet_across(doc, &name, material)?;
        doc.update_component(a.id, |c| c.extra.sheet = sheet_there)?;
        doc.set_active_side(here)?;
        return Ok(a.id);
    }
    doc.delete_components(&[a.id])?;
    doc.set_active_side(target)?;
    let sheet_there = if target == here {
        sheet
    } else {
        sheet_across(doc, &name, material)?
    };
    copy.extra.sheet = sheet_there;
    copy.level = doc.active_level();
    let id = doc.add_component(copy);
    doc.set_active_side(here)?;
    Ok(id)
}

/// Moves one attachment by `mv` (and onto its sheet).
fn apply_move(doc: &mut Document, a: &Attachment, mv: &Move) -> Result<bool, CoreError> {
    let still = (mv.m.a - 1.0).abs() < 1e-12
        && mv.m.b.abs() < 1e-12
        && mv.m.c.abs() < 1e-12
        && (mv.m.d - 1.0).abs() < 1e-12
        && mv.m.e.abs() < 1e-9
        && mv.m.f.abs() < 1e-9;
    match a.kind {
        AttachKind::Vector => {
            if !still {
                doc.transform_entities(&[a.id], &mv.m);
            }
            if doc.entity(a.id)?.sheet != mv.sheet {
                doc.set_entity_sheet(&[a.id], mv.sheet)?;
            }
        }
        AttachKind::Component => {
            let component = doc.component(a.id)?.clone();
            let moved = if still {
                component.clone()
            } else {
                crate::model::component_cmd::copy_of(doc, &component, &mv.m, &BTreeMap::new())?
            };
            let sheet = mv.sheet;
            doc.update_component(a.id, |c| {
                c.field = moved.field;
                c.extra = moved.extra;
                c.extra.sheet = sheet;
            })?;
        }
    }
    Ok(!still)
}
