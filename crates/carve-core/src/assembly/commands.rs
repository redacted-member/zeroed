//! Kernel commands of the vector part assembly (`assembly.*`, brief 017).
//! Every edit is one undo step, except `assembly.drive`, which moves a hinge
//! or slide continuously and is saved but not journalled. The Assembly
//! View's piece commands live in [`super::pieces_cmd`].

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::flatten::flatten_subpath;
use crate::geom::Tolerance;
use crate::kernel::KernelReply;

use super::interference::{clashes, Placed};
use super::pose::{Pose, Vec3};
use super::solid::{Face, Profile};
use super::triangulate::{signed_area2, P2};
use super::{Assembly, FaceFrames, Grain, Instance, Mate, MateKind, Part, PartPocket};

/// Chord tolerance for turning part vectors into polygons, in millimetres.
const CHORD: f64 = 0.05;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name.starts_with("assembly.")
}

/// Runs one assembly command.
///
/// # Errors
/// [`CoreError::Validation`] for bad arguments or references,
/// [`CoreError::UnknownCommand`] for a name this module does not know.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "assembly.get" => get(document),
        "assembly.makePart" => make_part(document, parse(name, args)?),
        "assembly.updatePart" => update_part(document, parse(name, args)?),
        "assembly.removePart" => remove(document, parse(name, args)?, Removing::Part),
        "assembly.addInstance" => add_instance(document, parse(name, args)?),
        "assembly.updateInstance" => update_instance(document, parse(name, args)?),
        "assembly.removeInstance" => remove(document, parse(name, args)?, Removing::Instance),
        "assembly.mate" => add_mate(document, parse(name, args)?),
        "assembly.updateMate" => update_mate(document, parse(name, args)?),
        "assembly.unmate" => remove(document, parse(name, args)?, Removing::Mate),
        "assembly.drive" => drive(document, parse(name, args)?),
        "assembly.meshes" => meshes(document, parse(name, args)?),
        "assembly.interference" => interference(document, parse(name, args)?),
        "assembly.cutList" => cut_list(document),
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

fn parse<T: serde::de::DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        trimmed
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

/// Runs `change` on a copy of the assembly as one undo step labelled `label`.
fn edit<F>(document: &mut Document, label: &str, change: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Assembly, &Document) -> Result<Value, CoreError>,
{
    let mut assembly = document.assembly().clone();
    let mut result = change(&mut assembly, document)?;
    document.begin(label);
    document.set_assembly(assembly);
    let revision = document.commit();
    if let Some(object) = result.as_object_mut() {
        object.insert("revision".to_string(), json!(revision));
    }
    KernelReply::ok(&result)
}

// ------------------------------------------------------------------ solids

/// The part solids of a document, built from their vectors.
pub struct Solids {
    /// Profiles by part id; parts whose vectors are missing are absent.
    pub profiles: BTreeMap<u64, Profile>,
    /// Why a part has no solid.
    pub problems: BTreeMap<u64, String>,
}

impl FaceFrames for Solids {
    fn face(&self, part: u64, face: Face) -> Option<(Pose, [f64; 2])> {
        let profile = self.profiles.get(&part)?;
        Some((profile.face_frame(face)?, profile.face_size(face)?))
    }
}

/// The closed ring of vector `id` in sheet coordinates, and its sheet.
pub(super) fn ring_of(document: &Document, id: u64) -> Result<(Vec<P2>, u64), CoreError> {
    let entity = document.entity(id)?;
    let path = entity
        .path()
        .ok_or_else(|| CoreError::validation(format!("vector {id} has no geometry")))?;
    let tol = Tolerance::new(CHORD)?;
    let sub = path
        .subpaths
        .iter()
        .find(|s| s.closed)
        .ok_or_else(|| CoreError::validation(format!("vector {id} is not closed")))?;
    let poly = flatten_subpath(sub, tol);
    let mut ring: Vec<P2> = poly.points.iter().map(|p| [p.x, p.y]).collect();
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() < 3 {
        return Err(CoreError::validation(format!(
            "vector {id} is too small to be a part"
        )));
    }
    Ok((ring, entity.sheet))
}

/// The solid profile of `part`, from its vectors.
pub(super) fn profile_of(document: &Document, part: &Part) -> Result<Profile, CoreError> {
    let (outer, sheet) = ring_of(document, part.outline)?;
    let holes = part
        .holes
        .iter()
        .map(|id| ring_of(document, *id).map(|(r, _)| r))
        .collect::<Result<Vec<_>, _>>()?;
    let pockets = part
        .pockets
        .iter()
        .map(|p| ring_of(document, p.vector).map(|(r, _)| (r, p.depth)))
        .collect::<Result<Vec<_>, _>>()?;
    let thickness = match part.thickness {
        Some(t) => t,
        None => document.sheet_material(sheet)?.thickness,
    };
    if thickness.is_nan() || thickness <= 0.0 {
        return Err(CoreError::validation("a part needs a thickness above zero"));
    }
    for pocket in &part.pockets {
        if pocket.depth.is_nan() || pocket.depth <= 0.0 || pocket.depth >= thickness {
            return Err(CoreError::validation(
                "a pocket must be shallower than the part is thick",
            ));
        }
    }
    Ok(Profile::with_frame(
        part.frame_outline.clone(),
        outer,
        holes,
        pockets,
        thickness,
    ))
}

/// Builds every part's solid.
pub fn solids(document: &Document) -> Solids {
    solids_in(document, document.assembly())
}

/// Builds the solid of every part of `assembly` (which may be an edited
/// copy of the document's) from the document's vectors.
pub fn solids_in(document: &Document, assembly: &Assembly) -> Solids {
    let mut profiles = BTreeMap::new();
    let mut problems = BTreeMap::new();
    for part in &assembly.parts {
        match profile_of(document, part) {
            Ok(profile) => {
                profiles.insert(part.id, profile);
            }
            Err(error) => {
                problems.insert(part.id, error.to_string());
            }
        }
    }
    Solids { profiles, problems }
}

// ------------------------------------------------------------------ reads

fn get(document: &Document) -> Result<KernelReply, CoreError> {
    let assembly = document.assembly();
    let solids = solids(document);
    let poses = assembly.poses(&solids, 0.0).unwrap_or_default();
    let parts: Vec<Value> = assembly
        .parts
        .iter()
        .map(|part| {
            let profile = solids.profiles.get(&part.id);
            json!({
                "part": part,
                "size": profile.map(|p| { let s = p.size(); [s[0], s[1], p.thickness] }),
                "edges": profile.map(Profile::edge_count),
                "instances": assembly.instances.iter().filter(|i| i.part == part.id).count(),
                "problem": solids.problems.get(&part.id),
            })
        })
        .collect();
    let instances: Vec<Value> = assembly
        .instances
        .iter()
        .map(|instance| {
            json!({
                "instance": instance,
                "matrix": poses.get(&instance.id).map(Pose::to_matrix),
                "mate": assembly.mate_of(instance.id).map(|m| m.id),
            })
        })
        .collect();
    KernelReply::ok(&json!({
        "revision": document.revision(),
        "parts": parts,
        "instances": instances,
        "mates": assembly.mates,
    }))
}

// ------------------------------------------------------------------ parts

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MakePartArgs {
    /// Closed vectors: the largest becomes the outline, the rest holes or
    /// pockets.
    ids: Vec<u64>,
    #[serde(default)]
    name: Option<String>,
    /// Vectors among `ids` that are pockets, with their depth.
    #[serde(default)]
    pockets: Vec<PartPocket>,
    #[serde(default)]
    thickness: Option<f64>,
    #[serde(default)]
    grain: Grain,
}

fn make_part(document: &mut Document, args: MakePartArgs) -> Result<KernelReply, CoreError> {
    if args.ids.is_empty() {
        return Err(CoreError::validation(
            "select the closed vectors of a part first",
        ));
    }
    let mut largest: Option<(u64, f64, Vec<P2>)> = None;
    let mut rings = Vec::new();
    for id in &args.ids {
        let (ring, _) = ring_of(document, *id)?;
        let area = signed_area2(&ring).abs() / 2.0;
        if largest.as_ref().is_none_or(|(_, a, _)| area > *a) {
            largest = Some((*id, area, ring.clone()));
        }
        rings.push((*id, ring));
    }
    let Some((outline, _, outer)) = largest else {
        return Err(CoreError::validation(
            "select the closed vectors of a part first",
        ));
    };
    let mut holes = Vec::new();
    for (id, ring) in &rings {
        if *id == outline {
            continue;
        }
        let n = ring.len() as f64;
        let centre = ring
            .iter()
            .fold([0.0, 0.0], |c, p| [c[0] + p[0] / n, c[1] + p[1] / n]);
        if !ring.iter().any(|p| super::solid::inside_ring(&outer, *p))
            && !super::solid::inside_ring(&outer, centre)
        {
            return Err(CoreError::validation(format!(
                "vector {id} lies outside the part's outline"
            )));
        }
        if !args.pockets.iter().any(|p| p.vector == *id) {
            holes.push(*id);
        }
    }
    let pockets: Vec<PartPocket> = args
        .pockets
        .into_iter()
        .filter(|p| p.vector != outline && args.ids.contains(&p.vector))
        .collect();
    edit(document, "Make Part", |assembly, doc| {
        let id = assembly.take_id();
        let name = args
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("Part {}", assembly.parts.len() + 1));
        let part = Part {
            id,
            name: name.clone(),
            outline,
            holes,
            pockets,
            thickness: args.thickness,
            grain: args.grain,
            frame_outline: None,
        };
        profile_of(doc, &part)?;
        assembly.parts.push(part);
        let instance = assembly.take_id();
        assembly.instances.push(Instance {
            id: instance,
            part: id,
            name,
            position: [0.0; 3],
            rotation: [0.0; 3],
        });
        Ok(json!({ "part": id, "instance": instance }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePartArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    /// `Some(None)` returns the part to its sheet's thickness.
    #[serde(default, with = "serde_with_none")]
    thickness: Option<Option<f64>>,
    #[serde(default)]
    grain: Option<Grain>,
    #[serde(default)]
    pockets: Option<Vec<PartPocket>>,
}

/// Deserializes a field that may be absent, null or a value.
mod serde_with_none {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

fn update_part(document: &mut Document, args: UpdatePartArgs) -> Result<KernelReply, CoreError> {
    edit(document, "Edit Part", |assembly, doc| {
        let part = assembly
            .parts
            .iter_mut()
            .find(|p| p.id == args.id)
            .ok_or_else(|| CoreError::validation(format!("there is no part {}", args.id)))?;
        if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
            part.name = name;
        }
        if let Some(thickness) = args.thickness {
            part.thickness = thickness;
        }
        if let Some(grain) = args.grain {
            part.grain = grain;
        }
        if let Some(pockets) = args.pockets {
            let known: Vec<u64> = part
                .holes
                .iter()
                .copied()
                .chain(part.pockets.iter().map(|p| p.vector))
                .collect();
            if let Some(stray) = pockets.iter().find(|p| !known.contains(&p.vector)) {
                return Err(CoreError::validation(format!(
                    "vector {} is not one of the part's inner vectors",
                    stray.vector
                )));
            }
            part.holes = known
                .into_iter()
                .filter(|id| !pockets.iter().any(|p| p.vector == *id))
                .collect();
            part.pockets = pockets;
        }
        profile_of(doc, part)?;
        Ok(json!({ "id": args.id }))
    })
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

enum Removing {
    Part,
    Instance,
    Mate,
}

fn remove(document: &mut Document, args: IdArgs, what: Removing) -> Result<KernelReply, CoreError> {
    let label = match what {
        Removing::Part => "Delete Part",
        Removing::Instance => "Delete Instance",
        Removing::Mate => "Delete Mate",
    };
    edit(document, label, |assembly, _| {
        match what {
            Removing::Part => {
                assembly.part(args.id)?;
                let gone: Vec<u64> = assembly
                    .instances
                    .iter()
                    .filter(|i| i.part == args.id)
                    .map(|i| i.id)
                    .collect();
                assembly.parts.retain(|p| p.id != args.id);
                assembly.instances.retain(|i| i.part != args.id);
                assembly
                    .mates
                    .retain(|m| !gone.contains(&m.parent) && !gone.contains(&m.child));
            }
            Removing::Instance => {
                assembly.instance(args.id)?;
                assembly.instances.retain(|i| i.id != args.id);
                assembly
                    .mates
                    .retain(|m| m.parent != args.id && m.child != args.id);
            }
            Removing::Mate => {
                if !assembly.mates.iter().any(|m| m.id == args.id) {
                    return Err(CoreError::validation(format!(
                        "there is no mate {}",
                        args.id
                    )));
                }
                assembly.mates.retain(|m| m.id != args.id);
            }
        }
        Ok(json!({ "id": args.id }))
    })
}

// ------------------------------------------------------------------ instances

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddInstanceArgs {
    part: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    position: Vec3,
    #[serde(default)]
    rotation: Vec3,
}

fn add_instance(document: &mut Document, args: AddInstanceArgs) -> Result<KernelReply, CoreError> {
    edit(document, "Add Instance", |assembly, _| {
        let part_name = assembly.part(args.part)?.name.clone();
        let copies = assembly
            .instances
            .iter()
            .filter(|i| i.part == args.part)
            .count();
        let id = assembly.take_id();
        assembly.instances.push(Instance {
            id,
            part: args.part,
            name: args
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| format!("{part_name} {}", copies + 1)),
            position: args.position,
            rotation: args.rotation,
        });
        Ok(json!({ "id": id }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInstanceArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    position: Option<Vec3>,
    #[serde(default)]
    rotation: Option<Vec3>,
}

fn update_instance(
    document: &mut Document,
    args: UpdateInstanceArgs,
) -> Result<KernelReply, CoreError> {
    edit(document, "Move Instance", |assembly, _| {
        let instance = assembly
            .instances
            .iter_mut()
            .find(|i| i.id == args.id)
            .ok_or_else(|| CoreError::validation(format!("there is no instance {}", args.id)))?;
        if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
            instance.name = name;
        }
        if let Some(position) = args.position {
            instance.position = position;
        }
        if let Some(rotation) = args.rotation {
            instance.rotation = rotation;
        }
        Ok(json!({ "id": args.id }))
    })
}

// ------------------------------------------------------------------ mates

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MateArgs {
    #[serde(default)]
    kind: MateKind,
    parent: u64,
    parent_face: Face,
    child: u64,
    child_face: Face,
    #[serde(default)]
    offset: [f64; 2],
    #[serde(default)]
    angle: f64,
    #[serde(default)]
    gap: f64,
    #[serde(default)]
    axis_point: Option<Vec3>,
    #[serde(default)]
    axis_dir: Option<Vec3>,
    #[serde(default)]
    value: f64,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
}

fn check_faces(solids: &Solids, assembly: &Assembly, mate: &Mate) -> Result<(), CoreError> {
    for (instance, face) in [
        (mate.parent, mate.parent_face),
        (mate.child, mate.child_face),
    ] {
        let part = assembly.instance(instance)?.part;
        if solids.face(part, face).is_none() {
            return Err(CoreError::validation(format!(
                "that face does not exist on {}",
                assembly.instance(instance)?.name
            )));
        }
    }
    Ok(())
}

fn add_mate(document: &mut Document, args: MateArgs) -> Result<KernelReply, CoreError> {
    let solids = solids(document);
    edit(document, "Mate", |assembly, _| {
        // Hinges default to the parent face's u axis through its origin (the
        // long corner of an edge face, where a door hinge sits), up to a
        // quarter turn; slides to its u axis, 0 to 400 mm.
        let (dir, lo, hi) = match args.kind {
            MateKind::Flush => ([0.0, 1.0, 0.0], 0.0, 0.0),
            MateKind::Hinge => ([1.0, 0.0, 0.0], 0.0, 90.0),
            MateKind::Slide => ([1.0, 0.0, 0.0], 0.0, 400.0),
        };
        let mate = Mate {
            id: assembly.take_id(),
            kind: args.kind,
            parent: args.parent,
            parent_face: args.parent_face,
            child: args.child,
            child_face: args.child_face,
            offset: args.offset,
            angle: args.angle,
            gap: args.gap,
            axis_point: args.axis_point.unwrap_or([0.0; 3]),
            axis_dir: args.axis_dir.unwrap_or(dir),
            value: args.value,
            min: args.min.unwrap_or(lo),
            max: args.max.unwrap_or(hi),
        };
        assembly.check_mate(&mate)?;
        check_faces(&solids, assembly, &mate)?;
        let id = mate.id;
        assembly.mates.push(mate);
        Ok(json!({ "id": id }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMateArgs {
    id: u64,
    #[serde(default)]
    kind: Option<MateKind>,
    #[serde(default)]
    offset: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    gap: Option<f64>,
    #[serde(default)]
    axis_point: Option<Vec3>,
    #[serde(default)]
    axis_dir: Option<Vec3>,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
}

fn update_mate(document: &mut Document, args: UpdateMateArgs) -> Result<KernelReply, CoreError> {
    edit(document, "Edit Mate", |assembly, _| {
        let mate = assembly
            .mates
            .iter_mut()
            .find(|m| m.id == args.id)
            .ok_or_else(|| CoreError::validation(format!("there is no mate {}", args.id)))?;
        if let Some(v) = args.kind {
            mate.kind = v;
        }
        if let Some(v) = args.offset {
            mate.offset = v;
        }
        if let Some(v) = args.angle {
            mate.angle = v;
        }
        if let Some(v) = args.gap {
            mate.gap = v;
        }
        if let Some(v) = args.axis_point {
            mate.axis_point = v;
        }
        if let Some(v) = args.axis_dir {
            mate.axis_dir = v;
        }
        if let Some(v) = args.value {
            mate.value = v;
        }
        if let Some(v) = args.min {
            mate.min = v;
        }
        if let Some(v) = args.max {
            mate.max = v;
        }
        Ok(json!({ "id": args.id }))
    })
}

#[derive(Debug, Deserialize)]
struct DriveArgs {
    id: u64,
    value: f64,
}

fn drive(document: &mut Document, args: DriveArgs) -> Result<KernelReply, CoreError> {
    let mut assembly = document.assembly().clone();
    let mate = assembly
        .mates
        .iter_mut()
        .find(|m| m.id == args.id)
        .ok_or_else(|| CoreError::validation(format!("there is no mate {}", args.id)))?;
    if mate.kind == MateKind::Flush {
        return Err(CoreError::validation("only hinges and slides move"));
    }
    mate.value = args
        .value
        .clamp(mate.min.min(mate.max), mate.max.max(mate.min));
    let value = mate.value;
    document.set_assembly_untracked(assembly);
    KernelReply::ok(&json!({ "id": args.id, "value": value }))
}

// ------------------------------------------------------------------ meshes

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MeshesArgs {
    /// Exploded view distance per mate, in millimetres.
    explode: f64,
}

fn meshes(document: &Document, args: MeshesArgs) -> Result<KernelReply, CoreError> {
    let assembly = document.assembly();
    let solids = solids(document);
    let poses = assembly.poses(&solids, args.explode)?;
    let mut sections = Vec::new();
    let mut payload: Vec<u8> = Vec::new();
    let mut section =
        |name: String, dtype: &str, bytes: Vec<u8>, count: usize, payload: &mut Vec<u8>| {
            sections.push(
                json!({ "name": name, "dtype": dtype, "offset": payload.len(), "count": count }),
            );
            payload.extend(bytes);
            while payload.len() % 4 != 0 {
                payload.push(0);
            }
        };
    let mut parts = Vec::new();
    for (id, profile) in &solids.profiles {
        let mesh = profile.mesh();
        let f32s = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let u32s = |v: &[u32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        section(
            format!("p{id}.positions"),
            "f32",
            f32s(&mesh.positions),
            mesh.positions.len(),
            &mut payload,
        );
        section(
            format!("p{id}.normals"),
            "f32",
            f32s(&mesh.normals),
            mesh.normals.len(),
            &mut payload,
        );
        section(
            format!("p{id}.indices"),
            "u32",
            u32s(&mesh.indices),
            mesh.indices.len(),
            &mut payload,
        );
        section(
            format!("p{id}.faces"),
            "u32",
            u32s(&mesh.faces),
            mesh.faces.len(),
            &mut payload,
        );
        parts.push(json!({ "id": id, "thickness": profile.thickness }));
    }
    let instances: Vec<Value> = assembly
        .instances
        .iter()
        .filter(|i| solids.profiles.contains_key(&i.part))
        .filter_map(|i| {
            poses
                .get(&i.id)
                .map(|pose| json!({ "id": i.id, "part": i.part, "matrix": pose.to_matrix() }))
        })
        .collect();
    let header = json!({
        "revision": document.revision(),
        "parts": parts,
        "instances": instances,
        "sections": sections,
    })
    .to_string()
    .into_bytes();
    let mut out = Vec::with_capacity(4 + header.len() + payload.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload);
    Ok(KernelReply::Bytes(out))
}

// ------------------------------------------------------------------ checks

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SweepArgs {
    mate: u64,
    #[serde(default = "default_steps")]
    steps: usize,
}

fn default_steps() -> usize {
    24
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct InterferenceArgs {
    /// Sample spacing in millimetres (default 2).
    step: Option<f64>,
    /// Also check across the whole range of one hinge or slide.
    sweep: Option<SweepArgs>,
}

fn clashes_of(
    assembly: &Assembly,
    solids: &Solids,
    step: f64,
) -> Result<Vec<super::interference::Clash>, CoreError> {
    let poses = assembly.poses(solids, 0.0)?;
    let placed: Vec<Placed> = assembly
        .instances
        .iter()
        .filter_map(|i| {
            Some(Placed {
                id: i.id,
                profile: solids.profiles.get(&i.part)?,
                pose: *poses.get(&i.id)?,
            })
        })
        .collect();
    Ok(clashes(&placed, step))
}

fn interference(document: &Document, args: InterferenceArgs) -> Result<KernelReply, CoreError> {
    let step = args.step.unwrap_or(2.0).max(0.25);
    let solids = solids(document);
    let assembly = document.assembly();
    let now = clashes_of(assembly, &solids, step)?;
    let mut sweep = Vec::new();
    if let Some(s) = args.sweep {
        let mate = assembly
            .mates
            .iter()
            .find(|m| m.id == s.mate)
            .ok_or_else(|| CoreError::validation(format!("there is no mate {}", s.mate)))?;
        let steps = s.steps.clamp(2, 200);
        let (lo, hi) = (mate.min.min(mate.max), mate.max.max(mate.min));
        for k in 0..=steps {
            let value = lo + (hi - lo) * k as f64 / steps as f64;
            let mut moved = assembly.clone();
            if let Some(m) = moved.mates.iter_mut().find(|m| m.id == s.mate) {
                m.value = value;
            }
            let found = clashes_of(&moved, &solids, step)?;
            let involved: Vec<&super::interference::Clash> = found
                .iter()
                .filter(|c| {
                    moved.is_ancestor(mate.child, c.a) || moved.is_ancestor(mate.child, c.b)
                })
                .collect();
            sweep.push(json!({ "value": value, "clashes": involved }));
        }
    }
    KernelReply::ok(&json!({ "clashes": now, "sweep": sweep }))
}

fn cut_list(document: &Document) -> Result<KernelReply, CoreError> {
    let solids = solids(document);
    let assembly = document.assembly();
    let rows: Vec<Value> = assembly
        .parts
        .iter()
        .map(|part| {
            let profile = solids.profiles.get(&part.id);
            let sheet = document.entity(part.outline).map(|e| e.sheet).ok();
            json!({
                "part": part.id,
                "name": part.name,
                "quantity": assembly.instances.iter().filter(|i| i.part == part.id).count(),
                "length": profile.map(|p| { let [lo, hi] = p.bounds(); hi[0] - lo[0] }),
                "width": profile.map(|p| { let [lo, hi] = p.bounds(); hi[1] - lo[1] }),
                "thickness": profile.map(|p| p.thickness),
                "sheet": sheet,
                "grain": part.grain,
            })
        })
        .collect();
    KernelReply::ok(&json!({ "rows": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{rect, Point};

    fn run(doc: &mut Document, name: &str, args: Value) -> Value {
        match dispatch(doc, name, &args.to_string()).unwrap_or_else(|e| panic!("{name}: {e}")) {
            KernelReply::Json(text) => {
                serde_json::from_str::<Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(_) => Value::Null,
        }
    }

    fn rectangle(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
        let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
        let sheet = doc.active_sheet();
        let layer = doc.active_layer();
        doc.begin("test");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        id
    }

    #[test]
    fn a_cabinet_side_with_a_door_that_hits_a_shelf() {
        let mut doc = Document::new();
        let side = rectangle(&mut doc, 0.0, 0.0, 400.0, 700.0);
        let door = rectangle(&mut doc, 500.0, 0.0, 900.0, 700.0);
        let shelf = rectangle(&mut doc, 0.0, 800.0, 400.0, 1100.0);
        let s = run(
            &mut doc,
            "assembly.makePart",
            json!({ "ids": [side], "name": "Side" }),
        );
        let d = run(
            &mut doc,
            "assembly.makePart",
            json!({ "ids": [door], "name": "Door" }),
        );
        let sh = run(
            &mut doc,
            "assembly.makePart",
            json!({ "ids": [shelf], "name": "Shelf" }),
        );
        let side_i = s["instance"].as_u64().expect("side");
        let door_i = d["instance"].as_u64().expect("door");
        let shelf_i = sh["instance"].as_u64().expect("shelf");
        // The door hangs on the side's front edge (edge 1, x = 400), its own
        // edge 3 against it, hinged about the edge's vertical axis.
        let hinge = run(
            &mut doc,
            "assembly.mate",
            json!({
                "kind": "hinge",
                "parent": side_i, "parentFace": { "kind": "edge", "index": 1 },
                "child": door_i, "childFace": { "kind": "edge", "index": 3 },
                "min": -120.0, "max": 120.0
            }),
        );
        let hinge_id = hinge["id"].as_u64().expect("hinge id");
        // The shelf lies flat on the side's top face, well clear of the door.
        run(
            &mut doc,
            "assembly.mate",
            json!({
                "parent": side_i, "parentFace": { "kind": "top" },
                "child": shelf_i, "childFace": { "kind": "bottom" },
                "offset": [0.0, 0.0]
            }),
        );
        let got = run(&mut doc, "assembly.get", json!({}));
        assert_eq!(got["parts"].as_array().map(Vec::len), Some(3));
        assert!(got["instances"][1]["matrix"].is_array());
        // Swinging the door through its range must hit the shelf somewhere.
        let sweep = run(
            &mut doc,
            "assembly.interference",
            json!({ "step": 4.0, "sweep": { "mate": hinge_id, "steps": 12 } }),
        );
        let steps = sweep["sweep"].as_array().expect("sweep");
        assert_eq!(steps.len(), 13);
        assert!(steps
            .iter()
            .any(|s| !s["clashes"].as_array().expect("c").is_empty()));
        assert!(steps
            .iter()
            .any(|s| s["clashes"].as_array().expect("c").is_empty()));
        // Driving clamps to the limits and leaves no undo step.
        let depth = doc.journal().undo_depth();
        let driven = run(
            &mut doc,
            "assembly.drive",
            json!({ "id": hinge_id, "value": 500.0 }),
        );
        assert_eq!(driven["value"], json!(120.0));
        assert_eq!(doc.journal().undo_depth(), depth);
        // Undo takes the last mate back.
        doc.undo();
        assert_eq!(doc.assembly().mates.len(), 1);
    }

    #[test]
    fn a_mate_to_a_missing_face_is_refused() {
        let mut doc = Document::new();
        let a = rectangle(&mut doc, 0.0, 0.0, 100.0, 100.0);
        let b = rectangle(&mut doc, 200.0, 0.0, 300.0, 100.0);
        let pa = run(&mut doc, "assembly.makePart", json!({ "ids": [a] }));
        let pb = run(&mut doc, "assembly.makePart", json!({ "ids": [b] }));
        let bad = dispatch(
            &mut doc,
            "assembly.mate",
            &json!({
                "parent": pa["instance"], "parentFace": { "kind": "edge", "index": 9 },
                "child": pb["instance"], "childFace": { "kind": "top" }
            })
            .to_string(),
        );
        assert!(bad.is_err());
    }

    #[test]
    fn meshes_frame_every_part() {
        let mut doc = Document::new();
        let outline = rectangle(&mut doc, 0.0, 0.0, 300.0, 200.0);
        let hole = rectangle(&mut doc, 50.0, 50.0, 80.0, 80.0);
        let dado = rectangle(&mut doc, -10.0, 90.0, 310.0, 108.0);
        run(
            &mut doc,
            "assembly.makePart",
            json!({ "ids": [outline, hole, dado], "pockets": [{ "vector": dado, "depth": 6.0 }] }),
        );
        let Ok(KernelReply::Bytes(bytes)) = dispatch(&mut doc, "assembly.meshes", "{}") else {
            panic!("expected bytes");
        };
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
        assert_eq!(header["sections"].as_array().map(Vec::len), Some(4));
        assert_eq!(header["instances"].as_array().map(Vec::len), Some(1));
    }
}
