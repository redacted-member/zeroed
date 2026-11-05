//! Nest Parts over the document (`ops.nest`): gathers the selected vectors
//! and components into parts, nests them with [`crate::geom::nest`] and
//! moves every part, copies included, onto its sheet.
//!
//! Objects that overlap are one part: a closed vector and the text, open
//! vectors or components lying on it move together, and the part's
//! footprint is the union of their outlines. A component's outline is the
//! boundary of its cells. A part holding a component is never mirrored,
//! since a relief cannot be turned over in place.
//!
//! With `twoSided` on a double sided job, the objects of the other side that
//! lie over a part (seen through the material) travel with it: their mapped
//! outlines join the part's footprint, and they get the same move mirrored
//! through the flip, so both faces of every part stay matched. Copies of a
//! part copy its other side objects too, and they land on the other side's
//! sheet in the same place in the sheet list.
//!
//! With `removeOriginals` off, the originals stay where they were and every
//! placed part is a copy.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::ops;
use super::Document;
use crate::error::CoreError;
use crate::geom::{
    self, path_intersections, point_in_path, BBox, FillRule, Mat3, Path, Point, Tolerance, Vec2,
};
use crate::kernel::{edit, parse_args, KernelReply};
use crate::model::transform::TransformParams;

/// The Nest Parts marks an object keeps (Individual Part Properties): how
/// many copies of it to nest and whether it is a filler part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestMark {
    /// Copies to nest, 1 or more.
    pub copies: usize,
    /// A filler part, copied into the space the other parts leave.
    #[serde(default)]
    pub filler: bool,
}

/// Most copies Individual Part Properties takes.
const MOST_COPIES: usize = 1000;

/// Gives the copies a command just made (original id to copy id) their
/// originals' Nest Parts marks, as part of that command's undo step, so a
/// copied, pasted or duplicated part nests like the part it was copied
/// from. The copies Nest Parts places are left unmarked: they are the
/// copies the mark asked for.
pub(crate) fn carry_marks(document: &mut Document, command: &str, copies: &BTreeMap<u64, u64>) {
    if command == "ops.nest" {
        return;
    }
    let marks = document.nest_marks();
    let mut next = marks.clone();
    for (original, copy) in copies {
        if let Some(mark) = marks.get(original) {
            if !marks.contains_key(copy) && document.entity(*copy).is_ok() {
                next.insert(*copy, *mark);
            }
        }
    }
    if next.len() == marks.len() || !document.reopen_newest_step() {
        return;
    }
    document.set_nest_marks(next);
    document.close_reopened();
}

/// Arguments of `ops.nestMarks`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkArgs {
    /// Objects to mark; the selection when left out.
    ids: Option<Vec<u64>>,
    copies: usize,
    #[serde(default)]
    filler: bool,
}

/// Runs `ops.nestMarks`: Apply in Individual Part Properties. The objects
/// keep the number of copies and the filler mark (saved with the job, one
/// undo step); 1 copy and no filler takes the mark away.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] with nothing selected or copies outside 1 to
/// 1000.
pub fn run_marks(
    document: &mut Document,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let args: MarkArgs = parse_args(name, args)?;
    let ids = document.target_ids(args.ids.as_deref());
    if ids.is_empty() {
        return Err(CoreError::validation("select the parts to mark"));
    }
    if args.copies == 0 || args.copies > MOST_COPIES {
        return Err(CoreError::validation(format!(
            "the number of copies must be from 1 to {MOST_COPIES}"
        )));
    }
    for id in &ids {
        document.entity(*id)?;
    }
    edit(document, "Nest Part Properties", |doc| {
        let mut marks: BTreeMap<u64, NestMark> = doc
            .nest_marks()
            .iter()
            .filter(|(id, _)| doc.entity(**id).is_ok())
            .map(|(id, mark)| (*id, *mark))
            .collect();
        for id in &ids {
            if args.copies == 1 && !args.filler {
                marks.remove(id);
            } else {
                marks.insert(
                    *id,
                    NestMark {
                        copies: args.copies,
                        filler: args.filler,
                    },
                );
            }
        }
        doc.set_nest_marks(marks);
        Ok(json!({ "ids": ids }))
    })
}

/// Arguments of `ops.nest`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestArgs {
    ids: Option<Vec<u64>>,
    /// Sheet size 0 (or left out) means the job's size.
    #[serde(default = "nest_params_on_job")]
    params: geom::nest::NestParams,
    /// Copies per part, keyed by the id of any of its vectors; parts left
    /// out get the copies their objects are marked with, else one.
    #[serde(default)]
    quantities: HashMap<String, usize>,
    /// Vector ids whose part is nested as a filler, besides the parts whose
    /// objects are marked as fillers.
    #[serde(default)]
    fillers: Vec<u64>,
    /// Components nested with the vectors.
    #[serde(default)]
    components: Vec<u64>,
    /// Move the originals (true, the default) rather than leave them and
    /// place copies.
    #[serde(default = "yes")]
    remove_originals: bool,
    /// Carry the other side's objects with each part on a double sided job.
    #[serde(default)]
    two_sided: bool,
    /// The sheet the nest starts on; the active sheet when left out.
    #[serde(default)]
    sheet: Option<u64>,
    /// Go on to the sheets after it, adding new ones, when the parts do not
    /// all fit (true, the default); false keeps every part on the chosen
    /// sheet and reports the ones left over.
    #[serde(default = "yes")]
    add_sheets: bool,
    /// Nest boundary layer: the first closed vector on this layer (on the
    /// sheet nested onto) bounds the nest instead of the material; it is
    /// never nested itself.
    #[serde(default)]
    boundary_layer: Option<u64>,
    /// Custom Sheet Selection: the sheets to nest onto, in order, each
    /// with how many of it to use (the sheet itself, then new copies of it
    /// with its material); a quantity left out is No Limit. When given it
    /// takes the place of `sheet` and `add_sheets`.
    #[serde(default)]
    sheet_plan: Option<Vec<SheetUse>>,
}

/// One row of the Custom Sheet Selection.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SheetUse {
    sheet: u64,
    /// How many of this sheet; `None` is No Limit.
    #[serde(default)]
    quantity: Option<usize>,
}

fn yes() -> bool {
    true
}

/// The sheets a nest fills, in order: a plan's rows (each sheet, then new
/// copies of it), or from the first sheet on through the sheets after it and
/// then new ones.
enum SheetTargets {
    Plan(Vec<SheetUse>),
    From(u64),
}

impl SheetTargets {
    /// Most sheets the targets can hold, or `None` for no limit.
    fn capacity(&self) -> Option<usize> {
        match self {
            SheetTargets::Plan(plan) => {
                let mut total = 0usize;
                for row in plan {
                    total = total.saturating_add(row.quantity?);
                }
                Some(total)
            }
            SheetTargets::From(_) => None,
        }
    }

    fn first(&self) -> u64 {
        match self {
            SheetTargets::Plan(plan) => plan[0].sheet,
            SheetTargets::From(sheet) => *sheet,
        }
    }

    /// The sheet the `index`th nested sheet goes on: `Ok(id)` for a sheet
    /// already in the list, `Err(template)` for a new copy of `template`,
    /// `Err(None)` for a new plain sheet.
    fn slot(&self, index: usize, doc: &Document) -> Result<u64, Option<u64>> {
        match self {
            SheetTargets::Plan(plan) => {
                let mut left = index;
                for row in plan {
                    let count = row.quantity.unwrap_or(usize::MAX);
                    if left < count {
                        return if left == 0 {
                            Ok(row.sheet)
                        } else {
                            Err(Some(row.sheet))
                        };
                    }
                    left -= count;
                }
                Err(plan.last().map(|row| row.sheet))
            }
            SheetTargets::From(first) => {
                let list: Vec<u64> = doc.sheets().iter().map(|s| s.id).collect();
                let start = list.iter().position(|s| s == first).unwrap_or(0);
                list.get(start + index).copied().ok_or(None)
            }
        }
    }
}

/// The document sheet of each nested sheet index, adding sheets (copies of a
/// plan row's sheet, with its material) as the nest needs them.
fn resolve_sheets(
    doc: &mut Document,
    targets: &SheetTargets,
    wanted: &[bool],
) -> Result<Vec<u64>, CoreError> {
    let mut used = Vec::with_capacity(wanted.len());
    for (index, want) in wanted.iter().enumerate() {
        if !want {
            // A sheet nothing went on: no new sheet for it.
            used.push(targets.slot(index, doc).unwrap_or(0));
            continue;
        }
        let id = match targets.slot(index, doc) {
            Ok(id) => id,
            Err(template) => {
                let id = doc.add_sheet(None);
                if let Some(template) = template {
                    let material = doc.sheet_material(template)?;
                    doc.set_sheet_material(id, material)?;
                }
                id
            }
        };
        used.push(id);
    }
    Ok(used)
}

/// The material bounds, in job millimetres, of the `index`th sheet of a
/// nest: its own sheet's, the sheet a new copy is made of, or the job's.
fn slot_bounds(doc: &Document, targets: &SheetTargets, index: usize) -> Result<BBox, CoreError> {
    let sheet = match targets.slot(index, doc) {
        Ok(id) | Err(Some(id)) => id,
        Err(None) => return Ok(doc.job_bounds()),
    };
    Ok(doc
        .sheet_material(sheet)?
        .applied_to(doc.job_setup())
        .material_bounds())
}

/// Nests onto the sheets of a plan one at a time, each at its own size:
/// every sheet takes, largest first, what still fits of the parts the
/// sheets before it did not take (as the nest over sheets of one size
/// does), then its share of the filler parts. A sheet too small for every
/// part left is passed over. Answers the placements, sheet by sheet index,
/// and each sheet's map from nest to job coordinates.
fn nest_sheet_by_sheet(
    doc: &Document,
    parts: &[geom::nest::NestPart],
    params: &geom::nest::NestParams,
    targets: &SheetTargets,
    tol: Tolerance,
) -> Result<(geom::nest::NestResult, Vec<Mat3>), CoreError> {
    use geom::nest::{NestResult, NestUnplaced, NestUnplacedReason};
    let limit = targets.capacity().unwrap_or(params.max_sheets).max(1);
    let mut left: Vec<usize> = parts.iter().map(|p| p.quantity).collect();
    let mut placed_count = vec![0usize; parts.len()];
    let mut reasons: HashMap<u64, NestUnplacedReason> = HashMap::new();
    let mut placements = Vec::new();
    let mut to_jobs: Vec<Mat3> = Vec::new();
    let any_regular = parts.iter().any(|p| !p.filler && p.quantity > 0);
    let edge = params.border + params.gap;
    for index in 0..limit {
        let regular_left = parts.iter().zip(&left).any(|(p, n)| !p.filler && *n > 0);
        if index > 0 && (!regular_left || !any_regular) {
            break;
        }
        let bounds = slot_bounds(doc, targets, index)?;
        let next = slot_bounds(doc, targets, index + 1).ok();
        let same_next = next.is_some_and(|b| {
            (b.width() - bounds.width()).abs() < 1e-9 && (b.height() - bounds.height()).abs() < 1e-9
        });
        to_jobs.push(Mat3::translate(Vec2::new(bounds.min.x, bounds.min.y)));
        if 2.0 * edge >= bounds.width().min(bounds.height()) {
            if same_next {
                break;
            }
            continue;
        }
        let sheet_params = geom::nest::NestParams {
            sheet_width: bounds.width(),
            sheet_height: bounds.height(),
            max_sheets: 1,
            ..*params
        };
        let remaining: Vec<geom::nest::NestPart> = parts
            .iter()
            .zip(&left)
            .map(|(p, n)| geom::nest::NestPart {
                quantity: if p.filler { p.quantity } else { *n },
                ..p.clone()
            })
            .collect();
        let got = geom::nest::nest_within(&remaining, &sheet_params, None, tol)?;
        for u in &got.unplaced {
            reasons.insert(u.part_id, u.reason);
        }
        let regular_here = got.placements.iter().any(|p| {
            parts
                .get(p.part_id as usize)
                .is_some_and(|part| !part.filler)
        });
        if any_regular && !regular_here {
            if same_next {
                break;
            }
            continue;
        }
        for mut p in got.placements {
            let at = p.part_id as usize;
            p.copy += placed_count[at];
            p.sheet = index;
            placements.push(p);
        }
        for (at, part) in parts.iter().enumerate() {
            let here = placements
                .iter()
                .filter(|p| p.sheet == index && p.part_id as usize == at)
                .count();
            placed_count[at] += here;
            if !part.filler {
                left[at] = left[at].saturating_sub(here);
            }
        }
        if !any_regular {
            break;
        }
    }
    let mut unplaced = Vec::new();
    for (at, part) in parts.iter().enumerate() {
        let id = part.id;
        let reason = reasons
            .get(&id)
            .copied()
            .unwrap_or(NestUnplacedReason::OutOfSheets);
        if part.filler {
            if placed_count[at] == 0 {
                unplaced.push(NestUnplaced {
                    part_id: id,
                    copy: 0,
                    reason,
                });
            }
            continue;
        }
        for copy in placed_count[at]..placed_count[at] + left[at] {
            unplaced.push(NestUnplaced {
                part_id: id,
                copy,
                reason,
            });
        }
    }
    placements.sort_by(|a, b| a.part_id.cmp(&b.part_id).then(a.copy.cmp(&b.copy)));
    let sheet_count = placements.iter().map(|p| p.sheet + 1).max().unwrap_or(0);
    to_jobs.truncate(sheet_count);
    Ok((
        NestResult {
            placements,
            unplaced,
            sheet_count,
        },
        to_jobs,
    ))
}

/// The first closed vector on `layer` on the sheet nested onto (the active
/// one when not given), with its geometry.
fn boundary_on(
    document: &Document,
    layer: u64,
    sheet: Option<u64>,
) -> Result<(u64, Path), CoreError> {
    document.layer(layer)?;
    let sheet = sheet.unwrap_or_else(|| document.active_sheet());
    document
        .order()
        .iter()
        .filter_map(|id| document.entity(*id).ok())
        .filter(|e| e.layer == layer && e.sheet == sheet)
        .find_map(|e| {
            e.path()
                .filter(|p| p.subpaths.iter().any(|s| s.closed))
                .map(|p| (e.id, p))
        })
        .ok_or_else(|| CoreError::validation("the nest boundary layer has no closed vector"))
}

fn nest_params_on_job() -> geom::nest::NestParams {
    geom::nest::NestParams {
        sheet_width: 0.0,
        sheet_height: 0.0,
        ..geom::nest::NestParams::default()
    }
}

/// One object of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Member {
    Entity(u64),
    Component(u64),
}

/// An object with the outlines it takes part in grouping with.
struct Item {
    member: Member,
    paths: Vec<Path>,
    bbox: Option<BBox>,
}

/// A part: its objects on this side and, for two sided nesting, on the
/// other side.
struct Part {
    members: Vec<Member>,
    passengers: Vec<Member>,
    paths: Vec<Path>,
}

const TOL: f64 = 0.01;

/// Runs `ops.nest`.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] with nothing to nest or when nothing fits, and
/// whatever nesting or moving the parts reports.
pub fn run(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    let args: NestArgs = parse_args(name, args)?;
    let mut ids = document.target_ids(args.ids.as_deref());
    let targets = match args.sheet_plan.as_deref() {
        Some(plan) if !plan.is_empty() => {
            for row in plan {
                document.sheet(row.sheet)?;
                if row.quantity == Some(0) {
                    return Err(CoreError::validation(
                        "each sheet in the list needs a quantity of at least 1",
                    ));
                }
            }
            SheetTargets::Plan(plan.to_vec())
        }
        _ => {
            if let Some(sheet) = args.sheet {
                document.sheet(sheet)?;
            }
            SheetTargets::From(args.sheet.unwrap_or_else(|| document.active_sheet()))
        }
    };
    let boundary = match args.boundary_layer {
        Some(layer) => Some(boundary_on(document, layer, Some(targets.first()))?),
        None => None,
    };
    if let Some((id, _)) = &boundary {
        ids.retain(|i| i != id);
    }
    if ids.is_empty() && args.components.is_empty() {
        return Err(CoreError::validation("select the parts to nest"));
    }
    for id in &args.components {
        document.component(*id)?;
    }
    let two_sided = args.two_sided && document.job_setup().is_double_sided();
    let job = document.job_bounds();
    let mut params = args.params;
    match targets.capacity() {
        Some(total) => params.max_sheets = total,
        None if !args.add_sheets && matches!(targets, SheetTargets::From(_)) => {
            params.max_sheets = 1;
        }
        None => {}
    }
    let sized = params.sheet_width > 0.0 && params.sheet_height > 0.0;
    if params.sheet_width <= 0.0 || params.sheet_height <= 0.0 {
        params.sheet_width = job.width();
        params.sheet_height = job.height();
    }
    let mut to_job = Mat3::translate(Vec2::new(job.min.x, job.min.y));
    // A boundary vector is the sheet: its box sets the size and the origin.
    let mut fence: Option<Path> = None;
    if let Some((_, path)) = &boundary {
        let b = geom::path_bbox(path)
            .ok_or_else(|| CoreError::validation("the nest boundary has no size"))?;
        params.sheet_width = b.width();
        params.sheet_height = b.height();
        to_job = Mat3::translate(Vec2::new(b.min.x, b.min.y));
        let home = Mat3::translate(Vec2::new(-b.min.x, -b.min.y));
        fence = Some(geom::transform_path(path, &home));
    }
    // A Custom Sheet Selection nests each sheet at its own size, unless a
    // boundary vector or a size given by the caller sets one for all.
    let per_sheet = matches!(targets, SheetTargets::Plan(_)) && fence.is_none() && !sized;
    let flip = flip_matrix(document);
    let tol = Tolerance::new(TOL)?;

    edit(document, "Nest Parts", |doc| {
        let mut items = Vec::new();
        for id in &ids {
            items.push(entity_item(doc, *id));
        }
        for id in &args.components {
            items.push(component_item(doc, *id)?);
        }
        let mut parts = group(items, tol);

        // The other side's objects over each part.
        let this_side = doc.active_side();
        if two_sided {
            doc.set_active_side(this_side.other())?;
            let mut others: Vec<Item> = doc
                .order()
                .to_vec()
                .into_iter()
                .map(|id| entity_item(doc, id))
                .collect();
            let component_ids: Vec<u64> = doc.all_components().iter().map(|c| c.id).collect();
            for id in component_ids {
                others.push(component_item(doc, id)?);
            }
            doc.set_active_side(this_side)?;
            for mut other in others {
                other.paths = other
                    .paths
                    .iter()
                    .map(|p| geom::transform_path(p, &flip))
                    .collect();
                other.bbox = bbox_of(&other.paths);
                if let Some(part) = parts
                    .iter_mut()
                    .find(|part| touches(&part.paths, bbox_of(&part.paths), &other, tol))
                {
                    part.passengers.push(other.member);
                    part.paths.extend(other.paths);
                }
            }
        }

        let nest_parts: Vec<geom::nest::NestPart> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let entity_ids = part.members.iter().filter_map(|m| match m {
                    Member::Entity(id) => Some(*id),
                    Member::Component(_) => None,
                });
                let marks = doc.nest_marks();
                let quantity = entity_ids
                    .clone()
                    .filter_map(|id| {
                        args.quantities
                            .get(&id.to_string())
                            .copied()
                            .or_else(|| marks.get(&id).map(|m| m.copies))
                    })
                    .max()
                    .unwrap_or(1);
                let filler = entity_ids.clone().any(|id| {
                    args.fillers.contains(&id) || marks.get(&id).is_some_and(|m| m.filler)
                });
                let has_relief = part
                    .members
                    .iter()
                    .chain(&part.passengers)
                    .any(|m| matches!(m, Member::Component(_)));
                geom::nest::NestPart {
                    id: index as u64,
                    paths: part.paths.clone(),
                    quantity,
                    filler,
                    no_mirror: has_relief,
                }
            })
            .collect();
        let (result, to_jobs) = if per_sheet {
            nest_sheet_by_sheet(doc, &nest_parts, &params, &targets, tol)?
        } else {
            let result = geom::nest::nest_within(&nest_parts, &params, fence.as_ref(), tol)?;
            let count = result
                .placements
                .iter()
                .map(|p| p.sheet + 1)
                .max()
                .unwrap_or(0);
            (result, vec![to_job; count])
        };
        if result.placements.is_empty() {
            return Err(CoreError::validation(
                "no part fits on the sheet; check the sheet size, border and spacing",
            ));
        }

        let sheet_count = to_jobs.len();
        let used: Vec<bool> = (0..sheet_count)
            .map(|i| result.placements.iter().any(|p| p.sheet == i))
            .collect();
        let sheets = resolve_sheets(doc, &targets, &used)?;
        // Where each of those sheets sits in this side's list; the other
        // side's objects go on the other side's sheet in the same place.
        let positions: Vec<usize> = sheets
            .iter()
            .map(|id| doc.sheets().iter().position(|s| s.id == *id).unwrap_or(0))
            .collect();
        // Copies are made from the originals before anything moves.
        let copy_of = |p: &geom::nest::NestPlacement| p.copy > 0 || !args.remove_originals;
        let mut moving: Vec<Vec<Member>> = Vec::with_capacity(result.placements.len());
        for p in &result.placements {
            let part = &parts[p.part_id as usize];
            let copy = copy_of(p);
            let mut these = Vec::with_capacity(part.members.len());
            for member in &part.members {
                these.push(if copy {
                    duplicate(doc, *member)?
                } else {
                    *member
                });
            }
            moving.push(these);
        }
        let mut placed_ids = Vec::new();
        for (p, members) in result.placements.iter().zip(&moving) {
            let m = p.matrix.then(&to_jobs[p.sheet]);
            for member in members {
                place(doc, *member, &m, sheets[p.sheet])?;
                if let Member::Entity(id) = member {
                    placed_ids.push(*id);
                }
            }
        }

        if two_sided && parts.iter().any(|p| !p.passengers.is_empty()) {
            doc.set_active_side(this_side.other())?;
            let mut other_sheets: Vec<u64> = doc.sheets().iter().map(|s| s.id).collect();
            let mut riders: Vec<Vec<Member>> = Vec::with_capacity(result.placements.len());
            for p in &result.placements {
                let part = &parts[p.part_id as usize];
                let copy = copy_of(p);
                let mut these = Vec::with_capacity(part.passengers.len());
                for member in &part.passengers {
                    these.push(if copy {
                        duplicate(doc, *member)?
                    } else {
                        *member
                    });
                }
                riders.push(these);
            }
            for (p, members) in result.placements.iter().zip(&riders) {
                let at = positions[p.sheet];
                while other_sheets.len() <= at {
                    other_sheets.push(doc.add_sheet(None));
                }
                // Other side point -> this side -> placed -> back again.
                let m = flip.then(&p.matrix.then(&to_jobs[p.sheet])).then(&flip);
                for member in members {
                    place(doc, *member, &m, other_sheets[at])?;
                }
            }
            doc.set_active_side(this_side)?;
        }

        doc.set_selection(&placed_ids);
        Ok(json!({
            "ids": placed_ids,
            "sheetCount": used.iter().filter(|u| **u).count(),
            "unplaced": result.unplaced,
        }))
    })
}

/// The affine map from one side's job space to the other's.
fn flip_matrix(document: &Document) -> Mat3 {
    let job = document.job_setup();
    let (ox, oy) = job.flip_xy(0.0, 0.0);
    let (xx, xy) = job.flip_xy(1.0, 0.0);
    let (yx, yy) = job.flip_xy(0.0, 1.0);
    Mat3::new(xx - ox, xy - oy, yx - ox, yy - oy, ox, oy)
}

fn bbox_of(paths: &[Path]) -> Option<BBox> {
    paths
        .iter()
        .filter_map(geom::path_bbox)
        .reduce(|a, b| a.union(b))
}

fn entity_item(doc: &Document, id: u64) -> Item {
    let paths: Vec<Path> = doc.paths_of(id).into_iter().map(|(_, p)| p).collect();
    let bbox = bbox_of(&paths);
    Item {
        member: Member::Entity(id),
        paths,
        bbox,
    }
}

/// A component's outline: the boundary round every cell it covers.
fn component_item(doc: &Document, id: u64) -> Result<Item, CoreError> {
    let component = doc.component(id)?;
    let paths =
        crate::model::contour::boundary(&component.field, f32::MIN, 0.0, Tolerance::new(TOL)?)?;
    let bbox = bbox_of(&paths);
    Ok(Item {
        member: Member::Component(id),
        paths,
        bbox,
    })
}

/// True when `item` overlaps the outlines `paths`: a node of either lies
/// inside the other's closed area, or their outlines cross.
fn touches(paths: &[Path], bbox: Option<BBox>, item: &Item, tol: Tolerance) -> bool {
    let (Some(a), Some(b)) = (bbox, item.bbox) else {
        return false;
    };
    if a.min.x > b.max.x || b.min.x > a.max.x || a.min.y > b.max.y || b.min.y > a.max.y {
        return false;
    }
    let inside = |outer: &[Path], inner: &[Path]| {
        inner.iter().any(|path| {
            nodes(path).any(|p| {
                outer
                    .iter()
                    .any(|o| point_in_path(o, p, FillRule::EvenOdd, tol))
            })
        })
    };
    if inside(paths, &item.paths) || inside(&item.paths, paths) {
        return true;
    }
    paths.iter().any(|a| {
        item.paths
            .iter()
            .any(|b| !path_intersections(a, b, tol).is_empty())
    })
}

/// Every node of a path.
fn nodes(path: &Path) -> impl Iterator<Item = Point> + '_ {
    path.subpaths
        .iter()
        .flat_map(|sub| std::iter::once(sub.start).chain(sub.segments.iter().map(|s| s.end())))
}

/// Groups overlapping items into parts, in the order of their first item.
fn group(items: Vec<Item>, tol: Tolerance) -> Vec<Part> {
    let n = items.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            if root(&mut parent, i) == root(&mut parent, j) {
                continue;
            }
            if touches(&items[i].paths, items[i].bbox, &items[j], tol) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut parts: Vec<Part> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for (i, item) in items.into_iter().enumerate() {
        let r = root(&mut parent, i);
        let index = *slot.entry(r).or_insert_with(|| {
            parts.push(Part {
                members: Vec::new(),
                passengers: Vec::new(),
                paths: Vec::new(),
            });
            parts.len() - 1
        });
        parts[index].members.push(item.member);
        parts[index].paths.extend(item.paths);
    }
    parts
}

/// A copy of one object, answering the copy.
fn duplicate(doc: &mut Document, member: Member) -> Result<Member, CoreError> {
    match member {
        Member::Entity(id) => Ok(Member::Entity(ops::duplicate(doc, id)?)),
        Member::Component(id) => {
            let clone = doc.component(id)?.clone();
            Ok(Member::Component(doc.add_component(clone)))
        }
    }
}

/// Moves one object by `m` onto `sheet`.
fn place(doc: &mut Document, member: Member, m: &Mat3, sheet: u64) -> Result<(), CoreError> {
    match member {
        Member::Entity(id) => {
            doc.transform_entities(&[id], m);
            doc.set_entity_sheet(&[id], sheet)
        }
        Member::Component(id) => {
            let params = TransformParams {
                offset: Point::new(m.e, m.f),
                rotate_deg: m.b.atan2(m.a).to_degrees(),
                centre: Some(Point::ORIGIN),
                ..TransformParams::default()
            };
            let field = crate::model::transform::transform(&doc.component(id)?.field, &params)?
                .cropped()
                .ok_or_else(|| CoreError::validation("a nested component came out empty"))?;
            doc.update_component(id, |c| {
                c.field = field;
                c.extra.sheet = sheet;
                if let Some(shape) = c.extra.shape.as_deref_mut() {
                    shape.moved(m, false);
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind, Side};
    use crate::geom::{circle, rect};

    fn add_path(doc: &mut Document, path: Path) -> u64 {
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        })
    }

    fn nest(doc: &mut Document, args: serde_json::Value) -> serde_json::Value {
        match run(doc, "ops.nest", &args.to_string()).expect("nest") {
            KernelReply::Json(text) => {
                serde_json::from_str::<serde_json::Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected JSON"),
        }
    }

    fn centre(doc: &Document, id: u64) -> Point {
        doc.bbox_of(id).expect("bbox").centre()
    }

    #[test]
    fn objects_on_a_part_move_with_it() {
        let mut doc = Document::new();
        doc.begin("Create");
        let outline = add_path(
            &mut doc,
            rect(Point::new(300.0, 300.0), Point::new(400.0, 350.0)).expect("rect"),
        );
        let hole = add_path(
            &mut doc,
            circle(Point::new(350.0, 325.0), 10.0).expect("circle"),
        );
        let other = add_path(
            &mut doc,
            rect(Point::new(500.0, 500.0), Point::new(520.0, 520.0)).expect("rect"),
        );
        doc.commit();
        let offset = |doc: &Document| {
            let a = centre(doc, outline);
            let b = centre(doc, hole);
            (b.x - a.x, b.y - a.y)
        };
        let before = offset(&doc);
        let reply = nest(
            &mut doc,
            json!({ "ids": [outline, hole, other], "params": { "rotationStepDeg": 0 } }),
        );
        assert_eq!(reply["ids"].as_array().expect("ids").len(), 3);
        let after = offset(&doc);
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
        // The outline went into the start corner, carrying the circle.
        let bbox = doc.bbox_of(outline).expect("bbox");
        let job = doc.job_bounds();
        assert!(
            bbox.min.x - job.min.x < 30.0 && bbox.min.y - job.min.y < 30.0,
            "{bbox:?}"
        );
    }

    #[test]
    fn a_boundary_layer_bounds_the_nest() {
        let mut doc = Document::new();
        doc.begin("Create");
        let part = add_path(
            &mut doc,
            rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect"),
        );
        let layer = doc.add_layer(Some("Boundary".to_string()), None);
        let fence = add_path(
            &mut doc,
            rect(Point::new(100.0, 60.0), Point::new(160.0, 120.0)).expect("rect"),
        );
        doc.set_entity_layer(&[fence], layer).expect("layer");
        doc.commit();
        nest(
            &mut doc,
            json!({ "ids": [part, fence], "boundaryLayer": layer,
                    "params": { "rotationStepDeg": 0, "border": 2.0, "gap": 1.0,
                                "toolDiameter": 0.0 } }),
        );
        // The part went into the boundary's corner with Border Gap plus
        // Clearance to spare; the boundary itself stayed put.
        let b = doc.bbox_of(part).expect("bbox");
        // (The boundary is flattened on the safe side, so a part may sit a
        // little further in than asked.)
        assert!(b.min.x >= 103.0 - 1e-3 && b.min.x < 105.0, "{b:?}");
        assert!(b.min.y >= 63.0 - 1e-3 && b.min.y < 65.0, "{b:?}");
        let f = doc.bbox_of(fence).expect("bbox");
        assert!((f.min.x - 100.0).abs() < 1e-9);
        let empty = doc.add_layer(Some("Empty".to_string()), None);
        let args = json!({ "ids": [part], "boundaryLayer": empty }).to_string();
        assert!(run(&mut doc, "ops.nest", &args).is_err());
    }

    #[test]
    fn keeping_the_originals_places_copies() {
        let mut doc = Document::new();
        doc.begin("Create");
        let part = add_path(
            &mut doc,
            rect(Point::new(300.0, 300.0), Point::new(340.0, 320.0)).expect("rect"),
        );
        doc.commit();
        let reply = nest(
            &mut doc,
            json!({ "ids": [part], "removeOriginals": false, "quantities": { part.to_string(): 2 } }),
        );
        let placed = reply["ids"].as_array().expect("ids");
        assert_eq!(placed.len(), 2);
        assert!(!placed.iter().any(|id| id.as_u64() == Some(part)));
        assert!((centre(&doc, part).x - 320.0).abs() < 1e-9);
    }

    #[test]
    fn a_component_nests_with_the_vector_round_it() {
        let mut doc = Document::new();
        doc.begin("Create");
        let outline = add_path(
            &mut doc,
            rect(Point::new(300.0, 300.0), Point::new(360.0, 340.0)).expect("rect"),
        );
        let mut field = crate::model::field::Field::empty(10, 10, 1.0, Point::new(320.5, 310.5));
        field.z.fill(2.0);
        let component = doc.add_component(crate::model::Component {
            id: 0,
            name: "dome".into(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        });
        doc.commit();
        let before = centre(&doc, outline);
        let origin_before = doc.component(component).expect("component").field.origin;
        nest(
            &mut doc,
            json!({
                "ids": [outline],
                "components": [component],
                "params": { "rotationStepDeg": 0 },
            }),
        );
        let after = centre(&doc, outline);
        let origin_after = doc.component(component).expect("component").field.origin;
        let moved = (after.x - before.x, after.y - before.y);
        assert!(moved.0.abs() > 1.0, "the part moved");
        assert!((origin_after.x - origin_before.x - moved.0).abs() < 1.0 + 1e-9);
        assert!((origin_after.y - origin_before.y - moved.1).abs() < 1.0 + 1e-9);
    }

    /// A job with a second sheet, and one part only one copy of which fits
    /// on a sheet.
    fn two_sheets_and_a_big_part() -> (Document, u64, u64, u64) {
        let mut doc = Document::new();
        let first = doc.active_sheet();
        doc.begin("Sheets");
        let second = doc.add_sheet(None);
        doc.commit();
        let job = doc.job_bounds();
        doc.begin("Create");
        let part = add_path(
            &mut doc,
            rect(
                Point::new(job.min.x, job.min.y),
                Point::new(
                    job.min.x + job.width() * 0.8,
                    job.min.y + job.height() * 0.8,
                ),
            )
            .expect("rect"),
        );
        doc.commit();
        (doc, first, second, part)
    }

    fn sheet_of(doc: &Document, id: u64) -> u64 {
        doc.entity(id).expect("entity").sheet
    }

    #[test]
    fn a_sheet_plan_fills_its_sheets_in_order_and_stops() {
        let (mut doc, first, second, part) = two_sheets_and_a_big_part();
        let reply = nest(
            &mut doc,
            json!({
                "ids": [part],
                "quantities": { part.to_string(): 3 },
                "params": { "rotationStepDeg": 0, "sheetWidth": 0, "sheetHeight": 0 },
                "sheetPlan": [{ "sheet": second, "quantity": 1 }, { "sheet": first, "quantity": 1 }],
            }),
        );
        let placed: Vec<u64> = reply["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        assert_eq!(placed.len(), 2, "two sheets hold two copies");
        assert_eq!(
            sheet_of(&doc, placed[0]),
            second,
            "the plan starts on its first row"
        );
        assert_eq!(sheet_of(&doc, placed[1]), first);
        assert_eq!(reply["unplaced"].as_array().map(Vec::len), Some(1));
        assert_eq!(doc.sheets().len(), 2, "no sheet is added past the plan");
    }

    #[test]
    fn a_sheet_plan_nests_each_sheet_at_its_own_size() {
        let mut doc = Document::new();
        let first = doc.active_sheet();
        doc.begin("Sheets");
        let small = doc.add_sheet(None);
        let mut material = doc.sheet_material(small).expect("material");
        material.width = 120.0;
        material.height = 120.0;
        doc.set_sheet_material(small, material).expect("material");
        doc.commit();
        doc.begin("Create");
        let part = add_path(
            &mut doc,
            rect(Point::new(0.0, 0.0), Point::new(80.0, 80.0)).expect("rect"),
        );
        doc.commit();
        let reply = nest(
            &mut doc,
            json!({
                "ids": [part],
                "quantities": { part.to_string(): 4 },
                "params": { "rotationStepDeg": 0, "sheetWidth": 0, "sheetHeight": 0 },
                "sheetPlan": [{ "sheet": small, "quantity": 1 }, { "sheet": first, "quantity": 1 }],
            }),
        );
        let placed: Vec<u64> = reply["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        assert_eq!(placed.len(), 4, "{reply}");
        // The 120 mm sheet holds one 80 mm part; the 300 by 200 job the rest.
        let on_small: Vec<u64> = placed
            .iter()
            .copied()
            .filter(|id| sheet_of(&doc, *id) == small)
            .collect();
        assert_eq!(on_small.len(), 1, "{reply}");
        let b = doc.bbox_of(on_small[0]).expect("box");
        assert!(b.min.x >= 0.0 && b.min.y >= 0.0, "{b:?}");
        assert!(b.max.x <= 120.0 && b.max.y <= 120.0, "{b:?}");
        let on_first = placed
            .iter()
            .filter(|id| sheet_of(&doc, **id) == first)
            .count();
        assert_eq!(on_first, 3);
        assert_eq!(reply["sheetCount"], json!(2));
        assert_eq!(reply["unplaced"], json!([]));
    }

    #[test]
    fn part_marks_are_kept_with_the_objects_saved_and_undone() {
        let (mut doc, _, _, _) = two_sheets_and_a_big_part();
        doc.begin("Create");
        let part = add_path(
            &mut doc,
            rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect"),
        );
        let filler = add_path(
            &mut doc,
            rect(Point::new(30.0, 0.0), Point::new(40.0, 10.0)).expect("rect"),
        );
        doc.commit();
        let marks = |doc: &mut Document, args: serde_json::Value| {
            run_marks(doc, "ops.nestMarks", &args.to_string()).expect("marks");
        };
        marks(&mut doc, json!({ "ids": [part], "copies": 3 }));
        marks(
            &mut doc,
            json!({ "ids": [filler], "copies": 1, "filler": true }),
        );
        assert_eq!(
            doc.summary(part).expect("summary").nest,
            Some(NestMark {
                copies: 3,
                filler: false
            })
        );
        assert!(doc
            .summary(filler)
            .expect("summary")
            .nest
            .is_some_and(|m| m.filler));
        // Saved with the job.
        let file = serde_json::to_value(doc.to_file()).expect("json");
        assert_eq!(file["nestMarks"][part.to_string()]["copies"], json!(3));
        let back =
            Document::from_file(serde_json::from_value(file).expect("file"), 1).expect("opens");
        assert_eq!(back.nest_marks(), doc.nest_marks());
        // One undo step each; 1 copy and no filler takes the mark away.
        assert!(doc.undo().is_some());
        assert!(doc.summary(filler).expect("summary").nest.is_none());
        marks(&mut doc, json!({ "ids": [part], "copies": 1 }));
        assert!(doc.nest_marks().is_empty());
        assert!(doc.undo().is_some());
        assert_eq!(doc.nest_marks().get(&part).map(|m| m.copies), Some(3));
        for bad in [0, 1001] {
            let args = json!({ "ids": [part], "copies": bad }).to_string();
            assert!(run_marks(&mut doc, "ops.nestMarks", &args).is_err());
        }
        // The nest takes the copies from the mark.
        let reply = nest(
            &mut doc,
            json!({ "ids": [part], "params": { "rotationStepDeg": 0 } }),
        );
        assert_eq!(reply["ids"].as_array().map(Vec::len), Some(3), "{reply}");
    }

    #[test]
    fn no_limit_adds_copies_of_that_sheet() {
        let (mut doc, _, second, part) = two_sheets_and_a_big_part();
        let mut material = doc.sheet_material(second).expect("material");
        material.thickness = 7.5;
        doc.begin("Material");
        doc.set_sheet_material(second, material).expect("material");
        doc.commit();
        let reply = nest(
            &mut doc,
            json!({
                "ids": [part],
                "quantities": { part.to_string(): 3 },
                "params": { "rotationStepDeg": 0, "sheetWidth": 0, "sheetHeight": 0 },
                "sheetPlan": [{ "sheet": second }],
            }),
        );
        assert_eq!(reply["ids"].as_array().map(Vec::len), Some(3));
        assert_eq!(doc.sheets().len(), 4, "two copies of the second sheet");
        for sheet in &doc.sheets()[2..] {
            let copy = doc.sheet_material(sheet.id).expect("material");
            assert!(
                (copy.thickness - 7.5).abs() < 1e-9,
                "a copy has its material"
            );
        }
    }

    #[test]
    fn a_sheet_plan_needs_known_sheets_and_quantities() {
        let (mut doc, first, _, part) = two_sheets_and_a_big_part();
        let zero = json!({ "ids": [part], "sheetPlan": [{ "sheet": first, "quantity": 0 }] });
        assert!(run(&mut doc, "ops.nest", &zero.to_string()).is_err());
        let unknown = json!({ "ids": [part], "sheetPlan": [{ "sheet": 999 }] });
        assert!(run(&mut doc, "ops.nest", &unknown.to_string()).is_err());
    }

    #[test]
    fn two_sided_parts_carry_the_other_side_with_them() {
        let mut doc = Document::new();
        let mut job = *doc.job_setup();
        job.kind = crate::doc::JobKind::DoubleSided;
        doc.begin("Job");
        doc.set_job(job).expect("job");
        doc.commit();
        doc.begin("Create");
        let outline = add_path(
            &mut doc,
            rect(Point::new(300.0, 300.0), Point::new(360.0, 340.0)).expect("rect"),
        );
        doc.commit();
        // A pocket on the back, under the part's centre as seen from the back.
        let flip = flip_matrix(&doc);
        let under = flip.apply(Point::new(330.0, 320.0));
        doc.begin("Back");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let pocket = add_path(&mut doc, circle(under, 5.0).expect("circle"));
        doc.set_active_side(Side::Top).expect("top");
        doc.commit();
        nest(
            &mut doc,
            json!({ "ids": [outline], "twoSided": true, "params": { "rotationStepDeg": 0 } }),
        );
        let part_centre = centre(&doc, outline);
        doc.set_active_side(Side::Bottom).expect("bottom");
        let pocket_centre = doc.bbox_of(pocket).expect("pocket").centre();
        let seen = flip.apply(pocket_centre);
        assert!(
            (seen.x - part_centre.x).abs() < 1e-6,
            "{seen:?} {part_centre:?}"
        );
        assert!((seen.y - part_centre.y).abs() < 1e-6);
    }
}
