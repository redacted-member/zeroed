//! What each sheet owns beyond its vectors: its own material (size,
//! thickness and datum) and its relief components.
//!
//! The job setup always describes the active sheet. Every other sheet keeps
//! its material in [`Sheet::material`](super::Sheet) and trades places with
//! the job setup when it becomes active, so the views, the model grid and
//! the toolpaths all follow the sheet on show without knowing about sheets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::{Document, JobSetup, XyOrigin, ZZero};

/// The part of the job setup that belongs to one sheet. Lengths are
/// millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetMaterial {
    /// Width along X.
    pub width: f64,
    /// Height along Y.
    pub height: f64,
    /// Material thickness along Z.
    pub thickness: f64,
    /// Which face Z zero refers to.
    pub z_zero: ZZero,
    /// Which point of the sheet the XY datum sits on.
    pub xy_origin: XyOrigin,
    /// Datum offset along X.
    pub offset_x: f64,
    /// Datum offset along Y.
    pub offset_y: f64,
}

impl SheetMaterial {
    /// The material a job setup describes.
    pub fn of(job: &JobSetup) -> Self {
        SheetMaterial {
            width: job.width,
            height: job.height,
            thickness: job.thickness,
            z_zero: job.z_zero,
            xy_origin: job.xy_origin,
            offset_x: job.offset_x,
            offset_y: job.offset_y,
        }
    }

    /// `job` with this material, everything else unchanged.
    pub fn applied_to(&self, job: &JobSetup) -> JobSetup {
        JobSetup {
            width: self.width,
            height: self.height,
            thickness: self.thickness,
            z_zero: self.z_zero,
            xy_origin: self.xy_origin,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            ..*job
        }
    }
}

/// Arguments of `sheets.edit`: one material for every sheet in `ids`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetEditArgs {
    /// Sheets to change; the Sheets tab can edit several at once.
    pub ids: Vec<u64>,
    /// Their new material.
    pub material: SheetMaterial,
    /// Design Scaling: scale each sheet's vectors and components with its
    /// new size.
    #[serde(default)]
    pub scale_design: bool,
}

impl Document {
    /// The material of a sheet: the job setup's for the active sheet, its
    /// own for the others (the job setup's when it has none yet).
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet.
    pub fn sheet_material(&self, id: u64) -> Result<SheetMaterial, CoreError> {
        let sheet = self.sheet(id)?;
        if id == self.active_sheet {
            return Ok(SheetMaterial::of(&self.job));
        }
        Ok(sheet
            .material
            .unwrap_or_else(|| SheetMaterial::of(&self.job)))
    }

    /// Changes a sheet's material. For the active sheet this is a job setup
    /// change; for another it is stored until the sheet becomes active.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet or a material
    /// the job setup would refuse (a size or thickness that is not positive).
    pub fn set_sheet_material(
        &mut self,
        id: u64,
        material: SheetMaterial,
    ) -> Result<(), CoreError> {
        self.sheet(id)?;
        let job = material.applied_to(&self.job).normalised();
        job.validate()?;
        if id == self.active_sheet {
            return self.set_job(job);
        }
        self.capture_sheets();
        if let Some(sheet) = self.sheets.iter_mut().find(|s| s.id == id) {
            sheet.material = Some(SheetMaterial::of(&job));
        }
        Ok(())
    }

    /// Gives a new sheet the active sheet's material, as adding a sheet
    /// copies the size of the one on show.
    pub(super) fn remember_sheet_material(&mut self, id: u64) {
        let material = SheetMaterial::of(&self.job);
        if let Some(sheet) = self.sheets.iter_mut().find(|s| s.id == id) {
            sheet.material = Some(material);
        }
    }

    /// Stores the active sheet's material on it and puts `to`'s material in
    /// the job setup. The caller has captured the sheet list for undo.
    pub(super) fn swap_sheet_material(&mut self, to: u64) -> Result<(), CoreError> {
        let from = self.active_sheet;
        if from == to {
            return Ok(());
        }
        let current = SheetMaterial::of(&self.job);
        let mut incoming = None;
        for sheet in &mut self.sheets {
            if sheet.id == from {
                sheet.material = Some(current);
            } else if sheet.id == to {
                incoming = sheet.material;
            }
        }
        self.model_stamp += 1;
        match incoming {
            Some(material) if material != current => {
                let job = material.applied_to(&self.job);
                self.set_job(job)
            }
            _ => Ok(()),
        }
    }

    /// Moves components onto another sheet.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet or component,
    /// before anything changes.
    pub fn set_component_sheet(&mut self, ids: &[u64], sheet: u64) -> Result<(), CoreError> {
        self.sheet(sheet)?;
        for id in ids {
            self.component(*id)?;
        }
        for id in ids {
            self.update_component(*id, |c| c.extra.sheet = sheet)?;
        }
        Ok(())
    }

    /// Copies the components of sheet `from` onto sheet `to`, keeping their
    /// order, and gives `to` the material of `from`. Answers the new
    /// component id for each old one.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet.
    pub fn copy_sheet_contents(
        &mut self,
        from: u64,
        to: u64,
    ) -> Result<BTreeMap<u64, u64>, CoreError> {
        let material = self.sheet_material(from)?;
        self.set_sheet_material(to, material)?;
        let copies: Vec<_> = self
            .component_order
            .iter()
            .filter_map(|id| self.components.get(id))
            .filter(|c| self.sheet_of_component(c) == from)
            .cloned()
            .collect();
        let mut map = BTreeMap::new();
        for mut component in copies {
            let old = component.id;
            component.extra.sheet = to;
            map.insert(old, self.add_component(component));
        }
        Ok(map)
    }

    /// Deletes the components on a sheet that is going away.
    pub(super) fn delete_sheet_components(&mut self, id: u64) -> Result<(), CoreError> {
        let doomed: Vec<u64> = self
            .components
            .values()
            .filter(|c| self.sheet_of_component(c) == id)
            .map(|c| c.id)
            .collect();
        if doomed.is_empty() {
            return Ok(());
        }
        self.delete_components(&doomed)
    }

    /// How many components sit on a sheet.
    pub fn components_on(&self, sheet: u64) -> usize {
        self.components
            .values()
            .filter(|c| self.sheet_of_component(c) == sheet)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use crate::doc::{Document, SheetMaterial, ZZero};
    use crate::geom::Point;
    use crate::model::field::Field;
    use crate::model::{Combine, Component};

    fn component(name: &str, x: f64) -> Component {
        let mut field = Field::empty(2, 2, 1.0, Point::new(x, 10.0));
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

    fn two_sheets() -> (Document, u64, u64) {
        let mut doc = Document::new();
        let first = doc.active_sheet();
        doc.begin("Add Sheet");
        let second = doc.add_sheet(None);
        doc.commit();
        (doc, first, second)
    }

    #[test]
    fn components_belong_to_the_sheet_they_were_made_on() {
        let (mut doc, first, second) = two_sheets();
        doc.begin("Create");
        let a = doc.add_component(component("a", 10.0));
        doc.commit();
        doc.begin("Active Sheet");
        doc.set_active_sheet(second).expect("activates");
        doc.commit();
        assert!(doc.components().is_empty(), "the second sheet has none");
        assert_eq!(doc.composite().at(Point::new(10.0, 10.0)), 0.0);
        doc.begin("Create");
        let b = doc.add_component(component("b", 20.0));
        doc.commit();
        assert_eq!(doc.components().len(), 1);
        assert_eq!(doc.component(b).map(|c| c.extra.sheet).ok(), Some(second));
        assert_eq!(doc.composite().at(Point::new(20.0, 10.0)), 3.0);
        doc.begin("Active Sheet");
        doc.set_active_sheet(first).expect("activates");
        doc.commit();
        let ids: Vec<u64> = doc.components().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a]);
        assert_eq!(doc.all_components().len(), 2);
    }

    #[test]
    fn switching_sheets_changes_the_model_stamp_and_undoes() {
        let (mut doc, first, second) = two_sheets();
        let before = doc.model_stamp();
        doc.begin("Active Sheet");
        doc.set_active_sheet(second).expect("activates");
        doc.commit();
        assert_ne!(doc.model_stamp(), before);
        let switched = doc.model_stamp();
        doc.undo();
        assert_eq!(doc.active_sheet(), first);
        assert_ne!(doc.model_stamp(), switched);
    }

    #[test]
    fn components_without_a_sheet_sit_on_the_first_sheet() {
        let (mut doc, first, second) = two_sheets();
        doc.begin("Create");
        let a = doc.add_component(component("a", 10.0));
        doc.update_component(a, |c| c.extra.sheet = 0)
            .expect("edits");
        doc.commit();
        let c = doc.component(a).expect("there");
        assert_eq!(doc.sheet_of_component(c), first);
        doc.begin("Active Sheet");
        doc.set_active_sheet(second).expect("activates");
        doc.commit();
        assert!(doc.components().is_empty());
        // Deleting the first sheet takes its components with it.
        doc.begin("Delete Sheet");
        doc.delete_sheet(first).expect("deletes");
        doc.commit();
        assert!(doc.all_components().is_empty());
        doc.undo();
        assert_eq!(doc.all_components().len(), 1);
    }

    #[test]
    fn components_move_between_sheets() {
        let (mut doc, _, second) = two_sheets();
        doc.begin("Create");
        let a = doc.add_component(component("a", 10.0));
        doc.commit();
        assert!(doc.set_component_sheet(&[a], 99).is_err());
        assert!(doc.set_component_sheet(&[a, 99], second).is_err());
        doc.begin("Move to Sheet");
        doc.set_component_sheet(&[a], second).expect("moves");
        doc.commit();
        assert!(doc.components().is_empty());
        assert_eq!(doc.components_on(second), 1);
        doc.undo();
        assert_eq!(doc.components().len(), 1);
    }

    #[test]
    fn copying_a_sheet_copies_its_components_and_material() {
        let (mut doc, first, second) = two_sheets();
        doc.begin("Create");
        let a = doc.add_component(component("a", 10.0));
        let b = doc.add_component(component("b", 30.0));
        doc.commit();
        let mut material = doc.sheet_material(first).expect("material");
        material.width = 321.0;
        doc.begin("Edit Sheet");
        doc.set_sheet_material(first, material).expect("edits");
        doc.commit();
        doc.begin("Duplicate");
        let map = doc.copy_sheet_contents(first, second).expect("copies");
        doc.commit();
        assert_eq!(map.len(), 2);
        assert_eq!(doc.components_on(second), 2);
        assert_eq!(
            doc.sheet_material(second).map(|m| m.width).ok(),
            Some(321.0)
        );
        doc.begin("Active Sheet");
        doc.set_active_sheet(second).expect("activates");
        doc.commit();
        let names: Vec<&str> = doc.components().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"], "order kept");
        assert!(!doc.components().iter().any(|c| c.id == a || c.id == b));
    }

    #[test]
    fn every_sheet_keeps_its_own_material() {
        let (mut doc, first, second) = two_sheets();
        let job = *doc.job();
        assert_eq!(
            doc.sheet_material(second).ok(),
            Some(SheetMaterial::of(&job)),
            "a new sheet copies the active sheet's material"
        );
        let other = SheetMaterial {
            width: 150.0,
            height: 90.0,
            thickness: 6.0,
            z_zero: ZZero::Bottom,
            ..SheetMaterial::of(&job)
        };
        doc.begin("Edit Sheet");
        doc.set_sheet_material(second, other).expect("edits");
        doc.commit();
        assert_eq!(doc.job().width, job.width, "the active sheet is untouched");
        doc.begin("Active Sheet");
        doc.set_active_sheet(second).expect("activates");
        doc.commit();
        assert_eq!(doc.job().width, 150.0);
        assert_eq!(doc.job().z_zero, ZZero::Bottom);
        assert_eq!(
            doc.sheet_material(first).map(|m| m.width).ok(),
            Some(job.width)
        );
        doc.begin("Active Sheet");
        doc.set_active_sheet(first).expect("activates");
        doc.commit();
        assert_eq!(doc.job().width, job.width);
        doc.undo();
        assert_eq!(doc.job().width, 150.0);
        let bad = SheetMaterial {
            width: 0.0,
            ..other
        };
        assert!(doc.set_sheet_material(first, bad).is_err());
    }

    #[test]
    fn sheet_materials_survive_a_round_trip() {
        let (mut doc, _, second) = two_sheets();
        let mut material = doc.sheet_material(second).expect("material");
        material.height = 77.0;
        doc.begin("Edit Sheet");
        doc.set_sheet_material(second, material).expect("edits");
        doc.commit();
        let json = serde_json::to_string(doc.sheets()).expect("writes");
        let back: Vec<crate::doc::Sheet> = serde_json::from_str(&json).expect("reads");
        assert_eq!(back.as_slice(), doc.sheets());
        let old: crate::doc::Sheet =
            serde_json::from_str(r#"{"id":3,"name":"Old"}"#).expect("an old sheet parses");
        assert!(old.material.is_none());
    }
}
