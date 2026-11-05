//! The document's relief model components, bottom of the list first, with
//! undo like every other part of the document.

use std::collections::BTreeSet;

use crate::error::CoreError;
use crate::geom::{Mat3, Path, Region, Tolerance};
use crate::model::field::Field;
use crate::model::shape::{inside_cells, Grid};
use crate::model::tree::{bake_model, color_map, Group};
use crate::model::{grid_for, Combine, Component, Level};

use super::{Change, Document};

/// A highlighted cell where the chosen component shows.
pub const HIGHLIGHT_SEEN: u8 = 1;
/// A highlighted cell where other components hide the chosen one.
pub const HIGHLIGHT_HIDDEN: u8 = 2;

impl Document {
    /// Components on the active sheet, bottom of the list first. The model,
    /// the component tree and 3D toolpaths all work on these.
    pub fn components(&self) -> Vec<&Component> {
        self.all_components()
            .into_iter()
            .filter(|c| self.sheet_of_component(c) == self.active_sheet)
            .collect()
    }

    /// Components on every sheet, bottom of the list first.
    pub fn all_components(&self) -> Vec<&Component> {
        self.component_order
            .iter()
            .filter_map(|id| self.components.get(id))
            .collect()
    }

    /// The sheet a component sits on: its own when that sheet exists, the
    /// first sheet otherwise (components from files written before
    /// components had a sheet).
    pub fn sheet_of_component(&self, component: &Component) -> u64 {
        let own = component.extra.sheet;
        if own != 0 && self.sheets.iter().any(|s| s.id == own) {
            return own;
        }
        self.sheets.first().map_or(own, |s| s.id)
    }

    /// One component.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn component(&self, id: u64) -> Result<&Component, CoreError> {
        self.components
            .get(&id)
            .ok_or_else(|| CoreError::validation(format!("no component with id {id}")))
    }

    /// Adds a component on top of the list, giving it the next id. A
    /// component with no known level goes on the active level, and one with
    /// no known sheet on the active sheet.
    pub fn add_component(&mut self, mut component: Component) -> u64 {
        if !self.levels.iter().any(|l| l.id == component.level) {
            component.level = self.active_level;
        }
        if !self.sheets.iter().any(|s| s.id == component.extra.sheet) {
            component.extra.sheet = self.active_sheet;
        }
        self.capture_counters();
        self.capture_component_order();
        let id = self.next_component_id;
        self.next_component_id += 1;
        component.id = id;
        self.capture_component(id);
        self.components.insert(id, component);
        self.component_order.push(id);
        id
    }

    /// Edits a component in place.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn update_component<F>(&mut self, id: u64, edit: F) -> Result<(), CoreError>
    where
        F: FnOnce(&mut Component),
    {
        self.component(id)?;
        self.capture_component(id);
        if let Some(component) = self.components.get_mut(&id) {
            edit(component);
        }
        Ok(())
    }

    /// The vectors `moved` (leaf ids) were moved by `m` without the
    /// components: a shape made from vectors that all moved stays where it
    /// lies, its placement taking the move back, so making it again from
    /// them does not shift it. A component moved in the same step records
    /// its own move afterwards (`ShapeSource::moved` without the vectors),
    /// which leaves it lying on them as before. Vectors that only partly
    /// moved, or were reshaped, change the shape they make as drawn.
    pub(crate) fn shapes_left_by(&mut self, moved: &BTreeSet<u64>, m: &Mat3) {
        if moved.is_empty() {
            return;
        }
        let left: Vec<u64> = self
            .component_order
            .iter()
            .copied()
            .filter(|id| {
                self.components
                    .get(id)
                    .and_then(|c| c.extra.shape.as_deref())
                    .is_some_and(|shape| {
                        let own: BTreeSet<u64> =
                            shape.vectors.iter().flat_map(|v| self.leaves(*v)).collect();
                        !own.is_empty() && own.is_subset(moved)
                    })
            })
            .collect();
        for id in left {
            self.capture_component(id);
            if let Some(shape) = self
                .components
                .get_mut(&id)
                .and_then(|c| c.extra.shape.as_deref_mut())
            {
                shape.vectors_moved(m);
            }
        }
    }

    /// Removes components. Unknown ids are refused before anything changes.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn delete_components(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.component(*id)?;
        }
        self.capture_component_order();
        for id in ids {
            self.capture_component(*id);
            self.components.remove(id);
        }
        self.component_order.retain(|id| !ids.contains(id));
        Ok(())
    }

    /// Puts the component list in the given order, bottom first. Ids left
    /// out keep their order after the ones given.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn reorder_components(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.component(*id)?;
        }
        self.capture_component_order();
        let mut order: Vec<u64> = Vec::with_capacity(self.component_order.len());
        for id in ids.iter().chain(self.component_order.iter()) {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        self.component_order = order;
        Ok(())
    }

    /// The visible components baked onto the model grid for the job, with
    /// groups and level clipping.
    pub fn composite(&self) -> Field {
        let grid = grid_for(&self.job);
        bake_model(&self.levels, &self.components(), &grid, |l| {
            self.clip_mask(l, &grid)
        })
    }

    /// The chosen visible components alone baked onto the model grid, as
    /// they would show with every other component hidden.
    pub fn composite_of(&self, ids: &[u64]) -> Field {
        let grid = grid_for(&self.job);
        let chosen: Vec<&Component> = self
            .components()
            .into_iter()
            .filter(|c| ids.contains(&c.id))
            .collect();
        bake_model(&self.levels, &chosen, &grid, |l| self.clip_mask(l, &grid))
    }

    /// The visible components on `sheet` baked onto that sheet's model grid
    /// (its own material size), with groups and level clipping: the sheet's
    /// raw model whether or not it is the active sheet.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown sheet.
    pub fn sheet_composite(&self, sheet: u64) -> Result<Field, CoreError> {
        self.sheet_composite_except(sheet, &[])
    }

    /// As [`Document::sheet_composite`] without the components `except`
    /// (the ones the Assembly View shows as pieces of their own).
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown sheet.
    pub fn sheet_composite_except(&self, sheet: u64, except: &[u64]) -> Result<Field, CoreError> {
        let job = self.sheet_material(sheet)?.applied_to(&self.job);
        let grid = grid_for(&job);
        let on_sheet: Vec<&Component> = self
            .all_components()
            .into_iter()
            .filter(|c| self.sheet_of_component(c) == sheet && !except.contains(&c.id))
            .collect();
        Ok(bake_model(&self.levels, &on_sheet, &grid, |l| {
            self.clip_mask(l, &grid)
        }))
    }

    /// One component baked alone on its sheet's model grid, as it would
    /// stand by itself: shown even when hidden, its heights added to the
    /// zero plane whatever its combine mode, no level clipping. Returns
    /// the sheet and the field.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown component.
    pub fn component_alone(&self, id: u64) -> Result<(u64, Field), CoreError> {
        let component = self.component(id)?;
        let sheet = self.sheet_of_component(component);
        let job = self.sheet_material(sheet)?.applied_to(&self.job);
        let grid = grid_for(&job);
        let mut alone = component.clone();
        alone.visible = true;
        alone.combine = crate::model::Combine::Add;
        Ok((sheet, crate::model::bake(std::iter::once(&alone), &grid)))
    }

    /// As [`Document::model_fingerprint`] for one component alone on its
    /// sheet's grid (see [`Document::component_alone`]); 0 for an unknown
    /// component.
    pub fn component_fingerprint(&self, id: u64) -> u64 {
        let Ok(component) = self.component(id) else {
            return 0;
        };
        let sheet = self.sheet_of_component(component);
        let mut shown = component.clone();
        shown.visible = true;
        let mut hash = self.fingerprint_of(&[&shown]);
        let mut mix = |value: u64| hash = (hash ^ value).wrapping_mul(0x0100_0000_01b3);
        if let Ok(material) = self.sheet_material(sheet) {
            let job = material.applied_to(&self.job);
            for value in [job.width, job.height, job.thickness] {
                mix(value.to_bits());
            }
        }
        mix(sheet);
        hash
    }

    /// How the chosen components show in the model, one byte per model
    /// cell, bottom row first: [`HIGHLIGHT_SEEN`] where a component reaches
    /// the cell and changes the model there, [`HIGHLIGHT_HIDDEN`] where it
    /// reaches the cell but other components hide it, 0 elsewhere. Hidden
    /// components and unknown ids mark nothing.
    pub fn highlight_mask(&self, ids: &[u64]) -> (Grid, Vec<u8>) {
        let grid = grid_for(&self.job);
        let mut mask = vec![0u8; grid.width * grid.height];
        let chosen: Vec<&Component> = self
            .components()
            .into_iter()
            .filter(|c| c.visible && ids.contains(&c.id))
            .collect();
        if chosen.is_empty() {
            return (grid, mask);
        }
        let full = self.composite();
        let others: Vec<&Component> = self
            .components()
            .into_iter()
            .filter(|c| !ids.contains(&c.id))
            .collect();
        let without = bake_model(&self.levels, &others, &grid, |l| self.clip_mask(l, &grid));
        for row in 0..grid.height {
            let y = grid.origin.y + row as f64 * grid.cell;
            for col in 0..grid.width {
                let at = crate::geom::Point::new(grid.origin.x + col as f64 * grid.cell, y);
                if !chosen.iter().any(|c| !c.field.at(at).is_nan()) {
                    continue;
                }
                let index = row * grid.width + col;
                let a = full.z.get(index).copied().unwrap_or(0.0);
                let b = without.z.get(index).copied().unwrap_or(0.0);
                let changed = if a.is_nan() || b.is_nan() {
                    a.is_nan() != b.is_nan()
                } else {
                    (a - b).abs() > 1e-4
                };
                mask[index] = if changed {
                    HIGHLIGHT_SEEN
                } else {
                    HIGHLIGHT_HIDDEN
                };
            }
        }
        (grid, mask)
    }

    /// Preview colours of the composite's cells as RGBA bytes; see
    /// [`color_map`].
    pub fn composite_colors(&self) -> Vec<u8> {
        let grid = grid_for(&self.job);
        color_map(&self.levels, &self.components(), &grid, |l| {
            self.clip_mask(l, &grid)
        })
    }

    /// The colour and preview material of every model cell; see
    /// [`crate::model::tree::appearance_map`].
    pub fn composite_appearance(&self) -> crate::model::tree::AppearanceMap {
        let grid = grid_for(&self.job);
        crate::model::tree::appearance_map(&self.levels, &self.components(), &grid, |l| {
            self.clip_mask(l, &grid)
        })
    }

    /// The cells inside a level's clipping boundary, or `None` when it
    /// encloses nothing. The boundary is the shape kept when the clipping
    /// was applied; levels from older files without one read their
    /// clipping vectors as they are now.
    fn clip_mask(&self, level: &Level, grid: &Grid) -> Option<Vec<bool>> {
        let paths: Vec<Path> = if level.clip_shape.is_empty() {
            self.clip_paths(&level.clip)
        } else {
            level.clip_shape.clone()
        };
        let tol = Tolerance::new(0.01).ok()?;
        let region = Region::from_paths(&paths, tol);
        if region.is_empty() {
            return None;
        }
        Some(inside_cells(&region, grid, tol))
    }

    /// The paths of the given vectors that are still in the document.
    pub fn clip_paths(&self, ids: &[u64]) -> Vec<Path> {
        ids.iter()
            .filter(|id| self.entity(**id).is_ok())
            .flat_map(|id| self.paths_of(*id).into_iter().map(|(_, p)| p))
            .collect()
    }

    /// Adds a group to a level, taking its id from the level counter.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown level.
    pub fn add_group(&mut self, level: u64, mut group: Group) -> Result<u64, CoreError> {
        let index = self.level_index(level)?;
        self.capture_levels();
        group.id = self.next_level_id;
        self.next_level_id += 1;
        let id = group.id;
        self.levels[index].groups.push(group);
        Ok(id)
    }

    /// A fingerprint of the visible model: the same components with the
    /// same settings and heights give the same number, in any session and
    /// any build (FNV-1a, not the standard hasher).
    pub fn model_fingerprint(&self) -> u64 {
        self.fingerprint_of(&self.components())
    }

    /// As [`Document::model_fingerprint`] for the model of `sheet`, which
    /// need not be the active sheet; the sheet's material size is part of
    /// it, as the model grid follows it.
    pub fn sheet_model_fingerprint(&self, sheet: u64) -> u64 {
        let on_sheet: Vec<&Component> = self
            .all_components()
            .into_iter()
            .filter(|c| self.sheet_of_component(c) == sheet)
            .collect();
        let mut hash = self.fingerprint_of(&on_sheet);
        if let Ok(material) = self.sheet_material(sheet) {
            let job = material.applied_to(&self.job);
            for value in [job.width, job.height, job.thickness] {
                hash = (hash ^ value.to_bits()).wrapping_mul(0x0100_0000_01b3);
            }
            hash = (hash ^ sheet).wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    fn fingerprint_of(&self, components: &[&Component]) -> u64 {
        const PRIME: u64 = 0x0100_0000_01b3;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(PRIME);
            }
        };
        for level in &self.levels {
            eat(&level.id.to_le_bytes());
            eat(&[u8::from(level.visible), level.combine as u8]);
            for group in &level.groups {
                eat(&group.id.to_le_bytes());
                eat(&[u8::from(group.visible), group.combine as u8]);
            }
            if level.clip_shape.is_empty() {
                for id in &level.clip {
                    eat(&id.to_le_bytes());
                    let revision = self.entity(*id).map_or(0, |e| e.revision);
                    eat(&revision.to_le_bytes());
                }
            } else if let Ok(shape) = serde_json::to_vec(&level.clip_shape) {
                eat(&shape);
            }
        }
        for component in components {
            if !component.visible {
                continue;
            }
            eat(&component.id.to_le_bytes());
            eat(&component.level.to_le_bytes());
            eat(&[component.combine as u8]);
            eat(&component.height_scale.to_le_bytes());
            eat(&component.extra.base_height.to_le_bytes());
            eat(&component.extra.group.to_le_bytes());
            let f = &component.field;
            eat(&(f.width as u64).to_le_bytes());
            eat(&(f.height as u64).to_le_bytes());
            eat(&f.cell.to_le_bytes());
            eat(&f.origin.x.to_le_bytes());
            eat(&f.origin.y.to_le_bytes());
            eat(&f.z_bytes());
        }
        hash
    }

    /// Changes whenever the model does; see the field's note.
    pub fn model_stamp(&self) -> u64 {
        self.model_stamp
    }

    /// Model levels, bottom first.
    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    /// The level new components go on.
    pub fn active_level(&self) -> u64 {
        self.active_level
    }

    fn level_index(&self, id: u64) -> Result<usize, CoreError> {
        self.levels
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| CoreError::validation(format!("no level with id {id}")))
    }

    /// Adds a level on top of the others and makes it active.
    pub fn add_level(&mut self, name: Option<String>, combine: Combine) -> u64 {
        self.capture_levels();
        let id = self.next_level_id;
        self.next_level_id += 1;
        let name = name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("Level {}", self.levels.len() + 1));
        self.levels.push(Level {
            id,
            name,
            visible: true,
            combine,
            groups: Vec::new(),
            clip: Vec::new(),
            clip_shape: Vec::new(),
            color: None,
            material: None,
            mirror: None,
            base_height: 0.0,
        });
        self.active_level = id;
        id
    }

    /// Edits a level in place.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn update_level<F>(&mut self, id: u64, edit: F) -> Result<(), CoreError>
    where
        F: FnOnce(&mut Level),
    {
        let index = self.level_index(id)?;
        self.capture_levels();
        edit(&mut self.levels[index]);
        Ok(())
    }

    /// Makes a level the one new components go on.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn set_active_level(&mut self, id: u64) -> Result<(), CoreError> {
        self.level_index(id)?;
        self.capture_levels();
        self.active_level = id;
        Ok(())
    }

    /// Removes a level; its components move down to the level below it (or
    /// up to the next one when it was the bottom level).
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id or the last level.
    pub fn delete_level(&mut self, id: u64) -> Result<(), CoreError> {
        let index = self.level_index(id)?;
        if self.levels.len() == 1 {
            return Err(CoreError::validation("the model needs at least one level"));
        }
        let heir = if index == 0 {
            self.levels[1].id
        } else {
            self.levels[index - 1].id
        };
        let moving: Vec<u64> = self
            .components
            .values()
            .filter(|c| c.level == id)
            .map(|c| c.id)
            .collect();
        for component in moving {
            self.update_component(component, |c| c.level = heir)?;
        }
        self.capture_levels();
        self.levels.remove(index);
        if self.active_level == id {
            self.active_level = heir;
        }
        Ok(())
    }

    /// Puts the levels in the given order, bottom first. Ids left out keep
    /// their order after the ones given.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn reorder_levels(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.level_index(*id)?;
        }
        self.capture_levels();
        let mut order: Vec<Level> = Vec::with_capacity(self.levels.len());
        for id in ids.iter().chain(self.levels.iter().map(|l| &l.id)) {
            if !order.iter().any(|l| l.id == *id) {
                if let Some(level) = self.levels.iter().find(|l| l.id == *id) {
                    order.push(level.clone());
                }
            }
        }
        self.levels = order;
        Ok(())
    }

    fn capture_levels(&mut self) {
        self.model_stamp += 1;
        if self.journal.is_recording() {
            self.journal.record(Change::Levels {
                levels: self.levels.clone(),
                active: self.active_level,
                next: self.next_level_id,
            });
        }
    }

    fn capture_component(&mut self, id: u64) {
        self.model_stamp += 1;
        if self.journal.is_recording() {
            let before = self.components.get(&id).cloned().map(Box::new);
            self.journal.record(Change::Component { id, before });
        }
    }

    fn capture_component_order(&mut self) {
        self.model_stamp += 1;
        if self.journal.is_recording() {
            let order = self.component_order.clone();
            self.journal.record(Change::ComponentOrder(order));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::doc::Document;
    use crate::geom::Point;
    use crate::model::field::Field;
    use crate::model::{Combine, Component};

    fn component(name: &str) -> Component {
        let mut field = Field::empty(2, 2, 1.0, Point::new(10.0, 10.0));
        field.z.fill(3.0);
        Component {
            id: 0,
            name: name.to_string(),
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        }
    }

    #[test]
    fn components_are_added_edited_and_undone() {
        let mut doc = Document::new();
        doc.begin("Create Shape");
        let a = doc.add_component(component("a"));
        doc.commit();
        doc.begin("Edit");
        doc.update_component(a, |c| c.height_scale = 2.0)
            .expect("edits");
        doc.commit();
        assert_eq!(doc.component(a).map(|c| c.height_scale).ok(), Some(2.0));
        doc.undo();
        assert_eq!(doc.component(a).map(|c| c.height_scale).ok(), Some(1.0));
        doc.undo();
        assert!(doc.components().is_empty());
        doc.redo();
        assert_eq!(doc.components().len(), 1);
    }

    #[test]
    fn the_composite_holds_the_visible_components() {
        let mut doc = Document::new();
        doc.begin("Create Shape");
        doc.add_component(component("a"));
        doc.commit();
        let composite = doc.composite();
        assert_eq!(composite.at(Point::new(10.0, 10.0)), 3.0);
        assert_eq!(composite.at(Point::new(100.0, 100.0)), 0.0);
    }

    #[test]
    fn delete_and_reorder_check_their_ids() {
        let mut doc = Document::new();
        doc.begin("Add");
        let a = doc.add_component(component("a"));
        let b = doc.add_component(component("b"));
        doc.commit();
        doc.begin("Reorder");
        doc.reorder_components(&[b]).expect("reorders");
        doc.commit();
        assert_eq!(doc.components()[0].name, "b");
        assert!(doc.delete_components(&[a, 99]).is_err());
        assert_eq!(doc.components().len(), 2, "nothing removed on a bad id");
    }
}
