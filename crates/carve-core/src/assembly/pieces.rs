//! Pieces of the Assembly View: the parts each sheet's toolpaths cut free,
//! taken from the sheet's toolpath simulation and placed in assembly space
//! to preview the finished piece of furniture.
//!
//! A sheet's simulation is split into parts: regions of material a through
//! cut separates from the waste round them. Tabs count as cut through, so a
//! part still held by tabs is its own piece; the slug that drops out of a
//! hole in a part is waste, unless a profile cut it free from a closed
//! vector that fits it (a part nested in another part's hole): cut on the
//! outside, or on the line when that cut's own kerf parts it and it is at
//! least two tool widths across. A sheet that
//! has nothing cut free is one piece, the whole sheet. A sheet can instead
//! give its raw model (its visible components baked) as its one piece,
//! part 0; a sheet with a model and no toolpaths always does. A component
//! from the component tree can be a piece of its own too, standing alone
//! on its sheet's model grid; it then leaves its sheet's raw model.
//!
//! Parts are numbered in scan order, so a toolpath edit can renumber them.
//! A stored placement keeps an [`Anchor`] (where its part's material lies
//! on the sheet) and follows the part that best fits it, the saved number
//! only breaking ties.
//!
//! A piece's local frame is the box round its part: X and Y from the box's
//! lower left corner, Z up from the material's underside, in millimetres.
//! Its pose puts the middle of that box at `position` after turning it by
//! `rotation` (degrees about X, then Y, then Z, fixed axes), so a turn never
//! moves the piece's middle.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cam::sim::{Stock, THROUGH};
use crate::cam::toolpath::MoveKind;
use crate::doc::{Document, Side};
use crate::error::CoreError;

use super::pose::{self, Pose, Vec3};

/// Space left between sheets laid out side by side before their pieces are
/// placed, in millimetres.
pub const LAYOUT_GAP: f64 = 20.0;

/// Material left closer than this to a surface does not count as running
/// into another piece, in cells of the other piece's grid: the reliefs are
/// sampled, so pieces that only touch must not report an overlap.
const TOUCH_CELLS: f64 = 0.5;

/// Most samples taken up one column of material.
const MAX_LAYERS: usize = 32;

/// Regions of material with fewer cells than this are crumbs, not parts.
const MIN_PART_CELLS: usize = 4;

/// Material a through cutting toolpath left standing counts as a tab when it
/// is thinner than this share of the material.
const TAB_SHARE: f32 = 0.5;

/// A wall normal within this many degrees of a sheet axis is taken as the
/// axis.
const AXIS_SNAP_DEGREES: f64 = 4.0;

/// Where a piece sits in assembly space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    /// The sheet whose simulation result the piece comes from.
    pub sheet: u64,
    /// The part of the sheet, from 1.
    #[serde(default = "first_part")]
    pub part: u32,
    /// Where the middle of the part's box sits, millimetres.
    pub position: Vec3,
    /// Turns about X, then Y, then Z, degrees.
    pub rotation: Vec3,
    /// Where the part lay on its sheet when the piece was placed, so the
    /// placement stays with that part when the parts are numbered again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    /// The component (from the component tree) this piece is, standing
    /// alone; `None` for a part of a sheet or a sheet's raw model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<u64>,
    /// Hidden in the Assembly View (still measured by Verify Fit).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Grounded: it cannot be dragged and stays put when mates solve.
    #[serde(default, skip_serializing_if = "is_false")]
    pub grounded: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn first_part() -> u32 {
    1
}

/// The part number of a sheet's raw model piece.
pub const MODEL_PART: u32 = 0;

/// Where a part's material lies on its sheet, seen from above: what keeps a
/// placed piece on the same physical part when a toolpath edit renumbers
/// the sheet's parts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    /// The middle (centroid) of the part's material, job X and Y, mm.
    pub centre: [f64; 2],
    /// The area of the part's material, square millimetres.
    pub area: f64,
}

impl Anchor {
    /// How badly a part with anchor `other` fits this one: `None` when it
    /// is too far off or too different in size to be the same part, else a
    /// cost that is smaller the better they fit.
    pub fn misfit(&self, other: &Anchor, cell: f64) -> Option<f64> {
        if self.area <= 0.0 || other.area <= 0.0 {
            return None;
        }
        let ratio = self.area / other.area;
        if !(0.5..=2.0).contains(&ratio) {
            return None;
        }
        let scale = self.area.max(other.area).sqrt();
        let distance = (self.centre[0] - other.centre[0]).hypot(self.centre[1] - other.centre[1]);
        let reach = (scale * ANCHOR_REACH).max(cell * 4.0).max(2.0);
        (distance <= reach).then(|| distance / scale + ratio.ln().abs())
    }
}

/// How far a part's middle may move and still be the same part, as a share
/// of the square root of its area.
const ANCHOR_REACH: f64 = 0.35;

impl Piece {
    /// The transform from the piece's local frame to assembly space, for a
    /// part box of `size` (width, height, thickness).
    pub fn pose(&self, size: Vec3) -> Pose {
        Pose::translation(self.position)
            .then_local(&Pose::euler(self.rotation))
            .then_local(&Pose::translation(pose::scale(size, -0.5)))
    }

    /// Which piece this is.
    pub fn key(&self) -> PieceRef {
        PieceRef {
            sheet: self.sheet,
            part: self.part,
            component: self.component,
        }
    }
}

/// Names a piece: its sheet and its part number on the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceRef {
    /// The sheet.
    pub sheet: u64,
    /// The part, from 1.
    #[serde(default = "first_part")]
    pub part: u32,
    /// The component for a component piece (its sheet and part 1 with it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<u64>,
}

/// The sheets that make pieces: every sheet that owns a toolpath, in sheet
/// order. A toolpath from before sheets owned toolpaths (sheet zero) counts
/// for every sheet.
pub fn piece_sheets(document: &Document) -> Vec<u64> {
    let toolpaths = document.toolpaths();
    document
        .sheets()
        .iter()
        .filter(|sheet| {
            toolpaths
                .iter()
                .any(|t| t.sheet == sheet.id || t.sheet == 0)
        })
        .map(|sheet| sheet.id)
        .collect()
}

/// A sheet's material box: width, height and thickness in millimetres.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown sheet.
pub fn material_size(document: &Document, sheet: u64) -> Result<Vec3, CoreError> {
    let material = document.sheet_material(sheet)?;
    Ok([material.width, material.height, material.thickness])
}

/// A sheet's simulation: its visible toolpaths cut into its own material on
/// a grid of about `cells` cells, nothing taken away.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown sheet or a toolpath whose tool
/// cannot be simulated.
pub fn simulate(document: &Document, sheet: u64, cells: f64) -> Result<Stock, CoreError> {
    let material = document.sheet_material(sheet)?;
    let job = material.applied_to(document.job_setup());
    let mut stock = Stock::with_cells(&job, cells);
    for (position, toolpath) in document.toolpath_order().iter().enumerate() {
        let Ok(toolpath) = document.toolpath(*toolpath) else {
            continue;
        };
        if !toolpath.visible || (toolpath.sheet != sheet && toolpath.sheet != 0) {
            continue;
        }
        let owner = u16::try_from(position + 1).unwrap_or(u16::MAX);
        stock.cut_for(&job, Side::Top, toolpath, owner)?;
    }
    Ok(stock)
}

fn is_material(stock: &Stock, index: usize) -> bool {
    stock.floor[index] < stock.z[index] - THROUGH
}

/// Labels the regions of `solid` cells joined edge to edge: the region of
/// each cell (`usize::MAX` for none), then each region's cell count and
/// whether it reaches the grid's edge.
fn regions(solid: &[bool], w: usize, h: usize) -> (Vec<usize>, Vec<(usize, bool)>) {
    let mut region = vec![usize::MAX; w * h];
    let mut found: Vec<(usize, bool)> = Vec::new();
    let mut queue = Vec::new();
    for start in 0..w * h {
        if region[start] != usize::MAX || !solid[start] {
            continue;
        }
        let id = found.len();
        let (mut count, mut edge) = (0, false);
        region[start] = id;
        queue.push(start);
        while let Some(i) = queue.pop() {
            count += 1;
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let mut visit = |j: usize| {
                if region[j] == usize::MAX && solid[j] {
                    region[j] = id;
                    queue.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        found.push((count, edge));
    }
    (region, found)
}

/// The empty cells joined to the grid's edge, corners included: the space
/// round the parts, as against the inside of their holes.
fn outside(void: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut out = vec![false; w * h];
    let mut queue = Vec::new();
    for i in 0..w * h {
        let (x, y) = (i % w, i / w);
        let edge = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
        if edge && void[i] && !out[i] {
            out[i] = true;
            queue.push(i);
        }
    }
    while let Some(i) = queue.pop() {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if void[j] && !out[j] {
                    out[j] = true;
                    queue.push(j);
                }
            }
        }
    }
    out
}

/// The anchor of the cells labelled `part`.
fn anchor_of(stock: &Stock, labels: &[u32], part: u32) -> Anchor {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for (i, label) in labels.iter().enumerate() {
        if *label == part {
            sx += (i % stock.width) as f64;
            sy += (i / stock.width) as f64;
            n += 1.0;
        }
    }
    let n_or_one = if n > 0.0 { n } else { 1.0 };
    Anchor {
        centre: [
            stock.origin_x + sx / n_or_one * stock.cell,
            stock.origin_y + sy / n_or_one * stock.cell,
        ],
        area: n * stock.cell * stock.cell,
    }
}

/// A closed vector a profile cut round on a sheet so that what it left
/// inside is a part, even when it lies in the hole of another part: a cut
/// outside the vector, or on it when the kerf alone parts it (see
/// [`outlines`]).
#[derive(Debug, Clone)]
pub struct Outline {
    /// The box the part inside fills, in job coordinates: min X, min Y,
    /// max X, max Y (the vector's box, shrunk by the tool radius for a cut
    /// on the line).
    pub bbox: [f64; 4],
    /// The area the part inside fills, square millimetres.
    pub area: f64,
    /// For a cut on the line: the cutting toolpath (its list position plus
    /// one, as the simulation's owners), whose kerf must be what parts the
    /// region from the material round it. `None` for an outside cut.
    pub kerf: Option<u16>,
    /// The narrowest the part may be across, millimetres: twice the tool
    /// diameter for a cut on the line (a narrower slug is the waste of a
    /// hole), zero for an outside cut.
    pub min_width: f64,
    /// The cutting toolpath (its list position plus one).
    pub owner: u16,
    /// The vector's inside, job coordinates: the part's material lies in it.
    pub region: crate::geom::Region,
    /// A hole: the vector was cut on the inside, so what it leaves inside
    /// is a slug of waste, never a part.
    pub hole: bool,
}

/// The vectors the visible profiles (or laser cuts) on `sheet` cut free:
/// every closed vector cut on the outside, and every closed vector cut on
/// the line, whose inside is its vector shrunk by the tool radius. Inside
/// cuts make holes, whose slugs are waste: they are listed as holes.
pub fn outlines(document: &Document, sheet: u64) -> Vec<Outline> {
    let Ok(tol) = crate::geom::Tolerance::new(0.01) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (position, id) in document.toolpath_order().iter().enumerate() {
        let Ok(toolpath) = document.toolpath(*id) else {
            continue;
        };
        if !toolpath.visible || (toolpath.sheet != sheet && toolpath.sheet != 0) {
            continue;
        }
        let (on, hole) = match cut_side(toolpath) {
            Some("outside") => (false, false),
            Some("on") => (true, false),
            Some("inside") => (false, true),
            _ => continue,
        };
        let radius = toolpath.tool.radius();
        let owner = u16::try_from(position + 1).unwrap_or(u16::MAX);
        for source in &toolpath.sources {
            for (leaf, path) in document.paths_of(source.id) {
                if !seen.insert((leaf, on, hole)) || !path.is_closed() {
                    continue;
                }
                let Some(bbox) = document.bbox_of(leaf) else {
                    continue;
                };
                let region = crate::geom::Region::from_paths(&[path], tol);
                let (bbox, area) = if on {
                    let inset = region.offset(radius, tol);
                    (
                        [
                            bbox.min.x + radius,
                            bbox.min.y + radius,
                            bbox.max.x - radius,
                            bbox.max.y - radius,
                        ],
                        inset.area(),
                    )
                } else {
                    (
                        [bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y],
                        region.area(),
                    )
                };
                if area > 0.0 && bbox[2] > bbox[0] && bbox[3] > bbox[1] {
                    out.push(Outline {
                        bbox,
                        area,
                        kerf: on.then_some(owner),
                        min_width: if on { 4.0 * radius } else { 0.0 },
                        owner,
                        region,
                        hole,
                    });
                }
            }
        }
    }
    out
}

/// The side of its vectors a profile or laser toolpath cut on, when it
/// records one.
fn cut_side(toolpath: &crate::cam::toolpath::Toolpath) -> Option<&str> {
    toolpath.params_json["params"]["side"].as_str()
}

/// The parts of one sheet's simulation.
#[derive(Clone)]
pub struct SheetParts {
    /// The sheet.
    pub sheet: u64,
    /// The sheet's simulation, every toolpath cut, nothing taken away.
    pub stock: Stock,
    /// Part number per cell (from 1), zero for none.
    labels: Vec<u32>,
    /// Each part's box in cells, `[x0, y0, x1, y1)`; part `n` at `n - 1`.
    boxes: Vec<[usize; 4]>,
    /// Each part's anchor (where its material lies), part `n` at `n - 1`.
    anchors: Vec<Anchor>,
    /// True when nothing was cut free and the whole sheet is the one piece.
    pub whole: bool,
    /// Parts thinner than the sheet by what they are (a joint's key cut
    /// from thinner stock): their thickness, the top of the sheet down.
    thin: BTreeMap<u32, f64>,
}

impl std::fmt::Debug for SheetParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetParts")
            .field("sheet", &self.sheet)
            .field("parts", &self.boxes.len())
            .field("whole", &self.whole)
            .finish()
    }
}

impl SheetParts {
    /// Splits a sheet's simulation into its parts (see the module notes),
    /// with no outside profiles known, so every region in a hole is a slug.
    pub fn find(sheet: u64, stock: Stock) -> SheetParts {
        SheetParts::find_with(sheet, stock, &[])
    }

    /// Splits a sheet's simulation into its parts: a region in the hole of
    /// another part is a part too when one of `outlines` fits its box.
    pub fn find_with(sheet: u64, stock: Stock, outlines: &[Outline]) -> SheetParts {
        let (w, h) = (stock.width, stock.height);
        let thickness = (stock.top - stock.bottom) as f32;
        // Toolpaths that cut right through somewhere; what they leave
        // standing thinner than a share of the material is a tab.
        let cutting: BTreeSet<u16> = (0..w * h)
            .filter(|i| !is_material(&stock, *i) && stock.owner[*i] != 0)
            .map(|i| stock.owner[i])
            .collect();
        let solid: Vec<bool> = (0..w * h)
            .map(|i| {
                let tab = cutting.contains(&stock.owner[i])
                    && stock.z[i] - stock.floor[i] < thickness * TAB_SHARE;
                is_material(&stock, i) && !tab
            })
            .collect();
        let (region, found) = regions(&solid, w, h);
        let free = |r: usize| {
            found
                .get(r)
                .is_some_and(|(n, edge)| !edge && *n >= MIN_PART_CELLS)
        };
        // Cells outside every free region are void; a free region is a
        // part when the void round it reaches the sheet's edge, a slug from
        // a hole when it does not (unless an outside cut fits it).
        let void: Vec<bool> = region.iter().map(|r| !free(*r)).collect();
        let out = outside(&void, w, h);
        let mut bounds = vec![[usize::MAX, usize::MAX, 0, 0]; found.len()];
        let mut touches = vec![false; found.len()];
        let mut sums = vec![[0.0_f64; 2]; found.len()];
        // Which toolpaths cut the cells round each free region, and how
        // many of them.
        let mut border: Vec<BTreeMap<u16, usize>> = vec![BTreeMap::new(); found.len()];
        for (i, r) in region.iter().enumerate() {
            if !free(*r) {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let b = &mut bounds[*r];
            *b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
            sums[*r][0] += x as f64;
            sums[*r][1] += y as f64;
            touches[*r] |= (x > 0 && out[i - 1])
                || (x + 1 < w && out[i + 1])
                || (y > 0 && out[i - w])
                || (y + 1 < h && out[i + w]);
            let near = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in near.into_iter().flatten() {
                if region[j] != *r {
                    *border[*r].entry(stock.owner[j]).or_insert(0) += 1;
                }
            }
        }
        let cell = stock.cell;
        let job_box = |b: [usize; 4]| {
            let left = stock.origin_x - cell / 2.0;
            let low = stock.origin_y - cell / 2.0;
            [
                left + b[0] as f64 * cell,
                low + b[1] as f64 * cell,
                left + b[2] as f64 * cell,
                low + b[3] as f64 * cell,
            ]
        };
        let fits_outline = |r: usize| {
            let got = job_box(bounds[r]);
            let area = found[r].0 as f64 * cell * cell;
            let tolerance = (cell * 3.0).max(2.0);
            let narrow = (got[2] - got[0]).min(got[3] - got[1]);
            // A cut on the line parts the region only where its own kerf
            // runs round it: most of the cells round it are that cut's.
            let parted_by = |owner: u16| {
                let all: usize = border[r].values().sum();
                let own = border[r].get(&owner).copied().unwrap_or(0);
                all > 0 && own * 2 > all
            };
            outlines.iter().filter(|o| !o.hole).any(|o| {
                (0..4).all(|k| (o.bbox[k] - got[k]).abs() <= tolerance)
                    && (area / o.area - 1.0).abs() <= 0.25
                    && narrow >= o.min_width
                    && o.kerf.is_none_or(parted_by)
            })
        };
        // When every through cut on the sheet is a profile round closed
        // vectors (outside, on the line, or inside for the holes), the parts
        // lie inside the vectors cut outside or on: a region outside all of
        // them (the slug left in a finger joint's notch between two kerfs, a
        // sliver between two parts laid out closer than two tools) is waste
        // even where the space round it is open.
        let owned = outlines.iter().any(|o| !o.hole)
            && cutting
                .iter()
                .all(|c| outlines.iter().any(|o| o.owner == *c));
        // Some two dozen cells spread evenly through each free region.
        let mut samples: Vec<Vec<usize>> = vec![Vec::new(); found.len()];
        if owned {
            let mut seen = vec![0usize; found.len()];
            for (i, r) in region.iter().enumerate() {
                if free(*r) {
                    let stride = (found[*r].0 / 24).max(1);
                    if seen[*r] % stride == stride / 2 {
                        samples[*r].push(i);
                    }
                    seen[*r] += 1;
                }
            }
        }
        let tol = crate::geom::Tolerance::new(0.01).ok();
        let inside_outline = |r: usize| {
            let (Some(tol), true) = (tol, owned) else {
                return true;
            };
            let list = &samples[r];
            let inside = list
                .iter()
                .filter(|&&i| {
                    let p = crate::geom::Point::new(
                        stock.origin_x + (i % w) as f64 * cell,
                        stock.origin_y + (i / w) as f64 * cell,
                    );
                    outlines
                        .iter()
                        .any(|o| !o.hole && o.region.contains(p, tol))
                })
                .count();
            list.is_empty() || inside * 2 >= list.len()
        };
        // Parts in the order their first cell turns up, row by row.
        let mut part_of_region = vec![0u32; found.len()];
        let mut boxes: Vec<[usize; 4]> = Vec::new();
        let mut anchors: Vec<Anchor> = Vec::new();
        for r in 0..found.len() {
            if free(r) && (touches[r] || fits_outline(r)) && inside_outline(r) {
                let n = found[r].0 as f64;
                boxes.push(bounds[r]);
                anchors.push(Anchor {
                    centre: [
                        stock.origin_x + sums[r][0] / n * cell,
                        stock.origin_y + sums[r][1] / n * cell,
                    ],
                    area: n * cell * cell,
                });
                part_of_region[r] = boxes.len() as u32;
            }
        }
        if boxes.is_empty() {
            let labels: Vec<u32> = (0..w * h)
                .map(|i| u32::from(is_material(&stock, i)))
                .collect();
            let anchor = anchor_of(&stock, &labels, 1);
            return SheetParts {
                sheet,
                stock,
                labels,
                boxes: vec![[0, 0, w, h]],
                anchors: vec![anchor],
                whole: true,
                thin: BTreeMap::new(),
            };
        }
        let labels = region
            .iter()
            .map(|r| part_of_region.get(*r).copied().unwrap_or(0))
            .collect();
        SheetParts {
            sheet,
            stock,
            labels,
            boxes,
            anchors,
            whole: false,
            thin: BTreeMap::new(),
        }
    }

    /// A sheet's raw model as its one part: every cell the model rises
    /// above its base, cropped to the box round them; `None` when the
    /// model is flat. The model's heights run from `stock.bottom` up.
    pub fn from_model(sheet: u64, stock: Stock) -> Option<SheetParts> {
        let (w, h) = (stock.width, stock.height);
        let labels: Vec<u32> = (0..w * h)
            .map(|i| u32::from(is_material(&stock, i)))
            .collect();
        let mut b = [usize::MAX, usize::MAX, 0, 0];
        for (i, label) in labels.iter().enumerate() {
            if *label == 1 {
                let (x, y) = (i % w, i / w);
                b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
            }
        }
        if b[0] == usize::MAX {
            return None;
        }
        let anchor = anchor_of(&stock, &labels, 1);
        Some(SheetParts {
            sheet,
            stock,
            labels,
            boxes: vec![b],
            anchors: vec![anchor],
            whole: false,
            thin: BTreeMap::new(),
        })
    }

    /// Where part `part`'s material lies on the sheet.
    pub fn anchor(&self, part: u32) -> Option<Anchor> {
        let index = usize::try_from(part.checked_sub(1)?).ok()?;
        self.anchors.get(index).copied()
    }

    /// The part at a job X and Y (from 1), zero for none.
    pub fn part_at(&self, x: f64, y: f64) -> u32 {
        let s = &self.stock;
        let cx = ((x - s.origin_x) / s.cell).round();
        let cy = ((y - s.origin_y) / s.cell).round();
        if cx < 0.0 || cy < 0.0 || cx >= s.width as f64 || cy >= s.height as f64 {
            return 0;
        }
        self.labels
            .get(cy as usize * s.width + cx as usize)
            .copied()
            .unwrap_or(0)
    }

    /// The part's thickness: the sheet's, or less for a part set thinner.
    pub fn thickness(&self, part: u32) -> f64 {
        let full = self.stock.top - self.stock.bottom;
        self.thin.get(&part).map_or(full, |t| t.min(full))
    }

    /// Gives part `part` its true thickness `t` (millimetres, from the top
    /// of the sheet down) when that is less than the sheet's: a joint's
    /// key or wedge cut out here as if from thinner stock.
    pub fn set_thickness(&mut self, part: u32, t: f64) {
        let full = self.stock.top - self.stock.bottom;
        if part >= 1 && part <= self.count() && t > 0.0 && t < full - 1e-6 {
            self.thin.insert(part, t);
        }
    }

    /// How many parts the sheet has (one at least).
    pub fn count(&self) -> u32 {
        self.boxes.len() as u32
    }

    fn cell_box(&self, part: u32) -> Option<[usize; 4]> {
        let index = usize::try_from(part.checked_sub(1)?).ok()?;
        self.boxes.get(index).copied()
    }

    /// The part's box: width, height and thickness in millimetres, or
    /// `None` for a part the sheet does not have.
    pub fn size(&self, part: u32) -> Option<Vec3> {
        let [x0, y0, x1, y1] = self.cell_box(part)?;
        let s = &self.stock;
        Some([
            (x1 - x0) as f64 * s.cell,
            (y1 - y0) as f64 * s.cell,
            self.thickness(part),
        ])
    }

    /// The middle of the part's box in the sheet's material frame (X and Y
    /// from the material's lower left corner, Z up from its underside).
    pub fn middle(&self, part: u32) -> Option<Vec3> {
        let [x0, y0, x1, y1] = self.cell_box(part)?;
        let s = &self.stock;
        Some([
            (x0 + x1) as f64 / 2.0 * s.cell,
            (y0 + y1) as f64 / 2.0 * s.cell,
            (s.top - s.bottom) - self.thickness(part) / 2.0,
        ])
    }

    /// The part's box in job coordinates: min X, min Y, max X, max Y.
    pub fn job_box(&self, part: u32) -> Option<[f64; 4]> {
        let [x0, y0, x1, y1] = self.cell_box(part)?;
        let s = &self.stock;
        let left = s.origin_x - s.cell / 2.0;
        let low = s.origin_y - s.cell / 2.0;
        Some([
            left + x0 as f64 * s.cell,
            low + y0 as f64 * s.cell,
            left + x1 as f64 * s.cell,
            low + y1 as f64 * s.cell,
        ])
    }

    /// The part's relief: the simulation cropped to the part's box, every
    /// cell outside the part (waste, tabs, other parts) cut through; a part
    /// set thinner keeps only its thickness from the top.
    pub fn part_stock(&self, part: u32) -> Option<Stock> {
        let [x0, y0, x1, y1] = self.cell_box(part)?;
        let s = &self.stock;
        let (w, h) = (x1 - x0, y1 - y0);
        let under = s.top - self.thickness(part);
        let bottom = under as f32;
        let mut out = Stock {
            width: w,
            height: h,
            cell: s.cell,
            origin_x: s.origin_x + x0 as f64 * s.cell,
            origin_y: s.origin_y + y0 as f64 * s.cell,
            top: s.top,
            bottom: under,
            z: Vec::with_capacity(w * h),
            floor: Vec::with_capacity(w * h),
            owner: Vec::with_capacity(w * h),
            burn: Vec::new(),
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * s.width + x;
                if self.labels[i] == part {
                    out.z.push(s.z[i].max(bottom));
                    out.floor.push(s.floor[i].max(bottom));
                    out.owner.push(s.owner[i]);
                } else {
                    out.z.push(bottom);
                    out.floor.push(bottom);
                    out.owner.push(0);
                }
            }
        }
        Some(out)
    }
}

/// The parts a joint makes on `sheet` from thinner stock than the sheet
/// (a key, a wedge): a point inside each (its box's middle) and its
/// thickness, in millimetres.
fn thin_parts(document: &Document, sheet: u64) -> Vec<([f64; 2], f64)> {
    let mut out = Vec::new();
    for joint in &document.assembly().joint_set.joints {
        for made in &joint.vectors {
            let crate::joints::Role::Part { thickness } = made.role else {
                continue;
            };
            if document.entity(made.id).map_or(true, |e| e.sheet != sheet) {
                continue;
            }
            if let Some(b) = document.bbox_of(made.id) {
                let at = [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0];
                out.push((at, thickness));
            }
        }
    }
    out
}

/// A hash of everything a sheet's simulation depends on: its material and
/// the moves and tools of the toolpaths that cut it.
fn fingerprint(document: &Document, sheet: u64, cells: f64) -> Result<u64, CoreError> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sheet.hash(&mut h);
    cells.to_bits().hash(&mut h);
    let job = document
        .sheet_material(sheet)?
        .applied_to(document.job_setup());
    serde_json::to_string(&job).unwrap_or_default().hash(&mut h);
    for (position, id) in document.toolpath_order().iter().enumerate() {
        let Ok(toolpath) = document.toolpath(*id) else {
            continue;
        };
        if !toolpath.visible || (toolpath.sheet != sheet && toolpath.sheet != 0) {
            continue;
        }
        (position, *id).hash(&mut h);
        format!("{:?}|{:?}", toolpath.side, toolpath.tool).hash(&mut h);
        toolpath.moves.len().hash(&mut h);
        for m in &toolpath.moves {
            std::mem::discriminant(&m.kind).hash(&mut h);
            if let MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } = m.kind {
                (centre.x.to_bits(), centre.y.to_bits()).hash(&mut h);
            }
            (m.to.x.to_bits(), m.to.y.to_bits(), m.to.z.to_bits()).hash(&mut h);
        }
    }
    Ok(h.finish())
}

/// A sheet's raw model as a relief: the visible components on the sheet
/// baked on its model grid, standing on a flat base at the model's lowest
/// point (its zero plane, or lower where the model dips below it).
/// `None` when the model is flat.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown sheet.
pub fn model_stock(document: &Document, sheet: u64) -> Result<Option<Stock>, CoreError> {
    model_stock_except(document, sheet, &[])
}

/// As [`model_stock`] without the components `except` (the ones that are
/// pieces of their own).
///
/// # Errors
/// [`CoreError::Validation`] for an unknown sheet.
pub fn model_stock_except(
    document: &Document,
    sheet: u64,
    except: &[u64],
) -> Result<Option<Stock>, CoreError> {
    Ok(relief_stock(
        &document.sheet_composite_except(sheet, except)?,
    ))
}

/// One component standing alone (see [`Document::component_alone`]) as a
/// relief on its sheet's model grid, with its sheet; `None` for a flat
/// component.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown component.
pub fn component_stock(document: &Document, id: u64) -> Result<(u64, Option<Stock>), CoreError> {
    let (sheet, field) = document.component_alone(id)?;
    Ok((sheet, relief_stock(&field)))
}

/// A model field as a relief standing on a flat base at its lowest point
/// (zero, or lower where it dips below); `None` when it is flat.
fn relief_stock(field: &crate::model::field::Field) -> Option<Stock> {
    let finite = || field.z.iter().copied().filter(|z| z.is_finite());
    let base = finite().fold(0.0_f32, f32::min);
    let top = finite().fold(base, f32::max);
    if top - base <= THROUGH {
        return None;
    }
    let z: Vec<f32> = field
        .z
        .iter()
        .map(|z| if z.is_finite() { *z } else { base })
        .collect();
    let cells = z.len();
    Some(Stock {
        width: field.width,
        height: field.height,
        cell: field.cell,
        origin_x: field.origin.x,
        origin_y: field.origin.y,
        top: f64::from(top),
        bottom: f64::from(base),
        z,
        floor: vec![base; cells],
        owner: vec![0; cells],
        burn: Vec::new(),
    })
}

/// Whether a sheet has any toolpath that cuts it.
fn has_toolpaths(document: &Document, sheet: u64) -> bool {
    document
        .toolpaths()
        .iter()
        .any(|t| t.sheet == sheet || t.sheet == 0)
}

/// The parts found last for each sheet, kept while nothing they depend on
/// changes (placing a piece does not), so listing, reliefs, the check and
/// the cut list simulate each sheet once; likewise each sheet's raw model.
/// Not saved.
#[derive(Default)]
pub struct PartsCache {
    entries: BTreeMap<u64, (u64, Arc<SheetParts>)>,
    models: BTreeMap<u64, (u64, Option<Arc<SheetParts>>)>,
    /// Each sheet's raw model as it would be without one more component
    /// (the one being dropped), apart from `models` so the two do not
    /// push each other out.
    leaving: BTreeMap<u64, (u64, Option<Arc<SheetParts>>)>,
    components: BTreeMap<u64, (u64, Option<Arc<SheetParts>>)>,
}

impl std::fmt::Debug for PartsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartsCache")
            .field("sheets", &self.entries.len())
            .field("models", &self.models.len())
            .field("components", &self.components.len())
            .finish()
    }
}

impl PartsCache {
    /// The parts of `sheet` simulated on about `cells` cells.
    ///
    /// # Errors
    /// As [`simulate`].
    pub fn get(
        &mut self,
        document: &Document,
        sheet: u64,
        cells: f64,
    ) -> Result<Arc<SheetParts>, CoreError> {
        let thin = thin_parts(document, sheet);
        let key = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            fingerprint(document, sheet, cells)?.hash(&mut h);
            for (at, t) in &thin {
                (at[0].to_bits(), at[1].to_bits(), t.to_bits()).hash(&mut h);
            }
            h.finish()
        };
        if let Some((k, parts)) = self.entries.get(&sheet) {
            if *k == key {
                return Ok(Arc::clone(parts));
            }
        }
        let outlines = outlines(document, sheet);
        let mut found = SheetParts::find_with(sheet, simulate(document, sheet, cells)?, &outlines);
        for (at, t) in thin {
            let part = found.part_at(at[0], at[1]);
            found.set_thickness(part, t);
        }
        let parts = Arc::new(found);
        self.entries.insert(sheet, (key, Arc::clone(&parts)));
        Ok(parts)
    }

    /// The raw model of `sheet` without the components `except` (pieces
    /// of their own) as its one part, `None` when it is flat.
    ///
    /// # Errors
    /// As [`model_stock`].
    pub fn model(
        &mut self,
        document: &Document,
        sheet: u64,
        except: &[u64],
    ) -> Result<Option<Arc<SheetParts>>, CoreError> {
        Self::model_in(&mut self.models, document, sheet, except)
    }

    /// As [`PartsCache::model`], kept apart: the model a component being
    /// dropped leaves when it becomes a piece.
    ///
    /// # Errors
    /// As [`model_stock_except`].
    pub fn model_leaving(
        &mut self,
        document: &Document,
        sheet: u64,
        except: &[u64],
    ) -> Result<Option<Arc<SheetParts>>, CoreError> {
        Self::model_in(&mut self.leaving, document, sheet, except)
    }

    fn model_in(
        slot: &mut BTreeMap<u64, (u64, Option<Arc<SheetParts>>)>,
        document: &Document,
        sheet: u64,
        except: &[u64],
    ) -> Result<Option<Arc<SheetParts>>, CoreError> {
        let mut key = document.sheet_model_fingerprint(sheet);
        for id in except {
            key = (key ^ id).wrapping_mul(0x0100_0000_01b3).rotate_left(7);
        }
        if let Some((k, parts)) = slot.get(&sheet) {
            if *k == key {
                return Ok(parts.clone());
            }
        }
        let parts = model_stock_except(document, sheet, except)?
            .and_then(|stock| SheetParts::from_model(sheet, stock))
            .map(Arc::new);
        slot.insert(sheet, (key, parts.clone()));
        Ok(parts)
    }

    /// Component `id` standing alone as its one part (on its sheet's
    /// grid), `None` when it is flat.
    ///
    /// # Errors
    /// As [`component_stock`].
    pub fn component(
        &mut self,
        document: &Document,
        id: u64,
    ) -> Result<Option<Arc<SheetParts>>, CoreError> {
        let key = document.component_fingerprint(id);
        if let Some((k, parts)) = self.components.get(&id) {
            if *k == key {
                return Ok(parts.clone());
            }
        }
        let (sheet, stock) = component_stock(document, id)?;
        let parts = stock
            .and_then(|stock| SheetParts::from_model(sheet, stock))
            .map(Arc::new);
        self.components.insert(id, (key, parts.clone()));
        Ok(parts)
    }
}

/// A piece with its pose, its part box and how many parts its sheet has.
#[derive(Debug, Clone)]
pub struct PieceInfo {
    /// Where it sits; its part is [`MODEL_PART`] for a raw model piece.
    pub piece: Piece,
    /// Its part box: width, height, thickness in millimetres.
    pub size: Vec3,
    /// Parts on its sheet.
    pub parts: u32,
    /// The sheet's parts (the model alone for a raw model piece).
    pub sheet_parts: Arc<SheetParts>,
    /// The piece's part in `sheet_parts` (1 for a raw model piece).
    pub local: u32,
    /// The stored placement it takes, an index into the assembly's pieces.
    pub stored: Option<usize>,
    /// Whether its sheet has a raw model to use instead of its simulation.
    pub has_model: bool,
    /// Whether its sheet has toolpaths to simulate.
    pub has_toolpaths: bool,
}

impl PieceInfo {
    /// Whether the piece is its sheet's raw model.
    pub fn is_model(&self) -> bool {
        self.piece.part == MODEL_PART && self.piece.component.is_none()
    }

    /// The piece's relief: its part cropped out of the sheet.
    pub fn stock(&self) -> Option<Stock> {
        self.sheet_parts.part_stock(self.local)
    }
}

/// Which stored placement each of a sheet's parts takes: the placement
/// whose anchor its part fits best, pairs taken best first, the saved part
/// number only breaking ties; placements saved without an anchor (older
/// files) go by their part number. `numbers[i]` is the part number of the
/// `i`th part, `anchors[i]` its anchor.
pub fn match_stored(
    stored: &[Piece],
    sheet: u64,
    numbers: &[u32],
    anchors: &[Anchor],
    cell: f64,
) -> Vec<Option<usize>> {
    let mut taken = vec![None; numbers.len()];
    let mut used = vec![false; stored.len()];
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (s, piece) in stored.iter().enumerate() {
        if piece.sheet != sheet || piece.component.is_some() {
            continue;
        }
        let Some(saved) = piece.anchor else {
            continue;
        };
        for (i, (number, anchor)) in numbers.iter().zip(anchors).enumerate() {
            if (piece.part == MODEL_PART) != (*number == MODEL_PART) {
                continue;
            }
            if let Some(cost) = saved.misfit(anchor, cell) {
                let hint = if piece.part == *number { 1e-9 } else { 0.0 };
                pairs.push((cost - hint, s, i));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, s, i) in pairs {
        if !used[s] && taken[i].is_none() {
            used[s] = true;
            taken[i] = Some(s);
        }
    }
    for (s, piece) in stored.iter().enumerate() {
        if used[s] || piece.sheet != sheet || piece.anchor.is_some() || piece.component.is_some() {
            continue;
        }
        if let Some(i) = numbers.iter().position(|n| *n == piece.part) {
            if taken[i].is_none() {
                used[s] = true;
                taken[i] = Some(s);
            }
        }
    }
    taken
}

/// The piece a saved reference (a mate end) names now, an index into
/// `infos`: a component piece by its component, a raw model piece by its
/// sheet, a part by `anchor` (the part of the sheet that fits it best, the
/// saved number only breaking ties) or, saved without one (older files),
/// by its number. `None` when that piece is gone.
pub fn follow(key: PieceRef, anchor: Option<&Anchor>, infos: &[PieceInfo]) -> Option<usize> {
    if let Some(component) = key.component {
        return infos
            .iter()
            .position(|info| info.piece.component == Some(component));
    }
    let Some(saved) = anchor.filter(|_| key.part != MODEL_PART) else {
        return infos.iter().position(|info| info.piece.key() == key);
    };
    infos
        .iter()
        .enumerate()
        .filter(|(_, info)| {
            info.piece.sheet == key.sheet && info.piece.component.is_none() && !info.is_model()
        })
        .filter_map(|(at, info)| {
            let now = info.sheet_parts.anchor(info.local)?;
            let cost = saved.misfit(&now, info.sheet_parts.stock.cell)?;
            let hint = if info.piece.part == key.part {
                1e-9
            } else {
                0.0
            };
            Some((cost - hint, at))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, at)| at)
}

/// Every piece with its pose: the stored one, or else where its part lies
/// on its sheet, the sheets laid flat in a row along X. A sheet gives its
/// simulation's parts, or its raw model when it has no toolpaths or is set
/// to give its model; then each component added as a piece of its own on
/// that sheet (those components are left out of the sheet's raw model).
///
/// # Errors
/// As [`PartsCache::get`].
pub fn pieces(
    document: &Document,
    cache: &mut PartsCache,
    cells: f64,
) -> Result<Vec<PieceInfo>, CoreError> {
    let assembly = document.assembly();
    let stored = &assembly.pieces;
    let mut along = 0.0;
    let mut out = Vec::new();
    for sheet in document.sheets().iter().map(|s| s.id) {
        let before = out.len();
        let own: Vec<u64> = assembly
            .components
            .iter()
            .copied()
            .filter(|id| {
                document
                    .component(*id)
                    .is_ok_and(|c| document.sheet_of_component(c) == sheet)
            })
            .collect();
        let has_toolpaths = has_toolpaths(document, sheet);
        let model = cache.model(document, sheet, &own)?;
        let wants_model = !has_toolpaths || assembly.model_sheets.contains(&sheet);
        let chosen = match (&model, wants_model) {
            (Some(model), true) => Some((Arc::clone(model), vec![MODEL_PART])),
            _ if has_toolpaths => {
                let parts = cache.get(document, sheet, cells)?;
                let numbers = (1..=parts.count()).collect();
                Some((parts, numbers))
            }
            _ => None,
        };
        if let Some((parts, numbers)) = chosen {
            let anchors: Vec<Anchor> = (1..=parts.count())
                .filter_map(|local| parts.anchor(local))
                .collect();
            let taken = match_stored(stored, sheet, &numbers, &anchors, parts.stock.cell);
            for (index, number) in numbers.iter().enumerate() {
                let local = index as u32 + 1;
                let (Some(size), Some(middle)) = (parts.size(local), parts.middle(local)) else {
                    continue;
                };
                let laid = Piece {
                    sheet,
                    part: *number,
                    position: [along + middle[0], middle[1], middle[2]],
                    rotation: [0.0; 3],
                    anchor: None,
                    component: None,
                    hidden: false,
                    grounded: false,
                };
                let at = taken.get(index).copied().flatten();
                let piece = at.and_then(|s| stored.get(s)).map_or(laid, |p| Piece {
                    part: *number,
                    ..*p
                });
                out.push(PieceInfo {
                    piece,
                    size,
                    parts: numbers.len() as u32,
                    sheet_parts: Arc::clone(&parts),
                    local,
                    stored: at,
                    has_model: model.is_some(),
                    has_toolpaths,
                });
            }
        }
        for id in own {
            let Some(parts) = cache.component(document, id)? else {
                continue;
            };
            let (Some(size), Some(middle)) = (parts.size(1), parts.middle(1)) else {
                continue;
            };
            let at = stored.iter().position(|p| p.component == Some(id));
            let piece = at.and_then(|s| stored.get(s)).copied().map_or(
                Piece {
                    sheet,
                    part: 1,
                    position: [along + middle[0], middle[1], middle[2]],
                    rotation: [0.0; 3],
                    anchor: None,
                    component: Some(id),
                    hidden: false,
                    grounded: false,
                },
                |p| Piece {
                    sheet,
                    part: 1,
                    ..p
                },
            );
            out.push(PieceInfo {
                piece,
                size,
                parts: 1,
                sheet_parts: parts,
                local: 1,
                stored: at,
                has_model: false,
                has_toolpaths: false,
            });
        }
        if out.len() > before {
            along += material_size(document, sheet)?[0] + LAYOUT_GAP;
        }
    }
    super::joint_mates::settle(assembly, &mut out);
    Ok(out)
}

/// Whether component `id` stands up from its base, so it can be a piece.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown component.
pub fn component_has_relief(
    document: &Document,
    cache: &mut PartsCache,
    id: u64,
) -> Result<bool, CoreError> {
    Ok(cache.component(document, id)?.is_some())
}

/// The box round a piece of part box `size` where it sits, assembly
/// space: min then max corner.
pub fn piece_bounds(piece: &Piece, size: Vec3) -> [Vec3; 2] {
    let pose = piece.pose(size);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for corner in 0..8 {
        let p = pose.apply([
            if corner & 1 == 0 { 0.0 } else { size[0] },
            if corner & 2 == 0 { 0.0 } else { size[1] },
            if corner & 4 == 0 { 0.0 } else { size[2] },
        ]);
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    [lo, hi]
}

/// Where an unturned piece of part box `size` sits with the middle of its
/// underside on `at`: resting on the surface or the ground there.
pub fn resting_at(size: Vec3, at: Vec3) -> Vec3 {
    [at[0], at[1], at[2] + size[2] / 2.0]
}

/// Where an unturned piece of part box `size` stands beside the pieces in
/// `boxes` (min and max corners): [`LAYOUT_GAP`] past the far side of them
/// all along X, centred on them along Y, on the lowest of their undersides;
/// at the origin (its box's corner there) when there are none.
pub fn beside(boxes: &[[Vec3; 2]], size: Vec3) -> Vec3 {
    let half = pose::scale(size, 0.5);
    let Some(first) = boxes.first() else {
        return half;
    };
    let (mut lo, mut hi) = (first[0], first[1]);
    for [a, b] in boxes {
        for k in 0..3 {
            lo[k] = lo[k].min(a[k]);
            hi[k] = hi[k].max(b[k]);
        }
    }
    [
        hi[0] + LAYOUT_GAP + half[0],
        (lo[1] + hi[1]) / 2.0,
        lo[2] + half[2],
    ]
}

/// The name of the vector a part was cut from, when one fits: a named
/// object on the part's sheet whose box matches the part's box to within
/// about a tool radius (a profile outside, on or inside the vector).
pub fn vector_name(document: &Document, parts: &SheetParts, part: u32) -> Option<String> {
    let [x0, y0, x1, y1] = parts.job_box(part)?;
    let tolerance = (parts.stock.cell * 3.0).max(8.0);
    let mut best: Option<(f64, String)> = None;
    for entity in document.entities().values() {
        let Some(name) = entity.name.as_ref().filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        if entity.sheet != parts.sheet {
            continue;
        }
        let Some(bbox) = document.bbox_of(entity.id) else {
            continue;
        };
        let miss = [
            (bbox.min.x - x0).abs(),
            (bbox.min.y - y0).abs(),
            (bbox.max.x - x1).abs(),
            (bbox.max.y - y1).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max);
        if miss <= tolerance && best.as_ref().is_none_or(|(m, _)| miss < *m) {
            best = Some((miss, name.clone()));
        }
    }
    best.map(|(_, name)| name)
}

/// The span of the material left in a relief, in its local frame (X and Y
/// from its lower left corner): min then max corner in X and Y, or `None`
/// when nothing is left.
pub fn material_span(stock: &Stock) -> Option<[[f64; 2]; 2]> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for y in 0..stock.height {
        for x in 0..stock.width {
            if is_material(stock, y * stock.width + x) {
                let (px, py) = (x as f64 * stock.cell, y as f64 * stock.cell);
                lo = [lo[0].min(px), lo[1].min(py)];
                hi = [hi[0].max(px + stock.cell), hi[1].max(py + stock.cell)];
            }
        }
    }
    (lo[0] <= hi[0]).then_some([lo, hi])
}

/// A face picked on a piece: a point on it and the way it faces, in the
/// piece's local frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickedFace {
    /// A point on the face, millimetres.
    pub point: Vec3,
    /// Unit normal pointing out of the material.
    pub normal: Vec3,
}

/// The face of a relief at a local point picked on its surface: the top
/// or the underside where the point lies at the material's height there,
/// else a wall (the outline, a hole or a pocket side), facing away from the
/// material round the point at that height. `None` when no material is
/// near the point.
pub fn face_at(stock: &Stock, local: Vec3) -> Option<PickedFace> {
    let s = stock;
    let cell = s.cell;
    let (fx, fy) = (local[0] / cell, local[1] / cell);
    let height = |i: usize| {
        (
            f64::from(s.z[i]) - s.bottom,
            f64::from(s.floor[i]) - s.bottom,
        )
    };
    // The material cell nearest the point.
    let mut nearest: Option<(f64, usize)> = None;
    let (cx, cy) = (fx.floor() as i64, fy.floor() as i64);
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= s.width as i64 || y >= s.height as i64 {
                continue;
            }
            let i = y as usize * s.width + x as usize;
            if !is_material(s, i) {
                continue;
            }
            let ox = (x as f64 - fx).max(fx - (x + 1) as f64).max(0.0);
            let oy = (y as f64 - fy).max(fy - (y + 1) as f64).max(0.0);
            let d = ox.hypot(oy);
            if nearest.is_none_or(|(best, _)| d < best) {
                nearest = Some((d, i));
            }
        }
    }
    let (_, i) = nearest?;
    let (top, under) = height(i);
    let tolerance = (cell * 0.75).max(0.05);
    let flat = |z: f64, up: bool| PickedFace {
        point: [local[0], local[1], z],
        normal: [0.0, 0.0, if up { 1.0 } else { -1.0 }],
    };
    if (local[2] - top).abs() <= tolerance {
        return Some(flat(top, true));
    }
    if (local[2] - under).abs() <= tolerance {
        return Some(flat(under, false));
    }
    // A wall: away from the middle of the material round the point at the
    // point's height.
    let reach = (3.0 / cell).max(4.0);
    let r = reach.ceil() as i64;
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= s.width as i64 || y >= s.height as i64 {
                continue;
            }
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            if (px - fx).hypot(py - fy) > reach {
                continue;
            }
            let j = y as usize * s.width + x as usize;
            let (t, u) = height(j);
            if is_material(s, j) && u <= local[2] && local[2] <= t {
                sx += px;
                sy += py;
                n += 1.0;
            }
        }
    }
    if n == 0.0 {
        return Some(flat(top, true));
    }
    let (ax, ay) = (fx - sx / n, fy - sy / n);
    if ax.hypot(ay) < 0.1 {
        return Some(flat(top, true));
    }
    let mut angle = ay.atan2(ax).to_degrees();
    let square = (angle / 90.0).round() * 90.0;
    if (angle - square).abs() <= AXIS_SNAP_DEGREES {
        angle = square;
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    let normal = [clean(cos), clean(sin), 0.0];
    // The wall itself: the nearest place along the normal where the
    // material at the point's height ends.
    let inside = |along: f64| {
        let (x, y) = (
            (local[0] + normal[0] * along) / cell,
            (local[1] + normal[1] * along) / cell,
        );
        if x < 0.0 || y < 0.0 || x >= s.width as f64 || y >= s.height as f64 {
            return false;
        }
        let j = y as usize * s.width + x as usize;
        let (t, u) = height(j);
        is_material(s, j) && u <= local[2] && local[2] <= t
    };
    let step = cell / 20.0;
    let mut wall: Option<f64> = None;
    for k in -40..40 {
        let along = f64::from(k) * step;
        if inside(along) && !inside(along + step) {
            let at = along + step / 2.0;
            if wall.is_none_or(|w| at.abs() < w.abs()) {
                wall = Some(at);
            }
        }
    }
    let along = wall.unwrap_or(0.0);
    Some(PickedFace {
        point: [
            local[0] + normal[0] * along,
            local[1] + normal[1] * along,
            local[2],
        ],
        normal,
    })
}

/// A direction along a face, the same for every face with that normal: the
/// local X axis on the top and underside, the thickness (Z) on a wall.
fn face_tangent(normal: Vec3) -> Vec3 {
    let guide = if normal[2].abs() > 0.7 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    pose::normalized(pose::sub(
        guide,
        pose::scale(normal, pose::dot(guide, normal)),
    ))
}

/// How a mate lays one face against the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MateOptions {
    /// Gap left between the faces along the target face's normal, mm
    /// (negative sinks the moving piece in).
    pub offset: f64,
    /// Turn of the moving piece about the face normal, degrees; `None`
    /// takes the quarter turn that keeps it closest to how it stands.
    pub turn: Option<f64>,
}

/// Places `moving` so that its `face` meets the `target` face of a piece
/// posed at `target_pose`: the picked points meet (plus the offset along
/// the target normal) and the faces look at each other. Turning the piece
/// over (Flip) is mating the face opposite the picked one, see
/// [`opposite_face`]. Answers the new placement and the turn it used.
pub fn mate(
    target_pose: &Pose,
    target: &PickedFace,
    moving: &Piece,
    moving_size: Vec3,
    face: &PickedFace,
    options: MateOptions,
) -> (Piece, f64) {
    let at = target_pose.apply(target.point);
    let normal = pose::normalized(target_pose.apply_vector(target.normal));
    let along = pose::normalized(target_pose.apply_vector(face_tangent(target.normal)));
    let facing = pose::scale(normal, -1.0);
    let m = pose::normalized(face.normal);
    let um = face_tangent(m);
    let vm = pose::cross(m, um);
    let rotation_for = |turn: f64| {
        let u = Pose::rotation(facing, turn).apply_vector(along);
        let v = pose::cross(facing, u);
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = facing[i] * m[j] + u[i] * um[j] + v[i] * vm[j];
            }
        }
        Pose { r, t: [0.0; 3] }
    };
    let turn = options.turn.unwrap_or_else(|| {
        let now = Pose::euler(moving.rotation);
        let score = |turn: f64| {
            let r = rotation_for(turn).r;
            (0..3)
                .flat_map(|i| (0..3).map(move |j| (i, j)))
                .map(|(i, j)| r[i][j] * now.r[i][j])
                .sum::<f64>()
        };
        [0.0, 90.0, 180.0, 270.0]
            .into_iter()
            .fold((0.0, f64::NEG_INFINITY), |best, turn| {
                let s = score(turn);
                if s > best.1 + 1e-9 {
                    (turn, s)
                } else {
                    best
                }
            })
            .0
    });
    let turned = rotation_for(turn);
    let lever = turned.apply_vector(pose::sub(face.point, pose::scale(moving_size, 0.5)));
    let meet = pose::add(at, pose::scale(normal, options.offset));
    let position = pose::sub(meet, lever).map(|v| (v * 1e9).round() / 1e9);
    (
        Piece {
            sheet: moving.sheet,
            part: moving.part,
            hidden: moving.hidden,
            grounded: moving.grounded,
            position,
            rotation: turned.euler_angles(),
            anchor: moving.anchor,
            component: moving.component,
        },
        turn,
    )
}

/// The face on the other side of the material from `face`, through the
/// point: the underside under a top face (and the top over the underside),
/// the far wall behind a wall. It faces the other way. Mating it in place
/// of the picked face turns the moving piece over, so its other side meets
/// the target.
pub fn opposite_face(stock: &Stock, face: &PickedFace) -> PickedFace {
    let s = stock;
    let cell = s.cell;
    let back = pose::scale(face.normal, -1.0);
    let local = face.point;
    let column = |x: f64, y: f64| {
        let (fx, fy) = (x / cell, y / cell);
        if fx < 0.0 || fy < 0.0 || fx >= s.width as f64 || fy >= s.height as f64 {
            return None;
        }
        let i = fy as usize * s.width + fx as usize;
        is_material(s, i).then(|| {
            (
                f64::from(s.z[i]) - s.bottom,
                f64::from(s.floor[i]) - s.bottom,
            )
        })
    };
    if face.normal[2].abs() > 0.7 {
        // Top or underside: the other surface of the material there, or of
        // the nearest cell that has material.
        let heights = column(local[0], local[1])
            .or_else(|| face_at(s, local).and_then(|f| column(f.point[0], f.point[1])));
        let (top, under) = heights.unwrap_or((s.top - s.bottom, 0.0));
        let z = if face.normal[2] > 0.0 { under } else { top };
        return PickedFace {
            point: [local[0], local[1], z],
            normal: back,
        };
    }
    // A wall: walk into the material until it ends at the point's height.
    let inside = |along: f64| {
        let (x, y) = (local[0] + back[0] * along, local[1] + back[1] * along);
        column(x, y).is_some_and(|(t, u)| u <= local[2] && local[2] <= t)
    };
    let step = cell / 4.0;
    let reach = (s.width as f64).hypot(s.height as f64) * cell;
    let mut along = step / 2.0;
    let mut last = 0.0;
    while along <= reach {
        if inside(along) {
            last = along;
        } else if along - last > cell * 1.5 {
            break;
        }
        along += step;
    }
    let depth = if last > 0.0 { last + step / 2.0 } else { 0.0 };
    PickedFace {
        point: [
            local[0] + back[0] * depth,
            local[1] + back[1] * depth,
            local[2],
        ],
        normal: back,
    }
}

/// A relief placed in assembly space.
pub struct PlacedRelief<'a> {
    /// The piece it is.
    pub key: PieceRef,
    /// The relief, everything but its part cut away.
    pub stock: &'a Stock,
    /// Local frame to assembly space.
    pub pose: Pose,
}

impl PlacedRelief<'_> {
    /// Whether a local point lies inside the material, at least `inset`
    /// from its top and underside.
    fn contains(&self, local: Vec3, inset: f64) -> bool {
        let s = self.stock;
        let (fx, fy) = (local[0] / s.cell, local[1] / s.cell);
        if fx < 0.0 || fy < 0.0 {
            return false;
        }
        let (x, y) = (fx as usize, fy as usize);
        if x >= s.width || y >= s.height {
            return false;
        }
        let i = y * s.width + x;
        if !is_material(s, i) {
            return false;
        }
        let z = local[2] + s.bottom;
        z > f64::from(s.floor[i]) + inset && z < f64::from(s.z[i]) - inset
    }

    /// Whether a local point lies inside the material with a margin of a
    /// fraction of a cell round it in X and Y, so points of a piece that
    /// only touches this one stay outside.
    fn contains_inset(&self, local: Vec3) -> bool {
        let inset = self.stock.cell * TOUCH_CELLS;
        [
            [0.0, 0.0],
            [inset, 0.0],
            [-inset, 0.0],
            [0.0, inset],
            [0.0, -inset],
        ]
        .iter()
        .all(|[dx, dy]| self.contains([local[0] + dx, local[1] + dy, local[2]], inset))
    }

    /// Corners of the relief's box in assembly space, min then max.
    pub fn bounds(&self) -> [Vec3; 2] {
        let s = self.stock;
        let size = [
            s.width as f64 * s.cell,
            s.height as f64 * s.cell,
            s.top - s.bottom,
        ];
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for corner in 0..8 {
            let p = self.pose.apply([
                if corner & 1 == 0 { 0.0 } else { size[0] },
                if corner & 2 == 0 { 0.0 } else { size[1] },
                if corner & 4 == 0 { 0.0 } else { size[2] },
            ]);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        [lo, hi]
    }
}

/// Two pieces that run into each other or come closer than the clearance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceIssue {
    /// First piece.
    pub a: PieceRef,
    /// Second piece.
    pub b: PieceRef,
    /// Approximate volume where they overlap, cubic millimetres.
    pub overlap: f64,
    /// Whether material of one lies within the clearance of the other
    /// (always true when they overlap).
    pub too_close: bool,
    /// Box of the samples that overlap or come too close, min then max.
    pub bounds: [Vec3; 2],
}

/// Checks every pair of pieces: where one's material runs into the
/// other's, and, with a `clearance` above zero, where it comes closer than
/// that (in any of the other piece's six axis directions). Pairs that
/// neither overlap nor come too close are left out.
pub fn check(pieces: &[PlacedRelief], clearance: f64) -> Vec<PieceIssue> {
    let clearance = clearance.max(0.0);
    let mut issues = Vec::new();
    for (i, a) in pieces.iter().enumerate() {
        for b in &pieces[i + 1..] {
            if let Some(issue) = check_pair(a, b, clearance) {
                issues.push(issue);
            }
        }
    }
    issues
}

/// Whether the material of `a` runs into the material of `b` (faces that
/// only touch do not count).
pub fn overlaps(a: &PlacedRelief, b: &PlacedRelief) -> bool {
    let box_b = b.bounds();
    if !boxes_meet(&a.bounds(), &box_b, 0.0) {
        return false;
    }
    let to_b = b.pose.inverse().then_local(&a.pose);
    let s = a.stock;
    for y in 0..s.height {
        for x in 0..s.width {
            let i = y * s.width + x;
            if !is_material(s, i) {
                continue;
            }
            let (low, high) = (f64::from(s.floor[i]), f64::from(s.z[i]));
            let layers = (((high - low) / s.cell).ceil() as usize).clamp(1, MAX_LAYERS);
            let dz = (high - low) / layers as f64;
            let (px, py) = ((x as f64 + 0.5) * s.cell, (y as f64 + 0.5) * s.cell);
            let ends = [
                a.pose.apply([px, py, low - s.bottom]),
                a.pose.apply([px, py, high - s.bottom]),
            ];
            let span = [
                [0, 1, 2].map(|k| ends[0][k].min(ends[1][k])),
                [0, 1, 2].map(|k| ends[0][k].max(ends[1][k])),
            ];
            if !boxes_meet(&span, &box_b, 0.0) {
                continue;
            }
            for k in 0..layers {
                let local = [px, py, low + (k as f64 + 0.5) * dz - s.bottom];
                if b.contains_inset(to_b.apply(local)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether any of the surface of `a` lies inside the material of `b` by
/// more than `eps`: `a` sampled just inside its top and underside over every
/// cell, and just inside its walls (where its material ends next to a cell)
/// from the underside to the top. Finer than [`overlaps`] for pieces that
/// meet: faces that touch stay apart, and a piece sunk into another by a
/// little more than `eps` is found.
pub fn runs_into(a: &PlacedRelief, b: &PlacedRelief, eps: f64) -> bool {
    let box_b = b.bounds();
    if !boxes_meet(&a.bounds(), &box_b, 0.0) {
        return false;
    }
    let to_b = b.pose.inverse().then_local(&a.pose);
    let s = a.stock;
    let cell = s.cell;
    let inset = eps.min(cell / 4.0);
    let inside = |local: Vec3| {
        let world = a.pose.apply(local);
        boxes_meet(&[world, world], &box_b, 0.0) && b.contains(to_b.apply(local), 0.0)
    };
    let heights = |i: usize| {
        (
            f64::from(s.floor[i]) - s.bottom,
            f64::from(s.z[i]) - s.bottom,
        )
    };
    for y in 0..s.height {
        for x in 0..s.width {
            let i = y * s.width + x;
            if !is_material(s, i) {
                continue;
            }
            let (low, high) = heights(i);
            let (x0, y0) = (x as f64 * cell, y as f64 * cell);
            // The column's box, every corner of a turned cell counted.
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for corner in 0..8 {
                let p = a.pose.apply([
                    if corner & 1 == 0 { x0 } else { x0 + cell },
                    if corner & 2 == 0 { y0 } else { y0 + cell },
                    if corner & 4 == 0 { low } else { high },
                ]);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            if !boxes_meet(&[lo, hi], &box_b, 0.0) {
                continue;
            }
            let (cx, cy) = (x0 + cell / 2.0, y0 + cell / 2.0);
            let (bottom, top) = (low + inset, high - inset);
            if inside([cx, cy, bottom]) || inside([cx, cy, top]) {
                return true;
            }
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                let open = nx < 0 || ny < 0 || nx >= s.width as i64 || ny >= s.height as i64 || {
                    let j = ny as usize * s.width + nx as usize;
                    !is_material(s, j) || {
                        let (l, h) = heights(j);
                        l > low + inset || h < high - inset
                    }
                };
                if !open {
                    continue;
                }
                let (px, py) = (
                    cx + dx as f64 * (cell / 2.0 - inset),
                    cy + dy as f64 * (cell / 2.0 - inset),
                );
                let layers = (((high - low) / cell).ceil() as usize).clamp(1, MAX_LAYERS);
                for k in 0..=layers {
                    let z = bottom + (top - bottom) * k as f64 / layers as f64;
                    if inside([px, py, z]) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn boxes_meet(a: &[Vec3; 2], b: &[Vec3; 2], margin: f64) -> bool {
    (0..3).all(|k| a[0][k] <= b[1][k] + margin && b[0][k] <= a[1][k] + margin)
}

fn check_pair(a: &PlacedRelief, b: &PlacedRelief, clearance: f64) -> Option<PieceIssue> {
    let (box_a, box_b) = (a.bounds(), b.bounds());
    if !boxes_meet(&box_a, &box_b, clearance) {
        return None;
    }
    let to_b = b.pose.inverse().then_local(&a.pose);
    let s = a.stock;
    let layer_step = s.cell;
    let column = s.cell * s.cell;
    let near: Vec<Vec3> = if clearance > 0.0 {
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        axes.iter()
            .flat_map(|axis| {
                [
                    pose::scale(*axis, clearance),
                    pose::scale(*axis, -clearance),
                ]
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut overlap = 0.0;
    let mut close = false;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for y in 0..s.height {
        for x in 0..s.width {
            let i = y * s.width + x;
            if !is_material(s, i) {
                continue;
            }
            let (low, high) = (f64::from(s.floor[i]), f64::from(s.z[i]));
            let span = high - low;
            let layers = ((span / layer_step).ceil() as usize).clamp(1, MAX_LAYERS);
            let dz = span / layers as f64;
            let (px, py) = ((x as f64 + 0.5) * s.cell, (y as f64 + 0.5) * s.cell);
            for k in 0..layers {
                let local = [px, py, low + (k as f64 + 0.5) * dz - s.bottom];
                let world = a.pose.apply(local);
                if !boxes_meet(&[world, world], &box_b, clearance) {
                    continue;
                }
                let in_b = to_b.apply(local);
                let hit = if b.contains_inset(in_b) {
                    overlap += column * dz;
                    true
                } else {
                    near.iter().any(|d| b.contains(pose::add(in_b, *d), 0.0))
                };
                if hit {
                    close = true;
                    for n in 0..3 {
                        lo[n] = lo[n].min(world[n]);
                        hi[n] = hi[n].max(world[n]);
                    }
                }
            }
        }
    }
    close.then_some(PieceIssue {
        a: a.key,
        b: b.key,
        overlap,
        too_close: close,
        bounds: [lo, hi],
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::cam::toolpath::{Move, StrategyKind, Toolpath};
    use crate::cam::Point3;
    use crate::doc::JobSetup;

    const CELLS: f64 = 20_000.0;

    pub(crate) fn document() -> Document {
        let mut doc = Document::new();
        doc.begin("job");
        doc.set_job(JobSetup {
            width: 100.0,
            height: 50.0,
            thickness: 10.0,
            ..JobSetup::default()
        })
        .expect("job");
        doc.commit();
        doc
    }

    /// A 6 mm end mill cutting round the rectangle (x0, y0)-(x1, y1) (tool
    /// centre on the line) down to `depth`, rising to `tab` below the top
    /// over the middle of the bottom side when given.
    pub(crate) fn cut_out(sheet: u64, rect: [f64; 4], depth: f64, tab: Option<f64>) -> Toolpath {
        let mut tp = Toolpath::new(
            0,
            "cut out",
            Tool {
                name: "6 mm".into(),
                tool_type: ToolGeometry::EndMill { diameter: 6.0 },
                ..Tool::default()
            },
            StrategyKind::Profile,
        );
        let [x0, y0, x1, y1] = rect;
        tp.sheet = sheet;
        tp.home = Point3::new(0.0, 0.0, 20.0);
        let mid = (x0 + x1) / 2.0;
        let mut moves = vec![
            Move::rapid(Point3::new(x0, y0, 5.0)),
            Move::plunge(Point3::new(x0, y0, -depth)),
        ];
        if let Some(tab) = tab {
            moves.push(Move::feed(Point3::new(mid - 8.0, y0, -depth)));
            moves.push(Move::feed(Point3::new(mid - 8.0, y0, -tab)));
            moves.push(Move::feed(Point3::new(mid + 8.0, y0, -tab)));
            moves.push(Move::feed(Point3::new(mid + 8.0, y0, -depth)));
        }
        moves.extend([
            Move::feed(Point3::new(x1, y0, -depth)),
            Move::feed(Point3::new(x1, y1, -depth)),
            Move::feed(Point3::new(x0, y1, -depth)),
            Move::feed(Point3::new(x0, y0, -depth)),
            Move::rapid(Point3::new(x0, y0, 5.0)),
        ]);
        tp.moves = moves;
        tp
    }

    pub(crate) fn add(doc: &mut Document, toolpath: Toolpath) {
        doc.begin("toolpath");
        doc.add_toolpath(toolpath);
        doc.commit();
    }

    fn parts_of(doc: &Document, sheet: u64) -> SheetParts {
        SheetParts::find(sheet, simulate(doc, sheet, CELLS).expect("simulates"))
    }

    #[test]
    fn only_sheets_with_toolpaths_make_pieces_laid_where_their_parts_lie() {
        let mut doc = document();
        let first = doc.active_sheet();
        doc.begin("sheet");
        let second = doc.add_sheet(Some("Side".into()));
        let third = doc.add_sheet(Some("Empty".into()));
        doc.commit();
        add(
            &mut doc,
            cut_out(first, [20.0, 10.0, 80.0, 40.0], 11.0, None),
        );
        add(
            &mut doc,
            cut_out(second, [20.0, 10.0, 80.0, 40.0], 11.0, None),
        );
        let mut cache = PartsCache::default();
        let listed = pieces(&doc, &mut cache, CELLS).expect("pieces");
        let sheets: Vec<u64> = listed.iter().map(|p| p.piece.sheet).collect();
        assert_eq!(sheets, vec![first, second]);
        assert!(!sheets.contains(&third));
        let tolerance = listed[0].sheet_parts.stock.cell * 1.5;
        let at = listed[0].piece.position;
        assert!((at[0] - 50.0).abs() < tolerance && (at[1] - 25.0).abs() < tolerance);
        assert!((at[2] - 5.0).abs() < 1e-9);
        let next = listed[1].piece.position;
        assert!((next[0] - (100.0 + LAYOUT_GAP + 50.0)).abs() < tolerance);
        assert!((listed[0].size[0] - 54.0).abs() < tolerance);
        assert_eq!(listed[0].parts, 1);
    }

    #[test]
    fn a_cut_out_part_loses_its_waste_and_keeps_its_size() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [20.0, 10.0, 80.0, 40.0], 11.0, None),
        );
        let parts = parts_of(&doc, sheet);
        assert!(!parts.whole);
        assert_eq!(parts.count(), 1);
        let stock = parts.part_stock(1).expect("the part");
        let [lo, hi] = material_span(&stock).expect("material left");
        // The part is the inside of the cut: 60 x 30 less the tool radius
        // each side, give or take a cell.
        let tolerance = stock.cell * 1.5;
        assert!((hi[0] - lo[0] - 54.0).abs() < tolerance, "{lo:?} {hi:?}");
        assert!((hi[1] - lo[1] - 24.0).abs() < tolerance, "{lo:?} {hi:?}");
        let size = parts.size(1).expect("size");
        assert!((size[0] - 54.0).abs() < tolerance && (size[2] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn tabs_count_as_cut_so_each_held_part_is_its_own_piece() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        // Two parts side by side, each held by a 3 mm tab.
        add(
            &mut doc,
            cut_out(sheet, [8.0, 10.0, 45.0, 40.0], 11.0, Some(7.0)),
        );
        add(
            &mut doc,
            cut_out(sheet, [55.0, 10.0, 92.0, 40.0], 11.0, Some(7.0)),
        );
        let parts = parts_of(&doc, sheet);
        assert!(!parts.whole);
        assert_eq!(parts.count(), 2, "{parts:?}");
        let [a, b] = [parts.job_box(1), parts.job_box(2)].map(|b| b.expect("box"));
        assert!(a[2] <= b[0], "left part first: {a:?} {b:?}");
        let tolerance = parts.stock.cell * 1.5;
        assert!((a[2] - a[0] - 31.0).abs() < tolerance, "{a:?}");
        // The tab under the part's bottom side stays with the waste: the
        // part starts at the inside of the cut.
        assert!((a[1] - 13.0).abs() < tolerance, "{a:?}");
    }

    #[test]
    fn the_slug_of_a_hole_is_waste_not_a_part() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [10.0, 5.0, 90.0, 45.0], 11.0, None),
        );
        // A window cut through the part.
        add(
            &mut doc,
            cut_out(sheet, [35.0, 17.0, 65.0, 33.0], 11.0, None),
        );
        let parts = parts_of(&doc, sheet);
        assert_eq!(parts.count(), 1);
        let stock = parts.part_stock(1).expect("part");
        let [x0, y0, _, _] = parts.job_box(1).expect("box");
        assert!(stock.is_through(50.0, 25.0), "the window is open");
        assert!(!stock.is_through(x0 + 3.0, y0 + 3.0));
    }

    /// A rectangle vector on `sheet` and a toolpath cutting round it on
    /// the outside with a 6 mm cutter (tool centre 3 mm out), as a profile
    /// calculated from it records.
    pub(crate) fn cut_round_vector(doc: &mut Document, sheet: u64, rect: [f64; 4]) -> u64 {
        use crate::cam::toolpath::SourceRef;
        use crate::doc::{Entity, EntityKind};
        use crate::geom::{rect as rectangle, Point};
        let [x0, y0, x1, y1] = rect;
        let path = rectangle(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
        let layer = doc.active_layer();
        doc.begin("vector");
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
        let mut toolpath = cut_out(sheet, [x0 - 3.0, y0 - 3.0, x1 + 3.0, y1 + 3.0], 11.0, None);
        toolpath.params_json = serde_json::json!({
            "strategy": "profile",
            "params": { "side": "outside" },
            "ids": [id],
        });
        toolpath.sources = vec![SourceRef { id, revision: 0 }];
        add(doc, toolpath);
        id
    }

    /// A rectangle vector on `sheet` and a 6 mm profile of it recorded as
    /// cut on `side`, its moves `depth` deep with the tool centre on the
    /// line, 3 mm inside it or 3 mm outside it as the side says.
    fn cut_vector(doc: &mut Document, sheet: u64, rect: [f64; 4], side: &str, depth: f64) {
        use crate::cam::toolpath::SourceRef;
        use crate::doc::{Entity, EntityKind};
        use crate::geom::{rect as rectangle, Point};
        let [x0, y0, x1, y1] = rect;
        let path = rectangle(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
        let layer = doc.active_layer();
        doc.begin("vector");
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
        let d = match side {
            "outside" => 3.0,
            "inside" => -3.0,
            _ => 0.0,
        };
        let mut toolpath = cut_out(sheet, [x0 - d, y0 - d, x1 + d, y1 + d], depth, None);
        toolpath.params_json = serde_json::json!({
            "strategy": "profile",
            "params": { "side": side },
            "ids": [id],
        });
        toolpath.sources = vec![SourceRef { id, revision: 0 }];
        add(doc, toolpath);
    }

    fn cached_parts(doc: &Document, sheet: u64) -> Arc<SheetParts> {
        PartsCache::default().get(doc, sheet, CELLS).expect("parts")
    }

    #[test]
    fn a_part_cut_free_on_the_line_is_a_part_but_a_small_on_hole_is_waste() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(&mut doc, cut_out(sheet, [2.0, 2.0, 98.0, 48.0], 11.0, None));
        // A 40 x 20 part cut free on its line inside the outer part: the
        // kerf is the hole round it, and what it leaves inside (the vector
        // less the tool radius all round, 34 x 14) is a nested part.
        cut_vector(&mut doc, sheet, [30.0, 15.0, 70.0, 35.0], "on", 11.0);
        // A 10 mm square cut on the line leaves a 4 mm slug, narrower than
        // two tool widths: the waste of a hole. An inside profile always
        // makes a hole.
        cut_vector(&mut doc, sheet, [75.0, 10.0, 85.0, 20.0], "on", 11.0);
        cut_vector(&mut doc, sheet, [72.0, 24.0, 92.0, 42.0], "inside", 11.0);
        let parts = cached_parts(&doc, sheet);
        assert_eq!(parts.count(), 2, "{parts:?}");
        let tolerance = parts.stock.cell * 1.5;
        let nested = (1..=2)
            .filter_map(|p| parts.job_box(p))
            .find(|b| b[2] - b[0] < 50.0)
            .expect("the nested part");
        for (got, want) in nested.iter().zip([33.0, 18.0, 67.0, 32.0]) {
            assert!((got - want).abs() < tolerance, "{nested:?}");
        }
    }

    #[test]
    fn a_region_that_fits_an_on_cut_is_waste_when_another_cut_parts_it() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(&mut doc, cut_out(sheet, [2.0, 2.0, 98.0, 48.0], 11.0, None));
        // The profile on the line only scores the top; a hole cut round
        // the same line by another toolpath is what drops the slug out.
        cut_vector(&mut doc, sheet, [30.0, 15.0, 70.0, 35.0], "on", 1.0);
        add(
            &mut doc,
            cut_out(sheet, [30.0, 15.0, 70.0, 35.0], 11.0, None),
        );
        assert_eq!(cached_parts(&doc, sheet).count(), 1);
    }

    #[test]
    fn a_part_cut_free_inside_a_hole_is_a_part_but_the_slug_round_it_is_not() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        // The outer part, and a window cut through it (a hole: its slug is
        // waste).
        add(&mut doc, cut_out(sheet, [2.0, 2.0, 98.0, 48.0], 11.0, None));
        add(
            &mut doc,
            cut_out(sheet, [15.0, 10.0, 85.0, 40.0], 11.0, None),
        );
        // Without the nested cut the window's slug is waste.
        assert_eq!(parts_of(&doc, sheet).count(), 1);
        // A small part nested in the window, cut round its vector outside.
        cut_round_vector(&mut doc, sheet, [35.0, 20.0, 65.0, 30.0]);
        let found = outlines(&doc, sheet);
        assert_eq!(found.len(), 1);
        let parts = SheetParts::find_with(
            sheet,
            simulate(&doc, sheet, CELLS).expect("simulates"),
            &found,
        );
        assert_eq!(parts.count(), 2, "{parts:?}");
        let tolerance = parts.stock.cell * 1.5;
        let nested = (1..=2)
            .filter_map(|p| parts.job_box(p))
            .find(|b| b[2] - b[0] < 50.0)
            .expect("the nested part");
        for (got, want) in nested.iter().zip([35.0, 20.0, 65.0, 30.0]) {
            assert!((got - want).abs() < tolerance, "{nested:?}");
        }
        // The ring of slug between the window and the nested cut stays
        // waste; the cache finds the nested part too.
        let mut cache = PartsCache::default();
        assert_eq!(cache.get(&doc, sheet, CELLS).expect("parts").count(), 2);
    }

    #[test]
    fn anchors_fit_the_same_part_moved_a_little_and_not_another() {
        let a = Anchor {
            centre: [50.0, 25.0],
            area: 1000.0,
        };
        let moved = Anchor {
            centre: [52.0, 25.0],
            area: 1100.0,
        };
        let far = Anchor {
            centre: [150.0, 25.0],
            area: 1000.0,
        };
        let small = Anchor {
            centre: [50.0, 25.0],
            area: 300.0,
        };
        assert!(a.misfit(&moved, 0.5).is_some());
        assert!(a.misfit(&far, 0.5).is_none());
        assert!(a.misfit(&small, 0.5).is_none());
        let stored = [
            Piece {
                sheet: 1,
                part: 1,
                position: [0.0; 3],
                rotation: [0.0; 3],
                anchor: Some(far),
                component: None,
                hidden: false,
                grounded: false,
            },
            Piece {
                sheet: 1,
                part: 2,
                position: [0.0; 3],
                rotation: [0.0; 3],
                anchor: Some(a),
                component: None,
                hidden: false,
                grounded: false,
            },
            Piece {
                sheet: 1,
                part: 3,
                position: [0.0; 3],
                rotation: [0.0; 3],
                anchor: None,
                component: None,
                hidden: false,
                grounded: false,
            },
        ];
        // Part 1 now is where stored part 2 was; stored part 1 is gone;
        // the older placement without an anchor goes by its number.
        let taken = match_stored(&stored, 1, &[1, 2, 3], &[moved, small, small], 0.5);
        assert_eq!(taken, vec![Some(1), None, Some(2)]);
    }

    #[test]
    fn a_sheets_raw_model_is_a_relief_standing_on_its_base() {
        use crate::model::{field::Field, tree::ComponentExtra, Combine, Component};
        let mut doc = document();
        let sheet = doc.active_sheet();
        assert!(model_stock(&doc, sheet).expect("model").is_none());
        let grid = crate::model::grid_for(doc.job_setup());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for (i, z) in field.z.iter_mut().enumerate() {
            let (x, y) = (
                grid.origin.x + (i % grid.width) as f64 * grid.cell,
                grid.origin.y + (i / grid.width) as f64 * grid.cell,
            );
            if (20.0..40.0).contains(&x) && (10.0..20.0).contains(&y) {
                *z = 15.0;
            }
        }
        doc.begin("model");
        doc.add_component(Component {
            id: 0,
            name: "block".into(),
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: ComponentExtra::default(),
            field: field.cropped().expect("cells"),
        });
        doc.commit();
        let stock = model_stock(&doc, sheet).expect("model").expect("not flat");
        assert!((stock.top - stock.bottom - 15.0).abs() < 1e-4);
        let parts = SheetParts::from_model(sheet, stock).expect("a part");
        let size = parts.size(1).expect("size");
        let tolerance = parts.stock.cell * 1.5;
        assert!((size[0] - 20.0).abs() < tolerance && (size[1] - 10.0).abs() < tolerance);
        assert!((size[2] - 15.0).abs() < 1e-4);
        // A sheet with a model and no toolpaths gives the model as its piece.
        let mut cache = PartsCache::default();
        let listed = pieces(&doc, &mut cache, CELLS).expect("pieces");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].piece.part, MODEL_PART);
        assert!(listed[0].is_model() && listed[0].has_model && !listed[0].has_toolpaths);
    }

    #[test]
    fn a_groove_that_does_not_cut_through_keeps_the_whole_sheet() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [20.0, 10.0, 80.0, 40.0], 3.0, None),
        );
        let parts = parts_of(&doc, sheet);
        assert!(parts.whole);
        let stock = parts.part_stock(1).expect("whole");
        assert!(!stock.is_through(2.0, 2.0));
        assert_eq!(stock.height_at(20.0, 25.0), Some(-3.0));
        assert_eq!(parts.size(1), Some([100.0, 50.0, 10.0]));
    }

    #[test]
    fn the_cache_simulates_once_until_a_toolpath_changes() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        add(
            &mut doc,
            cut_out(sheet, [20.0, 10.0, 80.0, 40.0], 11.0, None),
        );
        let mut cache = PartsCache::default();
        let first = cache.get(&doc, sheet, CELLS).expect("parts");
        let again = cache.get(&doc, sheet, CELLS).expect("parts");
        assert!(Arc::ptr_eq(&first, &again));
        add(
            &mut doc,
            cut_out(sheet, [30.0, 15.0, 70.0, 35.0], 11.0, None),
        );
        let changed = cache.get(&doc, sheet, CELLS).expect("parts");
        assert!(!Arc::ptr_eq(&first, &changed));
    }

    #[test]
    fn a_pose_turns_about_the_middle_of_the_part() {
        let piece = Piece {
            sheet: 1,
            part: 1,
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 90.0],
            anchor: None,
            component: None,
            hidden: false,
            grounded: false,
        };
        let pose = piece.pose([100.0, 50.0, 10.0]);
        let middle = pose.apply([50.0, 25.0, 5.0]);
        for (got, want) in middle.iter().zip([10.0, 20.0, 30.0]) {
            assert!((got - want).abs() < 1e-9);
        }
        // The material's X axis now runs along Y.
        let end = pose.apply([100.0, 25.0, 5.0]);
        assert!((end[1] - 70.0).abs() < 1e-9 && (end[0] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn faces_are_found_on_top_underside_and_walls() {
        let doc = document();
        let stock = simulate(&doc, doc.active_sheet(), CELLS).expect("stock");
        let top = face_at(&stock, [30.0, 20.0, 10.0]).expect("top");
        assert_eq!(top.normal, [0.0, 0.0, 1.0]);
        let under = face_at(&stock, [30.0, 20.0, 0.0]).expect("under");
        assert_eq!(under.normal, [0.0, 0.0, -1.0]);
        let right = face_at(&stock, [99.8, 20.0, 5.0]).expect("right wall");
        assert_eq!(right.normal, [1.0, 0.0, 0.0]);
        // A point picked a little inside lands on the wall.
        assert!(
            (right.point[0] - 100.0).abs() < stock.cell * 0.1,
            "{right:?}"
        );
        let front = face_at(&stock, [40.0, 0.0, 5.0]).expect("front wall");
        assert_eq!(front.normal, [0.0, -1.0, 0.0]);
        assert!(face_at(&stock, [300.0, 20.0, 5.0]).is_none());
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    #[test]
    fn a_mate_stands_an_edge_on_a_top_face_with_offset_and_turned_over() {
        let size = [100.0, 50.0, 10.0];
        let base = Piece {
            sheet: 1,
            part: 1,
            position: [50.0, 25.0, 5.0],
            rotation: [0.0; 3],
            anchor: None,
            component: None,
            hidden: false,
            grounded: false,
        };
        let side = Piece {
            sheet: 2,
            part: 1,
            position: [300.0, 25.0, 5.0],
            rotation: [0.0; 3],
            anchor: None,
            component: None,
            hidden: false,
            grounded: false,
        };
        let target = PickedFace {
            point: [50.0, 25.0, 10.0],
            normal: [0.0, 0.0, 1.0],
        };
        // The side's front edge (y = 0), picked in its middle.
        let edge = PickedFace {
            point: [50.0, 0.0, 5.0],
            normal: [0.0, -1.0, 0.0],
        };
        let options = MateOptions {
            offset: 0.0,
            turn: None,
        };
        let (placed, turn) = mate(&base.pose(size), &target, &side, size, &edge, options);
        let pose = placed.pose(size);
        // The edge's middle sits on the base's top middle and the side
        // stands up from it, 50 mm tall.
        assert!(close(pose.apply(edge.point), [50.0, 25.0, 10.0]));
        let far = pose.apply([50.0, 50.0, 5.0]);
        assert!((far[2] - 60.0).abs() < 1e-6, "{far:?}");
        assert!(close(pose.apply_vector(edge.normal), [0.0, 0.0, -1.0]));
        // Placing again with the turn it answered gives the same place.
        let (again, _) = mate(
            &base.pose(size),
            &target,
            &placed,
            size,
            &edge,
            MateOptions {
                turn: Some(turn),
                ..options
            },
        );
        assert!(close(again.position, placed.position));
        // An offset leaves a gap.
        let (gap, _) = mate(
            &base.pose(size),
            &target,
            &side,
            size,
            &edge,
            MateOptions {
                offset: 2.0,
                ..options
            },
        );
        assert!(close(gap.pose(size).apply(edge.point), [50.0, 25.0, 12.0]));
        // Turned over, the side's back edge (y = 50) meets the base: the
        // side still stands on it, the other way up.
        let doc = document();
        let stock = simulate(&doc, doc.active_sheet(), CELLS).expect("stock");
        let back = opposite_face(&stock, &edge);
        assert_eq!(back.normal, [0.0, 1.0, 0.0]);
        assert!((back.point[1] - 50.0).abs() < stock.cell, "{back:?}");
        let (flipped, _) = mate(&base.pose(size), &target, &side, size, &back, options);
        let pose = flipped.pose(size);
        assert!(close(pose.apply_vector(back.normal), [0.0, 0.0, -1.0]));
        assert!((pose.apply(back.point)[2] - 10.0).abs() < 1e-6);
        let far = pose.apply([50.0, 0.0, 5.0]);
        assert!((far[2] - 10.0 - (back.point[1])).abs() < 1e-6, "{far:?}");
    }

    #[test]
    fn the_opposite_of_a_top_face_is_the_underside_below_it() {
        let doc = document();
        let stock = simulate(&doc, doc.active_sheet(), CELLS).expect("stock");
        let top = face_at(&stock, [30.0, 20.0, 10.0]).expect("top");
        let under = opposite_face(&stock, &top);
        assert_eq!(under.normal, [0.0, 0.0, -1.0]);
        assert!(close(under.point, [30.0, 20.0, 0.0]));
        let left = face_at(&stock, [0.2, 20.0, 5.0]).expect("left wall");
        let right = opposite_face(&stock, &left);
        assert_eq!(right.normal, [1.0, 0.0, 0.0]);
        assert!((right.point[0] - 100.0).abs() < stock.cell, "{right:?}");
    }

    fn placed<'a>(sheet: u64, stock: &'a Stock, piece: Piece) -> PlacedRelief<'a> {
        let size = [
            stock.width as f64 * stock.cell,
            stock.height as f64 * stock.cell,
            stock.top - stock.bottom,
        ];
        PlacedRelief {
            key: PieceRef {
                sheet,
                part: 1,
                component: None,
            },
            stock,
            pose: piece.pose(size),
        }
    }

    fn flat(sheet: u64, position: Vec3) -> Piece {
        Piece {
            sheet,
            part: 1,
            position,
            rotation: [0.0; 3],
            anchor: None,
            component: None,
            hidden: false,
            grounded: false,
        }
    }

    #[test]
    fn stacked_pieces_touch_without_overlap_and_clash_when_sunk_in() {
        let doc = document();
        let stock = simulate(&doc, doc.active_sheet(), CELLS).expect("relief");
        // Two plain 100 x 50 x 10 sheets, the second on top of the first.
        let low = placed(1, &stock, flat(1, [50.0, 25.0, 5.0]));
        let touching = placed(2, &stock, flat(2, [50.0, 25.0, 15.0]));
        assert!(check(&[low, touching], 0.0).is_empty(), "touching is fine");

        let low = placed(1, &stock, flat(1, [50.0, 25.0, 5.0]));
        let touching = placed(2, &stock, flat(2, [50.0, 25.0, 15.0]));
        let issues = check(&[low, touching], 1.0);
        assert_eq!(issues.len(), 1, "closer than a 1 mm clearance");
        assert!(issues[0].too_close);
        assert!(issues[0].overlap.abs() < 1e-9);

        let low = placed(1, &stock, flat(1, [50.0, 25.0, 5.0]));
        let sunk = placed(2, &stock, flat(2, [50.0, 25.0, 12.0]));
        let issues = check(&[low, sunk], 0.0);
        assert_eq!(issues.len(), 1);
        // 100 x 50 x 3 overlap, less the untested rim.
        let volume = issues[0].overlap;
        assert!(volume > 10_000.0 && volume < 15_001.0, "{volume}");
        assert_eq!((issues[0].a.sheet, issues[0].b.sheet), (1, 2));
    }

    #[test]
    fn a_turned_piece_standing_clear_reports_nothing() {
        let doc = document();
        let stock = simulate(&doc, doc.active_sheet(), CELLS).expect("relief");
        let floor = placed(1, &stock, flat(1, [50.0, 25.0, 5.0]));
        // Stood on its long edge, 5 mm above the floor piece's top.
        let upright = placed(
            2,
            &stock,
            Piece {
                sheet: 2,
                part: 1,
                position: [50.0, 25.0, 10.0 + 5.0 + 25.0],
                rotation: [90.0, 0.0, 0.0],
                anchor: None,
                component: None,
                hidden: false,
                grounded: false,
            },
        );
        assert!(check(&[floor, upright], 2.0).is_empty());
    }
}
