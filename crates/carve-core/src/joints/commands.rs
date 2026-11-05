//! The `joints.*` commands of the Joint Creator panel.
//!
//! Every change (add, edit, remove, settings, box) is one undo step that
//! puts the vectors back as they were before any joint and applies every
//! joint again. `joints.preview` does the same inside a step it throws
//! away, so the panel shows exactly what Create would make.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::assembly::joint_mates;
use crate::assembly::mates::{MateFace, PieceMate, PieceMateKind};
use crate::assembly::pieces::{pieces, PartsCache, PieceInfo};
use crate::assembly::pieces_cmd;
use crate::assembly::pose as pose3;
use crate::cam::sim::Quality;
use crate::doc::{Document, EntityKind};
use crate::error::CoreError;
use crate::geom::{Path, Segment};
use crate::kernel::{edit, parse_args, KernelReply};

use super::geometry::{area2, cross2, dot, inside_rings, length, path_rings, sub, unit, Frame, P2};
use super::{
    delete_made, rebuild, EdgeLine, Joint, JointKind, JointParams, JointSet, JointSettings,
    Outcome, Target,
};

/// Whether `name` is a Joint Creator command.
pub fn handles(name: &str) -> bool {
    name.starts_with("joints.")
}

/// Runs one Joint Creator command.
///
/// # Errors
/// [`CoreError::Validation`] in plain words for anything the joint cannot
/// be made from, [`CoreError::UnknownCommand`] for a name not known here.
pub fn dispatch(
    document: &mut Document,
    cache: &mut PartsCache,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "joints.assemble" => assemble(document, cache, parse_args(name, args)?),
        "joints.list" => list(document),
        "joints.pickEdge" => pick_edge(document, parse_args(name, args)?),
        "joints.pickFace" => pick_face(document, parse_args(name, args)?),
        "joints.at" => joint_at(document, parse_args(name, args)?),
        "joints.preview" => preview(document, parse_args(name, args)?),
        "joints.add" => add(document, cache, parse_args(name, args)?),
        "joints.update" => update(document, cache, parse_args(name, args)?),
        "joints.remove" => remove(document, cache, parse_args(name, args)?),
        "joints.setSettings" => set_settings(document, cache, parse_args(name, args)?),
        "joints.box" => make_box(document, cache, parse_args(name, args)?),
        "joints.makeToolpaths" => make_toolpaths(document, cache, parse_args(name, args)?),
        "joints.coupon" => coupon(document, cache, parse_args(name, args)?),
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

fn joint_set(document: &Document) -> JointSet {
    document.assembly().joint_set.clone()
}

fn store(document: &mut Document, set: JointSet) {
    let mut assembly = document.assembly().clone();
    if assembly.joint_set != set {
        assembly.joint_set = set;
        document.set_assembly(assembly);
    }
}

/// Rebuilds `set` on the document and stores it, answering the outcomes.
fn apply(document: &mut Document, mut set: JointSet) -> Result<Vec<Outcome>, CoreError> {
    let outcomes = rebuild(document, &mut set, true)?;
    for (joint, outcome) in set.joints.iter_mut().zip(&outcomes) {
        joint.resolved = outcome.resolved.clone();
        joint.b_to_a = outcome.b_to_a;
    }
    store(document, set);
    Ok(outcomes)
}

fn outcome_of(outcomes: &[Outcome], id: u64) -> Value {
    outcomes
        .iter()
        .find(|o| o.id == id)
        .map_or(Value::Null, |o| json!(o))
}

// ------------------------------------------------------------------ list

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JointRow<'a> {
    #[serde(flatten)]
    joint: &'a Joint,
    /// Part A's and part B's vectors.
    parts: [u64; 2],
    /// Whether both parts still exist.
    ok: bool,
}

fn list(document: &Document) -> Result<KernelReply, CoreError> {
    let set = &document.assembly().joint_set;
    let rows: Vec<JointRow> = set
        .joints
        .iter()
        .map(|j| JointRow {
            joint: j,
            parts: [j.a.vector, j.b.vector()],
            ok: document.entity(j.a.vector).is_ok() && document.entity(j.b.vector()).is_ok(),
        })
        .collect();
    let sheets: Vec<Value> = document
        .sheets()
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "thickness": set.settings.thickness_of(document, s.id),
                "nominal": document.sheet_material(s.id).map(|m| m.thickness).unwrap_or(0.0),
            })
        })
        .collect();
    KernelReply::ok(&json!({
        "settings": set.settings,
        "clearance": set.settings.clearance(),
        "joints": rows,
        "sheets": sheets,
    }))
}

// ------------------------------------------------------------------ picks

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PointArgs {
    x: f64,
    y: f64,
    #[serde(default = "default_tolerance")]
    tolerance: f64,
    /// Part A's edge, for a face pick: A's middle then goes where it fits.
    #[serde(default)]
    a: Option<EdgeLine>,
}

fn default_tolerance() -> f64 {
    2.0
}

/// A straight edge of a closed vector, with the part on its left.
#[derive(Debug, Clone, Copy)]
struct Edge {
    vector: u64,
    from: P2,
    to: P2,
}

fn is_straight(segment: &Segment) -> bool {
    match segment {
        Segment::Line { .. } => true,
        Segment::Arc { bulge, .. } => bulge.abs() < 1e-6,
        _ => false,
    }
}

/// The straight edges of a path's closed subpaths, each turned so the
/// material of `rings` lies on its left.
fn straight_edges(vector: u64, path: &Path, rings: &[Vec<P2>]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for sub_path in path.subpaths.iter().filter(|s| s.closed) {
        for (start, segment) in sub_path.iter() {
            if !is_straight(segment) {
                continue;
            }
            let end = segment.end();
            let (from, to) = ([start.x, start.y], [end.x, end.y]);
            if length(sub(to, from)) < 1.0 {
                continue;
            }
            // The frame runs with the part on its left.
            let frame = Frame::of_edge(from, to, rings);
            let (from, to) = (frame.a, frame.at(frame.len, 0.0));
            edges.push(Edge { vector, from, to });
        }
    }
    edges
}

fn segment_distance(p: P2, a: P2, b: P2) -> (f64, f64) {
    let d = sub(b, a);
    let len2 = dot(d, d);
    let t = if len2 < 1e-12 {
        0.0
    } else {
        (dot(sub(p, a), d) / len2).clamp(0.0, 1.0)
    };
    let q = [a[0] + d[0] * t, a[1] + d[1] * t];
    (length(sub(p, q)), t)
}

/// Every pickable closed vector with its path and rings.
fn parts(document: &Document) -> Vec<(u64, Path, Vec<Vec<P2>>)> {
    document
        .order()
        .iter()
        .filter(|id| document.is_pickable(**id))
        .filter_map(|id| match &document.entity(*id).ok()?.kind {
            EntityKind::Path { path } => {
                let rings = path_rings(path);
                (!rings.is_empty()).then(|| (*id, path.clone(), rings))
            }
            _ => None,
        })
        .collect()
}

fn edge_json(document: &Document, e: &Edge) -> Value {
    let set = &document.assembly().joint_set;
    let entity = document.entity(e.vector).ok();
    let sheet = entity.map_or(0, |e| e.sheet);
    json!({
        "vector": e.vector,
        "from": e.from,
        "to": e.to,
        "length": length(sub(e.to, e.from)),
        "sheet": sheet,
        "name": entity.and_then(|e| e.name.clone()),
        "thickness": set.settings.thickness_of(document, sheet),
        "joint": joint_on(set, e.vector, e.from, e.to, 0.5),
    })
}

/// The joint whose edges run along the line `from`-`to` of `vector`.
fn joint_on(set: &JointSet, vector: u64, from: P2, to: P2, tol: f64) -> Option<u64> {
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    set.joints
        .iter()
        .find(|j| {
            [j.a, j.b.edge()].iter().any(|e| {
                e.vector == vector
                    && segment_distance(mid, e.from, e.to).0 < tol
                    && segment_distance(from, e.from, e.to).0 < tol
            })
        })
        .map(|j| j.id)
}

fn nearest_edge(document: &Document, at: P2, tolerance: f64) -> Option<Edge> {
    let mut best: Option<(f64, Edge)> = None;
    for (id, path, rings) in parts(document) {
        for edge in straight_edges(id, &path, &rings) {
            let (d, _) = segment_distance(at, edge.from, edge.to);
            if d <= tolerance && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, edge));
            }
        }
    }
    best.map(|(_, e)| e)
}

fn pick_edge(document: &Document, args: PointArgs) -> Result<KernelReply, CoreError> {
    let at = [args.x, args.y];
    match nearest_edge(document, at, args.tolerance) {
        Some(edge) => {
            let mut found = edge_json(document, &edge);
            // Where along the edge the click was, from its start.
            let d = sub(edge.to, edge.from);
            let at = dot(sub(at, edge.from), d) / length(d).max(1e-12);
            found["at"] = json!(at.clamp(0.0, length(d)));
            KernelReply::ok(&json!({ "edge": found }))
        }
        None => {
            // Say why: inside a part, or not near any closed vector.
            let inside = parts(document)
                .iter()
                .any(|(_, _, rings)| inside_rings(rings, at));
            let reason = if inside {
                "click on a straight edge of the part, not inside it"
            } else {
                "there is no straight edge of a closed vector here"
            };
            KernelReply::ok(&json!({ "edge": null, "reason": reason }))
        }
    }
}

fn pick_face(document: &Document, args: PointArgs) -> Result<KernelReply, CoreError> {
    let at = [args.x, args.y];
    // The smallest closed vector around the point.
    let host = parts(document)
        .into_iter()
        .filter(|(id, _, rings)| inside_rings(rings, at) && args.a.is_none_or(|a| a.vector != *id))
        .min_by(|x, y| area2(&x.2).abs().total_cmp(&area2(&y.2).abs()));
    let Some((id, path, rings)) = host else {
        return KernelReply::ok(&json!({
            "face": null,
            "reason": "click inside the second part, where the first part stands on it",
        }));
    };
    let edge = straight_edges(id, &path, &rings)
        .into_iter()
        .min_by(|x, y| {
            segment_distance(at, x.from, x.to)
                .0
                .total_cmp(&segment_distance(at, y.from, y.to).0)
        });
    let Some(edge) = edge else {
        return KernelReply::ok(&json!({
            "face": null,
            "reason": "the second part has no straight edge to measure from",
        }));
    };
    let frame = Frame::of_edge(edge.from, edge.to, &rings);
    let local = frame.local(at);
    // Clicked places go to the nearest half millimetre (1/32 inch in an
    // inch job), so the same place clicked on two parts drawn alike is the
    // same place and a frame of parts (legs, a top, a stretcher) closes.
    let step = match document.job().units {
        crate::doc::Units::Mm => 0.5,
        crate::doc::Units::Inch => 25.4 / 32.0,
    };
    let snap = |v: f64| (v / step).round() * step;
    let offset = snap((-local[1]).max(0.0));
    let mut along = snap(local[0]).clamp(0.0, frame.len);
    if let Some(a) = args.a {
        let la = length(sub(a.to, a.from));
        // A as long as the part is deep sits in the middle of it.
        let chord = super::geometry::chord(&rings, &frame, -offset.max(0.01), along);
        if let Some((c0, c1)) = chord {
            if (la - (c1 - c0)).abs() < 1.0 {
                along = (c0 + c1) / 2.0;
            } else {
                along = along.clamp(c0 + la / 2.0, (c1 - la / 2.0).max(c0 + la / 2.0));
            }
        }
    }
    KernelReply::ok(&json!({
        "face": {
            "edge": edge_json(document, &edge),
            "offset": offset,
            "along": along,
        }
    }))
}

fn joint_at(document: &Document, args: PointArgs) -> Result<KernelReply, CoreError> {
    let at = [args.x, args.y];
    let set = &document.assembly().joint_set;
    let hit = set.joints.iter().find(|j| {
        let near = |e: &EdgeLine| segment_distance(at, e.from, e.to).0 <= args.tolerance;
        let face_near = match j.b {
            Target::Face {
                edge,
                offset,
                along,
            } => {
                let Ok(rings) = super::part_rings(document, edge.vector) else {
                    return false;
                };
                let frame = Frame::of_edge(edge.from, edge.to, &rings);
                let l = frame.local(at);
                let la = length(sub(j.a.to, j.a.from));
                (l[1] + offset).abs() < args.tolerance + 20.0
                    && (l[0] - along).abs() <= la / 2.0 + args.tolerance
                    && -l[1] >= offset - args.tolerance
            }
            Target::Edge(e) => near(&e),
        };
        near(&j.a) || face_near
    });
    KernelReply::ok(&json!({ "joint": hit.map(|j| j.id) }))
}

// ------------------------------------------------------------------ joints

/// A joint as the panel sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JointSpec {
    kind: JointKind,
    a: EdgeLine,
    b: Target,
    #[serde(default)]
    params: JointParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewArgs {
    #[serde(flatten)]
    spec: JointSpec,
    /// The joint being edited, which the preview replaces.
    #[serde(default)]
    id: Option<u64>,
}

fn with_spec(
    document: &Document,
    set: &mut JointSet,
    spec: JointSpec,
    id: Option<u64>,
) -> Result<u64, CoreError> {
    let id = match id {
        Some(id) => {
            let joint = set
                .joints
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(|| CoreError::validation(format!("there is no joint {id}")))?;
            joint.kind = spec.kind;
            joint.a = spec.a;
            joint.b = spec.b;
            joint.params = spec.params;
            id
        }
        None => {
            let id = set.take_id();
            set.joints.push(Joint {
                id,
                kind: spec.kind,
                a: spec.a,
                b: spec.b,
                params: spec.params,
                vectors: Vec::new(),
                resolved: JointParams::default(),
                b_to_a: None,
                flip: false,
            });
            id
        }
    };
    settle_flip(document, set, id);
    Ok(id)
}

/// B's part space to A's from a column major 4 by 4 matrix.
fn pose_of_matrix(m: &[f64; 16]) -> pose3::Pose {
    pose3::Pose {
        r: [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]],
        t: [m[12], m[13], m[14]],
    }
}

/// The turn between two rotations, degrees.
fn turn_between(a: &pose3::Pose, b: &pose3::Pose) -> f64 {
    let mut trace = 0.0;
    for i in 0..3 {
        for k in 0..3 {
            trace += a.r[k][i] * b.r[k][i];
        }
    }
    ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Where the other joints hold B against A: B's part space to A's along a
/// chain of joints from B to A, or `None` when no chain joins them.
fn chain_b_to_a(set: &JointSet, id: u64, a: u64, b: u64) -> Option<pose3::Pose> {
    let edges: Vec<(u64, u64, pose3::Pose)> = set
        .joints
        .iter()
        .filter(|j| j.id != id && !j.kind.one_part())
        .filter_map(|j| Some((j.a.vector, j.b.vector(), pose_of_matrix(j.b_to_a.as_ref()?))))
        .collect();
    // Each part reached, with B's part space to its own.
    let mut reached: Vec<(u64, pose3::Pose)> = vec![(b, pose3::Pose::IDENTITY)];
    let mut k = 0;
    while k < reached.len() {
        let (at, from_b) = reached[k];
        if at == a {
            return Some(from_b);
        }
        for (ja, jb, t) in &edges {
            let next = if *jb == at {
                Some((*ja, t.then_local(&from_b)))
            } else if *ja == at {
                Some((*jb, t.inverse().then_local(&from_b)))
            } else {
                None
            };
            if let Some((part, pose)) = next {
                if !reached.iter().any(|(p, _)| *p == part) {
                    reached.push((part, pose));
                }
            }
        }
        k += 1;
    }
    None
}

/// A joint on B's face can stand A either way round (see
/// [`super::kinds`]); when other joints already hold A and B together
/// (two legs under a top, the stretcher going in between), the way that
/// agrees with them, else the plain way.
fn settle_flip(document: &Document, set: &mut JointSet, id: u64) {
    let Some(joint) = set.joints.iter().find(|j| j.id == id).cloned() else {
        return;
    };
    let flip = joint.kind.on_face()
        && chain_b_to_a(set, id, joint.a.vector, joint.b.vector()).is_some_and(|chain| {
            let turned = |flip: bool| -> Option<f64> {
                let fa = super::edge_frame(document, &set.originals, &joint.a).ok()?;
                let fb = super::edge_frame(document, &set.originals, &joint.b.edge()).ok()?;
                let map = super::kinds::tee_map_at(fa.len, 0.0, 0.0, 0.0, 0.0, 0.0, flip);
                Some(turn_between(&super::pose::b_to_a(&fa, &fb, &map), &chain))
            };
            matches!((turned(false), turned(true)), (Some(plain), Some(other)) if other + 1.0 < plain)
        });
    if let Some(j) = set.joints.iter_mut().find(|j| j.id == id) {
        j.flip = flip;
    }
}

fn paths_json(document: &Document, ids: &[u64]) -> Vec<Value> {
    ids.iter()
        .filter_map(|id| match &document.entity(*id).ok()?.kind {
            EntityKind::Path { path } => Some(json!({ "vector": id, "path": path })),
            _ => None,
        })
        .collect()
}

fn preview(document: &mut Document, args: PreviewArgs) -> Result<KernelReply, CoreError> {
    let mut set = joint_set(document);
    let id = with_spec(document, &mut set, args.spec, args.id)?;
    document.begin("Joint preview");
    let result = (|| -> Result<Value, CoreError> {
        let outcomes = rebuild(document, &mut set, true)?;
        let joint = set.joint(id)?.clone();
        let parts = [joint.a.vector, joint.b.vector()];
        let made: Vec<u64> = joint.vectors.iter().map(|v| v.id).collect();
        let settings = set.settings.clone();
        let rings_a = super::part_rings(document, parts[0])?;
        let rings_b = super::part_rings(document, parts[1])?;
        let made_rows: Vec<Value> = joint
            .vectors
            .iter()
            .filter_map(|v| {
                let e = document.entity(v.id).ok()?;
                let EntityKind::Path { path } = &e.kind else {
                    return None;
                };
                Some(json!({ "vector": v.id, "role": v.role, "path": path }))
            })
            .collect();
        Ok(json!({
            "outcome": outcome_of(&outcomes, id),
            "parts": paths_json(document, &parts),
            "made": made_rows,
            "madeIds": made,
            "solid": {
                "a": { "rings": rings_a, "thickness": settings.thickness_of(document, sheet_of(document, parts[0])) },
                "b": { "rings": rings_b, "thickness": settings.thickness_of(document, sheet_of(document, parts[1])) },
                "bToA": outcomes.iter().find(|o| o.id == id).and_then(|o| o.b_to_a),
            },
        }))
    })();
    document.rollback();
    KernelReply::ok(&result?)
}

fn sheet_of(document: &Document, id: u64) -> u64 {
    document.entity(id).map_or(0, |e| e.sheet)
}

fn add(
    document: &mut Document,
    cache: &mut PartsCache,
    spec: JointSpec,
) -> Result<KernelReply, CoreError> {
    edit(document, "Add Joint", |doc| {
        let mut set = joint_set(doc);
        let id = with_spec(doc, &mut set, spec, None)?;
        let outcomes = apply(doc, set)?;
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "id": id, "outcome": outcome_of(&outcomes, id), "mates": mates }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArgs {
    id: u64,
    #[serde(flatten)]
    spec: JointSpec,
}

fn update(
    document: &mut Document,
    cache: &mut PartsCache,
    args: UpdateArgs,
) -> Result<KernelReply, CoreError> {
    edit(document, "Edit Joint", |doc| {
        let mut set = joint_set(doc);
        let id = with_spec(doc, &mut set, args.spec, Some(args.id))?;
        let outcomes = apply(doc, set)?;
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "id": id, "outcome": outcome_of(&outcomes, id), "mates": mates }))
    })
}

#[derive(Deserialize)]
struct IdArgs {
    id: u64,
}

fn remove(
    document: &mut Document,
    cache: &mut PartsCache,
    args: IdArgs,
) -> Result<KernelReply, CoreError> {
    edit(document, "Remove Joint", |doc| {
        let mut set = joint_set(doc);
        let joint = set.joint(args.id)?.clone();
        set.joints.retain(|j| j.id != args.id);
        delete_made(doc, &joint);
        // Everything goes back, then the others are made again. A part
        // deleted meanwhile does not stop its joint from going.
        let outcomes = rebuild(doc, &mut set, false)?;
        for (joint, outcome) in set.joints.iter_mut().zip(&outcomes) {
            joint.resolved = outcome.resolved.clone();
            joint.b_to_a = outcome.b_to_a;
        }
        store(doc, set);
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "removed": args.id, "mates": mates }))
    })
}

#[derive(Deserialize)]
struct SettingsArgs {
    settings: JointSettings,
}

fn set_settings(
    document: &mut Document,
    cache: &mut PartsCache,
    args: SettingsArgs,
) -> Result<KernelReply, CoreError> {
    args.settings.validate()?;
    edit(document, "Joint Settings", |doc| {
        let mut set = joint_set(doc);
        set.settings = args.settings;
        let outcomes = apply(doc, set)?;
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "joints": outcomes.len(), "mates": mates }))
    })
}

// ------------------------------------------------------------------ box

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoxArgs {
    /// The panels, most important first (it keeps its corners).
    ids: Vec<u64>,
    #[serde(default = "default_box_kind")]
    kind: JointKind,
    #[serde(default)]
    params: JointParams,
    /// Only work out the pairs, change nothing.
    #[serde(default)]
    dry_run: bool,
}

fn default_box_kind() -> JointKind {
    JointKind::Finger
}

/// Pairs the matching edges of the panels of a box: edges of the same
/// length on different panels, each panel meeting each other panel once,
/// the closest pairs in the layout first. The panel listed first of each
/// pair is part A, so each three way corner has one owner.
/// The edge pairs of a box made of the closed vectors `ids`. With `fold`
/// (panels meeting square), the box is folded up in 3D from the first
/// panel, so each corner joins the edges that meet there whatever way the
/// panels were laid out; otherwise edges pair by length and nearness.
fn box_pairs(document: &Document, ids: &[u64], fold: bool) -> Result<Vec<(Edge, Edge)>, CoreError> {
    let mut edges: Vec<(usize, Edge)> = Vec::new();
    let mut rings: Vec<Vec<Vec<P2>>> = Vec::new();
    for (rank, id) in ids.iter().enumerate() {
        let entity = document.entity(*id)?;
        let EntityKind::Path { path } = &entity.kind else {
            return Err(CoreError::validation(
                "a box is made of closed vectors only",
            ));
        };
        let panel = path_rings(path);
        if panel.is_empty() {
            return Err(CoreError::validation(
                "a box is made of closed vectors only",
            ));
        }
        for e in straight_edges(*id, path, &panel) {
            if length(sub(e.to, e.from)) >= 5.0 {
                edges.push((rank, e));
            }
        }
        rings.push(panel);
    }
    let mid = |e: &Edge| [(e.from[0] + e.to[0]) / 2.0, (e.from[1] + e.to[1]) / 2.0];
    let len = |e: &Edge| length(sub(e.to, e.from));
    let mut candidates: Vec<(f64, f64, usize, usize)> = Vec::new();
    for i in 0..edges.len() {
        for j in 0..edges.len() {
            let (ri, ei) = &edges[i];
            let (rj, ej) = &edges[j];
            if ri >= rj {
                continue;
            }
            let diff = (len(ei) - len(ej)).abs();
            if diff <= 0.5 {
                candidates.push((diff, length(sub(mid(ei), mid(ej))), i, j));
            }
        }
    }
    candidates.sort_by(|x, y| {
        (x.0 * 10.0)
            .round()
            .total_cmp(&(y.0 * 10.0).round())
            .then(x.1.total_cmp(&y.1))
            .then(x.2.cmp(&y.2))
            .then(x.3.cmp(&y.3))
    });
    if fold {
        return found_pairs(folded_pairs(&edges, &rings, &candidates, ids.len()));
    }
    let mut used = vec![false; edges.len()];
    // Each pair made: the two panels and the edge each uses.
    let mut met: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut pairs = Vec::new();
    // Two panels joined to a third along its opposite (parallel, apart)
    // edges are the box's opposite faces, which never meet.
    let opposite = |met: &[(usize, usize, usize, usize)], x: usize, y: usize| {
        let on = |panel: usize, other: usize| {
            met.iter().find_map(|&(p, q, ep, eq)| {
                if p == panel && q == other {
                    Some(ep)
                } else if q == panel && p == other {
                    Some(eq)
                } else {
                    None
                }
            })
        };
        (0..ids.len()).any(|z| match (on(z, x), on(z, y)) {
            (Some(ex), Some(ey)) => {
                let (a, b) = (&edges[ex].1, &edges[ey].1);
                let da = sub(a.to, a.from);
                let db = sub(b.to, b.from);
                let parallel = cross2(unit(da), unit(db)).abs() < 0.05;
                parallel && cross2(unit(db), sub(a.from, b.from)).abs() > 1.0
            }
            _ => false,
        })
    };
    for (_, _, i, j) in candidates {
        let (ri, rj) = (edges[i].0, edges[j].0);
        if used[i]
            || used[j]
            || met.iter().any(|m| m.0 == ri && m.1 == rj)
            || opposite(&met, ri, rj)
        {
            continue;
        }
        used[i] = true;
        used[j] = true;
        met.push((ri, rj, i, j));
        pairs.push((edges[i].1, edges[j].1));
    }
    found_pairs(pairs)
}

fn found_pairs(pairs: Vec<(Edge, Edge)>) -> Result<Vec<(Edge, Edge)>, CoreError> {
    if pairs.is_empty() {
        return Err(CoreError::validation(
            "no edges of the same length were found on the selected parts; the panels of a box need matching edges",
        ));
    }
    Ok(pairs)
}

/// Box pairs by folding: the box is folded up from each panel in turn and
/// the fold that brings the most edges together wins (the first panel's
/// on a tie). See [`fold_from`].
fn folded_pairs(
    edges: &[(usize, Edge)],
    rings: &[Vec<Vec<P2>>],
    candidates: &[(f64, f64, usize, usize)],
    panels: usize,
) -> Vec<(Edge, Edge)> {
    let mut best: Vec<(usize, usize)> = Vec::new();
    for root in 0..panels {
        let pairs = fold_from(root, edges, rings, candidates, panels);
        if pairs.len() > best.len() {
            best = pairs;
        }
    }
    best.into_iter()
        .map(|(i, j)| (edges[i].1, edges[j].1))
        .collect()
}

/// One fold from panel `root`: each placed panel in turn (breadth first)
/// takes the panels not yet placed at its free edges, the nearest of the
/// same length first (`candidates` in order), each standing on its edge
/// square as a corner joint puts it; then every two free edges the fold
/// brings onto one another meet too. Answers the edge index pairs, the
/// first panel's edge first.
fn fold_from(
    root: usize,
    edges: &[(usize, Edge)],
    rings: &[Vec<Vec<P2>>],
    candidates: &[(f64, f64, usize, usize)],
    panels: usize,
) -> Vec<(usize, usize)> {
    let frame = |i: usize| {
        let (rank, e) = &edges[i];
        Frame::of_edge(e.from, e.to, &rings[*rank])
    };
    // Edge `j`'s panel in edge `i`'s panel space, `i` the first part.
    let b_in_a = |i: usize, j: usize| {
        let fa = frame(i);
        super::pose::b_to_a(&fa, &frame(j), &super::kinds::corner_map(fa.len))
    };
    let mut placed: Vec<Option<pose3::Pose>> = vec![None; panels];
    placed[root] = Some(pose3::Pose::IDENTITY);
    let mut order = vec![root];
    let mut used = vec![false; edges.len()];
    let mut pairs = Vec::new();
    let mut met: Vec<(usize, usize)> = Vec::new();
    let mut k = 0;
    while k < order.len() {
        let at = order[k];
        loop {
            let next = candidates.iter().find(|&&(_, _, i, j)| {
                let (ri, rj) = (edges[i].0, edges[j].0);
                !used[i]
                    && !used[j]
                    && ((ri == at && placed[rj].is_none()) || (rj == at && placed[ri].is_none()))
            });
            let Some(&(_, _, i, j)) = next else {
                break;
            };
            let (ri, rj) = (edges[i].0, edges[j].0);
            // Candidates list the first panel's edge first (ri < rj).
            if let Some(pi) = placed[ri] {
                placed[rj] = Some(pi.then_local(&b_in_a(i, j)));
                order.push(rj);
            } else if let Some(pj) = placed[rj] {
                placed[ri] = Some(pj.then_local(&b_in_a(i, j).inverse()));
                order.push(ri);
            }
            used[i] = true;
            used[j] = true;
            met.push((ri, rj));
            pairs.push((i, j));
        }
        k += 1;
    }
    // Free edges the fold brings onto one another (ends swapped, as the
    // corner turns B's edge against A's).
    let ends = |i: usize| {
        let (rank, e) = &edges[i];
        placed[*rank].map(|p| {
            (
                p.apply([e.from[0], e.from[1], 0.0]),
                p.apply([e.to[0], e.to[1], 0.0]),
            )
        })
    };
    let mut meeting: Vec<(f64, usize, usize)> = Vec::new();
    for &(_, _, i, j) in candidates {
        let (Some((ai, bi)), Some((aj, bj))) = (ends(i), ends(j)) else {
            continue;
        };
        let miss = length3(ai, bj).max(length3(bi, aj));
        if miss <= 1.0 {
            meeting.push((miss, i, j));
        }
    }
    meeting.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (_, i, j) in meeting {
        let (ri, rj) = (edges[i].0, edges[j].0);
        if used[i] || used[j] || met.contains(&(ri, rj)) {
            continue;
        }
        used[i] = true;
        used[j] = true;
        met.push((ri, rj));
        pairs.push((i, j));
    }
    pairs
}

fn length3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = pose3::sub(a, b);
    pose3::dot(d, d).sqrt()
}

fn make_box(
    document: &mut Document,
    cache: &mut PartsCache,
    args: BoxArgs,
) -> Result<KernelReply, CoreError> {
    if args.ids.len() < 2 {
        return Err(CoreError::validation(
            "select the panels of the box first (at least two closed vectors)",
        ));
    }
    if !matches!(
        args.kind,
        JointKind::Finger | JointKind::Rabbet | JointKind::VJoint
    ) {
        return Err(CoreError::validation(
            "a box is joined at its corners; pick Finger, V Joint or Rabbet",
        ));
    }
    let pairs = box_pairs(document, &args.ids, args.kind != JointKind::VJoint)?;
    let edges: Vec<Value> = pairs
        .iter()
        .map(|(a, b)| json!({ "a": edge_json(document, a), "b": edge_json(document, b) }))
        .collect();
    if args.dry_run {
        return KernelReply::ok(&json!({ "pairs": edges }));
    }
    edit(document, "Box Joints", |doc| {
        let mut set = joint_set(doc);
        let mut ids = Vec::new();
        for (a, b) in &pairs {
            let spec = JointSpec {
                kind: args.kind,
                a: EdgeLine {
                    vector: a.vector,
                    from: a.from,
                    to: a.to,
                },
                b: Target::Edge(EdgeLine {
                    vector: b.vector,
                    from: b.from,
                    to: b.to,
                }),
                params: args.params.clone(),
            };
            ids.push(with_spec(doc, &mut set, spec, None)?);
        }
        apply(doc, set)?;
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "ids": ids, "pairs": edges, "mates": mates }))
    })
}

// ------------------------------------------------------------------ assembly

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssembleArgs {
    id: u64,
    #[serde(default)]
    quality: Quality,
}

/// The Assembly View piece a part vector is cut into: on its sheet, the
/// part whose box holds the vector's middle, the closest in size.
fn piece_of<'a>(document: &Document, infos: &'a [PieceInfo], vector: u64) -> Option<&'a PieceInfo> {
    let entity = document.entity(vector).ok()?;
    let bbox = document.bbox_of(vector)?;
    let mid = [
        (bbox.min.x + bbox.max.x) / 2.0,
        (bbox.min.y + bbox.max.y) / 2.0,
    ];
    let size = [bbox.max.x - bbox.min.x, bbox.max.y - bbox.min.y];
    infos
        .iter()
        .filter(|i| i.piece.sheet == entity.sheet && i.piece.component.is_none() && !i.is_model())
        .filter_map(|i| {
            let jb = i.sheet_parts.job_box(i.local)?;
            let inside = mid[0] >= jb[0] && mid[0] <= jb[2] && mid[1] >= jb[1] && mid[1] <= jb[3];
            let misfit = ((jb[2] - jb[0]) - size[0]).abs() + ((jb[3] - jb[1]) - size[1]).abs();
            inside.then_some((misfit, i))
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, i)| i)
}

/// Each joint's second part, and each part it seats (a key), against its
/// first part, for the joints with no mate yet whose parts both have a
/// toolpath on their sheet: (joint, seated part, first part, second part).
fn wanted_mates<'a>(
    document: &Document,
    set: &'a JointSet,
    mates: &[PieceMate],
) -> Vec<(&'a Joint, Option<u64>, u64, u64)> {
    let cut = |vector: u64| {
        document.entity(vector).is_ok_and(|e| {
            document
                .toolpaths()
                .iter()
                .any(|t| t.sheet == e.sheet || t.sheet == 0)
        })
    };
    set.joints
        .iter()
        .filter(|j| !j.kind.one_part() && j.b_to_a.is_some())
        .flat_map(|j| {
            let seated = j
                .vectors
                .iter()
                .filter(|v| v.seat.is_some())
                .map(move |v| (j, Some(v.id), j.a.vector, v.id));
            std::iter::once((j, None, j.a.vector, j.b.vector())).chain(seated)
        })
        .filter(|(j, part, _, _)| {
            !mates
                .iter()
                .any(|m| m.joint == Some(j.id) && m.part == *part)
        })
        .filter(|(_, _, a, b)| cut(*a) && cut(*b))
        .collect()
}

/// After a toolpath command: joints made before their parts had toolpaths
/// (the Joint Creator without Make the toolpaths too, then a profile of
/// the user's own) get their mates now that the toolpaths cut the parts
/// free, folded into the step that made the toolpaths, so the Assembly
/// View puts the parts together as the joints do.
pub(crate) fn seat_after_toolpaths(document: &mut Document, cache: &mut PartsCache) {
    let set = &document.assembly().joint_set;
    if set.joints.is_empty()
        || wanted_mates(document, set, &document.assembly().piece_mates).is_empty()
    {
        return;
    }
    if !document.reopen_newest_step() {
        return;
    }
    // Parts the pieces cannot be worked out for yet are left for later.
    let _ = keep_mates(document, cache, Quality::default(), None);
    document.close_reopened();
}

/// Keeps an Assembly View mate for every joint: drops the mates of joints
/// that are gone and, where the toolpaths cut both parts free as pieces,
/// adds a joint mate for each joint that has none, then solves the mates
/// so each second part goes to its first (inside the caller's undo step).
/// `lead` names a joint whose first part leads the solve even when no mate
/// is added; its parts must be pieces. Answers how many mates were added.
/// The points that fix a path's shape: each start, segment end and cubic
/// control point.
fn shape_points(path: &Path) -> Vec<P2> {
    let mut out = Vec::new();
    for sub_path in &path.subpaths {
        out.push([sub_path.start.x, sub_path.start.y]);
        for segment in &sub_path.segments {
            if let Segment::Cubic { c1, c2, .. } = segment {
                out.push([c1.x, c1.y]);
                out.push([c2.x, c2.y]);
            }
            let to = segment.end();
            out.push([to.x, to.y]);
        }
    }
    out
}

/// The turn (and mirror) and shift that takes `before` onto `after`, when
/// the one is the other moved rigidly (a nest, a move, a rotate or a
/// mirror), within 0.001 mm.
fn rigid_move(before: &Path, after: &Path) -> Option<crate::geom::Mat3> {
    let (p, q) = (shape_points(before), shape_points(after));
    if p.len() != q.len() || p.is_empty() {
        return None;
    }
    let n = p.len() as f64;
    let centre = |v: &[P2]| {
        let s = v
            .iter()
            .fold([0.0, 0.0], |acc, x| [acc[0] + x[0], acc[1] + x[1]]);
        [s[0] / n, s[1] / n]
    };
    let (cp, cq) = (centre(&p), centre(&q));
    for mirror in [1.0, -1.0] {
        let (mut sc, mut ss) = (0.0, 0.0);
        for (a, b) in p.iter().zip(&q) {
            let u = [a[0] - cp[0], mirror * (a[1] - cp[1])];
            let v = [b[0] - cq[0], b[1] - cq[1]];
            sc += u[0] * v[0] + u[1] * v[1];
            ss += u[0] * v[1] - u[1] * v[0];
        }
        let angle = ss.atan2(sc);
        let (c, s) = (angle.cos(), angle.sin());
        // Rotation after the mirror across X (when there is one).
        let (ma, mb, mc, md) = (c, s, -s * mirror, c * mirror);
        let e = cq[0] - (ma * cp[0] + mc * cp[1]);
        let f = cq[1] - (mb * cp[0] + md * cp[1]);
        let m = crate::geom::Mat3::new(ma, mb, mc, md, e, f);
        let fits = p.iter().zip(&q).all(|(a, b)| {
            let got = m.apply(crate::geom::Point::new(a[0], a[1]));
            (got.x - b[0]).abs() < 1e-3 && (got.y - b[1]).abs() < 1e-3
        });
        if fits {
            return Some(m);
        }
    }
    None
}

/// After a command that moved joined parts rigidly (Nest Parts, a move,
/// rotate or mirror): the joints go with them, their edges and the
/// outlines from before any joint moved the same way and every joint made
/// again, inside that command's undo step, so a later joint change keeps
/// the parts where they are and the Assembly View still puts them
/// together.
pub(crate) fn follow_moved_parts(document: &mut Document) {
    let mut set = joint_set(document);
    if set.joints.is_empty() {
        return;
    }
    let mut vectors: Vec<u64> = set.originals.keys().copied().collect();
    for joint in &set.joints {
        vectors.push(joint.a.vector);
        vectors.push(joint.b.vector());
    }
    vectors.sort_unstable();
    vectors.dedup();
    let mut moves: Vec<(u64, crate::geom::Mat3)> = Vec::new();
    for id in vectors {
        let Some(Some(before)) = document.newest_step_before(id) else {
            continue;
        };
        let (EntityKind::Path { path: old }, Ok(now)) = (&before.kind, document.entity(id)) else {
            continue;
        };
        let EntityKind::Path { path: new } = &now.kind else {
            continue;
        };
        if old == new {
            continue;
        }
        if let Some(m) = rigid_move(old, new) {
            moves.push((id, m));
        }
    }
    if moves.is_empty() {
        return;
    }
    let point = |m: &crate::geom::Mat3, p: P2| {
        let q = m.apply(crate::geom::Point::new(p[0], p[1]));
        [q.x, q.y]
    };
    for (id, m) in &moves {
        if let Some(path) = set.originals.get_mut(id) {
            *path = crate::geom::transform::transform_path(path, m);
        }
        for joint in &mut set.joints {
            let mut lines: Vec<&mut EdgeLine> = vec![&mut joint.a];
            match &mut joint.b {
                Target::Edge(e) | Target::Face { edge: e, .. } => lines.push(e),
            }
            for line in lines.into_iter().filter(|l| l.vector == *id) {
                line.from = point(m, line.from);
                line.to = point(m, line.to);
            }
        }
    }
    if !document.reopen_newest_step() {
        return;
    }
    // A joint that cannot be made where its parts are now stays as it was.
    let mut made = set.clone();
    match rebuild(document, &mut made, false) {
        Ok(outcomes) => {
            for (joint, outcome) in made.joints.iter_mut().zip(&outcomes) {
                joint.resolved = outcome.resolved.clone();
                joint.b_to_a = outcome.b_to_a.or(joint.b_to_a);
            }
            store(document, made);
        }
        Err(_) => store(document, set),
    }
    document.close_reopened();
}

/// The stock thickness each sheet holding a joint's part is cut from, as
/// the joints were made at. Taken before a command, it lets
/// [`follow_thickness`] tell that the command changed it.
pub(crate) fn joint_thicknesses(document: &Document) -> Vec<(u64, f64)> {
    let set = &document.assembly().joint_set;
    if set.joints.is_empty() {
        return Vec::new();
    }
    let mut sheets: Vec<u64> = set
        .joints
        .iter()
        .flat_map(|j| {
            [
                sheet_of(document, j.a.vector),
                sheet_of(document, j.b.vector()),
            ]
        })
        .collect();
    sheets.sort_unstable();
    sheets.dedup();
    sheets
        .into_iter()
        .map(|s| (s, set.settings.thickness_of(document, s)))
        .collect()
}

/// Remakes every joint, in the step of the command that just ran, when that
/// command changed the stock thickness of a sheet holding a part (Job Setup
/// or a sheet's material): the notches and fingers are cut to the
/// thickness, so they would no longer close the box.
pub(crate) fn follow_thickness(document: &mut Document, before: &[(u64, f64)]) {
    if before.is_empty() || joint_thicknesses(document) == before {
        return;
    }
    if !document.reopen_newest_step() {
        return;
    }
    let mut made = joint_set(document);
    if let Ok(outcomes) = rebuild(document, &mut made, false) {
        for (joint, outcome) in made.joints.iter_mut().zip(&outcomes) {
            joint.resolved = outcome.resolved.clone();
            joint.b_to_a = outcome.b_to_a.or(joint.b_to_a);
        }
        store(document, made);
    }
    document.close_reopened();
}

fn keep_mates(
    document: &mut Document,
    cache: &mut PartsCache,
    quality: Quality,
    lead: Option<u64>,
) -> Result<usize, CoreError> {
    let mut assembly = document.assembly().clone();
    // A seated part moved by hand keeps its seat from where it is now.
    for joint in &mut assembly.joint_set.joints {
        for made in joint.vectors.iter_mut().filter(|v| v.seat.is_some()) {
            if let Some(b) = document.bbox_of(made.id) {
                made.corner = Some([b.min.x, b.min.y]);
            }
        }
    }
    let set = assembly.joint_set.clone();
    // Mates of joints (or seated parts) that are gone go too.
    assembly.piece_mates.retain(|m| {
        m.joint.is_none_or(|id| {
            set.joints.iter().any(|j| {
                j.id == id
                    && m.part
                        .is_none_or(|p| j.vectors.iter().any(|v| v.id == p && v.seat.is_some()))
            })
        })
    });
    let wanted = wanted_mates(document, &set, &assembly.piece_mates);
    let kept = assembly
        .piece_mates
        .iter()
        .any(|m| m.kind == PieceMateKind::Joint);
    let infos = if wanted.is_empty() && lead.is_none() && !kept {
        None
    } else {
        match pieces(document, cache, quality.cells()) {
            Ok(infos) => Some(infos),
            Err(error) if lead.is_some() => return Err(error),
            // The joint is made even when the pieces cannot be worked
            // out; the mates follow the next time they can.
            Err(_) => None,
        }
    };
    let Some(infos) = infos else {
        if assembly != *document.assembly() {
            document.set_assembly(assembly);
        }
        return Ok(0);
    };
    let missing = || {
        CoreError::validation(
            "make the toolpaths first: the Assembly View shows the parts the toolpaths cut free",
        )
    };
    let mut driver = match lead {
        Some(id) => {
            let joint = set.joint(id)?;
            let a = piece_of(document, &infos, joint.a.vector).ok_or_else(missing)?;
            piece_of(document, &infos, joint.b.vector()).ok_or_else(missing)?;
            Some(a.piece)
        }
        None => None,
    };
    let mut made = 0;
    for (joint, part, a_vector, b_vector) in wanted {
        let (Some(a), Some(b)) = (
            piece_of(document, &infos, a_vector),
            piece_of(document, &infos, b_vector),
        ) else {
            continue;
        };
        if a.piece.key() == b.piece.key() {
            continue;
        }
        // For show: B's middle and its top's normal, and where the joint
        // puts them on A.
        let middle = pose3::scale(b.size, 0.5);
        let up = [0.0, 0.0, 1.0];
        let lock = joint_mates::lock(&set, joint.id, part, a, b);
        let id = assembly.take_id();
        assembly.piece_mates.push(PieceMate {
            id,
            kind: PieceMateKind::Joint,
            a: a.piece.key(),
            a_face: MateFace {
                point: lock.map_or(middle, |l| l.apply(middle)),
                normal: lock.map_or(up, |l| l.apply_vector(up)),
                radius: None,
            },
            b: b.piece.key(),
            b_face: MateFace {
                point: middle,
                normal: up,
                radius: None,
            },
            value: 0.0,
            aligned: false,
            suppressed: false,
            a_anchor: pieces_cmd::end_anchor(a),
            b_anchor: pieces_cmd::end_anchor(b),
            joint: Some(joint.id),
            part,
            lock: None,
        });
        driver.get_or_insert(a.piece);
        made += 1;
    }
    pieces_cmd::solve_into(&mut assembly, &infos, driver);
    if assembly != *document.assembly() {
        document.set_assembly(assembly);
    }
    Ok(made)
}

/// Puts part B's Assembly View piece against part A's as the joint puts
/// them together, keeping the joint's mate (one undo step).
fn assemble(
    document: &mut Document,
    cache: &mut PartsCache,
    args: AssembleArgs,
) -> Result<KernelReply, CoreError> {
    let joint = document.assembly().joint_set.joint(args.id)?.clone();
    if joint.b_to_a.is_none() || joint.kind.one_part() {
        return Err(CoreError::validation(
            "the joint has not been made yet; press Create first",
        ));
    }
    edit(document, "Assemble Joint", |doc| {
        let made = keep_mates(doc, cache, args.quality, Some(args.id))?;
        Ok(json!({ "mates": made }))
    })
}

// ------------------------------------------------------------------ toolpaths

/// Names of the toolpaths the Joint Creator makes start with this.
pub const TOOLPATH_PREFIX: &str = "Joints";

/// The most a step of a joint's tapered wall stands out of the slope, mm:
/// a fifth of the usual clearance, so the fit stays true to it.
const TAPER_SCALLOP: f64 = 0.02;

/// A toolpath to make: its sheet, its place in the cutting order, its name.
type JobKey = (u64, u8, String);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolpathArgs {
    /// The end mill, as picked from the Tool Database.
    tool: crate::cam::tool::Tool,
}

/// The toolpaths the joints of `set` need: (sheet, order, name) to
/// (strategy, params, vectors).
fn joint_jobs(
    document: &Document,
    set: &JointSet,
) -> std::collections::BTreeMap<JobKey, (crate::cam::compute::Strategy, Value, Vec<u64>)> {
    jobs_for(
        document,
        set.joints.iter(),
        &[],
        &set.settings,
        TOOLPATH_PREFIX,
    )
}

/// The toolpaths `joints` need, and the outlines of the parts in
/// `outlines` too, their names starting with `prefix`: (sheet, order,
/// name) to (strategy, params, vectors). Pockets come first (order 1),
/// then holes (2), then the outlines (3).
pub(crate) fn jobs_for<'a>(
    document: &Document,
    joints: impl Iterator<Item = &'a Joint>,
    outlines: &[u64],
    settings: &JointSettings,
    prefix: &str,
) -> std::collections::BTreeMap<JobKey, (crate::cam::compute::Strategy, Value, Vec<u64>)> {
    use crate::cam::compute::Strategy;
    use std::collections::BTreeMap;

    // (sheet, order, name) -> (strategy, params, vectors)
    let mut jobs: BTreeMap<JobKey, (Strategy, Value, Vec<u64>)> = BTreeMap::new();
    let mut add_to = |document: &Document,
                      id: u64,
                      order: u8,
                      name: String,
                      strategy: Strategy,
                      params: Value| {
        let Ok(entity) = document.entity(id) else {
            return;
        };
        let entry = jobs
            .entry((entity.sheet, order, name))
            .or_insert_with(|| (strategy, params, Vec::new()));
        if !entry.2.contains(&id) {
            entry.2.push(id);
        }
    };
    let through = |document: &Document, id: u64| {
        let sheet = document.entity(id).map_or(0, |e| e.sheet);
        let nominal = document
            .sheet_material(sheet)
            .map_or(document.job_setup().thickness, |m| m.thickness);
        settings.thickness_of(document, sheet).max(nominal)
    };
    for part in outlines {
        let depth = through(document, *part);
        add_to(
            document,
            *part,
            3,
            format!("{prefix} outlines"),
            Strategy::Profile,
            json!({ "side": "outside", "cutDepth": depth }),
        );
    }
    for joint in joints {
        for part in [joint.a.vector, joint.b.vector()] {
            let depth = through(document, part);
            add_to(
                document,
                part,
                3,
                format!("{prefix} outlines"),
                Strategy::Profile,
                json!({ "side": "outside", "cutDepth": depth }),
            );
        }
        for made in &joint.vectors {
            match made.role {
                super::Role::Pocket { depth } => {
                    let rounded = (depth * 100.0).round() / 100.0;
                    add_to(
                        document,
                        made.id,
                        1,
                        format!("{prefix} pockets {rounded} mm"),
                        Strategy::Pocket,
                        json!({ "cutDepth": depth }),
                    );
                }
                super::Role::Hole => {
                    let depth = through(document, made.id);
                    add_to(
                        document,
                        made.id,
                        2,
                        format!("{prefix} holes"),
                        Strategy::Profile,
                        json!({ "side": "inside", "cutDepth": depth }),
                    );
                }
                super::Role::Part { thickness } => add_to(
                    document,
                    made.id,
                    3,
                    format!("{prefix} outlines"),
                    Strategy::Profile,
                    json!({ "side": "outside", "cutDepth": thickness }),
                ),
                super::Role::Mark => {}
                super::Role::Taper { depth, angle } => {
                    let rounded = (angle * 100.0).round() / 100.0;
                    add_to(
                        document,
                        made.id,
                        1,
                        format!("{prefix} tapered walls {rounded} deg"),
                        Strategy::Profile,
                        json!({
                            "openSide": "right",
                            "cutDepth": depth,
                            "wallAngle": angle,
                            "scallop": TAPER_SCALLOP,
                            "rigid": true,
                        }),
                    );
                }
            }
        }
    }
    jobs
}

/// One toolpath of a joint job, calculated on its sheet with `tool`.
///
/// # Errors
/// What calculating the toolpath reports.
pub(crate) fn job_toolpath(
    doc: &Document,
    sheet: u64,
    name: String,
    strategy: crate::cam::compute::Strategy,
    params: Value,
    vectors: Vec<u64>,
    tool: &crate::cam::tool::Tool,
) -> Result<crate::cam::toolpath::Toolpath, CoreError> {
    use crate::cam::compute::{calculate, Request, Strategy};
    // Profiles keep their whole settings, so the Profile form can open and
    // recalculate them.
    let params = if strategy == Strategy::Profile {
        let full: crate::cam::profile::ProfileParams = serde_json::from_value(params)
            .map_err(|e| CoreError::validation(format!("joint profile settings: {e}")))?;
        serde_json::to_value(full)
            .map_err(|e| CoreError::validation(format!("joint profile settings: {e}")))?
    } else {
        params
    };
    let request = Request {
        strategy,
        name: Some(name),
        tool: tool.clone(),
        params,
        ids: vectors,
    };
    let (mut toolpath, _) = calculate(doc, &request)?;
    toolpath.sheet = sheet;
    Ok(toolpath)
}

/// Makes the toolpaths that cut every joint of the job, per sheet: pockets
/// by depth, then holes (inside), then the part outlines and the extra
/// parts (outside), all with `tool`. Toolpaths an earlier run made are
/// replaced. One undo step.
fn make_toolpaths(
    document: &mut Document,
    cache: &mut PartsCache,
    args: ToolpathArgs,
) -> Result<KernelReply, CoreError> {
    if document.assembly().joint_set.joints.is_empty() {
        return Err(CoreError::validation("the job has no joints yet"));
    }
    edit(document, "Joint Toolpaths", |doc| {
        // The corner reliefs are sized for the tool in the joint settings:
        // a different tool remakes them for it first, as the panel does.
        let mut set = joint_set(doc);
        let diameter = args.tool.diameter();
        if diameter > 0.0 && (set.settings.tool_diameter - diameter).abs() > 1e-9 {
            set.settings.tool_diameter = diameter;
            set.settings.validate()?;
            apply(doc, set)?;
        }
        let jobs = joint_jobs(doc, &joint_set(doc));
        let old: Vec<u64> = doc
            .toolpaths()
            .iter()
            .filter(|t| t.name.starts_with(TOOLPATH_PREFIX))
            .map(|t| t.id)
            .collect();
        doc.delete_toolpaths(&old)?;
        let mut ids = Vec::new();
        // Each toolpath is made with its sheet on show: the job setup is the
        // sheet on show's material, and the depths and the through cut
        // follow it.
        let showing = doc.active_sheet();
        for ((sheet, _, name), (strategy, params, vectors)) in jobs {
            if doc.active_sheet() != sheet && doc.sheet(sheet).is_ok() {
                doc.set_active_sheet(sheet)?;
            }
            let toolpath = job_toolpath(doc, sheet, name, strategy, params, vectors, &args.tool)?;
            ids.push(doc.add_toolpath(toolpath));
        }
        if doc.active_sheet() != showing && doc.sheet(showing).is_ok() {
            doc.set_active_sheet(showing)?;
        }
        let mates = keep_mates(doc, cache, Quality::default(), None)?;
        Ok(json!({ "ids": ids, "replaced": old.len(), "mates": mates }))
    })
}

// ------------------------------------------------------------------ coupon

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CouponArgs {
    kind: JointKind,
    #[serde(default)]
    params: JointParams,
}

/// A closed rectangle vector on the active sheet and layer.
fn rectangle(document: &mut Document, name: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
    let ring = vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let entity = crate::doc::Entity {
        id: 0,
        layer: document.active_layer(),
        sheet: document.active_sheet(),
        revision: 0,
        name: Some(name.to_string()),
        parent: None,
        rotation: 0.0,
        kind: EntityKind::Path {
            path: super::geometry::rings_path(&[ring]),
        },
    };
    document.insert_entity(entity)
}

/// The lower left corner of a free `w` by `h` spot on the material, away
/// from every vector on the active sheet, or the material's corner.
fn free_spot(document: &Document, w: f64, h: f64) -> P2 {
    let job = document.job_bounds();
    let taken: Vec<crate::geom::BBox> = document
        .order()
        .iter()
        .filter(|id| {
            document
                .entity(**id)
                .is_ok_and(|e| e.sheet == document.active_sheet())
        })
        .filter_map(|id| document.bbox_of(*id))
        .collect();
    let margin = 10.0;
    let step = 10.0;
    let mut y = job.min.y + margin;
    while y + h + margin <= job.max.y {
        let mut x = job.min.x + margin;
        while x + w + margin <= job.max.x {
            let spot = crate::geom::BBox {
                min: crate::geom::Point::new(x - margin, y - margin),
                max: crate::geom::Point::new(x + w + margin, y + h + margin),
            };
            if !taken.iter().any(|b| b.intersects(spot)) {
                return [x, y];
            }
            x += step;
        }
        y += step;
    }
    [job.min.x + margin, job.min.y + margin]
}

/// Makes a small sample of a joint: two little parts at a free spot on the
/// material and the joint between them with the job's settings, to cut
/// first and try the fit. One undo step.
fn coupon(
    document: &mut Document,
    cache: &mut PartsCache,
    args: CouponArgs,
) -> Result<KernelReply, CoreError> {
    let settings = document.assembly().joint_set.settings.clone();
    let t = settings.thickness_of(document, document.active_sheet());
    // Wide enough for three fingers of about twice the stock.
    let w = (6.0 * t).clamp(40.0, 120.0);
    let h = (3.0 * t).clamp(30.0, 80.0);
    // The parts must stand apart by more than the tool, or they cut as one
    // piece: lay the coupon out once to see how far the joint reaches past
    // each part's edge, then lay it out for real with room for that.
    let clear = settings.tool_diameter + 3.0;
    let mut gap = 15.0;
    document.begin("Test Coupon layout");
    let probe = lay_coupon(document, &args, w, h, t, gap);
    let apart = probe.map(|(_, a, b)| coupon_apart(document, a, b, args.kind.on_face()));
    document.rollback();
    gap += (clear - apart?).max(0.0);
    edit(document, "Test Coupon", |doc| {
        let (id, a, b) = lay_coupon(doc, &args, w, h, t, gap)?;
        keep_mates(doc, cache, Quality::default(), None)?;
        doc.set_selection(&[a, b]);
        Ok(json!({ "id": id, "parts": [a, b] }))
    })
}

/// How far apart the coupon's two parts are once the joint is made: the
/// space between A's and B's boxes across the gap that separates them.
fn coupon_apart(document: &Document, a: u64, b: u64, on_face: bool) -> f64 {
    let (Some(ba), Some(bb)) = (document.bbox_of(a), document.bbox_of(b)) else {
        return 0.0;
    };
    if on_face {
        ba.min.y - bb.max.y
    } else {
        bb.min.y - ba.max.y
    }
}

/// Lays the coupon's two parts out `gap` apart and makes the joint between
/// them, answering the joint and the parts.
fn lay_coupon(
    doc: &mut Document,
    args: &CouponArgs,
    w: f64,
    h: f64,
    t: f64,
    gap: f64,
) -> Result<(u64, u64, u64), CoreError> {
    {
        let (a, b, spec_b, a_edge) = if args.kind.on_face() {
            // B lies flat; A stands across its middle.
            let [x, y] = free_spot(doc, w + 2.0 * gap, 2.0 * h + gap);
            let b = rectangle(doc, "Coupon B", x, y, x + w + 2.0 * gap, y + h);
            let a = rectangle(
                doc,
                "Coupon A",
                x + gap,
                y + h + gap,
                x + gap + w,
                y + 2.0 * h + gap,
            );
            let edge = EdgeLine {
                vector: b,
                from: [x, y],
                to: [x + w + 2.0 * gap, y],
            };
            let target = Target::Face {
                edge,
                offset: (h - t) / 2.0,
                along: (w + 2.0 * gap) / 2.0,
            };
            // A's bottom edge stands on B.
            let a_edge = EdgeLine {
                vector: a,
                from: [x + gap, y + h + gap],
                to: [x + gap + w, y + h + gap],
            };
            (a, b, target, a_edge)
        } else {
            let [x, y] = free_spot(doc, w, 2.0 * h + gap);
            let a = rectangle(doc, "Coupon A", x, y, x + w, y + h);
            let b = rectangle(doc, "Coupon B", x, y + h + gap, x + w, y + 2.0 * h + gap);
            // A's top edge meets B's bottom edge.
            let a_edge = EdgeLine {
                vector: a,
                from: [x + w, y + h],
                to: [x, y + h],
            };
            let target = Target::Edge(EdgeLine {
                vector: b,
                from: [x, y + h + gap],
                to: [x + w, y + h + gap],
            });
            (a, b, target, a_edge)
        };
        let mut set = joint_set(doc);
        let id = with_spec(
            doc,
            &mut set,
            JointSpec {
                kind: args.kind,
                a: a_edge,
                b: spec_b,
                params: args.params.clone(),
            },
            None,
        )?;
        apply(doc, set)?;
        Ok((id, a, b))
    }
}
