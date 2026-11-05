//! The two sides of a double sided job.
//!
//! Each side owns its layers, sheets, vectors, selection and relief model.
//! The active side's content lives in the document's usual fields, so every
//! command works on the active side without knowing about sides; the other
//! side's content waits in a [`SideState`] and the two are exchanged when the
//! user switches sides. Toolpaths stay in one list and carry the side they
//! were calculated on, because simulation and output need both sides.
//! Coordinates on each side are that side's own job space; see
//! [`JobSetup::flip_xy`](super::JobSetup::flip_xy) for the mapping.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::model::{Component, Level};

use super::entity::Entity;
use super::job::Side;
use super::layer::{Layer, Sheet};
use super::{Change, Document, FIRST_LAYER, FIRST_SHEET};

/// The content of the side that is not active.
#[derive(Debug, Clone, PartialEq)]
pub struct SideState {
    layers: Vec<Layer>,
    sheets: Vec<Sheet>,
    entities: BTreeMap<u64, Entity>,
    order: Vec<u64>,
    selection: Vec<u64>,
    active_layer: u64,
    active_sheet: u64,
    components: BTreeMap<u64, Component>,
    component_order: Vec<u64>,
    levels: Vec<Level>,
    active_level: u64,
}

/// One side's content as written to `document.json`. The top side's content
/// is the file's top level fields; the bottom side's sits in `bottom`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideFile {
    /// Layers in drawing order.
    pub layers: Vec<Layer>,
    /// Sheets in tab order.
    pub sheets: Vec<Sheet>,
    /// Entities, including group members.
    pub entities: Vec<Entity>,
    /// Top level z-order, back to front.
    pub order: Vec<u64>,
    /// Active layer id.
    pub active_layer: u64,
    /// Active sheet id.
    pub active_sheet: u64,
    /// Relief model components, bottom first. Their heights travel beside
    /// `document.json` like the top side's.
    #[serde(default)]
    pub components: Vec<Component>,
    /// Model levels, bottom first.
    #[serde(default)]
    pub levels: Vec<Level>,
    /// The level new components go on.
    #[serde(default)]
    pub active_level: u64,
}

impl SideState {
    /// Rebuilds a side from its file shape.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the side has no layers or
    /// sheets.
    pub(super) fn from_file(file: SideFile) -> Result<SideState, CoreError> {
        if file.layers.is_empty() {
            return Err(CoreError::validation("the bottom side has no layers"));
        }
        if file.sheets.is_empty() {
            return Err(CoreError::validation("the bottom side has no sheets"));
        }
        let entities: BTreeMap<u64, Entity> =
            file.entities.into_iter().map(|e| (e.id, e)).collect();
        let order = file
            .order
            .into_iter()
            .filter(|id| entities.contains_key(id))
            .collect();
        let levels = if file.levels.is_empty() {
            vec![Level::first()]
        } else {
            file.levels
        };
        let first_level = levels.first().map_or(1, |l| l.id);
        let active_level = if levels.iter().any(|l| l.id == file.active_level) {
            file.active_level
        } else {
            first_level
        };
        let mut components = file.components;
        for component in &mut components {
            if !levels.iter().any(|l| l.id == component.level) {
                component.level = first_level;
            }
        }
        let active_layer = if file.layers.iter().any(|l| l.id == file.active_layer) {
            file.active_layer
        } else {
            file.layers[0].id
        };
        let active_sheet = if file.sheets.iter().any(|s| s.id == file.active_sheet) {
            file.active_sheet
        } else {
            file.sheets[0].id
        };
        Ok(SideState {
            layers: file.layers,
            sheets: file.sheets,
            entities,
            order,
            selection: Vec::new(),
            active_layer,
            active_sheet,
            component_order: components.iter().map(|c| c.id).collect(),
            components: components.into_iter().map(|c| (c.id, c)).collect(),
            levels,
            active_level,
        })
    }

    /// The side in its file shape.
    pub(super) fn to_file(&self) -> SideFile {
        SideFile {
            layers: self.layers.clone(),
            sheets: self.sheets.clone(),
            entities: self.entities.values().cloned().collect(),
            order: self.order.clone(),
            active_layer: self.active_layer,
            active_sheet: self.active_sheet,
            components: self
                .component_order
                .iter()
                .filter_map(|id| self.components.get(id).cloned())
                .collect(),
            levels: self.levels.clone(),
            active_level: self.active_level,
        }
    }

    /// Largest ids in use, so counters can be kept above them:
    /// `(entity, layer, sheet, component, level)`.
    pub(super) fn max_ids(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.entities.keys().copied().max().unwrap_or(0),
            self.layers.iter().map(|l| l.id).max().unwrap_or(0),
            self.sheets.iter().map(|s| s.id).max().unwrap_or(0),
            self.components.keys().copied().max().unwrap_or(0),
            self.levels.iter().map(|l| l.id).max().unwrap_or(0),
        )
    }

    /// Relief components, for writing their heights beside the document.
    pub fn components_mut(&mut self) -> impl Iterator<Item = &mut Component> {
        self.components.values_mut()
    }

    /// Entity revisions, for telling whether an other side toolpath's
    /// vectors changed.
    pub(super) fn entity(&self, id: u64) -> Option<&Entity> {
        self.entities.get(&id)
    }
}

impl SideFile {
    /// Relief components, for reading their heights from beside the
    /// document.
    pub fn components_mut(&mut self) -> impl Iterator<Item = &mut Component> {
        self.components.iter_mut()
    }
}

impl Document {
    /// The other side's visible model baked on the model grid, in that
    /// side's own job space; `None` when that side has no components. Its
    /// levels are clipped by the shapes kept when their clipping was
    /// applied.
    pub fn other_side_composite(&self) -> Option<crate::model::field::Field> {
        use crate::geom::{Region, Tolerance};
        use crate::model::shape::inside_cells;
        let other = self.other_side.as_deref()?;
        let first_sheet = other.sheets.first().map(|s| s.id);
        let sheet_of = |c: &Component| {
            if other.sheets.iter().any(|s| s.id == c.extra.sheet) {
                Some(c.extra.sheet)
            } else {
                first_sheet
            }
        };
        let chosen: Vec<&Component> = other
            .component_order
            .iter()
            .filter_map(|id| other.components.get(id))
            .filter(|c| sheet_of(c) == Some(other.active_sheet))
            .collect();
        if chosen.is_empty() {
            return None;
        }
        let grid = crate::model::grid_for(&self.job);
        Some(crate::model::tree::bake_model(
            &other.levels,
            &chosen,
            &grid,
            |level| {
                let tol = Tolerance::new(0.01).ok()?;
                let region = Region::from_paths(&level.clip_shape, tol);
                (!region.is_empty()).then(|| inside_cells(&region, &grid, tol))
            },
        ))
    }

    /// Highest point of the other side's model above the shared modelling
    /// plane, in millimetres; zero when that side has no model or the job
    /// is not double sided.
    pub fn other_side_peak(&self) -> f64 {
        if !self.job.is_double_sided() {
            return 0.0;
        }
        self.other_side_composite()
            .and_then(|field| field.max())
            .map_or(0.0, |peak| f64::from(peak).max(0.0))
    }

    /// Job Z of the active side's model zero plane, for `composite`, the
    /// active side's baked model. Both sides of a double sided job share one
    /// modelling plane, placed by the Material Setup gap above or below the
    /// whole two sided model.
    pub fn model_zero(&self, composite: &crate::model::field::Field) -> f64 {
        let peak = f64::from(composite.max().unwrap_or(0.0)).max(0.0);
        let other = self.other_side_peak();
        let (top_peak, bottom_peak) = match self.active_side {
            Side::Top => (peak, other),
            Side::Bottom => (other, peak),
        };
        self.job()
            .model_zero(self.active_side, top_peak, bottom_peak)
    }

    /// The other side's vectors (paths and text outlines on its active
    /// sheet and visible layers) turned over into the active side's job space, for
    /// Multi Sided View. Empty for a single sided job.
    pub fn other_side_paths(&self) -> Vec<crate::geom::Path> {
        let Some(other) = self.other_side.as_deref() else {
            return Vec::new();
        };
        if !self.job.is_double_sided() {
            return Vec::new();
        }
        let (e, f) = self.job.flip_xy(0.0, 0.0);
        let (ax, ay) = self.job.flip_xy(1.0, 0.0);
        let (cx, cy) = self.job.flip_xy(0.0, 1.0);
        let turn = crate::geom::Mat3::new(ax - e, ay - f, cx - e, cy - f, e, f);
        let shown: std::collections::BTreeSet<u64> = other
            .layers
            .iter()
            .filter(|l| l.visible)
            .map(|l| l.id)
            .collect();
        other
            .order
            .iter()
            .filter_map(|id| other.entities.get(id))
            .flat_map(|e| {
                // A group's members are drawn with it.
                let mut all = vec![e];
                all.extend(other.entities.values().filter(|m| m.parent == Some(e.id)));
                all
            })
            .filter(|e| e.sheet == other.active_sheet && shown.contains(&e.layer))
            .flat_map(|e| match &e.kind {
                super::entity::EntityKind::Path { path } => vec![path],
                super::entity::EntityKind::Text(text) => text.paths.iter().collect(),
                _ => Vec::new(),
            })
            .map(|path| crate::geom::transform_path(path, &turn))
            .collect()
    }

    /// The side whose content the document shows and edits.
    pub fn active_side(&self) -> Side {
        self.active_side
    }

    /// The other side's content, when that side has any yet.
    pub fn other_side(&self) -> Option<&SideState> {
        self.other_side.as_deref()
    }

    /// A copy of the document with the other side of a double sided job
    /// active, for work in that side's job space (checking its toolpaths
    /// against its own vectors, model and clamps). The undo history stays
    /// behind, and so do the model components of the side active here; the
    /// other side's come too only with `model` (a check that needs that
    /// side's model surface), since they can be large. `None` when the job
    /// is single sided or the other side has no content yet.
    pub fn turned_over(&self, model: bool) -> Option<Document> {
        if !self.job.is_double_sided() {
            return None;
        }
        let other = self.other_side.as_deref()?;
        let other = SideState {
            layers: other.layers.clone(),
            sheets: other.sheets.clone(),
            entities: other.entities.clone(),
            order: other.order.clone(),
            selection: Vec::new(),
            active_layer: other.active_layer,
            active_sheet: other.active_sheet,
            components: if model {
                other.components.clone()
            } else {
                BTreeMap::new()
            },
            component_order: if model {
                other.component_order.clone()
            } else {
                Vec::new()
            },
            levels: other.levels.clone(),
            active_level: other.active_level,
        };
        let mut turned = Document {
            time_estimate: self.time_estimate,
            cam_settings: self.cam_settings,
            job: self.job,
            revision: self.revision,
            path: None,
            dirty: false,
            layers: self.layers.clone(),
            sheets: self.sheets.clone(),
            entities: self.entities.clone(),
            order: self.order.clone(),
            selection: Vec::new(),
            active_layer: self.active_layer,
            active_sheet: self.active_sheet,
            notes: String::new(),
            variables: self.variables.clone(),
            variable_formulas: self.variable_formulas.clone(),
            distortions: Vec::new(),
            nest_marks: BTreeMap::new(),
            copies_made: BTreeMap::new(),
            tiling: self.tiling,
            dimension_style: self.dimension_style.clone(),
            guides: Vec::new(),
            assembly: crate::assembly::Assembly::default(),
            next_entity_id: self.next_entity_id,
            next_layer_id: self.next_layer_id,
            next_sheet_id: self.next_sheet_id,
            toolpaths: self.toolpaths.clone(),
            toolpath_order: self.toolpath_order.clone(),
            next_toolpath_id: self.next_toolpath_id,
            toolpath_groups: self.toolpath_groups.clone(),
            components: BTreeMap::new(),
            component_order: Vec::new(),
            next_component_id: self.next_component_id,
            levels: self.levels.clone(),
            active_level: self.active_level,
            next_level_id: self.next_level_id,
            model_stamp: self.model_stamp,
            journal: super::undo::Journal::default(),
            active_side: self.active_side,
            other_side: Some(Box::new(other)),
            side_job: self.side_job,
            images: super::images::ImageStore::default(),
        };
        turned.swap_live(true);
        Some(turned)
    }

    /// Makes `side` the active one inside a command's undo step, bringing
    /// its content in; the step records the switch, so a command can work
    /// on the other side and come back. The first visit to the bottom side
    /// gives it an empty layer, sheet and level.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when asking for the bottom side of
    /// a job that is not double sided.
    pub fn set_active_side(&mut self, side: Side) -> Result<(), CoreError> {
        self.check_side(side)?;
        if side != self.active_side {
            self.switch(true, true);
        }
        Ok(())
    }

    /// Shows `side`: the user switching sides (keys 1 and 2, the side
    /// buttons). A view change, not an edit: no undo step records it and
    /// the job does not count as changed; Undo and Redo turn to the side
    /// of the step they take back (see [`Step::side`](super::undo::Step)).
    /// The revision moves so the views read the other side's content.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when asking for the bottom side of
    /// a job that is not double sided.
    pub fn show_side(&mut self, side: Side) -> Result<(), CoreError> {
        self.check_side(side)?;
        if side != self.active_side {
            self.switch(true, false);
            self.revision += 1;
        }
        Ok(())
    }

    /// Turns to the side an undo or redo step was made on, outside the
    /// journal, before its changes are reverted.
    pub(super) fn turn_to(&mut self, side: Option<Side>) {
        let Some(side) = side else {
            return;
        };
        if side != self.active_side && (side == Side::Top || self.other_side.is_some()) {
            self.switch(true, false);
        }
    }

    fn check_side(&self, side: Side) -> Result<(), CoreError> {
        if side == Side::Bottom && side != self.active_side && !self.job.is_double_sided() {
            return Err(CoreError::validation(
                "only a double sided job has a bottom side; change the job type in Job Setup",
            ));
        }
        Ok(())
    }

    /// Exchanges all content between the top and bottom sides, toolpaths
    /// included, leaving the active side where it is.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the job is not double sided.
    pub fn swap_sides(&mut self) -> Result<(), CoreError> {
        if !self.job.is_double_sided() {
            return Err(CoreError::validation(
                "sides can only be swapped on a double sided job",
            ));
        }
        self.switch(false, true);
        let ids = self.toolpath_order.clone();
        for id in ids {
            self.capture_toolpath(id);
            if let Some(toolpath) = self.toolpaths.get_mut(&id) {
                toolpath.side = toolpath.side.other();
            }
        }
        Ok(())
    }

    /// Swaps the live content with the other side's, creating that side's
    /// first layer, sheet and level on the first visit, and records it in
    /// the open step when `record` is set.
    fn switch(&mut self, toggle: bool, record: bool) {
        let created = self.other_side.is_none();
        if created {
            if record {
                self.capture_counters();
            }
            let layer = self.next_layer_id;
            let sheet = self.next_sheet_id;
            let level = self.next_level_id;
            self.next_layer_id += 1;
            self.next_sheet_id += 1;
            self.next_level_id += 1;
            let mut first_level = Level::first();
            first_level.id = level;
            self.other_side = Some(Box::new(SideState {
                layers: vec![Layer::new(layer, FIRST_LAYER, Layer::colour_for(0))],
                sheets: vec![Sheet::new(sheet, FIRST_SHEET)],
                entities: BTreeMap::new(),
                order: Vec::new(),
                selection: Vec::new(),
                active_layer: layer,
                active_sheet: sheet,
                components: BTreeMap::new(),
                component_order: Vec::new(),
                levels: vec![first_level],
                active_level: level,
            }));
        }
        self.swap_live(toggle);
        if record && self.journal.is_recording() {
            self.journal.record(Change::SwapSides {
                toggle,
                install: None,
                remove: created,
            });
        }
    }

    /// Exchanges the live fields with the stored other side.
    pub(super) fn swap_live(&mut self, toggle: bool) {
        let Some(other) = self.other_side.as_deref_mut() else {
            return;
        };
        std::mem::swap(&mut self.layers, &mut other.layers);
        std::mem::swap(&mut self.sheets, &mut other.sheets);
        std::mem::swap(&mut self.entities, &mut other.entities);
        std::mem::swap(&mut self.order, &mut other.order);
        std::mem::swap(&mut self.selection, &mut other.selection);
        std::mem::swap(&mut self.active_layer, &mut other.active_layer);
        std::mem::swap(&mut self.active_sheet, &mut other.active_sheet);
        std::mem::swap(&mut self.components, &mut other.components);
        std::mem::swap(&mut self.component_order, &mut other.component_order);
        std::mem::swap(&mut self.levels, &mut other.levels);
        std::mem::swap(&mut self.active_level, &mut other.active_level);
        if toggle {
            self.active_side = self.active_side.other();
        }
        self.model_stamp += 1;
        self.sync_job();
    }

    /// Reverts a [`Change::SwapSides`] and answers the change that redoes
    /// it.
    pub(super) fn revert_swap(
        &mut self,
        toggle: bool,
        install: Option<Box<SideState>>,
        remove: bool,
    ) -> Change {
        let installed = install.is_some();
        if install.is_some() {
            self.other_side = install;
        }
        self.swap_live(toggle);
        let removed = if remove { self.other_side.take() } else { None };
        Change::SwapSides {
            toggle,
            install: removed,
            remove: installed,
        }
    }

    /// Keeps the job the commands see in step with the active side's Z
    /// zero.
    pub(super) fn sync_job(&mut self) {
        self.side_job = self.job.for_side(self.active_side);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{StrategyKind, Toolpath};
    use crate::doc::{EntityKind, FlipAxis, JobKind, JobSetup, ZZero};
    use crate::geom::{rect, Point};

    fn double_sided() -> Document {
        let mut doc = Document::new();
        doc.begin("Job");
        doc.set_job(JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        })
        .expect("job");
        doc.commit();
        doc
    }

    fn add_rect(doc: &mut Document, x: f64) -> u64 {
        doc.begin("Rect");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            parent: None,
            revision: 0,
            kind: EntityKind::Path {
                path: rect(Point::new(x, 0.0), Point::new(x + 10.0, 10.0)).expect("rect"),
            },
            name: None,
        });
        doc.commit();
        id
    }

    fn add_bump(doc: &mut Document, name: &str) {
        use crate::model::field::Field;
        doc.begin("Bump");
        doc.add_component(crate::model::Component {
            id: 0,
            name: name.into(),
            visible: true,
            combine: crate::model::Combine::default(),
            height_scale: 1.0,
            level: 0,
            extra: crate::model::tree::ComponentExtra::default(),
            field: Field {
                width: 4,
                height: 4,
                cell: 1.0,
                origin: Point::new(1.0, 1.0),
                z: vec![2.0; 16],
            },
        });
        doc.commit();
    }

    #[test]
    fn the_turned_copy_brings_the_other_sides_model_only_when_asked() {
        let mut doc = double_sided();
        add_bump(&mut doc, "top");
        doc.set_active_side(Side::Bottom).expect("bottom");
        add_bump(&mut doc, "bottom");
        add_rect(&mut doc, 5.0);
        doc.set_active_side(Side::Top).expect("top");
        let names = |d: &Document| -> Vec<String> {
            d.all_components().iter().map(|c| c.name.clone()).collect()
        };
        let bare = doc.turned_over(false).expect("turned");
        assert_eq!(bare.active_side(), Side::Bottom);
        assert!(names(&bare).is_empty());
        assert_eq!(bare.entities().len(), 1);
        let with = doc.turned_over(true).expect("turned");
        assert_eq!(names(&with), vec!["bottom".to_string()]);
        // The side shown here stays behind without its model.
        let behind = with.other_side().expect("the top");
        assert!(behind.components.is_empty());
        assert_eq!(names(&doc), vec!["top".to_string()]);
    }

    #[test]
    fn a_single_sided_job_has_no_bottom_side() {
        let mut doc = Document::new();
        doc.begin("Side");
        assert!(doc.set_active_side(Side::Bottom).is_err());
        assert!(doc.swap_sides().is_err());
        doc.rollback();
        assert_eq!(doc.active_side(), Side::Top);
    }

    #[test]
    fn each_side_keeps_its_own_vectors() {
        let mut doc = double_sided();
        let top = add_rect(&mut doc, 0.0);
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        assert_eq!(doc.active_side(), Side::Bottom);
        assert!(doc.list(None).is_empty());
        assert_eq!(doc.layers().len(), 1);
        assert_ne!(doc.active_sheet(), 1, "the bottom side gets its own sheet");
        let bottom = add_rect(&mut doc, 50.0);
        assert_ne!(top, bottom);
        assert_eq!(doc.list(None).len(), 1);
        doc.begin("Side");
        doc.set_active_side(Side::Top).expect("top");
        doc.commit();
        let ids: Vec<u64> = doc.list(None).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![top]);
    }

    #[test]
    fn undo_and_redo_walk_back_through_side_switches() {
        let mut doc = double_sided();
        add_rect(&mut doc, 0.0);
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        add_rect(&mut doc, 50.0);
        assert!(doc.undo().is_some());
        assert!(doc.list(None).is_empty());
        assert!(doc.undo().is_some());
        assert_eq!(doc.active_side(), Side::Top);
        assert!(
            doc.other_side().is_none(),
            "the first visit is undone whole"
        );
        assert_eq!(doc.list(None).len(), 1);
        assert!(doc.redo().is_some());
        assert_eq!(doc.active_side(), Side::Bottom);
        assert!(doc.redo().is_some());
        assert_eq!(doc.list(None).len(), 1);
        assert_eq!(doc.list(None)[0].bbox[0], 50.0);
    }

    #[test]
    fn showing_a_side_is_no_undo_step_and_leaves_the_job_unchanged() {
        let mut doc = double_sided();
        add_rect(&mut doc, 0.0);
        doc.mark_saved(std::path::PathBuf::from("job.carve"));
        let revision = doc.revision();
        doc.show_side(Side::Bottom).expect("bottom");
        assert_eq!(doc.active_side(), Side::Bottom);
        assert!(doc.list(None).is_empty());
        assert!(doc.revision() > revision, "the views read the other side");
        assert!(!doc.is_dirty());
        assert_eq!(doc.journal().undo_label(), Some("Rect"));
        assert!(!doc.journal().can_redo());
        doc.show_side(Side::Top).expect("top");
        assert_eq!(doc.journal().undo_label(), Some("Rect"));
        assert!(Document::new().show_side(Side::Bottom).is_err());
    }

    #[test]
    fn undo_takes_back_the_last_edit_on_the_side_it_was_made() {
        let mut doc = double_sided();
        let top = add_rect(&mut doc, 0.0);
        doc.show_side(Side::Bottom).expect("bottom");
        let bottom = add_rect(&mut doc, 50.0);
        doc.show_side(Side::Top).expect("top");
        // Looking at the top side, Undo takes back the bottom rectangle
        // and shows the bottom side it came off.
        assert_eq!(doc.undo().as_deref(), Some("Rect"));
        assert_eq!(doc.active_side(), Side::Bottom);
        assert!(doc.list(None).is_empty());
        doc.show_side(Side::Top).expect("top");
        let ids: Vec<u64> = doc.list(None).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![top], "the top side is untouched");
        // Redo turns to the bottom side again and puts it back.
        assert_eq!(doc.redo().as_deref(), Some("Rect"));
        assert_eq!(doc.active_side(), Side::Bottom);
        let ids: Vec<u64> = doc.list(None).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![bottom]);
        // Two undos: the bottom rectangle, then the top one on the top side.
        doc.undo();
        assert_eq!(doc.undo().as_deref(), Some("Rect"));
        assert_eq!(doc.active_side(), Side::Top);
        assert!(doc.list(None).is_empty());
        doc.show_side(Side::Bottom).expect("bottom");
        assert!(doc.list(None).is_empty());
    }

    #[test]
    fn swapping_sides_exchanges_content_and_toolpath_sides() {
        let mut doc = double_sided();
        let top = add_rect(&mut doc, 0.0);
        doc.begin("Toolpath");
        let toolpath = doc.add_toolpath(Toolpath::new(
            0,
            "Profile",
            Tool::default(),
            StrategyKind::Profile,
        ));
        doc.commit();
        assert_eq!(doc.toolpath(toolpath).expect("toolpath").side, Side::Top);
        doc.begin("Swap Sides");
        doc.swap_sides().expect("swaps");
        doc.commit();
        assert_eq!(doc.active_side(), Side::Top);
        assert!(doc.list(None).is_empty());
        assert_eq!(doc.toolpath(toolpath).expect("toolpath").side, Side::Bottom);
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        assert_eq!(doc.list(None)[0].id, top);
        doc.undo();
        doc.undo();
        assert_eq!(doc.list(None)[0].id, top);
        assert_eq!(doc.toolpath(toolpath).expect("toolpath").side, Side::Top);
    }

    #[test]
    fn the_bottom_side_uses_its_own_z_zero() {
        let mut doc = Document::new();
        doc.begin("Job");
        let mut job = JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        };
        job.double_sided.bottom_z_zero = ZZero::Bottom;
        job.double_sided.flip = FlipAxis::Vertical;
        doc.set_job(job).expect("job");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        assert_eq!(doc.job().z_zero, ZZero::Bottom);
        assert_eq!(doc.job_setup().z_zero, ZZero::Top);
        doc.undo();
        assert_eq!(doc.job().z_zero, ZZero::Top);
    }

    #[test]
    fn making_the_job_single_sided_goes_back_to_the_top_side() {
        let mut doc = double_sided();
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        doc.begin("Job");
        doc.set_job(JobSetup::default()).expect("job");
        doc.commit();
        assert_eq!(doc.active_side(), Side::Top);
        assert!(doc.other_side().is_some(), "the bottom content is kept");
    }

    #[test]
    fn toolpaths_are_calculated_on_the_active_side() {
        let mut doc = double_sided();
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let id = doc.add_toolpath(Toolpath::new(
            0,
            "Pocket",
            Tool::default(),
            StrategyKind::Pocket,
        ));
        doc.commit();
        assert_eq!(doc.toolpath(id).expect("toolpath").side, Side::Bottom);
    }

    #[test]
    fn both_sides_round_trip_through_the_file_shape() {
        let mut doc = double_sided();
        add_rect(&mut doc, 0.0);
        doc.begin("Side");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        add_rect(&mut doc, 50.0);
        let file = doc.to_file();
        assert_eq!(file.active_side, Side::Bottom);
        assert_eq!(file.entities.len(), 1);
        assert_eq!(file.entities[0].bbox().map(|b| b.min.x), Some(0.0));
        let text = serde_json::to_string(&file).expect("serialises");
        let back =
            Document::from_file(serde_json::from_str(&text).expect("parses"), 7).expect("reads");
        assert_eq!(back, doc);
        assert_eq!(back.active_side(), Side::Bottom);
        assert_eq!(back.list(None)[0].bbox[0], 50.0);
    }
}
