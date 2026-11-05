//! Vector operations on document entities.
//!
//! Each function takes the document, the ids to work on and the arguments of
//! one `ops.*` command, and leaves the document in the state that command
//! should produce. The undo step is opened and committed by the caller, so a
//! command that fails part way can be rolled back whole.
//!
//! Where the exact behaviour matters, it is spelled out on the function: which
//! vector is the base of a boolean, what happens to the originals, and which
//! layer the result lands on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{
    self, close_with, join_paths_with, join_two, nearest_point_on_path, offset_path, path_bbox,
    reduce_nodes, BBox, FilletKind, JoinHow, Mat3, OffsetCorner, OffsetSide, Path, Point,
    SegmentKind, SubPath, Tolerance, Vec2,
};

/// One of the nine handles of a bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    /// Lower left corner.
    BottomLeft,
    /// Middle of the bottom edge.
    BottomCentre,
    /// Lower right corner.
    BottomRight,
    /// Middle of the left edge.
    MiddleLeft,
    /// Centre of the box.
    #[default]
    Centre,
    /// Middle of the right edge.
    MiddleRight,
    /// Upper left corner.
    TopLeft,
    /// Middle of the top edge.
    TopCentre,
    /// Upper right corner.
    TopRight,
}

impl Anchor {
    /// Where the anchor sits on a box.
    pub fn on(self, bbox: BBox) -> Point {
        let x = match self {
            Anchor::BottomLeft | Anchor::MiddleLeft | Anchor::TopLeft => bbox.min.x,
            Anchor::BottomCentre | Anchor::Centre | Anchor::TopCentre => bbox.centre().x,
            Anchor::BottomRight | Anchor::MiddleRight | Anchor::TopRight => bbox.max.x,
        };
        let y = match self {
            Anchor::BottomLeft | Anchor::BottomCentre | Anchor::BottomRight => bbox.min.y,
            Anchor::MiddleLeft | Anchor::Centre | Anchor::MiddleRight => bbox.centre().y,
            Anchor::TopLeft | Anchor::TopCentre | Anchor::TopRight => bbox.max.y,
        };
        Point::new(x, y)
    }
}

/// Which way a mirror flips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorAxis {
    /// Flip left to right about the vertical centre line.
    H,
    /// Flip top to bottom about the horizontal centre line.
    V,
    /// Flip about the lower left to upper right diagonal.
    Diag1,
    /// Flip about the upper left to lower right diagonal.
    Diag2,
    /// Flip about a line the caller gives.
    Line,
}

/// Which edge or centre a set of objects is lined up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlignMode {
    /// Left edges on the target's left edge.
    Left,
    /// Right edges on the target's right edge.
    Right,
    /// Top edges on the target's top edge.
    Top,
    /// Bottom edges on the target's bottom edge.
    Bottom,
    /// Centred left to right on the target.
    HCentre,
    /// Centred top to bottom on the target.
    VCentre,
    /// Centred both ways on the target.
    Centre,
    /// Right edges on the target's left edge, so the objects sit outside
    /// it on the left.
    OutsideLeft,
    /// Left edges on the target's right edge, outside it on the right.
    OutsideRight,
    /// Bottom edges on the target's top edge, outside it above.
    OutsideTop,
    /// Top edges on the target's bottom edge, outside it below.
    OutsideBottom,
}

/// What a set of objects is lined up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlignTarget {
    /// The box around everything being aligned.
    #[default]
    Selection,
    /// The material (job) outline.
    Material,
    /// The last object selected, which stays where it is.
    Last,
}

/// How objects are spread out. `h` modes work left to right, `v` modes
/// bottom to top; the edge and centre modes space that line evenly, the gap
/// modes make the empty space between neighbours equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DistributeMode {
    /// Left edges evenly spaced.
    HLeft,
    /// Centres evenly spaced left to right.
    HCentre,
    /// Right edges evenly spaced.
    HRight,
    /// Equal gaps between boxes, left to right.
    HGap,
    /// Top edges evenly spaced.
    VTop,
    /// Centres evenly spaced bottom to top.
    VCentre,
    /// Bottom edges evenly spaced.
    VBottom,
    /// Equal gaps between boxes, bottom to top.
    VGap,
}

/// How an array measures its spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrayMode {
    /// The number is the empty space between neighbours.
    #[default]
    Gap,
    /// The number is the distance between the copies' origins.
    Offset,
}

/// Which way rows or columns of an array are staggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stagger {
    /// No stagger.
    #[default]
    None,
    /// Every other row is shifted along X.
    Rows,
    /// Every other column is shifted along Y.
    Columns,
}

/// What a copy along a vector is spaced by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyAlongMode {
    /// Fit a number of copies evenly along the whole length.
    #[default]
    Count,
    /// Put a copy every so many millimetres.
    Spacing,
}

/// What the validator found, in the shape `ops.validate` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Entity the problem is on.
    pub id: u64,
    /// `open`, `selfIntersecting`, `zeroLengthSegment`, `duplicateNode` or
    /// `overlappingDuplicate`.
    pub kind: String,
    /// Where the problem is, when there is one place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// Where the problem is, when there is one place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// A sentence for the report list.
    pub message: String,
    /// Every place the view marks for it: both ends of an open vector,
    /// each crossing, the zero length span, the duplicate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Point>,
}

/// Which vectors a boundary is built round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryKind {
    /// One boundary round everything selected.
    #[default]
    Selection,
    /// One boundary round each selected vector.
    Each,
}

// --------------------------------------------------------------- helpers

/// The paths of the entities, in the order the ids were given, skipping ids
/// that carry no geometry.
fn paths_of(doc: &Document, ids: &[u64]) -> Vec<(u64, Path)> {
    ids.iter().flat_map(|id| doc.paths_of(*id)).collect()
}

/// Checks that every id is an entity and that at least one was given.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the list is empty or an id is not
/// an entity of this document.
pub fn require(doc: &Document, ids: &[u64], what: &str) -> Result<(), CoreError> {
    if ids.is_empty() {
        return Err(CoreError::validation(format!(
            "select at least one object to {what}"
        )));
    }
    for id in ids {
        doc.entity(*id)?;
    }
    Ok(())
}

/// Adds one new path entity built from a template entity.
fn add_like(doc: &mut Document, template: (u64, u64, Option<String>), path: Path) -> u64 {
    let (layer, sheet, name) = template;
    doc.insert_entity(Entity {
        rotation: 0.0,
        id: 0,
        layer,
        sheet,
        revision: 0,
        name,
        parent: None,
        kind: EntityKind::Path { path },
    })
}

/// Layer, sheet and name of an entity, for building results beside it.
fn template(doc: &Document, id: u64) -> Result<(u64, u64, Option<String>), CoreError> {
    let entity = doc.entity(id)?;
    Ok((entity.layer, entity.sheet, entity.name.clone()))
}

/// Replaces entities with new path entities carrying the given geometry, on
/// the layer of `base`. Answers the new ids.
fn replace_with(
    doc: &mut Document,
    base: u64,
    consumed: &[u64],
    paths: Vec<Path>,
) -> Result<Vec<u64>, CoreError> {
    let template = template(doc, base)?;
    doc.delete_entities(consumed);
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        if path.is_empty() {
            continue;
        }
        out.push(add_like(doc, template.clone(), path));
    }
    Ok(out)
}

/// A copy of an entity and everything under it, placed on top of the
/// z-order. Answers the id of the copy.
pub fn duplicate(doc: &mut Document, id: u64) -> Result<u64, CoreError> {
    duplicate_mapped(doc, id, &mut BTreeMap::new())
}

/// [`duplicate`], recording in `copies` the copy of every entity it copied
/// (the groups and everything under them), keyed by the original's id.
/// Copies keep their name and their own rotation.
pub fn duplicate_mapped(
    doc: &mut Document,
    id: u64,
    copies: &mut BTreeMap<u64, u64>,
) -> Result<u64, CoreError> {
    let source = doc.entity(id)?.clone();
    let copy = match source.kind {
        EntityKind::Group { children } => {
            let mut members = Vec::with_capacity(children.len());
            for child in children {
                members.push(duplicate_mapped(doc, child, copies)?);
            }
            let group = doc.group_copies(&members)?;
            if source.name.is_some() || source.rotation != 0.0 {
                let mut entity = doc.entity(group)?.clone();
                entity.name = source.name;
                entity.rotation = source.rotation;
                doc.replace_entity(group, entity)?;
            }
            group
        }
        kind => doc.insert_entity(Entity {
            rotation: source.rotation,
            id: 0,
            layer: source.layer,
            sheet: source.sheet,
            revision: 0,
            name: source.name.clone(),
            parent: None,
            kind,
        }),
    };
    copies.insert(id, copy);
    doc.note_copy(id, copy);
    Ok(copy)
}

// -------------------------------------------------------- joining, closing

/// Joins open vectors whose ends meet inside `tolerance`; with `smooth`,
/// a gap between two ends is bridged by a curve tangent to both.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn join(
    doc: &mut Document,
    ids: &[u64],
    tolerance: f64,
    smooth: bool,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "join")?;
    let tol = Tolerance::new(tolerance).unwrap_or_default();
    let paths: Vec<Path> = paths_of(doc, ids).into_iter().map(|(_, p)| p).collect();
    let joined = join_paths_with(&paths, tol, smooth);
    let base = *ids.first().unwrap_or(&0);
    let consumed: Vec<u64> = ids.to_vec();
    replace_with(doc, base, &consumed, joined)
}

/// The three Join / Close tools: with two open vectors selected, their
/// closest ends are joined whatever the gap; otherwise every selected open
/// vector is closed. `mode` is `straight` (a line), `smooth` (a curve that
/// carries on both ends) or `moveEnd` (both ends move to their midpoint).
/// Answers the ids of the vectors changed or made.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected or the mode
/// is unknown.
pub fn close(doc: &mut Document, ids: &[u64], mode: &str) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "close")?;
    let tol = Tolerance::default();
    let how = match mode {
        "smooth" => JoinHow::Smooth,
        "smoothShallow" => JoinHow::SmoothShallow,
        "straight" => JoinHow::Straight,
        "moveEnd" | "move_end" => JoinHow::MoveEnds,
        other => {
            return Err(CoreError::validation(format!(
                "close mode must be straight, smooth or moveEnd, got {other}"
            )))
        }
    };
    let open: Vec<(u64, usize)> = ids
        .iter()
        .flat_map(|id| doc.paths_of(*id))
        .flat_map(|(leaf, path)| {
            path.subpaths
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.closed && !s.segments.is_empty())
                .map(|(i, _)| (leaf, i))
                .collect::<Vec<_>>()
        })
        .collect();
    if let [(a_id, a_sub), (b_id, b_sub)] = open.as_slice() {
        if a_id != b_id || a_sub != b_sub {
            return join_pair(doc, (*a_id, *a_sub), (*b_id, *b_sub), how);
        }
    }
    let mut changed = Vec::new();
    for id in ids {
        for leaf in doc.leaves(*id) {
            let Some(path) = doc.entity(leaf)?.path() else {
                continue;
            };
            let mut next = Path::new();
            for sub in &path.subpaths {
                next.push(close_with(sub, how, tol));
            }
            set_path(doc, leaf, next)?;
            changed.push(leaf);
        }
    }
    Ok(changed)
}

/// Joins two open subpaths (in one vector or two) at their closest ends.
fn join_pair(
    doc: &mut Document,
    a: (u64, usize),
    b: (u64, usize),
    how: JoinHow,
) -> Result<Vec<u64>, CoreError> {
    let path_a = path_of(doc, a.0)?;
    let path_b = path_of(doc, b.0)?;
    let joined = join_two(subpath_at(&path_a, a.1)?, subpath_at(&path_b, b.1)?, how);
    let mut next = Path::new();
    for (index, sub) in path_a.subpaths.iter().enumerate() {
        if index == a.1 {
            next.push(joined.clone());
        } else if !(a.0 == b.0 && index == b.1) {
            next.push(sub.clone());
        }
    }
    if a.0 != b.0 {
        for (index, sub) in path_b.subpaths.iter().enumerate() {
            if index != b.1 {
                next.push(sub.clone());
            }
        }
        doc.delete_entities(&[b.0]);
    }
    set_path(doc, a.0, next)?;
    Ok(vec![a.0])
}

// ----------------------------------------------------------------- offset

/// Offsets vectors inward or outward.
///
/// The originals stay unless `delete_original` says otherwise, and every
/// result lands on the layer of the vector it came from. An open vector becomes a closed outline round it, unless the corner
/// style is sharp and one side was asked for, where a parallel curve comes
/// back instead.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected and
/// [`CoreError::Geometry`] when the distance is not usable.
pub fn offset(
    doc: &mut Document,
    ids: &[u64],
    distance: f64,
    side: OffsetSide,
    corner: OffsetCorner,
    delete_original: bool,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "offset")?;
    let tol = Tolerance::default();
    let mut out = Vec::new();
    let mut consumed = Vec::new();
    for id in ids {
        for (leaf, path) in doc.paths_of(*id) {
            let template = template(doc, leaf)?;
            let open = !path.is_closed();
            let results = if open && corner == OffsetCorner::Sharp && side != OffsetSide::Both {
                geom::parallel_offset_open(&path, distance, side == OffsetSide::Outward, tol)?
            } else {
                offset_path(&path, distance, side, corner, tol)?
            };
            for result in results {
                out.push(add_like(doc, template.clone(), result));
            }
        }
        if delete_original {
            consumed.push(*id);
        }
    }
    if !consumed.is_empty() {
        doc.delete_entities(&consumed);
    }
    Ok(out)
}

/// Builds a boundary round the selection, or round each vector, offset by
/// `offset` millimetres. With `rubber_band` the boundary is the band
/// stretched round everything (the convex hull) instead of the outline.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn boundary(
    doc: &mut Document,
    ids: &[u64],
    offset_mm: f64,
    kind: BoundaryKind,
    rubber_band: bool,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "build a boundary round")?;
    let tol = Tolerance::default();
    let mut out = Vec::new();
    let groups: Vec<Vec<u64>> = match kind {
        BoundaryKind::Selection => vec![ids.to_vec()],
        BoundaryKind::Each => ids.iter().map(|id| vec![*id]).collect(),
    };
    for group in groups {
        let paths: Vec<Path> = paths_of(doc, &group).into_iter().map(|(_, p)| p).collect();
        if paths.is_empty() {
            continue;
        }
        let welded = if rubber_band {
            rubber_band_path(&paths, tol).into_iter().collect()
        } else {
            geom::weld(&paths, tol)?
        };
        let template = template(doc, group[0])?;
        let results = if offset_mm.abs() <= tol.get() {
            welded
        } else {
            let mut grown = Vec::new();
            for path in &welded {
                grown.extend(offset_path(
                    path,
                    offset_mm.abs(),
                    if offset_mm >= 0.0 {
                        OffsetSide::Outward
                    } else {
                        OffsetSide::Inward
                    },
                    OffsetCorner::Round,
                    tol,
                )?);
            }
            grown
        };
        for result in results {
            out.push(add_like(doc, template.clone(), result));
        }
    }
    Ok(out)
}

/// The band stretched round every point of the paths: their convex hull,
/// as one closed polyline running counter clockwise. None when the points
/// do not span an area.
fn rubber_band_path(paths: &[Path], tol: Tolerance) -> Option<Path> {
    let mut points: Vec<Point> = paths
        .iter()
        .flat_map(|path| geom::flatten_path(path, tol))
        .flat_map(|line| line.points)
        .collect();
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| a.approx_eq(*b, tol));
    if points.len() < 3 {
        return None;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    // Andrew's monotone chain: lower then upper hull.
    let mut hull: Vec<Point> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &p in ordered {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 1e-12
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    if hull.len() < 3 {
        return None;
    }
    geom::polyline_path(&hull, true).ok()
}

// --------------------------------------------------------------- booleans

/// Which boolean to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Boolean {
    /// Merge everything into one outline.
    Weld,
    /// Cut the last selected out of every earlier one.
    Subtract,
    /// Clip every earlier one to the last selected (Overlap Vectors).
    Intersect,
    /// Keep what only one of them covers.
    Xor,
    /// Clip everything to the last selected vector, which is used up.
    Crop,
}

/// Runs a boolean on the selection.
///
/// Weld and Exclusive Or merge everything onto the first selected vector's
/// layer. Subtract and Overlap work so: the last selected
/// vector (or group) is the cutter, and every earlier closed vector is cut
/// by it, or clipped to it, on its own, keeping its layer. The originals and
/// the cutter are removed; an open vector is left alone.
///
/// # Errors
/// Returns [`CoreError::Validation`] when fewer than two vectors were
/// selected, and [`CoreError::Geometry`] when the geometry cannot be built.
pub fn boolean(doc: &mut Document, ids: &[u64], which: Boolean) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "combine")?;
    if ids.len() < 2 {
        return Err(CoreError::validation(
            "select at least two objects to combine them",
        ));
    }
    let tol = Tolerance::default();
    let paths: Vec<Path> = paths_of(doc, ids).into_iter().map(|(_, p)| p).collect();
    if paths.len() < 2 {
        return Err(CoreError::validation(
            "select at least two vectors to combine them",
        ));
    }
    let (base, results) = match which {
        Boolean::Weld => (ids[0], geom::weld(&paths, tol)?),
        Boolean::Xor => (ids[0], geom::xor(&paths[..1], &paths[1..], tol)?),
        Boolean::Subtract | Boolean::Intersect => return against_last(doc, ids, which, tol),
        Boolean::Crop => {
            let last = paths.len() - 1;
            (ids[0], geom::crop(&paths[..last], &paths[last..], tol)?)
        }
    };
    replace_with(doc, base, ids, results)
}

/// Subtract or Overlap: each earlier closed vector against the last
/// selected id, one result per vector on that vector's layer.
fn against_last(
    doc: &mut Document,
    ids: &[u64],
    which: Boolean,
    tol: Tolerance,
) -> Result<Vec<u64>, CoreError> {
    let (last, earlier) = ids
        .split_last()
        .ok_or_else(|| CoreError::validation("select at least two objects to combine them"))?;
    let cutter: Vec<Path> = doc.paths_of(*last).into_iter().map(|(_, p)| p).collect();
    if !cutter.iter().any(|p| p.subpaths.iter().any(|s| s.closed)) {
        return Err(CoreError::validation(
            "the last selected vector must be closed",
        ));
    }
    let mut work = Vec::new();
    for id in earlier {
        for (leaf, path) in doc.paths_of(*id) {
            if path.subpaths.iter().any(|s| s.closed) {
                work.push((leaf, path));
            }
        }
    }
    if work.is_empty() {
        return Err(CoreError::validation(
            "select the closed vectors first and the vector to cut with last",
        ));
    }
    let mut out = Vec::new();
    for (leaf, path) in work {
        let one = std::slice::from_ref(&path);
        let pieces = match which {
            Boolean::Subtract => geom::subtract(one, &cutter, tol)?,
            _ => geom::intersect_paths(one, &cutter, tol)?,
        };
        let template = template(doc, leaf)?;
        doc.delete_entities(&[leaf]);
        for piece in pieces {
            if !piece.is_empty() {
                out.push(add_like(doc, template.clone(), piece));
            }
        }
    }
    doc.delete_entities(&[*last]);
    Ok(out)
}

/// Which side of the boundary Trim Objects clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrimSide {
    /// Clear what lies inside the boundary.
    Inside,
    /// Clear what lies outside the boundary.
    #[default]
    Outside,
}

/// Trim Objects on vectors: the last id is the boundary (a closed vector or
/// a group of them) and stays; every earlier vector, open or closed, loses
/// what lies on the cleared side. `refit` fits arcs back onto the clipped
/// shapes, otherwise they stay as short lines. A vector with nothing left is
/// deleted. Answers the ids of the trimmed vectors that remain.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the boundary is missing or not
/// closed.
pub fn trim_objects(
    doc: &mut Document,
    ids: &[u64],
    side: TrimSide,
    refit: bool,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "trim")?;
    let (last, earlier) = ids
        .split_last()
        .ok_or_else(|| CoreError::validation("select the objects to trim, then the boundary"))?;
    let boundary = trim_boundary(doc, *last)?;
    let tol = Tolerance::default();
    let mut kept = Vec::new();
    for id in earlier {
        for (leaf, path) in doc.paths_of(*id) {
            let clipped =
                geom::clip_to_boundary(&path, &boundary, side == TrimSide::Inside, refit, tol);
            if clipped.is_empty() {
                doc.delete_entities(&[leaf]);
            } else {
                set_path(doc, leaf, clipped)?;
                kept.push(leaf);
            }
        }
    }
    Ok(kept)
}

/// The closed vectors of the boundary id of Trim Objects.
///
/// # Errors
/// [`CoreError::Validation`] when it holds no closed vector.
pub fn trim_boundary(doc: &Document, id: u64) -> Result<Vec<Path>, CoreError> {
    doc.entity(id)?;
    let boundary: Vec<Path> = doc.paths_of(id).into_iter().map(|(_, p)| p).collect();
    if !boundary.iter().any(|p| p.subpaths.iter().any(|s| s.closed)) {
        return Err(CoreError::validation(
            "select the boundary last: it must be a closed vector or a group of them",
        ));
    }
    Ok(boundary)
}

// ------------------------------------------------------ corners and edges

/// What a Create Fillets click at a point does to a vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilletTarget {
    /// Takes out the fillet arc of subpath `sub` at segment `segment`.
    Remove { sub: usize, segment: usize },
    /// Fillets the corner of subpath `sub` at node `node`.
    Corner { sub: usize, node: usize },
}

/// The fillet a click on `path` removes (the click is on an arc between two
/// straight walls, clearly nearer the arc than any node), else the corner
/// it fillets (the nearest node).
pub fn fillet_target(path: &Path, at: Point) -> Option<FilletTarget> {
    let (sub, node) = nearest_node(path, at)?;
    let node_distance = path.subpaths[sub].nodes()[node].distance(at);
    let mut best: Option<(usize, usize, f64)> = None;
    for (sub_index, subpath) in path.subpaths.iter().enumerate() {
        for (segment, (start, seg)) in subpath.iter().enumerate() {
            if !matches!(seg, geom::Segment::Arc { .. }) {
                continue;
            }
            let (_, near) = geom::nearest_on_segment(start, seg, at);
            let d = near.distance(at);
            if best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((sub_index, segment, d));
            }
        }
    }
    if let Some((sub_index, segment, d)) = best {
        if d < 0.5 * node_distance
            && geom::remove_fillet(&path.subpaths[sub_index], segment).is_ok()
        {
            return Some(FilletTarget::Remove {
                sub: sub_index,
                segment,
            });
        }
    }
    Some(FilletTarget::Corner { sub, node })
}

/// Rounds, dog-bones or t-bones the corner nearest a click point, or takes
/// out the fillet the click is on. A T-Bone runs along the wall the click
/// is clearly nearer to, else the longer one.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the entity has no path, and
/// [`CoreError::Geometry`] when the corner cannot take that fillet.
pub fn fillet(
    doc: &mut Document,
    id: u64,
    at: Point,
    radius: f64,
    kind: FilletKind,
) -> Result<(), CoreError> {
    let path = doc
        .entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation(format!("entity {id} has no path to fillet")))?;
    let target =
        fillet_target(&path, at).ok_or_else(|| CoreError::geometry("no corner near that point"))?;
    let mut next = path.clone();
    match target {
        FilletTarget::Remove { sub, segment } => {
            next.subpaths[sub] = geom::remove_fillet(&path.subpaths[sub], segment)?;
        }
        FilletTarget::Corner { sub, node } => {
            let side = geom::tbone_side_near(&path.subpaths[sub], node, at);
            next.subpaths[sub] =
                geom::fillet_corner_on(&path.subpaths[sub], node, radius, kind, side)?;
        }
    }
    set_path(doc, id, next)
}

/// Chamfers the corner nearest a click point.
///
/// # Errors
/// As [`fillet`].
pub fn chamfer(doc: &mut Document, id: u64, at: Point, distance: f64) -> Result<(), CoreError> {
    let path = doc
        .entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation(format!("entity {id} has no path to chamfer")))?;
    let (sub_index, node_index) =
        nearest_node(&path, at).ok_or_else(|| CoreError::geometry("no corner near that point"))?;
    let chamfered = geom::chamfer_corner(&path.subpaths[sub_index], node_index, distance)?;
    let mut next = path.clone();
    next.subpaths[sub_index] = chamfered;
    set_path(doc, id, next)
}

/// Trims the piece of a vector that the click point sits on, back to where it
/// crosses any other vector.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the vector crosses nothing.
pub fn trim(doc: &mut Document, id: u64, at: Point) -> Result<(), CoreError> {
    let path = doc
        .entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation(format!("entity {id} has no path to trim")))?;
    let tol = Tolerance::default();
    let cutter = others(doc, id);
    let (sub_index, _) = nearest_subpath(&path, at)
        .ok_or_else(|| CoreError::geometry("that point is not on the vector"))?;
    let pieces = geom::trim_to_intersection(&path.subpaths[sub_index], &cutter, at, tol)?;
    let mut next = Path::new();
    for (index, sub) in path.subpaths.iter().enumerate() {
        if index == sub_index {
            for piece in &pieces {
                next.push(piece.clone());
            }
        } else {
            next.push(sub.clone());
        }
    }
    set_path(doc, id, next)
}

/// Extends the open end of a vector nearest the click point until it meets
/// another vector.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the extension meets nothing.
pub fn extend(doc: &mut Document, id: u64, at: Point) -> Result<(), CoreError> {
    let path = doc
        .entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation(format!("entity {id} has no path to extend")))?;
    let tol = Tolerance::default();
    let targets = others(doc, id);
    let (sub_index, _) = nearest_subpath(&path, at)
        .ok_or_else(|| CoreError::geometry("that point is not on the vector"))?;
    let sub = &path.subpaths[sub_index];
    let at_start = at.distance(sub.start) <= at.distance(sub.end());
    let extended = geom::extend_to_nearest(sub, &targets, at_start, tol)?;
    let mut next = path.clone();
    next.subpaths[sub_index] = extended;
    set_path(doc, id, next)
}

/// One open end of a vector: which subpath, and its last node (`at_end`)
/// or its first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenEnd {
    /// The vector.
    pub id: u64,
    /// Index of the open subpath in its path.
    pub subpath: usize,
    /// The last node when true, the first when false.
    pub at_end: bool,
}

/// Extend Vectors' second click: each end is carried on in a straight line
/// to `to` (millimetres), the point the view found on a span or where two
/// extensions meet.
///
/// # Errors
/// Returns [`CoreError::Validation`] when an end is not the end of an open
/// subpath.
pub fn extend_ends_to(doc: &mut Document, ends: &[OpenEnd], to: Point) -> Result<(), CoreError> {
    if ends.is_empty() {
        return Err(CoreError::validation(
            "pick the end of an open vector to extend",
        ));
    }
    for end in ends {
        let mut path = path_of(doc, end.id)?;
        let sub = path
            .subpaths
            .get_mut(end.subpath)
            .filter(|s| !s.closed && !s.segments.is_empty())
            .ok_or_else(|| CoreError::validation("that is not the end of an open vector"))?;
        if end.at_end {
            if sub.end().distance(to) > 1e-9 {
                sub.line_to(to);
            }
        } else if sub.start.distance(to) > 1e-9 {
            let mut reversed = geom::reverse_subpath(sub);
            reversed.line_to(to);
            *sub = geom::reverse_subpath(&reversed);
        }
        set_path(doc, end.id, path)?;
    }
    Ok(())
}

/// Every other entity's geometry as one path, which trim and extend cut to.
fn others(doc: &Document, id: u64) -> Path {
    let mut out = Path::new();
    for other in doc.selectable() {
        if other == id {
            continue;
        }
        for (_, path) in doc.paths_of(other) {
            for sub in path.subpaths {
                out.push(sub);
            }
        }
    }
    out
}

/// The subpath and node nearest a point.
fn nearest_node(path: &Path, at: Point) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, f64)> = None;
    for (sub_index, sub) in path.subpaths.iter().enumerate() {
        for (node_index, node) in sub.nodes().iter().enumerate() {
            let d = node.distance_squared(at);
            if best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((sub_index, node_index, d));
            }
        }
    }
    best.map(|(s, n, _)| (s, n))
}

/// The subpath nearest a point, and how far away it is.
fn nearest_subpath(path: &Path, at: Point) -> Option<(usize, f64)> {
    let tol = Tolerance::default();
    let mut best: Option<(usize, f64)> = None;
    for (index, sub) in path.subpaths.iter().enumerate() {
        let one = Path::from_subpath(sub.clone());
        if let Some(near) = nearest_point_on_path(&one, at, tol) {
            if best.is_none_or(|(_, d)| near.distance < d) {
                best = Some((index, near.distance));
            }
        }
    }
    best
}

/// Writes a new path onto an entity, recording the change and bumping the
/// entity revision.
pub fn set_path(doc: &mut Document, id: u64, path: Path) -> Result<(), CoreError> {
    doc.entity(id)?;
    doc.capture(id);
    let revision = doc.revision() + 1;
    let entity = doc
        .entities_mut()
        .get_mut(&id)
        .ok_or_else(|| CoreError::validation(format!("no entity {id}")))?;
    entity.set_path(path)?;
    entity.revision = revision;
    Ok(())
}

// -------------------------------------------------------------- transforms

/// Moves entities by an offset.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn move_by(doc: &mut Document, ids: &[u64], dx: f64, dy: f64) -> Result<(), CoreError> {
    require(doc, ids, "move")?;
    doc.transform_entities(ids, &Mat3::translate(Vec2::new(dx, dy)));
    Ok(())
}

/// Moves entities so that the anchor of their box lands on a point.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected or the
/// selection has no geometry.
pub fn move_to(
    doc: &mut Document,
    ids: &[u64],
    x: f64,
    y: f64,
    anchor: Anchor,
) -> Result<(), CoreError> {
    require(doc, ids, "move")?;
    let bbox = doc
        .bbox_of_many(ids)
        .ok_or_else(|| CoreError::validation("the selection has no geometry to move"))?;
    let from = anchor.on(bbox);
    move_by(doc, ids, x - from.x, y - from.y)
}

/// Rotates entities about a point, by degrees counter clockwise.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn rotate(
    doc: &mut Document,
    ids: &[u64],
    angle_deg: f64,
    cx: f64,
    cy: f64,
) -> Result<(), CoreError> {
    require(doc, ids, "rotate")?;
    let m = Mat3::rotate_about(angle_deg.to_radians(), Point::new(cx, cy));
    doc.transform_entities(ids, &m);
    Ok(())
}

/// Resizes entities so their box becomes the given size.
///
/// `link_xy` keeps the aspect ratio, using whichever of width and height was
/// given, and `anchor` picks the point that stays put.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected, the selection
/// has no size, or the requested size is not positive.
pub fn set_size(
    doc: &mut Document,
    ids: &[u64],
    width: Option<f64>,
    height: Option<f64>,
    link_xy: bool,
    anchor: Anchor,
) -> Result<(), CoreError> {
    require(doc, ids, "resize")?;
    let bbox = doc
        .bbox_of_many(ids)
        .ok_or_else(|| CoreError::validation("the selection has no geometry to resize"))?;
    let (w, h) = (bbox.width(), bbox.height());
    if w <= 0.0 && h <= 0.0 {
        return Err(CoreError::validation(
            "the selection has no size, so it cannot be scaled to one",
        ));
    }
    for value in [width, height].into_iter().flatten() {
        if !value.is_finite() || value <= 0.0 {
            return Err(CoreError::validation(format!(
                "a size must be a positive number of millimeters, got {value}"
            )));
        }
    }
    let sx = match width {
        Some(target) if w > 0.0 => target / w,
        _ => 1.0,
    };
    let sy = match height {
        Some(target) if h > 0.0 => target / h,
        _ => 1.0,
    };
    let (sx, sy) = if link_xy {
        let k = match (width, height) {
            (Some(_), None) => sx,
            (None, Some(_)) => sy,
            _ => sx.min(sy),
        };
        (k, k)
    } else {
        (sx, sy)
    };
    let pivot = anchor.on(bbox);
    doc.transform_entities(ids, &Mat3::scale_about(sx, sy, pivot));
    Ok(())
}

/// Mirrors entities, optionally leaving the originals behind.
///
/// The mirror is about the centre of the selection, which is what the H and V
/// keys do. Answers the ids that ended up mirrored: the copies when
/// `copy` is set, the originals otherwise.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected, and
/// [`CoreError::Geometry`] when a mirror line has no direction.
pub fn mirror(
    doc: &mut Document,
    ids: &[u64],
    axis: MirrorAxis,
    line: Option<(Point, Point)>,
    copy: bool,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "mirror")?;
    let bbox = doc
        .bbox_of_many(ids)
        .ok_or_else(|| CoreError::validation("the selection has no geometry to mirror"))?;
    let centre = bbox.centre();
    let m = match axis {
        MirrorAxis::H => Mat3::mirror_across_line(centre, Vec2::new(0.0, 1.0))?,
        MirrorAxis::V => Mat3::mirror_across_line(centre, Vec2::new(1.0, 0.0))?,
        MirrorAxis::Diag1 => Mat3::mirror_across_line(centre, Vec2::new(1.0, 1.0))?,
        MirrorAxis::Diag2 => Mat3::mirror_across_line(centre, Vec2::new(1.0, -1.0))?,
        MirrorAxis::Line => {
            let (a, b) = line.ok_or_else(|| {
                CoreError::validation("mirroring about a line needs the line's two points")
            })?;
            Mat3::mirror_across_line(a, a.to(b))?
        }
    };
    let targets = if copy {
        let mut copies = Vec::with_capacity(ids.len());
        for id in ids {
            copies.push(duplicate(doc, *id)?);
        }
        copies
    } else {
        ids.to_vec()
    };
    doc.transform_entities(&targets, &m);
    Ok(targets)
}

/// Lines entities up with each other, the material, or the last one
/// selected (`ids` in selection order).
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn align(
    doc: &mut Document,
    ids: &[u64],
    mode: AlignMode,
    target: AlignTarget,
) -> Result<(), CoreError> {
    require(doc, ids, "align")?;
    let (reference, moving) = match target {
        AlignTarget::Material => (Some(doc.job_bounds()), ids),
        AlignTarget::Last => match ids.split_last() {
            Some((last, rest)) => (doc.bbox_of(*last), rest),
            None => (None, ids),
        },
        AlignTarget::Selection => (doc.bbox_of_many(ids), ids),
    };
    let Some(reference) = reference else {
        return Ok(());
    };
    let target_centre = reference.centre();
    if matches!(target, AlignTarget::Material) {
        // The selection moves as one block, as a design is centred in the
        // material without its parts being stacked on each other.
        let Some(bbox) = doc.bbox_of_many(moving) else {
            return Ok(());
        };
        let centre = bbox.centre();
        let (dx, dy) = match mode {
            AlignMode::HCentre => (target_centre.x - centre.x, 0.0),
            AlignMode::VCentre => (0.0, target_centre.y - centre.y),
            _ => (target_centre.x - centre.x, target_centre.y - centre.y),
        };
        if dx != 0.0 || dy != 0.0 {
            doc.transform_entities(moving, &Mat3::translate(Vec2::new(dx, dy)));
        }
        return Ok(());
    }
    for id in moving {
        let Some(bbox) = doc.bbox_of(*id) else {
            continue;
        };
        let centre = bbox.centre();
        let (dx, dy) = match mode {
            AlignMode::Left => (reference.min.x - bbox.min.x, 0.0),
            AlignMode::Right => (reference.max.x - bbox.max.x, 0.0),
            AlignMode::Top => (0.0, reference.max.y - bbox.max.y),
            AlignMode::Bottom => (0.0, reference.min.y - bbox.min.y),
            AlignMode::HCentre => (target_centre.x - centre.x, 0.0),
            AlignMode::VCentre => (0.0, target_centre.y - centre.y),
            AlignMode::Centre => (target_centre.x - centre.x, target_centre.y - centre.y),
            AlignMode::OutsideLeft => (reference.min.x - bbox.max.x, 0.0),
            AlignMode::OutsideRight => (reference.max.x - bbox.min.x, 0.0),
            AlignMode::OutsideTop => (0.0, reference.max.y - bbox.min.y),
            AlignMode::OutsideBottom => (0.0, reference.min.y - bbox.max.y),
        };
        if dx != 0.0 || dy != 0.0 {
            doc.transform_entities(&[*id], &Mat3::translate(Vec2::new(dx, dy)));
        }
    }
    Ok(())
}

/// Spreads entities out evenly. The outermost ones stay put; with
/// `spacing` the rest are laid out from the first with that step or gap.
/// Fewer than two objects are left alone.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn distribute(
    doc: &mut Document,
    ids: &[u64],
    mode: DistributeMode,
    spacing: Option<f64>,
) -> Result<(), CoreError> {
    require(doc, ids, "distribute")?;
    use DistributeMode as D;
    let horizontal = matches!(mode, D::HLeft | D::HCentre | D::HRight | D::HGap);
    let key = |b: &BBox| match mode {
        D::HLeft => b.min.x,
        D::HRight => b.max.x,
        D::HCentre | D::HGap => b.centre().x,
        D::VBottom => b.min.y,
        D::VTop => b.max.y,
        D::VCentre | D::VGap => b.centre().y,
    };
    let size = |b: &BBox| if horizontal { b.width() } else { b.height() };
    let low = |b: &BBox| if horizontal { b.min.x } else { b.min.y };
    let mut boxes: Vec<(u64, BBox)> = ids
        .iter()
        .filter_map(|id| doc.bbox_of(*id).map(|b| (*id, b)))
        .collect();
    if boxes.len() < 2 {
        return Ok(());
    }
    // A stable sort keeps ties in selection order, as the view expects.
    boxes.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
    let count = boxes.len();
    let first = boxes[0].1;
    let last = boxes[count - 1].1;
    let mut deltas = Vec::with_capacity(count);
    if matches!(mode, D::HGap | D::VGap) {
        let gap = spacing.unwrap_or_else(|| {
            let total: f64 = boxes.iter().map(|(_, b)| size(b)).sum();
            let span = low(&last) + size(&last) - low(&first);
            (span - total) / (count as f64 - 1.0)
        });
        let mut cursor = low(&first);
        for (id, bbox) in &boxes {
            deltas.push((*id, cursor - low(bbox)));
            cursor += size(bbox) + gap;
        }
    } else {
        let start = key(&first);
        let step = spacing.unwrap_or((key(&last) - start) / (count as f64 - 1.0));
        for (index, (id, bbox)) in boxes.iter().enumerate() {
            deltas.push((*id, start + step * index as f64 - key(bbox)));
        }
    }
    for (id, delta) in deltas {
        if delta != 0.0 {
            let v = if horizontal {
                Vec2::new(delta, 0.0)
            } else {
                Vec2::new(0.0, delta)
            };
            doc.transform_entities(&[id], &Mat3::translate(v));
        }
    }
    Ok(())
}

// ----------------------------------------------------------------- arrays

/// Arguments of `ops.arrayCopy`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayArgs {
    /// How many rows, including the original.
    pub rows: usize,
    /// How many columns, including the original.
    pub cols: usize,
    /// Space or pitch along X, in millimetres.
    #[serde(default)]
    pub gap_x: f64,
    /// Space or pitch along Y, in millimetres.
    #[serde(default)]
    pub gap_y: f64,
    /// Whether the numbers are gaps or pitches.
    #[serde(default)]
    pub mode: ArrayMode,
    /// Whether every other row or column is offset.
    #[serde(default)]
    pub stagger: Stagger,
    /// Fraction of a step the stagger shifts by, normally a half.
    #[serde(default = "half")]
    pub stagger_fraction: f64,
    /// Group the copies into one object.
    #[serde(default)]
    pub group: bool,
    /// Block symmetry: how the copy in each place of a two by two block is
    /// turned, first row then second row (the first place is the
    /// original, left as it is).
    #[serde(default)]
    pub symmetry: [BlockTurn; 4],
    /// Row displacement: every other row moves this far, X and Y mm.
    #[serde(default)]
    pub row_shift: [f64; 2],
    /// Column displacement: every other column moves this far, X and Y mm.
    #[serde(default)]
    pub col_shift: [f64; 2],
}

/// How a copy in a two by two symmetry block is turned about its middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockTurn {
    /// As the original.
    #[default]
    None,
    /// Mirrored left to right.
    MirrorX,
    /// Mirrored top to bottom.
    MirrorY,
    /// Turned half round.
    Rotate,
}

impl BlockTurn {
    /// The turn about `centre`.
    pub fn about(self, centre: Point) -> Mat3 {
        match self {
            BlockTurn::None => Mat3::IDENTITY,
            BlockTurn::MirrorX => Mat3::scale_about(-1.0, 1.0, centre),
            BlockTurn::MirrorY => Mat3::scale_about(1.0, -1.0, centre),
            BlockTurn::Rotate => Mat3::scale_about(-1.0, -1.0, centre),
        }
    }
}

fn half() -> f64 {
    0.5
}

/// Copies a selection into a grid. Answers the ids of the copies, which does
/// not include the original.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected or the grid
/// has no copies in it.
pub fn array_copy(
    doc: &mut Document,
    ids: &[u64],
    args: &ArrayArgs,
) -> Result<Vec<u64>, CoreError> {
    array_copy_with(doc, ids, args, None, &mut |_, _, _| Ok(()))
}

/// Makes the other objects' copies for one cell of an array: from the
/// cell's move and the entity copies made there (original id to copy).
pub type ArrayCell<'a> =
    dyn FnMut(&mut Document, &Mat3, &BTreeMap<u64, u64>) -> Result<(), CoreError> + 'a;

/// [`array_copy`] of the entities `ids` together with more objects whose box
/// is `more`: the grid steps over the box of both, and `cell` makes the
/// other objects' copies for each cell from the cell's move and the copies
/// made of the entities there (original id to copy).
///
/// # Errors
/// As [`array_copy`], and whatever `cell` answers.
pub fn array_copy_with(
    doc: &mut Document,
    ids: &[u64],
    args: &ArrayArgs,
    more: Option<BBox>,
    cell: &mut ArrayCell<'_>,
) -> Result<Vec<u64>, CoreError> {
    if more.is_none() {
        require(doc, ids, "copy into an array")?;
    }
    if args.rows == 0 || args.cols == 0 {
        return Err(CoreError::validation(
            "an array needs at least one row and one column",
        ));
    }
    let bbox = [doc.bbox_of_many(ids), more]
        .into_iter()
        .flatten()
        .reduce(|a, b| a.union(b))
        .ok_or_else(|| CoreError::validation("the selection has no geometry to array"))?;
    let step_x = match args.mode {
        ArrayMode::Gap => bbox.width() + args.gap_x,
        ArrayMode::Offset => args.gap_x,
    };
    let step_y = match args.mode {
        ArrayMode::Gap => bbox.height() + args.gap_y,
        ArrayMode::Offset => args.gap_y,
    };
    let mut copies = Vec::new();
    for row in 0..args.rows {
        for col in 0..args.cols {
            if row == 0 && col == 0 {
                continue;
            }
            let mut dx = step_x * col as f64;
            let mut dy = step_y * row as f64;
            match args.stagger {
                Stagger::Rows if row % 2 == 1 => dx += step_x * args.stagger_fraction,
                Stagger::Columns if col % 2 == 1 => dy += step_y * args.stagger_fraction,
                _ => {}
            }
            if row % 2 == 1 {
                dx += args.row_shift[0];
                dy += args.row_shift[1];
            }
            if col % 2 == 1 {
                dx += args.col_shift[0];
                dy += args.col_shift[1];
            }
            let turn = args.symmetry[(row % 2) * 2 + col % 2].about(bbox.centre());
            let mut made = Vec::with_capacity(ids.len());
            let mut map = BTreeMap::new();
            for id in ids {
                made.push(duplicate_mapped(doc, *id, &mut map)?);
            }
            let m = turn.then(&Mat3::translate(Vec2::new(dx, dy)));
            doc.transform_entities(&made, &m);
            cell(doc, &m, &map)?;
            copies.extend(made);
        }
    }
    if args.group && copies.len() > 1 {
        let group = doc.group(&copies)?;
        return Ok(vec![group]);
    }
    Ok(copies)
}

/// Arguments of `ops.circularArray`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircularArrayArgs {
    /// Centre X of the ring, in millimetres.
    pub cx: f64,
    /// Centre Y of the ring, in millimetres.
    pub cy: f64,
    /// How many objects the ring ends up with, including the original.
    pub count: usize,
    /// Total angle the copies spread over, in degrees. 360 closes the ring.
    #[serde(default = "full_turn")]
    pub angle: f64,
    /// Turn each copy to face out of the ring, rather than keeping it
    /// upright.
    #[serde(default = "yes")]
    pub rotate_copies: bool,
    /// Place the copies at this radius from the centre instead of leaving
    /// them where they are.
    #[serde(default)]
    pub radius: Option<f64>,
    /// Group the copies into one object.
    #[serde(default)]
    pub group: bool,
    /// Angle between neighbouring objects, in degrees (positive counter
    /// clockwise). When set it is used as it is and `angle` is ignored.
    #[serde(default)]
    pub step: Option<f64>,
}

fn full_turn() -> f64 {
    360.0
}

fn yes() -> bool {
    true
}

/// Copies a selection round a centre. Answers the ids of the copies.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected or fewer than
/// two objects were asked for.
pub fn circular_array(
    doc: &mut Document,
    ids: &[u64],
    args: &CircularArrayArgs,
) -> Result<Vec<u64>, CoreError> {
    circular_array_with(doc, ids, args, None, &mut |_, _, _| Ok(()))
}

/// [`circular_array`] of the entities `ids` together with more objects
/// whose box is `more`: the ring places the box of both, and `cell` makes
/// the other objects' copies for each place from its move and the copies
/// made of the entities there (see [`array_copy_with`]).
///
/// # Errors
/// As [`circular_array`], and whatever `cell` answers.
pub fn circular_array_with(
    doc: &mut Document,
    ids: &[u64],
    args: &CircularArrayArgs,
    more: Option<BBox>,
    cell: &mut ArrayCell<'_>,
) -> Result<Vec<u64>, CoreError> {
    if more.is_none() {
        require(doc, ids, "copy round a circle")?;
    }
    if args.count < 2 {
        return Err(CoreError::validation(
            "a circular array needs at least two objects",
        ));
    }
    let centre = Point::new(args.cx, args.cy);
    let bbox = [doc.bbox_of_many(ids), more]
        .into_iter()
        .flatten()
        .reduce(|a, b| a.union(b))
        .ok_or_else(|| CoreError::validation("the selection has no geometry to array"))?;
    // A full turn shares the angle between every object, a partial sweep puts
    // the last copy on the end of the sweep.
    let full = (args.angle.abs() - 360.0).abs() < 1e-9;
    let divisions = if full {
        args.count as f64
    } else {
        (args.count - 1) as f64
    };
    let step = args.step.unwrap_or(args.angle / divisions);
    // With a radius the objects are first put that far right of the centre.
    let start = match args.radius {
        Some(radius) => {
            let from = bbox.centre();
            Mat3::translate(Vec2::new(centre.x + radius - from.x, centre.y - from.y))
        }
        None => Mat3::IDENTITY,
    };
    let about = start.apply(bbox.centre());
    let mut copies = Vec::new();
    for index in 1..args.count {
        let angle = (step * index as f64).to_radians();
        let ring = if args.rotate_copies {
            Mat3::rotate_about(angle, centre)
        } else {
            // Move the copy round the ring but leave it the way up it was.
            let moved = Mat3::rotate_about(angle, centre).apply(about);
            Mat3::translate(Vec2::new(moved.x - about.x, moved.y - about.y))
        };
        let m = start.then(&ring);
        let mut made = Vec::with_capacity(ids.len());
        let mut map = BTreeMap::new();
        for id in ids {
            made.push(duplicate_mapped(doc, *id, &mut map)?);
        }
        doc.transform_entities(&made, &m);
        cell(doc, &m, &map)?;
        copies.extend(made);
    }
    if args.group && copies.len() > 1 {
        let group = doc.group(&copies)?;
        return Ok(vec![group]);
    }
    Ok(copies)
}

/// Arguments of `ops.copyAlong`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyAlongArgs {
    /// Vector the copies follow, when `path_ids` is empty.
    #[serde(default)]
    pub path_id: u64,
    /// The vectors the copies follow (copies the first
    /// selected object along every other selected vector).
    #[serde(default)]
    pub path_ids: Vec<u64>,
    /// Whether `count` or `spacing` sets the pitch.
    #[serde(default)]
    pub distribute_mode: CopyAlongMode,
    /// How many copies on each vector, when the mode is `count`.
    #[serde(default)]
    pub count: usize,
    /// Distance between copies in millimetres, when the mode is `spacing`.
    #[serde(default)]
    pub spacing: f64,
    /// Force even spacing: the distance is stretched so copies land on
    /// both ends of each vector.
    #[serde(default)]
    pub force_even: bool,
    /// Copy Circles: circles of this diameter, in millimetres, are placed
    /// instead of copies of the selection.
    #[serde(default)]
    pub circle_diameter: Option<f64>,
    /// Create Copies on new layer.
    #[serde(default)]
    pub new_layer: bool,
    /// Turn each copy to follow the curve.
    #[serde(default = "yes")]
    pub align_to_curve: bool,
    /// Start this far along the curve, in millimetres.
    #[serde(default)]
    pub start_offset: f64,
    /// Walk the curve backwards.
    #[serde(default)]
    pub reverse: bool,
    /// Delete the object that was copied.
    #[serde(default)]
    pub delete_original: bool,
}

impl CopyAlongArgs {
    /// The vectors the copies follow.
    pub fn guides(&self) -> Vec<u64> {
        if self.path_ids.is_empty() {
            vec![self.path_id]
        } else {
            self.path_ids.clone()
        }
    }
}

/// Name of the layer Create Copies on new layer makes.
pub const COPY_ALONG_LAYER: &str = "Copies Along Vectors";

/// The distances along a guide `length` long where copies go.
fn copy_along_stations(
    length: f64,
    closed: bool,
    args: &CopyAlongArgs,
) -> Result<Vec<f64>, CoreError> {
    let start = args.start_offset.clamp(0.0, length);
    let run = length - start;
    let stations = match args.distribute_mode {
        CopyAlongMode::Count => {
            let n = args.count;
            if n == 0 {
                return Err(CoreError::validation(
                    "copying along a vector needs at least one copy",
                ));
            }
            // A closed loop's end is its start, so the copies share the
            // loop out evenly instead of doubling up there.
            let parts = if closed {
                n
            } else {
                n.saturating_sub(1).max(1)
            };
            (0..n)
                .map(|i| {
                    if n == 1 && !closed {
                        start
                    } else {
                        start + run * i as f64 / parts as f64
                    }
                })
                .collect()
        }
        CopyAlongMode::Spacing => {
            if !args.spacing.is_finite() || args.spacing <= 0.0 {
                return Err(CoreError::validation(
                    "a spacing must be a positive number of millimeters",
                ));
            }
            if args.force_even {
                let parts = ((run / args.spacing).round() as usize).max(1);
                let step = run / parts as f64;
                let n = if closed { parts } else { parts + 1 };
                (0..n).map(|i| start + step * i as f64).collect()
            } else {
                let mut n = ((run / args.spacing).floor() as usize) + 1;
                if closed && n > 1 && (n - 1) as f64 * args.spacing >= run - 1e-9 {
                    n -= 1;
                }
                (0..n).map(|i| start + args.spacing * i as f64).collect()
            }
        }
    };
    Ok(stations)
}

/// Copies a selection along a vector. Answers the ids of the copies.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the guide vector is not a vector or
/// the spacing does not fit on it.
pub fn copy_along(
    doc: &mut Document,
    ids: &[u64],
    args: &CopyAlongArgs,
) -> Result<Vec<u64>, CoreError> {
    copy_along_with(doc, ids, args, None, &mut |_, _, _| Ok(()))
}

/// [`copy_along`] of the entities `ids` together with more objects whose
/// box is `more`: the box of both is placed along the guide, and `cell`
/// makes the other objects' copies for each place from its move and the
/// copies made of the entities there (see [`array_copy_with`]). Deleting
/// the originals deletes only the entities; the caller removes the rest.
///
/// # Errors
/// As [`copy_along`], and whatever `cell` answers.
pub fn copy_along_with(
    doc: &mut Document,
    ids: &[u64],
    args: &CopyAlongArgs,
    more: Option<BBox>,
    cell: &mut ArrayCell<'_>,
) -> Result<Vec<u64>, CoreError> {
    let circles = args.circle_diameter;
    if let Some(d) = circles {
        if !d.is_finite() || d <= 0.0 {
            return Err(CoreError::validation(
                "the circle diameter must be a positive number of millimeters",
            ));
        }
    } else if more.is_none() {
        require(doc, ids, "copy along a vector")?;
    }
    let tol = Tolerance::default();
    let mut guides = Vec::new();
    for id in args.guides() {
        let guide = doc
            .entity(id)?
            .path()
            .ok_or_else(|| CoreError::validation("the guide must be a vector"))?;
        let length = geom::path_length(&guide, tol);
        if length <= tol.get() {
            return Err(CoreError::validation("the guide vector has no length"));
        }
        let stations = copy_along_stations(length, geom::path_is_closed(&guide), args)?;
        guides.push((guide, length, stations));
    }
    let from = if circles.is_some() {
        Point::ORIGIN
    } else {
        [doc.bbox_of_many(ids), more]
            .into_iter()
            .flatten()
            .reduce(|a, b| a.union(b))
            .ok_or_else(|| CoreError::validation("the selection has no geometry to copy"))?
            .centre()
    };
    let layer = if args.new_layer {
        Some(doc.add_layer(Some(COPY_ALONG_LAYER.to_string()), None))
    } else {
        None
    };
    let mut copies = Vec::new();
    for (guide, length, stations) in &guides {
        for along in stations {
            let t = (along / length).clamp(0.0, 1.0);
            let t = if args.reverse { 1.0 - t } else { t };
            let Some(at) = geom::path_point_at(guide, t, tol) else {
                continue;
            };
            if let Some(d) = circles {
                let sheet = doc.active_sheet();
                let circle = geom::circle(at, d / 2.0)?;
                copies.push(doc.insert_entity(Entity {
                    rotation: 0.0,
                    id: 0,
                    layer: layer.unwrap_or_else(|| doc.active_layer()),
                    sheet,
                    revision: 0,
                    name: None,
                    parent: None,
                    kind: EntityKind::Path { path: circle },
                }));
                continue;
            }
            let turn = if args.align_to_curve {
                geom::path_tangent_at(guide, t, tol).map_or(Mat3::IDENTITY, |tangent| {
                    Mat3::rotate_about(tangent.angle(), from)
                })
            } else {
                Mat3::IDENTITY
            };
            let m = turn.then(&Mat3::translate(Vec2::new(at.x - from.x, at.y - from.y)));
            let mut made = Vec::with_capacity(ids.len());
            let mut map = BTreeMap::new();
            for id in ids {
                made.push(duplicate_mapped(doc, *id, &mut map)?);
            }
            doc.transform_entities(&made, &m);
            cell(doc, &m, &map)?;
            copies.extend(made);
        }
    }
    if let Some(layer) = layer {
        doc.set_entity_layer(&copies, layer)?;
    }
    if args.delete_original && circles.is_none() {
        doc.delete_entities(ids);
    }
    Ok(copies)
}

// ------------------------------------------------------- tidy and validate

/// Reduces the node count of vectors while staying inside `tolerance`.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn simplify(
    doc: &mut Document,
    ids: &[u64],
    tolerance: f64,
    keep_arcs: bool,
) -> Result<(), CoreError> {
    require(doc, ids, "simplify")?;
    let tol = Tolerance::new(tolerance).unwrap_or_default();
    for id in ids {
        for leaf in doc.leaves(*id) {
            let Some(path) = doc.entity(leaf)?.path() else {
                continue;
            };
            let source = if keep_arcs {
                path
            } else {
                geom::arcs_to_cubics(&path)
            };
            set_path(doc, leaf, reduce_nodes(&source, tol))?;
        }
    }
    Ok(())
}

/// Fit Curves to Vectors options besides the tolerance and the kind.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FitOptions {
    /// Keep Sharp Corners: corners where the direction turns by more than
    /// this many degrees stay sharp; `None` fits through every corner.
    pub sharp_angle: Option<f64>,
    /// Keep the originals and put the fitted copies on the active layer
    /// (Replace selected vectors off).
    pub keep_originals: bool,
}

/// Refits vectors as arcs, beziers or straight lines, in place.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an unknown kind or an empty
/// selection.
pub fn fit_curves(
    doc: &mut Document,
    ids: &[u64],
    tolerance: f64,
    kind: &str,
) -> Result<(), CoreError> {
    fit_curves_with(doc, ids, tolerance, kind, FitOptions::default()).map(|_| ())
}

/// Refits vectors as arcs, beziers or straight lines. Answers the ids of
/// the fitted vectors: the same ones, or new copies on the active layer
/// when the originals are kept.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an unknown kind or an empty
/// selection.
pub fn fit_curves_with(
    doc: &mut Document,
    ids: &[u64],
    tolerance: f64,
    kind: &str,
    options: FitOptions,
) -> Result<Vec<u64>, CoreError> {
    require(doc, ids, "refit")?;
    if !matches!(kind, "arcs" | "beziers" | "lines") {
        return Err(CoreError::validation(format!(
            "fit kind must be arcs, beziers or lines, got {kind}"
        )));
    }
    let tol = Tolerance::new(tolerance).unwrap_or_default();
    let layer = doc.active_layer();
    let mut out = Vec::new();
    for id in ids {
        for leaf in doc.leaves(*id) {
            let Some(path) = doc.entity(leaf)?.path() else {
                continue;
            };
            let mut next = Path::new();
            for sub in &path.subpaths {
                let polyline = geom::flatten_subpath(sub, tol);
                next.push(fit_polyline(&polyline, kind, tol, options.sharp_angle));
            }
            if options.keep_originals {
                let (_, sheet, name) = template(doc, leaf)?;
                out.push(add_like(doc, (layer, sheet, name), next));
            } else {
                set_path(doc, leaf, next)?;
                out.push(leaf);
            }
        }
    }
    if options.keep_originals {
        doc.set_selection(&out);
    }
    Ok(out)
}

/// Fits one run of points (no sharp corner inside it).
fn fit_run(points: &[Point], kind: &str, tol: Tolerance) -> SubPath {
    let polyline = geom::Polyline::new(points.to_vec(), false);
    match kind {
        // Runs of lines in one direction and arcs on one circle merge.
        "arcs" => geom::simplify::reduce_subpath(
            &geom::fit_arcs(&polyline, tol).unwrap_or_else(|| polyline_subpath(&polyline)),
            tol,
        ),
        "beziers" => geom::fit_cubics(points, tol).unwrap_or_else(|_| polyline_subpath(&polyline)),
        _ => polyline_subpath(&geom::simplify_polyline(&polyline, tol)),
    }
}

/// Indices of the points of `points` where the direction turns by more
/// than `angle` degrees. `closed` also looks at the first point.
fn sharp_corners(points: &[Point], closed: bool, angle: f64) -> Vec<usize> {
    let n = points.len();
    let limit = angle.to_radians();
    let mut out = Vec::new();
    for i in 0..n {
        let (before, after) = match (i, closed) {
            (0, false) => continue,
            (i, false) if i + 1 == n => continue,
            (0, true) => (points[n - 1], points[1 % n]),
            (i, _) => (points[i - 1], points[(i + 1) % n]),
        };
        let (Some(a), Some(b)) = (
            before.to(points[i]).normalised(),
            points[i].to(after).normalised(),
        ) else {
            continue;
        };
        if a.cross(b).atan2(a.dot(b)).abs() > limit {
            out.push(i);
        }
    }
    out
}

/// Fits a flattened subpath, kept sharp at its corners when asked.
fn fit_polyline(
    polyline: &geom::Polyline,
    kind: &str,
    tol: Tolerance,
    sharp: Option<f64>,
) -> SubPath {
    let points = &polyline.points;
    let corners = sharp
        .map(|angle| sharp_corners(points, polyline.closed, angle))
        .unwrap_or_default();
    if points.len() < 3 || corners.is_empty() {
        let mut fitted = if polyline.closed {
            let mut ring = points.clone();
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            let mut sub = fit_run(&ring, kind, tol);
            sub.closed = true;
            sub
        } else {
            fit_run(points, kind, tol)
        };
        fitted.closed = polyline.closed;
        return fitted;
    }
    // Runs from corner to corner, each fitted on its own, so the corners
    // stay nodes; a closed ring starts at its first corner.
    let mut order: Vec<Point> = Vec::with_capacity(points.len() + 1);
    let mut cuts: Vec<usize> = Vec::new();
    if polyline.closed {
        let start = corners[0];
        order.extend(points[start..].iter().copied());
        order.extend(points[..start].iter().copied());
        order.push(points[start]);
        cuts.extend(
            corners
                .iter()
                .map(|c| (c + points.len() - start) % points.len()),
        );
        cuts.push(order.len() - 1);
    } else {
        order.extend(points.iter().copied());
        cuts.push(0);
        cuts.extend(corners.iter().copied());
        cuts.push(points.len() - 1);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut sub = SubPath::open(order[cuts[0]]);
    for pair in cuts.windows(2) {
        let run = fit_run(&order[pair[0]..=pair[1]], kind, tol);
        sub.segments.extend(run.segments);
    }
    sub.closed = polyline.closed;
    sub
}

/// A polyline as a subpath of straight lines.
fn polyline_subpath(polyline: &geom::Polyline) -> SubPath {
    polyline
        .to_subpath()
        .unwrap_or_else(|| SubPath::open(*polyline.points.first().unwrap_or(&Point::ORIGIN)))
}

/// Checks vectors for the problems that stop a toolpath calculating.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn validate(doc: &Document, ids: &[u64]) -> Result<Vec<Issue>, CoreError> {
    validate_with(doc, ids, false)
}

/// Most places marked for one kind of crossing.
const MAX_MARKS: usize = 100;

/// [`validate`], marking where each problem is and adding the places where
/// different vectors cross. V-Groove Mode leaves out crossings that
/// involve text, which fonts often have and V-Groove copes with.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn validate_with(doc: &Document, ids: &[u64], vgroove: bool) -> Result<Vec<Issue>, CoreError> {
    require(doc, ids, "check")?;
    let tol = Tolerance::default();
    let mut leaves: Vec<(u64, Path)> = Vec::new();
    for id in ids {
        leaves.extend(doc.paths_of(*id));
    }
    let is_text = |id: u64| {
        doc.entity(id)
            .is_ok_and(|e| matches!(e.kind, EntityKind::Text(_)))
    };
    let paths: Vec<Path> = leaves.iter().map(|(_, p)| p.clone()).collect();
    let mut out = Vec::new();
    // Pairs that lie on top of each other are reported as duplicates only,
    // not as crossing everywhere their outlines touch.
    let mut duplicates: Vec<(usize, usize)> = Vec::new();
    for issue in geom::validate_set(&paths, tol) {
        if let Some(other) = issue.other_index {
            duplicates.push((other, issue.path_index));
        }
        let Some((id, path)) = leaves.get(issue.path_index) else {
            continue;
        };
        let sub = path.subpaths.get(issue.subpath_index);
        let marks = match (issue.kind, sub) {
            (geom::ValidationKind::Open, Some(sub)) => vec![sub.start, sub.end()],
            (geom::ValidationKind::SelfIntersecting, Some(sub)) => {
                if vgroove && is_text(*id) {
                    continue;
                }
                geom::subpath_self_crossings(sub, tol, MAX_MARKS)
            }
            _ => issue.point.into_iter().collect(),
        };
        // A duplicate names the vector it lies on by its id, as crossings do.
        let message = match issue.other_index.and_then(|i| leaves.get(i)) {
            Some((other, _)) => format!("this vector lies on top of vector {other}"),
            None => issue.message,
        };
        out.push(Issue {
            id: *id,
            kind: kind_name(issue.kind).to_string(),
            x: issue.point.or(marks.first().copied()).map(|p| p.x),
            y: issue.point.or(marks.first().copied()).map(|p| p.y),
            message,
            marks,
        });
    }
    // Different vectors crossing each other.
    let boxes: Vec<Option<BBox>> = paths.iter().map(path_bbox).collect();
    for i in 0..paths.len() {
        for j in i + 1..paths.len() {
            let (Some(a), Some(b)) = (boxes[i], boxes[j]) else {
                continue;
            };
            let (first, second) = (leaves[i].0, leaves[j].0);
            if !a.intersects(b)
                || duplicates.contains(&(i, j))
                || (vgroove && (is_text(first) || is_text(second)))
            {
                continue;
            }
            let mut marks: Vec<Point> = Vec::new();
            for hit in geom::path_intersections(&paths[i], &paths[j], tol) {
                if marks.len() >= MAX_MARKS {
                    break;
                }
                if !marks.iter().any(|p| p.approx_eq(hit.point, tol)) {
                    marks.push(hit.point);
                }
            }
            let Some(at) = marks.first().copied() else {
                continue;
            };
            out.push(Issue {
                id: first,
                kind: "intersection".to_string(),
                x: Some(at.x),
                y: Some(at.y),
                message: format!("this vector crosses vector {second}"),
                marks,
            });
        }
    }
    Ok(out)
}

/// Fix zero length Spans: takes out every span shorter than the
/// tolerance from the vectors. Answers how many went.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn fix_zero_length(doc: &mut Document, ids: &[u64]) -> Result<usize, CoreError> {
    require(doc, ids, "fix")?;
    let tol = Tolerance::default();
    let mut removed = 0;
    for id in ids {
        for leaf in doc.leaves(*id) {
            let Some(path) = doc.entity(leaf)?.path() else {
                continue;
            };
            let mut next = Path::new();
            let mut changed = false;
            for sub in &path.subpaths {
                let mut kept = SubPath::open(sub.start);
                for (start, seg) in sub.iter() {
                    if geom::segment_length(start, seg, tol) <= tol.get() && sub.segments.len() > 1
                    {
                        removed += 1;
                        changed = true;
                        continue;
                    }
                    kept.segments.push(*seg);
                }
                kept.closed = sub.closed;
                next.push(kept);
            }
            if changed {
                set_path(doc, leaf, next)?;
            }
        }
    }
    Ok(removed)
}

/// A place where the vectors of a selection cross, for the intersection
/// checks V-Groove, Pocket and Offset make before they calculate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Crossing {
    /// The vectors that cross there; one id when a vector crosses itself.
    pub ids: Vec<u64>,
    /// Where, in millimetres.
    pub x: f64,
    /// Where, in millimetres.
    pub y: f64,
}

/// Most crossings [`crossings`] reports; the checks only need to know there
/// are some and where the first ones are.
pub const MAX_CROSSINGS: usize = 100;

/// Where the selected vectors cross themselves or each other.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected.
pub fn crossings(doc: &Document, ids: &[u64]) -> Result<Vec<Crossing>, CoreError> {
    require(doc, ids, "check")?;
    let tol = Tolerance::default();
    let mut leaves: Vec<(u64, Path)> = Vec::new();
    for id in ids {
        leaves.extend(doc.paths_of(*id));
    }
    let paths: Vec<Path> = leaves.iter().map(|(_, p)| p.clone()).collect();
    let mut out = Vec::new();
    for issue in geom::validate_set(&paths, tol) {
        if issue.kind != geom::ValidationKind::SelfIntersecting {
            continue;
        }
        let Some((id, path)) = leaves.get(issue.path_index) else {
            continue;
        };
        // The self check does not say where; the middle of the vector does.
        if let Some(point) = issue.point.or_else(|| path_bbox(path).map(|b| b.centre())) {
            out.push(Crossing {
                ids: vec![*id],
                x: point.x,
                y: point.y,
            });
        }
    }
    let boxes: Vec<Option<BBox>> = paths.iter().map(path_bbox).collect();
    'pairs: for i in 0..paths.len() {
        for j in i + 1..paths.len() {
            let (Some(a), Some(b)) = (boxes[i], boxes[j]) else {
                continue;
            };
            if !a.intersects(b) {
                continue;
            }
            for hit in geom::path_intersections(&paths[i], &paths[j], tol) {
                if out.len() >= MAX_CROSSINGS {
                    break 'pairs;
                }
                let mut pair = vec![leaves[i].0, leaves[j].0];
                pair.dedup();
                out.push(Crossing {
                    ids: pair,
                    x: hit.point.x,
                    y: hit.point.y,
                });
            }
        }
    }
    out.truncate(MAX_CROSSINGS);
    Ok(out)
}

fn kind_name(kind: geom::ValidationKind) -> &'static str {
    match kind {
        geom::ValidationKind::Open => "open",
        geom::ValidationKind::SelfIntersecting => "selfIntersecting",
        geom::ValidationKind::ZeroLengthSegment => "zeroLengthSegment",
        geom::ValidationKind::DuplicateNode => "duplicateNode",
        geom::ValidationKind::OverlappingDuplicate => "overlappingDuplicate",
    }
}

// ------------------------------------------------------------ node editing

/// Adds a node at the point on a vector nearest a click.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the click is not near the vector.
pub fn node_insert(doc: &mut Document, id: u64, at: Point) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let tol = Tolerance::default();
    let near = nearest_point_on_path(&path, at, tol)
        .ok_or_else(|| CoreError::geometry("that point is not near the vector"))?;
    let updated = geom::insert_node(
        &path.subpaths[near.subpath_index],
        near.segment_index,
        near.t,
    )?;
    let mut next = path.clone();
    next.subpaths[near.subpath_index] = updated;
    set_path(doc, id, next)
}

/// Removes a node, joining the segments either side of it.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the indexes are out of range.
pub fn node_delete(
    doc: &mut Document,
    id: u64,
    subpath: usize,
    node: usize,
) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let sub = subpath_at(&path, subpath)?;
    let updated = geom::delete_node(sub, node)?;
    let mut next = path.clone();
    next.subpaths[subpath] = updated;
    set_path(doc, id, next)
}

/// Changes one segment into a line, an arc or a cubic.
///
/// # Errors
/// Returns [`CoreError::Validation`] for an unknown target shape.
pub fn node_convert(
    doc: &mut Document,
    id: u64,
    subpath: usize,
    segment: usize,
    to: &str,
) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let sub = subpath_at(&path, subpath)?;
    let kind = match to {
        "line" => SegmentKind::Line,
        "arc" => SegmentKind::Arc,
        "cubic" | "bezier" => SegmentKind::Cubic,
        other => {
            return Err(CoreError::validation(format!(
                "a segment can become a line, an arc or a cubic, not {other}"
            )))
        }
    };
    let updated = geom::convert_segment(sub, segment, kind)?;
    let mut next = path.clone();
    next.subpaths[subpath] = updated;
    set_path(doc, id, next)
}

/// Makes the curves either side of a node meet smoothly.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the node cannot be smoothed.
pub fn node_smooth(
    doc: &mut Document,
    id: u64,
    subpath: usize,
    node: usize,
) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let sub = subpath_at(&path, subpath)?;
    let updated = geom::smooth_node(sub, node)?;
    let mut next = path.clone();
    next.subpaths[subpath] = updated;
    set_path(doc, id, next)
}

/// Moves the start point of a closed vector to a node, which decides where a
/// profile toolpath plunges.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the vector is open.
pub fn node_set_start(
    doc: &mut Document,
    id: u64,
    subpath: usize,
    node: usize,
) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let sub = subpath_at(&path, subpath)?;
    let near = *sub
        .nodes()
        .get(node)
        .ok_or_else(|| CoreError::geometry(format!("no node {node}")))?;
    let updated = geom::set_start_node(sub, near, Tolerance::default())?;
    let mut next = path.clone();
    next.subpaths[subpath] = updated;
    set_path(doc, id, next)
}

/// Cuts a vector at a node: a closed vector opens there, an open one becomes
/// two.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the node is not on the subpath.
pub fn node_cut(doc: &mut Document, id: u64, subpath: usize, node: usize) -> Result<(), CoreError> {
    let path = path_of(doc, id)?;
    let sub = subpath_at(&path, subpath)?.clone();
    let nodes = sub.node_count();
    if node >= nodes {
        return Err(CoreError::geometry(format!("no node {node}")));
    }
    let pieces: Vec<SubPath> = if sub.closed {
        let opened = geom::set_start_node(
            &sub,
            *sub.nodes().get(node).unwrap_or(&sub.start),
            Tolerance::default(),
        )?;
        vec![SubPath::new(opened.start, opened.segments, false)]
    } else if node == 0 || node + 1 == nodes {
        vec![sub.clone()]
    } else {
        let first = SubPath::new(sub.start, sub.segments[..node].to_vec(), false);
        let second = SubPath::new(
            sub.segments[node - 1].end(),
            sub.segments[node..].to_vec(),
            false,
        );
        vec![first, second]
    };
    let mut next = Path::new();
    for (index, existing) in path.subpaths.iter().enumerate() {
        if index == subpath {
            for piece in &pieces {
                next.push(piece.clone());
            }
        } else {
            next.push(existing.clone());
        }
    }
    set_path(doc, id, next)
}

/// Reverses the direction a vector is drawn in, which flips climb and
/// conventional cutting on a profile toolpath.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the entity has no path.
pub fn reverse(doc: &mut Document, ids: &[u64]) -> Result<(), CoreError> {
    require(doc, ids, "reverse")?;
    for id in ids {
        for leaf in doc.leaves(*id) {
            let Some(path) = doc.entity(leaf)?.path() else {
                continue;
            };
            set_path(doc, leaf, geom::reverse(&path))?;
        }
    }
    Ok(())
}

/// The path of an entity, or an error naming the entity.
fn path_of(doc: &Document, id: u64) -> Result<Path, CoreError> {
    doc.entity(id)?
        .path()
        .ok_or_else(|| CoreError::validation(format!("entity {id} has no path to edit")))
}

/// One subpath by index, or an error.
fn subpath_at(path: &Path, index: usize) -> Result<&SubPath, CoreError> {
    path.subpaths
        .get(index)
        .ok_or_else(|| CoreError::geometry(format!("no subpath {index}")))
}

/// The box round a path, for callers that want one number.
pub fn bbox_of_path(path: &Path) -> Option<BBox> {
    path_bbox(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, rect, Segment};

    fn add(doc: &mut Document, path: Path) -> u64 {
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

    fn doc_with(paths: Vec<Path>) -> (Document, Vec<u64>) {
        let mut doc = Document::new();
        doc.begin("Create");
        let ids = paths.into_iter().map(|p| add(&mut doc, p)).collect();
        doc.commit();
        (doc, ids)
    }

    fn square(x: f64, y: f64, size: f64) -> Path {
        rect(Point::new(x, y), Point::new(x + size, y + size)).expect("rect")
    }

    #[test]
    fn moving_shifts_the_bounding_box() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Move");
        move_by(&mut doc, &ids, 5.0, -3.0).expect("moves");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!((bbox.min.x - 5.0).abs() < 1e-9 && (bbox.min.y + 3.0).abs() < 1e-9);
    }

    #[test]
    fn moving_to_an_anchor_puts_that_corner_on_the_point() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Move");
        move_to(&mut doc, &ids, 100.0, 50.0, Anchor::BottomLeft).expect("moves");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!((bbox.min.x - 100.0).abs() < 1e-9 && (bbox.min.y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn moving_nothing_is_refused() {
        let mut doc = Document::new();
        let error = move_by(&mut doc, &[], 1.0, 1.0).expect_err("nothing to move");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn rotating_a_quarter_turn_swaps_the_axes() {
        let (mut doc, ids) = doc_with(vec![
            rect(Point::new(0.0, 0.0), Point::new(20.0, 10.0)).expect("rect")
        ]);
        doc.begin("Rotate");
        rotate(&mut doc, &ids, 90.0, 0.0, 0.0).expect("rotates");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!((bbox.width() - 10.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.height() - 20.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn set_size_scales_about_the_anchor() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Size");
        set_size(&mut doc, &ids, Some(20.0), None, true, Anchor::BottomLeft).expect("resizes");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!((bbox.min.x).abs() < 1e-9, "{bbox:?}");
        assert!((bbox.width() - 20.0).abs() < 1e-9, "{bbox:?}");
        assert!((bbox.height() - 20.0).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn set_size_refuses_a_negative_size() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let error = set_size(&mut doc, &ids, Some(-5.0), None, false, Anchor::Centre)
            .expect_err("a negative size is refused");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn mirroring_in_place_keeps_the_box() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Mirror");
        let out = mirror(&mut doc, &ids, MirrorAxis::H, None, false).expect("mirrors");
        doc.commit();
        assert_eq!(out, ids);
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!((bbox.min.x).abs() < 1e-9 && (bbox.max.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn mirroring_a_copy_leaves_the_original_alone() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Mirror");
        let out = mirror(&mut doc, &ids, MirrorAxis::V, None, true).expect("mirrors");
        doc.commit();
        assert_eq!(out.len(), 1);
        assert_ne!(out[0], ids[0]);
        assert!(doc.entity(ids[0]).is_ok());
        assert_eq!(doc.order().len(), 2);
    }

    #[test]
    fn welding_two_overlapping_squares_leaves_one_vector() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(5.0, 0.0, 10.0)]);
        doc.begin("Weld");
        let out = boolean(&mut doc, &ids, Boolean::Weld).expect("welds");
        doc.commit();
        assert_eq!(out.len(), 1);
        assert!(doc.entity(ids[0]).is_err());
        let bbox = doc.bbox_of(out[0]).expect("bbox");
        assert!((bbox.width() - 15.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn welding_text_keeps_the_counters_of_its_letters() {
        use crate::geom::contain::{point_in_path, FillRule};
        use crate::geom::text::{layout_text, FontLibrary, TextSpec};
        // Two O's drawn in so far that they overlap, plus a bar across both.
        let spec = TextSpec {
            text: "OO".to_string(),
            size_mm: 20.0,
            char_spacing: -0.2,
            ..TextSpec::default()
        };
        let mut letters = layout_text(&FontLibrary::bundled(), &spec).expect("layout");
        assert_eq!(letters.len(), 2);
        let first = crate::geom::outlines_bbox(&letters[..1]).expect("bbox");
        let second = crate::geom::outlines_bbox(&letters[1..]).expect("bbox");
        assert!(second.min.x < first.max.x, "the letters overlap");
        let counters = [first.centre(), second.centre()];
        letters.push(
            rect(
                Point::new(first.min.x - 2.0, -1.0),
                Point::new(second.max.x + 2.0, 0.5),
            )
            .expect("rect"),
        );
        let (mut doc, ids) = doc_with(letters);
        doc.begin("Weld");
        let out = boolean(&mut doc, &ids, Boolean::Weld).expect("welds");
        doc.commit();
        assert_eq!(out.len(), 1, "one welded shape");
        let welded: Vec<Path> = doc.paths_of(out[0]).into_iter().map(|(_, p)| p).collect();
        let tol = Tolerance::default();
        let inside = |p: Point| {
            welded
                .iter()
                .any(|path| point_in_path(path, p, FillRule::EvenOdd, tol))
        };
        for centre in counters {
            assert!(!inside(centre), "the counter at {centre:?} stays open");
        }
        // The bar joined the letters, and the letters' strokes stay solid.
        assert!(inside(Point::new(first.min.x - 1.0, -0.25)));
        assert!(inside(Point::new(
            first.min.x + 0.8,
            (first.min.y + first.max.y) / 2.0
        )));
        let closed: usize = welded
            .iter()
            .flat_map(|p| p.subpaths.iter())
            .filter(|s| s.closed)
            .count();
        assert!(closed >= 3, "an outline and two counters, got {closed}");
    }

    #[test]
    fn subtracting_cuts_the_last_out_of_every_earlier_one() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 20.0),
            square(30.0, 0.0, 20.0),
            rect(Point::new(10.0, -5.0), Point::new(40.0, 25.0)).expect("rect"),
        ]);
        doc.begin("Subtract");
        let out = boolean(&mut doc, &ids, Boolean::Subtract).expect("subtracts");
        doc.commit();
        assert_eq!(out.len(), 2, "one result per earlier shape");
        assert!(ids.iter().all(|id| doc.entity(*id).is_err()));
        let left = doc.bbox_of(out[0]).expect("bbox");
        assert!((left.max.x - 10.0).abs() < 1e-6, "{left:?}");
        let right = doc.bbox_of(out[1]).expect("bbox");
        assert!((right.min.x - 40.0).abs() < 1e-6, "{right:?}");
    }

    #[test]
    fn overlapping_clips_every_earlier_one_to_the_last() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 20.0),
            square(30.0, 0.0, 20.0),
            rect(Point::new(10.0, -5.0), Point::new(40.0, 25.0)).expect("rect"),
        ]);
        doc.begin("Overlap");
        let out = boolean(&mut doc, &ids, Boolean::Intersect).expect("overlaps");
        doc.commit();
        assert_eq!(out.len(), 2);
        let left = doc.bbox_of(out[0]).expect("bbox");
        assert!((left.min.x - 10.0).abs() < 1e-6 && (left.max.x - 20.0).abs() < 1e-6);
        let right = doc.bbox_of(out[1]).expect("bbox");
        assert!((right.min.x - 30.0).abs() < 1e-6 && (right.max.x - 40.0).abs() < 1e-6);
    }

    #[test]
    fn trimming_objects_outside_keeps_the_boundary() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 20.0),
            open_line(Point::new(-10.0, 5.0), Point::new(30.0, 5.0)),
            square(10.0, -5.0, 30.0),
        ]);
        doc.begin("Trim Objects");
        let out = trim_objects(&mut doc, &ids, TrimSide::Outside, true).expect("trims");
        doc.commit();
        assert_eq!(out, vec![ids[0], ids[1]]);
        assert!(doc.entity(ids[2]).is_ok(), "the boundary stays");
        let area = doc.bbox_of(ids[0]).expect("bbox");
        assert!((area.min.x - 10.0).abs() < 1e-6, "{area:?}");
        let line = doc.bbox_of(ids[1]).expect("bbox");
        assert!((line.min.x - 10.0).abs() < 1e-6 && (line.max.x - 30.0).abs() < 1e-6);
    }

    #[test]
    fn trimming_objects_inside_clears_the_middle() {
        let (mut doc, ids) = doc_with(vec![
            open_line(Point::new(0.0, 5.0), Point::new(30.0, 5.0)),
            square(10.0, 0.0, 10.0),
        ]);
        doc.begin("Trim Objects");
        trim_objects(&mut doc, &ids, TrimSide::Inside, false).expect("trims");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert_eq!(path.subpaths.len(), 2, "the line is cut in two");
    }

    #[test]
    fn trimming_needs_a_closed_boundary_last() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 20.0),
            open_line(Point::new(-10.0, 5.0), Point::new(30.0, 5.0)),
        ]);
        let error =
            trim_objects(&mut doc, &ids, TrimSide::Outside, true).expect_err("open boundary");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn a_boolean_needs_two_vectors() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let error = boolean(&mut doc, &ids, Boolean::Weld).expect_err("one is not enough");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn offsetting_keeps_the_original_unless_asked() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Offset");
        let out = offset(
            &mut doc,
            &ids,
            2.0,
            OffsetSide::Outward,
            OffsetCorner::Round,
            false,
        )
        .expect("offsets");
        doc.commit();
        assert_eq!(out.len(), 1);
        assert!(doc.entity(ids[0]).is_ok());
        let bbox = doc.bbox_of(out[0]).expect("bbox");
        assert!((bbox.width() - 24.0).abs() < 1e-3, "{bbox:?}");
    }

    #[test]
    fn offsetting_can_delete_the_original() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Offset");
        offset(
            &mut doc,
            &ids,
            2.0,
            OffsetSide::Inward,
            OffsetCorner::Sharp,
            true,
        )
        .expect("offsets");
        doc.commit();
        assert!(doc.entity(ids[0]).is_err());
    }

    #[test]
    fn an_open_vector_offsets_to_a_closed_outline() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(50.0, 0.0));
        let (mut doc, ids) = doc_with(vec![open.into_path()]);
        doc.begin("Offset");
        let out = offset(
            &mut doc,
            &ids,
            3.0,
            OffsetSide::Outward,
            OffsetCorner::Round,
            false,
        )
        .expect("offsets");
        doc.commit();
        let path = doc.entity(out[0]).expect("result").path().expect("path");
        assert!(path.is_closed(), "an outline is closed");
    }

    #[test]
    fn joining_two_touching_open_vectors_makes_one() {
        let mut a = SubPath::open(Point::new(0.0, 0.0));
        a.line_to(Point::new(10.0, 0.0));
        let mut b = SubPath::open(Point::new(10.0, 0.0));
        b.line_to(Point::new(10.0, 10.0));
        let (mut doc, ids) = doc_with(vec![a.into_path(), b.into_path()]);
        doc.begin("Join");
        let out = join(&mut doc, &ids, 0.01, false).expect("joins");
        doc.commit();
        assert_eq!(out.len(), 1);
        let path = doc.entity(out[0]).expect("result").path().expect("path");
        assert_eq!(path.subpaths.len(), 1);
        assert_eq!(path.subpaths[0].segments.len(), 2);
    }

    #[test]
    fn closing_an_open_vector_adds_the_closing_segment() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let (mut doc, ids) = doc_with(vec![open.into_path()]);
        doc.begin("Close");
        close(&mut doc, &ids, "straight").expect("closes");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(path.is_closed());
    }

    fn open_line(a: Point, b: Point) -> Path {
        let mut sub = SubPath::open(a);
        sub.line_to(b);
        sub.into_path()
    }

    #[test]
    fn joining_two_open_vectors_bridges_the_closest_ends() {
        let (mut doc, ids) = doc_with(vec![
            open_line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            open_line(Point::new(30.0, 0.0), Point::new(15.0, 0.0)),
        ]);
        doc.begin("Close");
        let out = close(&mut doc, &ids, "straight").expect("joins");
        doc.commit();
        assert_eq!(out, vec![ids[0]]);
        assert!(doc.entity(ids[1]).is_err(), "the second vector is used up");
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert_eq!(path.subpaths.len(), 1);
        let sub = &path.subpaths[0];
        assert!(!sub.closed);
        assert!(sub.start.distance(Point::new(0.0, 0.0)) < 1e-9);
        assert!(sub.end().distance(Point::new(30.0, 0.0)) < 1e-9);
        assert_eq!(sub.segments.len(), 3, "the gap is bridged by a line");
    }

    #[test]
    fn moving_end_points_meets_halfway() {
        let (mut doc, ids) = doc_with(vec![
            open_line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            open_line(Point::new(14.0, 0.0), Point::new(30.0, 0.0)),
        ]);
        doc.begin("Close");
        close(&mut doc, &ids, "moveEnd").expect("joins");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        let sub = &path.subpaths[0];
        assert_eq!(sub.segments.len(), 2);
        let middle = sub.segments[0].end();
        assert!(middle.distance(Point::new(12.0, 0.0)) < 1e-9, "{middle:?}");
    }

    #[test]
    fn moving_end_points_closes_one_vector_at_the_midpoint() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .line_to(Point::new(0.0, 2.0));
        let (mut doc, ids) = doc_with(vec![open.into_path()]);
        doc.begin("Close");
        close(&mut doc, &ids, "moveEnd").expect("closes");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        let sub = &path.subpaths[0];
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3, "no segment is added");
        assert!(sub.start.distance(Point::new(0.0, 1.0)) < 1e-9);
    }

    #[test]
    fn a_smooth_join_adds_a_curve() {
        let (mut doc, ids) = doc_with(vec![
            open_line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            open_line(Point::new(20.0, 5.0), Point::new(30.0, 5.0)),
        ]);
        doc.begin("Close");
        close(&mut doc, &ids, "smooth").expect("joins");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        let sub = &path.subpaths[0];
        assert_eq!(sub.segments[1].kind(), SegmentKind::Cubic);
    }

    #[test]
    fn extending_ends_to_a_point_adds_straight_spans() {
        let (mut doc, ids) = doc_with(vec![
            open_line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            open_line(Point::new(20.0, 10.0), Point::new(20.0, 5.0)),
        ]);
        doc.begin("Extend");
        let ends = [
            OpenEnd {
                id: ids[0],
                subpath: 0,
                at_end: true,
            },
            OpenEnd {
                id: ids[1],
                subpath: 0,
                at_end: true,
            },
        ];
        extend_ends_to(&mut doc, &ends, Point::new(20.0, 0.0)).expect("extends");
        doc.commit();
        let first = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(first.subpaths[0].end().distance(Point::new(20.0, 0.0)) < 1e-9);
        let second = doc.entity(ids[1]).expect("entity").path().expect("path");
        assert_eq!(second.subpaths[0].segments.len(), 2);
    }

    #[test]
    fn extending_the_start_keeps_the_direction() {
        let (mut doc, ids) = doc_with(vec![open_line(Point::new(5.0, 0.0), Point::new(10.0, 0.0))]);
        let end = OpenEnd {
            id: ids[0],
            subpath: 0,
            at_end: false,
        };
        doc.begin("Extend");
        extend_ends_to(&mut doc, &[end], Point::new(0.0, 0.0)).expect("extends");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        let sub = &path.subpaths[0];
        assert!(sub.start.distance(Point::new(0.0, 0.0)) < 1e-9);
        assert!(sub.end().distance(Point::new(10.0, 0.0)) < 1e-9);
    }

    #[test]
    fn close_refuses_an_unknown_mode() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let error = close(&mut doc, &ids, "curly").expect_err("unknown mode");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn filleting_a_corner_rounds_it() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Fillet");
        fillet(
            &mut doc,
            ids[0],
            Point::new(0.0, 0.0),
            3.0,
            FilletKind::Normal,
        )
        .expect("fillets");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(
            path.subpaths[0]
                .segments
                .iter()
                .any(|s| matches!(s, Segment::Arc { .. })),
            "a fillet adds an arc"
        );
    }

    #[test]
    fn a_dog_bone_fillet_reaches_past_the_corner() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Fillet");
        fillet(
            &mut doc,
            ids[0],
            Point::new(0.0, 0.0),
            2.0,
            FilletKind::DogBone,
        )
        .expect("fillets");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!(bbox.min.x < -1e-6, "a dog bone reaches outside the corner");
    }

    #[test]
    fn clicking_a_fillet_takes_it_out_again() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Fillet");
        fillet(
            &mut doc,
            ids[0],
            Point::new(20.0, 0.0),
            3.0,
            FilletKind::Normal,
        )
        .expect("fillets");
        doc.commit();
        let arcs = |doc: &Document| {
            doc.entity(ids[0]).expect("e").path().expect("p").subpaths[0]
                .segments
                .iter()
                .filter(|s| matches!(s, geom::Segment::Arc { .. }))
                .count()
        };
        assert_eq!(arcs(&doc), 1);
        // A click on the middle of the arc, well away from its ends.
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let mid = Point::new(20.0 - 3.0 + 3.0 * h, 3.0 - 3.0 * h);
        let path = doc.entity(ids[0]).expect("e").path().expect("p");
        assert!(matches!(
            fillet_target(&path, mid),
            Some(FilletTarget::Remove { .. })
        ));
        doc.begin("Fillet");
        fillet(&mut doc, ids[0], mid, 3.0, FilletKind::Normal).expect("removes");
        doc.commit();
        assert_eq!(arcs(&doc), 0);
        let b = doc.bbox_of(ids[0]).expect("bbox");
        assert!((b.max.x - 20.0).abs() < 1e-9 && b.min.y.abs() < 1e-9);
    }

    #[test]
    fn a_t_bone_follows_the_wall_the_click_is_nearer() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
        doc.begin("Fillet");
        // Near the bottom wall of the corner at (20, 0), just inside.
        fillet(
            &mut doc,
            ids[0],
            Point::new(19.0, 0.1),
            2.0,
            FilletKind::TBone,
        )
        .expect("fillets");
        doc.commit();
        let b = doc.bbox_of(ids[0]).expect("bbox");
        assert!(b.min.y < -1.9 && (b.max.x - 20.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn aligning_puts_left_edges_flush() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
        doc.begin("Align");
        align(&mut doc, &ids, AlignMode::Left, AlignTarget::Selection).expect("aligns");
        doc.commit();
        let a = doc.bbox_of(ids[0]).expect("bbox");
        let b = doc.bbox_of(ids[1]).expect("bbox");
        assert!((a.min.x - b.min.x).abs() < 1e-9);
    }

    #[test]
    fn aligning_to_the_material_uses_the_job_bounds() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Align");
        align(&mut doc, &ids, AlignMode::Centre, AlignTarget::Material).expect("aligns");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        let job = doc.job_bounds();
        assert!((bbox.centre().x - job.centre().x).abs() < 1e-9);
        assert!((bbox.centre().y - job.centre().y).abs() < 1e-9);
    }

    #[test]
    fn centring_several_objects_in_the_material_moves_them_as_one() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(40.0, 20.0, 10.0)]);
        doc.begin("Align");
        align(&mut doc, &ids, AlignMode::Centre, AlignTarget::Material).expect("aligns");
        doc.commit();
        let a = doc.bbox_of(ids[0]).expect("bbox");
        let b = doc.bbox_of(ids[1]).expect("bbox");
        assert!((b.min.x - a.min.x - 40.0).abs() < 1e-9 && (b.min.y - a.min.y - 20.0).abs() < 1e-9);
        let both = doc.bbox_of_many(&ids).expect("bbox");
        let job = doc.job_bounds();
        assert!((both.centre().x - job.centre().x).abs() < 1e-9);
        assert!((both.centre().y - job.centre().y).abs() < 1e-9);
        doc.begin("Align");
        align(&mut doc, &ids, AlignMode::HCentre, AlignTarget::Material).expect("aligns");
        doc.commit();
        assert_eq!(doc.bbox_of(ids[1]), Some(b));
    }

    #[test]
    fn distributing_spreads_the_middle_object_evenly() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 10.0),
            square(15.0, 0.0, 10.0),
            square(100.0, 0.0, 10.0),
        ]);
        doc.begin("Distribute");
        distribute(&mut doc, &ids, DistributeMode::HGap, None).expect("distributes");
        doc.commit();
        let a = doc.bbox_of(ids[0]).expect("bbox");
        let b = doc.bbox_of(ids[1]).expect("bbox");
        let c = doc.bbox_of(ids[2]).expect("bbox");
        let gap1 = b.min.x - a.max.x;
        let gap2 = c.min.x - b.max.x;
        assert!((gap1 - gap2).abs() < 1e-6, "{gap1} vs {gap2}");
    }

    #[test]
    fn aligning_to_the_last_selected_leaves_it_in_place() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(30.0, 5.0, 10.0)]);
        doc.begin("Align");
        align(&mut doc, &ids, AlignMode::Bottom, AlignTarget::Last).expect("aligns");
        doc.commit();
        let a = doc.bbox_of(ids[0]).expect("bbox");
        let b = doc.bbox_of(ids[1]).expect("bbox");
        assert!((a.min.y - 5.0).abs() < 1e-9, "{a:?}");
        assert!((b.min.x - 30.0).abs() < 1e-9 && (b.min.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn aligning_outside_the_last_selected_puts_objects_against_its_edges() {
        let place = |mode| {
            let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(30.0, 40.0, 20.0)]);
            doc.begin("Align");
            align(&mut doc, &ids, mode, AlignTarget::Last).expect("aligns");
            doc.commit();
            let key = doc.bbox_of(ids[1]).expect("bbox");
            assert!((key.min.x - 30.0).abs() < 1e-9 && (key.min.y - 40.0).abs() < 1e-9);
            doc.bbox_of(ids[0]).expect("bbox")
        };
        let left = place(AlignMode::OutsideLeft);
        assert!(
            (left.max.x - 30.0).abs() < 1e-9 && left.min.y.abs() < 1e-9,
            "{left:?}"
        );
        let right = place(AlignMode::OutsideRight);
        assert!((right.min.x - 50.0).abs() < 1e-9 && right.min.y.abs() < 1e-9);
        let top = place(AlignMode::OutsideTop);
        assert!(
            (top.min.y - 60.0).abs() < 1e-9 && top.min.x.abs() < 1e-9,
            "{top:?}"
        );
        let bottom = place(AlignMode::OutsideBottom);
        assert!((bottom.max.y - 40.0).abs() < 1e-9 && bottom.min.x.abs() < 1e-9);
    }

    #[test]
    fn distributing_right_edges_spaces_them_evenly() {
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 10.0),
            square(12.0, 0.0, 30.0),
            square(100.0, 0.0, 20.0),
        ]);
        doc.begin("Distribute");
        distribute(&mut doc, &ids, DistributeMode::HRight, None).expect("distributes");
        doc.commit();
        let right: Vec<f64> = ids
            .iter()
            .map(|id| doc.bbox_of(*id).expect("bbox").max.x)
            .collect();
        assert!(
            (right[1] - right[0] - (right[2] - right[1])).abs() < 1e-9,
            "{right:?}"
        );
        assert!((right[0] - 10.0).abs() < 1e-9 && (right[2] - 120.0).abs() < 1e-9);
    }

    #[test]
    fn distributing_with_a_spacing_steps_from_the_first() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(50.0, 0.0, 10.0)]);
        doc.begin("Distribute");
        distribute(&mut doc, &ids, DistributeMode::HGap, Some(2.0)).expect("distributes");
        doc.commit();
        let b = doc.bbox_of(ids[1]).expect("bbox");
        assert!((b.min.x - 12.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn align_and_distribute_modes_use_the_names_the_view_sends() {
        let modes = [
            "left",
            "right",
            "top",
            "bottom",
            "hCentre",
            "vCentre",
            "centre",
            "outsideLeft",
            "outsideRight",
            "outsideTop",
            "outsideBottom",
        ];
        for mode in modes {
            serde_json::from_value::<AlignMode>(serde_json::json!(mode)).expect(mode);
        }
        for target in ["selection", "material", "last"] {
            serde_json::from_value::<AlignTarget>(serde_json::json!(target)).expect(target);
        }
        let spreads = [
            "hLeft", "hCentre", "hRight", "hGap", "vTop", "vCentre", "vBottom", "vGap",
        ];
        for mode in spreads {
            serde_json::from_value::<DistributeMode>(serde_json::json!(mode)).expect(mode);
        }
    }

    #[test]
    fn an_array_makes_the_right_number_of_copies() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Array");
        let out = array_copy(
            &mut doc,
            &ids,
            &ArrayArgs {
                rows: 2,
                cols: 3,
                gap_x: 5.0,
                gap_y: 5.0,
                mode: ArrayMode::Gap,
                stagger: Stagger::None,
                stagger_fraction: 0.5,
                group: false,
                symmetry: [BlockTurn::None; 4],
                row_shift: [0.0; 2],
                col_shift: [0.0; 2],
            },
        )
        .expect("arrays");
        doc.commit();
        assert_eq!(out.len(), 5);
        assert_eq!(doc.order().len(), 6);
        let last = doc.bbox_of(out[out.len() - 1]).expect("bbox");
        assert!((last.min.x - 30.0).abs() < 1e-9, "{last:?}");
        assert!((last.min.y - 15.0).abs() < 1e-9, "{last:?}");
    }

    #[test]
    fn a_staggered_array_shifts_every_other_row() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Array");
        let out = array_copy(
            &mut doc,
            &ids,
            &ArrayArgs {
                rows: 2,
                cols: 1,
                gap_x: 0.0,
                gap_y: 0.0,
                mode: ArrayMode::Gap,
                stagger: Stagger::Rows,
                stagger_fraction: 0.5,
                group: false,
                symmetry: [BlockTurn::None; 4],
                row_shift: [0.0; 2],
                col_shift: [0.0; 2],
            },
        )
        .expect("arrays");
        doc.commit();
        let bbox = doc.bbox_of(out[0]).expect("bbox");
        assert!((bbox.min.x - 5.0).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn block_symmetry_and_displacement_shape_the_array() {
        // An L so its mirrors can be told apart: the stub sits bottom right.
        let mut l = SubPath::open(Point::new(0.0, 0.0));
        l.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 4.0));
        let (mut doc, ids) = doc_with(vec![l.into_path()]);
        doc.begin("Array");
        let out = array_copy(
            &mut doc,
            &ids,
            &ArrayArgs {
                rows: 2,
                cols: 2,
                gap_x: 10.0,
                gap_y: 10.0,
                mode: ArrayMode::Gap,
                stagger: Stagger::None,
                stagger_fraction: 0.5,
                group: false,
                symmetry: [
                    BlockTurn::None,
                    BlockTurn::MirrorX,
                    BlockTurn::MirrorY,
                    BlockTurn::Rotate,
                ],
                row_shift: [3.0, 0.0],
                col_shift: [0.0, 2.0],
            },
        )
        .expect("arrays");
        doc.commit();
        let start = |id: u64| doc.entity(id).expect("e").path().expect("p").subpaths[0].start;
        // Row 0, column 1: mirrored left to right, one step (20) right and
        // the column displacement (2) up.
        assert!(
            start(out[0]).distance(Point::new(30.0, 2.0)) < 1e-9,
            "{:?}",
            start(out[0])
        );
        // Row 1, column 0: mirrored top to bottom, one step (14) up and the
        // row displacement (3) right.
        assert!(
            start(out[1]).distance(Point::new(3.0, 18.0)) < 1e-9,
            "{:?}",
            start(out[1])
        );
        // Row 1, column 1: turned half round, both displacements.
        assert!(
            start(out[2]).distance(Point::new(33.0, 20.0)) < 1e-9,
            "{:?}",
            start(out[2])
        );
    }

    #[test]
    fn a_circular_array_puts_copies_round_the_centre() {
        let (mut doc, ids) = doc_with(vec![square(20.0, -5.0, 10.0)]);
        doc.begin("Circular Array");
        let out = circular_array(
            &mut doc,
            &ids,
            &CircularArrayArgs {
                cx: 0.0,
                cy: 0.0,
                count: 4,
                angle: 360.0,
                rotate_copies: true,
                radius: None,
                group: false,
                step: None,
            },
        )
        .expect("arrays");
        doc.commit();
        assert_eq!(out.len(), 3);
        let quarter = doc.bbox_of(out[0]).expect("bbox");
        assert!(quarter.centre().y > 20.0, "{quarter:?}");
    }

    #[test]
    fn a_circular_array_takes_a_step_angle() {
        let (mut doc, ids) = doc_with(vec![square(20.0, -5.0, 10.0)]);
        doc.begin("Circular Array");
        let out = circular_array(
            &mut doc,
            &ids,
            &CircularArrayArgs {
                cx: 0.0,
                cy: 0.0,
                count: 3,
                angle: 360.0,
                rotate_copies: true,
                radius: None,
                group: false,
                step: Some(-90.0),
            },
        )
        .expect("arrays");
        doc.commit();
        assert_eq!(out.len(), 2);
        // A quarter turn clockwise puts the first copy below the centre.
        let first = doc.bbox_of(out[0]).expect("bbox");
        assert!(first.centre().y < -20.0, "{first:?}");
        let second = doc.bbox_of(out[1]).expect("bbox");
        assert!(second.centre().x < -20.0, "{second:?}");
    }

    #[test]
    fn a_circular_array_needs_two_objects() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let error = circular_array(
            &mut doc,
            &ids,
            &CircularArrayArgs {
                cx: 0.0,
                cy: 0.0,
                count: 1,
                angle: 360.0,
                rotate_copies: true,
                radius: None,
                group: false,
                step: None,
            },
        )
        .expect_err("one copy is not an array");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn copying_along_a_line_spaces_the_copies_out() {
        let mut guide = SubPath::open(Point::new(0.0, 0.0));
        guide.line_to(Point::new(100.0, 0.0));
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 4.0), guide.into_path()]);
        doc.begin("Copy Along");
        let out = copy_along(
            &mut doc,
            &ids[..1],
            &CopyAlongArgs {
                path_id: ids[1],
                path_ids: Vec::new(),
                distribute_mode: CopyAlongMode::Spacing,
                count: 0,
                spacing: 25.0,
                force_even: false,
                circle_diameter: None,
                new_layer: false,
                align_to_curve: false,
                start_offset: 0.0,
                reverse: false,
                delete_original: false,
            },
        )
        .expect("copies");
        doc.commit();
        assert_eq!(out.len(), 5);
        let first = doc.bbox_of(out[0]).expect("bbox").centre();
        let second = doc.bbox_of(out[1]).expect("bbox").centre();
        assert!(
            (second.x - first.x - 25.0).abs() < 1e-6,
            "{first:?} {second:?}"
        );
    }

    #[test]
    fn copy_along_follows_every_curve_evenly_and_can_place_circles() {
        let mut a = SubPath::open(Point::new(0.0, 0.0));
        a.line_to(Point::new(100.0, 0.0));
        let mut b = SubPath::open(Point::new(0.0, 50.0));
        b.line_to(Point::new(90.0, 50.0));
        let (mut doc, ids) = doc_with(vec![
            square(0.0, 0.0, 4.0),
            a.into_path(),
            b.into_path(),
            square(200.0, 0.0, 40.0),
        ]);
        let base = CopyAlongArgs {
            path_id: 0,
            path_ids: vec![ids[1], ids[2]],
            distribute_mode: CopyAlongMode::Spacing,
            count: 0,
            spacing: 45.0,
            force_even: true,
            circle_diameter: None,
            new_layer: true,
            align_to_curve: false,
            start_offset: 0.0,
            reverse: false,
            delete_original: false,
        };
        doc.begin("Copy Along");
        let out = copy_along(&mut doc, &ids[..1], &base).expect("copies");
        doc.commit();
        // 100 / 45 rounds to 2 parts (3 copies at 0, 50, 100); 90 / 45 is 2
        // parts (0, 45, 90): both ends of both curves are hit.
        assert_eq!(out.len(), 6);
        let xs: Vec<f64> = out
            .iter()
            .map(|id| doc.bbox_of(*id).expect("box").centre().x)
            .collect();
        for (x, want) in xs.iter().zip([0.0, 50.0, 100.0, 0.0, 45.0, 90.0]) {
            assert!((x - want).abs() < 1e-6, "{xs:?}");
        }
        let layer = doc.entity(out[0]).expect("copy").layer;
        assert_eq!(doc.layer(layer).expect("layer").name, COPY_ALONG_LAYER);

        // Copy Circles along the closed square: four spread round the loop.
        doc.begin("Copy Along");
        let circles = copy_along(
            &mut doc,
            &[],
            &CopyAlongArgs {
                path_ids: vec![ids[3]],
                distribute_mode: CopyAlongMode::Count,
                count: 4,
                circle_diameter: Some(6.0),
                new_layer: false,
                ..base.clone()
            },
        )
        .expect("circles");
        doc.commit();
        assert_eq!(circles.len(), 4);
        let b = doc.bbox_of(circles[1]).expect("box");
        assert!((b.width() - 6.0).abs() < 1e-6);
        let centres: Vec<Point> = circles
            .iter()
            .map(|id| doc.bbox_of(*id).expect("box").centre())
            .collect();
        for (i, p) in centres.iter().enumerate() {
            for q in &centres[i + 1..] {
                assert!(
                    p.distance(*q) > 1.0,
                    "no two copies on one spot: {centres:?}"
                );
            }
        }
    }

    #[test]
    fn validating_reports_an_open_vector() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(10.0, 0.0));
        let (doc, ids) = doc_with(vec![open.into_path()]);
        let issues = validate(&doc, &ids).expect("validates");
        assert!(issues.iter().any(|i| i.kind == "open"), "{issues:?}");
        assert_eq!(issues[0].id, ids[0]);
    }

    #[test]
    fn validating_a_clean_square_finds_nothing() {
        let (doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        assert!(validate(&doc, &ids).expect("validates").is_empty());
    }

    #[test]
    fn simplifying_a_flattened_circle_drops_nodes() {
        let circle = circle(Point::new(0.0, 0.0), 50.0).expect("circle");
        let polyline = geom::flatten_subpath(&circle.subpaths[0], Tolerance::default());
        let dense = Path::from_subpath(polyline.to_subpath().expect("subpath"));
        let before = dense.segment_count();
        let (mut doc, ids) = doc_with(vec![dense]);
        doc.begin("Simplify");
        simplify(&mut doc, &ids, 0.5, false).expect("simplifies");
        doc.commit();
        let after = doc
            .entity(ids[0])
            .expect("entity")
            .path()
            .expect("path")
            .segment_count();
        assert!(after < before, "{after} is not fewer than {before}");
    }

    #[test]
    fn fitting_arcs_turns_a_polygon_back_into_arcs() {
        let circle = circle(Point::new(0.0, 0.0), 30.0).expect("circle");
        let polyline =
            geom::flatten_subpath(&circle.subpaths[0], Tolerance::new(0.01).expect("tol"));
        let dense = Path::from_subpath(polyline.to_subpath().expect("subpath"));
        let (mut doc, ids) = doc_with(vec![dense]);
        doc.begin("Fit");
        fit_curves(&mut doc, &ids, 0.05, "arcs").expect("fits");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(
            path.subpaths[0]
                .segments
                .iter()
                .any(|s| matches!(s, Segment::Arc { .. })),
            "arcs came back"
        );
    }

    #[test]
    fn fitting_keeps_sharp_corners_and_can_keep_the_originals() {
        // A square flattened densely: fitted with Beziers through its
        // corners they round off; kept sharp they stay nodes.
        let dense = |doc_square: Path| {
            let tol = Tolerance::new(0.5).expect("tol");
            let mut sub = SubPath::open(doc_square.subpaths[0].start);
            let poly = geom::flatten_subpath(&doc_square.subpaths[0], tol);
            for w in poly.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                for k in 1..=10 {
                    sub.line_to(a.lerp(b, k as f64 / 10.0));
                }
            }
            sub.closed = true;
            Path::from_subpath(sub)
        };
        let (mut doc, ids) = doc_with(vec![dense(square(0.0, 0.0, 20.0))]);
        let corner_hit = |doc: &Document, id: u64| {
            let path = doc.entity(id).expect("e").path().expect("p");
            path.subpaths[0]
                .nodes()
                .iter()
                .any(|p| p.distance(Point::new(20.0, 20.0)) < 1e-6)
        };
        doc.begin("Fit");
        let options = FitOptions {
            sharp_angle: Some(20.0),
            keep_originals: true,
        };
        let made = fit_curves_with(&mut doc, &ids, 0.1, "beziers", options).expect("fits");
        doc.commit();
        assert_eq!(made.len(), 1);
        assert_ne!(made[0], ids[0], "a new vector");
        assert!(doc.entity(ids[0]).is_ok(), "the original stays");
        assert!(corner_hit(&doc, made[0]), "the corner stays a node");
        assert!(
            doc.entity(made[0])
                .expect("e")
                .path()
                .expect("p")
                .segment_count()
                < doc
                    .entity(ids[0])
                    .expect("e")
                    .path()
                    .expect("p")
                    .segment_count()
        );
        assert_eq!(doc.selection(), made.as_slice());
    }

    #[test]
    fn fitting_arcs_to_a_dense_corner_gives_two_lines() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        for i in 1..=40 {
            sub.line_to(Point::new(i as f64 * 2.5, 0.0));
        }
        for i in 1..=40 {
            sub.line_to(Point::new(100.0, i as f64 * 2.5));
        }
        let (mut doc, ids) = doc_with(vec![Path::from_subpath(sub)]);
        doc.begin("Fit");
        let options = FitOptions {
            sharp_angle: Some(20.0),
            keep_originals: false,
        };
        fit_curves_with(&mut doc, &ids, 0.1, "arcs", options).expect("fits");
        doc.commit();
        let path = doc.entity(ids[0]).expect("e").path().expect("p");
        assert_eq!(path.subpaths[0].segments.len(), 2, "{:?}", path.subpaths[0]);
    }

    #[test]
    fn the_validator_names_a_duplicate_by_its_id_and_not_as_a_crossing() {
        let (doc, ids) = doc_with(vec![
            square(0.0, 0.0, 10.0),
            square(40.0, 0.0, 10.0),
            square(40.0, 0.0, 10.0),
        ]);
        let issues = validate_with(&doc, &ids, false).expect("validates");
        let dup: Vec<_> = issues
            .iter()
            .filter(|i| i.kind == "overlappingDuplicate")
            .collect();
        assert_eq!(dup.len(), 1, "{issues:?}");
        assert_eq!(dup[0].id, ids[2]);
        assert_eq!(
            dup[0].message,
            format!("this vector lies on top of vector {}", ids[1])
        );
        assert!(
            issues.iter().all(|i| i.kind != "intersection"),
            "{issues:?}"
        );
    }

    #[test]
    fn the_validator_marks_ends_crossings_and_fixes_zero_spans() {
        // An open L, a bow tie and two squares that overlap.
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let mut bow = SubPath::open(Point::new(20.0, 0.0));
        bow.line_to(Point::new(30.0, 10.0))
            .line_to(Point::new(30.0, 0.0))
            .line_to(Point::new(20.0, 10.0));
        bow.closed = true;
        let (mut doc, ids) = doc_with(vec![
            Path::from_subpath(open),
            Path::from_subpath(bow),
            square(40.0, 0.0, 10.0),
            square(45.0, 5.0, 10.0),
        ]);
        let issues = validate_with(&doc, &ids, false).expect("validates");
        let of = |kind: &str| issues.iter().filter(|i| i.kind == kind).collect::<Vec<_>>();
        let open = of("open");
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].marks.len(), 2, "both ends are marked");
        let crossing: Vec<_> = of("selfIntersecting")
            .into_iter()
            .filter(|i| i.id == ids[1])
            .collect();
        assert_eq!(crossing.len(), 1);
        assert!(crossing[0].marks[0].distance(Point::new(25.0, 5.0)) < 1e-6);
        let between = of("intersection");
        assert_eq!(between.len(), 1);
        assert_eq!(between[0].marks.len(), 2, "two squares cross twice");
        assert_eq!(of("zeroLengthSegment").len(), 1);

        doc.begin("Fix");
        assert_eq!(fix_zero_length(&mut doc, &ids).expect("fixes"), 1);
        doc.commit();
        let again = validate_with(&doc, &ids, false).expect("validates");
        assert!(again.iter().all(|i| i.kind != "zeroLengthSegment"));
    }

    #[test]
    fn fit_curves_refuses_an_unknown_kind() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let error = fit_curves(&mut doc, &ids, 0.1, "splines").expect_err("unknown kind");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn inserting_a_node_adds_one_segment() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let before = doc
            .entity(ids[0])
            .expect("entity")
            .path()
            .expect("path")
            .segment_count();
        doc.begin("Insert Node");
        node_insert(&mut doc, ids[0], Point::new(5.0, 0.0)).expect("inserts");
        doc.commit();
        let after = doc
            .entity(ids[0])
            .expect("entity")
            .path()
            .expect("path")
            .segment_count();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn deleting_a_node_removes_one_segment() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        let before = doc
            .entity(ids[0])
            .expect("entity")
            .path()
            .expect("path")
            .segment_count();
        doc.begin("Delete Node");
        node_delete(&mut doc, ids[0], 0, 1).expect("deletes");
        doc.commit();
        let after = doc
            .entity(ids[0])
            .expect("entity")
            .path()
            .expect("path")
            .segment_count();
        assert_eq!(after, before - 1);
    }

    #[test]
    fn setting_the_start_node_moves_the_start_point() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Set Start");
        node_set_start(&mut doc, ids[0], 0, 2).expect("sets start");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(
            (path.subpaths[0].start.x - 10.0).abs() < 1e-9,
            "{:?}",
            path.subpaths[0].start
        );
    }

    #[test]
    fn cutting_a_closed_vector_at_a_node_opens_it() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Cut");
        node_cut(&mut doc, ids[0], 0, 2).expect("cuts");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(!path.subpaths[0].closed);
    }

    #[test]
    fn converting_a_segment_to_a_cubic_changes_its_kind() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0)]);
        doc.begin("Convert");
        node_convert(&mut doc, ids[0], 0, 0, "cubic").expect("converts");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!(matches!(
            path.subpaths[0].segments[0],
            Segment::Cubic { .. }
        ));
    }

    #[test]
    fn reversing_flips_the_direction() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(10.0, 0.0));
        let (mut doc, ids) = doc_with(vec![open.into_path()]);
        doc.begin("Reverse");
        reverse(&mut doc, &ids).expect("reverses");
        doc.commit();
        let path = doc.entity(ids[0]).expect("entity").path().expect("path");
        assert!((path.subpaths[0].start.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_boundary_wraps_the_selection() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(20.0, 0.0, 10.0)]);
        doc.begin("Boundary");
        let out = boundary(&mut doc, &ids, 2.0, BoundaryKind::Selection, false).expect("builds");
        doc.commit();
        assert!(!out.is_empty());
        let bbox = doc.bbox_of_many(&out).expect("bbox");
        assert!(bbox.min.x < -1.0 && bbox.max.x > 31.0, "{bbox:?}");
    }

    #[test]
    fn a_rubber_band_boundary_is_one_hull_round_everything() {
        // Two squares apart and a triangle above: the band spans them all.
        let tri = geom::polyline_path(
            &[
                Point::new(10.0, 20.0),
                Point::new(20.0, 20.0),
                Point::new(15.0, 30.0),
            ],
            true,
        )
        .expect("triangle");
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(20.0, 0.0, 10.0), tri]);
        doc.begin("Boundary");
        let out = boundary(&mut doc, &ids, 0.0, BoundaryKind::Selection, true).expect("builds");
        doc.commit();
        assert_eq!(out.len(), 1);
        let path = doc.entity(out[0]).expect("hull").path().expect("path");
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        // Corners of the hull: (0,0), (30,0), (30,10), (15,30), (0,10).
        assert_eq!(path.subpaths[0].node_count(), 5);
        let bbox = doc.bbox_of(out[0]).expect("bbox");
        assert!((bbox.max.y - 30.0).abs() < 1e-9 && (bbox.max.x - 30.0).abs() < 1e-9);
        // Offset grows the band.
        doc.begin("Boundary");
        let grown = boundary(&mut doc, &ids, 2.0, BoundaryKind::Selection, true).expect("builds");
        doc.commit();
        let bbox = doc.bbox_of(grown[0]).expect("bbox");
        assert!((bbox.min.x + 2.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn duplicating_a_group_copies_its_members() {
        let (mut doc, ids) = doc_with(vec![square(0.0, 0.0, 10.0), square(20.0, 0.0, 10.0)]);
        doc.begin("Group");
        let group = doc.group(&ids).expect("group");
        doc.commit();
        doc.begin("Duplicate");
        let copy = duplicate(&mut doc, group).expect("duplicates");
        doc.commit();
        assert_ne!(copy, group);
        assert_eq!(doc.leaves(copy).len(), 2);
        assert_eq!(doc.order().len(), 2);
    }

    #[test]
    fn trimming_cuts_back_to_the_crossing() {
        let mut across = SubPath::open(Point::new(-5.0, 5.0));
        across.line_to(Point::new(25.0, 5.0));
        let (mut doc, ids) = doc_with(vec![across.into_path(), square(0.0, 0.0, 10.0)]);
        doc.begin("Trim");
        trim(&mut doc, ids[0], Point::new(-4.0, 5.0)).expect("trims");
        doc.commit();
        let bbox = doc.bbox_of(ids[0]).expect("bbox");
        assert!(bbox.min.x >= -1e-6, "{bbox:?}");
    }
}
