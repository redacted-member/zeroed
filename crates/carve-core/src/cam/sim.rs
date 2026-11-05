//! Toolpath preview: a heightfield of the material that each toolpath's
//! tool is swept through, so the 3D view can show what the job will look
//! like once cut.
//!
//! The stock is a grid of cells over the job rectangle holding the height
//! of the material top in job Z, and which toolpath last cut each cell. A
//! tool is rasterised once into a footprint (cell offsets and the height of
//! the tool's bottom above its tip there); cutting at a point lowers every
//! cell under the footprint to the tool bottom. Cutting moves are stepped
//! at most half a cell at a time so the swept groove has no gaps. Rapids do
//! not cut, but a rapid that passes below the material is reported as a
//! collision.
//!
//! On a double sided job the preview is seen from the active side. The
//! stock also keeps the height of the material's underside per cell; the
//! other side's toolpaths are mapped through the flip into the active
//! side's job space and cut upwards into that underside, so where cuts from
//! the two faces meet the cell has no material left: a through cut.

use std::sync::OnceLock;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::doc::{FlipAxis, JobSetup, Side};
use crate::error::CoreError;

use super::collide::{IssueKind, Severity};
use super::tool::{Tool, ToolGeometry};
use super::toolpath::{Move, MoveKind, Point3, Toolpath};

/// How finely the preview is calculated, like a quality setting in a CAM package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    /// About a million cells.
    #[default]
    Standard,
    /// About two and a half million cells.
    High,
    /// About four million cells.
    VeryHigh,
    /// About eight million cells.
    ExtremelyHigh,
    /// About twenty five million cells, 25 times Standard, the finest the preview goes.
    Maximum,
}

impl Quality {
    /// About how many cells a stock of this quality has.
    pub fn cells(self) -> f64 {
        match self {
            Quality::Standard => 1.0e6,
            Quality::High => 2.5e6,
            Quality::VeryHigh => 4.0e6,
            Quality::ExtremelyHigh => 8.0e6,
            Quality::Maximum => 2.5e7,
        }
    }
}

/// Something the tool met while the preview cut: a rapid move that would
/// have driven the tool into the material, or another of the
/// [`IssueKind`]s the [`super::collide::Guard`] checks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collision {
    /// Toolpath the move belongs to.
    pub toolpath_id: u64,
    /// Index of the move in the toolpath.
    pub move_index: usize,
    /// Where the tool tip was, in job millimetres.
    pub x: f64,
    /// Where the tool tip was, in job millimetres.
    pub y: f64,
    /// Where the tool tip was, in job Z.
    pub z: f64,
    /// What was met.
    #[serde(default)]
    pub kind: IssueKind,
    /// How much it matters.
    #[serde(default)]
    pub severity: Severity,
}

/// Cell offsets under a tool and the height of its bottom above the tip.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    cells: Vec<(i32, i32, f32)>,
}

impl Footprint {
    /// Rasterises `tool` at `cell` millimetres per cell.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the tool has no cutting width.
    pub fn of(tool: &Tool, cell: f64) -> Result<Footprint, CoreError> {
        let radius = tool.radius();
        if !radius.is_finite() || radius <= 0.0 {
            return Err(CoreError::validation(
                "the tool has no cutting diameter to preview with",
            ));
        }
        let profile = tool.profile(128)?;
        let height_at = |r: f64| -> f64 {
            match profile.iter().position(|(pr, _)| *pr >= r) {
                Some(0) => profile[0].1,
                Some(i) => {
                    let (r0, z0) = profile[i - 1];
                    let (r1, z1) = profile[i];
                    if r1 - r0 <= 0.0 {
                        z1
                    } else {
                        z0 + (z1 - z0) * (r - r0) / (r1 - r0)
                    }
                }
                None => profile.last().map_or(0.0, |p| p.1),
            }
        };
        let reach = (radius / cell).ceil() as i32;
        let mut cells = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let r = ((dx as f64).hypot(dy as f64)) * cell;
                if r <= radius + 1e-9 {
                    cells.push((dx, dy, height_at(r) as f32));
                }
            }
        }
        Ok(Footprint { cells })
    }

    /// Cell offsets under the tool and the height of its bottom above the
    /// tip at each.
    pub fn cells(&self) -> &[(i32, i32, f32)] {
        &self.cells
    }
}

/// The eight cells round a cell, as offsets.
const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// The position of offset (`dx`, `dy`) in [`NEIGHBOURS`], or `None` unless
/// it is one of them.
fn neighbour(dx: i64, dy: i64) -> Option<usize> {
    if dx.abs() > 1 || dy.abs() > 1 || (dx == 0 && dy == 0) {
        return None;
    }
    let k = ((dy + 1) * 3 + dx + 1) as usize;
    Some(if k > 4 { k - 1 } else { k })
}

/// A tool footprint laid out for sweeping it along moves.
///
/// A stamp need only cut the cells where it is lower than the stamps just
/// before it and no higher than those just after (see
/// [`Stock::stamp_sample`]). For each cell that is a bound on the rise here
/// less the rise under the other stamp, `r(o) - r(o + shift)`. The cutter
/// keeps the cells in lines along each direction a move comes from
/// ([`Order`]) and, made when first wanted, those differences for each
/// shift along the lines with their running lowest ([`Profile`]): for a
/// stamp before, the lowest from the line's start, so a line starts where
/// that first meets its bound; for a stamp after, the lowest to the line's
/// end, so a line stops where no later cell can meet it. For any tool no
/// narrower lower down the lines then hold little more than the band the
/// stamp cuts. Every cell between is still checked, so the stock comes out
/// exact whatever the tool's shape.
#[derive(Debug, Clone)]
pub struct Cutter {
    /// (dx, dy, rise) by rise, lowest first.
    cells: Vec<(i32, i32, f32)>,
    /// The rise at each offset up to two cells beyond the tool (infinity
    /// outside it), row by row from (-reach - 2, -reach - 2).
    grid: Vec<f32>,
    /// How many cells from the centre the footprint reaches.
    reach: i32,
    /// Lines along each of the [`NEIGHBOURS`], made when first wanted;
    /// empty for a tool too large to keep them for.
    orders: Vec<OnceLock<Order>>,
    /// [`Profile`]s by [`Cutter::profile_slot`], made when first wanted.
    profiles: Vec<OnceLock<Profile>>,
    /// For a stamp at the same height as the stamps either side of it, one
    /// [`NEIGHBOURS`]`[j]` back and on: the cells it can cut, the band
    /// across the move, by rise. Made when first wanted.
    bands: Vec<OnceLock<Vec<(i32, i32, f32)>>>,
}

/// The cells of a [`Cutter`] in lines along one direction.
#[derive(Debug, Clone)]
struct Order {
    /// (dx, dy, rise), line after line, each in order along the direction.
    cells: Vec<(i32, i32, f32)>,
    /// Each line's first cell and one past its last in `cells`, and the
    /// lowest rise along it.
    spans: Vec<(u32, u32, f32)>,
}

/// For the cells of an [`Order`], the rise less the rise one `shift` on,
/// with its running lowest along each line: from the line's start for a
/// stamp before, to the line's end for a stamp after.
#[derive(Debug, Clone)]
struct Profile {
    difference: Vec<f32>,
    lowest: Vec<f32>,
}

/// The most cells a [`Cutter`] sets out in lines for (a dozen bytes a cell
/// for each direction and each shift a sweep compares).
const MOST_LINE_CELLS: usize = 60_000;

/// How far, in cells either way, the stamps a stamp is compared with may
/// lie.
const FAR: i32 = 2;

/// Slack in millimetres for comparisons between heights rounded to `f32`,
/// far above their rounding and far below a cell: a stamp may visit a few
/// cells it leaves as they are, never skip one it would cut.
const SLACK: f64 = 1e-4;

impl Cutter {
    /// The footprint laid out for sweeping.
    pub fn new(footprint: &Footprint) -> Cutter {
        let mut cells = footprint.cells.clone();
        cells.sort_by(|a, b| a.2.total_cmp(&b.2));
        let reach = cells
            .iter()
            .map(|&(dx, dy, _)| dx.abs().max(dy.abs()))
            .max()
            .unwrap_or(0);
        let pad = reach + FAR;
        let side = 2 * pad + 1;
        let mut grid = vec![f32::INFINITY; (side * side) as usize];
        for &(dx, dy, r) in &cells {
            grid[((dy + pad) * side + dx + pad) as usize] = r;
        }
        let lined = cells.len() <= MOST_LINE_CELLS;
        let (orders, profiles) = if lined {
            let shifts = (2 * FAR + 1) * (2 * FAR + 1);
            (
                (0..NEIGHBOURS.len()).map(|_| OnceLock::new()).collect(),
                (0..NEIGHBOURS.len() * shifts as usize * 2)
                    .map(|_| OnceLock::new())
                    .collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        Cutter {
            cells,
            grid,
            reach,
            orders,
            profiles,
            bands: (0..NEIGHBOURS.len()).map(|_| OnceLock::new()).collect(),
        }
    }

    /// How many cells from the tool centre the footprint reaches.
    pub fn reach(&self) -> i32 {
        self.reach
    }

    /// The rise at offset (`dx`, `dy`), infinity outside the tool.
    #[inline]
    fn rise_at(&self, dx: i32, dy: i32) -> f32 {
        let pad = self.reach + FAR;
        let side = 2 * pad + 1;
        let (x, y) = (dx + pad, dy + pad);
        if x < 0 || y < 0 || x >= side || y >= side {
            return f32::INFINITY;
        }
        self.grid[(y * side + x) as usize]
    }

    /// The lines along `NEIGHBOURS[j]`, when kept.
    fn order(&self, j: usize) -> Option<&Order> {
        let slot = self.orders.get(j)?;
        Some(slot.get_or_init(|| {
            let (ex, ey) = NEIGHBOURS[j];
            let mut cells = Vec::with_capacity(self.cells.len());
            let mut spans = Vec::new();
            for &(sx, sy, _) in &self.cells {
                // A line starts at a cell with none one step back.
                if self.rise_at(sx - ex, sy - ey).is_finite() {
                    continue;
                }
                let start = cells.len();
                let (mut x, mut y) = (sx, sy);
                let mut lowest = f32::INFINITY;
                loop {
                    let r = self.rise_at(x, y);
                    if !r.is_finite() {
                        break;
                    }
                    lowest = lowest.min(r);
                    cells.push((x, y, r));
                    x += ex;
                    y += ey;
                }
                spans.push((start as u32, cells.len() as u32, lowest));
            }
            Order { cells, spans }
        }))
    }

    /// The band a level stamp cuts on a straight run along
    /// `NEIGHBOURS[j]`: lower than one cell back, no higher than one on.
    fn band(&self, j: usize) -> &[(i32, i32, f32)] {
        self.bands[j].get_or_init(|| {
            let (ex, ey) = NEIGHBOURS[j];
            self.cells
                .iter()
                .copied()
                .filter(|&(dx, dy, r)| {
                    r < self.rise_at(dx + ex, dy + ey) && r <= self.rise_at(dx - ex, dy - ey)
                })
                .collect()
        })
    }

    fn profile_slot(j: usize, (sx, sy): (i32, i32), earlier: bool) -> usize {
        let side = 2 * FAR + 1;
        let shift = ((sy + FAR) * side + sx + FAR) as usize;
        (j * (side * side) as usize + shift) * 2 + usize::from(earlier)
    }

    /// The differences to the rise one `shift` on along the lines of
    /// `order` (direction `j`), with their running lowest from each line's
    /// start (`earlier`) or to its end.
    fn profile(
        &self,
        j: usize,
        order: &Order,
        shift: (i32, i32),
        earlier: bool,
    ) -> Option<&Profile> {
        let slot = self.profiles.get(Cutter::profile_slot(j, shift, earlier))?;
        Some(slot.get_or_init(|| {
            let difference: Vec<f32> = order
                .cells
                .iter()
                .map(|&(dx, dy, r)| r - self.rise_at(dx + shift.0, dy + shift.1))
                .collect();
            let mut lowest = difference.clone();
            for &(start, end, _) in &order.spans {
                let line = &mut lowest[start as usize..end as usize];
                if earlier {
                    for i in 1..line.len() {
                        line[i] = line[i].min(line[i - 1]);
                    }
                } else {
                    for i in (0..line.len().saturating_sub(1)).rev() {
                        line[i] = line[i].min(line[i + 1]);
                    }
                }
            }
            Profile { difference, lowest }
        }))
    }
}

/// One place the tool is stamped along a sweep: its centre cell and tip.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
    centre: (i64, i64),
    tip: f32,
}

/// Where a sweep has got to: the last stamps made, the samples waiting for
/// the ones after them (a stamp only cuts the cells the next ones do not
/// cut as low, and samples in the same cell make a single stamp at the
/// lowest tip) and the stamps made but not yet cut into the stock.
/// [`Stock::flush_sweep`] cuts the stamps made, [`Stock::finish_sweep`]
/// makes the waiting ones too and cuts them all. A sweep made with
/// [`Sweep::tracking`] also notes which tiles of the stock it changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sweep {
    /// The last two stamps made, the latest first.
    back: [Option<Sample>; 2],
    /// Up to three samples waiting, the oldest first: a stamp is made once
    /// the two samples after it are known.
    ahead: Vec<Sample>,
    /// Stamps worked out but not yet cut into the stock, the oldest first;
    /// [`Stock::flush_sweep`] cuts them.
    batch: Vec<Stamp>,
    changed: Option<Changed>,
}

/// One stamp of a sweep with the two stamps before it (the latest first)
/// and the two after it (the next first) that it is compared with.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stamp {
    before: [Option<Sample>; 2],
    at: Sample,
    after: [Option<Sample>; 2],
}

/// The most stamps a sweep holds before cutting them.
const MOST_BATCHED: usize = 4096;

/// Batches of stamps that visit at least about this many lines of the
/// tool between them are worked out on several threads.
const MANY_LINES: usize = 16_384;

/// The fewest stamps a thread works out at a time.
const FEWEST_A_THREAD: usize = 8;

impl Sweep {
    /// A sweep that notes the tiles of `stock` it changes.
    pub fn tracking(stock: &Stock) -> Sweep {
        Sweep {
            changed: Some(Changed::over(stock)),
            ..Sweep::default()
        }
    }

    /// The same sweep for another toolpath: the stamps start afresh, the
    /// changed tiles stay noted.
    pub fn restart(&mut self) {
        self.back = [None, None];
        self.ahead.clear();
        self.batch.clear();
    }

    /// The changed tiles noted since the last call, as rectangles of cells
    /// `[x0, y0, width, height]` row by row; the tiles are then clean.
    pub fn take_changed(&mut self) -> Vec<[usize; 4]> {
        self.changed.as_mut().map_or_else(Vec::new, Changed::take)
    }
}

/// Cells a side of the tiles [`Changed`] notes.
const TILE: usize = 32;

/// Which tiles of [`TILE`] by [`TILE`] cells of a stock have changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changed {
    width: usize,
    height: usize,
    columns: usize,
    tiles: Vec<bool>,
    any: bool,
}

impl Changed {
    /// No tiles of `stock` changed yet.
    pub fn over(stock: &Stock) -> Changed {
        let columns = stock.width.div_ceil(TILE);
        let rows = stock.height.div_ceil(TILE);
        Changed {
            width: stock.width,
            height: stock.height,
            columns,
            tiles: vec![false; columns * rows],
            any: false,
        }
    }

    /// Notes cell (`x`, `y`) as changed.
    #[inline]
    fn note(&mut self, x: usize, y: usize) {
        if let Some(tile) = self.tiles.get_mut((y / TILE) * self.columns + x / TILE) {
            *tile = true;
            self.any = true;
        }
    }

    /// The changed tiles as rectangles `[x0, y0, width, height]`: runs of
    /// tiles side by side, joined with the same run in the rows below.
    /// Forgets them.
    fn take(&mut self) -> Vec<[usize; 4]> {
        let mut out: Vec<[usize; 4]> = Vec::new();
        // Rectangles that reach down to the row of tiles before.
        let (mut open, mut next): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
        if !self.any {
            return out;
        }
        for (row, tiles) in self.tiles.chunks_mut(self.columns).enumerate() {
            let y0 = row * TILE;
            let height = TILE.min(self.height - y0);
            let mut column = 0;
            while column < tiles.len() {
                if !tiles[column] {
                    column += 1;
                    continue;
                }
                let first = column;
                while column < tiles.len() && tiles[column] {
                    tiles[column] = false;
                    column += 1;
                }
                let x0 = first * TILE;
                let x1 = (column * TILE).min(self.width);
                // A run under one just as wide in the row before joins it.
                match open
                    .iter()
                    .position(|&k| out[k][0] == x0 && out[k][2] == x1 - x0)
                {
                    Some(at) => {
                        let k = open[at];
                        out[k][3] += height;
                        next.push(k);
                    }
                    None => {
                        next.push(out.len());
                        out.push([x0, y0, x1 - x0, height]);
                    }
                }
            }
            std::mem::swap(&mut open, &mut next);
            next.clear();
        }
        self.any = false;
        out
    }
}

/// The material heightfield.
#[derive(Debug, Clone, PartialEq)]
pub struct Stock {
    /// Cells across (X).
    pub width: usize,
    /// Cells down (Y).
    pub height: usize,
    /// Cell size in millimetres.
    pub cell: f64,
    /// Job X of the first cell's centre.
    pub origin_x: f64,
    /// Job Y of the first cell's centre.
    pub origin_y: f64,
    /// Job Z of the uncut top.
    pub top: f64,
    /// Job Z of the bottom of the material.
    pub bottom: f64,
    /// Material height per cell, row by row from the lowest Y.
    pub z: Vec<f32>,
    /// Height of the material's underside per cell, in job Z: `bottom`
    /// until the other side of a double sided job cuts into it.
    pub floor: Vec<f32>,
    /// Which toolpath (its list position plus one) last cut each cell, zero
    /// for uncut.
    pub owner: Vec<u16>,
    /// How dark a laser has charred each cell's surface, 0 (untouched) to
    /// 255 (black); empty until a laser toolpath marks the stock.
    pub burn: Vec<u8>,
}

impl Stock {
    /// Uncut stock over a job's material at the given quality.
    pub fn new(job: &JobSetup, quality: Quality) -> Stock {
        Stock::with_cells(job, quality.cells())
    }

    /// Uncut stock over a job's material in about `cells` cells.
    pub fn with_cells(job: &JobSetup, cells: f64) -> Stock {
        let (datum_x, datum_y) = job.datum();
        let area = job.width * job.height;
        let cell = (area / cells.max(1.0)).sqrt().max(0.01);
        let width = ((job.width / cell).round() as usize).max(1);
        let height = ((job.height / cell).round() as usize).max(1);
        let top = job.surface_z();
        Stock {
            width,
            height,
            cell,
            origin_x: -datum_x + cell / 2.0,
            origin_y: -datum_y + cell / 2.0,
            top,
            bottom: top - job.thickness,
            z: vec![top as f32; width * height],
            floor: vec![(top - job.thickness) as f32; width * height],
            owner: vec![0; width * height],
            burn: Vec::new(),
        }
    }

    /// The cell under job point (`x`, `y`), which may lie outside the stock.
    pub(crate) fn cell_of(&self, x: f64, y: f64) -> (i64, i64) {
        (
            ((x - self.origin_x) / self.cell).round() as i64,
            ((y - self.origin_y) / self.cell).round() as i64,
        )
    }

    /// Material height under a point, or `None` outside the stock.
    pub fn height_at(&self, x: f64, y: f64) -> Option<f32> {
        let (cx, cy) = self.cell_of(x, y);
        if cx < 0 || cy < 0 || cx >= self.width as i64 || cy >= self.height as i64 {
            return None;
        }
        Some(self.z[cy as usize * self.width + cx as usize])
    }

    /// Height of the material's underside under a point, or `None` outside
    /// the stock.
    pub fn floor_at(&self, x: f64, y: f64) -> Option<f32> {
        let (cx, cy) = self.cell_of(x, y);
        if cx < 0 || cy < 0 || cx >= self.width as i64 || cy >= self.height as i64 {
            return None;
        }
        Some(self.floor[cy as usize * self.width + cx as usize])
    }

    /// True when `other` covers the same material with the same cells, so
    /// a preview can go on cutting into it.
    pub fn same_grid(&self, other: &Stock) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.cell == other.cell
            && self.origin_x == other.origin_x
            && self.origin_y == other.origin_y
            && self.top == other.top
            && self.bottom == other.bottom
    }

    /// Removes the piece of material under job point (`x`, `y`) that the
    /// cuts have freed: every cell with material left that joins that cell
    /// side by side, without crossing a cell cut through. Nothing is removed
    /// when no material is under the point or the piece is all the material
    /// left (nothing has been cut free). Answers the cells removed.
    pub fn remove_piece(&mut self, x: f64, y: f64) -> usize {
        let (cx, cy) = self.cell_of(x, y);
        if cx < 0 || cy < 0 || cx >= self.width as i64 || cy >= self.height as i64 {
            return 0;
        }
        let solid = |stock: &Stock, i: usize| stock.floor[i] < stock.z[i] - THROUGH;
        let start = cy as usize * self.width + cx as usize;
        if !solid(self, start) {
            return 0;
        }
        let mut seen = vec![false; self.z.len()];
        let mut piece = Vec::new();
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            piece.push(i);
            let (col, row) = (i % self.width, i / self.width);
            let mut visit = |j: usize, stack: &mut Vec<usize>| {
                if !seen[j] && solid(self, j) {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if col > 0 {
                visit(i - 1, &mut stack);
            }
            if col + 1 < self.width {
                visit(i + 1, &mut stack);
            }
            if row > 0 {
                visit(i - self.width, &mut stack);
            }
            if row + 1 < self.height {
                visit(i + self.width, &mut stack);
            }
        }
        let left = (0..self.z.len()).filter(|i| solid(self, *i)).count();
        if piece.len() == left {
            return 0;
        }
        let bottom = self.bottom as f32;
        for i in &piece {
            self.z[*i] = bottom;
            self.floor[*i] = bottom;
            if let Some(burn) = self.burn.get_mut(*i) {
                *burn = 0;
            }
        }
        piece.len()
    }

    /// This stock seen from the other side of a double sided job, `active`
    /// being the side it is seen from now: the same cells turned over, in
    /// the other side's job space, the underside on top. Cell `i` of this
    /// stock is cell [`Stock::turned_cell`]`(i)` of the turned one, and the
    /// other way round.
    pub fn turned(&self, setup: &JobSetup, active: Side) -> Stock {
        let top = setup.for_side(active.other()).surface_z();
        let last = |origin: f64, cells: usize| origin + (cells - 1) as f64 * self.cell;
        let (origin_x, origin_y) = match setup.double_sided.flip {
            FlipAxis::Horizontal => (
                setup.flip_xy(last(self.origin_x, self.width), 0.0).0,
                self.origin_y,
            ),
            FlipAxis::Vertical => (
                self.origin_x,
                setup.flip_xy(0.0, last(self.origin_y, self.height)).1,
            ),
        };
        let cells = self.width * self.height;
        let mut out = Stock {
            width: self.width,
            height: self.height,
            cell: self.cell,
            origin_x,
            origin_y,
            top,
            bottom: top - (self.top - self.bottom),
            z: vec![0.0; cells],
            floor: vec![0.0; cells],
            owner: vec![0; cells],
            // A laser marks the face it burns, which is underneath now.
            burn: Vec::new(),
        };
        let flip = |h: f32| setup.flip_z(f64::from(h), active) as f32;
        for i in 0..cells {
            let j = self.turned_cell(i, setup);
            out.z[j] = flip(self.floor[i]);
            out.floor[j] = flip(self.z[i]);
            out.owner[j] = self.owner[i];
        }
        out
    }

    /// The cell of the stock turned over ([`Stock::turned`]) lying on
    /// cell `i`.
    pub fn turned_cell(&self, i: usize, setup: &JobSetup) -> usize {
        let (x, y) = (i % self.width, i / self.width);
        match setup.double_sided.flip {
            FlipAxis::Horizontal => y * self.width + (self.width - 1 - x),
            FlipAxis::Vertical => (self.height - 1 - y) * self.width + x,
        }
    }

    /// True when no material is left in cell `i`.
    #[inline]
    pub fn is_through_cell(&self, i: usize) -> bool {
        self.floor[i] >= self.z[i] - THROUGH
    }

    /// True when no material is left under a point: the cuts from the top
    /// and from underneath have met, or a cut went through to the bottom.
    pub fn is_through(&self, x: f64, y: f64) -> bool {
        matches!(
            (self.height_at(x, y), self.floor_at(x, y)),
            (Some(z), Some(floor)) if floor >= z - THROUGH
        )
    }

    /// Sweeps a toolpath of either side through the stock, seen from
    /// `active`: the active side's toolpaths cut down from the top, the
    /// other side's are mapped through the flip and cut up from underneath.
    /// A toolpath of the bottom side of a job that is no longer double sided
    /// cuts nothing.
    ///
    /// # Errors
    /// As [`Stock::cut`].
    pub fn cut_for(
        &mut self,
        setup: &JobSetup,
        active: Side,
        toolpath: &Toolpath,
        owner: u16,
    ) -> Result<Vec<Collision>, CoreError> {
        if toolpath.side == active {
            self.cut(toolpath, owner)
        } else if setup.is_double_sided() {
            self.sweep(&flipped(toolpath, setup), owner, true)
        } else {
            Ok(Vec::new())
        }
    }

    /// Sweeps one toolpath through the stock and answers the rapids that
    /// hit material. `owner` tags the cells it cuts.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the toolpath's tool cannot be
    /// rasterised.
    pub fn cut(&mut self, toolpath: &Toolpath, owner: u16) -> Result<Vec<Collision>, CoreError> {
        self.sweep(toolpath, owner, false)
    }

    /// Sweeps one toolpath down from the top or, with `below`, up from the
    /// underside.
    fn sweep(
        &mut self,
        toolpath: &Toolpath,
        owner: u16,
        below: bool,
    ) -> Result<Vec<Collision>, CoreError> {
        if let Some(burn) = Burn::of(toolpath) {
            // A laser chars the face it is over, and the other side's
            // laser work is on the underside, out of sight.
            if !below {
                let mut sweep = Sweep::default();
                for (index, (from, one)) in toolpath.iter_moves().enumerate() {
                    let part = MovePart {
                        toolpath_id: toolpath.id,
                        index,
                        from,
                        one,
                        t0: 0.0,
                        t1: 1.0,
                    };
                    self.burn_move(&mut sweep, &part, &burn);
                }
            }
            return Ok(Vec::new());
        }
        let cutter = Cutter::new(&Footprint::of(&toolpath.tool, self.cell)?);
        let mut sweep = Sweep::default();
        let mut collisions = Vec::new();
        for (index, (from, one)) in toolpath.iter_moves().enumerate() {
            let part = MovePart {
                toolpath_id: toolpath.id,
                index,
                from,
                one,
                t0: 0.0,
                t1: 1.0,
            };
            if let Some(hit) = self.cut_move(&cutter, &mut sweep, &part, owner, below) {
                collisions.push(hit);
            }
        }
        self.finish_sweep(&cutter, &mut sweep, owner, below);
        Ok(collisions)
    }

    /// Sweeps the tool along part of one move, from fraction `t0` to `t1`
    /// of its length, and answers the collision if it is a rapid that
    /// enters the material there. The stamps may wait in `sweep`, the last
    /// for the next move: [`Stock::flush_sweep`] cuts the ones made and
    /// [`Stock::finish_sweep`] makes and cuts them all.
    pub fn cut_move(
        &mut self,
        cutter: &Cutter,
        sweep: &mut Sweep,
        part: &MovePart<'_>,
        owner: u16,
        below: bool,
    ) -> Option<Collision> {
        let (from, one) = (part.from, part.one);
        let (t0, t1) = (part.t0, part.t1);
        let step = self.cell * 0.5;
        match one.kind {
            MoveKind::Dwell { .. } => {}
            MoveKind::Rapid => {
                // Lifting straight up only passes through material the
                // tool is already in, which was reported on the way in.
                let away = if below {
                    one.to.z <= from.z
                } else {
                    one.to.z >= from.z
                };
                let straight_up =
                    (from.x - one.to.x).abs() < 1e-9 && (from.y - one.to.y).abs() < 1e-9 && away;
                if straight_up {
                    return None;
                }
                // The material the rapid passes must be cut up to here.
                self.finish_sweep(cutter, sweep, owner, below);
                let length = from.distance(one.to) * (t1 - t0);
                let steps = (length / step).ceil().max(1.0) as usize;
                for k in 1..=steps {
                    let p = lerp(from, one.to, t0 + (t1 - t0) * k as f64 / steps as f64);
                    let inside = if below {
                        self.floor_at(p.x, p.y).is_some_and(|floor| {
                            p.z > f64::from(floor) + 1e-3 && !self.is_through(p.x, p.y)
                        })
                    } else {
                        self.height_at(p.x, p.y).is_some_and(|material| {
                            p.z < f64::from(material) - 1e-3 && !self.is_through(p.x, p.y)
                        })
                    };
                    if inside {
                        return Some(Collision {
                            toolpath_id: part.toolpath_id,
                            move_index: part.index,
                            x: p.x,
                            y: p.y,
                            z: p.z,
                            kind: IssueKind::Rapid,
                            severity: IssueKind::Rapid.severity(),
                        });
                    }
                }
            }
            MoveKind::Feed | MoveKind::Plunge => {
                let length = from.distance(one.to) * (t1 - t0);
                let steps = (length / step).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = t0 + (t1 - t0) * k as f64 / steps as f64;
                    self.sample(cutter, sweep, lerp(from, one.to, t), owner, below);
                }
            }
            MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. } => {
                let length = one.length(from) * (t1 - t0);
                let steps = (length / step).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = t0 + (t1 - t0) * k as f64 / steps as f64;
                    if let Some(p) = point_on(from, one, t) {
                        self.sample(cutter, sweep, p, owner, below);
                    }
                }
            }
        }
        None
    }

    /// Chars the surface along part of one laser move, from fraction `t0`
    /// to `t1` of its length: every cell the beam passes over gets at least
    /// the move's darkness. Rapids and dwells leave no mark.
    pub fn burn_move(&mut self, sweep: &mut Sweep, part: &MovePart<'_>, burn: &Burn) {
        let level = burn.level(part.index);
        let (from, one) = (part.from, part.one);
        let (t0, t1) = (part.t0, part.t1);
        if level == 0 || matches!(one.kind, MoveKind::Rapid | MoveKind::Dwell { .. }) {
            return;
        }
        if self.burn.len() != self.z.len() {
            self.burn = vec![0; self.z.len()];
        }
        let step = self.cell * 0.5;
        let length = one.length(from) * (t1 - t0);
        let steps = (length / step).ceil().max(1.0) as usize;
        let reach = (burn.radius / self.cell).floor() as i64;
        for k in 0..=steps {
            let t = t0 + (t1 - t0) * k as f64 / steps as f64;
            let Some(p) = point_on(from, one, t) else {
                continue;
            };
            let (cx, cy) = self.cell_of(p.x, p.y);
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
                        continue;
                    }
                    let off = (dx as f64).hypot(dy as f64) * self.cell;
                    if (dx != 0 || dy != 0) && off > burn.radius {
                        continue;
                    }
                    let (x, y) = (x as usize, y as usize);
                    let cell = &mut self.burn[y * self.width + x];
                    if *cell < level {
                        *cell = level;
                        if let Some(changed) = sweep.changed.as_mut() {
                            changed.note(x, y);
                        }
                    }
                }
            }
        }
    }

    /// Makes the stamps waiting in `sweep` and cuts every stamp it holds,
    /// so the stock shows every cut up to where the tool is.
    pub fn finish_sweep(&mut self, cutter: &Cutter, sweep: &mut Sweep, owner: u16, below: bool) {
        while !sweep.ahead.is_empty() {
            self.stamp_next(cutter, sweep, owner, below);
        }
        self.flush_sweep(cutter, sweep, owner, below);
    }

    /// Cuts the stamps `sweep` has made so far into the stock, leaving the
    /// samples that still wait for the ones after them: the stock is then
    /// as if each stamp had cut as soon as it was made. A sweep keeps its
    /// stamps until then, so anything that reads the stock part way along
    /// the moves (the collision checks) flushes it first.
    ///
    /// Every cell ends at the lowest tool bottom any stamp puts over it
    /// (the highest from underneath), whatever order the stamps come in,
    /// so a large batch is worked out on several threads against the stock
    /// as it is and the cells it lowers are cut after, one by one.
    pub fn flush_sweep(&mut self, cutter: &Cutter, sweep: &mut Sweep, owner: u16, below: bool) {
        if sweep.batch.is_empty() {
            return;
        }
        let mut batch = std::mem::take(&mut sweep.batch);
        let heights = (self.top as f32, self.bottom as f32);
        let threads = rayon::current_num_threads();
        let lines = (2 * cutter.reach.max(0) as usize + 1) * batch.len();
        if lines < MANY_LINES || threads < 2 {
            for stamp in &batch {
                let at = stamp.at;
                stamp_cells(cutter, stamp, heights, below, |dx, dy, rise| {
                    if let Some((index, h)) = self.lowered(at, dx, dy, rise, below) {
                        self.set_cell(index, h, owner, below, &mut sweep.changed);
                    }
                });
            }
        } else {
            let stock = &*self;
            let chunk = batch.len().div_ceil(threads * 4).max(FEWEST_A_THREAD);
            let found: Vec<Vec<(usize, f32)>> = batch
                .par_chunks(chunk)
                .map(|stamps| {
                    let mut out = Vec::new();
                    for stamp in stamps {
                        let at = stamp.at;
                        stamp_cells(cutter, stamp, heights, below, |dx, dy, rise| {
                            out.extend(stock.lowered(at, dx, dy, rise, below));
                        });
                    }
                    out
                })
                .collect();
            for (index, h) in found.into_iter().flatten() {
                self.set_cell(index, h, owner, below, &mut sweep.changed);
            }
        }
        batch.clear();
        sweep.batch = batch;
    }

    /// Makes the oldest waiting stamp of `sweep`.
    fn stamp_next(&mut self, cutter: &Cutter, sweep: &mut Sweep, owner: u16, below: bool) {
        let at = sweep.ahead.remove(0);
        let after = [sweep.ahead.first().copied(), sweep.ahead.get(1).copied()];
        sweep.batch.push(Stamp {
            before: sweep.back,
            at,
            after,
        });
        sweep.back = [Some(at), sweep.back[0]];
        if sweep.batch.len() >= MOST_BATCHED {
            self.flush_sweep(cutter, sweep, owner, below);
        }
    }

    /// Takes the tool tip at `at` along a sweep. Samples in the same cell
    /// make one stamp at the deepest tip; a stamp waits for the next two
    /// samples in other cells, which tell it which cells it need not cut.
    fn sample(&mut self, cutter: &Cutter, sweep: &mut Sweep, at: Point3, owner: u16, below: bool) {
        let next = Sample {
            centre: self.cell_of(at.x, at.y),
            tip: at.z as f32,
        };
        if let Some(newest) = sweep.ahead.last_mut() {
            if newest.centre == next.centre {
                newest.tip = if below {
                    newest.tip.max(next.tip)
                } else {
                    newest.tip.min(next.tip)
                };
                return;
            }
        }
        sweep.ahead.push(next);
        if sweep.ahead.len() == 3 {
            self.stamp_next(cutter, sweep, owner, below);
        }
    }

    /// The cell (`dx`, `dy`) from the stamp at `at`, with the tool bottom
    /// `rise` above its tip, and the height the stamp leaves it at, when
    /// the cell is in the stock and the stamp cuts it.
    #[inline]
    fn lowered(
        &self,
        at: Sample,
        dx: i32,
        dy: i32,
        rise: f32,
        below: bool,
    ) -> Option<(usize, f32)> {
        let (x, y) = (at.centre.0 + i64::from(dx), at.centre.1 + i64::from(dy));
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        let index = y as usize * self.width + x as usize;
        if below {
            let reach = (at.tip - rise).min(self.top as f32);
            (reach > self.floor[index]).then_some((index, reach))
        } else {
            let bottom = (at.tip + rise).max(self.bottom as f32);
            (bottom < self.z[index]).then_some((index, bottom))
        }
    }

    /// Cuts cell `index` to height `h` if that is lower (higher from
    /// underneath), noting it in `changed`.
    #[inline]
    fn set_cell(
        &mut self,
        index: usize,
        h: f32,
        owner: u16,
        below: bool,
        changed: &mut Option<Changed>,
    ) {
        if below {
            if h <= self.floor[index] {
                return;
            }
            self.floor[index] = h;
        } else {
            if h >= self.z[index] {
                return;
            }
            self.z[index] = h;
        }
        self.owner[index] = owner;
        if let Some(changed) = changed {
            changed.note(index % self.width, index / self.width);
        }
    }

    /// The heights the stock can hold: from the material bottom to its top.
    fn height_span(&self) -> (f32, f32) {
        let (a, b) = (self.bottom as f32, self.top as f32);
        (a.min(b), a.max(b))
    }

    /// The `u16` code preview heights are sent under: steps over the whole
    /// material thickness, so every patch of an animation fits the code of
    /// the heightfield it started from and the view can copy its codes
    /// straight in. `None` when those steps would round a height by more
    /// than [`crate::blob::HEIGHT_TOLERANCE`] of a cell (thick material on
    /// a fine grid); the heights then go as `f32`.
    pub fn height_code(&self) -> Option<crate::blob::HeightCode> {
        let (low, high) = self.height_span();
        crate::blob::HeightCode::fit(low, high, self.cell * crate::blob::HEIGHT_TOLERANCE)
    }

    /// The heightfield as a `BLOB` reply body for the view: the `z`
    /// heights, the `owner` u16 section and, once the other side of a double
    /// sided job has cut into the underside, the `floor` heights (without
    /// it every cell's underside is `bottom`). Cells with no material left
    /// are NaN in both. Heights go as `u16` steps under
    /// [`Stock::height_code`] (or over the range they span, should one lie
    /// outside the material), as `f32` when no code is fine enough.
    pub fn to_blob(&self, extra: serde_json::Value) -> Vec<u8> {
        use crate::blob;
        let through = |i: usize| self.floor[i] >= self.z[i] - THROUGH;
        let open = |values: &[f32]| -> Vec<f32> {
            values
                .iter()
                .enumerate()
                .map(|(i, v)| if through(i) { f32::NAN } else { *v })
                .collect()
        };
        let z = open(&self.z);
        let bottom = self.bottom as f32;
        let flat = self
            .floor
            .iter()
            .enumerate()
            .all(|(i, f)| *f == bottom || through(i));
        let floor = (!flat).then(|| open(&self.floor));
        let range = blob::height_range(z.iter().chain(floor.iter().flatten()));
        let (low, high) = self.height_span();
        let code = match range {
            Some((lo, hi)) if lo < low || hi > high => {
                blob::HeightCode::fit(lo, hi, self.cell * blob::HEIGHT_TOLERANCE)
            }
            _ => self.height_code(),
        };
        let mut header = serde_json::json!({
            "width": self.width,
            "height": self.height,
            "cellSize": self.cell,
            "originX": self.origin_x,
            "originY": self.origin_y,
            "top": self.top,
            "bottom": self.bottom,
        });
        if let (Some(target), Some(more)) = (header.as_object_mut(), extra.as_object()) {
            for (key, value) in more {
                target.insert(key.clone(), value.clone());
            }
        }
        blob::describe_heights(&mut header, code.as_ref());
        let mut sections = vec![
            blob::heights("z", &z, code.as_ref()),
            blob::Section::u16("owner", &self.owner),
        ];
        if let Some(floor) = &floor {
            sections.push(blob::heights("floor", floor, code.as_ref()));
        }
        if self.burn.iter().any(|b| *b > 0) {
            sections.push(blob::Section::u8("burn", self.burn.clone()));
        }
        blob::frame(header, &sections)
    }
}

/// The cells the tool at `stamp.at` cuts down from the top or, for the
/// other side of a double sided job, up from underneath, between the two
/// stamps before it and the two after it of the same sweep: `cut` gets each
/// one's offset from the tool centre and the tool bottom's height above
/// the tip there. `heights` are the stock's top and bottom.
///
/// Every cell ends at the lowest tool bottom any stamp of the sweep puts
/// over it (the highest from underneath). The first stamp to reach that
/// lowest bottom is lower there than any stamp before it and no higher
/// than any after it, so each stamp only cuts the cells where that holds
/// against its neighbours: the band across the move where it is the
/// lowest. The rest the stamps either side cut at least as low, and the
/// stock comes out exactly as if every stamp cut the whole tool. Cells
/// whose tool bottom lies above the material are left out too.
fn stamp_cells(
    cutter: &Cutter,
    stamp: &Stamp,
    (top, bottom): (f32, f32),
    below: bool,
    mut cut: impl FnMut(i32, i32, f32),
) {
    let Stamp { before, at, after } = *stamp;
    // Heights measured into the material: down from the top, or up from
    // the underside.
    let sign = if below { -1.0 } else { 1.0 };
    let tip = f64::from(at.tip);
    let depth = if below {
        tip - f64::from(bottom)
    } else {
        f64::from(top) - tip
    };
    let tall = depth + SLACK;
    if cutter.cells.first().is_none_or(|c| f64::from(c.2) >= tall) {
        // The whole tool is out of the material.
        return;
    }
    // The stamps compared with: the cell at `o` from this stamp lies at
    // `o + shift` from the other, and this stamp must be lower there than
    // one before (no higher than one after) by more than how far the tool
    // sank since (rose to it; from underneath the other way).
    let mut compared: [Option<Compared>; 4] = [None; 4];
    let others = [
        (before[0], true),
        (before[1], true),
        (after[0], false),
        (after[1], false),
    ];
    for (slot, (other, earlier)) in compared.iter_mut().zip(others) {
        let Some(o) = other else {
            continue;
        };
        let (sx, sy) = (at.centre.0 - o.centre.0, at.centre.1 - o.centre.1);
        if sx.abs() > i64::from(FAR) || sy.abs() > i64::from(FAR) {
            continue;
        }
        let (limit, strict) = bound(sign * (f64::from(o.tip) - tip));
        let one = Compared {
            shift: (sx as i32, sy as i32),
            limit,
            strict: earlier && strict,
            earlier,
        };
        if sx == 0 && sy == 0 && !one.holds(0.0) {
            // A stamp in the same place at least as deep.
            return;
        }
        *slot = Some(one);
    }
    let compared = || compared.iter().flatten();
    let step = |other: Option<Sample>, from: Sample| {
        other.and_then(|o| neighbour(from.centre.0 - o.centre.0, from.centre.1 - o.centre.1))
    };
    let direction = step(before[0], at).or_else(|| {
        after[0].and_then(|n| neighbour(n.centre.0 - at.centre.0, n.centre.1 - at.centre.1))
    });
    // A level stamp on a straight run: its band, worked out once.
    if let (Some(j), Some(b), Some(n)) = (direction, before[0], after[0]) {
        let level = b.tip == at.tip && n.tip == at.tip;
        let straight = (n.centre.0 - at.centre.0, n.centre.1 - at.centre.1)
            == (at.centre.0 - b.centre.0, at.centre.1 - b.centre.1);
        if level && straight {
            let band = cutter.band(j);
            let count = band.partition_point(|c| f64::from(c.2) < tall);
            for &(dx, dy, rise) in &band[..count] {
                cut(dx, dy, rise);
            }
            return;
        }
    }
    let order = direction.and_then(|j| cutter.order(j).map(|order| (j, order)));
    let Some((j, order)) = order else {
        // No lines to follow: every cell under the material, checked.
        let count = cutter.cells.partition_point(|c| f64::from(c.2) < tall);
        for &(dx, dy, rise) in &cutter.cells[..count] {
            let fine =
                compared().all(|c| c.holds(rise - cutter.rise_at(dx + c.shift.0, dy + c.shift.1)));
            if fine {
                cut(dx, dy, rise);
            }
        }
        return;
    };
    let mut profiles: [Option<(&Profile, Compared)>; 4] = [None; 4];
    for (slot, c) in profiles.iter_mut().zip(compared()) {
        *slot = cutter
            .profile(j, order, c.shift, c.earlier)
            .map(|p| (p, *c));
    }
    let profiles = || profiles.iter().flatten();
    // Each profile's differences and the `f32` they must stay below, four
    // every time: those past the profiles are always met.
    let mut checks: [(&[f32], f32); 4] = [(&[], f32::INFINITY); 4];
    let mut count = 0;
    for (p, c) in profiles() {
        checks[count] = (p.difference.as_slice(), c.below());
        count += 1;
    }
    if count == 0 {
        // Nothing to compare with: every cell under the material.
        for &(dx, dy, rise) in &order.cells {
            if f64::from(rise) < tall {
                cut(dx, dy, rise);
            }
        }
        return;
    }
    let pad = checks[0].0;
    for check in &mut checks[count..] {
        check.0 = pad;
    }
    for &(start, end, lowest_rise) in &order.spans {
        if f64::from(lowest_rise) >= tall {
            continue;
        }
        let (start, end) = (start as usize, end as usize);
        // Start where every stamp before has been beaten somewhere, stop
        // where no later cell of the line is as low as a stamp after.
        let (mut first, mut stop) = (start, end);
        for (&(p, c), &(_, below)) in profiles().zip(&checks) {
            let lowest = &p.lowest[start..end];
            if c.earlier {
                first = first.max(start + lowest.partition_point(|&m| m >= below));
            } else {
                stop = stop.min(start + lowest.partition_point(|&m| m < below));
            }
        }
        if first >= stop {
            continue;
        }
        let cells = &order.cells[first..stop];
        let n = cells.len();
        let [(a, ta), (b, tb), (c, tc), (d, td)] = checks.map(|(d, t)| (&d[first..stop], t));
        let (a, b, c, d) = (&a[..n], &b[..n], &c[..n], &d[..n]);
        for k in 0..n {
            if a[k] < ta && b[k] < tb && c[k] < tc && d[k] < td {
                let (dx, dy, rise) = cells[k];
                if f64::from(rise) < tall {
                    cut(dx, dy, rise);
                }
            }
        }
    }
}

/// The least `f32` above `x`.
fn next_up(x: f32) -> f32 {
    if x.is_nan() || x == f32::INFINITY {
        x
    } else if x == 0.0 {
        f32::from_bits(1)
    } else if x > 0.0 {
        f32::from_bits(x.to_bits() + 1)
    } else {
        f32::from_bits(x.to_bits() - 1)
    }
}

/// How close the underside may come to the top before a cell counts as cut
/// through, in millimetres.
pub const THROUGH: f32 = 1e-4;

/// A stamp another is compared with: the offset of the other's centre
/// from this one's, and the bound on the difference in rise there.
#[derive(Debug, Clone, Copy)]
struct Compared {
    shift: (i32, i32),
    limit: f64,
    strict: bool,
    /// The other stamp came before this one.
    earlier: bool,
}

impl Compared {
    /// True when the difference `d` meets the bound.
    #[inline]
    fn holds(&self, d: f32) -> bool {
        let d = f64::from(d);
        d < self.limit || (!self.strict && d <= self.limit)
    }

    /// The least `f32` a difference that meets the bound stays below: `d`
    /// meets it just when `d` is below this.
    fn below(&self) -> f32 {
        // The nearest `f32` at or above the limit.
        let mut at = self.limit as f32;
        if f64::from(at) < self.limit {
            at = next_up(at);
        }
        if !self.strict && f64::from(at) == self.limit {
            next_up(at)
        } else {
            at
        }
    }
}

/// The limit a neighbour difference is held to for a height change of
/// `change` between two stamps, and whether it must be strictly below it:
/// at the same height the `f32` differences compare exactly; otherwise
/// with [`SLACK`] to spare, so no cell is ever wrongly left out.
fn bound(change: f64) -> (f64, bool) {
    if change == 0.0 {
        (0.0, true)
    } else {
        (change + SLACK, false)
    }
}

/// A toolpath of one side of a double sided job with its moves and safe
/// height mapped into the other side's job space. Mirroring reverses arc
/// directions.
pub fn flipped(toolpath: &Toolpath, setup: &JobSetup) -> Toolpath {
    let from = toolpath.side;
    let point = |p: Point3| {
        let (x, y) = setup.flip_xy(p.x, p.y);
        Point3::new(x, y, setup.flip_z(p.z, from))
    };
    let centre = |c: crate::geom::Point| {
        let (x, y) = setup.flip_xy(c.x, c.y);
        crate::geom::Point::new(x, y)
    };
    let mut out = toolpath.clone();
    out.side = from.other();
    out.home = point(toolpath.home);
    out.safe_z = setup.flip_z(toolpath.safe_z, from);
    for one in &mut out.moves {
        one.to = point(one.to);
        one.kind = match one.kind {
            MoveKind::ArcCw { centre: c } => MoveKind::ArcCcw { centre: centre(c) },
            MoveKind::ArcCcw { centre: c } => MoveKind::ArcCw { centre: centre(c) },
            other => other,
        };
    }
    out.bounds = None;
    out
}

/// Part of one move of a toolpath, for sweeping a preview a little at a
/// time.
#[derive(Debug, Clone, Copy)]
pub struct MovePart<'a> {
    /// The toolpath the move belongs to.
    pub toolpath_id: u64,
    /// The move's index in the toolpath.
    pub index: usize,
    /// Where the move starts.
    pub from: Point3,
    /// The move.
    pub one: &'a Move,
    /// Fraction of the move's length the part starts at, from 0.
    pub t0: f64,
    /// Fraction of the move's length the part ends at, up to 1.
    pub t1: f64,
}

/// How dark a laser leaves the surface at `power` percent of its maximum
/// moving at `feed` millimetres per minute, 0 (no mark) to 255 (black).
/// `max_burn_rate` is the tool's Maximum Burn Rate, the speed at which full
/// power chars black: at half the power, or twice the speed, the mark is
/// half as dark.
pub fn burn_level(power: f64, feed: f64, max_burn_rate: f64) -> u8 {
    if !(power > 0.0 && feed > 0.0 && max_burn_rate > 0.0) {
        return 0;
    }
    let dose = (power / 100.0) * (max_burn_rate / feed);
    (dose.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// How a laser toolpath marks the preview: the darkness of each move, from
/// the power in force there (the tool's until a move sets its own) and the
/// speed it moves at, and half the beam's kerf.
#[derive(Debug, Clone, PartialEq)]
pub struct Burn {
    levels: Vec<u8>,
    radius: f64,
}

impl Burn {
    /// The marks of `toolpath`, or `None` when its tool is not a laser.
    pub fn of(toolpath: &Toolpath) -> Option<Burn> {
        let ToolGeometry::LaserTool {
            power,
            kerf,
            max_burn_rate,
            ..
        } = toolpath.tool.tool_type
        else {
            return None;
        };
        let mut now = power;
        let levels = toolpath
            .moves
            .iter()
            .enumerate()
            .map(|(index, one)| {
                if let Some(set) = one.power {
                    now = set;
                }
                let rates = toolpath.rates_at(index);
                let feed = if one.kind == MoveKind::Plunge {
                    rates.plunge
                } else {
                    rates.feed
                };
                burn_level(now, feed, max_burn_rate)
            })
            .collect();
        Some(Burn {
            levels,
            radius: kerf.max(0.0) / 2.0,
        })
    }

    /// The darkness move `index` leaves.
    pub fn level(&self, index: usize) -> u8 {
        self.levels.get(index).copied().unwrap_or(0)
    }
}

/// The tool tip `t` of the way along a move (by length), or `None` for a
/// dwell.
pub fn point_on(from: Point3, one: &Move, t: f64) -> Option<Point3> {
    match one.kind {
        MoveKind::Dwell { .. } => None,
        MoveKind::Rapid | MoveKind::Feed | MoveKind::Plunge => Some(lerp(from, one.to, t)),
        MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
            let ccw = matches!(one.kind, MoveKind::ArcCcw { .. });
            let a0 = (from.y - centre.y).atan2(from.x - centre.x);
            let a1 = (one.to.y - centre.y).atan2(one.to.x - centre.x);
            let tau = std::f64::consts::TAU;
            let mut sweep = a1 - a0;
            if ccw {
                while sweep <= 1e-12 {
                    sweep += tau;
                }
            } else {
                while sweep >= -1e-12 {
                    sweep -= tau;
                }
            }
            let radius = (from.x - centre.x).hypot(from.y - centre.y);
            let a = a0 + sweep * t;
            Some(Point3::new(
                centre.x + radius * a.cos(),
                centre.y + radius * a.sin(),
                from.z + (one.to.z - from.z) * t,
            ))
        }
    }
}

fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    Point3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, StrategyKind};
    use crate::geom::Point;

    fn job() -> JobSetup {
        JobSetup {
            width: 100.0,
            height: 50.0,
            ..JobSetup::default()
        }
    }

    fn toolpath(tool: ToolGeometry, moves: Vec<Move>) -> Toolpath {
        let mut tp = Toolpath::new(
            1,
            "test",
            Tool {
                name: "t".into(),
                tool_type: tool,
                ..Tool::default()
            },
            StrategyKind::Profile,
        );
        tp.home = Point3::new(10.0, 25.0, 20.0);
        tp.moves = moves;
        tp
    }

    fn slot() -> Vec<Move> {
        vec![
            Move::rapid(Point3::new(10.0, 25.0, 5.0)),
            Move::plunge(Point3::new(10.0, 25.0, -2.0)),
            Move::feed(Point3::new(90.0, 25.0, -2.0)),
            Move::rapid(Point3::new(90.0, 25.0, 5.0)),
        ]
    }

    /// The stock a toolpath of `side` leaves on `setup` seen from the top
    /// when every sample stamps the tool's whole footprint, down from the
    /// top or up from underneath.
    fn whole_stamps(setup: &JobSetup, tp: &Toolpath, quality: Quality) -> Stock {
        let mut stock = Stock::new(setup, quality);
        let below = tp.side != Side::Top;
        let tp = if below {
            flipped(tp, setup)
        } else {
            tp.clone()
        };
        let footprint = Footprint::of(&tp.tool, stock.cell).expect("footprint");
        let step = stock.cell * 0.5;
        let (floor, ceiling) = (stock.bottom as f32, stock.top as f32);
        let mut from = tp.home;
        for one in &tp.moves {
            if !matches!(one.kind, MoveKind::Rapid | MoveKind::Dwell { .. }) {
                let length = one.length(from);
                let steps = (length / step).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    let Some(p) = point_on(from, one, t) else {
                        continue;
                    };
                    let (cx, cy) = stock.cell_of(p.x, p.y);
                    let tip = p.z as f32;
                    for &(dx, dy, rise) in footprint.cells() {
                        let (x, y) = (cx + i64::from(dx), cy + i64::from(dy));
                        if x < 0 || y < 0 || x >= stock.width as i64 || y >= stock.height as i64 {
                            continue;
                        }
                        let i = y as usize * stock.width + x as usize;
                        if below {
                            let reach = (tip - rise).min(ceiling);
                            if reach > stock.floor[i] {
                                stock.floor[i] = reach;
                                stock.owner[i] = 1;
                            }
                        } else {
                            let bottom = (tip + rise).max(floor);
                            if bottom < stock.z[i] {
                                stock.z[i] = bottom;
                                stock.owner[i] = 1;
                            }
                        }
                    }
                }
            }
            from = one.to;
        }
        stock
    }

    /// Moves that slope, turn and wander: flat, ramping, arcs at a slant,
    /// a zig-zag of V carving depths, a 3D surface in small steps, a rapid
    /// over the material and back in, and moves through the material top.
    fn awkward_moves() -> Vec<Move> {
        let mut moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, 5.0)),
            Move::plunge(Point3::new(10.0, 10.0, -2.0)),
            Move::feed(Point3::new(80.0, 13.0, -2.0)),
            Move::arc_ccw(Point::new(80.0, 25.0), Point3::new(80.0, 37.0, -2.0)),
            Move::feed(Point3::new(20.0, 40.0, -3.5)),
            Move::feed(Point3::new(35.0, 20.0, -1.0)),
            Move::arc_cw(Point::new(50.0, 20.0), Point3::new(65.0, 20.0, -2.5)),
            Move::feed(Point3::new(30.0, 30.0, 0.5)),
            Move::feed(Point3::new(12.0, 44.0, -4.0)),
        ];
        for k in 1..=40 {
            let x = 12.0 + f64::from(k) * 1.7;
            let y = 44.0 - f64::from(k % 5) * 0.9;
            let z = if k % 2 == 0 { -0.4 } else { -2.7 };
            moves.push(Move::feed(Point3::new(x, y, z)));
        }
        for k in 0..=120 {
            let x = 30.0 + f64::from(k) * 0.3;
            let y = 8.0 + (f64::from(k) * 0.21).sin() * 3.0;
            let z = -1.5 - (f64::from(k) * 0.13).cos() * 1.2;
            moves.push(Move::feed(Point3::new(x, y, z)));
        }
        moves.push(Move::rapid(Point3::new(66.0, 8.0, 3.0)));
        moves.push(Move::rapid(Point3::new(40.0, 30.0, 3.0)));
        moves.push(Move::plunge(Point3::new(40.0, 30.0, -3.0)));
        moves.push(Move::feed(Point3::new(41.0, 30.2, -3.0)));
        moves.push(Move::feed(Point3::new(41.0, 30.2, -3.2)));
        moves.push(Move::feed(Point3::new(90.0, 46.0, -1.0)));
        moves.push(Move::rapid(Point3::new(90.0, 46.0, 5.0)));
        moves
    }

    fn awkward_tools() -> [ToolGeometry; 4] {
        [
            ToolGeometry::EndMill { diameter: 6.0 },
            ToolGeometry::BallNose { diameter: 4.0 },
            ToolGeometry::VBit {
                diameter: 6.0,
                included_angle: 60.0,
            },
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
        ]
    }

    #[test]
    fn stamping_only_the_cells_a_stamp_cuts_lowest_leaves_the_same_stock() {
        for quality in [Quality::Standard, Quality::High] {
            for tool in awkward_tools() {
                let tp = toolpath(tool.clone(), awkward_moves());
                let mut stock = Stock::new(&job(), quality);
                stock.cut(&tp, 1).expect("cuts");
                let whole = whole_stamps(&job(), &tp, quality);
                assert!(
                    stock.z != Stock::new(&job(), quality).z,
                    "{tool:?} cut nothing"
                );
                assert_eq!(stock.z, whole.z, "{tool:?} {quality:?}");
                assert_eq!(stock.owner, whole.owner, "{tool:?} {quality:?}");
            }
        }
    }

    #[test]
    fn stamping_from_underneath_leaves_the_same_stock() {
        let setup = two_sided();
        for tool in awkward_tools() {
            let mut tp = toolpath(tool.clone(), awkward_moves());
            tp.side = Side::Bottom;
            let mut stock = Stock::new(&setup, Quality::Standard);
            stock.cut_for(&setup, Side::Top, &tp, 1).expect("cuts");
            let whole = whole_stamps(&setup, &tp, Quality::Standard);
            assert!(
                stock.floor != Stock::new(&setup, Quality::Standard).floor,
                "{tool:?} cut nothing"
            );
            assert_eq!(stock.floor, whole.floor, "{tool:?}");
            assert_eq!(stock.owner, whole.owner, "{tool:?}");
        }
    }

    #[test]
    fn a_tracking_sweep_notes_the_tiles_it_cut_as_few_rectangles() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let tp = toolpath(ToolGeometry::EndMill { diameter: 6.0 }, slot());
        let cutter = Cutter::new(&Footprint::of(&tp.tool, stock.cell).expect("footprint"));
        let mut sweep = Sweep::tracking(&stock);
        for (index, (from, one)) in tp.iter_moves().enumerate() {
            let part = MovePart {
                toolpath_id: 1,
                index,
                from,
                one,
                t0: 0.0,
                t1: 1.0,
            };
            stock.cut_move(&cutter, &mut sweep, &part, 1, false);
        }
        stock.finish_sweep(&cutter, &mut sweep, 1, false);
        let rects = sweep.take_changed();
        let fresh = Stock::new(&job(), Quality::Standard);
        let inside = |i: usize| {
            let (x, y) = (i % stock.width, i / stock.width);
            rects
                .iter()
                .any(|&[x0, y0, w, h]| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h)
        };
        assert!((0..stock.z.len()).all(|i| stock.z[i] == fresh.z[i] || inside(i)));
        // A straight slot is one band of tiles, joined row to row.
        assert!(rects.len() <= 3, "{rects:?}");
        let sent: usize = rects.iter().map(|r| r[2] * r[3]).sum();
        assert!(sent < stock.z.len() / 4, "{sent}");
        // Once taken, nothing is left.
        assert!(sweep.take_changed().is_empty());
    }

    #[test]
    fn a_six_mm_end_mill_cuts_a_six_mm_slot_two_deep() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let collisions = stock
            .cut(
                &toolpath(ToolGeometry::EndMill { diameter: 6.0 }, slot()),
                1,
            )
            .expect("cuts");
        assert!(collisions.is_empty());
        assert_eq!(stock.height_at(50.0, 25.0), Some(-2.0));
        let cell = stock.cell;
        assert_eq!(stock.height_at(50.0, 25.0 + 3.0 - cell), Some(-2.0));
        assert_eq!(stock.height_at(50.0, 25.0 + 3.0 + cell), Some(0.0));
        assert_eq!(stock.height_at(50.0, 25.0 - 3.0 - cell), Some(0.0));
        assert_eq!(
            stock.height_at(5.0, 25.0),
            Some(0.0),
            "uncut beyond the ends"
        );
    }

    #[test]
    fn maximum_quality_has_25_times_the_cells_of_standard() {
        assert_eq!(Quality::Maximum.cells() / Quality::Standard.cells(), 25.0);
        let at = |q| Stock::new(&job(), q).cell;
        let ratio = (at(Quality::Standard) / at(Quality::Maximum)).powi(2);
        assert!((ratio - 25.0).abs() < 0.5, "{ratio}");
    }

    #[test]
    fn each_quality_level_is_finer_than_the_one_before() {
        let levels = [
            Quality::Standard,
            Quality::High,
            Quality::VeryHigh,
            Quality::ExtremelyHigh,
            Quality::Maximum,
        ];
        let cells: Vec<f64> = levels.iter().map(|q| Stock::new(&job(), *q).cell).collect();
        assert!(cells.windows(2).all(|pair| pair[1] < pair[0]), "{cells:?}");
        let names: Vec<String> = levels
            .iter()
            .map(|q| serde_json::to_string(q).expect("name"))
            .collect();
        assert_eq!(names[3], "\"extremelyHigh\"");
        assert_eq!(names[4], "\"maximum\"");
    }

    #[test]
    fn a_ball_nose_leaves_a_round_bottom() {
        let mut stock = Stock::new(&job(), Quality::VeryHigh);
        stock
            .cut(
                &toolpath(ToolGeometry::BallNose { diameter: 6.0 }, slot()),
                1,
            )
            .expect("cuts");
        for offset in [0.0, 0.8, 1.5, 2.2, 2.7] {
            let want = -2.0 + (3.0 - (9.0f64 - offset * offset).sqrt());
            let got = f64::from(stock.height_at(50.0, 25.0 + offset).expect("inside"));
            // A cell away from the sample point the sphere has moved on.
            let slope = offset / (9.0f64 - offset * offset).sqrt();
            assert!(
                (got - want).abs() < 1e-3 + slope * stock.cell,
                "{offset}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn a_v_bit_plunge_leaves_a_cone() {
        let mut stock = Stock::new(&job(), Quality::High);
        let moves = vec![
            Move::rapid(Point3::new(50.0, 25.0, 5.0)),
            Move::plunge(Point3::new(50.0, 25.0, -3.0)),
            Move::rapid(Point3::new(50.0, 25.0, 5.0)),
        ];
        stock
            .cut(
                &toolpath(
                    ToolGeometry::VBit {
                        diameter: 12.0,
                        included_angle: 90.0,
                    },
                    moves,
                ),
                1,
            )
            .expect("cuts");
        assert!((f64::from(stock.height_at(50.0, 25.0).expect("in")) + 3.0).abs() < 0.05);
        let at_one = f64::from(stock.height_at(51.0, 25.0).expect("in"));
        assert!((at_one + 2.0).abs() < stock.cell + 1e-3, "{at_one}");
        assert_eq!(stock.height_at(54.0, 25.0), Some(0.0));
    }

    #[test]
    fn a_rapid_through_material_is_a_collision() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let moves = vec![
            Move::rapid(Point3::new(10.0, 25.0, -1.0)),
            Move::rapid(Point3::new(10.0, 25.0, 5.0)),
            Move::rapid(Point3::new(80.0, 25.0, 5.0)),
        ];
        let collisions = stock
            .cut(&toolpath(ToolGeometry::EndMill { diameter: 6.0 }, moves), 1)
            .expect("cuts");
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0].move_index, 0);
    }

    #[test]
    fn an_arc_cuts_a_groove_without_gaps() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let moves = vec![
            Move::rapid(Point3::new(70.0, 25.0, 5.0)),
            Move::plunge(Point3::new(70.0, 25.0, -1.0)),
            Move::arc_ccw(Point::new(50.0, 25.0), Point3::new(30.0, 25.0, -1.0)),
        ];
        stock
            .cut(&toolpath(ToolGeometry::EndMill { diameter: 2.0 }, moves), 1)
            .expect("cuts");
        for degrees in (0..=180).step_by(5) {
            let a = f64::from(degrees).to_radians();
            let (x, y) = (50.0 + 20.0 * a.cos(), 25.0 + 20.0 * a.sin());
            if y > 49.0 {
                continue;
            }
            assert_eq!(stock.height_at(x, y), Some(-1.0), "at {degrees} degrees");
        }
    }

    #[test]
    fn the_same_cut_gives_the_same_bytes() {
        let run = || {
            let mut stock = Stock::new(&job(), Quality::Standard);
            stock
                .cut(
                    &toolpath(ToolGeometry::BallNose { diameter: 6.0 }, slot()),
                    1,
                )
                .expect("cuts");
            stock.to_blob(serde_json::json!({}))
        };
        assert_eq!(run(), run());
    }

    fn two_sided() -> JobSetup {
        JobSetup {
            kind: crate::doc::JobKind::DoubleSided,
            ..job()
        }
    }

    /// A star of V carving: out from the middle and back over the same
    /// line at other depths, the tool buried past its shoulder.
    fn star_moves() -> Vec<Move> {
        let mut moves = vec![
            Move::rapid(Point3::new(50.0, 25.0, 5.0)),
            Move::plunge(Point3::new(50.0, 25.0, -1.0)),
        ];
        for k in 0..6 {
            let a = f64::from(k) * 1.1;
            let (x, y) = (50.0 + 15.0 * a.cos(), 25.0 + 12.0 * a.sin());
            moves.push(Move::feed(Point3::new(x, y, -1.0 - f64::from(k % 3))));
            moves.push(Move::feed(Point3::new(50.0, 25.0, -7.0)));
        }
        moves.push(Move::rapid(Point3::new(50.0, 25.0, 5.0)));
        moves
    }

    #[test]
    fn stamps_worked_out_on_several_threads_leave_the_same_stock() {
        let pool = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .expect("pool")
        };
        let (one, many) = (pool(1), pool(4));
        let setup = two_sided();
        let tools = [
            ToolGeometry::VBit {
                diameter: 6.0,
                included_angle: 60.0,
            },
            ToolGeometry::BallNose { diameter: 4.0 },
        ];
        for side in [Side::Top, Side::Bottom] {
            for tool in &tools {
                let mut tp = toolpath(tool.clone(), star_moves());
                tp.side = side;
                let cut = |pool: &rayon::ThreadPool| {
                    pool.install(|| {
                        let mut stock = Stock::new(&setup, Quality::Standard);
                        stock.cut_for(&setup, Side::Top, &tp, 1).expect("cuts");
                        stock
                    })
                };
                let (alone, together) = (cut(&one), cut(&many));
                assert!(together != Stock::new(&setup, Quality::Standard));
                assert!(alone == together, "{tool:?} {side:?}");
                let whole = whole_stamps(&setup, &tp, Quality::Standard);
                assert_eq!(together.z, whole.z, "{tool:?} {side:?}");
                assert_eq!(together.floor, whole.floor, "{tool:?} {side:?}");
                assert_eq!(together.owner, whole.owner, "{tool:?} {side:?}");
            }
        }
    }

    /// A 6 mm plunge `depth` deep at (x, y) on `side`.
    fn plunge_on(side: Side, x: f64, y: f64, depth: f64) -> Toolpath {
        let mut tp = toolpath(
            ToolGeometry::EndMill { diameter: 6.0 },
            vec![
                Move::rapid(Point3::new(x, y, 5.0)),
                Move::plunge(Point3::new(x, y, -depth)),
                Move::rapid(Point3::new(x, y, 5.0)),
            ],
        );
        tp.side = side;
        tp
    }

    #[test]
    fn flipping_a_toolpath_mirrors_it_and_reverses_its_arcs() {
        let setup = two_sided();
        let mut tp = toolpath(
            ToolGeometry::EndMill { diameter: 2.0 },
            vec![Move::arc_ccw(
                Point::new(30.0, 25.0),
                Point3::new(20.0, 25.0, -1.0),
            )],
        );
        tp.side = Side::Bottom;
        let out = flipped(&tp, &setup);
        assert_eq!(out.side, Side::Top);
        assert_eq!(out.moves[0].to, Point3::new(80.0, 25.0, -17.0));
        assert_eq!(
            out.moves[0].kind,
            MoveKind::ArcCw {
                centre: Point::new(70.0, 25.0)
            }
        );
        assert_eq!(flipped(&out, &setup), tp);
        // The safe height goes over with the moves: 5 above one face is 5
        // below the other.
        assert!((out.safe_z - setup.flip_z(tp.safe_z, Side::Bottom)).abs() < 1e-12);
    }

    #[test]
    fn turning_the_stock_over_puts_the_underside_on_top() {
        let mut setup = two_sided();
        for flip in [FlipAxis::Horizontal, FlipAxis::Vertical] {
            setup.double_sided.flip = flip;
            let mut stock = Stock::new(&setup, Quality::Standard);
            // A plunge from the bottom side at its (20, 25), seen from the top.
            let plunge = plunge_on(Side::Bottom, 20.0, 25.0, 4.0);
            stock.cut_for(&setup, Side::Top, &plunge, 1).expect("cuts");
            let turned = stock.turned(&setup, Side::Top);
            // Where the bottom side drilled, 4 deep from its own top.
            let at = turned.height_at(20.0, 25.0).expect("inside");
            let top = setup.for_side(Side::Bottom).surface_z();
            assert!((f64::from(at) - (top - 4.0)).abs() < 1e-4, "{flip:?} {at}");
            let (x, y) = setup.flip_xy(20.0, 25.0);
            let (cx, cy) = stock.cell_of(x, y);
            let i = cy as usize * stock.width + cx as usize;
            assert_eq!(turned.owner[stock.turned_cell(i, &setup)], 1);
            // Turned twice is the same stock.
            let back = turned.turned(&setup, Side::Bottom);
            let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
            assert!(near(back.origin_x, stock.origin_x) && near(back.origin_y, stock.origin_y));
            assert!(near(back.top, stock.top) && near(back.bottom, stock.bottom));
            assert_eq!(back.owner, stock.owner);
            let same = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-4);
            assert!(same(&back.z, &stock.z) && same(&back.floor, &stock.floor));
        }
    }

    #[test]
    fn cuts_from_the_other_side_come_up_from_underneath() {
        let setup = two_sided();
        let mut stock = Stock::new(&setup, Quality::Standard);
        stock
            .cut_for(
                &setup,
                Side::Top,
                &plunge_on(Side::Bottom, 20.0, 25.0, 5.0),
                1,
            )
            .expect("cuts");
        // Bottom (20, 25) is top (80, 25): the pocket is 5 mm into the
        // underside, so the top is untouched and the floor is 13 mm down.
        assert_eq!(stock.height_at(80.0, 25.0), Some(0.0));
        assert_eq!(stock.floor_at(80.0, 25.0), Some(-13.0));
        assert_eq!(stock.floor_at(20.0, 25.0), Some(-18.0));
        assert!(!stock.is_through(80.0, 25.0));
    }

    #[test]
    fn a_through_cut_is_seen_from_both_sides() {
        let setup = two_sided();
        // 10 mm from the top and 10 mm from the bottom of an 18 mm board
        // meet in the middle.
        let top = plunge_on(Side::Top, 80.0, 25.0, 10.0);
        let bottom = plunge_on(Side::Bottom, 20.0, 25.0, 10.0);
        for (active, x) in [(Side::Top, 80.0), (Side::Bottom, 20.0)] {
            let mut stock = Stock::new(&setup, Quality::Standard);
            for tp in [&top, &bottom] {
                stock.cut_for(&setup, active, tp, 1).expect("cuts");
            }
            assert!(stock.is_through(x, 25.0), "{active:?}");
            assert!(!stock.is_through(100.0 - x, 25.0), "{active:?}");
            let blob = stock.to_blob(serde_json::json!({}));
            let (_, z) = crate::blob::read_heights(&blob, "z");
            let through = (0..z.len())
                .filter(|i| stock.floor[*i] >= stock.z[*i] - THROUGH)
                .count();
            assert!(through > 0);
            assert_eq!(z.iter().filter(|v| v.is_nan()).count(), through);
        }
        // Either cut alone leaves material.
        let mut stock = Stock::new(&setup, Quality::Standard);
        stock.cut_for(&setup, Side::Top, &top, 1).expect("cuts");
        assert!(!stock.is_through(80.0, 25.0));
    }

    #[test]
    fn the_preview_goes_as_u16_heights_within_a_hundredth_of_a_cell() {
        let setup = two_sided();
        // Cut from below only: the underside is raised, not cut through.
        let mut stock = Stock::new(&setup, Quality::Standard);
        stock
            .cut_for(&setup, Side::Top, &plunge_on(Side::Top, 80.0, 25.0, 4.0), 1)
            .expect("cuts");
        stock
            .cut_for(
                &setup,
                Side::Top,
                &plunge_on(Side::Bottom, 20.0, 25.0, 6.0),
                2,
            )
            .expect("cuts");
        let blob = stock.to_blob(serde_json::json!({ "revision": 3 }));
        let (header, z) = crate::blob::read_heights(&blob, "z");
        let (_, floor) = crate::blob::read_heights(&blob, "floor");
        assert_eq!(header["revision"], serde_json::json!(3));
        assert_eq!(header["sections"][0]["dtype"], serde_json::json!("u16"));
        let step = header["heightStep"].as_f64().expect("a step");
        assert!(step * 0.5 <= stock.cell * crate::blob::HEIGHT_TOLERANCE);
        let off = |sent: &[f32], kept: &[f32]| {
            sent.iter()
                .zip(kept)
                .map(|(s, k)| f64::from((s - k).abs()))
                .fold(0.0, f64::max)
        };
        assert!(off(&z, &stock.z) <= step * 0.5 + 1e-5);
        assert!(off(&floor, &stock.floor) <= step * 0.5 + 1e-5);
        assert!(floor.iter().any(|f| *f > -17.0), "the raised underside");
        // A single sided preview leaves the flat underside out.
        let mut single = Stock::new(&job(), Quality::Standard);
        single
            .cut_for(&job(), Side::Top, &plunge_on(Side::Top, 80.0, 25.0, 4.0), 1)
            .expect("cuts");
        let blob = single.to_blob(serde_json::json!({}));
        let (header, _) = crate::blob::read_heights(&blob, "z");
        assert_eq!(header["sections"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn other_side_toolpaths_cut_nothing_on_a_single_sided_job() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let collisions = stock
            .cut_for(
                &job(),
                Side::Top,
                &plunge_on(Side::Bottom, 20.0, 25.0, 10.0),
                1,
            )
            .expect("cuts");
        assert!(collisions.is_empty());
        assert_eq!(stock.floor_at(80.0, 25.0), Some(-18.0));
    }

    #[test]
    fn nothing_cuts_below_the_bottom_of_the_material() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let moves = vec![
            Move::rapid(Point3::new(50.0, 25.0, 5.0)),
            Move::plunge(Point3::new(50.0, 25.0, -40.0)),
        ];
        stock
            .cut(&toolpath(ToolGeometry::EndMill { diameter: 6.0 }, moves), 1)
            .expect("cuts");
        assert_eq!(stock.height_at(50.0, 25.0), Some(-18.0));
    }

    #[test]
    fn a_piece_cut_free_is_removed_and_the_rest_stays() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        // Nothing is cut free yet: the material stays.
        assert_eq!(stock.remove_piece(20.0, 20.0), 0);
        // A slot through the material across the whole job at X 50.
        let through = vec![
            Move::rapid(Point3::new(50.0, -5.0, 5.0)),
            Move::plunge(Point3::new(50.0, -5.0, -20.0)),
            Move::feed(Point3::new(50.0, 55.0, -20.0)),
            Move::rapid(Point3::new(50.0, 55.0, 5.0)),
        ];
        stock
            .cut(
                &toolpath(ToolGeometry::EndMill { diameter: 6.0 }, through),
                1,
            )
            .expect("cuts");
        let removed = stock.remove_piece(20.0, 20.0);
        assert!(removed > 0);
        assert!(stock.is_through(20.0, 20.0));
        assert!(stock.is_through(5.0, 45.0));
        assert!(!stock.is_through(80.0, 20.0));
        // Over a hole or outside the stock nothing happens.
        assert_eq!(stock.remove_piece(20.0, 20.0), 0);
        assert_eq!(stock.remove_piece(-50.0, 20.0), 0);
        // The last piece left is the part, not waste.
        assert_eq!(stock.remove_piece(80.0, 20.0), 0);
        assert!(stock.same_grid(&Stock::new(&job(), Quality::Standard)));
        assert!(!stock.same_grid(&Stock::new(&job(), Quality::High)));
    }

    fn laser(power: f64, speed: f64, moves: Vec<Move>) -> Toolpath {
        let mut tp = toolpath(
            ToolGeometry::LaserTool {
                power,
                speed,
                kerf: 0.2,
                max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
            },
            moves,
        );
        tp.feed = speed;
        tp.plunge = speed;
        tp
    }

    fn burn_at(stock: &Stock, x: f64, y: f64) -> u8 {
        let (cx, cy) = stock.cell_of(x, y);
        stock.burn[cy as usize * stock.width + cx as usize]
    }

    #[test]
    fn a_laser_chars_the_surface_it_passes_over_and_cuts_nothing() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let untouched = stock.z.clone();
        // Full power at 3000 mm/min chars black; the tool's own 40 % at
        // the same speed on the second line, set by nothing but the tool.
        let mut line = Move::feed(Point3::new(90.0, 20.0, 0.0));
        line.power = Some(100.0);
        let mut back = Move::feed(Point3::new(10.0, 30.0, 0.0));
        back.power = Some(0.0);
        let tp = laser(
            40.0,
            3000.0,
            vec![
                Move::rapid(Point3::new(10.0, 10.0, 0.0)),
                Move::feed(Point3::new(90.0, 10.0, 0.0)),
                Move::rapid(Point3::new(10.0, 20.0, 0.0)),
                line,
                Move::rapid(Point3::new(90.0, 30.0, 0.0)),
                back,
            ],
        );
        let hits = stock.cut(&tp, 1).expect("burns");
        assert!(hits.is_empty());
        assert_eq!(stock.z, untouched, "a laser removes no material");
        assert_eq!(burn_at(&stock, 50.0, 10.0), 102);
        assert_eq!(burn_at(&stock, 50.0, 20.0), 255);
        // The beam at zero power and the rapids leave no mark.
        assert_eq!(burn_at(&stock, 50.0, 30.0), 0);
        assert_eq!(burn_at(&stock, 50.0, 15.0), 0);
        // A narrow line: a cell or so wide, like the beam.
        assert_eq!(burn_at(&stock, 50.0, 10.0 + 3.0 * stock.cell), 0);
    }

    #[test]
    fn a_slower_or_stronger_laser_chars_darker_and_marks_keep_the_darkest() {
        assert_eq!(burn_level(100.0, 3000.0, 3000.0), 255);
        assert_eq!(burn_level(50.0, 3000.0, 3000.0), 128);
        assert_eq!(burn_level(50.0, 6000.0, 3000.0), 64);
        assert_eq!(burn_level(100.0, 300.0, 3000.0), 255);
        assert_eq!(burn_level(0.0, 300.0, 3000.0), 0);
        // A higher Maximum Burn Rate chars darker at the same power and speed.
        assert_eq!(burn_level(50.0, 6000.0, 6000.0), 128);
        assert_eq!(burn_level(50.0, 6000.0, 1500.0), 32);
        let mut stock = Stock::new(&job(), Quality::Standard);
        let dark = laser(
            100.0,
            1500.0,
            vec![Move::feed(Point3::new(90.0, 25.0, 0.0))],
        );
        let light = laser(10.0, 3000.0, vec![Move::feed(Point3::new(90.0, 25.0, 0.0))]);
        stock.cut(&dark, 1).expect("burns");
        stock.cut(&light, 2).expect("burns");
        assert_eq!(burn_at(&stock, 50.0, 25.0), 255);
    }

    #[test]
    fn the_tools_maximum_burn_rate_sets_how_dark_the_preview_chars() {
        let burnt = |rate: f64| {
            let mut stock = Stock::new(&job(), Quality::Standard);
            let mut tp = laser(50.0, 6000.0, vec![Move::feed(Point3::new(90.0, 25.0, 0.0))]);
            if let ToolGeometry::LaserTool { max_burn_rate, .. } = &mut tp.tool.tool_type {
                *max_burn_rate = rate;
            }
            stock.cut(&tp, 1).expect("burns");
            burn_at(&stock, 50.0, 25.0)
        };
        assert_eq!(burnt(1500.0), 32);
        assert_eq!(burnt(3000.0), 64);
        assert_eq!(burnt(6000.0), 128);
        // A laser tool saved before the field existed reads the default.
        let old: ToolGeometry =
            serde_json::from_str(r#"{"kind":"laserTool","power":80,"speed":1200,"kerf":0.1}"#)
                .expect("reads");
        assert!(matches!(
            old,
            ToolGeometry::LaserTool { max_burn_rate, .. }
                if max_burn_rate == crate::cam::tool::DEFAULT_MAX_BURN_RATE
        ));
    }

    #[test]
    fn the_preview_blob_carries_the_laser_marks_once_there_are_any() {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let names = |blob: &[u8]| -> Vec<String> {
            let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
            let header: serde_json::Value =
                serde_json::from_slice(&blob[4..4 + len]).expect("header");
            header["sections"]
                .as_array()
                .expect("sections")
                .iter()
                .map(|s| s["name"].as_str().unwrap_or("").to_string())
                .collect()
        };
        assert!(!names(&stock.to_blob(serde_json::json!({}))).contains(&"burn".to_string()));
        stock
            .cut(
                &laser(80.0, 1200.0, vec![Move::feed(Point3::new(90.0, 25.0, 0.0))]),
                1,
            )
            .expect("burns");
        assert!(names(&stock.to_blob(serde_json::json!({}))).contains(&"burn".to_string()));
    }
}
