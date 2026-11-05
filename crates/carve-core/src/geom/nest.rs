//! True shape nesting: packs closed parts onto one or more rectangular
//! sheets without overlaps, keeping a spacing between parts and a border
//! along the sheet edges.
//!
//! Every part is reduced to a footprint: the outer outlines of its closed
//! vectors grown by half the spacing, flattened so that the polygon always
//! encloses the true grown outline. Holes are filled in. With
//! [`NestParams::nest_in_holes`] every hole of a placed part also becomes a
//! region of its own: the hole outline shrunk by half the spacing, flattened
//! so that the polygon always lies inside it, with any islands in the hole as
//! obstacles. Later parts try the holes on a sheet before its open area, so
//! small parts drop into the inner loops of larger ones and still keep the
//! spacing from the hole's edge. Parts are placed one
//! copy at a time, largest first, with a bottom left fill: for every allowed
//! orientation and a set of candidate X positions the part is dropped to the
//! lowest Y where its footprint clears every footprint already on the sheet.
//! The drop is exact for the polygons, found by sweeping the edge pairs. The
//! best position across orientations wins (lowest top edge, then lowest
//! right edge). A copy that fits on no open sheet opens the next sheet,
//! up to [`NestParams::max_sheets`].
//!
//! Filler parts ([`NestPart::filler`]) are placed after every other part:
//! the largest filler is copied until no more fit on the sheets already in
//! use, then the next, so they fill the leftover space. Fillers never open a
//! new sheet (one sheet is used when only fillers are given) and stop after
//! [`FILLER_LIMIT`] copies in total.
//!
//! The start corner and fill direction are handled by solving in a frame
//! where the start corner is the bottom left and rows run along X, then
//! mapping the result back. Output is deterministic: the same input gives
//! the same placements, bit for bit.

use std::collections::HashMap;

use rayon::prelude::*;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::bbox::BBox;
use super::edit::reverse_subpath;
use super::flatten::flatten_segment_into;
use super::metrics::{path_bbox, subpath_signed_area};
use super::offset::{offset_path, OffsetCorner, OffsetSide};
use super::path::{ArcGeom, Path, Segment, SubPath};
use super::point::{Mat3, Point, Tolerance, Vec2};
use super::transform::transform_path;

/// Widening applied to every forbidden drop interval, in millimetres, so
/// rounding never lets two outlines cross.
const INTERVAL_EPS: f64 = 1e-9;
/// How far above a touching position a part is dropped, in millimetres.
const CLEAR_EPS: f64 = 4e-9;
/// How far a part may reach past the border because of [`CLEAR_EPS`]
/// steps, in millimetres.
const SHEET_EPS: f64 = 1e-7;
/// Evenly spaced candidate X positions tried across the sheet, on top of
/// the positions next to parts already placed.
const GRID_STEPS: usize = 32;
/// Coarsest chord tolerance used for footprints, in millimetres.
const MIN_FOOTPRINT_TOL: f64 = 0.025;
/// Most filler copies placed in one run, over every filler part.
pub const FILLER_LIMIT: usize = 2000;

/// The sheet corner the first part goes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NestCorner {
    /// Bottom left corner (the default).
    #[default]
    BottomLeft,
    /// Bottom right corner.
    BottomRight,
    /// Top left corner.
    TopLeft,
    /// Top right corner.
    TopRight,
}

/// Which way the sheet fills from the start corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NestDirection {
    /// Rows along X, working away from the start corner in Y (the default).
    #[default]
    X,
    /// Columns along Y, working away from the start corner in X.
    Y,
}

/// Settings for [`nest`]. Lengths are millimetres, angles degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NestParams {
    /// Diameter of the cutter that will cut the parts out. It is added to
    /// `gap` for the spacing between parts. Default 6.35.
    pub tool_diameter: f64,
    /// Extra clearance between parts on top of the tool diameter. Default 1.
    pub gap: f64,
    /// Border Gap: distance from each sheet edge to any part outline on
    /// top of the Clearance (`gap`). Default 10.
    pub border: f64,
    /// Rotation step in degrees; 0 keeps parts as drawn, 90 tries the four
    /// quarter turns, 45 tries eight. Otherwise 1 to 360. Default 90.
    pub rotation_step_deg: f64,
    /// Also try every orientation mirrored left to right. Default false.
    pub allow_mirror: bool,
    /// Sheet size along X. Default 1220.
    pub sheet_width: f64,
    /// Sheet size along Y. Default 2440.
    pub sheet_height: f64,
    /// Corner the first part goes into. Default bottom left.
    pub start_corner: NestCorner,
    /// Fill direction from the start corner. Default X.
    pub direction: NestDirection,
    /// Most sheets to use, at least 1. Default 100.
    pub max_sheets: usize,
    /// Let parts go inside the holes of parts already placed, keeping the
    /// spacing from the hole's edge. Default false.
    pub nest_in_holes: bool,
}

impl Default for NestParams {
    fn default() -> Self {
        NestParams {
            tool_diameter: 6.35,
            gap: 1.0,
            border: 10.0,
            rotation_step_deg: 90.0,
            allow_mirror: false,
            sheet_width: 1220.0,
            sheet_height: 2440.0,
            start_corner: NestCorner::BottomLeft,
            direction: NestDirection::X,
            max_sheets: 100,
            nest_in_holes: false,
        }
    }
}

fn default_quantity() -> usize {
    1
}

/// One part to nest: its closed vectors and how many copies to place.
///
/// Open subpaths are ignored. Holes are treated as solid unless
/// [`NestParams::nest_in_holes`] is on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestPart {
    /// Caller's id for the part, echoed in the result.
    pub id: u64,
    /// The part's vectors in job coordinates, millimetres.
    pub paths: Vec<Path>,
    /// Number of copies to place. Default 1; 0 places none. Ignored for a
    /// filler part, which is copied as often as it fits.
    #[serde(default = "default_quantity")]
    pub quantity: usize,
    /// A filler part fills the space left after the other parts are placed.
    /// Default false.
    #[serde(default)]
    pub filler: bool,
    /// Keep this part unmirrored even when mirroring is allowed, for a part
    /// carrying a relief. Default false.
    #[serde(default)]
    pub no_mirror: bool,
}

/// Where one copy of a part went.
///
/// A point `p` of the part (job coordinates) lands at
/// `R(rotation_deg) * (mirrored ? (-p.x, p.y) : p) + translation` in the
/// sheet's own coordinates, whose origin is the sheet's bottom left corner.
/// `matrix` is the same transform as a [`Mat3`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestPlacement {
    /// The part's id.
    pub part_id: u64,
    /// Which copy of the part this is, from 0.
    pub copy: usize,
    /// Sheet index, from 0.
    pub sheet: usize,
    /// Counter clockwise rotation in degrees, 0 up to but not including 360.
    pub rotation_deg: f64,
    /// True when the part is mirrored left to right before rotating.
    pub mirrored: bool,
    /// Translation after the rotation, in millimetres.
    pub translation: Vec2,
    /// The whole transform from job to sheet coordinates.
    pub matrix: Mat3,
}

/// Why a copy of a part was not placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NestUnplacedReason {
    /// The part has no closed vector with an area.
    NoClosedOutline,
    /// No allowed orientation fits inside an empty sheet's border.
    TooLarge,
    /// Every allowed sheet is full.
    OutOfSheets,
}

/// A copy of a part that could not be placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestUnplaced {
    /// The part's id.
    pub part_id: u64,
    /// Which copy of the part this is, from 0.
    pub copy: usize,
    /// Why it was left out.
    pub reason: NestUnplacedReason,
}

/// The result of [`nest`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestResult {
    /// Every placed copy, ordered by part in input order, then copy.
    pub placements: Vec<NestPlacement>,
    /// Every copy left out, in the same order.
    pub unplaced: Vec<NestUnplaced>,
    /// Number of sheets used.
    pub sheet_count: usize,
}

/// A closed polygon with its edges indexed by X for range queries.
#[derive(Debug, Clone)]
struct Poly {
    pts: Vec<Point>,
    bbox: BBox,
    /// `(min x, max x, edge index)` sorted by min x. Edge `i` runs from
    /// `pts[i]` to `pts[i + 1]`, wrapping.
    edges: Vec<(f64, f64, usize)>,
    /// Widest edge along X.
    max_w: f64,
}

impl Poly {
    fn new(pts: Vec<Point>) -> Option<Poly> {
        if pts.len() < 3 || pts.iter().any(|p| !p.is_finite()) {
            return None;
        }
        let bbox = BBox::of_points(pts.iter().copied())?;
        let n = pts.len();
        let mut edges = Vec::with_capacity(n);
        let mut max_w: f64 = 0.0;
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let (x0, x1) = min_max(a.x, b.x);
            max_w = max_w.max(x1 - x0);
            edges.push((x0, x1, i));
        }
        edges.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.2.cmp(&q.2)));
        Some(Poly {
            pts,
            bbox,
            edges,
            max_w,
        })
    }

    fn edge(&self, i: usize) -> (Point, Point) {
        (self.pts[i], self.pts[(i + 1) % self.pts.len()])
    }

    fn translated(&self, by: Vec2) -> Poly {
        let pts: Vec<Point> = self.pts.iter().map(|p| p.offset(by)).collect();
        let edges = self
            .edges
            .iter()
            .map(|&(x0, x1, i)| (x0 + by.x, x1 + by.x, i))
            .collect();
        Poly {
            pts,
            bbox: BBox::new(self.bbox.min.offset(by), self.bbox.max.offset(by)),
            edges,
            max_w: self.max_w,
        }
    }

    fn mapped(&self, m: &Mat3) -> Option<Poly> {
        Poly::new(self.pts.iter().map(|&p| m.apply(p)).collect())
    }

    /// Calls `f` with every edge whose X range meets `[x0, x1]`.
    fn edges_in(&self, x0: f64, x1: f64, mut f: impl FnMut(Point, Point)) {
        let from = x0 - self.max_w - INTERVAL_EPS;
        let start = self.edges.partition_point(|e| e.0 < from);
        for &(ex0, ex1, i) in &self.edges[start..] {
            if ex0 > x1 + INTERVAL_EPS {
                break;
            }
            if ex1 >= x0 - INTERVAL_EPS {
                let (a, b) = self.edge(i);
                f(a, b);
            }
        }
    }

    fn area(&self) -> f64 {
        let n = self.pts.len();
        let mut twice = 0.0;
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            twice += a.x * b.y - b.x * a.y;
        }
        0.5 * twice.abs()
    }
}

/// One allowed orientation of a part, in the solving frame.
#[derive(Debug, Clone)]
struct Oriented {
    /// Linear part of the map from job coordinates into the solving frame,
    /// already followed by the shift that puts the part's bounding box
    /// corner on the origin.
    map: Mat3,
    /// Part bounding box size after the map.
    size: Vec2,
    /// Footprint after the map.
    polys: Vec<Poly>,
    /// Bounding box of the footprint.
    bbox: BBox,
    /// Holes other parts may go into, after the map. Empty unless nesting
    /// into holes.
    holes: Vec<Hole>,
}

/// A hole of a part: the space a nested part's footprint must stay inside,
/// and the footprints of islands standing in it.
#[derive(Debug, Clone)]
struct Hole {
    container: Poly,
    islands: Vec<Poly>,
}

impl Hole {
    fn mapped(&self, m: &Mat3) -> Option<Hole> {
        Some(Hole {
            container: self.container.mapped(m)?,
            islands: self.islands.iter().filter_map(|p| p.mapped(m)).collect(),
        })
    }
}

/// A part ready to place: every orientation, or why it cannot be placed.
struct Prepared {
    orients: Vec<Oriented>,
    area: f64,
    empty: bool,
}

/// Somewhere parts go: the open sheet, or a hole of a placed part.
#[derive(Debug, Clone)]
struct Region {
    /// For a hole, a polygon inside the hole outline shrunk by half the
    /// spacing; `None` for the open sheet, which [`Space`] bounds instead.
    container: Option<Poly>,
    /// Footprints already in the region.
    occupants: Vec<Poly>,
    /// Lowest Y each part orientation could still take at an X offset, keyed
    /// by part and orientation, then by the offset's bits. Occupants only
    /// ever get added, so a part can never go lower than it could before
    /// and later searches start from here.
    lows: HashMap<(usize, usize), Lows>,
}

impl Region {
    fn new(container: Option<Poly>, occupants: Vec<Poly>) -> Region {
        Region {
            container,
            occupants,
            lows: HashMap::new(),
        }
    }
}

/// Lowest Y still open at each X offset (by its bits) for one part
/// orientation in one region.
type Lows = HashMap<u64, f64>;

/// The regions of one sheet; the first is the open sheet.
type SheetState = Vec<Region>;

fn open_sheet(boundary: Option<&Poly>) -> SheetState {
    vec![Region::new(boundary.cloned(), Vec::new())]
}

/// Packs `parts` onto sheets of `params.sheet_width` by
/// `params.sheet_height` millimetres.
///
/// Parts keep at least `tool_diameter + gap` millimetres apart and at least
/// `border` millimetres from every sheet edge (within 1e-7 mm). Curved
/// outlines are flattened conservatively at `tol` (no coarser than 0.025 mm),
/// so curves never come closer than the spacing; Bezier outlines may sit up
/// to that tolerance further apart than asked.
///
/// # Errors
/// Returns [`CoreError::Validation`] when a sheet size is not positive and
/// finite, the tool diameter, gap or border is negative or not finite, the
/// border leaves no room on the sheet, the rotation step is not 0 or 1 to
/// 360 degrees, or `max_sheets` is 0. Returns [`CoreError::Geometry`] when
/// offsetting an outline fails.
pub fn nest(
    parts: &[NestPart],
    params: &NestParams,
    tol: Tolerance,
) -> Result<NestResult, CoreError> {
    nest_within(parts, params, None, tol)
}

/// [`nest`] inside a boundary vector instead of the whole sheet: every part
/// keeps the Border Gap plus the Clearance inside `boundary`, a closed
/// vector in sheet millimetres (0,0 to the sheet size, which should be its
/// box).
///
/// # Errors
/// As [`nest`], and [`CoreError::Validation`] for a boundary with no closed
/// outline.
pub fn nest_within(
    parts: &[NestPart],
    params: &NestParams,
    boundary: Option<&Path>,
    tol: Tolerance,
) -> Result<NestResult, CoreError> {
    validate(params)?;
    let ftol = Tolerance::new(tol.get().max(MIN_FOOTPRINT_TOL))?;
    let half_spacing = 0.5 * (params.tool_diameter + params.gap);
    let (frame, sheet_w, sheet_h) = solving_frame(params);
    let back = frame.invert()?;
    let fence = match boundary {
        Some(path) => Some(boundary_container(
            path,
            params.border + params.gap - half_spacing,
            ftol,
            &frame,
        )?),
        None => None,
    };
    let angles = rotation_angles(params.rotation_step_deg);
    let mirrors: &[bool] = if params.allow_mirror {
        &[false, true]
    } else {
        &[false]
    };

    let mut prepared = Vec::with_capacity(parts.len());
    for part in parts {
        prepared.push(prepare(
            part,
            half_spacing,
            ftol,
            &frame,
            &angles,
            if part.no_mirror { &[false] } else { mirrors },
            params.nest_in_holes,
        )?);
    }

    let mut order: Vec<(usize, usize)> = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        if part.filler {
            continue;
        }
        for copy in 0..part.quantity {
            order.push((pi, copy));
        }
    }
    order.sort_by(|a, b| {
        prepared[b.0]
            .area
            .total_cmp(&prepared[a.0].area)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    // The Border Gap adds to the Clearance at the sheet edges.
    let edge = params.border + params.gap;
    let space = Space {
        lo: edge,
        hi_x: sheet_w - edge,
        hi_y: sheet_h - edge,
    };
    let holes = params.nest_in_holes;
    let mut sheets: Vec<SheetState> = Vec::new();
    let mut placed: Vec<(usize, usize, NestPlacement)> = Vec::new();
    let mut unplaced: Vec<(usize, usize, NestUnplaced)> = Vec::new();
    let unplaced_entry = |pi: usize, copy: usize, reason: NestUnplacedReason| {
        (
            pi,
            copy,
            NestUnplaced {
                part_id: parts[pi].id,
                copy,
                reason,
            },
        )
    };

    // Fillers count as one more step, however many copies they take.
    let steps = order.len() + 1;
    for (step, (pi, copy)) in order.into_iter().enumerate() {
        crate::progress::check()?;
        crate::progress::report_steps(step, steps);
        let part = &prepared[pi];
        if let Some(reason) = cannot_place(part, &space) {
            unplaced.push(unplaced_entry(pi, copy, reason));
            continue;
        }
        match try_place(
            &mut sheets,
            pi,
            part,
            (&space, fence.as_ref()),
            holes,
            params.max_sheets,
        ) {
            Some((sheet, m)) => {
                let matrix = m.then(&back);
                placed.push((pi, copy, placement(parts[pi].id, copy, sheet, matrix)));
            }
            None => unplaced.push(unplaced_entry(pi, copy, NestUnplacedReason::OutOfSheets)),
        }
    }

    let mut fillers: Vec<usize> = (0..parts.len()).filter(|&pi| parts[pi].filler).collect();
    fillers.sort_by(|&a, &b| {
        prepared[b]
            .area
            .total_cmp(&prepared[a].area)
            .then(a.cmp(&b))
    });
    while sheets.last().is_some_and(|s| s[0].occupants.is_empty()) {
        sheets.pop();
    }
    if !fillers.is_empty() && sheets.is_empty() {
        sheets.push(open_sheet(fence.as_ref()));
    }
    let open = sheets.len();
    let mut filled = 0;
    for pi in fillers {
        let part = &prepared[pi];
        if let Some(reason) = cannot_place(part, &space) {
            unplaced.push(unplaced_entry(pi, 0, reason));
            continue;
        }
        let mut copy = 0;
        while filled < FILLER_LIMIT {
            crate::progress::check()?;
            let Some((sheet, m)) =
                try_place(&mut sheets, pi, part, (&space, fence.as_ref()), holes, open)
            else {
                break;
            };
            let matrix = m.then(&back);
            placed.push((pi, copy, placement(parts[pi].id, copy, sheet, matrix)));
            copy += 1;
            filled += 1;
        }
        if copy == 0 {
            unplaced.push(unplaced_entry(pi, 0, NestUnplacedReason::OutOfSheets));
        }
    }

    while sheets.last().is_some_and(|s| s[0].occupants.is_empty()) {
        sheets.pop();
    }
    placed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    unplaced.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(NestResult {
        placements: placed.into_iter().map(|p| p.2).collect(),
        unplaced: unplaced.into_iter().map(|u| u.2).collect(),
        sheet_count: sheets.len(),
    })
}

/// Why a part can never be placed, if it cannot.
fn cannot_place(part: &Prepared, space: &Space) -> Option<NestUnplacedReason> {
    if part.empty {
        Some(NestUnplacedReason::NoClosedOutline)
    } else if !part.orients.iter().any(|o| space.fits(o.size)) {
        Some(NestUnplacedReason::TooLarge)
    } else {
        None
    }
}

/// Places one copy on the first of up to `limit` sheets it fits on,
/// opening sheets as needed, and returns the sheet and the map from job
/// coordinates into the solving frame.
fn try_place(
    sheets: &mut Vec<SheetState>,
    pi: usize,
    part: &Prepared,
    (space, fence): (&Space, Option<&Poly>),
    holes: bool,
    limit: usize,
) -> Option<(usize, Mat3)> {
    let mut sheet = 0;
    while sheet < limit {
        if sheet == sheets.len() {
            sheets.push(open_sheet(fence));
        }
        if let Some((ri, oi, pos)) = place_on(&mut sheets[sheet], pi, &part.orients, space, holes) {
            let o = &part.orients[oi];
            let shift = Vec2::new(pos.x, pos.y);
            let regions = &mut sheets[sheet];
            for poly in &o.polys {
                regions[ri].occupants.push(poly.translated(shift));
            }
            for hole in &o.holes {
                regions.push(Region::new(
                    Some(hole.container.translated(shift)),
                    hole.islands.iter().map(|p| p.translated(shift)).collect(),
                ));
            }
            return Some((sheet, o.map.then(&Mat3::translate(shift))));
        }
        if sheets[sheet][0].occupants.is_empty() {
            return None;
        }
        sheet += 1;
    }
    None
}

/// Region index, orientation index and translation for a part on one
/// sheet: the first hole it fits in when nesting into holes, else the
/// open sheet.
fn place_on(
    sheet: &mut [Region],
    pi: usize,
    orients: &[Oriented],
    space: &Space,
    holes: bool,
) -> Option<(usize, usize, Point)> {
    if holes {
        for (ri, region) in sheet.iter_mut().enumerate().skip(1) {
            if let Some((oi, pos)) = best_in(region, pi, orients, space) {
                return Some((ri, oi, pos));
            }
        }
    }
    best_in(&mut sheet[0], pi, orients, space).map(|(oi, pos)| (0, oi, pos))
}

fn validate(p: &NestParams) -> Result<(), CoreError> {
    let positive = |v: f64| v.is_finite() && v > 0.0;
    let non_negative = |v: f64| v.is_finite() && v >= 0.0;
    if !positive(p.sheet_width) || !positive(p.sheet_height) {
        return Err(CoreError::validation(
            "nesting needs a sheet width and height above zero",
        ));
    }
    if !non_negative(p.tool_diameter) || !non_negative(p.gap) || !non_negative(p.border) {
        return Err(CoreError::validation(
            "nesting tool diameter, gap and border must be zero or more",
        ));
    }
    if 2.0 * (p.border + p.gap) >= p.sheet_width.min(p.sheet_height) {
        return Err(CoreError::validation(
            "the nesting border leaves no room on the sheet",
        ));
    }
    let step = p.rotation_step_deg;
    if !(step == 0.0 || (step.is_finite() && (1.0..=360.0).contains(&step))) {
        return Err(CoreError::validation(
            "nesting rotation step must be 0 or between 1 and 360 degrees",
        ));
    }
    if p.max_sheets == 0 {
        return Err(CoreError::validation("nesting needs at least one sheet"));
    }
    Ok(())
}

/// The map from sheet coordinates into the solving frame, where the start
/// corner is the origin and rows run along X, plus the frame's sheet size.
fn solving_frame(p: &NestParams) -> (Mat3, f64, f64) {
    let (w, h) = (p.sheet_width, p.sheet_height);
    let mut m = Mat3::IDENTITY;
    if matches!(
        p.start_corner,
        NestCorner::BottomRight | NestCorner::TopRight
    ) {
        m = m.then(&Mat3::new(-1.0, 0.0, 0.0, 1.0, w, 0.0));
    }
    if matches!(p.start_corner, NestCorner::TopLeft | NestCorner::TopRight) {
        m = m.then(&Mat3::new(1.0, 0.0, 0.0, -1.0, 0.0, h));
    }
    match p.direction {
        NestDirection::X => (m, w, h),
        NestDirection::Y => (m.then(&Mat3::new(0.0, 1.0, 1.0, 0.0, 0.0, 0.0)), h, w),
    }
}

/// Rotation angles in degrees for a step, 0 first.
fn rotation_angles(step: f64) -> Vec<f64> {
    if step == 0.0 {
        return vec![0.0];
    }
    let mut out = Vec::new();
    let mut k = 0.0;
    while k * step < 360.0 - 1e-9 {
        out.push(k * step);
        k += 1.0;
    }
    out
}

/// Exact rotation for quarter turns, so rectangles stay axis aligned.
fn rotation(deg: f64) -> Mat3 {
    let quarter = deg / 90.0;
    if (quarter - quarter.round()).abs() < 1e-12 {
        let (s, c) = match (quarter.round() as i64).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
        return Mat3::new(c, s, -s, c, 0.0, 0.0);
    }
    Mat3::rotate(deg.to_radians())
}

fn prepare(
    part: &NestPart,
    half_spacing: f64,
    ftol: Tolerance,
    frame: &Mat3,
    angles: &[f64],
    mirrors: &[bool],
    with_holes: bool,
) -> Result<Prepared, CoreError> {
    let footprint = footprint(&part.paths, half_spacing, ftol)?;
    let holes = if with_holes {
        part_holes(&part.paths, half_spacing, ftol)?
    } else {
        Vec::new()
    };
    if footprint.is_empty() {
        return Ok(Prepared {
            orients: Vec::new(),
            area: 0.0,
            empty: true,
        });
    }
    let area = footprint.iter().map(Poly::area).sum();
    let closed: Vec<Path> = part
        .paths
        .iter()
        .map(|p| Path::from_subpaths(p.subpaths.iter().filter(|s| s.closed).cloned().collect()))
        .collect();
    let mut orients = Vec::new();
    for &mirror in mirrors {
        for &deg in angles {
            let mut lin = *frame;
            if mirror {
                lin = lin.then(&Mat3::scale(-1.0, 1.0));
            }
            lin = lin.then(&rotation(deg));
            let mut bbox: Option<BBox> = None;
            for path in &closed {
                if let Some(b) = path_bbox(&transform_path(path, &lin)) {
                    bbox = Some(bbox.map_or(b, |a| a.union(b)));
                }
            }
            let Some(bbox) = bbox else {
                continue;
            };
            let map = lin.then(&Mat3::translate(Vec2::new(-bbox.min.x, -bbox.min.y)));
            let polys: Vec<Poly> = footprint.iter().filter_map(|p| p.mapped(&map)).collect();
            let Some(fb) = polys
                .iter()
                .map(|p| p.bbox)
                .reduce(|a: BBox, b: BBox| a.union(b))
            else {
                continue;
            };
            let oriented = Oriented {
                map,
                size: bbox.size(),
                polys,
                bbox: fb,
                holes: holes.iter().filter_map(|h| h.mapped(&map)).collect(),
            };
            // A symmetric part looks the same in several orientations;
            // trying each again only costs time.
            if !orients.iter().any(|o| same_shape(o, &oriented)) {
                orients.push(oriented);
            }
        }
    }
    Ok(Prepared {
        empty: orients.is_empty(),
        orients,
        area,
    })
}

/// True when two orientations have the same footprint and holes: the same
/// vertices in the same cyclic order, within 1e-9 mm.
fn same_shape(a: &Oriented, b: &Oriented) -> bool {
    fn near(u: Point, v: Point) -> bool {
        (u.x - v.x).abs() <= 1e-9 && (u.y - v.y).abs() <= 1e-9
    }
    fn same(p: &Poly, q: &Poly) -> bool {
        let n = p.pts.len();
        if n != q.pts.len() {
            return false;
        }
        (0..n)
            .any(|k| near(p.pts[0], q.pts[k]) && (0..n).all(|i| near(p.pts[i], q.pts[(i + k) % n])))
    }
    (a.size.x - b.size.x).abs() <= 1e-9
        && (a.size.y - b.size.y).abs() <= 1e-9
        && a.polys.len() == b.polys.len()
        && a.holes.len() == b.holes.len()
        && a.polys.iter().zip(&b.polys).all(|(p, q)| same(p, q))
        && a.holes
            .iter()
            .zip(&b.holes)
            .all(|(p, q)| same(&p.container, &q.container) && p.islands.len() == q.islands.len())
}

/// The part's closed outlines grown by `half_spacing`, as polygons that
/// enclose the grown outlines, with loops lying inside another loop dropped.
fn footprint(paths: &[Path], half_spacing: f64, ftol: Tolerance) -> Result<Vec<Poly>, CoreError> {
    let margin = footprint_margin(paths, half_spacing, ftol);
    let mut polys = Vec::new();
    for sub in ccw_loops(paths, ftol) {
        polys.extend(grown(&sub, margin, ftol)?);
    }
    let mut keep = vec![true; polys.len()];
    for i in 0..polys.len() {
        for j in 0..polys.len() {
            if i == j || !keep[j] || !keep[i] {
                continue;
            }
            if polys[j].bbox.contains_bbox(polys[i].bbox)
                && (polys[i].bbox != polys[j].bbox || i > j)
                && poly_inside(&polys[i], &polys[j])
            {
                keep[i] = false;
            }
        }
    }
    Ok(polys
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect())
}

/// Half the spacing, plus the chord tolerance when Bezier curves are
/// flattened (their chords may fall on either side of the curve).
fn footprint_margin(paths: &[Path], half_spacing: f64, ftol: Tolerance) -> f64 {
    let has_cubic = paths.iter().any(|p| {
        p.subpaths.iter().any(|s| {
            s.closed
                && s.segments
                    .iter()
                    .any(|g| matches!(g, Segment::Cubic { .. }))
        })
    });
    half_spacing + if has_cubic { ftol.get() } else { 0.0 }
}

/// Every closed loop with an area, counter clockwise, closing edge explicit.
fn ccw_loops(paths: &[Path], ftol: Tolerance) -> Vec<SubPath> {
    let min_area = ftol.get() * ftol.get();
    let mut out = Vec::new();
    for path in paths {
        for sub in &path.subpaths {
            if !sub.closed || sub.segments.is_empty() {
                continue;
            }
            let sub = normalised_loop(sub);
            let area = subpath_signed_area(&sub);
            if !area.is_finite() || area.abs() <= min_area {
                continue;
            }
            out.push(if area < 0.0 {
                reverse_subpath(&sub)
            } else {
                sub
            });
        }
    }
    out
}

/// A counter clockwise loop grown by `margin`, as polygons enclosing it.
fn grown(sub: &SubPath, margin: f64, ftol: Tolerance) -> Result<Vec<Poly>, CoreError> {
    let min_area = ftol.get() * ftol.get();
    let loops: Vec<SubPath> = if margin > 0.0 {
        offset_path(
            &Path::from_subpath(sub.clone()),
            margin,
            OffsetSide::Outward,
            OffsetCorner::Round,
            ftol,
        )?
        .into_iter()
        .flat_map(|p| p.subpaths)
        .filter(|s| subpath_signed_area(s) > min_area)
        .collect()
    } else {
        vec![sub.clone()]
    };
    Ok(loops
        .iter()
        .filter_map(|l| Poly::new(enclosing_points(l, ftol)))
        .collect())
}

/// The holes of a part, each shrunk by the footprint margin into polygons
/// that lie inside the shrunk hole, with the grown islands standing in it.
///
/// A loop is a hole when an odd number of the part's other loops enclose
/// it; the loops one level deeper inside it are its islands.
fn part_holes(paths: &[Path], half_spacing: f64, ftol: Tolerance) -> Result<Vec<Hole>, CoreError> {
    let margin = footprint_margin(paths, half_spacing, ftol);
    let min_area = ftol.get() * ftol.get();
    let loops = ccw_loops(paths, ftol);
    if loops.len() < 2 {
        return Ok(Vec::new());
    }
    let raw: Vec<Option<Poly>> = loops
        .iter()
        .map(|l| Poly::new(enclosing_points(l, ftol)))
        .collect();
    let inside = |i: usize, j: usize| match (&raw[i], &raw[j]) {
        (Some(a), Some(b)) => i != j && b.bbox.contains_bbox(a.bbox) && poly_inside(a, b),
        _ => false,
    };
    let depth: Vec<usize> = (0..loops.len())
        .map(|i| (0..loops.len()).filter(|&j| inside(i, j)).count())
        .collect();
    let mut out = Vec::new();
    for (h, sub) in loops.iter().enumerate() {
        if depth[h] % 2 == 0 {
            continue;
        }
        let mut islands = Vec::new();
        for (k, island) in loops.iter().enumerate() {
            if depth[k] == depth[h] + 1 && inside(k, h) {
                islands.extend(grown(island, margin, ftol)?);
            }
        }
        let shrunk: Vec<SubPath> = if margin > 0.0 {
            offset_path(
                &Path::from_subpath(sub.clone()),
                margin,
                OffsetSide::Inward,
                OffsetCorner::Round,
                ftol,
            )?
            .into_iter()
            .flat_map(|p| p.subpaths)
            .filter(|s| subpath_signed_area(s) > min_area)
            .collect()
        } else {
            vec![sub.clone()]
        };
        for l in &shrunk {
            // Clockwise, the enclosing flattening grows the outside of the
            // loop, so the polygon stays inside the hole.
            if let Some(container) = Poly::new(enclosing_points(&reverse_subpath(l), ftol)) {
                out.push(Hole {
                    container,
                    islands: islands.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// The polygon a boundary vector leaves for part footprints: its largest
/// loop moved `inset` millimetres inward (outward when negative), flattened
/// so it stays inside, in the solving frame.
fn boundary_container(
    boundary: &Path,
    inset: f64,
    ftol: Tolerance,
    frame: &Mat3,
) -> Result<Poly, CoreError> {
    let no_room = || CoreError::validation("the nest boundary leaves no room for parts");
    let largest = |subs: Vec<SubPath>| {
        subs.into_iter().max_by(|a, b| {
            subpath_signed_area(a)
                .abs()
                .total_cmp(&subpath_signed_area(b).abs())
        })
    };
    let outline = largest(ccw_loops(std::slice::from_ref(boundary), ftol))
        .ok_or_else(|| CoreError::validation("the nest boundary must be a closed vector"))?;
    let moved = if inset.abs() > 1e-9 {
        let side = if inset > 0.0 {
            OffsetSide::Inward
        } else {
            OffsetSide::Outward
        };
        let offset = offset_path(
            &Path::from_subpath(outline),
            inset.abs(),
            side,
            OffsetCorner::Round,
            ftol,
        )?;
        largest(offset.into_iter().flat_map(|p| p.subpaths).collect()).ok_or_else(no_room)?
    } else {
        outline
    };
    Poly::new(enclosing_points(&reverse_subpath(&moved), ftol))
        .and_then(|p| p.mapped(frame))
        .ok_or_else(no_room)
}

/// A closed loop with an explicit closing segment back to its start.
///
/// Closed subpaths may leave the closing edge implicit; the offset and
/// flattening code expects it spelled out, so it is added here.
fn normalised_loop(sub: &SubPath) -> SubPath {
    let mut out = sub.clone();
    if !out.end().approx_eq(out.start, Tolerance::EQUALITY) {
        out.segments.push(Segment::line(out.start));
    }
    out
}

/// True when polygon `a` lies inside polygon `b` with no boundary contact.
fn poly_inside(a: &Poly, b: &Poly) -> bool {
    a.pts.iter().all(|&p| point_in_poly(p, b)) && !boundaries_touch(a, b)
}

/// Flattens a loop so the polygon encloses the area on the loop's left
/// (its inside when counter clockwise, its outside when clockwise): counter
/// clockwise arcs use tangent lines outside the arc, clockwise arcs use
/// chords (which lie on the arc's left there).
fn enclosing_points(sub: &SubPath, ftol: Tolerance) -> Vec<Point> {
    let mut pts = vec![sub.start];
    for (start, seg) in sub.iter() {
        match *seg {
            Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
                Some(arc) => {
                    let r = arc.radius;
                    let step = 2.0 * (r / (r + ftol.get())).clamp(-1.0, 1.0).acos();
                    let n = if step > 1e-9 {
                        ((arc.sweep.abs() / step).ceil() as usize).clamp(1, 4096)
                    } else {
                        1
                    };
                    let d = arc.sweep / n as f64;
                    if arc.is_ccw() {
                        let reach = r / (0.5 * d).cos();
                        for k in 0..n {
                            let a = arc.start_angle + (k as f64 + 0.5) * d;
                            pts.push(Point::new(
                                arc.centre.x + reach * a.cos(),
                                arc.centre.y + reach * a.sin(),
                            ));
                        }
                    } else {
                        for k in 1..n {
                            pts.push(arc.point_at(k as f64 / n as f64));
                        }
                    }
                    pts.push(to);
                }
                None => pts.push(to),
            },
            _ => flatten_segment_into(start, seg, ftol, &mut pts),
        }
    }
    if pts.len() > 1 && pts[0].approx_eq(pts[pts.len() - 1], Tolerance::EQUALITY) {
        pts.pop();
    }
    pts.dedup_by(|a, b| a.approx_eq(*b, Tolerance::EQUALITY));
    pts
}

/// The usable area of a sheet in the solving frame.
struct Space {
    lo: f64,
    hi_x: f64,
    hi_y: f64,
}

impl Space {
    fn fits(&self, size: Vec2) -> bool {
        self.lo + size.x <= self.hi_x + SHEET_EPS && self.lo + size.y <= self.hi_y + SHEET_EPS
    }
}

/// Placed polygons ordered by the left edge of their bounding boxes, for
/// range queries along X.
struct XIndex<'a> {
    polys: Vec<&'a Poly>,
    /// Widest bounding box along X.
    max_w: f64,
}

impl<'a> XIndex<'a> {
    fn new(placed: &'a [Poly]) -> XIndex<'a> {
        let mut polys: Vec<&Poly> = placed.iter().collect();
        polys.sort_by(|a, b| a.bbox.min.x.total_cmp(&b.bbox.min.x));
        let max_w = polys
            .iter()
            .map(|p| p.bbox.max.x - p.bbox.min.x)
            .fold(0.0, f64::max);
        XIndex { polys, max_w }
    }

    /// Polygons whose bounding box meets `[x0, x1]` widened by
    /// [`INTERVAL_EPS`], in placing order among equal left edges.
    fn in_x(&self, x0: f64, x1: f64) -> impl Iterator<Item = &'a Poly> + '_ {
        let from = x0 - self.max_w - INTERVAL_EPS;
        let start = self.polys.partition_point(|p| p.bbox.min.x < from);
        self.polys[start..]
            .iter()
            .copied()
            .take_while(move |p| p.bbox.min.x <= x1 + INTERVAL_EPS)
            .filter(move |p| p.bbox.max.x >= x0 - INTERVAL_EPS)
    }
}

/// The translations a part's bounding box may take in a region.
struct Bounds {
    x_lo: f64,
    x_hi: f64,
    y_lo: f64,
    y_hi: f64,
}

fn region_bounds(region: &Region, o: &Oriented, space: &Space) -> Option<Bounds> {
    match &region.container {
        None => {
            if !space.fits(o.size) {
                return None;
            }
            Some(Bounds {
                x_lo: space.lo,
                x_hi: (space.hi_x - o.size.x + SHEET_EPS).max(space.lo),
                y_lo: space.lo,
                y_hi: (space.hi_y - o.size.y + SHEET_EPS).max(space.lo),
            })
        }
        Some(h) => {
            let b = Bounds {
                x_lo: h.bbox.min.x - o.bbox.min.x,
                x_hi: h.bbox.max.x - o.bbox.max.x,
                y_lo: h.bbox.min.y - o.bbox.min.y,
                y_hi: h.bbox.max.y - o.bbox.max.y,
            };
            (b.x_hi >= b.x_lo && b.y_hi >= b.y_lo).then_some(b)
        }
    }
}

/// Best orientation index and translation in one region, or `None`.
fn best_in(
    region: &mut Region,
    pi: usize,
    orients: &[Oriented],
    space: &Space,
) -> Option<(usize, Point)> {
    // Each orientation searches on its own (their lows are kept apart), so
    // they run side by side; the best is then picked in orientation order
    // as one after the other would.
    let searches: Vec<(usize, &Oriented, Bounds)> = orients
        .iter()
        .enumerate()
        .filter_map(|(oi, o)| region_bounds(region, o, space).map(|b| (oi, o, b)))
        .collect();
    let jobs: Vec<(usize, &Oriented, Bounds, Lows)> = searches
        .into_iter()
        .map(|(oi, o, b)| (oi, o, b, region.lows.remove(&(pi, oi)).unwrap_or_default()))
        .collect();
    let (occupants, container) = (&region.occupants, region.container.as_ref());
    let found: Vec<(usize, &Oriented, Option<Point>, Lows)> = jobs
        .into_par_iter()
        .map(|(oi, o, bounds, mut lows)| {
            let pos = lowest_position(occupants, container, o, &bounds, &mut lows);
            (oi, o, pos, lows)
        })
        .collect();
    let mut best: Option<(f64, f64, usize, Point)> = None;
    for (oi, o, pos, lows) in found {
        region.lows.insert((pi, oi), lows);
        let Some(pos) = pos else {
            continue;
        };
        let top = pos.y + o.size.y;
        let right = pos.x + o.size.x;
        let better = match best {
            None => true,
            Some((bt, br, _, _)) => top < bt - 1e-9 || (top <= bt + 1e-9 && right < br - 1e-9),
        };
        if better {
            best = Some((top, right, oi, pos));
        }
    }
    best.map(|b| (b.2, b.3))
}

/// Lowest, then leftmost, clear translation for one orientation, clear of
/// `placed` and inside `container` when there is one.
fn lowest_position(
    placed: &[Poly],
    container: Option<&Poly>,
    o: &Oriented,
    b: &Bounds,
    lows: &mut Lows,
) -> Option<Point> {
    let (x_lo, x_hi, y_lo, y_hi) = (b.x_lo, b.x_hi, b.y_lo, b.y_hi);

    let mut xs = Vec::with_capacity(GRID_STEPS + 2 + 2 * placed.len());
    for k in 0..=GRID_STEPS {
        xs.push(x_lo + (x_hi - x_lo) * k as f64 / GRID_STEPS as f64);
    }
    for p in placed {
        xs.push(p.bbox.max.x - o.bbox.min.x + CLEAR_EPS);
        xs.push(p.bbox.min.x - o.bbox.max.x - CLEAR_EPS);
    }
    xs.retain(|&x| x >= x_lo && x <= x_hi);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);

    let index = XIndex::new(placed);
    let mut best: Option<Point> = None;
    let mut intervals = Vec::new();
    for &x in &xs {
        let cap = best.map_or(y_hi, |b| b.y.min(y_hi));
        let from = lows.get(&x.to_bits()).map_or(y_lo, |&low| low.max(y_lo));
        if from > cap {
            continue;
        }
        let found = lowest_y(&index, container, o, x, from, cap, &mut intervals);
        lows.insert(x.to_bits(), found.unwrap_or(cap));
        let Some(y) = found else {
            continue;
        };
        if best.is_some_and(|b| y >= b.y - 1e-12) {
            continue;
        }
        if overlaps_any(&index, container, o, Vec2::new(x, y)) {
            continue;
        }
        best = Some(Point::new(x, y));
    }
    best
}

/// Lowest Y from `y_lo` to `y_hi` where the footprint at X offset `x` clears
/// every placed polygon and lies inside `container` when there is one.
///
/// Each pair of edges whose X ranges meet rules out one interval of Y
/// offsets, the offsets at which they touch or cross. Between those
/// intervals the footprint either clears everything or lies wholly inside
/// (or around) a placed polygon, which one point test tells apart; the same
/// goes for lying inside or outside the container.
fn lowest_y(
    placed: &XIndex,
    container: Option<&Poly>,
    o: &Oriented,
    x: f64,
    y_lo: f64,
    y_hi: f64,
    intervals: &mut Vec<(f64, f64)>,
) -> Option<f64> {
    intervals.clear();
    let fx0 = o.bbox.min.x + x;
    let fx1 = o.bbox.max.x + x;
    let dy_lo = y_lo + o.bbox.min.y;
    let dy_hi = y_hi + o.bbox.max.y;
    let relevant: Vec<&Poly> = placed
        .in_x(fx0, fx1)
        .filter(|p| p.bbox.max.y >= dy_lo - INTERVAL_EPS && p.bbox.min.y <= dy_hi + INTERVAL_EPS)
        .collect();
    let shift = Vec2::new(x, 0.0);
    for m in &o.polys {
        let mx0 = m.bbox.min.x + x;
        let mx1 = m.bbox.max.x + x;
        for p in relevant.iter().copied().chain(container) {
            if p.bbox.max.x < mx0 - INTERVAL_EPS || p.bbox.min.x > mx1 + INTERVAL_EPS {
                continue;
            }
            let n = m.pts.len();
            for i in 0..n {
                let a0 = m.pts[i].offset(shift);
                let a1 = m.pts[(i + 1) % n].offset(shift);
                let (ex0, ex1) = min_max(a0.x, a1.x);
                p.edges_in(ex0, ex1, |b0, b1| {
                    if let Some((lo, hi)) = drop_interval(a0, a1, b0, b1) {
                        if hi >= y_lo && lo <= y_hi {
                            intervals.push((lo, hi));
                        }
                    }
                });
            }
        }
    }
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

    let clear_at = |y: f64| {
        let shift = Vec2::new(x, y);
        container.is_none_or(|h| within(h, o, shift)) && !contained(&relevant, o, shift)
    };
    let mut cur = y_lo;
    let mut i = 0;
    while i < intervals.len() {
        let (lo, mut hi) = intervals[i];
        i += 1;
        while i < intervals.len() && intervals[i].0 <= hi {
            hi = hi.max(intervals[i].1);
            i += 1;
        }
        if hi < cur {
            continue;
        }
        if lo > cur {
            if cur > y_hi {
                return None;
            }
            if clear_at(cur) {
                return Some(cur);
            }
        }
        cur = cur.max(hi + CLEAR_EPS);
    }
    if cur <= y_hi && clear_at(cur) {
        return Some(cur);
    }
    None
}

/// The Y offsets at which segment `a` moved straight up or down touches or
/// crosses segment `b`, widened by [`INTERVAL_EPS`].
fn drop_interval(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<(f64, f64)> {
    let (ax0, ax1) = min_max(a0.x, a1.x);
    let (bx0, bx1) = min_max(b0.x, b1.x);
    let mut xa = ax0.max(bx0);
    let mut xb = ax1.min(bx1);
    if xa > xb + INTERVAL_EPS {
        return None;
    }
    if xa > xb {
        let mid = 0.5 * (xa + xb);
        xa = mid;
        xb = mid;
    }
    let (a_lo, a_hi) = y_range_at(a0, a1, xa);
    let (b_lo, b_hi) = y_range_at(b0, b1, xa);
    let (a_lo2, a_hi2) = y_range_at(a0, a1, xb);
    let (b_lo2, b_hi2) = y_range_at(b0, b1, xb);
    let lo = (b_lo - a_hi).min(b_lo2 - a_hi2) - INTERVAL_EPS;
    let hi = (b_hi - a_lo).max(b_hi2 - a_lo2) + INTERVAL_EPS;
    Some((lo, hi))
}

/// The Y values a segment takes at `x`: one value, or its whole range when
/// it is vertical.
fn y_range_at(p0: Point, p1: Point, x: f64) -> (f64, f64) {
    let dx = p1.x - p0.x;
    if dx.abs() <= 1e-12 {
        return min_max(p0.y, p1.y);
    }
    let t = ((x - p0.x) / dx).clamp(0.0, 1.0);
    let y = p0.y + t * (p1.y - p0.y);
    (y, y)
}

/// With no boundary contact, true when a footprint polygon at `shift` lies
/// inside a placed polygon or the other way round.
fn contained(placed: &[&Poly], o: &Oriented, shift: Vec2) -> bool {
    for m in &o.polys {
        let mb = BBox::new(m.bbox.min.offset(shift), m.bbox.max.offset(shift));
        let back = Vec2::new(-shift.x, -shift.y);
        for p in placed {
            if !mb.intersects(p.bbox) {
                continue;
            }
            if point_in_poly(m.pts[0].offset(shift), p) || point_in_poly(p.pts[0].offset(back), m) {
                return true;
            }
        }
    }
    false
}

/// With no boundary contact, true when every footprint polygon at `shift`
/// lies inside `container`.
fn within(container: &Poly, o: &Oriented, shift: Vec2) -> bool {
    o.polys
        .iter()
        .all(|m| point_in_poly(m.pts[0].offset(shift), container))
}

/// Conservative final check: true when the footprint at `shift` touches or
/// overlaps any placed polygon, or leaves the container.
fn overlaps_any(placed: &XIndex, container: Option<&Poly>, o: &Oriented, shift: Vec2) -> bool {
    for m in &o.polys {
        let moved = m.translated(shift);
        if let Some(h) = container {
            if boundaries_touch(&moved, h) || !point_in_poly(moved.pts[0], h) {
                return true;
            }
        }
        for p in placed.in_x(moved.bbox.min.x, moved.bbox.max.x) {
            if !moved.bbox.intersects(p.bbox) {
                continue;
            }
            if boundaries_touch(&moved, p)
                || point_in_poly(moved.pts[0], p)
                || point_in_poly(p.pts[0], &moved)
            {
                return true;
            }
        }
    }
    false
}

/// True when any edge of `a` touches or crosses any edge of `b`.
fn boundaries_touch(a: &Poly, b: &Poly) -> bool {
    let n = a.pts.len();
    for i in 0..n {
        let a0 = a.pts[i];
        let a1 = a.pts[(i + 1) % n];
        let (x0, x1) = min_max(a0.x, a1.x);
        let mut hit = false;
        b.edges_in(x0, x1, |b0, b1| {
            hit = hit || segments_touch(a0, a1, b0, b1);
        });
        if hit {
            return true;
        }
    }
    false
}

fn orient(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn on_segment(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Closed segment test: touching end points and collinear overlaps count.
fn segments_touch(p1: Point, p2: Point, q1: Point, q2: Point) -> bool {
    let d1 = orient(q1, q2, p1);
    let d2 = orient(q1, q2, p2);
    let d3 = orient(p1, p2, q1);
    let d4 = orient(p1, p2, q2);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_segment(q1, q2, p1))
        || (d2 == 0.0 && on_segment(q1, q2, p2))
        || (d3 == 0.0 && on_segment(p1, p2, q1))
        || (d4 == 0.0 && on_segment(p1, p2, q2))
}

/// Even odd point in polygon test.
fn point_in_poly(p: Point, poly: &Poly) -> bool {
    if !poly.bbox.contains(p) {
        return false;
    }
    let n = poly.pts.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let a = poly.pts[i];
        let b = poly.pts[j];
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn min_max(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Builds the reported placement from the job to sheet transform.
fn placement(part_id: u64, copy: usize, sheet: usize, matrix: Mat3) -> NestPlacement {
    let mirrored = matrix.determinant() < 0.0;
    let angle = if mirrored {
        (-matrix.b).atan2(-matrix.a)
    } else {
        matrix.b.atan2(matrix.a)
    };
    let mut deg = (angle.to_degrees().rem_euclid(360.0) * 1e9).round() / 1e9;
    if deg >= 360.0 || deg == 0.0 {
        deg = 0.0;
    }
    NestPlacement {
        part_id,
        copy,
        sheet,
        rotation_deg: deg,
        mirrored,
        translation: Vec2::new(matrix.e, matrix.f),
        matrix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::boolean::intersect_paths;
    use crate::geom::construct::circle;
    use crate::geom::metrics::path_area;
    use crate::geom::path::SubPath;

    fn tol() -> Tolerance {
        Tolerance::FLATTEN
    }

    fn rect_part(id: u64, w: f64, h: f64, quantity: usize) -> NestPart {
        let sub = SubPath::new(
            Point::new(0.0, 0.0),
            vec![
                Segment::line(Point::new(w, 0.0)),
                Segment::line(Point::new(w, h)),
                Segment::line(Point::new(0.0, h)),
            ],
            true,
        );
        NestPart {
            id,
            paths: vec![sub.into_path()],
            quantity,
            filler: false,
            no_mirror: false,
        }
    }

    fn l_part(id: u64, quantity: usize) -> NestPart {
        let pts = [
            (10.0, 10.0),
            (70.0, 10.0),
            (70.0, 25.0),
            (25.0, 25.0),
            (25.0, 60.0),
            (10.0, 60.0),
        ];
        let segs = pts[1..]
            .iter()
            .map(|&(x, y)| Segment::line(Point::new(x, y)))
            .collect();
        NestPart {
            id,
            paths: vec![SubPath::new(Point::new(pts[0].0, pts[0].1), segs, true).into_path()],
            quantity,
            filler: false,
            no_mirror: false,
        }
    }

    fn params(w: f64, h: f64) -> NestParams {
        NestParams {
            tool_diameter: 0.0,
            gap: 0.0,
            border: 0.0,
            sheet_width: w,
            sheet_height: h,
            ..NestParams::default()
        }
    }

    fn placed_paths(parts: &[NestPart], r: &NestResult) -> Vec<(usize, Path)> {
        r.placements
            .iter()
            .map(|pl| {
                let part = parts.iter().find(|p| p.id == pl.part_id).expect("part");
                let mut path = Path::new();
                for p in &part.paths {
                    let closed = Path::from_subpaths(
                        p.subpaths
                            .iter()
                            .filter(|s| s.closed)
                            .map(normalised_loop)
                            .collect(),
                    );
                    path.extend(transform_path(&closed, &pl.matrix));
                }
                (pl.sheet, path)
            })
            .collect()
    }

    fn check_valid(parts: &[NestPart], p: &NestParams, r: &NestResult) {
        let placed = placed_paths(parts, r);
        let edge = p.border + p.gap;
        for (_, path) in &placed {
            let b = path_bbox(path).expect("bbox");
            assert!(b.min.x >= edge - 1e-6, "left {b:?}");
            assert!(b.min.y >= edge - 1e-6, "bottom {b:?}");
            assert!(b.max.x <= p.sheet_width - edge + 1e-6, "right {b:?}");
            assert!(b.max.y <= p.sheet_height - edge + 1e-6, "top {b:?}");
        }
        let spacing = p.gap + p.tool_diameter;
        let grown: Vec<(usize, Vec<Path>)> = placed
            .iter()
            .map(|(s, path)| {
                let g = if spacing > 0.01 {
                    let loops = offset_path(
                        path,
                        0.5 * spacing - 0.005,
                        OffsetSide::Outward,
                        OffsetCorner::Round,
                        tol(),
                    )
                    .expect("offset");
                    // One vector per part, so its holes stay holes.
                    vec![Path::from_subpaths(
                        loops.into_iter().flat_map(|l| l.subpaths).collect(),
                    )]
                } else {
                    vec![path.clone()]
                };
                (*s, g)
            })
            .collect();
        for i in 0..grown.len() {
            for j in i + 1..grown.len() {
                if grown[i].0 != grown[j].0 {
                    continue;
                }
                let common = intersect_paths(&grown[i].1, &grown[j].1, tol()).expect("boolean");
                let area: f64 = common.iter().map(|c| path_area(c, tol()).abs()).sum();
                assert!(area < 1e-4, "parts {i} and {j} overlap by {area}");
            }
        }
    }

    #[test]
    fn parts_nest_inside_a_boundary_vector() {
        // A right triangle 200 on each side: squares only fit in its lower
        // left, never across the long edge.
        let triangle = SubPath::new(
            Point::new(0.0, 0.0),
            vec![
                Segment::line(Point::new(200.0, 0.0)),
                Segment::line(Point::new(0.0, 200.0)),
                Segment::line(Point::new(0.0, 0.0)),
            ],
            true,
        )
        .into_path();
        let parts = vec![rect_part(1, 20.0, 20.0, 12)];
        let p = NestParams {
            tool_diameter: 0.0,
            gap: 2.0,
            border: 3.0,
            sheet_width: 200.0,
            sheet_height: 200.0,
            max_sheets: 1,
            ..NestParams::default()
        };
        let r = nest_within(&parts, &p, Some(&triangle), tol()).expect("nest");
        assert!(!r.placements.is_empty());
        for (_, path) in placed_paths(&parts, &r) {
            let b = path_bbox(&path).expect("bbox");
            // Inside the triangle x + y <= 200 with 5 mm to spare along the
            // slope (5 * sqrt 2 in x + y) and from both straight edges.
            assert!(b.min.x >= 5.0 - 1e-6 && b.min.y >= 5.0 - 1e-6, "{b:?}");
            assert!(
                b.max.x + b.max.y <= 200.0 - 5.0 * 2f64.sqrt() + 1e-3,
                "{b:?}"
            );
        }
        let empty = SubPath::open(Point::new(0.0, 0.0)).into_path();
        assert!(nest_within(&parts, &p, Some(&empty), tol()).is_err());
    }

    #[test]
    fn the_border_gap_adds_to_the_clearance_at_the_edges() {
        let parts = vec![rect_part(1, 20.0, 20.0, 1)];
        let p = NestParams {
            tool_diameter: 0.0,
            gap: 4.0,
            border: 6.0,
            sheet_width: 100.0,
            sheet_height: 100.0,
            ..NestParams::default()
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        let (_, path) = &placed_paths(&parts, &r)[0];
        let b = path_bbox(path).expect("bbox");
        assert!(
            (b.min.x - 10.0).abs() < 1e-6 && (b.min.y - 10.0).abs() < 1e-6,
            "{b:?}"
        );
    }

    #[test]
    fn mixed_parts_do_not_overlap_and_stay_inside_border() {
        let parts = vec![
            rect_part(1, 60.0, 30.0, 3),
            NestPart {
                id: 2,
                paths: vec![circle(Point::new(0.0, 0.0), 18.0).expect("circle")],
                quantity: 4,
                filler: false,
                no_mirror: false,
            },
            l_part(3, 3),
            NestPart {
                id: 4,
                paths: vec![SubPath::new(
                    Point::new(0.0, 0.0),
                    vec![
                        Segment::line(Point::new(40.0, 0.0)),
                        Segment::arc(Point::new(40.0, 25.0), 1.0),
                        Segment::line(Point::new(0.0, 25.0)),
                        Segment::arc(Point::new(0.0, 0.0), 1.0),
                    ],
                    true,
                )
                .into_path()],
                quantity: 2,
                filler: false,
                no_mirror: false,
            },
        ];
        let p = NestParams {
            tool_diameter: 3.0,
            gap: 1.0,
            border: 5.0,
            sheet_width: 300.0,
            sheet_height: 200.0,
            ..NestParams::default()
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        assert_eq!(r.placements.len(), 12);
        assert_eq!(r.sheet_count, 1);
        check_valid(&parts, &p, &r);
    }

    #[test]
    fn exact_fit_with_no_spacing() {
        let parts = vec![rect_part(1, 50.0, 50.0, 4)];
        let r = nest(&parts, &params(100.0, 100.0), tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        assert_eq!(r.sheet_count, 1);
        check_valid(&parts, &params(100.0, 100.0), &r);
    }

    #[test]
    fn rotation_used_when_it_helps() {
        let parts = vec![rect_part(1, 30.0, 60.0, 1)];
        let p = params(100.0, 50.0);
        let r = nest(&parts, &p, tol()).expect("nest");
        assert_eq!(r.placements.len(), 1);
        let rot = r.placements[0].rotation_deg;
        assert!(rot == 90.0 || rot == 270.0, "rotation {rot}");
        check_valid(&parts, &p, &r);

        let fixed = NestParams {
            rotation_step_deg: 0.0,
            ..p
        };
        let r = nest(&parts, &fixed, tol()).expect("nest");
        assert!(r.placements.is_empty());
        assert_eq!(r.unplaced[0].reason, NestUnplacedReason::TooLarge);
    }

    #[test]
    fn overflows_to_a_second_sheet() {
        let parts = vec![rect_part(7, 60.0, 60.0, 2)];
        let p = params(100.0, 100.0);
        let r = nest(&parts, &p, tol()).expect("nest");
        assert_eq!(r.sheet_count, 2);
        let sheets: Vec<usize> = r.placements.iter().map(|pl| pl.sheet).collect();
        assert_eq!(sheets, vec![0, 1]);
        check_valid(&parts, &p, &r);

        let one = NestParams { max_sheets: 1, ..p };
        let r = nest(&parts, &one, tol()).expect("nest");
        assert_eq!(r.placements.len(), 1);
        assert_eq!(
            r.unplaced,
            vec![NestUnplaced {
                part_id: 7,
                copy: 1,
                reason: NestUnplacedReason::OutOfSheets
            }]
        );
    }

    #[test]
    fn too_big_and_open_parts_are_unplaced() {
        let open = NestPart {
            id: 9,
            paths: vec![SubPath::new(
                Point::new(0.0, 0.0),
                vec![Segment::line(Point::new(10.0, 0.0))],
                false,
            )
            .into_path()],
            quantity: 1,
            filler: false,
            no_mirror: false,
        };
        let parts = vec![
            rect_part(1, 200.0, 20.0, 1),
            open,
            rect_part(2, 10.0, 10.0, 1),
        ];
        let p = NestParams {
            border: 5.0,
            ..params(100.0, 100.0)
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        assert_eq!(r.placements.len(), 1);
        assert_eq!(r.placements[0].part_id, 2);
        let reasons: Vec<(u64, NestUnplacedReason)> =
            r.unplaced.iter().map(|u| (u.part_id, u.reason)).collect();
        assert_eq!(
            reasons,
            vec![
                (1, NestUnplacedReason::TooLarge),
                (9, NestUnplacedReason::NoClosedOutline)
            ]
        );
    }

    #[test]
    fn deterministic() {
        let parts = vec![
            l_part(1, 5),
            NestPart {
                id: 2,
                paths: vec![circle(Point::new(3.0, 4.0), 11.0).expect("circle")],
                quantity: 5,
                filler: false,
                no_mirror: false,
            },
        ];
        let p = NestParams {
            rotation_step_deg: 45.0,
            allow_mirror: true,
            tool_diameter: 2.0,
            gap: 0.5,
            border: 3.0,
            sheet_width: 160.0,
            sheet_height: 120.0,
            ..NestParams::default()
        };
        let a = nest(&parts, &p, tol()).expect("nest");
        let b = nest(&parts, &p, tol()).expect("nest");
        assert_eq!(a, b);
        let ja = serde_json::to_string(&a).expect("json");
        let jb = serde_json::to_string(&b).expect("json");
        assert_eq!(ja, jb);
        check_valid(&parts, &p, &a);
    }

    /// A rectangle loop from (x0, y0) to (x1, y1), counter clockwise or
    /// clockwise, with the closing edge implicit or spelled out.
    fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64, ccw: bool, explicit: bool) -> SubPath {
        let mut pts = if ccw {
            vec![Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)]
        } else {
            vec![Point::new(x0, y1), Point::new(x1, y1), Point::new(x1, y0)]
        };
        if explicit {
            pts.push(Point::new(x0, y0));
        }
        SubPath::new(
            Point::new(x0, y0),
            pts.into_iter().map(Segment::line).collect(),
            true,
        )
    }

    fn spaced_params() -> NestParams {
        NestParams {
            tool_diameter: 6.0,
            gap: 1.0,
            border: 10.0,
            sheet_width: 300.0,
            sheet_height: 200.0,
            ..NestParams::default()
        }
    }

    #[test]
    fn outline_winding_and_closing_do_not_matter() {
        let p = spaced_params();
        for ccw in [true, false] {
            for explicit in [false, true] {
                let parts: Vec<NestPart> = [
                    (1, 10.0, 10.0, 70.0, 50.0),
                    (2, 100.0, 10.0, 130.0, 90.0),
                    (3, 150.0, 10.0, 190.0, 30.0),
                ]
                .iter()
                .map(|&(id, x0, y0, x1, y1)| NestPart {
                    id,
                    paths: vec![rect_loop(x0, y0, x1, y1, ccw, explicit).into_path()],
                    quantity: 4,
                    filler: false,
                    no_mirror: false,
                })
                .collect();
                let r = nest(&parts, &p, tol()).expect("nest");
                assert_eq!(r.placements.len() + r.unplaced.len(), 12);
                assert!(r.placements.len() >= 10, "ccw {ccw} explicit {explicit}");
                check_valid(&parts, &p, &r);
            }
        }
    }

    #[test]
    fn mixed_windings_and_a_part_with_a_hole() {
        let mut frame = Path::from_subpath(rect_loop(0.0, 0.0, 60.0, 60.0, true, false));
        frame.push(rect_loop(20.0, 20.0, 40.0, 40.0, false, true));
        let mut frame_same_way = Path::from_subpath(rect_loop(0.0, 0.0, 50.0, 40.0, false, false));
        frame_same_way.push(rect_loop(10.0, 10.0, 40.0, 30.0, false, false));
        let parts = vec![
            NestPart {
                id: 1,
                paths: vec![rect_loop(5.0, 5.0, 45.0, 25.0, true, false).into_path()],
                quantity: 3,
                filler: false,
                no_mirror: false,
            },
            NestPart {
                id: 2,
                paths: vec![rect_loop(-20.0, 0.0, 10.0, 50.0, false, true).into_path()],
                quantity: 3,
                filler: false,
                no_mirror: false,
            },
            NestPart {
                id: 3,
                paths: vec![frame],
                quantity: 3,
                filler: false,
                no_mirror: false,
            },
            NestPart {
                id: 4,
                paths: vec![frame_same_way],
                quantity: 2,
                filler: false,
                no_mirror: false,
            },
            l_part(5, 2),
        ];
        let p = spaced_params();
        let r = nest(&parts, &p, tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        assert_eq!(r.placements.len(), 13);
        check_valid(&parts, &p, &r);
    }

    #[test]
    fn start_corner_and_direction() {
        let parts = vec![rect_part(1, 20.0, 10.0, 2)];
        let base = NestParams {
            rotation_step_deg: 0.0,
            ..params(100.0, 80.0)
        };
        let tr = NestParams {
            start_corner: NestCorner::TopRight,
            ..base
        };
        let r = nest(&parts, &tr, tol()).expect("nest");
        check_valid(&parts, &tr, &r);
        let first = placed_paths(&parts, &r);
        let b = path_bbox(&first[0].1).expect("bbox");
        assert!((b.max.x - 100.0).abs() < 1e-6 && (b.max.y - 80.0).abs() < 1e-6);
        assert!(!r.placements[0].mirrored);
        assert_eq!(r.placements[0].rotation_deg, 0.0);

        let col = NestParams {
            direction: NestDirection::Y,
            ..base
        };
        let r = nest(&parts, &col, tol()).expect("nest");
        check_valid(&parts, &col, &r);
        let placed = placed_paths(&parts, &r);
        let b = path_bbox(&placed[1].1).expect("bbox");
        assert!(b.min.x.abs() < 1e-6, "second part should stack in Y: {b:?}");
        assert!(b.min.y >= 10.0 - 1e-6);
    }

    #[test]
    fn mirror_placement_formula() {
        let parts = vec![l_part(1, 1)];
        let p = NestParams {
            allow_mirror: true,
            rotation_step_deg: 90.0,
            ..params(200.0, 200.0)
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        let pl = &r.placements[0];
        let q = Point::new(25.0, 60.0);
        let v = if pl.mirrored {
            Point::new(-q.x, q.y)
        } else {
            q
        };
        let rot = rotation(pl.rotation_deg).apply(v);
        let expect = Point::new(rot.x + pl.translation.x, rot.y + pl.translation.y);
        assert!(pl
            .matrix
            .apply(q)
            .approx_eq(expect, Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn rejects_bad_params() {
        let parts = vec![rect_part(1, 10.0, 10.0, 1)];
        for bad in [
            NestParams {
                sheet_width: 0.0,
                ..NestParams::default()
            },
            NestParams {
                gap: -1.0,
                ..NestParams::default()
            },
            NestParams {
                rotation_step_deg: 0.5,
                ..NestParams::default()
            },
            NestParams {
                max_sheets: 0,
                ..NestParams::default()
            },
            NestParams {
                border: 700.0,
                ..NestParams::default()
            },
        ] {
            assert!(nest(&parts, &bad, tol()).is_err());
        }
    }

    #[test]
    fn params_serde_names_and_defaults() {
        let p: NestParams = serde_json::from_str(
            r#"{"toolDiameter":3,"rotationStepDeg":45,"allowMirror":true,
                "sheetWidth":500,"startCorner":"topRight","direction":"y","maxSheets":2}"#,
        )
        .expect("json");
        assert_eq!(p.tool_diameter, 3.0);
        assert_eq!(p.start_corner, NestCorner::TopRight);
        assert_eq!(p.direction, NestDirection::Y);
        assert_eq!(p.sheet_height, 2440.0);
        let part: NestPart = serde_json::from_str(r#"{"id":4,"paths":[]}"#).expect("json");
        assert_eq!(part.quantity, 1);
    }

    /// A square frame from (0, 0) to (100, 100) with a 60 by 60 hole.
    fn frame_part(id: u64) -> NestPart {
        let mut frame = Path::from_subpath(rect_loop(0.0, 0.0, 100.0, 100.0, true, false));
        frame.push(rect_loop(20.0, 20.0, 80.0, 80.0, false, true));
        NestPart {
            id,
            paths: vec![frame],
            quantity: 1,
            filler: false,
            no_mirror: false,
        }
    }

    fn hole_params() -> NestParams {
        NestParams {
            tool_diameter: 2.0,
            gap: 0.0,
            border: 1.0,
            rotation_step_deg: 0.0,
            sheet_width: 104.0,
            sheet_height: 104.0,
            max_sheets: 1,
            ..NestParams::default()
        }
    }

    #[test]
    fn small_parts_nest_into_holes_keeping_the_spacing() {
        let parts = vec![frame_part(1), rect_part(2, 20.0, 20.0, 4)];
        let off = hole_params();
        let r = nest(&parts, &off, tol()).expect("nest");
        assert_eq!(r.placements.len(), 1, "holes stay solid by default");
        assert_eq!(r.unplaced.len(), 4);

        let on = NestParams {
            nest_in_holes: true,
            ..off
        };
        let r = nest(&parts, &on, tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        assert_eq!(r.sheet_count, 1);
        check_valid(&parts, &on, &r);
        let frame = &r.placements[0];
        assert_eq!(frame.part_id, 1);
        let lo = frame.matrix.apply(Point::new(20.0, 20.0));
        let hi = frame.matrix.apply(Point::new(80.0, 80.0));
        let spacing = on.tool_diameter + on.gap;
        for (_, path) in placed_paths(&parts, &r).iter().skip(1) {
            let b = path_bbox(path).expect("bbox");
            assert!(b.min.x >= lo.x + spacing - 1e-6 && b.min.y >= lo.y + spacing - 1e-6);
            assert!(b.max.x <= hi.x - spacing + 1e-6 && b.max.y <= hi.y - spacing + 1e-6);
        }
    }

    #[test]
    fn curved_holes_take_parts_and_islands_block_them() {
        let hole = |r: f64| {
            let c = circle(Point::new(0.0, 0.0), r).expect("circle");
            Path::from_subpaths(c.subpaths.iter().map(reverse_subpath).collect())
        };
        let mut ring = circle(Point::new(0.0, 0.0), 50.0).expect("circle");
        ring.extend(hole(35.0));
        let parts = vec![
            NestPart {
                id: 1,
                paths: vec![ring],
                quantity: 1,
                filler: false,
                no_mirror: false,
            },
            rect_part(2, 12.0, 12.0, 6),
        ];
        let p = NestParams {
            tool_diameter: 3.0,
            gap: 1.0,
            border: 0.0,
            sheet_width: 104.0,
            sheet_height: 104.0,
            max_sheets: 1,
            nest_in_holes: true,
            ..NestParams::default()
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        check_valid(&parts, &p, &r);
        let centre = r.placements[0].matrix.apply(Point::new(0.0, 0.0));
        let in_hole = placed_paths(&parts, &r)
            .iter()
            .skip(1)
            .filter(|(_, path)| {
                let b = path_bbox(path).expect("bbox");
                [
                    b.min,
                    b.max,
                    Point::new(b.min.x, b.max.y),
                    Point::new(b.max.x, b.min.y),
                ]
                .iter()
                .all(|c| c.distance(centre) <= 35.0 - 4.0 + 1e-6)
            })
            .count();
        assert!(in_hole >= 4, "only {in_hole} parts in the hole");

        // A solid disc filling the hole leaves no room in it.
        let mut plugged = circle(Point::new(0.0, 0.0), 50.0).expect("circle");
        plugged.extend(hole(35.0));
        plugged.extend(circle(Point::new(0.0, 0.0), 30.0).expect("circle"));
        let parts = vec![
            NestPart {
                id: 1,
                paths: vec![plugged],
                quantity: 1,
                filler: false,
                no_mirror: false,
            },
            rect_part(2, 12.0, 12.0, 6),
        ];
        let r = nest(&parts, &p, tol()).expect("nest");
        let centre = r.placements[0].matrix.apply(Point::new(0.0, 0.0));
        for (_, path) in placed_paths(&parts, &r).iter().skip(1) {
            let b = path_bbox(path).expect("bbox");
            let mid = Point::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y));
            assert!(
                mid.distance(centre) > 35.0,
                "a part went into the plugged hole"
            );
        }
    }

    #[test]
    fn fillers_fill_the_leftover_space_without_new_sheets() {
        let mut filler = rect_part(2, 10.0, 10.0, 1);
        filler.filler = true;
        let parts = vec![rect_part(1, 60.0, 60.0, 1), filler.clone()];
        let p = NestParams {
            rotation_step_deg: 0.0,
            ..params(100.0, 100.0)
        };
        let r = nest(&parts, &p, tol()).expect("nest");
        assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
        assert_eq!(r.sheet_count, 1);
        let fillers = r.placements.iter().filter(|pl| pl.part_id == 2).count();
        assert!(fillers >= 60, "only {fillers} fillers");
        assert!(r.placements.iter().all(|pl| pl.sheet == 0));
        check_valid(&parts, &p, &r);

        let only = nest(&[filler.clone()], &params(30.0, 30.0), tol()).expect("nest");
        assert_eq!(only.placements.len(), 9);
        assert_eq!(only.sheet_count, 1);

        let big = NestPart {
            filler: true,
            no_mirror: false,
            ..rect_part(3, 200.0, 10.0, 1)
        };
        let r = nest(&[rect_part(1, 10.0, 10.0, 1), big], &p, tol()).expect("nest");
        assert_eq!(r.unplaced.len(), 1);
        assert_eq!(r.unplaced[0].reason, NestUnplacedReason::TooLarge);
    }

    #[test]
    fn fillers_also_go_into_holes_and_stay_deterministic() {
        let mut filler = rect_part(2, 15.0, 15.0, 1);
        filler.filler = true;
        let parts = vec![frame_part(1), filler];
        let p = NestParams {
            nest_in_holes: true,
            rotation_step_deg: 90.0,
            ..hole_params()
        };
        let a = nest(&parts, &p, tol()).expect("nest");
        let b = nest(&parts, &p, tol()).expect("nest");
        assert_eq!(a, b);
        let fillers = a.placements.iter().filter(|pl| pl.part_id == 2).count();
        assert!(fillers >= 4, "only {fillers} fillers in the hole");
        check_valid(&parts, &p, &a);
    }

    #[test]
    fn hole_and_filler_serde_names() {
        let p: NestParams = serde_json::from_str(r#"{"nestInHoles":true}"#).expect("json");
        assert!(p.nest_in_holes);
        assert!(!NestParams::default().nest_in_holes);
        let part: NestPart =
            serde_json::from_str(r#"{"id":4,"paths":[],"filler":true}"#).expect("json");
        assert!(part.filler);
    }
}
