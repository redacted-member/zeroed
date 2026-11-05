//! The `furniture.*` commands. Every editing command is one undo step and
//! answers the design's summary (see [`super::design::summary`]).
//!
//! Reply shapes (all lengths millimetres, furniture space: X width, Y depth
//! with the front at 0, Z height):
//!
//! ```text
//! furniture.templates   -> { templates: [{ id, name, category, description, license,
//!                             author, source, credit, tags, builtIn, params }] }
//! furniture.template    -> the template JSON
//! furniture.preview     -> { panels: [{ key, name, material, thickness,
//!                             mesh: { positions, indices, normals }, box: { min, max } }],
//!                            connections: [Connection], issues: [Issue], origin, size }
//! furniture.create      -> { design, summary, revision }
//! furniture.update      -> { design, summary, revision }
//! furniture.remove      -> { revision }
//! furniture.list        -> { designs: [{ id, name, template, style, values, origin,
//!                            classJoints, connectionJoints,
//!                            made: { panel key: { vector, sheet, piece } },
//!                            models, carving }] } (models on faces, carving toolpaths)
//! furniture.importTemplate { json } / saveTemplate { design, id, name } -> { id, path }
//! furniture.removeTemplate { id } -> { id }; setLibraryFolder { path } -> { folder }
//! furniture.templates also answers the library `folder`; built-in templates come
//! first, then the maker's (builtIn false).
//! furniture.addModel { design, panel, face: "top"|"bottom", source: { clipart | file |
//!                        component }, point?: [x, y] from the part's lower left on the sheet,
//!                        fit?: { margin } } -> { design, ids, summary, revision }
//! furniture.attachments { design } -> { attachments: [{ panel, kind, id, face, faceWord, at,
//!                        stretch, otherSide }] }
//! furniture.setStretch { design, kind: "component"|"vector", id, stretch } -> { design,
//!                        revision } (one undo step)
//! furniture.panelOf { sheet, point } -> { design, panel, name, top, bottom } (nulls off a panel)
//! furniture.makeCarving { design } -> { ids, revision }
//! furniture.materials   -> { thicknesses: [{ id, label, nominal, actual, system }],
//!                            sizes: [{ id, label, width, height, system }] }
//! furniture.cutList { design } -> { rows: [{ names, panels, quantity, length, width,
//!                            thickness, material, materialLabel, grain, banding,
//!                            bandingLength }], panels }
//! furniture.hardware { design } -> { rows: [{ name, count, note }] }
//! furniture.estimate { design } -> { sheets: [{ material, label, thickness, count, area,
//!                            used, waste }], seconds, toolpaths }
//! furniture.connections -> { connections: [Connection] }
//! furniture.setJoint    -> { design, summary, revision }
//! furniture.at          -> { connection: Connection | null }
//!
//! Connection = { key, a, b, type, class, classLabel, joint, params, resolved, source,
//!                choices, made, seam: { from, to } }
//! Issue      = { message, panel?, connection?, suggest? }
//! ```
//!
//! `type` is tee, corner, cross, edge or faceOnEdges; `joint` and each of
//! `choices` a joint name (none, dado, stoppedDado, tabSlot, ...); `source`
//! style, class or connection; `made` false when the joint asked for could
//! not be made (an issue says why). Seams are in furniture space; add the
//! design's `origin` to put them in the Assembly View.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::{edit, parse_args, KernelReply};

use super::connect::{FJoint, JointChoice, Style};
use super::design::{regenerate, remove, FurnitureDesign};
use super::generate::{generate, Generated, Joined};
use super::library::Library;
use super::mesh::prism;
use super::template::{builtin, Template};

/// Whether `name` is a Furniture Designer command.
pub fn handles(name: &str) -> bool {
    name.starts_with("furniture.")
}

/// Runs one Furniture Designer command.
///
/// # Errors
/// [`CoreError::Validation`] in plain words, [`CoreError::UnknownCommand`]
/// for a name not known here.
pub fn dispatch(
    document: &mut Document,
    library: &mut Library,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "furniture.templates" => templates(library),
        "furniture.template" => template(library, parse_args(name, args)?),
        "furniture.setLibraryFolder" => {
            let args: crate::kernel::PathArgs = parse_args(name, args)?;
            library.folder = Some(args.path);
            KernelReply::ok(&json!({ "folder": library.folder }))
        }
        "furniture.importTemplate" => import_template(library, parse_args(name, args)?),
        "furniture.saveTemplate" => save_template(document, library, parse_args(name, args)?),
        "furniture.removeTemplate" => {
            let args: IdArgs = parse_args(name, args)?;
            library.remove(&args.id)?;
            KernelReply::ok(&json!({ "id": args.id }))
        }
        "furniture.materials" => materials(),
        "furniture.addModel" => super::models::add_model(document, parse_args(name, args)?),
        "furniture.attachments" => {
            let args: DesignArgs = parse_args(name, args)?;
            super::models::attachments(document, args.design)
        }
        "furniture.setStretch" => super::models::set_stretch(document, parse_args(name, args)?),
        "furniture.panelOf" => {
            let args: PanelOfArgs = parse_args(name, args)?;
            super::models::panel_of(document, args.sheet, args.point)
        }
        "furniture.makeCarving" => {
            let args: DesignArgs = parse_args(name, args)?;
            super::models::make_carving(document, args.design)
        }
        "furniture.cutList" => report(document, parse_args(name, args)?, super::report::cut_list),
        "furniture.hardware" => report(document, parse_args(name, args)?, super::report::hardware),
        "furniture.estimate" => report(document, parse_args(name, args)?, super::report::estimate),
        "furniture.preview" => preview(document, library, parse_args(name, args)?),
        "furniture.create" => create(document, library, parse_args(name, args)?),
        "furniture.update" => update(document, parse_args(name, args)?),
        "furniture.remove" => remove_design(document, parse_args(name, args)?),
        "furniture.list" => list(document),
        "furniture.connections" => connections(document, parse_args(name, args)?),
        "furniture.setJoint" => set_joint(document, parse_args(name, args)?),
        "furniture.at" => at(document, parse_args(name, args)?),
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

/// A template named by id, or given whole.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum TemplateRef {
    Id(String),
    Full(Box<Template>),
}

fn find_template(library: &Library, r: &TemplateRef) -> Result<Template, CoreError> {
    match r {
        TemplateRef::Id(id) => library
            .find(id)
            .ok_or_else(|| CoreError::validation(format!("there is no furniture template {id}"))),
        TemplateRef::Full(t) => {
            t.validate()?;
            Ok((**t).clone())
        }
    }
}

/// Joint choices sent with a command.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct JointsArg {
    style: Option<Style>,
    classes: Option<BTreeMap<String, JointChoice>>,
    connections: Option<BTreeMap<String, JointChoice>>,
}

impl JointsArg {
    fn apply(self, design: &mut FurnitureDesign) {
        if let Some(style) = self.style {
            design.style = style;
        }
        if let Some(classes) = self.classes {
            design.class_joints = classes;
        }
        if let Some(connections) = self.connections {
            design.connection_joints = connections;
        }
    }
}

/// The design `id` as stored, if any.
fn stored_design(document: &Document, id: Option<u64>) -> Option<FurnitureDesign> {
    let id = id?;
    document
        .assembly()
        .designs
        .iter()
        .find(|d| d.id == id)
        .cloned()
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

fn templates(library: &Library) -> Result<KernelReply, CoreError> {
    let rows: Vec<Value> = builtin()
        .into_iter()
        .map(|t| (t, true))
        .chain(library.user_templates().into_iter().map(|t| (t, false)))
        .map(|(t, built_in)| {
            json!({
                "id": t.id,
                "name": t.name,
                "category": t.category,
                "description": t.description,
                "license": t.license,
                "author": t.author,
                "source": t.source,
                "credit": t.credit,
                "tags": t.tags,
                "builtIn": built_in,
                "params": t.params,
            })
        })
        .collect();
    KernelReply::ok(&json!({ "templates": rows, "folder": library.folder }))
}

#[derive(Deserialize)]
struct JsonArgs {
    /// The template, as JSON text or as an object.
    json: Value,
}

fn import_template(library: &Library, args: JsonArgs) -> Result<KernelReply, CoreError> {
    let template = match &args.json {
        Value::String(text) => Template::from_json(text)?,
        other => Template::from_json(&other.to_string())?,
    };
    let path = library.save(&template)?;
    KernelReply::ok(&json!({ "id": template.id, "path": path }))
}

#[derive(Deserialize)]
struct SaveArgs {
    design: u64,
    id: String,
    name: String,
}

/// A design's template with its current values as the defaults, saved to
/// the maker's library under a new id and name.
fn save_template(
    document: &Document,
    library: &Library,
    args: SaveArgs,
) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    let mut template = design.template.clone();
    template.id = args.id.trim().to_string();
    template.name = args.name.trim().to_string();
    if template.name.is_empty() {
        return Err(CoreError::validation("the template needs a name"));
    }
    for param in &mut template.params {
        if let Some(value) = design.values.get(&param.key) {
            param.default = value.clone();
        }
    }
    let path = library.save(&template)?;
    KernelReply::ok(&json!({ "id": template.id, "path": path }))
}

/// Sheet sizes and stock thicknesses makers buy, for the thickness
/// parameters and the job's sheet size: `{ thicknesses: [{ id, label,
/// nominal, actual, system }], sizes: [{ id, label, width, height, system }] }`.
fn materials() -> Result<KernelReply, CoreError> {
    let data: Value = serde_json::from_str(include_str!("data/hardware.json"))?;
    let sheets = data["sheets"].as_array().cloned().unwrap_or_default();
    let pick = |kind: &str, fields: &[&str]| -> Vec<Value> {
        sheets
            .iter()
            .filter(|s| s["kind"] == kind)
            .map(|s| {
                let mut row = serde_json::Map::new();
                for f in fields {
                    row.insert((*f).to_string(), s[*f].clone());
                }
                Value::Object(row)
            })
            .collect()
    };
    KernelReply::ok(&json!({
        "thicknesses": pick("thickness", &["id", "label", "nominal", "actual", "system"]),
        "sizes": pick("size", &["id", "label", "width", "height", "system"]),
    }))
}

/// A report on a design: its cut list, hardware or estimate.
fn report(
    document: &Document,
    args: DesignArgs,
    make: fn(&Document, &FurnitureDesign, &Generated) -> Value,
) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    let settings = document.assembly().joint_set.settings.clone();
    let (gen, _) = generate(&design.inputs_in(document), &settings)?;
    KernelReply::ok(&make(document, &design, &gen))
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

fn template(library: &Library, args: IdArgs) -> Result<KernelReply, CoreError> {
    let t = find_template(library, &TemplateRef::Id(args.id))?;
    KernelReply::ok(&t.to_value()?)
}

/// The rows of `furniture.connections` and the preview.
fn connection_rows(template: &Template, gen: &Generated, joined: &Joined) -> Vec<Value> {
    gen.conns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (asked, source) = &gen.resolved[i];
            let outcome = joined.outcomes.get(&i);
            let made = gen.used[i].joint == asked.joint
                && (asked.joint == FJoint::None || outcome.is_some_and(|o| o.broken.is_none()));
            json!({
                "key": c.key,
                "a": gen.eval.panels[c.a].key,
                "b": gen.eval.panels[c.b].key,
                "type": c.kind,
                "class": c.class,
                "classLabel": class_label(template, &c.class),
                "joint": asked.joint,
                "params": asked.params,
                "resolved": outcome.map(|o| o.resolved.clone()),
                "source": source,
                "choices": c.choices,
                "made": made,
                "seam": { "from": c.seam[0], "to": c.seam[1] },
            })
        })
        .collect()
}

/// The label of a class: the template's, else the class name ("Other
/// joints" for connections no rule names).
fn class_label(template: &Template, class: &str) -> String {
    match template.joint_classes.get(class) {
        Some(c) => c.label.clone(),
        None if class == "other" => "Other joints".to_string(),
        None => class.to_string(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewArgs {
    #[serde(default)]
    template: Option<TemplateRef>,
    #[serde(default)]
    design: Option<u64>,
    #[serde(default)]
    values: BTreeMap<String, Value>,
    #[serde(default)]
    joints: JointsArg,
}

fn preview(
    document: &Document,
    library: &Library,
    args: PreviewArgs,
) -> Result<KernelReply, CoreError> {
    let mut design = match (args.design, &args.template) {
        (Some(id), _) => design_of(document, id)?,
        (None, Some(r)) => {
            let template = find_template(library, r)?;
            FurnitureDesign {
                id: 0,
                name: template.name.clone(),
                template,
                values: BTreeMap::new(),
                style: Style::Template,
                class_joints: BTreeMap::new(),
                connection_joints: BTreeMap::new(),
                made: BTreeMap::new(),
                sheets: Vec::new(),
                layer: 0,
                joints: BTreeMap::new(),
                toolpaths: BTreeMap::new(),
                origin: [0.0; 3],
                attachments: Vec::new(),
                carving: BTreeMap::new(),
                back: Default::default(),
            }
        }
        (None, None) => {
            return Err(CoreError::validation(
                "say which template or design to preview",
            ))
        }
    };
    for (k, v) in args.values {
        design.values.insert(k, v);
    }
    args.joints.apply(&mut design);
    let settings = document.assembly().joint_set.settings.clone();
    let (gen, joined) = generate(&design.inputs_in(document), &settings)?;
    // A design's face models, from where its panels lie now.
    let now = if design.attachments.is_empty() {
        None
    } else {
        stored_design(document, args.design).and_then(|d| {
            super::generate::lay_out(
                &d.inputs_in(document),
                &BTreeMap::new(),
                settings.tool_diameter,
            )
            .ok()
        })
    };
    let mut composites = BTreeMap::new();
    let mut panels = Vec::with_capacity(gen.eval.panels.len());
    for (i, panel) in gen.eval.panels.iter().enumerate() {
        let rings = crate::joints::geometry::path_rings(&joined.outlines[i]);
        let Some(outer) = rings.first() else {
            continue;
        };
        let mut holes: Vec<Vec<[f64; 2]>> = rings[1..].to_vec();
        for m in joined.made.iter().filter(|m| m.panel == i) {
            if matches!(m.role, crate::joints::Role::Hole) {
                holes.extend(crate::joints::geometry::path_rings(&m.path));
            }
        }
        for f in &gen.features[i] {
            if f.role == super::features::FeatureRole::Hole {
                holes.extend(crate::joints::geometry::path_rings(&f.path));
            }
        }
        let mut mesh = prism(outer, &holes, panel.thickness, &gen.frames[i].pose());
        if let Some(relief) = now.as_ref().and_then(|now| {
            super::models::face_relief(document, &design, now, &gen, i, &mut composites)
        }) {
            mesh.append(relief);
        }
        let (min, max) = mesh.bounds().unwrap_or((panel.min, panel.max));
        panels.push(json!({
            "key": panel.key,
            "name": panel.name,
            "material": panel.material,
            "thickness": panel.thickness,
            "mesh": mesh,
            "box": { "min": min, "max": max },
        }));
    }
    let (min, max) = super::layout::bounds(&gen.eval.panels);
    let mut issues = gen.issues.clone();
    issues.extend(super::models::face_model_issues(document, &design, &gen));
    KernelReply::ok(&json!({
        "panels": panels,
        "connections": connection_rows(&design.template, &gen, &joined),
        "issues": issues,
        "origin": design.origin,
        "size": [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    template: TemplateRef,
    #[serde(default)]
    values: BTreeMap<String, Value>,
    #[serde(default)]
    joints: JointsArg,
    #[serde(default)]
    name: Option<String>,
}

/// A name no other design has: the template's name, numbered.
fn fresh_name(document: &Document, base: &str) -> String {
    let taken = |n: &str| document.assembly().designs.iter().any(|d| d.name == n);
    (1..)
        .map(|k| format!("{base} {k}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| base.to_string())
}

fn create(
    document: &mut Document,
    library: &Library,
    args: CreateArgs,
) -> Result<KernelReply, CoreError> {
    let template = find_template(library, &args.template)?;
    edit(document, "New Furniture", |doc| {
        let name = match args.name.map(|n| n.trim().to_string()) {
            Some(n) if !n.is_empty() => n,
            _ => fresh_name(doc, &template.name),
        };
        let mut assembly = doc.assembly().clone();
        let id = assembly.take_id();
        doc.set_assembly(assembly);
        let mut design = FurnitureDesign {
            id,
            name,
            template: template.clone(),
            values: args.values,
            style: Style::Template,
            class_joints: BTreeMap::new(),
            connection_joints: BTreeMap::new(),
            made: BTreeMap::new(),
            sheets: Vec::new(),
            layer: 0,
            joints: BTreeMap::new(),
            toolpaths: BTreeMap::new(),
            origin: [0.0; 3],
            attachments: Vec::new(),
            carving: BTreeMap::new(),
            back: Default::default(),
        };
        args.joints.apply(&mut design);
        let summary = regenerate(doc, &mut design)?;
        Ok(json!({ "design": id, "summary": summary }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArgs {
    design: u64,
    #[serde(default)]
    values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    joints: JointsArg,
    #[serde(default)]
    style: Option<Style>,
    #[serde(default)]
    name: Option<String>,
}

fn update(document: &mut Document, args: UpdateArgs) -> Result<KernelReply, CoreError> {
    let mut design = design_of(document, args.design)?;
    // What was done by hand is read against the design as it was made,
    // before the new values change it.
    super::design::take_in(document, &mut design);
    edit(document, "Edit Furniture", |doc| {
        if let Some(values) = args.values {
            for (k, v) in values {
                design.values.insert(k, v);
            }
        }
        args.joints.apply(&mut design);
        if let Some(style) = args.style {
            design.style = style;
        }
        if let Some(name) = args
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
        {
            design.name = name;
        }
        let summary = regenerate(doc, &mut design)?;
        Ok(json!({ "design": design.id, "summary": summary }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveArgs {
    design: u64,
    #[serde(default)]
    keep_vectors: bool,
}

fn remove_design(document: &mut Document, args: RemoveArgs) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    edit(document, "Remove Furniture", |doc| {
        remove(doc, &design, args.keep_vectors)?;
        Ok(json!({}))
    })
}

fn list(document: &Document) -> Result<KernelReply, CoreError> {
    let rows: Vec<Value> = document
        .assembly()
        .designs
        .iter()
        .map(|d| {
            let made: serde_json::Map<String, Value> = d
                .made
                .iter()
                .map(|(key, m)| {
                    (
                        key.clone(),
                        json!({ "vector": m.vector, "sheet": m.sheet, "piece": m.piece }),
                    )
                })
                .collect();
            json!({
                "id": d.id,
                "name": d.name,
                "template": d.template.id,
                "style": d.style,
                "values": d.values,
                "origin": d.origin,
                "classJoints": d.class_joints,
                "connectionJoints": d.connection_joints,
                "made": made,
                "models": d.attachments.iter().filter(|a| a.kind == super::attach::AttachKind::Component).count(),
                "carving": d.carving.values().map(Vec::len).sum::<usize>(),
            })
        })
        .collect();
    KernelReply::ok(&json!({ "designs": rows }))
}

#[derive(Deserialize)]
struct DesignArgs {
    design: u64,
}

#[derive(Deserialize)]
struct PanelOfArgs {
    sheet: u64,
    /// Job millimetres on that sheet.
    point: [f64; 2],
}

fn connections(document: &Document, args: DesignArgs) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    let settings = document.assembly().joint_set.settings.clone();
    let (gen, joined) = generate(&design.inputs_in(document), &settings)?;
    KernelReply::ok(&json!({ "connections": connection_rows(&design.template, &gen, &joined) }))
}

/// Where a joint choice applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Scope {
    Connection,
    Class,
    Style,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetJointArgs {
    design: u64,
    scope: Scope,
    /// The connection key, the class, or the style.
    key: String,
    /// `null` drops the override.
    #[serde(default)]
    joint: Option<FJoint>,
    #[serde(default)]
    params: Option<crate::joints::JointParams>,
}

fn set_joint(document: &mut Document, args: SetJointArgs) -> Result<KernelReply, CoreError> {
    let mut design = design_of(document, args.design)?;
    super::design::take_in(document, &mut design);
    match args.scope {
        Scope::Style => {
            design.style = serde_json::from_value(json!(args.key)).map_err(|_| {
                CoreError::validation(format!("there is no joinery style {}", args.key))
            })?;
        }
        Scope::Class | Scope::Connection => {
            let map = if args.scope == Scope::Class {
                &mut design.class_joints
            } else {
                &mut design.connection_joints
            };
            match args.joint {
                Some(joint) => {
                    map.insert(
                        args.key.clone(),
                        JointChoice {
                            joint,
                            params: args.params.unwrap_or_default(),
                        },
                    );
                }
                None => {
                    map.remove(&args.key);
                }
            }
        }
    }
    edit(document, "Change Joint", |doc| {
        let summary = regenerate(doc, &mut design)?;
        Ok(json!({ "design": design.id, "summary": summary }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AtArgs {
    design: u64,
    /// The pick ray's start, Assembly View millimetres.
    point: [f64; 3],
    /// Its direction.
    direction: [f64; 3],
}

/// How far a pick ray may pass from a seam and still pick it, millimetres.
pub const PICK_REACH: f64 = 8.0;

/// The distance between the ray `p + s d` (s >= 0) and segment `a b`, and
/// how far along the ray (in lengths of `d`) the nearest point lies.
pub fn ray_segment_distance(p: [f64; 3], d: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, f64) {
    use crate::assembly::pose::{add, dot, scale, sub};
    let dd = dot(d, d);
    if dd < 1e-12 {
        return (f64::INFINITY, 0.0);
    }
    let e = sub(b, a);
    let ee = dot(e, e);
    let w0 = sub(p, a);
    let (de, dw, ew) = (dot(d, e), dot(d, w0), dot(e, w0));
    let denom = dd * ee - de * de;
    let mut t = if ee < 1e-12 || denom.abs() < 1e-12 {
        0.0
    } else {
        ((dd * ew - de * dw) / denom).clamp(0.0, 1.0)
    };
    let mut s = (dot(d, add(a, scale(e, t))) - dot(d, p)) / dd;
    if s < 0.0 {
        s = 0.0;
        t = if ee < 1e-12 {
            0.0
        } else {
            (ew / ee).clamp(0.0, 1.0)
        };
    }
    let q = add(p, scale(d, s));
    let r = add(a, scale(e, t));
    let v = sub(q, r);
    (dot(v, v).sqrt(), s)
}

fn at(document: &Document, args: AtArgs) -> Result<KernelReply, CoreError> {
    let design = design_of(document, args.design)?;
    let settings = document.assembly().joint_set.settings.clone();
    let (gen, joined) = generate(&design.inputs_in(document), &settings)?;
    let o = design.origin;
    let shift = |p: [f64; 3]| [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
    // Of the seams the ray passes within reach of, the first along it (the
    // one in front, as the view shows it).
    let best = gen
        .conns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                ray_segment_distance(
                    args.point,
                    args.direction,
                    shift(c.seam[0]),
                    shift(c.seam[1]),
                ),
                i,
            )
        })
        .filter(|((d, _), _)| *d <= PICK_REACH)
        .min_by(|a, b| a.0 .1.total_cmp(&b.0 .1).then(a.0 .0.total_cmp(&b.0 .0)));
    let rows = connection_rows(&design.template, &gen, &joined);
    let row = best.and_then(|(_, i)| rows.get(i).cloned());
    KernelReply::ok(&json!({ "connection": row }))
}
