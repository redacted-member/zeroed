//! Relief model: components (heightfields made by the modelling tools) and
//! the composite they bake to, which the 3D view shows and 3D toolpaths
//! will cut.
//!
//! Heights are millimetres above the model's zero plane. Components are
//! stored cropped to the cells they cover, on the grid they were made on;
//! baking resamples them onto the model grid, bottom of the list first.

pub mod bitmap_relief;
pub mod brush_file;
pub mod combine_ops;
pub mod commands;
pub mod component_cmd;
pub mod contour;
pub mod create_cmd;
pub mod cross_section;
pub mod custom_brush;
pub mod distort;
pub mod dynamic;
pub mod edits;
pub mod extrude;
pub mod field;
pub mod import;
pub mod mesh;
pub mod refine;
pub mod sculpt;
pub mod shape;
pub mod slice;
pub mod split;
pub mod sweep;
pub mod texture_area;
pub mod tools_cmd;
pub mod transform;
pub mod tree;
pub mod tree_cmd;
pub mod turn;

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::geom::Point;

use field::Field;
use shape::Grid;

/// Cells the model grid aims for over the job, as a preview at standard
/// quality does.
pub const MODEL_CELLS: f64 = 1.0e6;

/// How a component meets what is below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Combine {
    /// Heights add to what is there.
    #[default]
    Add,
    /// Heights are taken away from what is there.
    Subtract,
    /// The higher of the two wins.
    MergeHigh,
    /// The lower of the two wins where the component is.
    MergeLow,
    /// Heights multiply what is there.
    Multiply,
}

/// One part of the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Document wide id, never reused.
    pub id: u64,
    /// Name in the component list.
    pub name: String,
    /// Hidden components do not bake.
    pub visible: bool,
    /// How it meets what is below it.
    pub combine: Combine,
    /// Multiplies every height.
    #[serde(default = "one")]
    pub height_scale: f64,
    /// The level it sits on; 0 in files written before levels existed,
    /// which puts it on the first level.
    #[serde(default)]
    pub level: u64,
    /// Base height, colour and group; defaults in older files.
    #[serde(flatten)]
    pub extra: tree::ComponentExtra,
    /// Heights, cropped to the cells the component covers.
    pub field: Field,
}

/// A layer of the model: its components bake together first, then the
/// level meets the levels below it with its own combine mode. A level of
/// bumps can be subtracted from the levels below as one piece, for example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    /// Document wide id, never reused.
    pub id: u64,
    /// Name in the component list.
    pub name: String,
    /// Hidden levels do not bake.
    pub visible: bool,
    /// How the level meets the levels below it.
    pub combine: Combine,
    /// Groups of this level's components; absent in older files.
    #[serde(default)]
    pub groups: Vec<tree::Group>,
    /// Ids of the closed vectors the clipping was last taken from; empty
    /// for none. Update Clipping reads them again when nothing is selected.
    #[serde(default)]
    pub clip: Vec<u64>,
    /// The clipping boundary as it was when the clipping was applied or
    /// last updated, so later edits to the vectors do not move it. Empty in
    /// older files, which clip to the live vectors in `clip` instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clip_shape: Vec<crate::geom::Path>,
    /// Preview colour as `#rrggbb` for every component in it; none takes
    /// the colours of the components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Name of the preview material `color` was taken from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Mirror mode: the level's model in one half (or quarter) of the job
    /// is mirrored live into the rest; none for no mirroring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<MirrorMode>,
    /// Millimetres added under everything the level's components cover,
    /// after they combine and before the level meets the levels below. A
    /// bake of the components leaves it out.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub base_height: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// How a level mirrors its model about the middle of the job. The first
/// four keep one half (the named side) and mirror it onto the other; the
/// quadrants keep one quarter and mirror it both ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorMode {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Level {
    /// The level every document starts with.
    pub fn first() -> Level {
        Level {
            id: 1,
            name: "Level 1".to_string(),
            visible: true,
            combine: Combine::Add,
            groups: Vec::new(),
            clip: Vec::new(),
            clip_shape: Vec::new(),
            color: None,
            material: None,
            mirror: None,
            base_height: 0.0,
        }
    }

    /// Whether the level has clipping applied.
    pub fn is_clipped(&self) -> bool {
        !self.clip.is_empty() || !self.clip_shape.is_empty()
    }
}

fn one() -> f64 {
    1.0
}

/// The model grid over a job: square cells, about [`MODEL_CELLS`] of them
/// at standard resolution and that many times the job's
/// [`crate::doc::ModelResolution::cell_factor`] otherwise.
pub fn grid_for(job: &JobSetup) -> Grid {
    let (datum_x, datum_y) = job.datum();
    let cells = MODEL_CELLS * job.resolution.cell_factor();
    let cell = ((job.width * job.height) / cells).sqrt().max(0.01);
    let width = ((job.width / cell).round() as usize).max(1);
    let height = ((job.height / cell).round() as usize).max(1);
    Grid {
        width,
        height,
        cell,
        origin: Point::new(-datum_x + cell / 2.0, -datum_y + cell / 2.0),
    }
}

/// Bakes visible components, bottom of the list first, onto `grid`.
/// Heights start at zero, the model's zero plane.
pub fn bake<'a, I>(components: I, grid: &Grid) -> Field
where
    I: IntoIterator<Item = &'a Component>,
{
    bake_covering(components, grid).0
}

/// Bakes visible components onto `grid` and answers which cells any of
/// them reached.
fn bake_covering<'a, I>(components: I, grid: &Grid) -> (Field, Vec<bool>)
where
    I: IntoIterator<Item = &'a Component>,
{
    let mut out = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    out.z.fill(0.0);
    let mut covered = vec![false; grid.width * grid.height];
    bake_into(&mut out, &mut covered, components, grid);
    (out, covered)
}

/// Bakes visible components onto `out` (on `grid`), marking the cells
/// they reach in `covered`.
fn bake_into<'a, I>(out: &mut Field, covered: &mut [bool], components: I, grid: &Grid)
where
    I: IntoIterator<Item = &'a Component>,
{
    for component in components {
        if !component.visible {
            continue;
        }
        let f = &component.field;
        let extra = &component.extra;
        // Only the cells under the component's own box need visiting.
        let i0 = (((f.origin.x - grid.origin.x) / grid.cell).floor().max(0.0)) as usize;
        let j0 = (((f.origin.y - grid.origin.y) / grid.cell).floor().max(0.0)) as usize;
        let i1 = ((((f.origin.x + f.width as f64 * f.cell) - grid.origin.x) / grid.cell).ceil())
            .clamp(0.0, grid.width as f64) as usize;
        let j1 = ((((f.origin.y + f.height as f64 * f.cell) - grid.origin.y) / grid.cell).ceil())
            .clamp(0.0, grid.height as f64) as usize;
        for j in j0..j1 {
            for i in i0..i1 {
                let p = Point::new(
                    grid.origin.x + i as f64 * grid.cell,
                    grid.origin.y + j as f64 * grid.cell,
                );
                let v = f.at(p);
                if v.is_nan() {
                    continue;
                }
                let v = dynamic::apply(
                    v,
                    p,
                    component.height_scale,
                    extra.base_height,
                    extra.fade.as_ref(),
                    extra.tilt.as_ref(),
                );
                let index = j * grid.width + i;
                covered[index] = true;
                let slot = &mut out.z[index];
                *slot = combine(component.combine, *slot, v);
            }
        }
    }
}

fn combine(mode: Combine, below: f32, v: f32) -> f32 {
    match mode {
        Combine::Add => below + v,
        Combine::Subtract => below - v,
        Combine::MergeHigh => below.max(v),
        Combine::MergeLow => below.min(v),
        Combine::Multiply => below * v,
    }
}

/// Bakes the model level by level, bottom level first: each visible
/// level's components bake together from the zero plane, then the level
/// meets what is below it with its own combine mode, only where its
/// components reach. A component whose level is 0 or unknown sits on the
/// first level.
pub fn bake_levels<'a, I>(levels: &[Level], components: I, grid: &Grid) -> Field
where
    I: IntoIterator<Item = &'a Component>,
{
    let components: Vec<&Component> = components.into_iter().collect();
    tree::bake_model(levels, &components, grid, |_| None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(
        id: u64,
        x0: usize,
        x1: usize,
        height: f32,
        combine: Combine,
        grid: &Grid,
    ) -> Component {
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for j in 0..grid.height {
            for i in x0..x1 {
                field.z[j * grid.width + i] = height;
            }
        }
        Component {
            id,
            name: format!("block {id}"),
            visible: true,
            combine,
            height_scale: 1.0,
            level: 0,
            extra: tree::ComponentExtra::default(),
            field: field.cropped().expect("cells"),
        }
    }

    fn grid() -> Grid {
        Grid {
            width: 10,
            height: 2,
            cell: 1.0,
            origin: Point::new(0.5, 0.5),
        }
    }

    fn at(field: &Field, i: usize) -> f32 {
        field.z[i]
    }

    #[test]
    fn combine_modes_meet_what_is_below() {
        let g = grid();
        let base = block(1, 0, 6, 4.0, Combine::Add, &g);
        for (combine, overlap, alone) in [
            (Combine::Add, 6.0, 2.0),
            (Combine::Subtract, 2.0, -2.0),
            (Combine::MergeHigh, 4.0, 2.0),
            (Combine::MergeLow, 2.0, 0.0),
        ] {
            let top = block(2, 4, 8, 2.0, combine, &g);
            let baked = bake([&base, &top], &g);
            assert_eq!(at(&baked, 1), 4.0, "{combine:?} below only");
            assert_eq!(at(&baked, 5), overlap, "{combine:?} overlap");
            assert_eq!(at(&baked, 7), alone, "{combine:?} top only");
            assert_eq!(at(&baked, 9), 0.0, "{combine:?} neither");
        }
    }

    #[test]
    fn hidden_components_and_height_scale() {
        let g = grid();
        let mut a = block(1, 0, 4, 4.0, Combine::Add, &g);
        a.height_scale = 0.5;
        let mut b = block(2, 0, 4, 9.0, Combine::Add, &g);
        b.visible = false;
        let baked = bake([&a, &b], &g);
        assert_eq!(at(&baked, 0), 2.0);
    }

    #[test]
    fn the_model_grid_covers_the_job() {
        let job = JobSetup::default();
        let g = grid_for(&job);
        assert!((g.width as f64 * g.cell - job.width).abs() < g.cell);
        assert!(((g.width * g.height) as f64 - MODEL_CELLS).abs() / MODEL_CELLS < 0.01);
    }

    #[test]
    fn finer_resolutions_give_more_cells() {
        use crate::doc::ModelResolution;
        let standard = JobSetup::default();
        for (resolution, factor) in [
            (ModelResolution::High, 3.0),
            (ModelResolution::Highest, 7.0),
        ] {
            let job = JobSetup {
                resolution,
                ..standard
            };
            let g = grid_for(&job);
            let wanted = MODEL_CELLS * factor;
            assert!(((g.width * g.height) as f64 - wanted).abs() / wanted < 0.01);
            assert!((g.width as f64 * g.cell - job.width).abs() < g.cell);
        }
    }

    #[test]
    fn heights_round_trip_through_bytes() {
        let g = grid();
        let c = block(1, 2, 5, 3.5, Combine::Add, &g);
        let mut copy = c.field.clone();
        copy.z.clear();
        copy.set_z_bytes(&c.field.z_bytes()).expect("sizes match");
        assert_eq!(copy, c.field);
        assert!(copy.set_z_bytes(&[0, 1]).is_err());
    }
}
