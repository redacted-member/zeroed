//! Kernel commands of the Assembly View's pieces (`assembly.pieces`,
//! `assembly.pieceRelief`, `assembly.placePiece`, `assembly.matePieces`,
//! `assembly.checkPieces`, `assembly.pieceCutList`, `assembly.pieceSource`,
//! `assembly.addComponentPieces`, `assembly.removeComponentPiece`,
//! `assembly.verifyFit`, `assembly.measure`, `assembly.exportAssembly`,
//! `assembly.pieceState`, `assembly.editMate`, `assembly.deleteMate`,
//! `assembly.setExplodeSteps`, `assembly.solvePlace`,
//! `assembly.componentRelief`, `assembly.componentThumbnail`,
//! `assembly.dropPlace`).
//! Placing or mating a piece, editing or deleting a mate, hiding, showing
//! or grounding pieces, changing a sheet's piece source and
//! adding or removing component pieces are one undo step each; the rest
//! only read. Placing a piece and changing a mate solve the mates again
//! (see [`super::mates`]) in the same step.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cam::sim::{Quality, Stock, THROUGH};
use crate::clipart::thumbnail;
use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::Point;
use crate::kernel::KernelReply;
use crate::model::field::Field;

use super::exact::ExactSolid;
use super::mate_faces;
use super::mates::{self, MateFace, MateReport, PieceMate, PieceMateKind, SolvePiece};
use super::pieces::{
    self, Anchor, MateOptions, PartsCache, Piece, PieceInfo, PieceRef, PlacedRelief,
};
use super::pose::Vec3;
use super::steps::ExplodeStep;
use super::Assembly;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "assembly.pieces"
            | "assembly.pieceRelief"
            | "assembly.placePiece"
            | "assembly.matePieces"
            | "assembly.checkPieces"
            | "assembly.pieceCutList"
            | "assembly.pieceSource"
            | "assembly.pieceFace"
            | "assembly.addComponentPieces"
            | "assembly.removeComponentPiece"
            | "assembly.verifyFit"
            | "assembly.measure"
            | "assembly.exportAssembly"
            | "assembly.pieceState"
            | "assembly.editMate"
            | "assembly.deleteMate"
            | "assembly.setExplodeSteps"
            | "assembly.solvePlace"
            | "assembly.componentRelief"
            | "assembly.componentThumbnail"
            | "assembly.dropPlace"
    )
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QualityArgs {
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReliefArgs {
    #[serde(flatten)]
    piece: PieceRef,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceArgs {
    #[serde(flatten)]
    piece: PieceRef,
    position: Vec3,
    rotation: Vec3,
    /// The quality the caller shows the pieces at, so the parts it looks the
    /// piece up in are the ones already simulated.
    #[serde(default)]
    quality: Quality,
}

/// A face picked on a piece: the piece and a point on its surface in its
/// local frame.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FaceArgs {
    #[serde(flatten)]
    piece: PieceRef,
    point: Vec3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PieceFaceArgs {
    #[serde(flatten)]
    face: FaceArgs,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MateArgs {
    /// The face that stays put.
    target: FaceArgs,
    /// The face whose piece moves to meet it.
    moving: FaceArgs,
    /// What the mate holds.
    #[serde(default)]
    kind: PieceMateKind,
    /// The gap (mm) or, for an angle mate, the angle (degrees).
    #[serde(default, alias = "value")]
    offset: f64,
    /// The mate this one takes the place of (changing a mate just made).
    #[serde(default)]
    replace: Option<u64>,
    /// Turn the moving piece over: its face opposite the picked one meets
    /// the target.
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    turn: Option<f64>,
    #[serde(default)]
    quality: Quality,
}

/// What `assembly.pieceState` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StateAction {
    Hide,
    Show,
    ShowAll,
    Ground,
    Unground,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateArgs {
    action: StateAction,
    /// The piece; every piece for Show All.
    #[serde(default)]
    piece: Option<PieceRef>,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditMateArgs {
    id: u64,
    #[serde(default)]
    kind: Option<PieceMateKind>,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    aligned: Option<bool>,
    #[serde(default)]
    suppressed: Option<bool>,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepsArgs {
    /// Every step, in order; a step with id 0 is new.
    steps: Vec<super::steps::ExplodeStep>,
    /// The undo step's name: what changed.
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteMateArgs {
    id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceArgs {
    sheet: u64,
    /// True for the sheet's raw model, false for its toolpath simulation.
    model: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddComponentsArgs {
    /// Components to show as pieces of their own.
    components: Vec<u64>,
    /// Where the first one lands: the middle of its underside, assembly
    /// space (a point on a piece or the ground under the pointer).
    #[serde(default)]
    at: Option<Vec3>,
    /// Stand each one on the ground beside the pieces (after the one put
    /// `at` a point, if any); left out, a piece lies where its component
    /// lies on its sheet.
    #[serde(default)]
    beside: bool,
    /// The first one's place and turns, as `assembly.dropPlace` answers
    /// them; before `at`.
    #[serde(default)]
    pose: Option<PoseArgs>,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoseArgs {
    position: Vec3,
    rotation: Vec3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DropArgs {
    /// The component let go, a piece already or not.
    component: u64,
    /// The assembly point under the pointer.
    point: Vec3,
    /// The piece the point is on; left out for the ground.
    #[serde(default)]
    onto: Option<PieceRef>,
    #[serde(default)]
    quality: Quality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentArgs {
    component: u64,
    /// Thumbnail size in pixels.
    #[serde(default)]
    size: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveComponentArgs {
    component: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckArgs {
    /// Smallest gap wanted between pieces, millimetres.
    #[serde(default)]
    clearance: f64,
    #[serde(default)]
    quality: Quality,
}

/// Runs one piece command; `cache` keeps each sheet's parts between them.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] for a piece that does not exist,
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses.
pub fn dispatch(
    document: &mut Document,
    cache: &mut PartsCache,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "assembly.pieces" => {
            let args: QualityArgs = parse(name, args)?;
            list(document, cache, args.quality)
        }
        "assembly.pieceRelief" => {
            let args: ReliefArgs = parse(name, args)?;
            let info = find(document, cache, args.piece, args.quality)?;
            let stock = part_stock(&info)?;
            Ok(KernelReply::Bytes(stock.to_blob(json!({
                "revision": document.revision(),
                "sheet": args.piece.sheet,
                "part": args.piece.part,
                "component": args.piece.component,
            }))))
        }
        "assembly.placePiece" => place(document, cache, parse(name, args)?),
        "assembly.solvePlace" => solve_place(document, cache, parse(name, args)?),
        "assembly.matePieces" => mate(document, cache, parse(name, args)?),
        "assembly.checkPieces" => check(document, cache, parse(name, args)?),
        "assembly.pieceCutList" => {
            let args: QualityArgs = parse(name, args)?;
            cut_list(document, cache, args.quality)
        }
        "assembly.pieceSource" => source(document, parse(name, args)?),
        "assembly.pieceFace" => {
            let args: PieceFaceArgs = parse(name, args)?;
            let info = find(document, cache, args.face.piece, args.quality)?;
            let face = pieces::face_at(&part_stock(&info)?, args.face.point);
            KernelReply::ok(&json!({
                "face": face.map(|f| json!({ "point": f.point, "normal": f.normal })),
            }))
        }
        "assembly.addComponentPieces" => add_components(document, cache, parse(name, args)?),
        "assembly.dropPlace" => drop_place(document, cache, parse(name, args)?),
        "assembly.removeComponentPiece" => remove_component(document, parse(name, args)?),
        "assembly.componentRelief" => {
            let args: ComponentArgs = parse(name, args)?;
            let (stock, size) = component_relief(document, cache, args.component)?;
            Ok(KernelReply::Bytes(stock.to_blob(json!({
                "revision": document.revision(),
                "component": args.component,
                "size": size,
            }))))
        }
        "assembly.componentThumbnail" => {
            let args: ComponentArgs = parse(name, args)?;
            let size = args.size.unwrap_or(THUMBNAIL_PIXELS);
            if !(16..=thumbnail::MAX_THUMBNAIL_SIZE).contains(&size) {
                return Err(CoreError::validation(format!(
                    "a thumbnail is 16 to {} pixels",
                    thumbnail::MAX_THUMBNAIL_SIZE
                )));
            }
            let (stock, _) = component_relief(document, cache, args.component)?;
            let png = thumbnail::png(&thumbnail::relief(&relief_field(&stock), size))?;
            Ok(KernelReply::Bytes(png_blob(&png, args.component, size)))
        }
        "assembly.verifyFit" => super::fit::verify(document, cache, args),
        "assembly.measure" => super::measure::measure(document, cache, args),
        "assembly.exportAssembly" => super::export::export(document, cache, args),
        "assembly.pieceState" => piece_state(document, cache, parse(name, args)?),
        "assembly.editMate" => edit_mate(document, cache, parse(name, args)?),
        "assembly.deleteMate" => delete_mate(document, parse(name, args)?),
        "assembly.setExplodeSteps" => set_steps(document, cache, parse(name, args)?),
        other => Err(CoreError::UnknownCommand(other.to_string())),
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

/// The piece named `key`, with its pose and parts.
fn find(
    document: &Document,
    cache: &mut PartsCache,
    key: PieceRef,
    quality: Quality,
) -> Result<PieceInfo, CoreError> {
    let all = pieces::pieces(document, cache, quality.cells())?;
    if let Some(component) = key.component {
        return all
            .into_iter()
            .find(|info| info.piece.component == Some(component))
            .ok_or_else(|| CoreError::validation(format!("component {component} is not a piece")));
    }
    if !all.iter().any(|info| info.piece.sheet == key.sheet) {
        return Err(CoreError::validation(format!(
            "sheet {} has no toolpaths or model, so it makes no piece",
            key.sheet
        )));
    }
    all.into_iter()
        .find(|info| info.piece.key() == key)
        .ok_or_else(|| {
            CoreError::validation(format!("sheet {} has no part {}", key.sheet, key.part))
        })
}

fn part_stock(info: &PieceInfo) -> Result<Stock, CoreError> {
    info.stock()
        .ok_or_else(|| CoreError::validation("that part is gone"))
}

fn sheet_name(document: &Document, sheet: u64) -> String {
    document
        .sheet(sheet)
        .map(|s| s.name.clone())
        .unwrap_or_default()
}

/// What every piece row starts with: which piece and what it is called.
fn naming(document: &Document, info: &PieceInfo) -> Value {
    let component = info.piece.component;
    json!({
        "sheet": info.piece.sheet,
        "part": info.piece.part,
        "component": component,
        "componentName": component
            .and_then(|id| document.component(id).ok())
            .map(|c| c.name.clone()),
        "parts": info.parts,
        "sheetName": sheet_name(document, info.piece.sheet),
        "vectorName": if info.is_model() || component.is_some() {
            None
        } else {
            pieces::vector_name(document, &info.sheet_parts, info.local)
        },
        "model": info.is_model(),
    })
}

/// The components that can still be added as pieces: every component not
/// yet one, with its sheet, in component list order, and `look`, which
/// changes when the component's shape does (its thumbnail is stale then).
fn addable(document: &Document) -> Vec<Value> {
    let taken = &document.assembly().components;
    document
        .all_components()
        .into_iter()
        .filter(|c| !taken.contains(&c.id))
        .map(|c| {
            let sheet = document.sheet_of_component(c);
            json!({
                "id": c.id,
                "name": c.name,
                "sheet": sheet,
                "sheetName": sheet_name(document, sheet),
                "look": format!("{:016x}", document.component_fingerprint(c.id)),
            })
        })
        .collect()
}

fn list(
    document: &Document,
    cache: &mut PartsCache,
    quality: Quality,
) -> Result<KernelReply, CoreError> {
    let rows: Vec<Value> = pieces::pieces(document, cache, quality.cells())?
        .iter()
        .map(|info| {
            let mut row = naming(document, info);
            if let Some(object) = row.as_object_mut() {
                object.insert("position".into(), json!(info.piece.position));
                object.insert("rotation".into(), json!(info.piece.rotation));
                object.insert("size".into(), json!(info.size));
                object.insert("hasModel".into(), json!(info.has_model));
                object.insert("hasToolpaths".into(), json!(info.has_toolpaths));
                object.insert("hidden".into(), json!(info.piece.hidden));
                object.insert("grounded".into(), json!(info.piece.grounded));
            }
            row
        })
        .collect();
    let infos = pieces::pieces(document, cache, quality.cells())?;
    KernelReply::ok(&json!({
        "pieces": rows,
        "components": addable(document),
        "mates": mate_rows(document.assembly(), &infos),
        "explodeSteps": steps_now(&document.assembly().explode_steps, &infos),
        "revision": document.revision(),
    }))
}

/// Each mate with how it stands, in the order they were made, its ends
/// named by the pieces they join now.
fn mate_rows(assembly: &Assembly, infos: &[PieceInfo]) -> Vec<Value> {
    let solving = solve_pieces(infos);
    let now = mates_now(assembly, infos);
    let reports = mates::report(&solving, &now);
    assembly
        .piece_mates
        .iter()
        .zip(&now)
        .zip(reports)
        .map(|((saved, mate), report)| {
            let end = |now: PieceRef, saved: PieceRef| if now.part == GONE { saved } else { now };
            json!({
                "id": mate.id,
                "kind": mate.kind,
                "a": end(mate.a, saved.a),
                "b": end(mate.b, saved.b),
                "value": mate.value,
                "aligned": mate.aligned,
                "suppressed": mate.suppressed,
                "state": report.state,
                "distanceError": report.distance_error,
                "angleError": report.angle_error,
                "radius": [mate.a_face.radius, mate.b_face.radius],
                "joint": mate.joint,
            })
        })
        .collect()
}

/// The part number of a mate end whose piece is gone: no piece has it.
const GONE: u32 = u32::MAX;

/// The anchor a mate end keeps for the piece `info`: its part's; none for
/// a component piece or a raw model piece, which their ids already name.
pub(crate) fn end_anchor(info: &PieceInfo) -> Option<Anchor> {
    if info.piece.component.is_some() || info.is_model() {
        None
    } else {
        info.sheet_parts.anchor(info.local)
    }
}

/// The piece a mate end joins now, when it is still there.
fn end_now<'a>(
    key: PieceRef,
    anchor: Option<&Anchor>,
    infos: &'a [PieceInfo],
) -> Option<&'a PieceInfo> {
    pieces::follow(key, anchor, infos).and_then(|at| infos.get(at))
}

/// Mates as the solver takes them: each end on the piece it joins now,
/// however a toolpath edit has numbered the parts; an end whose piece is
/// gone names no piece, so its mate reads as missing. A joint mate gets
/// what it holds from its joint.
pub(crate) fn mates_now(assembly: &Assembly, infos: &[PieceInfo]) -> Vec<PieceMate> {
    let mates = &assembly.piece_mates;
    let end = |key: PieceRef, anchor: Option<&Anchor>| {
        end_now(key, anchor, infos).map_or(
            PieceRef {
                part: GONE,
                component: None,
                ..key
            },
            |info| info.piece.key(),
        )
    };
    let mut now: Vec<PieceMate> = mates
        .iter()
        .map(|mate| PieceMate {
            a: end(mate.a, mate.a_anchor.as_ref()),
            b: end(mate.b, mate.b_anchor.as_ref()),
            ..mate.clone()
        })
        .collect();
    super::joint_mates::fill_locks(&assembly.joint_set, &mut now, infos);
    now
}

/// Mates to keep: each end whose piece is still there saved with its
/// number and anchor as they are now, so the anchor follows a part that
/// moves a little at a time; an end whose piece is gone stays as saved, in
/// case the part comes back.
fn mates_kept(mates: &[PieceMate], infos: &[PieceInfo]) -> Vec<PieceMate> {
    mates
        .iter()
        .map(|mate| {
            let mut kept = mate.clone();
            if let Some(info) = end_now(mate.a, mate.a_anchor.as_ref(), infos) {
                kept.a = info.piece.key();
                kept.a_anchor = end_anchor(info);
            }
            if let Some(info) = end_now(mate.b, mate.b_anchor.as_ref(), infos) {
                kept.b = info.piece.key();
                kept.b_anchor = end_anchor(info);
            }
            kept
        })
        .collect()
}

/// Explode steps as the Explode slider plays them: each piece on the part
/// it moves now, however a toolpath edit has numbered the parts, with that
/// part's anchor now; a piece that is gone is left out.
fn steps_now(steps: &[ExplodeStep], infos: &[PieceInfo]) -> Vec<ExplodeStep> {
    steps
        .iter()
        .map(|step| {
            let (pieces, anchors) = step
                .pieces
                .iter()
                .zip(step_anchors(step))
                .filter_map(|(key, anchor)| end_now(*key, anchor.as_ref(), infos))
                .map(|info| (info.piece.key(), end_anchor(info)))
                .unzip();
            ExplodeStep {
                pieces,
                anchors,
                ..step.clone()
            }
        })
        .collect()
}

/// A step's anchors, one per piece (none where it was saved without).
fn step_anchors(step: &ExplodeStep) -> Vec<Option<Anchor>> {
    let mut anchors = step.anchors.clone();
    anchors.resize(step.pieces.len(), None);
    anchors
}

/// Explode steps to keep: each piece still there saved with its part's
/// number and anchor as they are now, so the anchor follows a part that
/// moves a little at a time; a piece that is gone stays as saved, in case
/// the part comes back. `before` are the steps kept so far: a piece of the
/// same step that is gone now, which the list left out, is kept too.
fn steps_kept(
    steps: &[ExplodeStep],
    before: &[ExplodeStep],
    infos: &[PieceInfo],
) -> Vec<ExplodeStep> {
    steps
        .iter()
        .map(|step| {
            let mut ends: Vec<(PieceRef, Option<Anchor>)> = step
                .pieces
                .iter()
                .zip(step_anchors(step))
                .map(
                    |(key, anchor)| match end_now(*key, anchor.as_ref(), infos) {
                        Some(info) => (info.piece.key(), end_anchor(info)),
                        None => (*key, anchor),
                    },
                )
                .collect();
            let old = before.iter().find(|b| b.id != 0 && b.id == step.id);
            for (key, anchor) in old
                .map(|b| b.pieces.iter().zip(step_anchors(b)))
                .into_iter()
                .flatten()
            {
                let gone = end_now(*key, anchor.as_ref(), infos).is_none();
                // Its saved number may be another part's now: the anchor
                // tells them apart.
                if gone && !ends.iter().any(|(k, a)| k == key && *a == anchor) {
                    ends.push((*key, anchor));
                }
            }
            let (pieces, anchors) = ends.into_iter().unzip();
            ExplodeStep {
                pieces,
                anchors,
                ..step.clone()
            }
        })
        .collect()
}

fn solve_pieces(infos: &[PieceInfo]) -> Vec<SolvePiece> {
    infos
        .iter()
        .map(|info| SolvePiece {
            key: info.piece.key(),
            position: info.piece.position,
            rotation: info.piece.rotation,
            size: info.size,
            grounded: info.piece.grounded,
        })
        .collect()
}

/// Puts pieces' places into `assembly`, each with the anchor of its part
/// so the place stays with that part: over the stored placement the piece
/// took, and in place of older ones saved without an anchor under the same
/// number. The infos must come from `assembly` as it stands.
fn put_all(assembly: &mut Assembly, placed: &[(&PieceInfo, Piece)]) {
    let mut slots: Vec<Option<Piece>> = assembly.pieces.iter().copied().map(Some).collect();
    let mut mine = Vec::new();
    for (info, piece) in placed {
        let mut piece = *piece;
        piece.component = info.piece.component;
        piece.anchor = if piece.component.is_some() {
            None
        } else {
            info.sheet_parts.anchor(info.local)
        };
        match info.stored.and_then(|i| slots.get_mut(i)) {
            Some(slot) => *slot = Some(piece),
            None => slots.push(Some(piece)),
        }
        mine.push((info.stored.unwrap_or(slots.len() - 1), piece));
    }
    for (at, piece) in &mine {
        for (i, slot) in slots.iter_mut().enumerate() {
            let stale = slot.is_some_and(|p| {
                p.anchor.is_none()
                    && p.sheet == piece.sheet
                    && p.part == piece.part
                    && p.component == piece.component
            });
            if i != *at && stale && !mine.iter().any(|(m, _)| *m == i) {
                *slot = None;
            }
        }
    }
    assembly.pieces = slots.into_iter().flatten().collect();
}

/// Commits `assembly` as one undo step labelled `label`.
fn commit(document: &mut Document, assembly: Assembly, label: &str) -> u64 {
    document.begin(label);
    document.set_assembly(assembly);
    document.commit()
}

/// Solves `assembly`'s mates over the pieces `infos` lists, with `moved`
/// placed first (the piece the user just moved, the solve's driver), and
/// puts every piece that moves (and `moved`) into `assembly`, with each
/// mate end saved on the piece it joins now. Answers the place each piece
/// ends at and how each mate stands.
pub(crate) fn solve_into(
    assembly: &mut Assembly,
    infos: &[PieceInfo],
    moved: Option<Piece>,
) -> (Vec<SolvePiece>, Vec<MateReport>) {
    let mut solving = solve_pieces(infos);
    if let Some(piece) = moved {
        if let Some(p) = solving.iter_mut().find(|p| p.key == piece.key()) {
            p.position = piece.position;
            p.rotation = piece.rotation;
        }
    }
    let now = mates_now(assembly, infos);
    let reports = mates::solve(&mut solving, &now, moved.map(|p| p.key()));
    let changed: Vec<(&PieceInfo, Piece)> = infos
        .iter()
        .zip(&solving)
        .filter(|(info, solved)| {
            moved.is_some_and(|m| m.key() == info.piece.key())
                || solved.position != info.piece.position
                || solved.rotation != info.piece.rotation
        })
        .map(|(info, solved)| {
            (
                info,
                Piece {
                    position: solved.position,
                    rotation: solved.rotation,
                    ..info.piece
                },
            )
        })
        .collect();
    put_all(assembly, &changed);
    assembly.piece_mates = mates_kept(&assembly.piece_mates, infos);
    assembly.explode_steps = steps_kept(&assembly.explode_steps, &[], infos);
    (solving, reports)
}

/// The assembly with the piece `args` names moved where it says and the
/// mates solved again, the pieces' places after the solve and the mates'
/// reports.
fn placed(
    document: &Document,
    cache: &mut PartsCache,
    args: &PlaceArgs,
) -> Result<(Assembly, Vec<SolvePiece>, Vec<MateReport>), CoreError> {
    let info = find(document, cache, args.piece, args.quality)?;
    let finite = args
        .position
        .iter()
        .chain(args.rotation.iter())
        .all(|v| v.is_finite());
    if !finite {
        return Err(CoreError::validation("a piece's place must be a number"));
    }
    if info.piece.grounded {
        return Err(CoreError::validation(
            "that piece is grounded; unground it to move it",
        ));
    }
    let piece = Piece {
        position: args.position,
        rotation: args.rotation,
        ..info.piece
    };
    let infos = pieces::pieces(document, cache, args.quality.cells())?;
    let mut assembly = document.assembly().clone();
    let (solved, reports) = solve_into(&mut assembly, &infos, Some(piece));
    Ok((assembly, solved, reports))
}

fn place(
    document: &mut Document,
    cache: &mut PartsCache,
    args: PlaceArgs,
) -> Result<KernelReply, CoreError> {
    let (assembly, _, _) = placed(document, cache, &args)?;
    let revision = commit(document, assembly, "Move Piece");
    KernelReply::ok(&json!({ "revision": revision }))
}

/// `assembly.solvePlace`: where every piece would end if the piece were
/// placed there, the mates solved again, for showing a drag live; each
/// piece with its place, and each mate's state. Reads only.
fn solve_place(
    document: &Document,
    cache: &mut PartsCache,
    args: PlaceArgs,
) -> Result<KernelReply, CoreError> {
    let (_, solved, reports) = placed(document, cache, &args)?;
    let places: Vec<Value> = solved
        .iter()
        .map(|p| json!({ "piece": p.key, "position": p.position, "rotation": p.rotation }))
        .collect();
    KernelReply::ok(&json!({
        "places": places,
        "conflicts": reports.iter().filter(|r| r.state == mates::MateState::Conflict).count(),
        "revision": document.revision(),
    }))
}

/// A piece's exact solid in its own frame, when it has exact edges.
fn exact_solid(document: &Document, info: &PieceInfo) -> Option<ExactSolid> {
    if info.is_model() || info.piece.component.is_some() {
        return None;
    }
    let sheet = super::exact::sheet_exact(document, info.piece.sheet).ok()?;
    super::exact::piece_solid(&sheet, &info.sheet_parts, info.local)
}

fn mate(
    document: &mut Document,
    cache: &mut PartsCache,
    args: MateArgs,
) -> Result<KernelReply, CoreError> {
    if args.target.piece == args.moving.piece {
        return Err(CoreError::validation(
            "pick a face on another piece to mate to",
        ));
    }
    if args.kind == PieceMateKind::Joint {
        return Err(CoreError::validation(
            "joint mates are made by the Joint Creator",
        ));
    }
    let finite = args
        .target
        .point
        .iter()
        .chain(args.moving.point.iter())
        .chain([args.offset].iter())
        .chain(args.turn.iter())
        .all(|v| v.is_finite());
    if !finite {
        return Err(CoreError::validation("a mate's values must be numbers"));
    }
    let mut target = find(document, cache, args.target.piece, args.quality)?;
    let mut moving = find(document, cache, args.moving.piece, args.quality)?;
    let (mut target_point, mut moving_point) = (args.target.point, args.moving.point);
    if moving.piece.grounded {
        if target.piece.grounded {
            return Err(CoreError::validation(
                "both pieces are grounded; unground one to mate them",
            ));
        }
        // The grounded piece stays; the other moves to meet it.
        std::mem::swap(&mut target, &mut moving);
        std::mem::swap(&mut target_point, &mut moving_point);
    }
    let target_stock = part_stock(&target)?;
    let moving_stock = part_stock(&moving)?;
    let target_solid = exact_solid(document, &target);
    let moving_solid = exact_solid(document, &moving);
    let target_pose = target.piece.pose(target.size);
    let (a_face, b_face, placed, turn) = if args.kind == PieceMateKind::Concentric {
        let no_axis = || CoreError::validation("pick the round side of a hole, pin or dowel");
        let a = mate_faces::round_axis(&target_stock, target_solid.as_ref(), target_point)
            .ok_or_else(no_axis)?;
        let b = mate_faces::round_axis(&moving_stock, moving_solid.as_ref(), moving_point)
            .ok_or_else(no_axis)?;
        (a, b, None, 0.0)
    } else {
        let no_face = || CoreError::validation("there is no face of the piece there");
        let target_face =
            mate_faces::plane_face(&target_stock, target_solid.as_ref(), target_point)
                .ok_or_else(no_face)?;
        let mut moving_face =
            mate_faces::plane_face(&moving_stock, moving_solid.as_ref(), moving_point)
                .ok_or_else(no_face)?;
        if args.flip {
            moving_face =
                mate_faces::opposite_plane(&moving_stock, moving_solid.as_ref(), &moving_face);
        }
        let face = |f: &pieces::PickedFace| MateFace {
            point: f.point,
            normal: f.normal,
            radius: None,
        };
        // Face against face brings the picked points together, turned the
        // quarter turn nearest how the piece stands; the other kinds move
        // the piece the least that meets them.
        let (placed, turn) = if args.kind == PieceMateKind::Against {
            let (placed, turn) = pieces::mate(
                &target_pose,
                &target_face,
                &moving.piece,
                moving.size,
                &moving_face,
                MateOptions {
                    offset: args.offset,
                    turn: args.turn,
                },
            );
            (Some(placed), turn)
        } else {
            (None, 0.0)
        };
        (face(&target_face), face(&moving_face), placed, turn)
    };
    let mut assembly = document.assembly().clone();
    let moving_pose = placed.map_or(moving.piece.pose(moving.size), |p| p.pose(moving.size));
    let (_, aligned) =
        mates::current_value(args.kind, &a_face, &target_pose, &b_face, &moving_pose);
    let id = match args.replace {
        Some(id) if assembly.piece_mates.iter().any(|m| m.id == id) => id,
        _ => assembly.take_id(),
    };
    let made = PieceMate {
        id,
        kind: args.kind,
        a: target.piece.key(),
        a_face,
        b: moving.piece.key(),
        b_face,
        value: args.offset,
        aligned,
        suppressed: false,
        a_anchor: end_anchor(&target),
        b_anchor: end_anchor(&moving),
        joint: None,
        part: None,
        lock: None,
    };
    match assembly.piece_mates.iter_mut().find(|m| m.id == id) {
        Some(slot) => *slot = made,
        None => assembly.piece_mates.push(made),
    }
    let infos = pieces::pieces(document, cache, args.quality.cells())?;
    // Face against face has placed the moving piece, which leads the
    // solve; for the other kinds the target stays and the moving piece
    // follows.
    let driver = placed.or(Some(target.piece));
    let (solved, reports) = solve_into(&mut assembly, &infos, driver);
    let at = solved.iter().find(|p| p.key == moving.piece.key()).copied();
    let state = reports.iter().find(|r| r.id == id).map(|r| r.state);
    let label = if args.replace.is_some() {
        "Edit Mate"
    } else {
        "Mate Pieces"
    };
    let revision = commit(document, assembly, label);
    KernelReply::ok(&json!({
        "revision": revision,
        "mate": id,
        "state": state,
        "moved": moving.piece.key(),
        "position": at.map(|p| p.position),
        "rotation": at.map(|p| p.rotation),
        "turn": turn,
    }))
}

/// Changes a mate (its kind, value, direction or whether it is
/// suppressed) and solves the mates again; one undo step.
fn edit_mate(
    document: &mut Document,
    cache: &mut PartsCache,
    args: EditMateArgs,
) -> Result<KernelReply, CoreError> {
    let mut assembly = document.assembly().clone();
    let mate = assembly
        .piece_mates
        .iter_mut()
        .find(|m| m.id == args.id)
        .ok_or_else(|| CoreError::validation(format!("there is no mate {}", args.id)))?;
    if let Some(kind) = args.kind {
        if (kind == PieceMateKind::Joint) != (mate.kind == PieceMateKind::Joint) {
            return Err(CoreError::validation(
                "a joint mate stays a joint mate; remove the joint in the Joint Creator",
            ));
        }
        let round = mate.a_face.radius.is_some() && mate.b_face.radius.is_some();
        if (kind == PieceMateKind::Concentric) != round {
            return Err(CoreError::validation(if round {
                "a mate between round axes can only be concentric"
            } else {
                "a concentric mate needs two round faces"
            }));
        }
        mate.kind = kind;
    }
    if let Some(value) = args.value {
        if !value.is_finite() {
            return Err(CoreError::validation("a mate's value must be a number"));
        }
        mate.value = value;
    }
    if let Some(aligned) = args.aligned {
        mate.aligned = aligned;
    }
    let label = match args.suppressed {
        Some(true) if !mate.suppressed => "Suppress Mate",
        Some(false) if mate.suppressed => "Unsuppress Mate",
        _ => "Edit Mate",
    };
    if let Some(suppressed) = args.suppressed {
        mate.suppressed = suppressed;
    }
    let infos = pieces::pieces(document, cache, args.quality.cells())?;
    solve_into(&mut assembly, &infos, None);
    let revision = commit(document, assembly, label);
    KernelReply::ok(&json!({ "revision": revision }))
}

/// Keeps the explode steps as given (new ones get an id), each piece with
/// its part's anchor; one undo step.
fn set_steps(
    document: &mut Document,
    cache: &mut PartsCache,
    args: StepsArgs,
) -> Result<KernelReply, CoreError> {
    let mut assembly = document.assembly().clone();
    let infos = pieces::pieces(document, cache, args.quality.cells())?;
    // The pieces come as the list named them, on the parts they are now;
    // their anchors are taken from those parts, never from the request.
    let mut given = args.steps;
    for step in &mut given {
        step.anchors.clear();
    }
    let steps = super::steps::tidy_steps(given)?;
    let mut steps = steps_kept(&steps, &assembly.explode_steps, &infos);
    for step in &mut steps {
        if step.id == 0 {
            step.id = assembly.take_id();
        }
    }
    if steps == assembly.explode_steps {
        return KernelReply::ok(&json!({ "revision": document.revision(), "steps": steps }));
    }
    assembly.explode_steps = steps.clone();
    let label = args
        .label
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| "Explode Steps".into());
    let revision = commit(document, assembly, &label);
    KernelReply::ok(&json!({ "revision": revision, "steps": steps }))
}

/// Deletes a mate; the pieces stay where they are. One undo step.
fn delete_mate(document: &mut Document, args: DeleteMateArgs) -> Result<KernelReply, CoreError> {
    let mut assembly = document.assembly().clone();
    let before = assembly.piece_mates.len();
    assembly.piece_mates.retain(|m| m.id != args.id);
    if assembly.piece_mates.len() == before {
        return Err(CoreError::validation(format!(
            "there is no mate {}",
            args.id
        )));
    }
    let revision = commit(document, assembly, "Delete Mate");
    KernelReply::ok(&json!({ "revision": revision }))
}

/// Hides, shows or grounds pieces; one undo step. Isolate is a view state
/// of the Assembly View and never changes what is saved hidden.
fn piece_state(
    document: &mut Document,
    cache: &mut PartsCache,
    args: StateArgs,
) -> Result<KernelReply, CoreError> {
    let infos = pieces::pieces(document, cache, args.quality.cells())?;
    let key = match (args.action, args.piece) {
        (StateAction::ShowAll, _) => None,
        (_, Some(key)) => {
            if !infos.iter().any(|i| i.piece.key() == key) {
                return Err(CoreError::validation("that piece is gone"));
            }
            Some(key)
        }
        (_, None) => return Err(CoreError::validation("name the piece")),
    };
    let is = |info: &PieceInfo| Some(info.piece.key()) == key;
    let changed: Vec<(&PieceInfo, Piece)> = infos
        .iter()
        .filter_map(|info| {
            let mut piece = info.piece;
            match args.action {
                StateAction::Hide if is(info) => piece.hidden = true,
                StateAction::Show if is(info) => piece.hidden = false,
                StateAction::ShowAll => piece.hidden = false,
                StateAction::Ground if is(info) => piece.grounded = true,
                StateAction::Unground if is(info) => piece.grounded = false,
                _ => {}
            }
            (piece != info.piece).then_some((info, piece))
        })
        .collect();
    if changed.is_empty() {
        return KernelReply::ok(&json!({ "revision": document.revision() }));
    }
    let mut assembly = document.assembly().clone();
    put_all(&mut assembly, &changed);
    let label = match args.action {
        StateAction::Hide => "Hide Piece",
        StateAction::Show => "Show Piece",
        StateAction::ShowAll => "Show All Pieces",
        StateAction::Ground => "Ground Piece",
        StateAction::Unground => "Unground Piece",
    };
    let revision = commit(document, assembly, label);
    KernelReply::ok(&json!({ "revision": revision }))
}

/// Sets whether a sheet's piece is its raw model or its toolpath
/// simulation, one undo step; a sheet without a model cannot give it.
fn source(document: &mut Document, args: SourceArgs) -> Result<KernelReply, CoreError> {
    document.sheet(args.sheet)?;
    let mut assembly = document.assembly().clone();
    let listed = assembly.model_sheets.contains(&args.sheet);
    if listed == args.model {
        return KernelReply::ok(&json!({ "revision": document.revision() }));
    }
    let own = assembly.components.clone();
    if args.model && pieces::model_stock_except(document, args.sheet, &own)?.is_none() {
        return Err(CoreError::validation(
            "the sheet has no model to use as its piece",
        ));
    }
    if args.model {
        assembly.model_sheets.push(args.sheet);
        assembly.model_sheets.sort_unstable();
    } else {
        assembly.model_sheets.retain(|s| *s != args.sheet);
    }
    document.begin("Piece Source");
    document.set_assembly(assembly);
    let revision = document.commit();
    KernelReply::ok(&json!({ "revision": revision }))
}

/// Adds components as pieces of their own, one undo step; components
/// already pieces are left as they are, and a flat one is refused.
fn add_components(
    document: &mut Document,
    cache: &mut PartsCache,
    args: AddComponentsArgs,
) -> Result<KernelReply, CoreError> {
    let numbers = args.at.iter().flatten().chain(
        args.pose
            .iter()
            .flat_map(|p| p.position.iter().chain(&p.rotation)),
    );
    if numbers.clone().any(|v| !v.is_finite()) {
        return Err(CoreError::validation("a place must be numbers"));
    }
    let mut assembly = document.assembly().clone();
    let mut added = Vec::new();
    for id in args.components {
        document.component(id)?;
        if assembly.components.contains(&id) || added.contains(&id) {
            continue;
        }
        if !pieces::component_has_relief(document, cache, id)? {
            return Err(CoreError::validation(format!(
                "component {id} is flat, so it makes no piece"
            )));
        }
        added.push(id);
    }
    if added.is_empty() {
        return KernelReply::ok(&json!({ "revision": document.revision(), "added": added }));
    }
    let mut boxes: Vec<[Vec3; 2]> = if args.beside {
        pieces::pieces(document, cache, args.quality.cells())?
            .iter()
            .map(|info| pieces::piece_bounds(&info.piece, info.size))
            .collect()
    } else {
        Vec::new()
    };
    for (index, id) in added.iter().enumerate() {
        let size = component_size(document, cache, *id)?;
        let (position, rotation) = match (args.pose, args.at) {
            (Some(pose), _) if index == 0 => (pose.position, pose.rotation),
            (None, Some(at)) if index == 0 => (pieces::resting_at(size, at), [0.0; 3]),
            _ if args.beside => (pieces::beside(&boxes, size), [0.0; 3]),
            _ => continue,
        };
        let piece = Piece {
            sheet: document.sheet_of_component(document.component(*id)?),
            part: 1,
            position,
            rotation,
            anchor: None,
            component: Some(*id),
            hidden: false,
            grounded: false,
        };
        boxes.push(pieces::piece_bounds(&piece, size));
        assembly.pieces.retain(|p| p.component != Some(*id));
        assembly.pieces.push(piece);
    }
    assembly.components.extend(added.iter().copied());
    document.begin("Add Piece");
    document.set_assembly(assembly);
    let revision = document.commit();
    KernelReply::ok(&json!({ "revision": revision, "added": added }))
}

/// `assembly.dropPlace`: where a component let go at a point lands, flush
/// against the face under it and clear of every piece (see
/// [`super::drop`]); a component that is a piece already keeps its turns
/// where it can. Reads only.
fn drop_place(
    document: &Document,
    cache: &mut PartsCache,
    args: DropArgs,
) -> Result<KernelReply, CoreError> {
    if args.point.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation("a place must be numbers"));
    }
    document.component(args.component)?;
    let all = reliefs(document, cache, args.quality)?;
    let own = all
        .iter()
        .find(|(info, _)| info.piece.component == Some(args.component));
    let (moving, size, stock) = match own {
        Some((info, stock)) => {
            if info.piece.grounded {
                return Err(CoreError::validation(
                    "that piece is grounded; unground it to move it",
                ));
            }
            (info.piece, info.size, stock.clone())
        }
        None => {
            let (stock, size) = component_relief(document, cache, args.component)?;
            let piece = Piece {
                sheet: document.sheet_of_component(document.component(args.component)?),
                part: 1,
                position: [0.0; 3],
                rotation: [0.0; 3],
                anchor: None,
                component: Some(args.component),
                hidden: false,
                grounded: false,
            };
            (piece, size, stock)
        }
    };
    // A new component leaves its sheet's raw model piece as it lands, so
    // that piece is checked as it will be without it, where it stands now.
    let leaving = if own.is_none() {
        model_leaving(document, cache, &all, moving.sheet, args.component)?
    } else {
        None
    };
    let others: Vec<PlacedRelief> = all
        .iter()
        .filter(|(info, _)| info.piece.component != Some(args.component))
        .filter_map(|(info, stock)| match &leaving {
            Some((key, left)) if *key == info.piece.key() => {
                left.as_ref().map(|(stock, pose)| PlacedRelief {
                    key: *key,
                    stock,
                    pose: *pose,
                })
            }
            _ => Some(PlacedRelief {
                key: info.piece.key(),
                stock,
                pose: info.piece.pose(info.size),
            }),
        })
        .collect();
    let onto = args.onto.and_then(|key| {
        all.iter()
            .find(|(info, _)| {
                info.piece.key() == key && info.piece.component != Some(args.component)
            })
            .map(|(info, stock)| super::drop::Surface {
                pose: info.piece.pose(info.size),
                stock,
            })
    });
    let placed = super::drop::drop_place(onto.as_ref(), args.point, &moving, size, &stock, &others);
    KernelReply::ok(&json!({
        "position": placed.position,
        "rotation": placed.rotation,
        "revision": document.revision(),
    }))
}

/// What is left of a raw model piece: its relief and its pose.
type Leftover = (Stock, super::pose::Pose);

/// The raw model piece of `sheet` among `all`, if it shows, as it will be
/// once component `id` is a piece: its key and its relief without the
/// component posed where the rest of its material stands now (none when
/// nothing else is left of it).
fn model_leaving(
    document: &Document,
    cache: &mut PartsCache,
    all: &[(PieceInfo, Stock)],
    sheet: u64,
    id: u64,
) -> Result<Option<(PieceRef, Option<Leftover>)>, CoreError> {
    let Some((info, old)) = all
        .iter()
        .find(|(info, _)| info.is_model() && info.piece.sheet == sheet)
    else {
        return Ok(None);
    };
    let mut except: Vec<u64> = document
        .assembly()
        .components
        .iter()
        .copied()
        .filter(|c| {
            document
                .component(*c)
                .is_ok_and(|c| document.sheet_of_component(c) == sheet)
        })
        .collect();
    except.push(id);
    let left = cache
        .model_leaving(document, sheet, &except)?
        .and_then(|parts| parts.part_stock(1))
        .map(|stock| {
            let shift = [
                stock.origin_x - old.origin_x,
                stock.origin_y - old.origin_y,
                stock.bottom - old.bottom,
            ];
            let pose = info
                .piece
                .pose(info.size)
                .then_local(&super::pose::Pose::translation(shift));
            (stock, pose)
        });
    Ok(Some((info.piece.key(), left)))
}

/// Pixels along a side of a component thumbnail when none is asked for.
const THUMBNAIL_PIXELS: u32 = 96;

/// A component's relief standing alone, cropped to its box, and the box's
/// size (width, height, thickness), whether or not it is a piece yet.
fn component_relief(
    document: &Document,
    cache: &mut PartsCache,
    id: u64,
) -> Result<(Stock, Vec3), CoreError> {
    let parts = cache
        .component(document, id)?
        .ok_or_else(|| CoreError::validation(format!("component {id} is flat")))?;
    let (Some(stock), Some(size)) = (parts.part_stock(1), parts.size(1)) else {
        return Err(CoreError::validation(format!("component {id} is flat")));
    };
    Ok((stock, size))
}

fn component_size(document: &Document, cache: &mut PartsCache, id: u64) -> Result<Vec3, CoreError> {
    component_relief(document, cache, id).map(|(_, size)| size)
}

/// A relief as a model field for drawing: empty where there is no material.
fn relief_field(stock: &Stock) -> Field {
    let z = stock
        .z
        .iter()
        .zip(&stock.floor)
        .map(|(z, floor)| if *floor < *z - THROUGH { *z } else { f32::NAN })
        .collect();
    Field {
        width: stock.width,
        height: stock.height,
        cell: stock.cell,
        origin: Point::new(stock.origin_x, stock.origin_y),
        z,
    }
}

/// A PNG as a kernel blob: a JSON header, then the file's bytes.
fn png_blob(png: &[u8], component: u64, size: u32) -> Vec<u8> {
    let header = json!({
        "component": component,
        "width": size,
        "height": size,
        "mime": "image/png",
        "sections": [{ "name": "png", "dtype": "u8", "offset": 0, "count": png.len() }],
    })
    .to_string()
    .into_bytes();
    let mut out = Vec::with_capacity(4 + header.len() + png.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(png);
    out
}

/// Takes a component piece out of the Assembly View (the component goes
/// back into its sheet's raw model), with its placement; one undo step.
fn remove_component(
    document: &mut Document,
    args: RemoveComponentArgs,
) -> Result<KernelReply, CoreError> {
    let mut assembly = document.assembly().clone();
    if !assembly.components.contains(&args.component) {
        return Err(CoreError::validation(format!(
            "component {} is not a piece",
            args.component
        )));
    }
    assembly.components.retain(|id| *id != args.component);
    assembly
        .pieces
        .retain(|p| p.component != Some(args.component));
    assembly
        .piece_mates
        .retain(|m| m.a.component != Some(args.component) && m.b.component != Some(args.component));
    document.begin("Remove Piece");
    document.set_assembly(assembly);
    let revision = document.commit();
    KernelReply::ok(&json!({ "revision": revision }))
}

/// Every piece's relief with its pose.
fn reliefs(
    document: &Document,
    cache: &mut PartsCache,
    quality: Quality,
) -> Result<Vec<(PieceInfo, Stock)>, CoreError> {
    pieces::pieces(document, cache, quality.cells())?
        .into_iter()
        .map(|info| part_stock(&info).map(|stock| (info, stock)))
        .collect()
}

fn check(
    document: &Document,
    cache: &mut PartsCache,
    args: CheckArgs,
) -> Result<KernelReply, CoreError> {
    if !args.clearance.is_finite() || args.clearance < 0.0 {
        return Err(CoreError::validation("the clearance must be zero or more"));
    }
    let all = reliefs(document, cache, args.quality)?;
    let placed: Vec<PlacedRelief> = all
        .iter()
        .map(|(info, stock)| PlacedRelief {
            key: info.piece.key(),
            stock,
            pose: info.piece.pose(info.size),
        })
        .collect();
    let issues = pieces::check(&placed, args.clearance);
    KernelReply::ok(&json!({ "issues": issues, "clearance": args.clearance }))
}

/// Each piece's finished size: from its exact cut edges when it has them,
/// else the simulation grid; with the area of its outline so identical
/// pieces can be grouped.
fn cut_list(
    document: &Document,
    cache: &mut PartsCache,
    quality: Quality,
) -> Result<KernelReply, CoreError> {
    let held = super::fit::solids(document, cache, quality)?;
    let rows: Vec<Value> = reliefs(document, cache, quality)?
        .iter()
        .zip(&held)
        .map(|((info, stock), piece)| {
            let (length, width, area, exact) = match &piece.solid {
                Some(solid) => {
                    let b = solid.bounds();
                    let outline = solid
                        .sections
                        .first()
                        .map_or(0.0, super::exact::shapes_area);
                    (Some(b[2] - b[0]), Some(b[3] - b[1]), outline, true)
                }
                None => {
                    let span = pieces::material_span(stock);
                    let cells = (0..stock.z.len())
                        .filter(|i| stock.floor[*i] < stock.z[*i] - crate::cam::sim::THROUGH)
                        .count();
                    (
                        span.map(|[lo, hi]| hi[0] - lo[0]),
                        span.map(|[lo, hi]| hi[1] - lo[1]),
                        cells as f64 * stock.cell * stock.cell,
                        false,
                    )
                }
            };
            // The length is the longer side, however the piece lies on the
            // sheet, so the same piece drawn turned lists (and groups) the
            // same.
            let (length, width) = match (length, width) {
                (Some(l), Some(w)) if w > l => (Some(w), Some(l)),
                other => other,
            };
            let mut row = naming(document, info);
            if let Some(object) = row.as_object_mut() {
                object.insert("length".into(), json!(length));
                object.insert("width".into(), json!(width));
                object.insert("thickness".into(), json!(info.size[2]));
                object.insert("area".into(), json!(area));
                object.insert("exact".into(), json!(exact));
            }
            row
        })
        .collect();
    KernelReply::ok(&json!({ "rows": rows }))
}

#[cfg(test)]
mod tests {
    use super::super::pieces::tests::{add, cut_out, document};
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{rect, Point};

    fn run(doc: &mut Document, cache: &mut PartsCache, name: &str, args: Value) -> Value {
        match dispatch(doc, cache, name, &args.to_string())
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        {
            KernelReply::Json(text) => {
                serde_json::from_str::<Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(bytes) => json!({ "bytes": bytes.len() }),
        }
    }

    /// A sheet with two parts held by tabs, a named vector round the second.
    fn two_parts() -> Document {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [8.0, 10.0, 45.0, 40.0], 11.0, Some(7.0)),
        );
        add(
            &mut doc,
            cut_out(sheet, [55.0, 10.0, 92.0, 40.0], 11.0, Some(7.0)),
        );
        let path = rect(Point::new(55.0, 10.0), Point::new(92.0, 40.0)).expect("rect");
        let layer = doc.active_layer();
        doc.begin("vector");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: 0,
            name: Some("Shelf".into()),
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        doc
    }

    #[test]
    fn each_part_is_a_piece_named_by_its_vector_or_number() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let rows = listed["pieces"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["part"], json!(1));
        assert_eq!(rows[1]["part"], json!(2));
        assert_eq!(rows[0]["parts"], json!(2));
        assert_eq!(rows[0]["sheet"], json!(sheet));
        assert_eq!(rows[0]["vectorName"], Value::Null);
        assert_eq!(rows[1]["vectorName"], json!("Shelf"));
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.pieceRelief",
            json!({ "sheet": sheet, "part": 2 }),
        );
        assert!(reply["bytes"].as_u64().unwrap_or(0) > 500);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.pieceRelief",
            &json!({ "sheet": sheet, "part": 3 }).to_string()
        )
        .is_err());
        let rows = run(&mut doc, &mut cache, "assembly.pieceCutList", json!({}))["rows"].clone();
        assert_eq!(rows.as_array().map(Vec::len), Some(2));
        let length = rows[0]["length"].as_f64().unwrap_or(0.0);
        assert!((length - 31.0).abs() < 2.0, "{length}");
        assert_eq!(rows[1]["vectorName"], json!("Shelf"));
    }

    /// A piece drawn standing up on the sheet lists its longer side as its
    /// length, as it would lying down.
    #[test]
    fn the_cut_list_length_is_the_longer_side() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [10.0, 5.0, 30.0, 45.0], 11.0, Some(7.0)),
        );
        let mut cache = PartsCache::default();
        let rows = run(&mut doc, &mut cache, "assembly.pieceCutList", json!({}))["rows"].clone();
        let length = rows[0]["length"].as_f64().unwrap_or(0.0);
        let width = rows[0]["width"].as_f64().unwrap_or(0.0);
        assert!(
            (length - 34.0).abs() < 2.0 && (width - 14.0).abs() < 2.0,
            "{rows}"
        );
    }

    #[test]
    fn placing_a_piece_looks_it_up_at_the_callers_quality() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        run(
            &mut doc,
            &mut cache,
            "assembly.pieces",
            json!({ "quality": "high" }),
        );
        let before = cache
            .get(&doc, sheet, Quality::High.cells())
            .expect("parts");
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": sheet, "part": 2, "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 0.0], "quality": "high" }),
        );
        let after = cache
            .get(&doc, sheet, Quality::High.cells())
            .expect("parts");
        assert!(std::sync::Arc::ptr_eq(&before, &after));
        let listed = run(
            &mut doc,
            &mut cache,
            "assembly.pieces",
            json!({ "quality": "high" }),
        );
        assert_eq!(listed["pieces"][1]["position"], json!([1.0, 2.0, 3.0]));
    }

    #[test]
    fn placing_and_mating_a_piece_is_saved_listed_and_undone() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": sheet, "part": 2, "position": [1.0, 2.0, 3.0], "rotation": [90.0, 0.0, 0.0] }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["position"], json!([1.0, 2.0, 3.0]));
        assert_eq!(listed["pieces"][1]["rotation"], json!([90.0, 0.0, 0.0]));
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert_eq!(saved["pieces"][0]["part"], json!(2));
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["rotation"], json!([0.0, 0.0, 0.0]));

        // Part 2's top face (local Z = thickness) against part 1's right
        // wall: part 2 turns to stand against it.
        let size = listed["pieces"][0]["size"].clone();
        let width = size[0].as_f64().unwrap_or(0.0);
        let height = size[1].as_f64().unwrap_or(0.0);
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            json!({
                "target": { "sheet": sheet, "part": 1, "point": [width, height / 2.0, 5.0] },
                "moving": { "sheet": sheet, "part": 2, "point": [10.0, 10.0, 10.0] },
                "offset": 1.0
            }),
        );
        assert!(reply["turn"].is_number());
        let rotation = reply["rotation"].clone();
        assert_ne!(rotation, json!([0.0, 0.0, 0.0]));
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["rotation"], rotation);
        // Now the pieces stand 1 mm apart: clear at zero clearance, too
        // close at 2 mm.
        let issues = run(
            &mut doc,
            &mut cache,
            "assembly.checkPieces",
            json!({ "clearance": 0.0 }),
        );
        assert_eq!(issues["issues"], json!([]));
        let issues = run(
            &mut doc,
            &mut cache,
            "assembly.checkPieces",
            json!({ "clearance": 2.0 }),
        );
        assert_eq!(
            issues["issues"][0]["a"],
            json!({ "sheet": sheet, "part": 1 })
        );
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["rotation"], json!([0.0, 0.0, 0.0]));
        // A piece cannot mate to itself.
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            &json!({
                "target": { "sheet": sheet, "part": 1, "point": [0.0, 0.0, 10.0] },
                "moving": { "sheet": sheet, "part": 1, "point": [0.0, 0.0, 10.0] }
            })
            .to_string()
        )
        .is_err());
    }

    fn right_part_position(listed: &Value) -> Option<Value> {
        listed["pieces"]
            .as_array()?
            .iter()
            .find(|row| row["vectorName"] == json!("Shelf"))
            .map(|row| row["position"].clone())
    }

    #[test]
    fn a_placement_stays_on_its_part_when_toolpath_edits_renumber_the_parts() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": sheet, "part": 2, "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 0.0] }),
        );
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert!(saved["pieces"][0]["anchor"]["area"].as_f64().unwrap_or(0.0) > 500.0);
        // Deleting the left part's toolpath makes the shelf part 1: its
        // placement follows it.
        let left = doc.toolpath_order()[0];
        doc.begin("delete");
        doc.delete_toolpaths(&[left]).expect("delete");
        doc.commit();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"].as_array().map(Vec::len), Some(1));
        assert_eq!(listed["pieces"][0]["part"], json!(1));
        assert_eq!(right_part_position(&listed), Some(json!([1.0, 2.0, 3.0])));
        // Cutting the left part again numbers the shelf 2 again, and the
        // new left part lies where it is cut, unplaced.
        add(
            &mut doc,
            cut_out(sheet, [8.0, 10.0, 45.0, 40.0], 11.0, Some(7.0)),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(right_part_position(&listed), Some(json!([1.0, 2.0, 3.0])));
        assert_ne!(listed["pieces"][0]["position"], json!([1.0, 2.0, 3.0]));
        // Moving the shelf's cut 1 mm keeps the placement on it; placing
        // it again keeps one stored placement for it.
        let shelf = doc.toolpath_order()[0];
        doc.begin("edit");
        doc.replace_toolpath(
            shelf,
            cut_out(sheet, [56.0, 10.0, 93.0, 40.0], 11.0, Some(7.0)),
        )
        .expect("replace");
        doc.commit();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(right_part_position(&listed), Some(json!([1.0, 2.0, 3.0])));
        let part = listed["pieces"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["vectorName"] == json!("Shelf")))
            .map(|r| r["part"].clone())
            .expect("shelf");
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": sheet, "part": part, "position": [4.0, 5.0, 6.0], "rotation": [0.0, 0.0, 0.0] }),
        );
        assert_eq!(doc.assembly().pieces.len(), 1);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(right_part_position(&listed), Some(json!([4.0, 5.0, 6.0])));
    }

    #[test]
    fn flip_turns_the_moving_piece_over_so_its_other_side_meets_the_target() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let size = |row: usize, k: usize| listed["pieces"][row]["size"][k].as_f64().unwrap_or(0.0);
        let (w1, h1) = (size(0, 0), size(0, 1));
        let (w2, h2) = (size(1, 0), size(1, 1));
        // Part 2's top on part 1's top: without Flip it lies upside down on
        // it, with Flip the right way up, its underside on part 1's top.
        let mate = |flip: bool| {
            json!({
                "target": { "sheet": sheet, "part": 1, "point": [w1 / 2.0, h1 / 2.0, 10.0] },
                "moving": { "sheet": sheet, "part": 2, "point": [w2 / 2.0, h2 / 2.0, 10.0] },
                "flip": flip
            })
        };
        let plain = run(&mut doc, &mut cache, "assembly.matePieces", mate(false));
        let p1 = listed["pieces"][0]["position"][2].as_f64().unwrap_or(0.0);
        let z = plain["position"][2].as_f64().unwrap_or(0.0);
        assert!((z - (p1 + 10.0)).abs() < 1e-6, "{plain}");
        let upside_down = plain["rotation"].clone();
        // Apply again with Flip changes the mate just made, not a second one.
        let mut again = mate(true);
        again["replace"] = plain["mate"].clone();
        let flipped = run(&mut doc, &mut cache, "assembly.matePieces", again);
        assert_eq!(doc.assembly().piece_mates.len(), 1);
        let z = flipped["position"][2].as_f64().unwrap_or(0.0);
        assert!((z - (p1 + 10.0)).abs() < 1e-6, "{flipped}");
        assert_ne!(flipped["rotation"], upside_down);
        let turns = flipped["rotation"].as_array().expect("rotation");
        let level = |v: &Value| {
            let d = v.as_f64().unwrap_or(1.0).rem_euclid(360.0);
            d.min(360.0 - d) < 1e-6
        };
        assert!(level(&turns[0]) && level(&turns[1]), "{flipped}");
        // One undo step takes the flipped mate back to the plain one.
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["rotation"], upside_down);
    }

    #[test]
    fn the_face_under_a_point_is_answered_for_lighting() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.pieceFace",
            json!({ "sheet": sheet, "part": 1, "point": [5.0, 5.0, 10.0] }),
        );
        assert_eq!(reply["face"]["normal"], json!([0.0, 0.0, 1.0]));
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.pieceFace",
            json!({ "sheet": sheet, "part": 1, "point": [500.0, 5.0, 10.0] }),
        );
        assert_eq!(reply["face"], Value::Null);
    }

    #[test]
    fn a_sheet_can_give_its_raw_model_as_its_piece() {
        use crate::model::{field::Field, tree::ComponentExtra, Combine, Component};
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        // Without a model the sheet cannot give one.
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.pieceSource",
            &json!({ "sheet": sheet, "model": true }).to_string()
        )
        .is_err());
        let grid = crate::model::grid_for(doc.job_setup());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        field.z.fill(4.0);
        doc.begin("model");
        doc.add_component(Component {
            id: 0,
            name: "slab".into(),
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: ComponentExtra::default(),
            field,
        });
        doc.commit();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][0]["hasModel"], json!(true));
        assert_eq!(listed["pieces"][0]["model"], json!(false));
        run(
            &mut doc,
            &mut cache,
            "assembly.pieceSource",
            json!({ "sheet": sheet, "model": true }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let rows = listed["pieces"].as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["part"], json!(0));
        assert_eq!(rows[0]["model"], json!(true));
        let depth = rows[0]["size"][2].as_f64().unwrap_or(0.0);
        assert!((depth - 4.0).abs() < 1e-4, "{depth}");
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.pieceRelief",
            json!({ "sheet": sheet, "part": 0 }),
        );
        assert!(reply["bytes"].as_u64().unwrap_or(0) > 500);
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": sheet, "part": 0, "position": [7.0, 8.0, 9.0], "rotation": [0.0, 0.0, 0.0] }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][0]["position"], json!([7.0, 8.0, 9.0]));
        // Undo twice: the placement, then the source; the parts are back.
        assert!(doc.undo().is_some());
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_sheet_without_toolpaths_is_refused() {
        let mut doc = Document::new();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            &json!({ "sheet": sheet, "position": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0] })
                .to_string()
        )
        .is_err());
        assert_eq!(
            run(&mut doc, &mut cache, "assembly.pieces", json!({}))["pieces"],
            json!([])
        );
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.checkPieces",
            &json!({ "clearance": -1.0 }).to_string()
        )
        .is_err());
    }

    /// A component on the sheet over the block (x0, y0)-(x1, y1), `height`
    /// mm high, hidden when `visible` is false.
    fn block_component(
        doc: &mut Document,
        name: &str,
        rect: [f64; 4],
        height: f32,
        visible: bool,
    ) -> u64 {
        use crate::model::{field::Field, tree::ComponentExtra, Combine, Component};
        let grid = crate::model::grid_for(doc.job_setup());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for row in 0..grid.height {
            for col in 0..grid.width {
                let x = grid.origin.x + col as f64 * grid.cell;
                let y = grid.origin.y + row as f64 * grid.cell;
                if x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3] {
                    field.z[row * grid.width + col] = height;
                }
            }
        }
        doc.begin("model");
        let id = doc.add_component(Component {
            id: 0,
            name: name.into(),
            visible,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: ComponentExtra::default(),
            field,
        });
        doc.commit();
        id
    }

    #[test]
    fn a_component_is_a_piece_of_its_own_placed_mated_checked_saved_and_undone() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        // A mockup of a bought part, hidden so it is not carved, and a
        // flat component that stands up from nothing.
        let screen = block_component(&mut doc, "Screen", [10.0, 10.0, 40.0, 30.0], 6.0, false);
        let flat = block_component(&mut doc, "Flat", [10.0, 10.0, 20.0, 20.0], 0.0, true);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"].as_array().map(Vec::len), Some(2));
        let addable: Vec<u64> = listed["components"]
            .as_array()
            .expect("components")
            .iter()
            .filter_map(|c| c["id"].as_u64())
            .collect();
        assert_eq!(addable, vec![screen, flat]);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            &json!({ "components": [flat] }).to_string()
        )
        .is_err());

        let before = doc.revision();
        run(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            json!({ "components": [screen] }),
        );
        assert_ne!(doc.revision(), before);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let rows = listed["pieces"].as_array().expect("rows");
        assert_eq!(rows.len(), 3);
        let row = &rows[2];
        assert_eq!(row["component"], json!(screen));
        assert_eq!(row["componentName"], json!("Screen"));
        assert_eq!(row["model"], json!(false));
        assert_eq!(row["vectorName"], Value::Null);
        let size: Vec<f64> = row["size"]
            .as_array()
            .expect("size")
            .iter()
            .filter_map(Value::as_f64)
            .collect();
        assert!(
            (size[0] - 30.0).abs() < 2.0 && (size[1] - 20.0).abs() < 2.0,
            "{size:?}"
        );
        assert!((size[2] - 6.0).abs() < 1e-4, "{size:?}");
        assert!(!listed["components"]
            .as_array()
            .expect("components")
            .iter()
            .any(|c| c["id"] == json!(screen)));
        let piece = json!({ "sheet": sheet, "part": 1, "component": screen });
        let reply = run(&mut doc, &mut cache, "assembly.pieceRelief", piece.clone());
        assert!(reply["bytes"].as_u64().unwrap_or(0) > 200);

        // Placed by value, then its top laid on part 1's top, turned over
        // so its underside sits there.
        let mut place = piece.clone();
        place["position"] = json!([200.0, 0.0, 50.0]);
        place["rotation"] = json!([0.0, 0.0, 0.0]);
        run(&mut doc, &mut cache, "assembly.placePiece", place);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][2]["position"], json!([200.0, 0.0, 50.0]));
        assert_eq!(listed["pieces"][1]["rotation"], json!([0.0, 0.0, 0.0]));
        let mut moving = piece.clone();
        moving["point"] = json!([size[0] / 2.0, size[1] / 2.0, 6.0]);
        let mated = run(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            json!({
                "target": { "sheet": sheet, "part": 1, "point": [5.0, 5.0, 10.0] },
                "moving": moving,
                "flip": true
            }),
        );
        let p1 = listed["pieces"][0]["position"][2].as_f64().unwrap_or(0.0);
        let z = mated["position"][2].as_f64().unwrap_or(0.0);
        assert!((z - (p1 + 5.0 + 3.0)).abs() < 1e-6, "{mated}");
        // Resting on part 1 it touches, sunk into it it clashes.
        let issues = run(
            &mut doc,
            &mut cache,
            "assembly.checkPieces",
            json!({ "clearance": 0.0 }),
        );
        assert_eq!(issues["issues"], json!([]), "{issues}");
        let mut sunk = piece.clone();
        sunk["position"] = mated["position"].clone();
        sunk["position"][2] = json!(z - 2.0);
        sunk["rotation"] = mated["rotation"].clone();
        // With part 1 grounded the mate holds it on the face: moved down,
        // it comes back up.
        run(
            &mut doc,
            &mut cache,
            "assembly.pieceState",
            json!({ "action": "ground", "piece": { "sheet": sheet, "part": 1 } }),
        );
        run(&mut doc, &mut cache, "assembly.placePiece", sunk.clone());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let back_z = listed["pieces"][2]["position"][2].as_f64().unwrap_or(0.0);
        assert!((back_z - z).abs() < 1e-6, "{listed}");
        assert_eq!(listed["mates"][0]["state"], json!("solved"));
        let id = mated["mate"].clone();
        run(
            &mut doc,
            &mut cache,
            "assembly.deleteMate",
            json!({ "id": id }),
        );
        run(&mut doc, &mut cache, "assembly.placePiece", sunk);
        let issues = run(
            &mut doc,
            &mut cache,
            "assembly.checkPieces",
            json!({ "clearance": 0.0 }),
        );
        assert_eq!(
            issues["issues"][0]["b"]["component"],
            json!(screen),
            "{issues}"
        );

        // Saved with the document: the component list and its placement.
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert_eq!(saved["components"], json!([screen]));
        assert_eq!(saved["pieces"][0]["component"], json!(screen));
        let back: super::super::Assembly = serde_json::from_value(saved).expect("reads");
        assert_eq!(&back, doc.assembly());
        let rows = run(&mut doc, &mut cache, "assembly.pieceCutList", json!({}))["rows"].clone();
        assert_eq!(rows[2]["componentName"], json!("Screen"));

        // Removed, and back with Undo.
        run(
            &mut doc,
            &mut cache,
            "assembly.removeComponentPiece",
            json!({ "component": screen }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"].as_array().map(Vec::len), Some(2));
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][2]["position"][2].as_f64(), Some(z - 2.0));
        // Undo the placements, the mate, the ground, the mate's deletion
        // and the add: no component piece.
        for _ in 0..7 {
            assert!(doc.undo().is_some());
        }
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_component_piece_leaves_its_sheets_raw_model() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let slab = block_component(&mut doc, "Slab", [5.0, 5.0, 45.0, 45.0], 4.0, true);
        let knob = block_component(&mut doc, "Knob", [60.0, 20.0, 70.0, 30.0], 12.0, true);
        run(
            &mut doc,
            &mut cache,
            "assembly.pieceSource",
            json!({ "sheet": sheet, "model": true }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let model_width = listed["pieces"][0]["size"][0].as_f64().unwrap_or(0.0);
        assert!(model_width > 60.0, "{listed}");
        run(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            json!({ "components": [knob] }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let rows = listed["pieces"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        // The sheet's model is the slab alone now.
        let size = rows[0]["size"].clone();
        assert!(
            (size[0].as_f64().unwrap_or(0.0) - 40.0).abs() < 2.0,
            "{size}"
        );
        assert!(
            (size[2].as_f64().unwrap_or(0.0) - 4.0).abs() < 1e-4,
            "{size}"
        );
        assert_eq!(rows[1]["component"], json!(knob));
        assert!((rows[1]["size"][2].as_f64().unwrap_or(0.0) - 12.0).abs() < 1e-4);
        assert_ne!(slab, knob);
    }

    fn piece(sheet: u64, part: u32) -> Value {
        json!({ "sheet": sheet, "part": part })
    }

    fn position(listed: &Value, row: usize) -> [f64; 3] {
        let p = &listed["pieces"][row]["position"];
        [0, 1, 2].map(|k| p[k].as_f64().unwrap_or(f64::NAN))
    }

    #[test]
    fn hide_show_all_and_ground_are_saved_and_undone_one_step_each() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let state =
            |doc: &mut Document, cache: &mut PartsCache, action: &str, part: Option<u32>| {
                let mut args = json!({ "action": action });
                if let Some(part) = part {
                    args["piece"] = piece(sheet, part);
                }
                run(doc, cache, "assembly.pieceState", args)
            };
        state(&mut doc, &mut cache, "hide", Some(1));
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][0]["hidden"], json!(true));
        assert_eq!(listed["pieces"][1]["hidden"], json!(false));
        // Isolate is the view's own: the kernel does not take it.
        let isolate = json!({ "action": "isolate", "piece": piece(sheet, 1) });
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.pieceState",
            &isolate.to_string()
        )
        .is_err());
        state(&mut doc, &mut cache, "hide", Some(2));
        state(&mut doc, &mut cache, "showAll", None);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][0]["hidden"], json!(false));
        assert_eq!(listed["pieces"][1]["hidden"], json!(false));
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][0]["hidden"], json!(true));
        assert_eq!(listed["pieces"][1]["hidden"], json!(true));
        // Grounded: saved, and it cannot be moved.
        state(&mut doc, &mut cache, "ground", Some(2));
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert!(saved["pieces"]
            .as_array()
            .expect("pieces")
            .iter()
            .any(|p| p["grounded"] == json!(true) && p["part"] == json!(2)));
        let mut place = piece(sheet, 2);
        place["position"] = json!([1.0, 2.0, 3.0]);
        place["rotation"] = json!([0.0, 0.0, 0.0]);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            &place.to_string()
        )
        .is_err());
        state(&mut doc, &mut cache, "unground", Some(2));
        run(&mut doc, &mut cache, "assembly.placePiece", place);
        assert!(doc.undo().is_some());
        assert!(doc.undo().is_some());
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["grounded"], json!(true));
    }

    #[test]
    fn a_mate_is_kept_and_solved_again_when_a_piece_moves() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let size = |row: usize, k: usize| listed["pieces"][row]["size"][k].as_f64().unwrap_or(0.0);
        let (w1, h1, t1) = (size(0, 0), size(0, 1), size(0, 2));
        let (w2, h2) = (size(1, 0), size(1, 1));
        // Part 2's underside on part 1's top; part 1 grounded.
        run(
            &mut doc,
            &mut cache,
            "assembly.pieceState",
            json!({ "action": "ground", "piece": piece(sheet, 1) }),
        );
        let mated = run(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            json!({
                "target": { "sheet": sheet, "part": 1, "point": [w1 / 2.0, h1 / 2.0, t1] },
                "moving": { "sheet": sheet, "part": 2, "point": [w2 / 2.0, h2 / 2.0, 0.0] },
            }),
        );
        assert_eq!(mated["state"], json!("solved"), "{mated}");
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let base = position(&listed, 0);
        let on = position(&listed, 1);
        assert!((on[2] - (base[2] + t1)).abs() < 1e-6, "{listed}");
        assert_eq!(listed["mates"][0]["kind"], json!("against"));
        // Lifted and slid: it slides, but drops back onto the face; asked
        // first while dragging, the solve says so and keeps nothing.
        let mut place = piece(sheet, 2);
        place["position"] = json!([on[0] + 15.0, on[1], on[2] + 40.0]);
        place["rotation"] = listed["pieces"][1]["rotation"].clone();
        let revision = doc.revision();
        let preview = run(&mut doc, &mut cache, "assembly.solvePlace", place.clone());
        assert_eq!(doc.revision(), revision);
        let moved = preview["places"]
            .as_array()
            .and_then(|places| places.iter().find(|p| p["piece"]["part"] == json!(2)))
            .map(|p| p["position"].clone())
            .expect("the moved piece");
        assert!(
            (moved[2].as_f64().unwrap_or(0.0) - on[2]).abs() < 1e-6,
            "{preview}"
        );
        assert!((moved[0].as_f64().unwrap_or(0.0) - (on[0] + 15.0)).abs() < 1e-6);
        assert_eq!(preview["conflicts"], json!(0));
        run(&mut doc, &mut cache, "assembly.placePiece", place);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let now = position(&listed, 1);
        assert!(
            (now[0] - (on[0] + 15.0)).abs() < 1e-6 && (now[2] - on[2]).abs() < 1e-6,
            "{now:?}"
        );
        // A 5 mm gap moves it up; suppressed, the mate lets it be moved off.
        let id = mated["mate"].clone();
        run(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            json!({ "id": id, "value": 5.0 }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert!((position(&listed, 1)[2] - (on[2] + 5.0)).abs() < 1e-6);
        run(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            json!({ "id": id, "suppressed": true }),
        );
        let mut place = piece(sheet, 2);
        place["position"] = json!([0.0, 0.0, 100.0]);
        place["rotation"] = json!([0.0, 0.0, 0.0]);
        run(&mut doc, &mut cache, "assembly.placePiece", place);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["pieces"][1]["position"], json!([0.0, 0.0, 100.0]));
        assert_eq!(listed["mates"][0]["state"], json!("suppressed"));
        // Switched back on, it pulls the piece back to the face.
        run(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            json!({ "id": id, "suppressed": false }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["mates"][0]["state"], json!("solved"), "{listed}");
        assert!((position(&listed, 1)[2] - (on[2] + 5.0)).abs() < 1e-6);
        // Saved with the document and undone a step at a time.
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert_eq!(saved["pieceMates"][0]["kind"], json!("against"));
        let back: super::super::Assembly = serde_json::from_value(saved).expect("reads");
        assert_eq!(&back, doc.assembly());
        run(
            &mut doc,
            &mut cache,
            "assembly.deleteMate",
            json!({ "id": id }),
        );
        assert!(doc.assembly().piece_mates.is_empty());
        assert!(doc.undo().is_some());
        assert_eq!(doc.assembly().piece_mates.len(), 1);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            &json!({ "id": 999 }).to_string()
        )
        .is_err());
    }

    #[test]
    fn flush_and_angle_mates_move_the_piece_the_least() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let size = |row: usize, k: usize| listed["pieces"][row]["size"][k].as_f64().unwrap_or(0.0);
        let (w1, h1, t1) = (size(0, 0), size(0, 1), size(0, 2));
        let (w2, h2, t2) = (size(1, 0), size(1, 1), size(1, 2));
        let base = position(&listed, 0);
        // Part 2 lifted 30 mm; its top made flush with part 1's top.
        let mut place = piece(sheet, 2);
        let start = position(&listed, 1);
        place["position"] = json!([start[0], start[1], start[2] + 30.0]);
        place["rotation"] = json!([0.0, 0.0, 0.0]);
        run(&mut doc, &mut cache, "assembly.placePiece", place);
        let flush = run(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            json!({
                "kind": "flush",
                "target": { "sheet": sheet, "part": 1, "point": [w1 / 2.0, h1 / 2.0, t1] },
                "moving": { "sheet": sheet, "part": 2, "point": [w2 / 2.0, h2 / 2.0, t2] },
            }),
        );
        assert_eq!(flush["state"], json!("solved"), "{flush}");
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let now = position(&listed, 1);
        assert!(
            (now[2] - base[2]).abs() < 1e-6 && (now[0] - start[0]).abs() < 1e-6,
            "{listed}"
        );
        assert_eq!(position(&listed, 0), base);
        // Turned to stand at 90 degrees to part 1's top instead.
        let id = flush["mate"].clone();
        run(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            json!({ "id": id, "kind": "angle", "value": 90.0 }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["mates"][0]["state"], json!("solved"), "{listed}");
        let r = &listed["pieces"][1]["rotation"];
        let turned = [0, 1]
            .iter()
            .any(|k| (r[*k].as_f64().unwrap_or(0.0).abs() - 90.0).abs() < 1e-6);
        assert!(turned, "{r}");
        // A concentric mate needs round faces.
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.editMate",
            &json!({ "id": id, "kind": "concentric" }).to_string()
        )
        .is_err());
    }

    #[test]
    fn explode_steps_are_kept_listed_and_undone() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.setExplodeSteps",
            json!({
                "label": "Add Explode Step",
                "steps": [{ "name": "Shelf up", "pieces": [piece(sheet, 2)], "direction": [0.0, 0.0, 3.0], "distance": 60.0 }]
            }),
        );
        let id = reply["steps"][0]["id"].as_u64().expect("an id");
        assert!(id > 0);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["explodeSteps"][0]["name"], json!("Shelf up"));
        assert_eq!(
            listed["explodeSteps"][0]["direction"],
            json!([0.0, 0.0, 1.0])
        );
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert_eq!(saved["explodeSteps"][0]["id"], json!(id));
        assert_eq!(doc.undo().as_deref(), Some("Add Explode Step"));
        assert!(doc.assembly().explode_steps.is_empty());
    }

    /// A sheet with three parts held by tabs, a named vector round each.
    fn three_parts() -> Document {
        let mut doc = document();
        let sheet = doc.active_sheet();
        let layer = doc.active_layer();
        for (name, x0, x1) in [
            ("Left", 5.0, 30.0),
            ("Middle", 37.0, 62.0),
            ("Right", 70.0, 95.0),
        ] {
            add(
                &mut doc,
                cut_out(sheet, [x0, 10.0, x1, 40.0], 11.0, Some(7.0)),
            );
            let path = rect(Point::new(x0, 10.0), Point::new(x1, 40.0)).expect("rect");
            doc.begin("vector");
            doc.insert_entity(Entity {
                rotation: 0.0,
                id: 0,
                layer,
                sheet,
                revision: 0,
                name: Some(name.into()),
                parent: None,
                kind: EntityKind::Path { path },
            });
            doc.commit();
        }
        doc
    }

    /// The listed piece cut from the vector called `name`.
    fn named<'a>(listed: &'a Value, name: &str) -> &'a Value {
        listed["pieces"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["vectorName"] == json!(name)))
            .unwrap_or_else(|| panic!("no piece {name} in {listed}"))
    }

    /// Checks the mate joins the Right piece (a) and the Middle piece (b),
    /// is met, and holds the Middle piece on the Right piece's top however
    /// it is moved; answers the listing.
    fn mate_holds_right_and_middle(doc: &mut Document, cache: &mut PartsCache) -> Value {
        let listed = run(doc, cache, "assembly.pieces", json!({}));
        let key = |row: &Value| json!({ "sheet": row["sheet"], "part": row["part"] });
        let (right, middle) = (named(&listed, "Right"), named(&listed, "Middle"));
        let mate = &listed["mates"][0];
        assert_eq!(mate["a"], key(right), "{listed}");
        assert_eq!(mate["b"], key(middle), "{listed}");
        assert_eq!(mate["state"], json!("solved"), "{listed}");
        // Lifted, the Middle piece drops back onto the Right piece's top.
        let top = right["position"][2].as_f64().unwrap_or(0.0)
            + right["size"][2].as_f64().unwrap_or(0.0) / 2.0;
        let mut place = key(middle);
        let at = middle["position"].clone();
        let lifted = at[2].as_f64().unwrap_or(0.0) + 40.0;
        place["position"] = json!([at[0], at[1], lifted]);
        place["rotation"] = middle["rotation"].clone();
        run(doc, cache, "assembly.placePiece", place);
        let listed = run(doc, cache, "assembly.pieces", json!({}));
        let middle = named(&listed, "Middle");
        let underside = middle["position"][2].as_f64().unwrap_or(0.0)
            - middle["size"][2].as_f64().unwrap_or(0.0) / 2.0;
        assert!((underside - top).abs() < 1e-6, "{listed}");
        assert_eq!(listed["mates"][0]["state"], json!("solved"), "{listed}");
        listed
    }

    #[test]
    fn a_mate_stays_on_its_parts_when_toolpath_edits_renumber_them() {
        let mut doc = three_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(named(&listed, "Middle")["part"], json!(2));
        assert_eq!(named(&listed, "Right")["part"], json!(3));
        let size = |name: &str, k: usize| named(&listed, name)["size"][k].as_f64().unwrap_or(0.0);
        // The Middle piece's underside on the grounded Right piece's top.
        run(
            &mut doc,
            &mut cache,
            "assembly.pieceState",
            json!({ "action": "ground", "piece": piece(sheet, 3) }),
        );
        let mated = run(
            &mut doc,
            &mut cache,
            "assembly.matePieces",
            json!({
                "target": { "sheet": sheet, "part": 3, "point": [size("Right", 0) / 2.0, size("Right", 1) / 2.0, size("Right", 2)] },
                "moving": { "sheet": sheet, "part": 2, "point": [size("Middle", 0) / 2.0, size("Middle", 1) / 2.0, 0.0] },
            }),
        );
        assert_eq!(mated["state"], json!("solved"), "{mated}");
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert!(
            saved["pieceMates"][0]["aAnchor"]["area"]
                .as_f64()
                .unwrap_or(0.0)
                > 300.0
        );
        mate_holds_right_and_middle(&mut doc, &mut cache);

        // Deleting the Left part's toolpath numbers the Middle part 1 and
        // the Right part 2.
        let left = doc.toolpath_order()[0];
        doc.begin("delete");
        doc.delete_toolpaths(&[left]).expect("delete");
        doc.commit();
        let listed = mate_holds_right_and_middle(&mut doc, &mut cache);
        assert_eq!(named(&listed, "Middle")["part"], json!(1));
        assert_eq!(named(&listed, "Right")["part"], json!(2));
        // Saved on the parts' numbers now.
        assert_eq!(doc.assembly().piece_mates[0].a.part, 2);
        assert_eq!(doc.assembly().piece_mates[0].b.part, 1);

        // Cutting the Left part again numbers them 2 and 3 again.
        add(
            &mut doc,
            cut_out(sheet, [5.0, 10.0, 30.0, 40.0], 11.0, Some(7.0)),
        );
        let listed = mate_holds_right_and_middle(&mut doc, &mut cache);
        assert_eq!(named(&listed, "Middle")["part"], json!(2));

        // Moving the Right part's cut 1 mm keeps the mate on it.
        let right = doc.toolpath_order()[1];
        doc.begin("edit");
        doc.replace_toolpath(
            right,
            cut_out(sheet, [71.0, 10.0, 96.0, 40.0], 11.0, Some(7.0)),
        )
        .expect("replace");
        doc.commit();
        mate_holds_right_and_middle(&mut doc, &mut cache);

        // With the Right part's toolpath gone, the mate is missing, not on
        // another piece.
        doc.begin("delete");
        doc.delete_toolpaths(&[right]).expect("delete");
        doc.commit();
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert_eq!(listed["mates"][0]["state"], json!("missing"), "{listed}");
    }

    #[test]
    fn an_explode_step_stays_on_its_parts_when_toolpath_edits_renumber_them() {
        let mut doc = three_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        run(
            &mut doc,
            &mut cache,
            "assembly.setExplodeSteps",
            json!({
                "label": "Add Explode Step",
                "steps": [{ "name": "Lift", "pieces": [piece(sheet, 2), piece(sheet, 3)], "direction": [0.0, 0.0, 1.0], "distance": 50.0 }]
            }),
        );
        let saved = serde_json::to_value(doc.assembly()).expect("json");
        assert!(
            saved["explodeSteps"][0]["anchors"][0]["area"]
                .as_f64()
                .unwrap_or(0.0)
                > 300.0
        );
        // The listed step moves the Middle and Right pieces, whatever their
        // numbers are now.
        let moves = |doc: &mut Document, cache: &mut PartsCache| {
            let listed = run(doc, cache, "assembly.pieces", json!({}));
            let key = |name: &str| {
                listed["pieces"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["vectorName"] == json!(name)))
                    .map(|row| json!({ "sheet": row["sheet"], "part": row["part"] }))
            };
            let (middle, right) = (key("Middle"), key("Right"));
            let step = listed["explodeSteps"][0].clone();
            let pieces: Vec<Value> = step["pieces"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .map(|p| json!({ "sheet": p["sheet"], "part": p["part"] }))
                        .collect()
                })
                .unwrap_or_default();
            let want: Vec<Value> = [middle, right].into_iter().flatten().collect();
            (listed, step, pieces, want)
        };
        let (listed, _, pieces, want) = moves(&mut doc, &mut cache);
        assert_eq!(pieces, want, "{listed}");

        // Deleting the Left part's toolpath numbers the Middle part 1 and
        // the Right part 2: the step follows them.
        let left = doc.toolpath_order()[0];
        doc.begin("delete");
        doc.delete_toolpaths(&[left]).expect("delete");
        doc.commit();
        let (listed, step, pieces, want) = moves(&mut doc, &mut cache);
        assert_eq!(named(&listed, "Middle")["part"], json!(1));
        assert_eq!(pieces, want, "{listed}");
        // Kept again as listed, it is saved on the numbers now.
        run(
            &mut doc,
            &mut cache,
            "assembly.setExplodeSteps",
            json!({ "label": "Edit Explode Step", "steps": [step] }),
        );
        let kept = &doc.assembly().explode_steps[0];
        assert_eq!(
            kept.pieces.iter().map(|p| p.part).collect::<Vec<_>>(),
            vec![1, 2]
        );

        // Cutting the Left part again numbers them 2 and 3 again.
        add(
            &mut doc,
            cut_out(sheet, [5.0, 10.0, 30.0, 40.0], 11.0, Some(7.0)),
        );
        let (listed, _, pieces, want) = moves(&mut doc, &mut cache);
        assert_eq!(pieces, want, "{listed}");

        // With the Right part's toolpath gone the step moves only the
        // Middle piece, never the piece that took the Right part's number;
        // kept meanwhile, it still has the Right part when it comes back.
        let right_cut = doc.toolpath_order()[1];
        doc.begin("delete");
        doc.delete_toolpaths(&[right_cut]).expect("delete");
        doc.commit();
        let (listed, step, pieces, want) = moves(&mut doc, &mut cache);
        assert_eq!(want.len(), 1, "only the Middle piece is left");
        assert_eq!(pieces, want, "{listed}");
        run(
            &mut doc,
            &mut cache,
            "assembly.setExplodeSteps",
            json!({ "label": "Edit Explode Step", "steps": [step] }),
        );
        assert_eq!(doc.assembly().explode_steps[0].pieces.len(), 2);
        add(
            &mut doc,
            cut_out(sheet, [70.0, 10.0, 95.0, 40.0], 11.0, Some(7.0)),
        );
        let (listed, _, pieces, want) = moves(&mut doc, &mut cache);
        assert_eq!(want.len(), 2, "{listed}");
        assert_eq!(pieces, want, "{listed}");
    }

    fn triple(value: &Value) -> Vec<f64> {
        value
            .as_array()
            .expect("triple")
            .iter()
            .filter_map(Value::as_f64)
            .collect()
    }

    #[test]
    fn a_component_lands_at_a_point_or_beside_the_pieces_and_shows_before_it_is_added() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let screen = block_component(&mut doc, "Screen", [10.0, 10.0, 40.0, 30.0], 6.0, false);
        let knob = block_component(&mut doc, "Knob", [60.0, 10.0, 70.0, 20.0], 4.0, false);

        // Its relief and thumbnail come before it is a piece.
        let relief = run(
            &mut doc,
            &mut cache,
            "assembly.componentRelief",
            json!({ "component": screen }),
        );
        assert!(relief["bytes"].as_u64().unwrap_or(0) > 200);
        let thumb = run(
            &mut doc,
            &mut cache,
            "assembly.componentThumbnail",
            json!({ "component": screen, "size": 64 }),
        );
        assert!(thumb["bytes"].as_u64().unwrap_or(0) > 60);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.componentThumbnail",
            &json!({ "component": screen, "size": 4 }).to_string()
        )
        .is_err());
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            &json!({ "components": [screen], "at": [1.0, "x", 0.0] }).to_string()
        )
        .is_err());

        // Dropped on a point: the middle of its underside sits there.
        let before = doc.revision();
        run(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            json!({ "components": [screen], "at": [300.0, 40.0, 11.0] }),
        );
        assert_ne!(doc.revision(), before);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let rows = listed["pieces"].as_array().expect("rows");
        let row = rows
            .iter()
            .find(|r| r["component"] == json!(screen))
            .expect("screen piece");
        let size = triple(&row["size"]);
        let at = triple(&row["position"]);
        assert!(
            (at[0] - 300.0).abs() < 1e-9 && (at[1] - 40.0).abs() < 1e-9,
            "{at:?}"
        );
        assert!((at[2] - (11.0 + size[2] / 2.0)).abs() < 1e-9, "{at:?}");

        // Added beside: past every piece along X, on the lowest underside.
        let far = rows
            .iter()
            .map(|r| {
                let p = triple(&r["position"]);
                let s = triple(&r["size"]);
                p[0] + s[0] / 2.0
            })
            .fold(f64::NEG_INFINITY, f64::max);
        let low = rows
            .iter()
            .map(|r| triple(&r["position"])[2] - triple(&r["size"])[2] / 2.0)
            .fold(f64::INFINITY, f64::min);
        run(
            &mut doc,
            &mut cache,
            "assembly.addComponentPieces",
            json!({ "components": [knob], "beside": true }),
        );
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let row = listed["pieces"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|r| r["component"] == json!(knob))
            .expect("knob piece")
            .clone();
        let size = triple(&row["size"]);
        let at = triple(&row["position"]);
        let left = at[0] - size[0] / 2.0;
        assert!(
            (left - (far + pieces::LAYOUT_GAP)).abs() < 1e-6,
            "{at:?} {far}"
        );
        assert!((at[2] - size[2] / 2.0 - low).abs() < 1e-6, "{at:?} {low}");

        // One undo step takes it out again.
        doc.undo().expect("undo");
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        assert!(!listed["pieces"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|r| r["component"] == json!(knob)));
    }

    /// The piece row of `component`, or of `sheet` part `part`.
    fn row_of(listed: &Value, component: Option<u64>, part: u32) -> Value {
        listed["pieces"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|r| match component {
                Some(id) => r["component"] == json!(id),
                None => r["component"].is_null() && r["part"] == json!(part),
            })
            .expect("row")
            .clone()
    }

    /// The box of a listed piece row that is not turned, min then max.
    fn row_box(row: &Value) -> ([f64; 3], [f64; 3]) {
        let (p, s) = (triple(&row["position"]), triple(&row["size"]));
        let lo = [0, 1, 2].map(|k| p[k] - s[k] / 2.0);
        let hi = [0, 1, 2].map(|k| p[k] + s[k] / 2.0);
        (lo, hi)
    }

    /// Nothing interferes: Verify Fit finds no joint running into another
    /// and the grid check no overlap.
    fn assert_clear(doc: &mut Document, cache: &mut PartsCache) {
        let fit = run(
            doc,
            cache,
            "assembly.verifyFit",
            json!({ "target": 0.2, "tolerance": 0.1 }),
        );
        for joint in fit["joints"].as_array().expect("joints") {
            assert_ne!(joint["status"], "interference", "{joint}");
            assert!(
                joint["overlapVolume"].as_f64().unwrap_or(0.0) <= 1e-9,
                "{joint}"
            );
        }
        let checked = run(doc, cache, "assembly.checkPieces", json!({}));
        assert_eq!(checked["issues"], json!([]), "{checked}");
    }

    /// Drops `component` at `point` on `onto` (or the ground) and adds it
    /// or moves it where the kernel says; answers the new list.
    fn drop_at(
        doc: &mut Document,
        cache: &mut PartsCache,
        component: u64,
        point: [f64; 3],
        onto: Option<Value>,
    ) -> Value {
        let place = run(
            doc,
            cache,
            "assembly.dropPlace",
            json!({ "component": component, "point": point, "onto": onto }),
        );
        let listed = run(doc, cache, "assembly.pieces", json!({}));
        let is_piece = listed["pieces"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|r| r["component"] == json!(component));
        if is_piece {
            run(
                doc,
                cache,
                "assembly.placePiece",
                json!({
                    "sheet": row_of(&listed, Some(component), 1)["sheet"],
                    "part": 1,
                    "component": component,
                    "position": place["position"],
                    "rotation": place["rotation"],
                }),
            );
        } else {
            run(
                doc,
                cache,
                "assembly.addComponentPieces",
                json!({ "components": [component], "pose": place }),
            );
        }
        run(doc, cache, "assembly.pieces", json!({}))
    }

    #[test]
    fn a_dropped_component_sits_flush_on_the_face_under_the_pointer_and_never_overlaps() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let sheet = doc.active_sheet();
        let screen = block_component(&mut doc, "Screen", [10.0, 10.0, 40.0, 30.0], 6.0, false);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let first = row_of(&listed, None, 1);
        let (lo, hi) = row_box(&first);
        let (lo2, hi2) = row_box(&row_of(&listed, None, 2));
        let onto = json!({ "sheet": sheet, "part": 1 });
        let mid = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);

        // On the front (-Y) side: the screen's back side against it, the
        // middle of that side on the point, its turns kept.
        let listed = drop_at(
            &mut doc,
            &mut cache,
            screen,
            [mid[0], lo[1], mid[2]],
            Some(onto.clone()),
        );
        let row = row_of(&listed, Some(screen), 1);
        let (slo, shi) = row_box(&row);
        let size = triple(&row["size"]);
        assert_eq!(triple(&row["rotation"]), vec![0.0, 0.0, 0.0]);
        assert!((shi[1] - lo[1]).abs() < 0.01, "{slo:?} {shi:?} {lo:?}");
        assert!(((slo[0] + shi[0]) / 2.0 - mid[0]).abs() < 1e-6);
        assert!(((slo[2] + shi[2]) / 2.0 - mid[2]).abs() < 1e-6);
        assert!((shi[2] - slo[2] - size[2]).abs() < 1e-6);
        assert_clear(&mut doc, &mut cache);

        // On the right (+X) side the screen would reach into the second
        // part: it moves the least way that clears it, still flush.
        let listed = drop_at(
            &mut doc,
            &mut cache,
            screen,
            [hi[0], mid[1], mid[2]],
            Some(onto.clone()),
        );
        let row = row_of(&listed, Some(screen), 1);
        let (slo, shi) = row_box(&row);
        assert!((slo[0] - hi[0]).abs() < 0.01, "{slo:?} {hi:?}");
        let clear_of_second = slo[2] >= hi2[2] - 0.02
            || shi[1] <= lo2[1] + 0.02
            || slo[1] >= hi2[1] - 0.02
            || slo[0] >= hi2[0] - 0.02;
        assert!(clear_of_second, "{slo:?} {shi:?} {lo2:?} {hi2:?}");
        assert_clear(&mut doc, &mut cache);

        // On top: it stands on the part, its underside on the top.
        let listed = drop_at(
            &mut doc,
            &mut cache,
            screen,
            [mid[0], mid[1], hi[2]],
            Some(onto.clone()),
        );
        let (slo, _) = row_box(&row_of(&listed, Some(screen), 1));
        assert!((slo[2] - hi[2]).abs() < 0.01, "{slo:?} {hi:?}");
        assert_clear(&mut doc, &mut cache);

        // Turned on its side, dropped on the top again: the side facing
        // down the most turns to face the top and lies on it.
        let row = row_of(&listed, Some(screen), 1);
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({
                "sheet": row["sheet"], "part": 1, "component": screen,
                "position": [300.0, 0.0, 50.0], "rotation": [80.0, 0.0, 0.0],
            }),
        );
        let place = run(
            &mut doc,
            &mut cache,
            "assembly.dropPlace",
            json!({ "component": screen, "point": [mid[0], mid[1], hi[2]], "onto": onto }),
        );
        let angles: Vec3 = triple(&place["rotation"]).try_into().expect("3");
        let turned = super::super::pose::Pose::euler(angles).r;
        // Its local Y (the long side's normal, facing down after 80 degrees
        // about X) now points straight down or up.
        assert!(turned[2][1].abs() > 1.0 - 1e-9, "{place}");
        let listed = drop_at(
            &mut doc,
            &mut cache,
            screen,
            [mid[0], mid[1], hi[2]],
            Some(json!({ "sheet": sheet, "part": 1 })),
        );
        let row = row_of(&listed, Some(screen), 1);
        let bounds = pieces::piece_bounds(
            &Piece {
                sheet,
                part: 1,
                position: triple(&row["position"]).try_into().expect("3"),
                rotation: triple(&row["rotation"]).try_into().expect("3"),
                anchor: None,
                component: Some(screen),
                hidden: false,
                grounded: false,
            },
            triple(&row["size"]).try_into().expect("3"),
        );
        assert!((bounds[0][2] - hi[2]).abs() < 0.01, "{bounds:?} {hi:?}");
        assert_clear(&mut doc, &mut cache);

        // On the ground: the underside on it, clear of the parts.
        let knob = block_component(&mut doc, "Knob", [60.0, 10.0, 70.0, 20.0], 4.0, false);
        let listed = drop_at(&mut doc, &mut cache, knob, [mid[0], mid[1], 0.0], None);
        let (klo, _) = row_box(&row_of(&listed, Some(knob), 1));
        assert!(klo[2] >= -1e-6, "{klo:?}");
        assert_clear(&mut doc, &mut cache);
        assert!(dispatch(
            &mut doc,
            &mut cache,
            "assembly.dropPlace",
            &json!({ "component": knob, "point": [0.0, "x", 0.0] }).to_string()
        )
        .is_err());
    }

    #[test]
    fn a_component_dropped_where_it_lies_on_its_model_piece_does_not_meet_itself() {
        // No toolpaths: the sheet's raw model is its piece and holds both
        // components until one becomes a piece of its own.
        let mut doc = document();
        let mut cache = PartsCache::default();
        block_component(&mut doc, "Block", [10.0, 10.0, 40.0, 30.0], 6.0, true);
        let knob = block_component(&mut doc, "Knob", [60.0, 10.0, 70.0, 20.0], 4.0, true);
        let listed = run(&mut doc, &mut cache, "assembly.pieces", json!({}));
        let model = listed["pieces"][0].clone();
        assert_eq!(model["model"], json!(true), "{listed}");
        let (lo, hi) = row_box(&model);
        // The knob's own place on the model piece (its box starts at the
        // block's corner, job X 10), on the ground.
        let x = lo[0] + 55.0;
        assert!(x < hi[0]);
        let place = run(
            &mut doc,
            &mut cache,
            "assembly.dropPlace",
            json!({ "component": knob, "point": [x, 15.0, lo[2]] }),
        );
        let at = triple(&place["position"]);
        let size = component_size(&doc, &mut cache, knob).expect("size");
        assert!((at[2] - (lo[2] + size[2] / 2.0)).abs() < 1e-6, "{place}");
        assert!((at[0] - x).abs() < 1e-6, "{place}");
        let listed = drop_at(&mut doc, &mut cache, knob, [x, 15.0, lo[2]], None);
        assert_eq!(
            listed["pieces"].as_array().map(Vec::len),
            Some(2),
            "{listed}"
        );
        assert_clear(&mut doc, &mut cache);
        // The block, still in the model piece, is in the way of the knob
        // dropped again under it: the knob moves clear.
        let listed = drop_at(
            &mut doc,
            &mut cache,
            knob,
            [lo[0] + 5.0, lo[1] + 5.0, lo[2]],
            None,
        );
        let (klo, _) = row_box(&row_of(&listed, Some(knob), 1));
        assert!(klo[2] >= lo[2] - 1e-6, "{klo:?}");
        assert_clear(&mut doc, &mut cache);
    }

    #[test]
    fn an_addable_components_look_changes_with_its_shape_only() {
        let mut doc = two_parts();
        let mut cache = PartsCache::default();
        let knob = block_component(&mut doc, "Knob", [60.0, 10.0, 70.0, 20.0], 4.0, false);
        let look = |doc: &mut Document, cache: &mut PartsCache| {
            let listed = run(doc, cache, "assembly.pieces", json!({}));
            listed["components"][0]["look"]
                .as_str()
                .expect("look")
                .to_owned()
        };
        let before = look(&mut doc, &mut cache);
        doc.begin("rename");
        doc.update_component(knob, |c| c.name = "Dial".into())
            .expect("rename");
        doc.commit();
        assert_eq!(look(&mut doc, &mut cache), before);
        doc.begin("taller");
        doc.update_component(knob, |c| c.height_scale = 2.0)
            .expect("scale");
        doc.commit();
        assert_ne!(look(&mut doc, &mut cache), before);
    }

    #[test]
    fn beside_stands_right_of_every_box_on_the_lowest_floor() {
        let boxes = [
            [[0.0, 0.0, 0.0], [100.0, 50.0, 10.0]],
            [[-20.0, 60.0, -5.0], [40.0, 90.0, 5.0]],
        ];
        let at = pieces::beside(&boxes, [10.0, 20.0, 4.0]);
        assert_eq!(at, [100.0 + pieces::LAYOUT_GAP + 5.0, 45.0, -3.0]);
        assert_eq!(pieces::beside(&[], [10.0, 20.0, 4.0]), [5.0, 10.0, 2.0]);
        assert_eq!(
            pieces::resting_at([10.0, 20.0, 4.0], [1.0, 2.0, 3.0]),
            [1.0, 2.0, 5.0]
        );
    }
}
