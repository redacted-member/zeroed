//! Toolpath groups: named folders in the toolpath list. A toolpath names
//! its group; the members of a group stay next to each other in the list
//! (which is the cutting order), so the group shows where its first member
//! is. An empty group shows at the end of its sheet's list. Each group
//! belongs to a sheet, like the toolpaths in it.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::{Change, Document};

/// One folder in the toolpath list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolpathGroup {
    /// Document wide id, never reused.
    pub id: u64,
    /// Name in the list.
    pub name: String,
    /// The sheet the group belongs to; zero shows on every sheet.
    #[serde(default)]
    pub sheet: u64,
    /// Whether the list shows its toolpaths.
    #[serde(default = "yes")]
    pub expanded: bool,
}

fn yes() -> bool {
    true
}

impl Document {
    /// The toolpath groups, oldest first.
    pub fn toolpath_groups(&self) -> &[ToolpathGroup] {
        &self.toolpath_groups
    }

    fn group_index(&self, id: u64) -> Result<usize, CoreError> {
        self.toolpath_groups
            .iter()
            .position(|g| g.id == id)
            .ok_or_else(|| CoreError::validation(format!("no toolpath group with id {id}")))
    }

    fn capture_groups(&mut self) {
        if self.journal.is_recording() {
            let groups = self.toolpath_groups.clone();
            self.journal.record(Change::ToolpathGroups(groups));
        }
    }

    /// Makes a group named `name` on `sheet` holding `members` (moved next
    /// to the first of them, in list order), and answers its id.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown toolpath or an empty
    /// name.
    pub fn add_toolpath_group(
        &mut self,
        name: &str,
        sheet: u64,
        members: &[u64],
    ) -> Result<u64, CoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::validation("a toolpath group needs a name"));
        }
        for id in members {
            self.toolpath(*id)?;
        }
        self.capture_groups();
        let id = self
            .toolpath_groups
            .iter()
            .map(|g| g.id + 1)
            .max()
            .unwrap_or(1);
        self.toolpath_groups.push(ToolpathGroup {
            id,
            name: name.to_string(),
            sheet,
            expanded: true,
        });
        let ordered: Vec<u64> = self
            .toolpath_order
            .iter()
            .copied()
            .filter(|t| members.contains(t))
            .collect();
        if let Some(&first) = ordered.first() {
            self.place_toolpaths(&ordered, id, Some(first))?;
        }
        Ok(id)
    }

    /// Renames a group, opens or closes it, or moves it and its toolpaths
    /// to another sheet.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group or an empty
    /// name.
    pub fn update_toolpath_group(
        &mut self,
        id: u64,
        name: Option<&str>,
        expanded: Option<bool>,
        sheet: Option<u64>,
    ) -> Result<(), CoreError> {
        let index = self.group_index(id)?;
        if name.is_some_and(|n| n.trim().is_empty()) {
            return Err(CoreError::validation("a toolpath group needs a name"));
        }
        self.capture_groups();
        let group = &mut self.toolpath_groups[index];
        if let Some(name) = name {
            group.name = name.trim().to_string();
        }
        if let Some(expanded) = expanded {
            group.expanded = expanded;
        }
        if let Some(sheet) = sheet {
            group.sheet = sheet;
            let members: Vec<u64> = self.group_members(id);
            for member in members {
                self.update_toolpath(member, |t| t.sheet = sheet)?;
            }
        }
        Ok(())
    }

    /// The toolpaths in a group, in list order.
    pub fn group_members(&self, id: u64) -> Vec<u64> {
        self.toolpath_order
            .iter()
            .copied()
            .filter(|t| self.toolpaths.get(t).is_some_and(|t| t.group == id))
            .collect()
    }

    /// Removes a group. Its toolpaths stay in the list, out of any group,
    /// unless `delete_members`, when they go too.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group.
    pub fn remove_toolpath_group(
        &mut self,
        id: u64,
        delete_members: bool,
    ) -> Result<(), CoreError> {
        let index = self.group_index(id)?;
        let members = self.group_members(id);
        self.capture_groups();
        self.toolpath_groups.remove(index);
        if delete_members {
            self.delete_toolpaths(&members)?;
        } else {
            for member in members {
                self.update_toolpath(member, |t| t.group = 0)?;
            }
        }
        Ok(())
    }

    /// Moves toolpaths (kept in their list order) into group `group` (zero
    /// for none), in front of toolpath `before`, or after the group's last
    /// member when `before` is `None` (the end of the list for no group or
    /// an empty one). Groups the toolpaths leave keep their other members.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown toolpath or group,
    /// or a `before` that is one of the toolpaths moved.
    pub fn place_toolpaths(
        &mut self,
        ids: &[u64],
        group: u64,
        before: Option<u64>,
    ) -> Result<(), CoreError> {
        for id in ids {
            self.toolpath(*id)?;
        }
        if group != 0 {
            self.group_index(group)?;
        }
        if let Some(b) = before {
            self.toolpath(b)?;
        }
        let moving: Vec<u64> = self
            .toolpath_order
            .iter()
            .copied()
            .filter(|t| ids.contains(t))
            .collect();
        let anchor = match before {
            Some(b) if moving.contains(&b) => {
                // In front of itself: find the next toolpath that stays.
                let at = self
                    .toolpath_order
                    .iter()
                    .position(|t| *t == b)
                    .unwrap_or(0);
                self.toolpath_order[at..]
                    .iter()
                    .copied()
                    .find(|t| !moving.contains(t))
            }
            Some(b) => Some(b),
            None if group != 0 => {
                let last = self
                    .group_members(group)
                    .into_iter()
                    .rfind(|t| !moving.contains(t));
                match last {
                    Some(last) => {
                        let at = self
                            .toolpath_order
                            .iter()
                            .position(|t| *t == last)
                            .unwrap_or(0);
                        self.toolpath_order[at + 1..]
                            .iter()
                            .copied()
                            .find(|t| !moving.contains(t))
                    }
                    None => None,
                }
            }
            None => None,
        };
        let mut order: Vec<u64> = self
            .toolpath_order
            .iter()
            .copied()
            .filter(|t| !moving.contains(t))
            .collect();
        let at = anchor
            .and_then(|a| order.iter().position(|t| *t == a))
            .unwrap_or(order.len());
        order.splice(at..at, moving.iter().copied());
        self.reorder_toolpaths(&order)?;
        let sheet = (group != 0)
            .then(|| self.toolpath_groups.iter().find(|g| g.id == group))
            .flatten()
            .map(|g| g.sheet)
            .filter(|s| *s != 0);
        for id in moving {
            self.update_toolpath(id, |t| {
                t.group = group;
                if let Some(sheet) = sheet {
                    t.sheet = sheet;
                }
            })?;
        }
        Ok(())
    }

    /// Moves toolpaths to another sheet. They leave any group that belongs
    /// to a different sheet.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown toolpath or sheet.
    pub fn set_toolpath_sheet(&mut self, ids: &[u64], sheet: u64) -> Result<(), CoreError> {
        self.sheet(sheet)?;
        for id in ids {
            self.toolpath(*id)?;
        }
        for id in ids {
            let group = self.toolpath(*id)?.group;
            let leaves = self
                .toolpath_groups
                .iter()
                .find(|g| g.id == group)
                .is_some_and(|g| g.sheet != 0 && g.sheet != sheet);
            self.update_toolpath(*id, |t| {
                t.sheet = sheet;
                if leaves {
                    t.group = 0;
                }
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{StrategyKind, Toolpath};
    use crate::doc::Document;

    fn doc_with(n: usize) -> (Document, Vec<u64>) {
        let mut doc = Document::new();
        doc.begin("add");
        let ids = (0..n)
            .map(|k| {
                doc.add_toolpath(Toolpath::new(
                    0,
                    format!("t{k}"),
                    Tool::default(),
                    StrategyKind::Profile,
                ))
            })
            .collect();
        doc.commit();
        (doc, ids)
    }

    fn order(doc: &Document) -> Vec<u64> {
        doc.toolpath_order().to_vec()
    }

    #[test]
    fn grouping_pulls_members_together_at_the_first() {
        let (mut doc, ids) = doc_with(4);
        doc.begin("group");
        let g = doc
            .add_toolpath_group("Roughing", 1, &[ids[3], ids[1]])
            .expect("groups");
        doc.commit();
        assert_eq!(order(&doc), vec![ids[0], ids[1], ids[3], ids[2]]);
        assert_eq!(doc.group_members(g), vec![ids[1], ids[3]]);
        doc.undo().expect("undo");
        assert!(doc.toolpath_groups().is_empty());
        assert_eq!(order(&doc), ids);
        assert_eq!(doc.toolpath(ids[1]).expect("t").group, 0);
    }

    #[test]
    fn toolpaths_move_in_and_out_of_groups() {
        let (mut doc, ids) = doc_with(4);
        doc.begin("group");
        let g = doc
            .add_toolpath_group("G", 1, &[ids[0], ids[1]])
            .expect("g");
        // Dropped on the group: after its last member.
        doc.place_toolpaths(&[ids[3]], g, None).expect("moves");
        assert_eq!(order(&doc), vec![ids[0], ids[1], ids[3], ids[2]]);
        // Dragged out in front of another toolpath.
        doc.place_toolpaths(&[ids[0]], 0, Some(ids[2]))
            .expect("moves");
        assert_eq!(order(&doc), vec![ids[1], ids[3], ids[0], ids[2]]);
        assert_eq!(doc.toolpath(ids[0]).expect("t").group, 0);
        doc.commit();
        // An empty group takes a toolpath at the end of the list.
        doc.begin("empty");
        let e = doc.add_toolpath_group("Empty", 1, &[]).expect("e");
        doc.place_toolpaths(&[ids[1]], e, None).expect("moves");
        doc.commit();
        assert_eq!(order(&doc).last(), Some(&ids[1]));
        assert_eq!(doc.group_members(e), vec![ids[1]]);
    }

    #[test]
    fn removing_a_group_keeps_or_deletes_its_toolpaths() {
        let (mut doc, ids) = doc_with(3);
        doc.begin("g");
        let g = doc
            .add_toolpath_group("G", 1, &[ids[0], ids[1]])
            .expect("g");
        doc.remove_toolpath_group(g, false).expect("ungroups");
        assert_eq!(doc.toolpaths().len(), 3);
        assert!(doc.toolpaths().iter().all(|t| t.group == 0));
        let g = doc
            .add_toolpath_group("G", 1, &[ids[0], ids[1]])
            .expect("g");
        doc.remove_toolpath_group(g, true).expect("deletes");
        doc.commit();
        assert_eq!(order(&doc), vec![ids[2]]);
        assert!(doc.remove_toolpath_group(99, false).is_err());
    }

    #[test]
    fn groups_rename_close_and_survive_a_recalculation() {
        let (mut doc, ids) = doc_with(1);
        doc.begin("g");
        let g = doc.add_toolpath_group("G", 1, &ids).expect("g");
        doc.update_toolpath_group(g, Some(" Finishing "), Some(false), None)
            .expect("updates");
        assert!(doc.update_toolpath_group(g, Some(" "), None, None).is_err());
        let fresh = Toolpath::new(0, "again", Tool::default(), StrategyKind::Profile);
        doc.replace_toolpath(ids[0], fresh).expect("replaces");
        doc.commit();
        assert_eq!(doc.toolpath_groups()[0].name, "Finishing");
        assert!(!doc.toolpath_groups()[0].expanded);
        assert_eq!(doc.toolpath(ids[0]).expect("t").group, g);
    }

    #[test]
    fn groups_save_and_open() {
        let (mut doc, ids) = doc_with(2);
        doc.begin("g");
        doc.add_toolpath_group("G", 1, &[ids[1]]).expect("g");
        doc.commit();
        let file = doc.to_file();
        let text = serde_json::to_string(&file).expect("saves");
        let back =
            Document::from_file(serde_json::from_str(&text).expect("reads"), 1).expect("opens");
        assert_eq!(back.toolpath_groups(), doc.toolpath_groups());
        assert_eq!(back.toolpath(ids[1]).expect("t").group, 1);
        // Files from before groups open with none.
        let mut old: serde_json::Value = serde_json::from_str(&text).expect("json");
        if let Some(map) = old.as_object_mut() {
            map.remove("toolpathGroups");
        }
        let back =
            Document::from_file(serde_json::from_value(old).expect("reads"), 1).expect("opens");
        assert!(back.toolpath_groups().is_empty());
    }
}
