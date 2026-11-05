//! Models on panel faces: add a model to a face (clipart, a model file or a
//! component), list what is on the panels, find the panel under a point,
//! make the carving toolpaths, and the face models' heights for the
//! preview.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::assembly::pose::Vec3;
use crate::cam::compute::{calculate, Request, Strategy};
use crate::doc::{Document, JobSetup};
use crate::error::CoreError;
use crate::geom::{Mat3, Point, Vec2};
use crate::kernel::{edit, KernelReply};
use crate::model::Combine;

use super::attach::{component_box, AttachKind, Attachment};
use super::design::{end_mill, regenerate, FurnitureDesign};
use super::eval::{Issue, Panel, TOL};
use super::generate::{Generated, Placement};
use super::layout::PanelFrame;
use super::mesh::Mesh;

/// The least material to leave under the deepest point of a face model
/// when the job's Material Position in Material says nothing narrower,
/// millimetres.
const DEFAULT_SKIN: f64 = 3.0;

/// Job Z of a face model's zero plane, for a panel `thickness` thick
/// carrying a model `peak` high: counted up from the panel's underside (0)
/// to its top (`thickness`), by the same Material Position in Material rule
/// as [`crate::doc::JobSetup::model_zero`] (Gap Below Model keeps `model_gap`
/// clear under the model; Gap Above Model, the default, sits the model's
/// peak `model_gap` below the top), so the preview shows what the carving
/// actually reaches: the surface never runs above the panel's top nor below
/// its underside.
fn face_model_zero(job: &JobSetup, thickness: f64, peak: f64) -> f64 {
    let gap = job.machine.model_gap.max(0.0);
    let depth = if job.machine.gap_below {
        (thickness - gap).max(0.0)
    } else {
        (gap + peak.max(0.0)).min(thickness)
    };
    thickness - depth
}

/// The height a face model's own skin (the material kept under its deepest
/// point) asks for, millimetres: the job's Material Position in Material
/// gap when Gap Below Model is on and asks for more than nothing, else
/// [`DEFAULT_SKIN`].
fn face_model_skin(job: &JobSetup) -> f64 {
    if job.machine.gap_below && job.machine.model_gap > 0.0 {
        job.machine.model_gap
    } else {
        DEFAULT_SKIN
    }
}

/// Where a model comes from.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// A clipart file (a clip, a mesh or vectors).
    #[serde(default)]
    pub clipart: Option<std::path::PathBuf>,
    /// A model file (STL or OBJ).
    #[serde(default)]
    pub file: Option<std::path::PathBuf>,
    /// A component already in the job.
    #[serde(default)]
    pub component: Option<u64>,
}

/// How a model is sized to its panel.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Fit {
    /// The margin left round it, as a share of the panel's smaller side.
    pub margin: f64,
}

impl Default for Fit {
    fn default() -> Self {
        Fit { margin: 0.1 }
    }
}

/// `furniture.addModel` arguments.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModelArgs {
    pub design: u64,
    /// The panel's key.
    pub panel: String,
    /// `top` or `bottom`, as the panel lies on its sheet.
    #[serde(default = "top_word")]
    pub face: String,
    pub source: Source,
    /// Where its middle goes, millimetres from the panel's lower left on
    /// the sheet; the panel's middle when left out.
    #[serde(default)]
    pub point: Option<[f64; 2]>,
    #[serde(default)]
    pub fit: Fit,
}

fn top_word() -> String {
    "top".into()
}

fn design_of(document: &Document, id: u64) -> Result<FurnitureDesign, CoreError> {
    document
        .assembly()
        .designs
        .iter()
        .find(|d| d.id == id)
        .cloned()
        .ok_or_else(|| CoreError::validation(format!("there is no furniture design {id}")))
}

fn reply_value(reply: KernelReply) -> Value {
    match reply {
        KernelReply::Json(text) => serde_json::from_str::<Value>(&text)
            .map(|v| v["result"].clone())
            .unwrap_or(Value::Null),
        KernelReply::Bytes(_) => Value::Null,
    }
}

/// The design as it is made now (its frames and boxes).
fn made_layout(doc: &Document, design: &FurnitureDesign) -> Result<Generated, CoreError> {
    super::generate::lay_out(
        &design.inputs_in(doc),
        &BTreeMap::new(),
        doc.assembly().joint_set.settings.tool_diameter,
    )
}

/// Adds a model to a panel's face: inserts the clipart or model file (or
/// takes the component) onto the panel's sheet at the point, sized to fit
/// the panel less the margin and combined Add, attaches it and makes the
/// design again so the face it is on is cut from the top. One undo step.
///
/// # Errors
/// [`CoreError::Validation`] for a design or panel not there, a face that
/// is not top or bottom, or no source.
pub fn add_model(document: &mut Document, args: AddModelArgs) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    let made =
        design.made.get(&args.panel).cloned().ok_or_else(|| {
            CoreError::validation(format!("the design has no panel {}", args.panel))
        })?;
    let up = match args.face.as_str() {
        "top" => true,
        "bottom" => false,
        _ => return Err(CoreError::validation("the face is top or bottom")),
    };
    let outline = document
        .bbox_of(made.vector)
        .ok_or_else(|| CoreError::validation("the panel's outline is gone"))?;
    let at = args.point.map_or_else(
        || {
            [
                (outline.min.x + outline.max.x) / 2.0,
                (outline.min.y + outline.max.y) / 2.0,
            ]
        },
        |p| [outline.min.x + p[0], outline.min.y + p[1]],
    );
    let gen = made_layout(document, &design)?;
    let i = gen
        .eval
        .panels
        .iter()
        .position(|p| p.key == args.panel)
        .ok_or_else(|| CoreError::validation(format!("the design has no panel {}", args.panel)))?;
    let frame = gen.frames[i];
    let showing = document.active_sheet();
    document.begin_undo_group(Some("Add Model to Face".into()));
    let result = (|| -> Result<Value, CoreError> {
        if document.active_sheet() != made.sheet {
            document.begin("Show Sheet");
            document.set_active_sheet(made.sheet)?;
            document.commit();
        }
        // The object: inserted, or the component given.
        let (kind, ids) = match (
            &args.source.clipart,
            &args.source.file,
            args.source.component,
        ) {
            (Some(path), _, _) | (None, Some(path), _) => {
                let inserted = reply_value(crate::clipart::insert(
                    document,
                    &crate::clipart::InsertArgs {
                        path: path.clone(),
                        x: at[0],
                        y: at[1],
                        scale: 1.0,
                        combine: Combine::Add,
                    },
                )?);
                if inserted["kind"] == "relief" {
                    (
                        AttachKind::Component,
                        vec![inserted["id"].as_u64().unwrap_or(0)],
                    )
                } else {
                    let ids: Vec<u64> = inserted["ids"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    (AttachKind::Vector, ids)
                }
            }
            (None, None, Some(id)) => {
                document.component(id)?;
                (AttachKind::Component, vec![id])
            }
            (None, None, None) => {
                return Err(CoreError::validation(
                    "say which clipart, model file or component to add",
                ))
            }
        };
        let panel_thickness = gen.eval.panels[i].thickness;
        edit(document, "Add Model to Face", |doc| {
            // Sized to fit inside the panel less the margin, its middle
            // where asked (kept inside).
            let (w, h) = (outline.width(), outline.height());
            let margin = args.fit.margin.clamp(0.0, 0.45) * w.min(h);
            let boxes: Vec<crate::geom::BBox> = ids
                .iter()
                .filter_map(|id| match kind {
                    AttachKind::Component => component_box(doc, *id),
                    AttachKind::Vector => doc.bbox_of(*id),
                })
                .collect();
            let Some(own) = boxes.iter().copied().reduce(|a, b| a.union(b)) else {
                return Err(CoreError::validation("the model has nothing in it"));
            };
            let room = [(w - 2.0 * margin).max(1.0), (h - 2.0 * margin).max(1.0)];
            let scale = (room[0] / own.width().max(1e-9)).min(room[1] / own.height().max(1e-9));
            let half = [own.width() * scale / 2.0, own.height() * scale / 2.0];
            let centre = [
                at[0].clamp(
                    outline.min.x + margin + half[0],
                    outline.max.x - margin - half[0],
                ),
                at[1].clamp(
                    outline.min.y + margin + half[1],
                    outline.max.y - margin - half[1],
                ),
            ];
            let from = [(own.min.x + own.max.x) / 2.0, (own.min.y + own.max.y) / 2.0];
            let m = Mat3::translate(Vec2::new(-from[0], -from[1]))
                .then(&Mat3::scale(scale, scale))
                .then(&Mat3::translate(Vec2::new(centre[0], centre[1])));
            match kind {
                AttachKind::Vector => {
                    doc.transform_entities(&ids, &m);
                    doc.set_entity_sheet(&ids, made.sheet)?;
                }
                AttachKind::Component => {
                    for id in &ids {
                        let c = doc.component(*id)?.clone();
                        let moved =
                            crate::model::component_cmd::copy_of(doc, &c, &m, &BTreeMap::new())?;
                        doc.update_component(*id, |c| {
                            c.field = moved.field;
                            c.extra = moved.extra;
                            c.extra.sheet = made.sheet;
                            c.combine = Combine::Add;
                            c.height_scale *= scale;
                        })?;
                        // Scaled in height to fit the panel's stock less a
                        // skin, so the carving never has to flatten it: at
                        // least the skin stays under the deepest cut, as
                        // `face_model_zero` places the model for the
                        // preview and the real toolpaths.
                        let peak = crate::model::component_cmd::top_of(doc.component(*id)?);
                        if let Some(peak) = peak.filter(|p| *p > 0.0) {
                            let skin = face_model_skin(doc.job_setup());
                            let available = (panel_thickness - skin).max(0.0);
                            if peak > available {
                                let factor = available / peak;
                                doc.update_component(*id, |c| c.height_scale *= factor)?;
                            }
                        }
                    }
                }
            }
            // Attached to the face asked for, then the design made again.
            let mut design = design.clone();
            let panel = &gen.eval.panels[i];
            let size = [panel.size(frame.u), panel.size(frame.v)];
            let place = Placement {
                turn: made.place[0],
                shift: [made.place[1], made.place[2]],
            };
            let w = frame.pose().apply({
                let f = place.back(centre);
                [f[0], f[1], 0.0]
            });
            let uv = [
                w[frame.u] - panel.min[frame.u],
                w[frame.v] - panel.min[frame.v],
            ];
            for id in &ids {
                design
                    .attachments
                    .retain(|a| !(a.id == *id && a.kind == kind));
                design.attachments.push(Attachment {
                    panel: args.panel.clone(),
                    kind,
                    id: *id,
                    face: if up { frame.up } else { -frame.up },
                    at: [uv[0] / size[0], uv[1] / size[1]],
                    panel_size: size,
                    turn: made.place[0],
                    mirror: frame.mirror,
                    stretch: false,
                    other_side: false,
                });
            }
            let summary = regenerate(doc, &mut design)?;
            if doc.active_sheet() != showing && doc.sheet(showing).is_ok() {
                doc.set_active_sheet(showing)?;
            }
            Ok(json!({ "design": design.id, "ids": ids, "summary": summary }))
        })
        .map(reply_value)
    })();
    document.end_undo_group();
    let value = result?;
    KernelReply::ok(&value)
}

/// The word of the face that points along `dir` on axis `n` (left, right,
/// front, back, bottom, top).
fn face_word(n: usize, dir: f64) -> &'static str {
    match (n, dir > 0.0) {
        (0, false) => "left",
        (0, true) => "right",
        (1, false) => "front",
        (1, true) => "back",
        (_, false) => "bottom",
        (_, true) => "top",
    }
}

/// `furniture.attachments { design }` -> `{ attachments: [{ panel, kind,
/// id, face ("top"|"bottom" as the panel lies on its sheet), faceWord
/// (the face in furniture space: left, right, front, back, bottom, top),
/// at, stretch, otherSide }] }`.
pub fn attachments(document: &Document, id: u64) -> Result<KernelReply, CoreError> {
    let design = design_of(document, id)?;
    let gen = made_layout(document, &design)?;
    let rows: Vec<Value> = design
        .attachments
        .iter()
        .map(|a| {
            let frame = gen
                .eval
                .panels
                .iter()
                .position(|p| p.key == a.panel)
                .map(|i| (gen.eval.panels[i].normal, gen.frames[i].up));
            let (face, word) = frame.map_or(("top", ""), |(n, up)| {
                (
                    if up == a.face { "top" } else { "bottom" },
                    face_word(n, a.face),
                )
            });
            json!({
                "panel": a.panel,
                "kind": a.kind,
                "id": a.id,
                "face": face,
                "faceWord": word,
                "at": a.at,
                "stretch": a.stretch,
                "otherSide": a.other_side,
            })
        })
        .collect();
    KernelReply::ok(&json!({ "attachments": rows }))
}

/// `furniture.setStretch` arguments.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetStretchArgs {
    pub design: u64,
    pub kind: AttachKind,
    /// The component or vector.
    pub id: u64,
    /// Stretched with its panel when the design changes size, else kept
    /// its size, centered on its share of the panel.
    pub stretch: bool,
}

/// `furniture.setStretch { design, kind, id, stretch }` -> `{ design,
/// revision }`: one undo step, "Stretch to Fit" or "Keep Size".
pub fn set_stretch(
    document: &mut Document,
    args: SetStretchArgs,
) -> Result<KernelReply, CoreError> {
    let mut design = design_of(document, args.design)?;
    let a = design
        .attachments
        .iter_mut()
        .find(|a| a.kind == args.kind && a.id == args.id)
        .ok_or_else(|| CoreError::validation("that object is not on a panel of the design"))?;
    a.stretch = args.stretch;
    let label = if args.stretch {
        "Stretch to Fit"
    } else {
        "Keep Size"
    };
    edit(document, label, |doc| {
        let mut assembly = doc.assembly().clone();
        if let Some(slot) = assembly.designs.iter_mut().find(|d| d.id == design.id) {
            *slot = design.clone();
        }
        doc.set_assembly(assembly);
        Ok(json!({ "design": design.id }))
    })
}

/// `furniture.panelOf { sheet, point }` -> `{ design, panel, name, top,
/// bottom }` for the panel under a job point on a sheet (`top` and
/// `bottom` name the faces that lie up and down there in furniture
/// space), or nulls.
pub fn panel_of(
    document: &Document,
    sheet: u64,
    point: [f64; 2],
) -> Result<KernelReply, CoreError> {
    for design in &document.assembly().designs {
        let Some(key) = super::attach::panel_under(document, design, sheet, point) else {
            continue;
        };
        let gen = made_layout(document, design)?;
        let Some(i) = gen.eval.panels.iter().position(|p| p.key == key) else {
            continue;
        };
        let (n, up) = (gen.eval.panels[i].normal, gen.frames[i].up);
        return KernelReply::ok(&json!({
            "design": design.id,
            "panel": key,
            "name": gen.eval.panels[i].name,
            "top": face_word(n, up),
            "bottom": face_word(n, -up),
        }));
    }
    KernelReply::ok(&json!({ "design": null, "panel": null }))
}

/// `furniture.makeCarving { design }`: 3D roughing (the joints' end mill)
/// and finishing (a 3.175 mm ball nose) over the models on each of the
/// design's sheets that has some, replacing the ones it made before; the
/// toolpaths the maker made are left alone. One undo step. Answers
/// `{ ids }`.
///
/// # Errors
/// [`CoreError::Validation`] when no panel has a model.
pub fn make_carving(document: &mut Document, id: u64) -> Result<KernelReply, CoreError> {
    let mut design = design_of(document, id)?;
    let sheets: Vec<u64> = design
        .sheets
        .iter()
        .map(|s| s.id)
        .filter(|s| {
            design.attachments.iter().any(|a| {
                a.kind == AttachKind::Component
                    && !a.other_side
                    && document
                        .component(a.id)
                        .is_ok_and(|c| document.sheet_of_component(c) == *s)
            })
        })
        .collect();
    if sheets.is_empty() {
        return Err(CoreError::validation(
            "no panel of this design has a model to carve",
        ));
    }
    let rough = end_mill(document.assembly().joint_set.settings.tool_diameter);
    let finish = crate::cam::tool::Tool {
        name: "Ball Nose (3.175 mm)".into(),
        tool_type: crate::cam::tool::ToolGeometry::BallNose { diameter: 3.175 },
        ..crate::cam::tool::Tool::default()
    };
    edit(document, "Make Carving Toolpaths", |doc| {
        let showing = doc.active_sheet();
        let mut ids = Vec::new();
        for sheet in &sheets {
            if doc.active_sheet() != *sheet {
                doc.set_active_sheet(*sheet)?;
            }
            let old: Vec<u64> = design
                .carving
                .get(sheet)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|t| doc.toolpath(*t).is_ok())
                .collect();
            let mut made = Vec::new();
            for (k, (strategy, tool, label)) in [
                (Strategy::Rough3d, &rough, "carving roughing"),
                (Strategy::Finish3d, &finish, "carving finishing"),
            ]
            .into_iter()
            .enumerate()
            {
                let request = Request {
                    strategy,
                    name: Some(format!("{} {label}", design.name)),
                    tool: tool.clone(),
                    params: json!({}),
                    ids: Vec::new(),
                };
                let (mut toolpath, _) = calculate(doc, &request)?;
                toolpath.sheet = *sheet;
                let id = match old.get(k) {
                    Some(id) => {
                        doc.replace_toolpath(*id, toolpath)?;
                        *id
                    }
                    None => doc.add_toolpath(toolpath),
                };
                made.push(id);
            }
            ids.extend(made.iter().copied());
            design.carving.insert(*sheet, made);
        }
        if doc.active_sheet() != showing && doc.sheet(showing).is_ok() {
            doc.set_active_sheet(showing)?;
        }
        let mut assembly = doc.assembly().clone();
        if let Some(slot) = assembly.designs.iter_mut().find(|d| d.id == design.id) {
            slot.carving = design.carving.clone();
        }
        doc.set_assembly(assembly);
        Ok(json!({ "ids": ids }))
    })
}

/// The face models of panel `i` of a preview `gen` as a mesh of raised
/// cells on its top face: the heights of its sheet's model under where
/// the panel lies now (`now`, the design as made), mapped by their share
/// of the panel onto the panel as previewed. `None` without models.
pub fn face_relief(
    doc: &Document,
    design: &FurnitureDesign,
    now: &Generated,
    gen: &Generated,
    i: usize,
    composites: &mut BTreeMap<u64, crate::model::field::Field>,
) -> Option<Mesh> {
    let panel: &Panel = &gen.eval.panels[i];
    let frame: &PanelFrame = &gen.frames[i];
    if !design
        .attachments
        .iter()
        .any(|a| a.panel == panel.key && a.kind == AttachKind::Component && !a.other_side)
    {
        return None;
    }
    let made = design.made.get(&panel.key)?;
    let j = now.eval.panels.iter().position(|p| p.key == panel.key)?;
    let (was, was_frame) = (&now.eval.panels[j], &now.frames[j]);
    if let std::collections::btree_map::Entry::Vacant(slot) = composites.entry(made.sheet) {
        slot.insert(doc.sheet_composite(made.sheet).ok()?);
    }
    let field = composites.get(&made.sheet)?;
    let place = Placement {
        turn: made.place[0],
        shift: [made.place[1], made.place[2]],
    };
    let size = [panel.size(frame.u), panel.size(frame.v)];
    let old = [was.size(was_frame.u), was.size(was_frame.v)];
    let step = field.cell.max(size[0].max(size[1]) / 200.0);
    let (nu, nv) = (
        (size[0] / step).ceil() as usize,
        (size[1] / step).ceil() as usize,
    );
    let height = |u: f64, v: f64| -> f32 {
        let flat = was_frame.flat_uv(was, u / size[0] * old[0], v / size[1] * old[1]);
        let job = place.apply(flat);
        field.at(Point::new(job[0], job[1]))
    };
    let pose = frame.pose();
    let up = pose.apply_vector([0.0, 0.0, 1.0]);
    // One continuous surface over a grid of nodes, so the model reads as a
    // smooth relief: each node at its height above the face (on the face
    // where there is none), a cell drawn where any of its corners is raised.
    let (cols, rows) = (nu + 1, nv + 1);
    let node_uv = |a: usize, b: usize| {
        (
            (a as f64 * step).min(size[0]),
            (b as f64 * step).min(size[1]),
        )
    };
    let raw: Vec<f64> = (0..rows)
        .flat_map(|b| (0..cols).map(move |a| (a, b)))
        .map(|(a, b)| {
            let (u, v) = node_uv(a, b);
            let h = f64::from(height(u, v));
            if h.is_finite() && h > 0.0 {
                h
            } else {
                0.0
            }
        })
        .collect();
    // Carving can only reach within the panel's own stock: the model's
    // surface is placed by the same Material Position in Material rule as
    // the real toolpaths (`face_model_zero`), so a model taller than the
    // panel is cut flat at the panel's top instead of standing proud of it.
    let peak = raw.iter().copied().fold(0.0_f64, f64::max);
    let zero = face_model_zero(doc.job_setup(), panel.thickness, peak);
    let heights: Vec<f64> = raw
        .iter()
        .map(|h| (zero + h).clamp(0.0, panel.thickness))
        .collect();
    let point = |a: usize, b: usize| {
        let (u, v) = node_uv(a, b);
        let f = frame.flat_uv(panel, u, v);
        pose.apply([f[0], f[1], heights[b * cols + a]])
    };
    let raised = |a: usize, b: usize| {
        raw[b * cols + a] > 0.0
            || raw[b * cols + a + 1] > 0.0
            || raw[(b + 1) * cols + a] > 0.0
            || raw[(b + 1) * cols + a + 1] > 0.0
    };
    let mut index: Vec<Option<u32>> = vec![None; cols * rows];
    let mut mesh = Mesh::default();
    let mut node = |mesh: &mut Mesh, a: usize, b: usize| -> u32 {
        if let Some(k) = index[b * cols + a] {
            return k;
        }
        let k = (mesh.positions.len() / 3) as u32;
        let p = point(a, b);
        // The normal from the neighbours on either side, facing out.
        let (a0, a1) = (a.saturating_sub(1), (a + 1).min(cols - 1));
        let (b0, b1) = (b.saturating_sub(1), (b + 1).min(rows - 1));
        let (pa0, pa1, pb0, pb1) = (point(a0, b), point(a1, b), point(a, b0), point(a, b1));
        let du = [pa1[0] - pa0[0], pa1[1] - pa0[1], pa1[2] - pa0[2]];
        let dv = [pb1[0] - pb0[0], pb1[1] - pb0[1], pb1[2] - pb0[2]];
        let mut n = [
            du[1] * dv[2] - du[2] * dv[1],
            du[2] * dv[0] - du[0] * dv[2],
            du[0] * dv[1] - du[1] * dv[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let facing = n[0] * up[0] + n[1] * up[1] + n[2] * up[2];
        if len < 1e-12 {
            n = up;
        } else {
            let s = if facing < 0.0 { -1.0 / len } else { 1.0 / len };
            n = [n[0] * s, n[1] * s, n[2] * s];
        }
        mesh.positions.extend(p.iter().map(|x| *x as f32));
        mesh.normals.extend(n.iter().map(|x| *x as f32));
        index[b * cols + a] = Some(k);
        k
    };
    for a in 0..nu {
        for b in 0..nv {
            if !raised(a, b) {
                continue;
            }
            let q = [
                node(&mut mesh, a, b),
                node(&mut mesh, a + 1, b),
                node(&mut mesh, a + 1, b + 1),
                node(&mut mesh, a, b + 1),
            ];
            // Counter-clockwise seen from the top in flat space; a mirrored
            // frame turns the panel coordinates the other way.
            if frame.mirror {
                mesh.indices.extend([q[0], q[2], q[1], q[0], q[3], q[2]]);
            } else {
                mesh.indices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
        }
    }
    (!mesh.indices.is_empty()).then_some(mesh)
}

/// "inside" when `face` (the attachment's +1/-1 along the panel's normal)
/// points towards the piece's middle, else "outside".
fn face_side_word(panel: &Panel, face: f64, middle: Vec3) -> &'static str {
    let n = panel.normal;
    let inwards = if middle[n] - panel.centre()[n] < -super::eval::TOL {
        -1.0
    } else {
        1.0
    };
    if (face - inwards).abs() < TOL {
        "inside"
    } else {
        "outside"
    }
}

/// Issues for the design's face models taller than the panel that carries
/// them: carving only reaches within the panel's own stock
/// ([`face_model_zero`]), so a model taller than the panel is cut flat at
/// the panel's thickness instead of standing at its own height. One issue
/// per attachment that does not fit, naming the panel, its face and both
/// heights.
pub fn face_model_issues(doc: &Document, design: &FurnitureDesign, gen: &Generated) -> Vec<Issue> {
    let middle = super::layout::middle_of(&gen.eval.panels);
    design
        .attachments
        .iter()
        .filter(|a| a.kind == AttachKind::Component && !a.other_side)
        .filter_map(|a| {
            let i = gen.eval.panels.iter().position(|p| p.key == a.panel)?;
            let panel = &gen.eval.panels[i];
            let component = doc.component(a.id).ok()?;
            let height = crate::model::component_cmd::top_of(component)?;
            (height > panel.thickness + TOL).then(|| {
                let word = face_side_word(panel, a.face, middle);
                Issue::on_panel(
                    format!(
                        "{} {word} face: the model is {height:.0} mm tall but the panel is \
                         {thickness:.0} mm; it will be cut flat at {thickness:.0} mm. Scale the \
                         model height to fit.",
                        panel.name,
                        thickness = panel.thickness
                    ),
                    &panel.key,
                )
            })
        })
        .collect()
}
