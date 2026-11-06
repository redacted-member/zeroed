//! 3D toolpaths over the relief model: the model surface in job Z, a drop
//! cutter that finds how low a tool can go at a point without cutting into
//! it, and the finishing (surface following raster or offset rings) and
//! roughing (level by level raster or offset clearing) strategies built on
//! it.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::doc::{Document, JobSetup};
use crate::error::CoreError;
use crate::geom::{path_bbox, rect, reduce_subpath, subpath_length, Point, Region, Tolerance};
use crate::model::contour;
use crate::model::field::{distance_to, Field};
use crate::model::shape::{inside_cells, Grid};

use super::chain::{Chain, CutSeg};
use super::common::{order_nearest, pass_depths, Emitter, Heights, RampSpec};
use super::profile::{Calculated, CutDirection};
use super::sim::Footprint;
use super::tool::Tool;
use super::toolpath::Point3;

/// The model's top surface in job Z on the model grid.
#[derive(Debug, Clone)]
pub struct Surface {
    grid: Grid,
    /// Job Z of each cell.
    z: Vec<f32>,
    /// Job Z of the model's zero plane, where there is no model.
    zero: f64,
    /// Cells covered by a component, for the model boundary.
    covered: Vec<bool>,
    /// Cells covered by the visible components of each model level, by
    /// level id, for a level boundary.
    levels: Vec<(u64, Vec<bool>)>,
}

impl Surface {
    /// The document's baked model, placed in the material at the job's
    /// modelling plane ([`Document::model_zero`]).
    pub fn of(doc: &Document) -> Surface {
        let composite = doc.composite();
        let zero = doc.model_zero(&composite);
        let mut surface = Surface::from_composite_at(&composite, doc.job(), zero);
        let first = doc.levels().first().map_or(1, |l| l.id);
        let grid = surface.grid;
        for level in doc.levels() {
            let mut covered = vec![false; grid.width * grid.height];
            let on_level = doc
                .components()
                .into_iter()
                .filter(|c| c.visible && (if c.level == 0 { first } else { c.level }) == level.id);
            for component in on_level {
                cover(&mut covered, &component.field, &grid);
            }
            surface.levels.push((level.id, covered));
        }
        surface
    }

    /// The surface with the cells a model level covers, for a level
    /// boundary.
    pub fn with_level(mut self, id: u64, covered: Vec<bool>) -> Surface {
        self.levels.retain(|(l, _)| *l != id);
        self.levels.push((id, covered));
        self
    }

    /// The cells the level's components cover, or an error when it has
    /// none.
    fn level_cells(&self, id: u64) -> Result<&[bool], CoreError> {
        self.levels
            .iter()
            .find(|(l, _)| *l == id)
            .map(|(_, cells)| cells.as_slice())
            .filter(|cells| cells.iter().any(|c| *c))
            .ok_or_else(|| {
                CoreError::validation("the chosen level has no visible components to machine")
            })
    }

    /// A surface from a baked composite on a single sided job: Model
    /// Position in Material puts the model top the gap below the material
    /// top, never pushing the zero plane under the bottom.
    pub fn from_composite(composite: &Field, job: &JobSetup) -> Surface {
        let peak = f64::from(composite.max().unwrap_or(0.0)).max(0.0);
        let zero = job.model_zero(crate::doc::Side::Top, peak, 0.0);
        Surface::from_composite_at(composite, job, zero)
    }

    /// A surface from a baked composite whose zero plane lies at job Z
    /// `zero`, in millimetres.
    pub fn from_composite_at(composite: &Field, job: &JobSetup, zero: f64) -> Surface {
        let top = job.surface_z();
        let z = composite
            .z
            .iter()
            .map(|h| {
                let h = if h.is_nan() { 0.0 } else { f64::from(*h) };
                (zero + h).min(top) as f32
            })
            .collect();
        let covered = composite
            .z
            .iter()
            .map(|h| !h.is_nan() && *h != 0.0)
            .collect();
        Surface {
            grid: Grid {
                width: composite.width,
                height: composite.height,
                cell: composite.cell,
                origin: composite.origin,
            },
            z,
            zero,
            covered,
            levels: Vec::new(),
        }
    }

    /// Job Z of the model's zero plane, in millimetres.
    pub fn zero(&self) -> f64 {
        self.zero
    }

    fn height(&self, i: i64, j: i64) -> f32 {
        if i < 0 || j < 0 || i >= self.grid.width as i64 || j >= self.grid.height as i64 {
            return self.zero as f32;
        }
        self.z[j as usize * self.grid.width + i as usize]
    }

    /// How low the tool's tip can go at `p` without cutting into the model.
    pub fn drop_cutter(&self, footprint: &Footprint, p: Point) -> f64 {
        let ci = ((p.x - self.grid.origin.x) / self.grid.cell).round() as i64;
        let cj = ((p.y - self.grid.origin.y) / self.grid.cell).round() as i64;
        let mut tip = f32::MIN;
        for &(dx, dy, rise) in footprint.cells() {
            let bottom = self.height(ci + i64::from(dx), cj + i64::from(dy)) - rise;
            if bottom > tip {
                tip = bottom;
            }
        }
        f64::from(tip)
    }

    /// The same surface `by` millimetres lower, as when a model is placed
    /// below the material top.
    pub fn lowered(mut self, by: f64) -> Surface {
        if by != 0.0 {
            for z in &mut self.z {
                *z -= by as f32;
            }
            self.zero -= by;
        }
        self
    }

    /// The cells inside the components' outline: those the model covers
    /// with every hole closed (a hole is uncovered cells no path of
    /// uncovered cells joins to the grid's edge), or None when no component
    /// raises anything.
    fn model_outline(&self) -> Option<Vec<bool>> {
        if !self.covered.iter().any(|c| *c) {
            return None;
        }
        let (w, h) = (self.grid.width, self.grid.height);
        // Flood the uncovered cells from the edge; what the flood misses is
        // inside the outline.
        let mut outside = vec![false; w * h];
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..w {
            stack.push(i);
            stack.push((h - 1) * w + i);
        }
        for j in 0..h {
            stack.push(j * w);
            stack.push(j * w + w - 1);
        }
        while let Some(at) = stack.pop() {
            if outside[at] || self.covered[at] {
                continue;
            }
            outside[at] = true;
            let (i, j) = (at % w, at / w);
            if i > 0 {
                stack.push(at - 1);
            }
            if i + 1 < w {
                stack.push(at + 1);
            }
            if j > 0 {
                stack.push(at - w);
            }
            if j + 1 < h {
                stack.push(at + w);
            }
        }
        Some(outside.iter().map(|o| !o).collect())
    }

    /// The box of the cells the model covers, or the whole grid when no
    /// component raises anything.
    pub fn model_box(&self) -> (Point, Point) {
        let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0, 0);
        for j in 0..self.grid.height {
            for i in 0..self.grid.width {
                if self.covered[j * self.grid.width + i] {
                    i0 = i0.min(i);
                    j0 = j0.min(j);
                    i1 = i1.max(i);
                    j1 = j1.max(j);
                }
            }
        }
        let g = &self.grid;
        if i0 == usize::MAX {
            return (
                g.origin,
                Point::new(
                    g.origin.x + (g.width - 1) as f64 * g.cell,
                    g.origin.y + (g.height - 1) as f64 * g.cell,
                ),
            );
        }
        (
            Point::new(
                g.origin.x + i0 as f64 * g.cell,
                g.origin.y + j0 as f64 * g.cell,
            ),
            Point::new(
                g.origin.x + i1 as f64 * g.cell,
                g.origin.y + j1 as f64 * g.cell,
            ),
        )
    }

    /// The grid the surface is on.
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Job Z of the surface at `p`, interpolated between the cell centres
    /// around it. Outside the grid it is the zero plane.
    pub fn z_at(&self, p: Point) -> f64 {
        let fx = (p.x - self.grid.origin.x) / self.grid.cell;
        let fy = (p.y - self.grid.origin.y) / self.grid.cell;
        if !fx.is_finite() || !fy.is_finite() {
            return self.zero;
        }
        let (i, j) = (fx.floor() as i64, fy.floor() as i64);
        let (tx, ty) = (fx - i as f64, fy - j as f64);
        let h = |di: i64, dj: i64| f64::from(self.height(i + di, j + dj));
        let bottom = h(0, 0) * (1.0 - tx) + h(1, 0) * tx;
        let top = h(0, 1) * (1.0 - tx) + h(1, 1) * tx;
        bottom * (1.0 - ty) + top * ty
    }
}

/// How rest machining decides what a later tool of a multi tool 3D
/// Finishing toolpath cuts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RestParams {
    /// Material the earlier tools left that is thinner than this, in
    /// millimetres, is not worth another pass.
    pub minimum_detail: f64,
    /// How far each area left for the tool is grown, in millimetres: more
    /// joins small areas into fewer, larger ones and blends the edges.
    /// Named apart from the toolpath's own boundary offset, as both travel
    /// in one set of settings.
    #[serde(rename = "restBoundaryOffset")]
    pub boundary_offset: f64,
}

impl Default for RestParams {
    fn default() -> Self {
        RestParams {
            minimum_detail: 0.1,
            boundary_offset: 1.0,
        }
    }
}

impl Surface {
    /// The surface `tool` leaves when it finishes the whole grid: at each
    /// cell, the lowest the tool's bottom reaches there from any position
    /// resting on the model. Cells the tool is too big to get into stay
    /// higher than the model.
    ///
    /// A large tool is worked out on a coarser grid, each coarse cell as
    /// high as the highest cell under it, so the answer errs on the side of
    /// leaving more for the tools after it.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a tool with no cutting width.
    pub fn machined_by(&self, tool: &Tool) -> Result<Vec<f32>, CoreError> {
        let cell = self.grid.cell;
        let factor = ((tool.radius() / (8.0 * cell)).floor() as usize).max(1);
        if factor == 1 {
            return Ok(self.machined_on_grid(&Footprint::of(tool, cell)?));
        }
        let (w, h) = (self.grid.width, self.grid.height);
        let (cw, ch) = (w.div_ceil(factor), h.div_ceil(factor));
        let mut z = vec![f32::MIN; cw * ch];
        for j in 0..h {
            for i in 0..w {
                let k = (j / factor) * cw + i / factor;
                z[k] = z[k].max(self.z[j * w + i]);
            }
        }
        let shift = (factor - 1) as f64 * cell / 2.0;
        let coarse = Surface {
            grid: Grid {
                width: cw,
                height: ch,
                cell: cell * factor as f64,
                origin: Point::new(self.grid.origin.x + shift, self.grid.origin.y + shift),
            },
            z,
            zero: self.zero,
            covered: Vec::new(),
            levels: Vec::new(),
        };
        let reached = coarse.machined_on_grid(&Footprint::of(tool, coarse.grid.cell)?);
        let mut out = vec![0f32; w * h];
        for j in 0..h {
            for i in 0..w {
                let k = (j / factor) * cw + i / factor;
                out[j * w + i] = reached[k].max(self.z[j * w + i]);
            }
        }
        Ok(out)
    }

    /// [`Surface::machined_by`] at the surface's own resolution.
    fn machined_on_grid(&self, footprint: &Footprint) -> Vec<f32> {
        let (w, h) = (self.grid.width, self.grid.height);
        let cell = self.grid.cell;
        let mut tips = vec![0f32; w * h];
        for j in 0..h {
            for i in 0..w {
                let p = Point::new(
                    self.grid.origin.x + i as f64 * cell,
                    self.grid.origin.y + j as f64 * cell,
                );
                tips[j * w + i] = self.drop_cutter(footprint, p) as f32;
            }
        }
        let mut out = vec![f32::MAX; w * h];
        for j in 0..h as i64 {
            for i in 0..w as i64 {
                let mut low = f32::MAX;
                for &(dx, dy, rise) in footprint.cells() {
                    let (pi, pj) = (i - i64::from(dx), j - i64::from(dy));
                    if pi < 0 || pj < 0 || pi >= w as i64 || pj >= h as i64 {
                        continue;
                    }
                    let reach = tips[pj as usize * w + pi as usize] + rise;
                    if reach < low {
                        low = reach;
                    }
                }
                out[j as usize * w + i as usize] = low.max(self.height(i, j));
            }
        }
        out
    }

    /// The cells `tool` should finish after the `earlier` tools: where it
    /// gets deeper than all of them by more than the minimum detail, grown
    /// by the rest boundary offset.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a tool with no cutting width or a
    /// negative minimum detail.
    pub fn rest_cells(
        &self,
        earlier: &[&Tool],
        tool: &Tool,
        rest: &RestParams,
    ) -> Result<Vec<bool>, CoreError> {
        if !rest.minimum_detail.is_finite() || rest.minimum_detail < 0.0 {
            return Err(CoreError::validation(
                "the minimum detail must be zero or more",
            ));
        }
        if !rest.boundary_offset.is_finite() || rest.boundary_offset < 0.0 {
            return Err(CoreError::validation(
                "the rest machining boundary offset must be zero or more",
            ));
        }
        let mut before = vec![f32::MAX; self.z.len()];
        for tool in earlier {
            let reached = self.machined_by(tool)?;
            for (b, r) in before.iter_mut().zip(reached) {
                *b = b.min(r);
            }
        }
        let now = self.machined_by(tool)?;
        let detail = rest.minimum_detail.max(1e-4) as f32;
        let mut cells: Vec<bool> = before
            .iter()
            .zip(&now)
            .map(|(b, n)| b - n > detail)
            .collect();
        if rest.boundary_offset > 0.0 && cells.iter().any(|c| *c) {
            grow(&mut cells, &self.grid, rest.boundary_offset);
        }
        Ok(cells)
    }
}

/// Marks the grid cells a component's field raises.
fn cover(covered: &mut [bool], f: &Field, grid: &Grid) {
    let i0 = ((f.origin.x - grid.origin.x) / grid.cell).floor().max(0.0) as usize;
    let j0 = ((f.origin.y - grid.origin.y) / grid.cell).floor().max(0.0) as usize;
    let i1 = (((f.origin.x + f.width as f64 * f.cell) - grid.origin.x) / grid.cell)
        .ceil()
        .clamp(0.0, grid.width as f64) as usize;
    let j1 = (((f.origin.y + f.height as f64 * f.cell) - grid.origin.y) / grid.cell)
        .ceil()
        .clamp(0.0, grid.height as f64) as usize;
    for j in j0..j1 {
        for i in i0..i1 {
            let p = Point::new(
                grid.origin.x + i as f64 * grid.cell,
                grid.origin.y + j as f64 * grid.cell,
            );
            let v = f.at(p);
            if !v.is_nan() && v != 0.0 {
                covered[j * grid.width + i] = true;
            }
        }
    }
}

/// Where a 3D toolpath may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Boundary {
    /// Inside the outline of the model's components.
    #[default]
    Model,
    /// The whole material.
    Material,
    /// Inside the selected closed vectors.
    Selection,
    /// The outline of the components on one model level.
    Level,
}

/// How 3D Finishing covers the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishStrategy {
    /// Back and forth lines at the raster angle.
    #[default]
    Raster,
    /// Lines at the raster angle, then again across them at the angle plus
    /// 90 degrees.
    RasterXY,
    /// Rings a stepover apart, offset inward from the boundary outline.
    Offset,
}

/// The 3D Finishing form's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FinishParams {
    /// Distance between raster lines or offset rings, in millimetres; the
    /// tool's own when absent.
    pub stepover: Option<f64>,
    /// Raster angle in degrees from the X axis; offset rings ignore it.
    pub angle: f64,
    /// Material left on the surface, in millimetres.
    pub allowance: f64,
    /// Where the toolpath may go.
    pub boundary: Boundary,
    /// The model level whose components limit a level boundary.
    pub level: u64,
    /// How far the boundary is grown outward (shrunk when negative), in
    /// millimetres, so the tool can run past a raised edge.
    pub boundary_offset: f64,
    /// Raster, raster in X and Y, or offset rings.
    pub strategy: FinishStrategy,
    /// Which way offset rings are cut.
    pub direction: CutDirection,
    /// With offset rings, how far the tool lifts between one ring and the
    /// next, in millimetres; zero steps over along the surface.
    pub stepover_retract: f64,
}

impl Default for FinishParams {
    fn default() -> Self {
        FinishParams {
            stepover: None,
            angle: 0.0,
            allowance: 0.0,
            boundary: Boundary::Model,
            level: 0,
            boundary_offset: 0.0,
            strategy: FinishStrategy::Raster,
            direction: CutDirection::Climb,
            stepover_retract: 0.0,
        }
    }
}

/// How 3D Roughing clears the material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoughStrategy {
    /// Z Level: flat levels, each cleared with back and forth lines.
    #[default]
    ZLevelRaster,
    /// Z Level with each level cleared by rings a stepover apart, offset
    /// inward from the outline of the area it clears, cut from the middle
    /// outwards.
    ZLevelOffset,
    /// 3D Raster: at every level, raster lines that follow the model plus
    /// the allowance wherever it is above the level, leaving an even skin
    /// for the finishing pass.
    Raster3d,
}

/// When a Z level toolpath cuts round the outline of each level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoughProfile {
    /// No outline pass.
    #[default]
    None,
    /// The outline first, then the clearing.
    First,
    /// The clearing, then the outline.
    Last,
}

/// The order a Z level toolpath takes the levels in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoughOrder {
    /// A whole level over the model, then the next level down.
    #[default]
    LevelByLevel,
    /// Where a level splits into separate areas, each area down to the
    /// bottom before the next.
    DepthFirst,
}

/// The 3D Roughing form's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoughParams {
    /// Distance between raster lines or offset rings, in millimetres; the
    /// tool's own when absent.
    pub stepover: Option<f64>,
    /// Depth of each level in millimetres; the tool's own when absent.
    pub pass_depth: Option<f64>,
    /// Raster angle in degrees from the X axis (0 along X, 90 along Y);
    /// offset clearing ignores it.
    pub angle: f64,
    /// Material left over the surface for the finishing pass, in
    /// millimetres.
    pub allowance: f64,
    /// Where the toolpath may go.
    pub boundary: Boundary,
    /// The model level whose components limit a level boundary.
    pub level: u64,
    /// How far the boundary is grown outward (shrunk when negative), in
    /// millimetres.
    pub boundary_offset: f64,
    /// Z level raster, Z level offset, or 3D raster.
    pub strategy: RoughStrategy,
    /// Z level raster: when each level is cut round its outline. Absent in
    /// older files, which used `profile_each_level`.
    pub profile: Option<RoughProfile>,
    /// Older files: finish each raster level round its outline.
    pub profile_each_level: bool,
    /// Level by level or depth first.
    pub order: RoughOrder,
    /// Step from the other side: raster lines start at the far edge.
    pub reverse_step: bool,
    /// 3D raster: leave out what an earlier level already cut to the
    /// allowance.
    pub avoid_machined: bool,
    /// Ramp down along the cut instead of plunging.
    pub ramp: RampSpec,
}

impl Default for RoughParams {
    fn default() -> Self {
        RoughParams {
            stepover: None,
            pass_depth: None,
            angle: 0.0,
            allowance: 0.5,
            boundary: Boundary::Model,
            level: 0,
            boundary_offset: 0.0,
            strategy: RoughStrategy::ZLevelRaster,
            profile: None,
            profile_each_level: false,
            order: RoughOrder::LevelByLevel,
            reverse_step: false,
            avoid_machined: false,
            ramp: RampSpec::default(),
        }
    }
}

impl RoughParams {
    /// When the outline of each level is cut. Offset clearing always ends
    /// on the outline, and 3D raster has none.
    pub fn profile_pass(&self) -> RoughProfile {
        match self.strategy {
            RoughStrategy::ZLevelOffset | RoughStrategy::Raster3d => RoughProfile::None,
            RoughStrategy::ZLevelRaster => self.profile.unwrap_or(if self.profile_each_level {
                RoughProfile::Last
            } else {
                RoughProfile::None
            }),
        }
    }
}

/// The part of the grid a 3D toolpath's tool centre may visit: a box, and
/// with a selection boundary the cells inside the selected vectors.
struct Limits {
    grid: Grid,
    lo: Point,
    hi: Point,
    mask: Option<Vec<bool>>,
}

/// Which boundary a 3D toolpath keeps to, and how far it is grown.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    boundary: Boundary,
    level: u64,
    offset: f64,
}

impl Limits {
    fn new(
        surface: &Surface,
        bounds: Bounds,
        selection: Option<&Region>,
        job: &JobSetup,
        tol: Tolerance,
    ) -> Result<Limits, CoreError> {
        let grid = *surface.grid();
        let mut mask = match bounds.boundary {
            Boundary::Selection => Some(inside_cells(selected(selection)?, &grid, tol)),
            Boundary::Level => Some(surface.level_cells(bounds.level)?.to_vec()),
            // The components' outline, not their box.
            Boundary::Model => surface.model_outline(),
            _ => None,
        };
        let (mut lo, mut hi) = match bounds.boundary {
            Boundary::Model => surface.model_box(),
            _ => {
                let (dx, dy) = job.datum();
                (
                    Point::new(-dx, -dy),
                    Point::new(job.width - dx, job.height - dy),
                )
            }
        };
        let offset = bounds.offset;
        if !offset.is_finite() {
            return Err(CoreError::validation(
                "the boundary offset must be a number",
            ));
        }
        if offset != 0.0 {
            match mask.as_mut() {
                Some(cells) => {
                    grow(cells, &grid, offset);
                    if !cells.iter().any(|c| *c) {
                        return Err(CoreError::validation(
                            "the boundary offset leaves nothing to machine",
                        ));
                    }
                    let out = offset.max(0.0);
                    lo = Point::new(lo.x - out, lo.y - out);
                    hi = Point::new(hi.x + out, hi.y + out);
                }
                None => {
                    lo = Point::new(lo.x - offset, lo.y - offset);
                    hi = Point::new(hi.x + offset, hi.y + offset);
                    if hi.x < lo.x || hi.y < lo.y {
                        return Err(CoreError::validation(
                            "the boundary offset leaves nothing to machine",
                        ));
                    }
                }
            }
        }
        Ok(Limits { grid, lo, hi, mask })
    }

    /// True when the tool centre may be at `p`.
    fn contains(&self, p: Point) -> bool {
        let (lo, hi) = (self.lo, self.hi);
        if p.x < lo.x - 1e-9 || p.y < lo.y - 1e-9 || p.x > hi.x + 1e-9 || p.y > hi.y + 1e-9 {
            return false;
        }
        match &self.mask {
            None => true,
            Some(cells) => {
                let g = &self.grid;
                let i = ((p.x - g.origin.x) / g.cell).round() as i64;
                let j = ((p.y - g.origin.y) / g.cell).round() as i64;
                i >= 0
                    && j >= 0
                    && (i as usize) < g.width
                    && (j as usize) < g.height
                    && cells[j as usize * g.width + i as usize]
            }
        }
    }
}

/// The selected region, or an error when nothing is selected.
fn selected(selection: Option<&Region>) -> Result<&Region, CoreError> {
    selection
        .filter(|r| !r.is_empty())
        .ok_or_else(|| CoreError::validation("select the closed vectors to machine inside"))
}

/// Grows the marked cells outward by `by` millimetres, or shrinks them
/// when it is negative.
fn grow(cells: &mut [bool], grid: &Grid, by: f64) {
    let reach = by.abs() / grid.cell + 1e-9;
    if by > 0.0 {
        let d = distance_to(cells, grid.width, grid.height);
        for (c, d) in cells.iter_mut().zip(d) {
            *c = d <= reach;
        }
    } else {
        let outside: Vec<bool> = cells.iter().map(|c| !c).collect();
        let d = distance_to(&outside, grid.width, grid.height);
        for (c, d) in cells.iter_mut().zip(d) {
            *c = *c && d > reach;
        }
    }
}

/// The outline of the marked cells, traced half way between marked and
/// unmarked cell centres.
fn mask_region(grid: &Grid, cells: &[bool], tol: Tolerance) -> Result<Region, CoreError> {
    // Signed distance to the edge, so the traced outline runs smoothly
    // across the cells instead of in steps, which the offsetter chokes on.
    let outside: Vec<bool> = cells.iter().map(|c| !c).collect();
    let to_inside = distance_to(cells, grid.width, grid.height);
    let to_outside = distance_to(&outside, grid.width, grid.height);
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for (k, value) in field.z.iter_mut().enumerate() {
        *value = if cells[k] {
            (to_outside[k] - 0.5) as f32
        } else {
            (0.5 - to_inside[k]) as f32
        };
    }
    let paths = contour::boundary(&field, 0.0, 0.0, tol)?;
    // Fewer, longer spans: tiny ones trip the offsetter.
    let merge = Tolerance::new(grid.cell * 0.1).unwrap_or(tol);
    Ok(Region {
        loops: paths
            .into_iter()
            .flat_map(|p| p.subpaths)
            .map(|s| reduce_subpath(&s, merge))
            .collect(),
    })
}

/// The area inside the boundary as a region, for offset finishing.
fn boundary_region(
    limits: &Limits,
    bounds: Bounds,
    selection: Option<&Region>,
    tol: Tolerance,
) -> Result<Region, CoreError> {
    match bounds.boundary {
        Boundary::Selection => {
            let region = selected(selection)?;
            return Ok(if bounds.offset == 0.0 {
                region.clone()
            } else {
                safe_offset(region, -bounds.offset, tol)
            });
        }
        // A model or level boundary is the components' outline, not their
        // box: offset rings from the box run through the uncut material
        // around a model that does not fill it.
        Boundary::Level | Boundary::Model => {
            if let Some(cells) = &limits.mask {
                return mask_region(&limits.grid, cells, tol);
            }
        }
        _ => {}
    }
    let outline = rect(limits.lo, limits.hi)
        .map_err(|_| CoreError::validation("the boundary has no area to machine"))?;
    Ok(Region::from_paths(&[outline], tol))
}

/// Raster lines over the area: for each line, the sample points along it
/// that lie inside the boundary, split into runs where it leaves.
struct Raster {
    lines: Vec<Vec<Vec<Point>>>,
}

fn raster(limits: &Limits, stepover: f64, angle_deg: f64, reverse: bool) -> Raster {
    let (lo, hi) = (limits.lo, limits.hi);
    let a = angle_deg.to_radians();
    let (u, n) = (Point::new(a.cos(), a.sin()), Point::new(-a.sin(), a.cos()));
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    let along = |p: Point| p.x * u.x + p.y * u.y;
    let across = |p: Point| p.x * n.x + p.y * n.y;
    let (s0, s1) = corners.iter().fold((f64::MAX, f64::MIN), |(a0, a1), p| {
        (a0.min(along(*p)), a1.max(along(*p)))
    });
    let (t0, t1) = corners.iter().fold((f64::MAX, f64::MIN), |(a0, a1), p| {
        (a0.min(across(*p)), a1.max(across(*p)))
    });
    let step = limits.grid.cell;
    let mut lines = Vec::new();
    // Stepping from the far side starts at the other edge.
    let mut t = if reverse { t1 } else { t0 };
    let mut forward = true;
    while (!reverse && t <= t1 + 1e-9) || (reverse && t >= t0 - 1e-9) {
        let mut runs: Vec<Vec<Point>> = Vec::new();
        let mut run: Vec<Point> = Vec::new();
        let count = ((s1 - s0) / step).ceil() as usize;
        for k in 0..=count {
            let s = if forward {
                s0 + k as f64 * step
            } else {
                s1 - k as f64 * step
            };
            let p = Point::new(u.x * s + n.x * t, u.y * s + n.y * t);
            if limits.contains(p) {
                run.push(p);
            } else if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        lines.push(runs);
        t += if reverse { -stepover } else { stepover };
        forward = !forward;
    }
    Raster { lines }
}

/// How low the tool may go at every cell centre inside the limits: the
/// drop cutter tip plus the allowance in job Z millimetres, NaN outside.
struct Floors {
    grid: Grid,
    z: Vec<f64>,
}

impl Floors {
    fn new(surface: &Surface, footprint: &Footprint, limits: &Limits, allowance: f64) -> Floors {
        let grid = *surface.grid();
        let (w, h) = (grid.width, grid.height);
        let cells = footprint.cells();
        let reach = cells
            .iter()
            .map(|&(dx, dy, _)| dx.abs().max(dy.abs()))
            .max()
            .unwrap_or(0) as usize;
        // Offsets into the height array for cells whose footprint stays on
        // the grid, so the inner loop needs no bounds checks.
        let offsets: Vec<(isize, f32)> = cells
            .iter()
            .map(|&(dx, dy, rise)| (dy as isize * w as isize + dx as isize, rise))
            .collect();
        let mut z = vec![f64::NAN; w * h];
        // Rows are independent: each is filled on its own thread.
        z.par_chunks_mut(w.max(1)).enumerate().for_each(|(j, row)| {
            let y = grid.origin.y + j as f64 * grid.cell;
            for (i, out) in row.iter_mut().enumerate() {
                let p = Point::new(grid.origin.x + i as f64 * grid.cell, y);
                if !limits.contains(p) {
                    continue;
                }
                let tip = if i >= reach && j >= reach && i + reach < w && j + reach < h {
                    let base = (j * w + i) as isize;
                    let mut tip = f32::MIN;
                    for &(off, rise) in &offsets {
                        let bottom = surface.z[(base + off) as usize] - rise;
                        if bottom > tip {
                            tip = bottom;
                        }
                    }
                    f64::from(tip)
                } else {
                    surface.drop_cutter(footprint, p)
                };
                *out = tip + allowance;
            }
        });
        Floors { grid, z }
    }

    /// The lowest floor inside the limits, `f64::MAX` when none is.
    fn lowest(&self) -> f64 {
        self.z
            .iter()
            .filter(|z| z.is_finite())
            .fold(f64::MAX, |a, b| a.min(*b))
    }

    /// True when the tool may be at height `z` over cell `(i, j)`.
    fn ok(&self, i: i64, j: i64, z: f64) -> bool {
        let g = &self.grid;
        if i < 0 || j < 0 || i >= g.width as i64 || j >= g.height as i64 {
            return false;
        }
        let floor = self.z[j as usize * g.width + i as usize];
        floor.is_finite() && floor <= z + 1e-9
    }

    /// True when the tool can go straight from `a` to `b` at height `z`:
    /// every cell nearest to any point of the move is clear.
    fn link_ok(&self, a: Point, b: Point, z: f64) -> bool {
        let g = &self.grid;
        let steps = (a.distance(b) / (g.cell * 0.5)).ceil().max(1.0) as usize;
        (0..=steps).all(|k| {
            let p = a.lerp(b, k as f64 / steps as f64);
            let fx = (p.x - g.origin.x) / g.cell;
            let fy = (p.y - g.origin.y) / g.cell;
            let (i0, i1) = ((fx - 0.25).round() as i64, (fx + 0.25).round() as i64);
            let (j0, j1) = ((fy - 0.25).round() as i64, (fy + 0.25).round() as i64);
            self.ok(i0, j0, z) && self.ok(i1, j0, z) && self.ok(i0, j1, z) && self.ok(i1, j1, z)
        })
    }

    /// The outline of the area the tool may clear at height `z` over the
    /// cells `keep` accepts, traced round the clear cells and pulled in by
    /// one cell so that every point on or inside it is nearest to a clear
    /// cell.
    fn outline_where<K: Fn(usize) -> bool>(
        &self,
        z: f64,
        keep: K,
        tol: Tolerance,
    ) -> Result<Region, CoreError> {
        let g = &self.grid;
        let mut field = Field::empty(g.width, g.height, g.cell, g.origin);
        for (index, (value, floor)) in field.z.iter_mut().zip(&self.z).enumerate() {
            *value = if !floor.is_finite() {
                f32::NAN
            } else if *floor <= z + 1e-9 && keep(index) {
                ((z - floor) as f32).max(1e-6)
            } else {
                ((z - floor) as f32).min(-1e-6)
            };
        }
        let paths = contour::boundary(&field, 0.0, 0.0, tol)?;
        let traced = Region {
            loops: paths.into_iter().flat_map(|p| p.subpaths).collect(),
        };
        Ok(safe_offset(&traced, g.cell, tol))
    }
}

/// Each outer loop of `region` with the holes inside it, so that a level's
/// separate areas are cleared one after the other.
pub(crate) fn components(region: &Region, tol: Tolerance) -> Vec<Region> {
    let outers: Vec<Region> = region
        .outers()
        .map(|s| Region {
            loops: vec![s.clone()],
        })
        .collect();
    let areas: Vec<f64> = outers.iter().map(Region::area).collect();
    let mut parts = outers.clone();
    for hole in region.holes() {
        let owner = outers
            .iter()
            .enumerate()
            .filter(|(_, o)| o.contains(hole.start, tol))
            .min_by(|a, b| areas[a.0].total_cmp(&areas[b.0]))
            .map(|(k, _)| k);
        if let Some(k) = owner {
            parts[k].loops.push(hole.clone());
        }
    }
    parts
}

/// `region.offset(delta)`, retried a hair further in or out when the
/// offsetter fails on the exact distance (it asserts on some traced
/// outlines); empty if every try fails.
fn safe_offset(region: &Region, delta: f64, tol: Tolerance) -> Region {
    try_offset(region, delta, tol).unwrap_or(Region { loops: Vec::new() })
}

/// [`safe_offset`], or `None` when the offsetter failed at every nudge
/// (an empty region means the offset really left nothing).
fn try_offset(region: &Region, delta: f64, tol: Tolerance) -> Option<Region> {
    for nudge in [0.0, 1e-3, -1e-3, 5e-3] {
        let tried = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            region.offset(delta + nudge, tol)
        }));
        if let Ok(done) = tried {
            return Some(done);
        }
    }
    None
}

/// The total length of every loop of `region`.
fn region_length(region: &Region, tol: Tolerance) -> f64 {
    region.loops.iter().map(|s| subpath_length(s, tol)).sum()
}

/// True when `ring` could be `from` offset inward by `gap`: every point taken
/// away lies within `gap` of an edge, so the area can fall by little more
/// than the edges' length times `gap`, the rounded inside corners being the
/// little. The offsetter can answer with whole pieces of a ring gone, which
/// takes away far more, or with a hole gone, which adds area.
fn ring_is_whole(from: &Region, ring: &Region, gap: f64, tol: Tolerance) -> bool {
    let removed = from.area() - ring.area();
    let corners = from.loops.len() as f64 * std::f64::consts::TAU * gap * gap;
    let most = region_length(from, tol) * gap * 1.25 + corners;
    removed >= -tol.get() && removed <= most
}

/// The cells of `region` with how far each is inside it, in cells, for tracing
/// a ring where the offsetter cannot make one. Past the grid counts as outside,
/// so an area that fills the grid is measured from the grid's edge.
fn depth_cells(region: &Region, grid: &Grid, tol: Tolerance) -> Vec<f32> {
    let cells = inside_cells(region, grid, tol);
    let (w, h) = (grid.width + 2, grid.height + 2);
    let mut inside = vec![false; w * h];
    for (row, from) in inside
        .chunks_mut(w)
        .skip(1)
        .zip(cells.chunks(grid.width.max(1)))
    {
        row[1..=from.len()].copy_from_slice(from);
    }
    let outside: Vec<bool> = inside.iter().map(|c| !c).collect();
    let to_inside = distance_to(&inside, w, h);
    let to_outside = distance_to(&outside, w, h);
    let mut depths = Vec::with_capacity(cells.len());
    for j in 1..=grid.height {
        for i in 1..=grid.width {
            let k = j * w + i;
            depths.push(if inside[k] {
                (to_outside[k] - 0.5) as f32
            } else {
                (0.5 - to_inside[k]) as f32
            });
        }
    }
    depths
}

/// The ring `depth` millimetres inside the area `depths` describes, traced on
/// the grid. It follows the area to within about a cell, which is why it only
/// stands in for a ring the offsetter broke.
fn traced_ring(depths: &[f32], grid: &Grid, depth: f64, tol: Tolerance) -> Region {
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    let level = depth / grid.cell;
    for (value, inside) in field.z.iter_mut().zip(depths) {
        *value = (f64::from(*inside) - level) as f32;
    }
    let paths = contour::boundary(&field, 0.0, 0.0, tol).unwrap_or_default();
    let merge = Tolerance::new(grid.cell * 0.1).unwrap_or(tol);
    Region {
        loops: paths
            .into_iter()
            .flat_map(|p| p.subpaths)
            .map(|s| reduce_subpath(&s, merge))
            .collect(),
    }
}

/// The region and its inward offsets a stepover apart, outermost first.
fn rings(region: &Region, stepover: f64, grid: &Grid, tol: Tolerance) -> Vec<Region> {
    rings_by(region, stepover, grid, tol, |depth| {
        try_offset(region, depth, tol)
    })
}

/// [`rings`] with what `offset` answers for each depth.
fn rings_by(
    region: &Region,
    stepover: f64,
    grid: &Grid,
    tol: Tolerance,
    offset: impl Fn(f64) -> Option<Region>,
) -> Vec<Region> {
    let mut rings = vec![region.clone()];
    let Some(bbox) = path_bbox(&region.as_path()) else {
        return rings;
    };
    // No ring can be further in than half the area's narrower side.
    let side = (bbox.max.x - bbox.min.x).min(bbox.max.y - bbox.min.y);
    let last = (side / 2.0 / stepover).ceil() as usize + 1;
    let mut depths: Option<Vec<f32>> = None;
    for k in 1..=last {
        // Each ring is offset from the region itself: offsetting the ring
        // before piles up tiny arcs that the offsetter cannot handle. Where it
        // still fails or answers with pieces missing, the ring is traced
        // instead: leaving rings out left ridges, and stopping at too many
        // left the whole middle of the area uncut.
        let depth = stepover * k as f64;
        let before = rings.last().unwrap_or(region);
        let ring = offset(depth)
            .filter(|ring| ring_is_whole(before, ring, stepover, tol))
            .unwrap_or_else(|| {
                let depths = depths.get_or_insert_with(|| depth_cells(region, grid, tol));
                traced_ring(depths, grid, depth, tol)
            });
        if ring.is_empty() {
            break;
        }
        rings.push(ring);
    }
    rings
}

/// The loops of `region` as chains ordered nearest first from `from`, each
/// restarted at its node nearest to where the one before ends.
fn nearest_loops(region: &Region, from: Point, tol: Tolerance) -> Vec<Chain> {
    let mut left: Vec<Chain> = region
        .loops
        .iter()
        .map(|s| Chain::from_subpath(s, tol))
        .filter(|c| !c.is_empty())
        .collect();
    let mut out = Vec::with_capacity(left.len());
    let mut at = from;
    while !left.is_empty() {
        let best = left
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let d = c
                    .segs
                    .iter()
                    .map(|s| s.from.distance_squared(at))
                    .fold(f64::MAX, f64::min);
                (k, d)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map_or(0, |(k, _)| k);
        let chain = left.remove(best).starting_near(at);
        if let Some(end) = chain.end() {
            at = end;
        }
        out.push(chain);
    }
    out
}

/// How a roughing cut gets down to its level where it cannot stay down.
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Ramp distance in millimetres, or none to plunge.
    ramp: Option<f64>,
    /// Height the ramp starts from: just above the level before.
    top: f64,
}

/// Cuts a closed chain flat at `z`, staying down from the last cut when the
/// link is short and clear of the model, otherwise lifting to it and
/// plunging or ramping down.
fn cut_loop(
    out: &mut Emitter,
    floors: &Floors,
    chain: &Chain,
    z: f64,
    link_limit: f64,
    entry: Entry,
) {
    let Some(begin) = chain.start() else {
        return;
    };
    let here = out.at();
    let stay_down = (here.z - z).abs() < 1e-9
        && here.xy().distance(begin) <= link_limit
        && floors.link_ok(here.xy(), begin, z);
    if stay_down {
        out.cut(
            &CutSeg {
                from: here.xy(),
                to: begin,
                bulge: 0.0,
            },
            z,
        );
    } else if let Some(distance) = entry.ramp {
        out.travel_to(begin);
        out.rapid_down_to(entry.top.max(z));
        out.cut_chain_ramped(chain, z, distance);
        return;
    } else {
        out.travel_to(begin);
        out.plunge_to(z);
    }
    out.cut_flat(&chain.segs, z);
}

/// Starts a flat run at `z` by ramping down along its first `distance`
/// millimetres from `top` and coming back along it at depth.
fn ramp_in(out: &mut Emitter, path: &[Point3], z: f64, distance: f64, top: f64) {
    let Some(first) = path.first() else {
        return;
    };
    out.travel_to(first.xy());
    out.rapid_down_to(top.max(z));
    let mut segs: Vec<CutSeg> = Vec::new();
    let mut left = distance;
    for pair in path.windows(2) {
        if left <= 1e-9 {
            break;
        }
        let seg = CutSeg {
            from: pair[0].xy(),
            to: pair[1].xy(),
            bulge: 0.0,
        };
        let length = seg.length();
        if length <= left {
            segs.push(seg);
            left -= length;
        } else {
            segs.push(seg.split(left).0);
            left = 0.0;
        }
    }
    if segs.is_empty() {
        out.plunge_to(z);
        return;
    }
    out.cut_sloped(&segs, z);
    let back: Vec<CutSeg> = segs.iter().rev().map(CutSeg::reversed).collect();
    out.cut_flat(&back, z);
}

/// Cuts a flat span at `z`, staying down when `near`, else lifting and
/// plunging or ramping into its start.
fn cut_span(out: &mut Emitter, span: &[Point3], z: f64, near: bool, entry: Entry) {
    match entry.ramp {
        Some(distance) if !near && span.len() > 1 => {
            ramp_in(out, span, z, distance, entry.top);
            cut_run(out, span, true);
        }
        _ => cut_run(out, span, near),
    }
}

/// Drops points within `tol` of the line between their neighbours (3D).
fn thin(points: &[Point3], tol: f64) -> Vec<Point3> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    let mut anchor = points[0];
    for k in 1..points.len() - 1 {
        let (p, next) = (points[k], points[k + 1]);
        let (dx, dy, dz) = (next.x - anchor.x, next.y - anchor.y, next.z - anchor.z);
        let len2 = dx * dx + dy * dy + dz * dz;
        let t = if len2 > 0.0 {
            (((p.x - anchor.x) * dx + (p.y - anchor.y) * dy + (p.z - anchor.z) * dz) / len2)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = Point3::new(anchor.x + dx * t, anchor.y + dy * t, anchor.z + dz * t);
        if q.distance(p) > tol {
            out.push(p);
            anchor = p;
        }
    }
    out.push(points[points.len() - 1]);
    out
}

/// Cuts a run of 3D points, lifting and travelling to its start unless the
/// tool can move there directly at `link_z` or above.
pub(super) fn cut_run(out: &mut Emitter, run: &[Point3], stay_down: bool) {
    let Some(first) = run.first().copied() else {
        return;
    };
    if !stay_down {
        out.travel_to(first.xy());
        out.plunge_to(first.z);
    } else {
        out.cut(
            &CutSeg {
                from: out.at().xy(),
                to: first.xy(),
                bulge: 0.0,
            },
            first.z,
        );
    }
    for p in &run[1..] {
        out.cut(
            &CutSeg {
                from: out.at().xy(),
                to: p.xy(),
                bulge: 0.0,
            },
            p.z,
        );
    }
}

fn check(tool: &Tool, stepover: f64) -> Result<(), CoreError> {
    if !stepover.is_finite() || stepover <= 0.0 {
        return Err(CoreError::validation("the stepover must be more than zero"));
    }
    if tool.radius() <= 0.0 {
        return Err(CoreError::validation("the tool has no cutting diameter"));
    }
    Ok(())
}

/// What a finishing pass needs to place points on the surface.
struct Finisher<'a> {
    surface: &'a Surface,
    footprint: Footprint,
    allowance: f64,
    heights: Heights,
    stepover: f64,
    direction: CutDirection,
    /// Lift between offset rings, millimetres; zero stays on the surface.
    retract: f64,
}

impl Finisher<'_> {
    /// The tool tip over `p` resting on the surface plus the allowance,
    /// never above the material top.
    fn place(&self, p: Point) -> Point3 {
        let z = self.surface.drop_cutter(&self.footprint, p) + self.allowance;
        Point3::new(p.x, p.y, z.min(self.heights.surface))
    }

    /// Raster lines following the surface. `part` is the share of the
    /// whole calculation these lines are, for its progress.
    fn raster(
        &self,
        out: &mut Emitter,
        raster: &Raster,
        part: (f64, f64),
    ) -> Result<(), CoreError> {
        // Each point drops on its own: every line at once, in parallel,
        // thinned there too, then the moves are made line by line in order.
        // The ordered pass is the last tenth of the progress and stops when
        // asked, as the parallel one does.
        let total = raster.lines.len().max(1);
        let dropping = (part.0, part.0 + (part.1 - part.0) * 0.9);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let dropped: Vec<Option<Vec<Vec<Point3>>>> = raster
            .lines
            .par_iter()
            .map(|line| {
                if crate::progress::stopped() {
                    return None;
                }
                let runs = line
                    .iter()
                    .map(|run| {
                        let placed: Vec<Point3> = run.iter().map(|p| self.place(*p)).collect();
                        thin(&placed, 0.005)
                    })
                    .collect();
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                crate::progress::report(
                    dropping.0 + (dropping.1 - dropping.0) * n as f64 / total as f64,
                );
                Some(runs)
            })
            .collect();
        crate::progress::check()?;
        for (k, line) in dropped.into_iter().flatten().enumerate() {
            if k % 64 == 0 {
                crate::progress::check()?;
                crate::progress::report(
                    dropping.1 + (part.1 - dropping.1) * k as f64 / total as f64,
                );
            }
            for points in line {
                // Step over to the next line along the surface when it is close.
                let here = out.at();
                let near = points.first().is_some_and(|p| {
                    here.z < self.heights.surface
                        && p.xy().distance(here.xy()) <= self.stepover * 1.5
                });
                cut_run(out, &points, near);
            }
        }
        Ok(())
    }

    /// Rings offset inward from `area` a stepover apart, outermost first,
    /// every point dropped onto the surface. Short links between rings run
    /// along the surface too.
    fn offset(&self, out: &mut Emitter, area: &Region, tol: Tolerance) -> Result<(), CoreError> {
        let step = self.surface.grid().cell;
        let mut points: Vec<Point> = Vec::new();
        let rings = rings(area, self.stepover, self.surface.grid(), tol);
        let total = rings.len();
        for (k, ring) in rings.into_iter().enumerate() {
            crate::progress::check()?;
            crate::progress::report_steps(k, total);
            for chain in nearest_loops(&ring, out.at().xy(), tol) {
                let chain = match self.direction {
                    CutDirection::Climb => chain,
                    CutDirection::Conventional => chain.reversed(),
                };
                let Some(begin) = chain.start() else {
                    continue;
                };
                points.clear();
                let here = out.at();
                let link = here.z < self.heights.surface
                    && here.xy().distance(begin) <= self.stepover * 2.0;
                if link && self.retract > 0.0 {
                    // Up clear of the surface, across, and back down.
                    let lift =
                        (here.z.max(self.place(begin).z) + self.retract).min(self.heights.safe);
                    let up = CutSeg {
                        from: here.xy(),
                        to: here.xy(),
                        bulge: 0.0,
                    };
                    out.cut(&up, lift);
                    out.cut(
                        &CutSeg {
                            from: here.xy(),
                            to: begin,
                            bulge: 0.0,
                        },
                        lift,
                    );
                    points.push(begin);
                } else if link {
                    sample(
                        &CutSeg {
                            from: here.xy(),
                            to: begin,
                            bulge: 0.0,
                        },
                        step,
                        &mut points,
                    );
                } else {
                    points.push(begin);
                }
                for seg in &chain.segs {
                    sample(seg, step, &mut points);
                }
                let placed: Vec<Point3> = points.par_iter().map(|p| self.place(*p)).collect();
                cut_run(out, &thin(&placed, 0.005), link);
            }
        }
        Ok(())
    }
}

/// Points along `seg` about `step` apart, its end included and its start
/// left out.
fn sample(seg: &CutSeg, step: f64, into: &mut Vec<Point>) {
    let length = seg.length();
    let count = (length / step).ceil().max(1.0) as usize;
    for k in 1..=count {
        into.push(seg.point_at(length * k as f64 / count as f64));
    }
}

/// 3D Finishing: raster lines, raster lines both ways, or offset rings
/// that follow the model surface.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a bad stepover or tool, or a
/// selection boundary with nothing selected.
pub fn finish(
    surface: &Surface,
    job: &JobSetup,
    tool: &Tool,
    params: &FinishParams,
    selection: Option<&Region>,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    finish_within(surface, job, tool, params, selection, None, tol)
}

/// 3D Finishing kept to the marked cells of the surface grid as well as
/// its boundary: how a rest machining tool finishes only what the tools
/// before it left.
///
/// # Errors
/// As [`finish`].
pub fn finish_within(
    surface: &Surface,
    job: &JobSetup,
    tool: &Tool,
    params: &FinishParams,
    selection: Option<&Region>,
    only: Option<&[bool]>,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let stepover = params.stepover.unwrap_or(tool.stepover);
    check(tool, stepover)?;
    if !params.stepover_retract.is_finite() || params.stepover_retract < 0.0 {
        return Err(CoreError::validation(
            "the stepover retract must be zero or more",
        ));
    }
    let bounds = Bounds {
        boundary: params.boundary,
        level: params.level,
        offset: params.boundary_offset,
    };
    let mut limits = Limits::new(surface, bounds, selection, job, tol)?;
    if let Some(only) = only {
        limits.mask = Some(match limits.mask.take() {
            Some(mask) => mask.iter().zip(only).map(|(a, b)| *a && *b).collect(),
            None => only.to_vec(),
        });
    }
    let heights = Heights::from_job(job);
    let finisher = Finisher {
        surface,
        footprint: Footprint::of(tool, surface.grid().cell)?,
        allowance: params.allowance,
        heights,
        stepover,
        direction: params.direction,
        retract: params.stepover_retract,
    };
    let mut out = Emitter::new(heights);
    match params.strategy {
        FinishStrategy::Raster => {
            let lines = raster(&limits, stepover, params.angle, false);
            finisher.raster(&mut out, &lines, (0.0, 1.0))?;
        }
        FinishStrategy::RasterXY => {
            let lines = raster(&limits, stepover, params.angle, false);
            finisher.raster(&mut out, &lines, (0.0, 0.5))?;
            let lines = raster(&limits, stepover, params.angle + 90.0, false);
            finisher.raster(&mut out, &lines, (0.5, 1.0))?;
        }
        FinishStrategy::Offset => {
            let area = match (&limits.mask, only) {
                (Some(cells), Some(_)) => mask_region(&limits.grid, cells, tol)?,
                _ => boundary_region(&limits, bounds, selection, tol)?,
            };
            finisher.offset(&mut out, &area, tol)?;
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

/// How low the tool may go at each raster sample, line by line and run by
/// run.
type SampleFloors = Vec<Vec<Vec<(Point, f64)>>>;

/// What every roughing level needs.
struct Rougher<'a> {
    heights: Heights,
    stepover: f64,
    link_limit: f64,
    ramp: Option<f64>,
    /// Floors on the grid, for outlines and depth first order.
    grid: Option<&'a Floors>,
    /// Floors at the raster samples, empty for offset clearing.
    samples: &'a SampleFloors,
    offset: bool,
    profile: RoughProfile,
    tol: Tolerance,
}

impl Rougher<'_> {
    /// How a cut at a level below `above` gets down.
    fn entry(&self, above: f64) -> Entry {
        let gap = self.heights.plunge_from - self.heights.surface;
        Entry {
            ramp: self.ramp,
            top: (above + gap).min(self.heights.plunge_from),
        }
    }

    /// The raster spans at height `z` where the tool may go down to it and
    /// `keep` accepts the sample.
    fn raster_level<K: Fn(Point) -> bool>(&self, out: &mut Emitter, z: f64, entry: Entry, keep: K) {
        for line in self.samples {
            for run in line {
                let mut span: Vec<Point3> = Vec::new();
                let mut spans: Vec<Vec<Point3>> = Vec::new();
                for &(p, floor) in run {
                    if floor <= z + 1e-9 && keep(p) {
                        span.push(Point3::new(p.x, p.y, z));
                    } else if !span.is_empty() {
                        spans.push(std::mem::take(&mut span));
                    }
                }
                if !span.is_empty() {
                    spans.push(span);
                }
                for span in spans {
                    let span = thin(&span, 0.005);
                    let here = out.at();
                    let near = span.first().is_some_and(|p| {
                        (here.z - z).abs() < 1e-9
                            && p.xy().distance(here.xy()) <= self.stepover * 1.5
                    });
                    cut_span(out, &span, z, near, entry);
                }
            }
        }
    }

    /// The loops round `outline` at height `z`.
    fn profile_level(
        &self,
        out: &mut Emitter,
        grid: &Floors,
        outline: &Region,
        z: f64,
        entry: Entry,
    ) {
        for chain in nearest_loops(outline, out.at().xy(), self.tol) {
            cut_loop(out, grid, &chain, z, self.link_limit, entry);
        }
    }

    /// Offset rings clearing `outline` at height `z`, each separate area
    /// from the middle outwards, ending on its outline.
    fn offset_level(
        &self,
        out: &mut Emitter,
        grid: &Floors,
        outline: &Region,
        z: f64,
        entry: Entry,
    ) {
        let tol = self.tol;
        let parts = order_nearest(
            components(outline, tol),
            out.at().xy(),
            |c| c.loops.first().map_or(Point::new(0.0, 0.0), |s| s.start),
            |c| c.loops.first().map_or(Point::new(0.0, 0.0), |s| s.start),
        );
        for part in parts {
            for ring in rings(&part, self.stepover, &grid.grid, tol).iter().rev() {
                for chain in nearest_loops(ring, out.at().xy(), tol) {
                    cut_loop(out, grid, &chain, z, self.link_limit, entry);
                }
            }
        }
    }

    /// One level over the area `keep_cell` and `keep_point` accept: the
    /// profile first or last and the raster or offset clearing.
    fn level<C, P>(
        &self,
        out: &mut Emitter,
        z: f64,
        above: f64,
        keep_cell: C,
        keep_point: P,
    ) -> Result<(), CoreError>
    where
        C: Fn(usize) -> bool,
        P: Fn(Point) -> bool,
    {
        let entry = self.entry(above);
        let outline = match self.grid {
            Some(grid) if self.offset || self.profile != RoughProfile::None => {
                Some((grid, grid.outline_where(z, keep_cell, self.tol)?))
            }
            _ => None,
        };
        if let (Some((grid, outline)), RoughProfile::First) = (&outline, self.profile) {
            self.profile_level(out, grid, outline, z, entry);
        }
        if !self.offset {
            self.raster_level(out, z, entry, keep_point);
        }
        if let Some((grid, outline)) = &outline {
            if self.offset {
                self.offset_level(out, grid, outline, z, entry);
            } else if self.profile == RoughProfile::Last {
                self.profile_level(out, grid, outline, z, entry);
            }
        }
        Ok(())
    }

    /// Depth first: each area a level splits into is cut down to the
    /// bottom before the next.
    fn depth_first(
        &self,
        out: &mut Emitter,
        grid: &Floors,
        levels: &[f64],
    ) -> Result<(), CoreError> {
        let Some(&first) = levels.first() else {
            return Ok(());
        };
        let g = grid.grid;
        let mut stamp = vec![0u32; g.width * g.height];
        let mut serial = 0u32;
        let all: Vec<usize> = (0..g.width * g.height).collect();
        let roots = split_areas(&all, |c| clear(grid, c, first), &g, &mut stamp, &mut serial);
        let mut mark = vec![0u32; g.width * g.height];
        let mut node = 0u32;
        self.visit_areas(
            out,
            grid,
            levels,
            0,
            roots,
            &mut mark,
            &mut node,
            &mut stamp,
            &mut serial,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_areas(
        &self,
        out: &mut Emitter,
        grid: &Floors,
        levels: &[f64],
        k: usize,
        areas: Vec<Vec<usize>>,
        mark: &mut Vec<u32>,
        node: &mut u32,
        stamp: &mut Vec<u32>,
        serial: &mut u32,
    ) -> Result<(), CoreError> {
        let g = grid.grid;
        let centre = |c: usize| {
            Point::new(
                g.origin.x + (c % g.width) as f64 * g.cell,
                g.origin.y + (c / g.width) as f64 * g.cell,
            )
        };
        let areas = order_nearest(areas, out.at().xy(), |a| centre(a[0]), |a| centre(a[0]));
        let z = levels[k];
        let above = if k == 0 {
            self.heights.surface
        } else {
            levels[k - 1]
        };
        for area in areas {
            *node += 1;
            let id = *node;
            for &c in &area {
                mark[c] = id;
            }
            {
                let marks: &[u32] = mark;
                let keep_point = |p: Point| {
                    let i = ((p.x - g.origin.x) / g.cell).round();
                    let j = ((p.y - g.origin.y) / g.cell).round();
                    i >= 0.0
                        && j >= 0.0
                        && (i as usize) < g.width
                        && (j as usize) < g.height
                        && marks[j as usize * g.width + i as usize] == id
                };
                self.level(out, z, above, |c| marks[c] == id, keep_point)?;
            }
            out.retract();
            if let Some(&next) = levels.get(k + 1) {
                let children = split_areas(&area, |c| clear(grid, c, next), &g, stamp, serial);
                self.visit_areas(
                    out,
                    grid,
                    levels,
                    k + 1,
                    children,
                    mark,
                    node,
                    stamp,
                    serial,
                )?;
            }
        }
        Ok(())
    }

    /// 3D raster: at every level, lines at the level or on the model plus
    /// the allowance where that is higher.
    fn raster3d<F: Fn(Point) -> f64>(
        &self,
        out: &mut Emitter,
        levels: &[f64],
        avoid: bool,
        floor_at: F,
        step: f64,
    ) {
        let top = self.heights.surface;
        for (k, &z) in levels.iter().enumerate() {
            let above = if k == 0 { f64::INFINITY } else { levels[k - 1] };
            for line in self.samples {
                for run in line {
                    let mut span: Vec<Point3> = Vec::new();
                    let mut spans: Vec<Vec<Point3>> = Vec::new();
                    for &(p, floor) in run {
                        let material = floor < top - 1e-9;
                        let fresh = !avoid || floor < above - 1e-9;
                        if material && fresh {
                            span.push(Point3::new(p.x, p.y, floor.max(z).min(top)));
                        } else if !span.is_empty() {
                            spans.push(std::mem::take(&mut span));
                        }
                    }
                    if !span.is_empty() {
                        spans.push(span);
                    }
                    for mut span in spans {
                        let here = out.at();
                        let near = span.first().is_some_and(|p| {
                            here.z < top - 1e-9 && p.xy().distance(here.xy()) <= self.stepover * 1.5
                        });
                        if near {
                            // Step over along the model, not straight through it.
                            let mut link: Vec<Point> = Vec::new();
                            let to = span[0].xy();
                            let seg = CutSeg {
                                from: here.xy(),
                                to,
                                bulge: 0.0,
                            };
                            sample(&seg, step, &mut link);
                            link.pop();
                            let lifted = link
                                .iter()
                                .map(|p| Point3::new(p.x, p.y, floor_at(*p).max(z).min(top)));
                            span.splice(0..0, lifted);
                        }
                        cut_run(out, &thin(&span, 0.005), near);
                    }
                }
            }
            out.retract();
        }
    }
}

/// True when the tool may go down to `z` over cell `c`.
fn clear(grid: &Floors, c: usize, z: f64) -> bool {
    let floor = grid.z[c];
    floor.is_finite() && floor <= z + 1e-9
}

/// The cells of `cells` that `keep` accepts, split into areas that touch
/// along cell sides, each in index order, the areas by their first cell.
fn split_areas<K: Fn(usize) -> bool>(
    cells: &[usize],
    keep: K,
    grid: &Grid,
    stamp: &mut [u32],
    serial: &mut u32,
) -> Vec<Vec<usize>> {
    *serial += 1;
    let member = *serial;
    for &c in cells {
        if keep(c) {
            stamp[c] = member;
        }
    }
    *serial += 1;
    let done = *serial;
    let w = grid.width;
    let mut out = Vec::new();
    for &c in cells {
        if stamp[c] != member {
            continue;
        }
        stamp[c] = done;
        let mut area = vec![c];
        let mut next = 0;
        while next < area.len() {
            let x = area[next];
            next += 1;
            let (i, j) = (x % w, x / w);
            let mut around = [usize::MAX; 4];
            if i > 0 {
                around[0] = x - 1;
            }
            if i + 1 < w {
                around[1] = x + 1;
            }
            if j > 0 {
                around[2] = x - w;
            }
            if j + 1 < grid.height {
                around[3] = x + w;
            }
            for n in around {
                if n != usize::MAX && stamp[n] == member {
                    stamp[n] = done;
                    area.push(n);
                }
            }
        }
        area.sort_unstable();
        out.push(area);
    }
    out
}

/// 3D Roughing: Z level raster or offset clearing level by level or depth
/// first, or 3D raster passes, taking everything above the model plus the
/// allowance.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a bad stepover, pass depth, ramp
/// or tool, a boundary with nothing in it, or when the model leaves nothing
/// to rough out.
pub fn rough(
    surface: &Surface,
    job: &JobSetup,
    tool: &Tool,
    params: &RoughParams,
    selection: Option<&Region>,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let stepover = params.stepover.unwrap_or(tool.stepover);
    check(tool, stepover)?;
    let ramp = if params.ramp.enabled && params.strategy != RoughStrategy::Raster3d {
        if !params.ramp.distance.is_finite() || params.ramp.distance <= 0.0 {
            return Err(CoreError::validation(
                "the ramp distance must be more than zero",
            ));
        }
        Some(params.ramp.distance)
    } else {
        None
    };
    let footprint = Footprint::of(tool, surface.grid().cell)?;
    let bounds = Bounds {
        boundary: params.boundary,
        level: params.level,
        offset: params.boundary_offset,
    };
    let limits = Limits::new(surface, bounds, selection, job, tol)?;
    let heights = Heights::from_job(job);
    let offset = params.strategy == RoughStrategy::ZLevelOffset;
    let profile = params.profile_pass();
    let depth_first =
        params.order == RoughOrder::DepthFirst && params.strategy != RoughStrategy::Raster3d;
    let grid_floors = (offset || profile != RoughProfile::None || depth_first)
        .then(|| Floors::new(surface, &footprint, &limits, params.allowance));
    // How low the tool may go at each raster sample.
    let samples: SampleFloors = if offset {
        Vec::new()
    } else {
        raster(&limits, stepover, params.angle, params.reverse_step)
            .lines
            .par_iter()
            .map(|line| {
                line.iter()
                    .map(|run| {
                        run.iter()
                            .map(|p| (*p, surface.drop_cutter(&footprint, *p) + params.allowance))
                            .collect()
                    })
                    .collect()
            })
            .collect()
    };
    let lowest = match &grid_floors {
        Some(grid) if offset => grid.lowest(),
        _ => samples
            .iter()
            .flatten()
            .flatten()
            .map(|(_, z)| *z)
            .fold(f64::MAX, f64::min),
    };
    let depth = heights.surface - lowest;
    if !depth.is_finite() || depth <= 1e-6 {
        return Err(CoreError::validation(
            "the model leaves no material to rough out",
        ));
    }
    let levels: Vec<f64> = pass_depths(0.0, depth, params.pass_depth.unwrap_or(tool.pass_depth))?
        .into_iter()
        .map(|d| heights.z_at(d))
        .collect();
    let rougher = Rougher {
        heights,
        stepover,
        link_limit: stepover * 2.0,
        ramp,
        grid: grid_floors.as_ref(),
        samples: &samples,
        offset,
        profile,
        tol,
    };
    let mut out = Emitter::new(heights);
    if params.strategy == RoughStrategy::Raster3d {
        let floor_at = |p: Point| surface.drop_cutter(&footprint, p) + params.allowance;
        rougher.raster3d(
            &mut out,
            &levels,
            params.avoid_machined,
            floor_at,
            surface.grid().cell,
        );
    } else if let (true, Some(grid)) = (depth_first, grid_floors.as_ref()) {
        rougher.depth_first(&mut out, grid, &levels)?;
    } else {
        for (k, &z) in levels.iter().enumerate() {
            let above = if k == 0 {
                heights.surface
            } else {
                levels[k - 1]
            };
            rougher.level(&mut out, z, above, |_| true, |_| true)?;
            out.retract();
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::model::field::Field;

    fn job() -> JobSetup {
        JobSetup {
            width: 60.0,
            height: 40.0,
            ..JobSetup::default()
        }
    }

    /// A dome 5 high and 10 across in the middle of the job.
    fn dome() -> Field {
        let cell = 0.25;
        let (w, h) = ((60.0 / cell) as usize, (40.0 / cell) as usize);
        let mut field = Field::empty(w, h, cell, Point::new(cell / 2.0, cell / 2.0));
        for j in 0..h {
            for i in 0..w {
                let p = field.centre(i, j);
                let r = p.distance(Point::new(30.0, 20.0));
                field.z[j * w + i] = if r < 5.0 {
                    (25.0f64 - r * r).sqrt() as f32
                } else {
                    0.0
                };
            }
        }
        field
    }

    fn ball(d: f64) -> Tool {
        let mut tool = Tool {
            name: "ball".into(),
            tool_type: ToolGeometry::BallNose { diameter: d },
            pass_depth: 2.0,
            ..Tool::default()
        };
        tool.set_stepover_percent(10.0);
        tool
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    /// Two 3 mm plateaus with a 2 mm wide groove between them at X 15.
    fn grooved() -> (Surface, JobSetup) {
        let job = JobSetup {
            width: 30.0,
            height: 20.0,
            ..JobSetup::default()
        };
        let cell = 0.25;
        let (w, h) = ((30.0 / cell) as usize, (20.0 / cell) as usize);
        let mut field = Field::empty(w, h, cell, Point::new(cell / 2.0, cell / 2.0));
        for j in 0..h {
            for i in 0..w {
                let p = field.centre(i, j);
                let plateau = (5.0..25.0).contains(&p.x) && (5.0..15.0).contains(&p.y);
                let groove = (p.x - 15.0).abs() < 1.0;
                field.z[j * w + i] = if plateau && !groove { 3.0 } else { 0.0 };
            }
        }
        (Surface::from_composite(&field, &job), job)
    }

    fn end_mill(d: f64) -> Tool {
        let mut tool = Tool {
            name: format!("End Mill {d}"),
            tool_type: ToolGeometry::EndMill { diameter: d },
            pass_depth: 2.0,
            ..Tool::default()
        };
        tool.set_stepover_percent(40.0);
        tool
    }

    #[test]
    fn a_later_tool_finishes_only_where_the_earlier_one_could_not_reach() {
        let (surface, job) = grooved();
        let big = end_mill(6.0);
        let small = end_mill(1.0);
        let at = |cells: &[bool], x: f64, y: f64| {
            let g = surface.grid();
            let i = ((x - g.origin.x) / g.cell).round() as usize;
            let j = ((y - g.origin.y) / g.cell).round() as usize;
            cells[j * g.width + i]
        };
        let rest = RestParams {
            minimum_detail: 0.1,
            boundary_offset: 0.0,
        };
        let cells = surface.rest_cells(&[&big], &small, &rest).expect("rest");
        assert!(at(&cells, 15.0, 10.0), "the groove floor");
        assert!(!at(&cells, 8.0, 10.0), "the open plateau");
        assert!(!at(&cells, 2.0, 2.0), "the open floor");
        let grown = surface
            .rest_cells(
                &[&big],
                &small,
                &RestParams {
                    boundary_offset: 2.0,
                    ..rest
                },
            )
            .expect("rest");
        assert!(at(&grown, 17.5, 10.0) && !at(&cells, 17.5, 10.0));
        let none = surface.rest_cells(&[&small], &big, &rest).expect("rest");
        assert!(!none.iter().any(|c| *c), "a bigger tool has nothing left");
        let deep = RestParams {
            minimum_detail: 5.0,
            ..rest
        };
        let skipped = surface.rest_cells(&[&big], &small, &deep).expect("rest");
        assert!(
            !skipped.iter().any(|c| *c),
            "3 mm is under the minimum detail"
        );

        let params = FinishParams {
            boundary: Boundary::Material,
            ..FinishParams::default()
        };
        let done = finish_within(&surface, &job, &small, &params, None, Some(&cells), tol())
            .expect("finishes");
        let cuts: Vec<_> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.to.z < -1.0)
            .collect();
        assert!(!cuts.is_empty());
        assert!(
            cuts.iter().all(|m| (m.to.x - 15.0).abs() <= 1.5),
            "only in the groove"
        );
        let offset = FinishParams {
            strategy: FinishStrategy::Offset,
            ..params
        };
        assert!(
            finish_within(&surface, &job, &small, &offset, None, Some(&grown), tol())
                .expect("offset rings")
                .moves
                .iter()
                .any(|m| m.kind.is_cutting())
        );
    }

    #[test]
    fn the_model_position_in_material_lowers_the_model_by_the_gap() {
        let mut lowered = job();
        lowered.machine.model_gap = 3.0;
        let surface = Surface::from_composite(&dome(), &lowered);
        let footprint = Footprint::of(&ball(0.01), 0.25).expect("tiny tool");
        let top = surface.drop_cutter(&footprint, Point::new(30.0, 20.0));
        assert!((top + 3.0).abs() < 0.05, "{top}");
        // A gap deeper than the material allows stops at the bottom.
        lowered.machine.model_gap = 17.0;
        let deep = Surface::from_composite(&dome(), &lowered);
        let flat = deep.drop_cutter(&footprint, Point::new(5.0, 5.0));
        assert!((flat + 18.0).abs() < 0.01, "{flat}");
        assert!(lowered.validate().is_ok());
        lowered.machine.model_gap = 19.0;
        assert!(lowered.validate().is_err(), "more than the thickness");
    }

    #[test]
    fn the_model_sits_with_its_top_at_the_material_top() {
        let surface = Surface::from_composite(&dome(), &job());
        let footprint = Footprint::of(&ball(0.01), 0.25).expect("tiny tool");
        let top = surface.drop_cutter(&footprint, Point::new(30.0, 20.0));
        assert!(top.abs() < 0.05, "{top}");
        let flat = surface.drop_cutter(&footprint, Point::new(5.0, 5.0));
        // The sampled dome peaks a hair under 5, as no cell centre is on the
        // apex.
        assert!((flat + 5.0).abs() < 0.01, "zero plane 5 below: {flat}");
    }

    #[test]
    fn a_ball_nose_drop_on_a_dome_rides_on_the_sphere() {
        let surface = Surface::from_composite(&dome(), &job());
        let footprint = Footprint::of(&ball(6.0), 0.25).expect("footprint");
        // Over the top the tip touches the crest.
        assert!(
            surface
                .drop_cutter(&footprint, Point::new(30.0, 20.0))
                .abs()
                < 0.05
        );
        // Off to the side the ball's side meets the dome: the tip is where the
        // centre of a 3 mm ball rolling on a 5 mm sphere puts it.
        let x = 6.0;
        let centre_height = ((8.0f64 * 8.0) - x * x).sqrt();
        let want = centre_height - 3.0 - 5.0;
        let got = surface.drop_cutter(&footprint, Point::new(30.0 + x, 20.0));
        // The model is the sphere sampled on a 0.25 mm grid, so near a steep
        // contact the answer is good to about half a cell times the slope.
        assert!((got - want).abs() < 0.2, "{got} vs {want}");
    }

    #[test]
    fn finishing_follows_the_surface_without_gouging() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let done = finish(
            &surface,
            &job(),
            &tool,
            &FinishParams::default(),
            None,
            tol(),
        )
        .expect("finishes");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let cutting: Vec<_> = done.moves.iter().filter(|m| m.kind.is_cutting()).collect();
        assert!(cutting.len() > 20);
        for m in cutting {
            let floor = surface.drop_cutter(&footprint, m.to.xy());
            assert!(m.to.z >= floor - 1e-4, "gouge at {:?}: floor {floor}", m.to);
        }
        let crest = done.moves.iter().map(|m| m.to.z).fold(f64::MIN, f64::max);
        assert!(crest > 0.0, "reaches back up to safe height");
    }

    #[test]
    fn roughing_clears_down_to_the_allowance_in_levels() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = RoughParams {
            boundary: Boundary::Material,
            ..RoughParams::default()
        };
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let levels: std::collections::BTreeSet<i64> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| (m.to.z * 1000.0).round() as i64)
            .collect();
        assert!(levels.len() >= 3, "{levels:?}");
        for m in done
            .moves
            .iter()
            .filter(|m| m.kind == crate::cam::toolpath::MoveKind::Feed)
        {
            let floor = surface.drop_cutter(&footprint, m.to.xy()) + params.allowance;
            assert!(
                m.to.z >= floor - 1e-6,
                "cuts into the allowance at {:?}",
                m.to
            );
        }
    }

    /// Move count and a position weighted sum, to spot any change.
    fn fingerprint(moves: &[crate::cam::toolpath::Move]) -> (usize, f64) {
        let sum = moves
            .iter()
            .enumerate()
            .map(|(i, m)| (i as f64 + 1.0) * (m.to.x + 3.0 * m.to.y + 7.0 * m.to.z))
            .sum();
        (moves.len(), sum)
    }

    #[test]
    fn settings_without_the_new_fields_give_the_same_moves() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let rough_params: RoughParams =
            serde_json::from_str(r#"{"allowance":0.5,"angle":30,"boundary":"material"}"#)
                .expect("parses");
        let done = rough(&surface, &job(), &tool, &rough_params, None, tol()).expect("roughs");
        let (n, sum) = fingerprint(&done.moves);
        let finish_params: FinishParams =
            serde_json::from_str(r#"{"angle":15,"allowance":0.1}"#).expect("parses");
        let done = finish(&surface, &job(), &tool, &finish_params, None, tol()).expect("finishes");
        let (fn_, fsum) = fingerprint(&done.moves);
        // Recorded from the toolpaths before the strategies were added.
        assert_eq!(n, 1435);
        assert!((sum - 97_237_144.435_129).abs() < 1e-3, "{sum}");
        // The finish keeps to the Model boundary, recorded again when that
        // became the dome's outline instead of its box (fewer moves).
        assert_eq!(fn_, 420);
        assert!((fsum - 8_038_594.372_004).abs() < 1e-3, "{fsum}");
    }

    /// Points along a cutting move about 0.1 mm apart with the tool height
    /// there, arcs followed round their sweep.
    fn along(from: Point3, m: &crate::cam::toolpath::Move) -> Vec<Point3> {
        let length = m.length(from);
        let count = (length / 0.1).ceil().max(1.0) as usize;
        let sweep = m.arc_sweep(from);
        (0..=count)
            .map(|k| {
                let t = k as f64 / count as f64;
                let z = from.z + (m.to.z - from.z) * t;
                match (m.kind, sweep) {
                    (
                        crate::cam::toolpath::MoveKind::ArcCw { centre }
                        | crate::cam::toolpath::MoveKind::ArcCcw { centre },
                        Some(sweep),
                    ) => {
                        let start = centre.to(from.xy()).angle() + sweep * t;
                        let r = centre.distance(from.xy());
                        Point3::new(centre.x + r * start.cos(), centre.y + r * start.sin(), z)
                    }
                    _ => Point3::new(
                        from.x + (m.to.x - from.x) * t,
                        from.y + (m.to.y - from.y) * t,
                        z,
                    ),
                }
            })
            .collect()
    }

    /// Every cutting move with where it starts.
    fn cuts(moves: &[crate::cam::toolpath::Move]) -> Vec<(Point3, crate::cam::toolpath::Move)> {
        let mut at = Heights::from_job(&job()).home;
        let mut out = Vec::new();
        for m in moves {
            if m.kind.is_cutting() {
                out.push((at, *m));
            }
            at = m.to;
        }
        out
    }

    fn offset_rough() -> RoughParams {
        RoughParams {
            boundary: Boundary::Material,
            strategy: RoughStrategy::ZLevelOffset,
            ..RoughParams::default()
        }
    }

    #[test]
    fn offset_roughing_never_cuts_below_the_allowance() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = offset_rough();
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let moves = cuts(&done.moves);
        assert!(moves.len() > 20);
        for (from, m) in &moves {
            for p in along(*from, m) {
                let floor = surface.drop_cutter(&footprint, p.xy()) + params.allowance;
                assert!(p.z >= floor - 1e-6, "cuts into the allowance at {p:?}");
            }
        }
        let levels: std::collections::BTreeSet<i64> = moves
            .iter()
            .map(|(_, m)| (m.to.z * 1000.0).round() as i64)
            .collect();
        assert!(levels.len() >= 3, "{levels:?}");
    }

    #[test]
    fn offset_roughing_covers_each_level() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = offset_rough();
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        // Flat cuts by level.
        let mut levels: std::collections::BTreeMap<i64, Vec<Point>> = Default::default();
        for (from, m) in cuts(&done.moves) {
            if (from.z - m.to.z).abs() < 1e-9 {
                let key = (m.to.z * 1000.0).round() as i64;
                levels
                    .entry(key)
                    .or_default()
                    .extend(along(from, &m).iter().map(|p| p.xy()));
            }
        }
        assert!(levels.len() >= 3);
        // A lattice every half millimetre over the material.
        let (w, h, pitch) = (120usize, 80usize, 0.5);
        let reach = tool.stepover + 2.0 * 0.25;
        for (key, points) in &levels {
            let z = *key as f64 / 1000.0;
            let mut seen = vec![false; w * h];
            for p in points {
                let (ci, cj) = ((p.x / pitch) as i64, (p.y / pitch) as i64);
                for j in cj - 3..=cj + 3 {
                    for i in ci - 3..=ci + 3 {
                        if i < 0 || j < 0 || i >= w as i64 || j >= h as i64 {
                            continue;
                        }
                        let q = Point::new((i as f64 + 0.5) * pitch, (j as f64 + 0.5) * pitch);
                        if q.distance(*p) <= reach {
                            seen[j as usize * w + i as usize] = true;
                        }
                    }
                }
            }
            for j in 0..h {
                for i in 0..w {
                    let q = Point::new((i as f64 + 0.5) * pitch, (j as f64 + 0.5) * pitch);
                    let floor = surface.drop_cutter(&footprint, q) + params.allowance;
                    if floor <= z {
                        assert!(seen[j * w + i], "{q:?} missed at level {z}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_new_settings_parse_from_json() {
        let rough: RoughParams =
            serde_json::from_str(r#"{"strategy":"zLevelOffset","profileEachLevel":true}"#)
                .expect("parses");
        assert_eq!(rough.strategy, RoughStrategy::ZLevelOffset);
        assert!(rough.profile_each_level);
        let finish: FinishParams =
            serde_json::from_str(r#"{"strategy":"rasterXY"}"#).expect("parses");
        assert_eq!(finish.strategy, FinishStrategy::RasterXY);
        let finish: FinishParams =
            serde_json::from_str(r#"{"strategy":"offset"}"#).expect("parses");
        assert_eq!(finish.strategy, FinishStrategy::Offset);
    }

    #[test]
    fn a_profile_pass_rounds_each_raster_level() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let plain = RoughParams {
            boundary: Boundary::Material,
            ..RoughParams::default()
        };
        let profiled = RoughParams {
            profile_each_level: true,
            ..plain
        };
        let before = rough(&surface, &job(), &tool, &plain, None, tol()).expect("roughs");
        let after = rough(&surface, &job(), &tool, &profiled, None, tol()).expect("roughs");
        assert!(after.moves.len() > before.moves.len());
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        for (from, m) in cuts(&after.moves) {
            for p in along(from, &m) {
                let floor = surface.drop_cutter(&footprint, p.xy()) + plain.allowance;
                assert!(p.z >= floor - 1e-6, "cuts into the allowance at {p:?}");
            }
        }
    }

    #[test]
    fn raster_xy_cuts_both_ways() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = FinishParams {
            strategy: FinishStrategy::RasterXY,
            ..FinishParams::default()
        };
        let done = finish(&surface, &job(), &tool, &params, None, tol()).expect("finishes");
        let (mut in_x, mut in_y) = (0, 0);
        for (from, m) in cuts(&done.moves) {
            let (dx, dy) = ((m.to.x - from.x).abs(), (m.to.y - from.y).abs());
            if dx > 0.2 && dy < 1e-6 {
                in_x += 1;
            }
            if dy > 0.2 && dx < 1e-6 {
                in_y += 1;
            }
        }
        assert!(in_x > 10 && in_y > 10, "{in_x} along X, {in_y} along Y");
        let single = finish(
            &surface,
            &job(),
            &tool,
            &FinishParams::default(),
            None,
            tol(),
        )
        .expect("finishes");
        assert!(done.moves.len() > single.moves.len() * 3 / 2);
    }

    #[test]
    fn offset_finishing_rings_the_dome_on_the_surface() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = FinishParams {
            strategy: FinishStrategy::Offset,
            allowance: 0.1,
            ..FinishParams::default()
        };
        let done = finish(&surface, &job(), &tool, &params, None, tol()).expect("finishes");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let top = job().surface_z();
        let moves = cuts(&done.moves);
        assert!(moves.len() > 50);
        for (_, m) in &moves {
            let want = (surface.drop_cutter(&footprint, m.to.xy()) + params.allowance).min(top);
            assert!((m.to.z - want).abs() < 1e-9, "{:?} off the surface", m.to);
        }
        // Rings cross the line through the middle a stepover apart.
        let (lo, hi) = surface.model_box();
        let mid = (lo.y + hi.y) / 2.0 + 0.01;
        let mut xs: Vec<f64> = moves
            .iter()
            .filter(|(a, b)| (a.y - mid) * (b.to.y - mid) < 0.0)
            .map(|(a, b)| a.x + (mid - a.y) * (b.to.x - a.x) / (b.to.y - a.y))
            .filter(|x| *x > (lo.x + hi.x) / 2.0 + tool.stepover)
            .collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 0.05);
        let want = ((hi.x - lo.x) / 2.0 / tool.stepover) as usize;
        assert!(xs.len() + 2 >= want, "{} rings for {want}", xs.len());
        for pair in xs.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                gap > tool.stepover * 0.5 && gap < tool.stepover * 1.5,
                "rings {gap} apart: {xs:?}"
            );
        }
    }

    #[test]
    fn a_selection_boundary_needs_vectors() {
        let surface = Surface::from_composite(&dome(), &job());
        let params = FinishParams {
            boundary: Boundary::Selection,
            ..FinishParams::default()
        };
        assert!(finish(&surface, &job(), &ball(6.0), &params, None, tol()).is_err());
    }

    /// Two domes 5 high, 10 across, 30 apart along X.
    fn two_domes() -> Field {
        let cell = 0.25;
        let (w, h) = ((60.0 / cell) as usize, (40.0 / cell) as usize);
        let mut field = Field::empty(w, h, cell, Point::new(cell / 2.0, cell / 2.0));
        for j in 0..h {
            for i in 0..w {
                let p = field.centre(i, j);
                let r = p
                    .distance(Point::new(15.0, 20.0))
                    .min(p.distance(Point::new(45.0, 20.0)));
                field.z[j * w + i] = if r < 5.0 {
                    (25.0f64 - r * r).sqrt() as f32
                } else {
                    0.0
                };
            }
        }
        field
    }

    /// A plateau 5 high with two bowls 5 deep and 16 across, 30 apart.
    fn two_bowls() -> Field {
        let mut field = two_domes();
        let w = field.width;
        for j in 0..field.height {
            for i in 0..w {
                let p = field.centre(i, j);
                let r = p
                    .distance(Point::new(15.0, 20.0))
                    .min(p.distance(Point::new(45.0, 20.0)));
                let dip = (64.0 - r * r).max(0.0).sqrt() * 5.0 / 8.0;
                field.z[j * w + i] = (5.0 - dip) as f32 + 0.01;
            }
        }
        field
    }

    fn assert_no_gouge(
        surface: &Surface,
        tool: &Tool,
        moves: &[crate::cam::toolpath::Move],
        allowance: f64,
    ) {
        let footprint = Footprint::of(tool, 0.25).expect("footprint");
        // Move ends only: between samples the drop cutter is only as good
        // as the grid.
        for (_, m) in cuts(moves) {
            let floor = surface.drop_cutter(&footprint, m.to.xy()) + allowance;
            assert!(
                m.to.z >= floor - 1e-6,
                "cuts into the allowance at {:?}",
                m.to
            );
        }
    }

    fn reach_x(moves: &[crate::cam::toolpath::Move]) -> (f64, f64) {
        cuts(moves)
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), (_, m)| {
                (a.min(m.to.x), b.max(m.to.x))
            })
    }

    #[test]
    fn a_boundary_offset_lets_the_tool_run_past_the_model() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let plain = finish(
            &surface,
            &job(),
            &tool,
            &FinishParams::default(),
            None,
            tol(),
        )
        .expect("finishes");
        let grown = FinishParams {
            boundary_offset: 3.0,
            ..FinishParams::default()
        };
        let wide = finish(&surface, &job(), &tool, &grown, None, tol()).expect("finishes");
        let (a0, a1) = reach_x(&plain.moves);
        let (b0, b1) = reach_x(&wide.moves);
        assert!(
            (a0 - b0 - 3.0).abs() < 0.3 && (b1 - a1 - 3.0).abs() < 0.3,
            "{a0} {a1} {b0} {b1}"
        );
        let shrunk = FinishParams {
            boundary_offset: -20.0,
            ..FinishParams::default()
        };
        assert!(finish(&surface, &job(), &tool, &shrunk, None, tol()).is_err());
    }

    #[test]
    fn a_level_boundary_keeps_to_that_levels_components() {
        let field = two_domes();
        let grid = Grid {
            width: field.width,
            height: field.height,
            cell: field.cell,
            origin: field.origin,
        };
        // Level 7 holds only the left dome.
        let mut left = vec![false; grid.width * grid.height];
        cover(
            &mut left,
            &Field {
                z: field
                    .z
                    .iter()
                    .enumerate()
                    .map(|(k, z)| {
                        if k % grid.width < grid.width / 2 {
                            *z
                        } else {
                            0.0
                        }
                    })
                    .collect(),
                ..field.clone()
            },
            &grid,
        );
        let surface = Surface::from_composite(&field, &job()).with_level(7, left);
        let tool = ball(6.0);
        let params = FinishParams {
            boundary: Boundary::Level,
            level: 7,
            ..FinishParams::default()
        };
        let done = finish(&surface, &job(), &tool, &params, None, tol()).expect("finishes");
        let (x0, x1) = reach_x(&done.moves);
        assert!(x0 > 9.0 && x1 < 21.0, "{x0} {x1}");
        let offset = FinishParams {
            strategy: FinishStrategy::Offset,
            boundary_offset: 1.0,
            ..params
        };
        let done = finish(&surface, &job(), &tool, &offset, None, tol()).expect("finishes");
        let (x0, x1) = reach_x(&done.moves);
        assert!(x0 > 8.0 && x1 < 22.0 && x0 < 10.5, "{x0} {x1}");
        let missing = FinishParams { level: 3, ..params };
        assert!(finish(&surface, &job(), &tool, &missing, None, tol()).is_err());
    }

    #[test]
    fn the_model_boundary_is_the_components_outline_not_their_box() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(3.0);
        let done = finish(
            &surface,
            &job(),
            &tool,
            &FinishParams::default(),
            None,
            tol(),
        )
        .expect("finishes");
        // Every cut keeps inside the dome's round outline, so none reaches
        // the corners of its box.
        let centre = Point::new(30.0, 20.0);
        let far = cuts(&done.moves)
            .iter()
            .map(|(_, m)| m.to.xy().distance(centre))
            .fold(0.0, f64::max);
        assert!(far < 5.3, "{far}");
        assert!(far > 4.5, "{far}");
        // A ring's hole is inside its outline too.
        let mut ring = dome();
        let w = ring.width;
        for j in 0..ring.height {
            for i in 0..w {
                if ring.centre(i, j).distance(centre) < 2.0 {
                    ring.z[j * w + i] = 0.0;
                }
            }
        }
        let outline = Surface::from_composite(&ring, &job())
            .model_outline()
            .expect("an outline");
        let at = |p: Point| {
            let (i, j) = ((p.x / 0.25) as usize, (p.y / 0.25) as usize);
            outline[j * w + i]
        };
        assert!(at(centre));
        assert!(at(Point::new(33.0, 20.0)));
        assert!(!at(Point::new(34.0, 24.0)));
    }

    #[test]
    fn offset_finishing_keeps_to_the_model_outline() {
        // A round dome in a square box: the offset rings stay inside the
        // dome's outline and never run round the box's corners on the flat.
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(3.0);
        let params = FinishParams {
            strategy: FinishStrategy::Offset,
            ..FinishParams::default()
        };
        let done = finish(&surface, &job(), &tool, &params, None, tol()).expect("finishes");
        // The dome is 5 mm round about (30, 20); its box corners are 7 mm out.
        let centre = Point::new(30.0, 20.0);
        let moves = cuts(&done.moves);
        assert!(moves.len() > 50);
        for (from, m) in &moves {
            for p in [from.xy(), m.to.xy()] {
                assert!(p.distance(centre) < 5.3, "{p:?} off the model");
            }
        }
    }

    #[test]
    fn raster_3d_roughing_leaves_an_even_skin() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        // The Model boundary is the dome's outline, not its box: grown by the
        // tool radius the tool runs past the dome's edge down to the flat,
        // which upper levels clear for Avoid machined to skip; on the bare
        // outline the ball only rides the dome top.
        let params = RoughParams {
            strategy: RoughStrategy::Raster3d,
            boundary_offset: 3.0,
            ..RoughParams::default()
        };
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        assert_no_gouge(&surface, &tool, &done.moves, params.allowance);
        // The last level rides on the dome plus the allowance.
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let on_skin = cuts(&done.moves)
            .iter()
            .filter(|(_, m)| {
                let floor = surface.drop_cutter(&footprint, m.to.xy()) + params.allowance;
                (m.to.z - floor).abs() < 1e-6 && floor > -4.0
            })
            .count();
        assert!(on_skin > 50, "{on_skin}");
        let avoid = RoughParams {
            avoid_machined: true,
            ..params
        };
        let fewer = rough(&surface, &job(), &tool, &avoid, None, tol()).expect("roughs");
        assert_no_gouge(&surface, &tool, &fewer.moves, params.allowance);
        let length = |moves: &[crate::cam::toolpath::Move]| -> f64 {
            cuts(moves).iter().map(|(a, m)| m.length(*a)).sum()
        };
        let ratio = length(&fewer.moves) / length(&done.moves);
        assert!(ratio < 0.9, "{ratio}");
    }

    #[test]
    fn depth_first_finishes_one_area_before_the_next() {
        let surface = Surface::from_composite(&two_bowls(), &job());
        let tool = ball(3.0);
        let params = RoughParams {
            boundary: Boundary::Material,
            order: RoughOrder::DepthFirst,
            pass_depth: Some(1.0),
            ..RoughParams::default()
        };
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        assert_no_gouge(&surface, &tool, &done.moves, params.allowance);
        // Each bowl is a separate area: depth first takes one bowl to the
        // bottom, then the other.
        let deep: Vec<bool> = cuts(&done.moves)
            .iter()
            .filter(|(_, m)| m.to.z < -1.5)
            .map(|(_, m)| m.to.x < 30.0)
            .collect();
        assert!(deep.len() > 10);
        assert!(deep.windows(2).filter(|w| w[0] != w[1]).count() == 1);
        let switches = deep.windows(2).filter(|w| w[0] != w[1]).count();
        let level = RoughParams {
            order: RoughOrder::LevelByLevel,
            ..params
        };
        let by_level = rough(&surface, &job(), &tool, &level, None, tol()).expect("roughs");
        let deep_level: Vec<bool> = cuts(&by_level.moves)
            .iter()
            .filter(|(_, m)| m.to.z < -1.5)
            .map(|(_, m)| m.to.x < 30.0)
            .collect();
        let level_switches = deep_level.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(switches < level_switches, "{switches} vs {level_switches}");
    }

    #[test]
    fn a_profile_can_come_before_the_raster() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let first = RoughParams {
            boundary: Boundary::Material,
            profile: Some(RoughProfile::First),
            ..RoughParams::default()
        };
        let last = RoughParams {
            profile: Some(RoughProfile::Last),
            ..first
        };
        let a = rough(&surface, &job(), &tool, &first, None, tol()).expect("roughs");
        let b = rough(&surface, &job(), &tool, &last, None, tol()).expect("roughs");
        assert_no_gouge(&surface, &tool, &a.moves, first.allowance);
        assert_ne!(a.moves[..20], b.moves[..20]);
        // The old flag reads as a profile after the raster.
        let old: RoughParams =
            serde_json::from_str(r#"{"profileEachLevel":true}"#).expect("parses");
        assert_eq!(old.profile_pass(), RoughProfile::Last);
    }

    #[test]
    fn reversed_steps_start_from_the_far_side() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let params = RoughParams {
            boundary: Boundary::Material,
            reverse_step: true,
            ..RoughParams::default()
        };
        let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
        let first = cuts(&done.moves)[0].1.to;
        assert!(first.y > 35.0, "{first:?}");
        let plain = rough(
            &surface,
            &job(),
            &tool,
            &RoughParams {
                reverse_step: false,
                ..params
            },
            None,
            tol(),
        )
        .expect("roughs");
        assert!(cuts(&plain.moves)[0].1.to.y < 5.0);
    }

    #[test]
    fn ramps_replace_plunges() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        for strategy in [RoughStrategy::ZLevelRaster, RoughStrategy::ZLevelOffset] {
            let params = RoughParams {
                boundary: Boundary::Material,
                strategy,
                ramp: RampSpec {
                    enabled: true,
                    distance: 10.0,
                    ..RampSpec::default()
                },
                ..RoughParams::default()
            };
            let done = rough(&surface, &job(), &tool, &params, None, tol()).expect("roughs");
            assert!(
                !done
                    .moves
                    .iter()
                    .any(|m| m.kind == crate::cam::toolpath::MoveKind::Plunge),
                "{strategy:?} still plunges"
            );
            assert_no_gouge(&surface, &tool, &done.moves, params.allowance);
        }
    }

    /// Twice the signed area of the first ring cut after the first plunge.
    fn first_ring_turn(moves: &[crate::cam::toolpath::Move]) -> f64 {
        let start = moves
            .iter()
            .position(|m| m.kind == crate::cam::toolpath::MoveKind::Plunge)
            .expect("plunges");
        let ring: Vec<Point3> = moves[start..]
            .iter()
            .take_while(|m| m.kind != crate::cam::toolpath::MoveKind::Rapid)
            .map(|m| m.to)
            .collect();
        ring.windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum()
    }

    #[test]
    fn offset_finishing_turns_either_way_and_can_lift() {
        let surface = Surface::from_composite(&dome(), &job());
        let tool = ball(6.0);
        let climb = FinishParams {
            strategy: FinishStrategy::Offset,
            ..FinishParams::default()
        };
        let conventional = FinishParams {
            direction: CutDirection::Conventional,
            ..climb
        };
        let a = finish(&surface, &job(), &tool, &climb, None, tol()).expect("finishes");
        let b = finish(&surface, &job(), &tool, &conventional, None, tol()).expect("finishes");
        assert!(first_ring_turn(&a.moves) * first_ring_turn(&b.moves) < 0.0);
        let lifted = FinishParams {
            stepover_retract: 2.0,
            ..climb
        };
        let c = finish(&surface, &job(), &tool, &lifted, None, tol()).expect("finishes");
        let footprint = Footprint::of(&tool, 0.25).expect("footprint");
        let lifts = c
            .moves
            .iter()
            .filter(|m| m.kind == crate::cam::toolpath::MoveKind::Feed)
            .filter(|m| m.to.z - surface.drop_cutter(&footprint, m.to.xy()) > 1.5)
            .count();
        assert!(lifts > 5, "{lifts}");
        assert_no_gouge(&surface, &tool, &c.moves, 0.0);
    }

    #[test]
    fn offset_rings_on_a_traced_outline_are_whole_and_a_stepover_apart() {
        let document = crate::io::native::read_bytes(
            include_bytes!("../../assets/samples/relief-plaque.carve"),
            "relief-plaque",
            1,
        )
        .expect("the sample opens");
        let surface = Surface::of(&document);
        let bounds = Bounds {
            boundary: Boundary::Model,
            level: 0,
            offset: 0.0,
        };
        let limits = Limits::new(&surface, bounds, None, document.job(), tol()).expect("limits");
        let area = boundary_region(&limits, bounds, None, tol()).expect("an outline");
        let stepover = 0.635;
        let found = rings(&area, stepover, &limits.grid, tol());

        // What each ring should enclose, worked out apart from the offsetter:
        // the cells further than its distance from the traced outline.
        let grid = limits.grid;
        let cells = limits.mask.as_ref().expect("a model outline");
        let outside: Vec<bool> = cells.iter().map(|c| !c).collect();
        let depth: Vec<f64> = distance_to(&outside, grid.width, grid.height)
            .iter()
            .map(|d| (*d - 0.5) * grid.cell)
            .collect();
        let deepest = depth.iter().fold(0.0f64, |a, b| a.max(*b));
        assert!(
            found.len() as f64 >= deepest / stepover - 2.0,
            "{} rings stop short of {deepest}",
            found.len()
        );
        let bad: Vec<(usize, f64, f64)> = found
            .iter()
            .enumerate()
            .map(|(k, ring)| {
                let want = depth.iter().filter(|d| **d > k as f64 * stepover).count() as f64
                    * grid.cell
                    * grid.cell;
                (k, ring, want)
            })
            .filter(|(_, ring, want)| {
                let band = region_length(ring, tol()) * grid.cell + 4.0 * grid.cell * grid.cell;
                (ring.area() - want).abs() > band
            })
            .map(|(k, ring, want)| (k, ring.area(), want))
            .collect();
        assert!(bad.is_empty(), "{} rings are wrong: {bad:?}", bad.len());
    }

    #[test]
    fn a_ring_with_pieces_gone_is_not_whole() {
        let square = |lo: f64, hi: f64| {
            Region::from_paths(
                &[rect(Point::new(lo, lo), Point::new(hi, hi)).expect("a square")],
                tol(),
            )
        };
        let from = square(0.0, 100.0);
        // A true offset by 1 takes 396 from a square with edges 400 long.
        assert!(ring_is_whole(&from, &square(1.0, 99.0), 1.0, tol()));
        // The limit is 500 and a little for the corners: just past it fails.
        assert!(ring_is_whole(&from, &square(2.0, 98.0), 2.0, tol()));
        assert!(!ring_is_whole(&from, &square(3.0, 97.0), 1.0, tol()));
        // Most of the ring gone, or all of it while the middle is still wide.
        assert!(!ring_is_whole(&from, &square(40.0, 60.0), 1.0, tol()));
        assert!(!ring_is_whole(
            &from,
            &Region { loops: Vec::new() },
            1.0,
            tol()
        ));
        // The last ring of a thin area really can leave nothing.
        assert!(ring_is_whole(
            &square(0.0, 2.0),
            &Region { loops: Vec::new() },
            1.0,
            tol()
        ));
        // Area can only fall as a ring moves in: a dropped hole adds some.
        let holed = Region::from_paths(
            &[
                rect(Point::new(0.0, 0.0), Point::new(100.0, 100.0)).expect("a square"),
                rect(Point::new(40.0, 40.0), Point::new(60.0, 60.0)).expect("a square"),
            ],
            tol(),
        );
        assert!(!ring_is_whole(&holed, &square(1.0, 99.0), 1.0, tol()));
    }

    #[test]
    fn a_ring_the_offsetter_fails_on_or_breaks_is_traced_in_its_place() {
        let square = |lo: f64, hi: f64| {
            Region::from_paths(
                &[rect(Point::new(lo, lo), Point::new(hi, hi)).expect("a square")],
                tol(),
            )
        };
        let grid = Grid {
            origin: Point::new(0.125, 0.125),
            cell: 0.25,
            width: 160,
            height: 160,
        };
        let region = square(0.0, 40.0);
        // A failure, a ring with most of it gone, and nothing at all while the
        // middle is still 20 wide, none of which may stop or thin the rings.
        let found = rings_by(&region, 2.0, &grid, tol(), |depth| match depth as i64 {
            4 => None,
            6 => Some(square(19.0, 21.0)),
            10 => Some(Region { loops: Vec::new() }),
            20.. => Some(Region { loops: Vec::new() }),
            _ => Some(square(depth, 40.0 - depth)),
        });
        let sides: Vec<f64> = found.iter().map(|ring| ring.area().sqrt()).collect();
        assert!(sides.len() >= 10, "{sides:?}");
        for (k, side) in sides.iter().take(10).enumerate() {
            let want = 40.0 - 4.0 * k as f64;
            assert!((side - want).abs() < 1.0, "ring {k}: {sides:?}");
        }
    }

    #[test]
    fn offset_rings_survive_an_offsetter_failure() {
        let job = JobSetup {
            width: 300.0,
            height: 200.0,
            ..JobSetup::default()
        };
        let grid = crate::model::grid_for(&job);
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for j in 0..grid.height {
            for i in 0..grid.width {
                let r = field.centre(i, j).distance(Point::new(150.0, 100.0));
                field.z[j * grid.width + i] = if r < 50.0 {
                    (50.0 - r) as f32 * 0.2
                } else {
                    0.0
                };
            }
        }
        let covered: Vec<bool> = field.z.iter().map(|z| *z > 0.0).collect();
        let surface = Surface::from_composite(&field, &job).with_level(1, covered);
        let params = FinishParams {
            boundary: Boundary::Level,
            level: 1,
            strategy: FinishStrategy::Offset,
            ..FinishParams::default()
        };
        // This tool made the offsetter assert on one of the rings.
        let mut tool = ball(3.175);
        tool.set_stepover_percent(10.0);
        let done = finish(&surface, &job, &tool, &params, None, tol()).expect("finishes");
        assert!(done.moves.len() > 100);
    }
}
