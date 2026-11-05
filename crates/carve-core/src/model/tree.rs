//! The component tree: groups of components inside a level, the per
//! component properties the tree shows (base height, colour, group), and
//! the level by level bake that honours groups and level clipping.
//!
//! A level bakes its components together from the zero plane, bottom
//! first. A group bakes its own members together from the zero plane at
//! the place of its lowest member, then meets what the level has so far
//! with the group's combine mode where its members reach. The level then
//! meets the levels below with its own combine mode where its components
//! reach, and only inside its clipping vectors when it has any.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::field::Field;
use super::shape::Grid;
use super::{bake_covering, bake_into, combine, Combine, Component, Level, MirrorMode};

/// Properties of a component that files written before the component tree
/// did not have. Every field defaults, so old files load unchanged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComponentExtra {
    /// Flat riser in millimetres added under every cell the component
    /// covers, after the height scale; the relief itself is not changed.
    pub base_height: f64,
    /// Preview colour as `#rrggbb`; `None` takes its parent's appearance.
    pub color: Option<String>,
    /// Name of the preview material `color` was taken from; none when the
    /// colour is a plain colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Id of the group it belongs to inside its level; 0 for none.
    pub group: u64,
    /// Id of the sheet it sits on. 0 (files written before components had
    /// a sheet) or an id no sheet has any more puts it on the first sheet.
    pub sheet: u64,
    /// How it was imported from a mesh file, for Reimport; none otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<Box<super::import::ImportSource>>,
    /// Dynamic fade between two anchor points; none when off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade: Option<super::dynamic::Fade>,
    /// Dynamic tilt about an anchor point; none when off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tilt: Option<super::dynamic::Tilt>,
    /// The Create Shape settings it was made with, so the shape tools can
    /// edit it again; none for every other component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<Box<super::shape::ShapeSource>>,
    /// The Create Texture Area settings of a texture area; none for every
    /// other component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<Box<super::texture_area::TextureSource>>,
}

/// Components inside a level that bake together and then meet the rest of
/// the level as one piece, and hide, show and move as one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Document wide id, from the same counter as levels; never reused.
    pub id: u64,
    /// Name in the component tree.
    pub name: String,
    /// Hidden groups do not bake.
    pub visible: bool,
    /// How the group meets what is below it in the level.
    pub combine: Combine,
    /// Open in the component tree.
    #[serde(default = "yes")]
    pub expanded: bool,
    /// Preview colour as `#rrggbb` for every member; `None` takes the
    /// colours of the members.
    #[serde(default)]
    pub color: Option<String>,
    /// Name of the preview material `color` was taken from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
}

fn yes() -> bool {
    true
}

/// Reads a `#rrggbb` colour.
///
/// # Errors
/// [`CoreError::Validation`] for anything else.
pub fn parse_color(text: &str) -> Result<[u8; 3], CoreError> {
    let bad = || CoreError::validation(format!("{text:?} is not a color like #a0522d"));
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map_err(|_| bad());
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

/// The group a component bakes with, when it names one of `groups`.
fn group_of<'a>(component: &Component, groups: &'a [Group]) -> Option<&'a Group> {
    if component.extra.group == 0 {
        return None;
    }
    groups.iter().find(|g| g.id == component.extra.group)
}

/// Bakes one level's components (bottom first) onto `grid` from the zero
/// plane, each group as one piece at the place of its lowest member.
/// Answers the heights and which cells anything reached.
pub fn bake_stack(components: &[&Component], groups: &[Group], grid: &Grid) -> (Field, Vec<bool>) {
    if !components.iter().any(|c| group_of(c, groups).is_some()) {
        return bake_covering(components.iter().copied(), grid);
    }
    let mut out = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    out.z.fill(0.0);
    let mut covered = vec![false; grid.width * grid.height];
    let mut done: Vec<u64> = Vec::new();
    for component in components {
        let Some(group) = group_of(component, groups) else {
            bake_into(&mut out, &mut covered, [*component], grid);
            continue;
        };
        if done.contains(&group.id) {
            continue;
        }
        done.push(group.id);
        if !group.visible {
            continue;
        }
        let members = components
            .iter()
            .copied()
            .filter(|c| c.extra.group == group.id);
        let (field, reached) = bake_covering(members, grid);
        for (index, slot) in out.z.iter_mut().enumerate() {
            if reached[index] {
                *slot = combine(group.combine, *slot, field.z[index]);
                covered[index] = true;
            }
        }
    }
    (out, covered)
}

/// Bakes the whole model level by level, bottom level first; see the
/// module note. `clip` answers, for a level, which grid cells it may reach
/// (its clipping vectors), or `None` for everywhere.
pub fn bake_model<F>(levels: &[Level], components: &[&Component], grid: &Grid, clip: F) -> Field
where
    F: Fn(&Level) -> Option<Vec<bool>>,
{
    let first = levels.first().map_or(0, |l| l.id);
    let level_of = |c: &Component| {
        if levels.iter().any(|l| l.id == c.level) {
            c.level
        } else {
            first
        }
    };
    if levels.is_empty() {
        return bake_stack(components, &[], grid).0;
    }
    let mut out = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    out.z.fill(0.0);
    for level in levels.iter().filter(|l| l.visible) {
        let members: Vec<&Component> = components
            .iter()
            .copied()
            .filter(|c| level_of(c) == level.id)
            .collect();
        if members.is_empty() {
            continue;
        }
        let (mut field, mut covered) = bake_stack(&members, &level.groups, grid);
        if let Some(mode) = level.mirror {
            mirror_cells(&mut field.z, grid.width, grid.height, mode);
            mirror_cells(&mut covered, grid.width, grid.height, mode);
        }
        if level.base_height != 0.0 {
            let base = level.base_height as f32;
            for (z, hit) in field.z.iter_mut().zip(&covered) {
                if *hit && !z.is_nan() {
                    *z += base;
                }
            }
        }
        let mask = if !level.is_clipped() {
            None
        } else {
            clip(level)
        };
        for (index, slot) in out.z.iter_mut().enumerate() {
            let inside = mask
                .as_ref()
                .is_none_or(|m| m.get(index).copied().unwrap_or(false));
            if covered[index] && inside {
                *slot = combine(level.combine, *slot, field.z[index]);
            }
        }
    }
    out
}

/// Mirrors a level's cells (row by row, bottom row first) about the middle
/// of the grid as `mode` says: the kept half or quarter is copied, turned
/// over, onto the rest, which it replaces.
pub fn mirror_cells<T: Copy>(cells: &mut [T], width: usize, height: usize, mode: MirrorMode) {
    use MirrorMode::{
        BottomLeft, BottomRight, BottomToTop, LeftToRight, RightToLeft, TopLeft, TopRight,
        TopToBottom,
    };
    if cells.len() < width * height {
        return;
    }
    // Keep the left (true) or right (false) half, the top (true) or bottom
    // (false) half, or leave that axis alone (None).
    let (keep_left, keep_top) = match mode {
        LeftToRight => (Some(true), None),
        RightToLeft => (Some(false), None),
        TopToBottom => (None, Some(true)),
        BottomToTop => (None, Some(false)),
        TopLeft => (Some(true), Some(true)),
        TopRight => (Some(false), Some(true)),
        BottomLeft => (Some(true), Some(false)),
        BottomRight => (Some(false), Some(false)),
    };
    if let Some(left) = keep_left {
        for j in 0..height {
            let row = j * width;
            for i in 0..width / 2 {
                let (kept, other) = if left {
                    (i, width - 1 - i)
                } else {
                    (width - 1 - i, i)
                };
                cells[row + other] = cells[row + kept];
            }
        }
    }
    if let Some(top) = keep_top {
        for j in 0..height / 2 {
            let (kept, other) = if top {
                (height - 1 - j, j)
            } else {
                (j, height - 1 - j)
            };
            for i in 0..width {
                cells[other * width + i] = cells[kept * width + i];
            }
        }
    }
}

/// Preview colour per grid cell, bottom row first, as RGBA bytes: the
/// colour of the topmost visible component reaching the cell (its level's
/// or group's colour when that has one), alpha 0 where that component has no
/// colour or nothing reaches.
pub fn color_map<F>(levels: &[Level], components: &[&Component], grid: &Grid, clip: F) -> Vec<u8>
where
    F: Fn(&Level) -> Option<Vec<bool>>,
{
    appearance_map(levels, components, grid, clip).rgba
}

/// Preview appearance per grid cell: the colours of [`color_map`] and the
/// preview material each was taken from.
#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceMap {
    /// RGBA bytes per cell, bottom row first, as [`color_map`] answers.
    pub rgba: Vec<u8>,
    /// Per cell, 0 for a plain colour or none, else one more than the
    /// index in `materials` of the preview material its colour came from.
    pub material: Vec<u16>,
    /// The preview material names the cells use, in first use order.
    pub materials: Vec<String>,
}

/// The colour and preview material of every cell; see [`color_map`].
pub fn appearance_map<F>(
    levels: &[Level],
    components: &[&Component],
    grid: &Grid,
    clip: F,
) -> AppearanceMap
where
    F: Fn(&Level) -> Option<Vec<bool>>,
{
    let cells = grid.width * grid.height;
    let mut out = AppearanceMap {
        rgba: vec![0u8; cells * 4],
        material: vec![0u16; cells],
        materials: Vec::new(),
    };
    let first = levels.first().map_or(0, |l| l.id);
    let level_of = |c: &Component| {
        if levels.iter().any(|l| l.id == c.level) {
            c.level
        } else {
            first
        }
    };
    let plain: Vec<Level> = if levels.is_empty() {
        vec![Level::first()]
    } else {
        levels.to_vec()
    };
    for level in plain.iter().filter(|l| l.visible) {
        let mask = if !level.is_clipped() {
            None
        } else {
            clip(level)
        };
        for component in components
            .iter()
            .filter(|c| (levels.is_empty() || level_of(c) == level.id) && c.visible)
        {
            let group = group_of(component, &level.groups);
            if group.is_some_and(|g| !g.visible) {
                continue;
            }
            // A colour on the level or group covers its members' own, and
            // the material it was taken from goes with it.
            let (colour, material) = if level.color.is_some() {
                (level.color.as_deref(), level.material.as_deref())
            } else if let Some(g) = group.filter(|g| g.color.is_some()) {
                (g.color.as_deref(), g.material.as_deref())
            } else {
                (
                    component.extra.color.as_deref(),
                    component.extra.material.as_deref(),
                )
            };
            let colour = colour.and_then(|c| parse_color(c).ok());
            let slot = match (colour, material) {
                (Some(_), Some(name)) => match out.materials.iter().position(|m| m == name) {
                    Some(at) => at + 1,
                    None => {
                        out.materials.push(name.to_string());
                        out.materials.len()
                    }
                },
                _ => 0,
            };
            let slot = u16::try_from(slot).unwrap_or(0);
            let (_, mut reached) = bake_covering([*component], grid);
            if let Some(mode) = level.mirror {
                mirror_cells(&mut reached, grid.width, grid.height, mode);
            }
            for (index, hit) in reached.iter().enumerate() {
                let inside = mask
                    .as_ref()
                    .is_none_or(|m| m.get(index).copied().unwrap_or(false));
                if !(*hit && inside) {
                    continue;
                }
                let px = &mut out.rgba[index * 4..index * 4 + 4];
                match colour {
                    Some([r, g, b]) => px.copy_from_slice(&[r, g, b, 255]),
                    None => px.copy_from_slice(&[0, 0, 0, 0]),
                }
                out.material[index] = slot;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn grid() -> Grid {
        Grid {
            width: 10,
            height: 1,
            cell: 1.0,
            origin: Point::new(0.5, 0.5),
        }
    }

    fn block(id: u64, x0: usize, x1: usize, height: f32, combine: Combine) -> Component {
        let g = grid();
        let mut field = Field::empty(g.width, g.height, g.cell, g.origin);
        for i in x0..x1 {
            field.z[i] = height;
        }
        Component {
            id,
            name: format!("block {id}"),
            visible: true,
            combine,
            height_scale: 1.0,
            level: 1,
            extra: ComponentExtra::default(),
            field: field.cropped().expect("cells"),
        }
    }

    fn group(id: u64, combine: Combine) -> Group {
        Group {
            id,
            name: format!("Group {id}"),
            visible: true,
            combine,
            expanded: true,
            color: None,
            material: None,
        }
    }

    fn level(id: u64, combine: Combine) -> Level {
        Level {
            id,
            name: format!("Level {id}"),
            visible: true,
            combine,
            groups: Vec::new(),
            clip: Vec::new(),
            clip_shape: Vec::new(),
            color: None,
            material: None,
            mirror: None,
            base_height: 0.0,
        }
    }

    fn no_clip(_: &Level) -> Option<Vec<bool>> {
        None
    }

    #[test]
    fn multiply_scales_what_is_below() {
        let g = grid();
        let base = block(1, 0, 6, 4.0, Combine::Add);
        let times = block(2, 4, 8, 0.5, Combine::Multiply);
        let baked = super::super::bake([&base, &times], &g);
        assert_eq!(baked.z[1], 4.0, "below only");
        assert_eq!(baked.z[5], 2.0, "overlap halves");
        assert_eq!(baked.z[7], 0.0, "nothing below stays at zero");
    }

    #[test]
    fn base_height_lifts_the_whole_footprint_after_the_scale() {
        let g = grid();
        let mut c = block(1, 2, 5, 3.0, Combine::Add);
        c.height_scale = 2.0;
        c.extra.base_height = 1.5;
        let baked = super::super::bake([&c], &g);
        assert_eq!(baked.z[3], 7.5);
        assert_eq!(baked.z[0], 0.0, "outside the footprint stays at zero");
    }

    #[test]
    fn a_group_meets_the_level_as_one_piece() {
        // A group of two blocks subtracted from a slab: the group first
        // adds its members together, then the sum is taken away.
        let g = grid();
        let slab = block(1, 0, 10, 10.0, Combine::Add);
        let mut a = block(2, 2, 6, 2.0, Combine::Add);
        let mut b = block(3, 4, 8, 3.0, Combine::Add);
        a.extra.group = 50;
        b.extra.group = 50;
        let groups = [group(50, Combine::Subtract)];
        let (baked, covered) = bake_stack(&[&slab, &a, &b], &groups, &g);
        assert_eq!(baked.z[0], 10.0);
        assert_eq!(baked.z[3], 8.0, "a alone");
        assert_eq!(baked.z[5], 5.0, "a and b add, then subtract");
        assert_eq!(baked.z[7], 7.0, "b alone");
        assert!(covered.iter().all(|c| *c));
    }

    #[test]
    fn a_hidden_group_hides_its_members() {
        let g = grid();
        let mut a = block(2, 2, 6, 2.0, Combine::Add);
        a.extra.group = 50;
        let mut hidden = group(50, Combine::Add);
        hidden.visible = false;
        let (baked, covered) = bake_stack(&[&a], &[hidden], &g);
        assert_eq!(baked.z[3], 0.0);
        assert!(!covered[3]);
    }

    #[test]
    fn ungrouped_members_combine_as_before() {
        // With a group present, the other components still meet the level
        // with their own modes, including the ones that lose their height
        // against the zero plane.
        let g = grid();
        let slab = block(1, 0, 10, 4.0, Combine::Add);
        let mut grouped = block(2, 0, 1, 1.0, Combine::Add);
        grouped.extra.group = 9;
        let groups = [group(9, Combine::Add)];
        for (mode, expect) in [
            (Combine::Subtract, 3.0),
            (Combine::MergeHigh, 4.0),
            (Combine::MergeLow, 1.0),
            (Combine::Multiply, 4.0),
        ] {
            let top = block(3, 4, 6, 1.0, mode);
            let (baked, _) = bake_stack(&[&slab, &grouped, &top], &groups, &g);
            let plain = super::super::bake([&slab, &top], &g);
            assert_eq!(baked.z[5], expect, "{mode:?}");
            assert_eq!(baked.z[5], plain.z[5], "{mode:?} matches the plain bake");
        }
    }

    #[test]
    fn levels_combine_and_clip() {
        let g = grid();
        let slab = block(1, 0, 10, 5.0, Combine::Add);
        let mut cut = block(2, 0, 10, 2.0, Combine::Add);
        cut.level = 2;
        let mut upper = level(2, Combine::Subtract);
        upper.clip = vec![77];
        let levels = [level(1, Combine::Add), upper];
        // The clip lets the upper level reach only the first half.
        let clip = |l: &Level| (l.id == 2).then(|| (0..10).map(|i| i < 5).collect());
        let baked = bake_model(&levels, &[&slab, &cut], &g, clip);
        assert_eq!(baked.z[1], 3.0);
        assert_eq!(baked.z[8], 5.0);
        let unclipped = bake_model(&levels, &[&slab, &cut], &g, no_clip);
        assert_eq!(unclipped.z[8], 3.0, "no mask means everywhere");
    }

    #[test]
    fn mirror_cells_copy_the_kept_half_or_quarter_over() {
        // 4 wide, 2 high, bottom row first.
        let cells = [1, 2, 3, 4, 5, 6, 7, 8];
        let run = |mode| {
            let mut c = cells;
            mirror_cells(&mut c, 4, 2, mode);
            c
        };
        assert_eq!(run(MirrorMode::LeftToRight), [1, 2, 2, 1, 5, 6, 6, 5]);
        assert_eq!(run(MirrorMode::RightToLeft), [4, 3, 3, 4, 8, 7, 7, 8]);
        assert_eq!(run(MirrorMode::TopToBottom), [5, 6, 7, 8, 5, 6, 7, 8]);
        assert_eq!(run(MirrorMode::BottomToTop), [1, 2, 3, 4, 1, 2, 3, 4]);
        assert_eq!(run(MirrorMode::TopLeft), [5, 6, 6, 5, 5, 6, 6, 5]);
        assert_eq!(run(MirrorMode::BottomRight), [4, 3, 3, 4, 4, 3, 3, 4]);
        // An odd width keeps its middle column.
        let mut odd = [1, 2, 3];
        mirror_cells(&mut odd, 3, 1, MirrorMode::LeftToRight);
        assert_eq!(odd, [1, 2, 1]);
    }

    #[test]
    fn a_mirrored_level_shows_its_half_on_both_sides() {
        let g = grid();
        let a = block(1, 1, 3, 4.0, Combine::Add);
        let mut levels = [level(1, Combine::Add)];
        levels[0].mirror = Some(MirrorMode::LeftToRight);
        let baked = bake_model(&levels, &[&a], &g, no_clip);
        assert_eq!(
            &baked.z[..],
            &[0.0, 4.0, 4.0, 0.0, 0.0, 0.0, 0.0, 4.0, 4.0, 0.0]
        );
        let map = color_map(&levels, &[&a], &g, no_clip);
        assert_eq!(map.len(), 40);
        levels[0].mirror = Some(MirrorMode::RightToLeft);
        let baked = bake_model(&levels, &[&a], &g, no_clip);
        assert!(
            baked.z.iter().all(|&z| z == 0.0),
            "the kept right half is empty"
        );
    }

    #[test]
    fn one_plain_level_matches_the_plain_bake() {
        let g = grid();
        let a = block(1, 0, 6, 4.0, Combine::Add);
        let b = block(2, 3, 9, 2.0, Combine::MergeLow);
        let levels = [level(1, Combine::Add)];
        let baked = bake_model(&levels, &[&a, &b], &g, no_clip);
        assert_eq!(baked, super::super::bake([&a, &b], &g));
    }

    #[test]
    fn colours_follow_the_topmost_component_and_group() {
        let g = grid();
        let mut a = block(1, 0, 6, 4.0, Combine::Add);
        a.extra.color = Some("#ff0000".to_string());
        let mut b = block(2, 4, 8, 1.0, Combine::Add);
        b.extra.color = Some("#00ff00".to_string());
        b.extra.group = 9;
        let mut tinted = group(9, Combine::Add);
        let mut levels = [level(1, Combine::Add)];
        levels[0].groups = vec![tinted.clone()];
        let map = color_map(&levels, &[&a, &b], &g, no_clip);
        assert_eq!(&map[0..4], &[255, 0, 0, 255]);
        assert_eq!(&map[5 * 4..5 * 4 + 4], &[0, 255, 0, 255]);
        assert_eq!(map[9 * 4 + 3], 0, "nothing reaches the last cell");
        tinted.color = Some("#0000ff".to_string());
        levels[0].groups = vec![tinted];
        let map = color_map(&levels, &[&a, &b], &g, no_clip);
        assert_eq!(
            &map[5 * 4..5 * 4 + 4],
            &[0, 0, 255, 255],
            "the group's colour wins"
        );
    }

    #[test]
    fn each_cell_knows_the_material_its_colour_came_from() {
        let g = grid();
        let mut a = block(1, 0, 6, 4.0, Combine::Add);
        a.extra.color = Some("#b98c57".to_string());
        a.extra.material = Some("oak".to_string());
        let mut b = block(2, 4, 8, 1.0, Combine::Add);
        b.extra.color = Some("#00ff00".to_string());
        let mut levels = [level(1, Combine::Add)];
        let map = appearance_map(&levels, &[&a, &b], &g, no_clip);
        assert_eq!(map.materials, vec!["oak".to_string()]);
        assert_eq!(map.material[0], 1);
        assert_eq!(map.material[5], 0, "a plain colour has no material");
        assert_eq!(map.material[9], 0);
        levels[0].color = Some("#6a4631".to_string());
        levels[0].material = Some("walnut".to_string());
        let map = appearance_map(&levels, &[&a, &b], &g, no_clip);
        assert_eq!(map.materials, vec!["walnut".to_string()]);
        assert!(map.material[..8].iter().all(|m| *m == 1));
        assert_eq!(&map.rgba[0..4], &[0x6a, 0x46, 0x31, 255]);
    }

    #[test]
    fn colours_parse_strictly() {
        assert_eq!(parse_color("#a0522d").ok(), Some([0xa0, 0x52, 0x2d]));
        assert!(parse_color("a0522d").is_err());
        assert!(parse_color("#12345").is_err());
        assert!(parse_color("#12345g").is_err());
    }

    #[test]
    fn old_components_and_levels_load_with_defaults() {
        let c = block(1, 0, 2, 1.0, Combine::Add);
        let mut json = serde_json::to_value(&c).expect("writes");
        let object = json.as_object_mut().expect("an object");
        for key in ["baseHeight", "color", "group", "sheet"] {
            assert!(object.remove(key).is_some(), "{key} is written flat");
        }
        let old: Component = serde_json::from_value(json).expect("an old component parses");
        assert_eq!(old.extra, ComponentExtra::default());
        let l: Level =
            serde_json::from_str(r#"{"id":1,"name":"L","visible":true,"combine":"add"}"#)
                .expect("an old level parses");
        assert!(l.groups.is_empty() && l.clip.is_empty());
        let mut l = l;
        l.groups.push(group(4, Combine::Multiply));
        l.clip.push(12);
        let round: Level =
            serde_json::from_str(&serde_json::to_string(&l).expect("writes")).expect("reads");
        assert_eq!(round, l);
    }
}
