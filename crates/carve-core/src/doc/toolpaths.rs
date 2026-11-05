//! The document's calculated toolpaths: the toolpath list, in order, with
//! undo like every other part of the document.

use crate::cam::common::Heights;
use crate::cam::toolpath::{SourceRef, Toolpath};
use crate::error::CoreError;

use super::{Change, Document};

impl Document {
    /// Toolpaths in list order.
    pub fn toolpaths(&self) -> Vec<&Toolpath> {
        self.toolpath_order
            .iter()
            .filter_map(|id| self.toolpaths.get(id))
            .collect()
    }

    /// Toolpath ids in list order.
    pub fn toolpath_order(&self) -> &[u64] {
        &self.toolpath_order
    }

    /// One toolpath.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn toolpath(&self, id: u64) -> Result<&Toolpath, CoreError> {
        self.toolpaths
            .get(&id)
            .ok_or_else(|| CoreError::validation(format!("no toolpath with id {id}")))
    }

    /// True when a vector the toolpath was calculated from has changed or
    /// gone since, which the list shows as needing recalculation.
    pub fn toolpath_is_stale(&self, toolpath: &Toolpath) -> bool {
        // Loaded from a template and never calculated.
        if toolpath.params_json.get("uncalculated") == Some(&serde_json::Value::Bool(true)) {
            return true;
        }
        if toolpath.side != self.active_side {
            // A side the job no longer has: the job was double sided when
            // the toolpath was made, and it cannot be calculated or cut
            // until it is again.
            if !self.job_setup().is_double_sided() {
                return true;
            }
            // The other side's vectors and model are put away.
            return self.other_side().is_none_or(|other| {
                toolpath.sources.iter().any(|source| {
                    other
                        .entity(source.id)
                        .is_none_or(|e| e.revision != source.revision)
                })
            });
        }
        let vectors_changed =
            toolpath
                .sources
                .iter()
                .any(|source| match self.entities.get(&source.id) {
                    Some(entity) => entity.revision != source.revision,
                    None => true,
                });
        // Toolpaths cut from the model record the model they were cut from.
        let model_changed = toolpath
            .params_json
            .get("model")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|print| print != self.model_fingerprint());
        vectors_changed || model_changed || self.heights_moved(toolpath)
    }

    /// True when the material setup now asks for other heights (the Z zero,
    /// the thickness under a bed zero, the clearance or home) than the
    /// toolpath's moves were calculated at. Only the shown sheet's toolpaths
    /// are compared, and imported toolpaths keep the heights of their file.
    fn heights_moved(&self, toolpath: &Toolpath) -> bool {
        if toolpath.moves.is_empty()
            || toolpath.sheet != self.active_sheet()
            || toolpath.params_json.get("imported").is_some()
        {
            return false;
        }
        let heights = Heights::from_job(self.job());
        const SAME: f64 = 1e-6;
        (heights.safe - toolpath.safe_z).abs() > SAME
            || (heights.home.x - toolpath.home.x).abs() > SAME
            || (heights.home.y - toolpath.home.y).abs() > SAME
            || (heights.home.z - toolpath.home.z).abs() > SAME
    }

    /// The revision stamps of the given entities, for a toolpath's sources.
    pub fn source_refs(&self, ids: &[u64]) -> Vec<SourceRef> {
        ids.iter()
            .filter_map(|id| {
                self.entities.get(id).map(|entity| SourceRef {
                    id: *id,
                    revision: entity.revision,
                })
            })
            .collect()
    }

    /// How machining times are estimated.
    pub fn time_estimate(&self) -> &crate::cam::TimeEstimate {
        &self.time_estimate
    }

    /// Estimates machining times this way from now on, and estimates every
    /// toolpath again. Not an edit: nothing to undo.
    pub fn set_time_estimate(&mut self, estimate: crate::cam::TimeEstimate) {
        self.time_estimate = estimate;
        let for_job = self.job_estimate();
        for toolpath in self.toolpaths.values_mut() {
            for_job.apply(toolpath);
        }
    }

    /// The toolpath preferences calculations follow.
    pub fn cam_settings(&self) -> &crate::cam::CamSettings {
        &self.cam_settings
    }

    /// Calculates toolpaths with these preferences from now on. Toolpaths
    /// already calculated keep their moves. Not an edit: nothing to undo.
    pub fn set_cam_settings(&mut self, settings: crate::cam::CamSettings) {
        self.cam_settings = settings;
    }

    /// The estimate settings for this job: on a rotary job the axis that
    /// wraps round the blank takes the rotary axis's acceleration.
    fn job_estimate(&self) -> crate::cam::TimeEstimate {
        if self.job.is_rotary() {
            let rotary = &self.job.rotary;
            self.time_estimate
                .for_rotary(rotary.axis == crate::doc::RotaryAxis::X, rotary.diameter)
        } else {
            self.time_estimate
        }
    }

    /// Adds a toolpath at the end of the list, giving it the next id, which
    /// it answers.
    pub fn add_toolpath(&mut self, mut toolpath: Toolpath) -> u64 {
        self.job_estimate().apply(&mut toolpath);
        self.capture_counters();
        self.capture_toolpath_order();
        let id = self.next_toolpath_id;
        self.next_toolpath_id += 1;
        toolpath.id = id;
        toolpath.side = self.active_side;
        if toolpath.sheet == 0 {
            toolpath.sheet = self.active_sheet;
        }
        if toolpath.through_cut.is_none() {
            toolpath.through_cut = crate::cam::depth::through_cut(self, &toolpath);
        }
        self.capture_toolpath(id);
        self.toolpaths.insert(id, toolpath);
        self.toolpath_order.push(id);
        id
    }

    /// Replaces a toolpath in place, keeping its id and list position. A
    /// replacement without a through cut allowance or gadget keeps the old
    /// toolpath's (a recalculation from stored settings).
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn replace_toolpath(&mut self, id: u64, mut toolpath: Toolpath) -> Result<(), CoreError> {
        self.job_estimate().apply(&mut toolpath);
        if self.toolpath(id)?.side != self.active_side {
            return Err(CoreError::validation(
                "this toolpath cuts the other side; switch to that side to recalculate it",
            ));
        }
        let sheet = self.toolpath(id)?.sheet;
        let group = self.toolpath(id)?.group;
        let old = self.toolpath(id)?;
        let kept_cut = old.through_cut;
        let kept_gadget = old.gadget.clone();
        self.capture_toolpath(id);
        toolpath.id = id;
        toolpath.group = group;
        toolpath.side = self.active_side;
        toolpath.sheet = if sheet == 0 { self.active_sheet } else { sheet };
        toolpath.through_cut = toolpath
            .through_cut
            .or(kept_cut)
            .or_else(|| crate::cam::depth::through_cut(self, &toolpath));
        toolpath.gadget = toolpath.gadget.or(kept_gadget);
        self.toolpaths.insert(id, toolpath);
        Ok(())
    }

    /// Measures again how far a toolpath cuts below its material, for a
    /// toolpath just calculated from settings the user edited.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn remeasure_through_cut(&mut self, id: u64) -> Result<(), CoreError> {
        let measured = crate::cam::depth::through_cut(self, self.toolpath(id)?);
        self.update_toolpath(id, |t| t.through_cut = measured)
    }

    /// Gives toolpaths from older files, which never recorded a through cut
    /// allowance, the one they have in the job they were saved with.
    pub(super) fn fill_through_cuts(&mut self) {
        let missing: Vec<u64> = self
            .toolpaths
            .values()
            .filter(|t| t.through_cut.is_none())
            .map(|t| t.id)
            .collect();
        for id in missing {
            let measured = self
                .toolpaths
                .get(&id)
                .and_then(|t| crate::cam::depth::through_cut(self, t));
            if let Some(toolpath) = self.toolpaths.get_mut(&id) {
                toolpath.through_cut = measured;
            }
        }
    }

    /// Edits a toolpath in place.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn update_toolpath<F>(&mut self, id: u64, edit: F) -> Result<(), CoreError>
    where
        F: FnOnce(&mut Toolpath),
    {
        self.toolpath(id)?;
        self.capture_toolpath(id);
        if let Some(toolpath) = self.toolpaths.get_mut(&id) {
            edit(toolpath);
        }
        Ok(())
    }

    /// Removes toolpaths. Unknown ids are refused before anything changes.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn delete_toolpaths(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.toolpath(*id)?;
        }
        self.capture_toolpath_order();
        for id in ids {
            self.capture_toolpath(*id);
            self.toolpaths.remove(id);
        }
        self.toolpath_order.retain(|id| !ids.contains(id));
        Ok(())
    }

    /// Puts the toolpath list in the given order. Ids left out keep their
    /// relative order after the ones given.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown id.
    pub fn reorder_toolpaths(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.toolpath(*id)?;
        }
        self.capture_toolpath_order();
        let mut order: Vec<u64> = Vec::with_capacity(self.toolpath_order.len());
        for id in ids {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        for id in &self.toolpath_order {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        self.toolpath_order = order;
        Ok(())
    }

    pub(super) fn capture_toolpath(&mut self, id: u64) {
        if self.journal.is_recording() {
            let before = self.toolpaths.get(&id).cloned().map(Box::new);
            self.journal.record(Change::Toolpath { id, before });
        }
    }

    fn capture_toolpath_order(&mut self) {
        if self.journal.is_recording() {
            let order = self.toolpath_order.clone();
            self.journal.record(Change::ToolpathOrder(order));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{StrategyKind, Toolpath};
    use crate::doc::Document;

    fn toolpath(name: &str) -> Toolpath {
        Toolpath::new(0, name, Tool::default(), StrategyKind::Profile)
    }

    #[test]
    fn toolpaths_are_added_in_order_and_undone() {
        let mut doc = Document::new();
        doc.begin("Profile");
        let a = doc.add_toolpath(toolpath("a"));
        doc.commit();
        doc.begin("Pocket");
        let b = doc.add_toolpath(toolpath("b"));
        doc.commit();
        assert_eq!(doc.toolpath_order(), &[a, b]);
        doc.undo();
        assert_eq!(doc.toolpath_order(), &[a]);
        assert!(doc.toolpath(b).is_err());
        doc.redo();
        assert_eq!(
            doc.toolpath(b).map(|t| t.name.clone()).ok(),
            Some("b".into())
        );
    }

    #[test]
    fn an_undone_toolpath_id_is_not_handed_out_again() {
        let mut doc = Document::new();
        doc.begin("Profile");
        let first = doc.add_toolpath(toolpath("a"));
        doc.commit();
        doc.undo();
        doc.begin("Profile");
        let second = doc.add_toolpath(toolpath("b"));
        doc.commit();
        assert_ne!(first, second);
    }

    #[test]
    fn reorder_and_delete_are_undoable() {
        let mut doc = Document::new();
        doc.begin("Add");
        let a = doc.add_toolpath(toolpath("a"));
        let b = doc.add_toolpath(toolpath("b"));
        doc.commit();
        doc.begin("Reorder");
        doc.reorder_toolpaths(&[b]).expect("reorders");
        doc.commit();
        assert_eq!(doc.toolpath_order(), &[b, a]);
        doc.begin("Delete");
        doc.delete_toolpaths(&[b]).expect("deletes");
        doc.commit();
        assert_eq!(doc.toolpath_order(), &[a]);
        doc.undo();
        doc.undo();
        assert_eq!(doc.toolpath_order(), &[a, b]);
        assert!(doc.delete_toolpaths(&[99]).is_err());
    }

    #[test]
    fn toolpaths_survive_the_document_file() {
        let mut doc = Document::new();
        doc.begin("Add");
        doc.add_toolpath(toolpath("keep me"));
        doc.commit();
        let back = Document::from_file(doc.to_file(), 1).expect("reads back");
        assert_eq!(back.toolpaths().len(), 1);
        assert_eq!(back.toolpaths()[0].name, "keep me");
    }
}
