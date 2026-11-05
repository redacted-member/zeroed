//! The tool database: a tree of groups holding tools, plus the material and
//! machine lists that scope a tool's feeds and speeds.
//!
//! The database lives in one JSON file. The shipped default database is
//! embedded in the binary and written out the first time the app runs. Where
//! that file sits is injectable so that tests never touch the real user
//! directory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::tool::Tool;

/// The shipped default database, written out on first run.
pub const DEFAULT_DATABASE_JSON: &str = include_str!("../../assets/tools/default.json");

/// File format version written into the database file. Version 2 added
/// list order, the active material, name formats, flutes and naming
/// variables; a version 1 file reads with those at their defaults.
pub const FORMAT_VERSION: u32 = 2;

/// Group id standing for the top level of the tree.
pub const ROOT_GROUP: u64 = 0;

/// A folder in the tool tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGroup {
    /// Database identity.
    pub id: u64,
    /// Name shown in the tree.
    pub name: String,
    /// Parent group, `None` for a group at the top level.
    pub parent: Option<u64>,
    /// Position among its sibling groups; ties fall back to the id.
    #[serde(default)]
    pub order: u64,
}

/// A tool together with the group it sits in, which is how the file stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTool {
    /// Group the tool sits in, [`ROOT_GROUP`] for the top level.
    #[serde(default)]
    group: u64,
    /// Position among the tools of its group; ties fall back to the id.
    #[serde(default)]
    order: u64,
    /// The tool itself.
    #[serde(flatten)]
    tool: Tool,
}

/// What the database file holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseFile {
    version: u32,
    next_id: u64,
    groups: Vec<ToolGroup>,
    tools: Vec<StoredTool>,
    #[serde(default)]
    materials: Vec<String>,
    #[serde(default)]
    machines: Vec<String>,
    #[serde(default)]
    active_material: String,
    #[serde(default)]
    name_formats: BTreeMap<String, String>,
}

/// The tool tree, its tools and the material and machine lists.
///
/// Everything is held in ordered maps and sorted lists, so that writing the
/// database twice gives the same bytes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolDatabase {
    next_id: u64,
    groups: BTreeMap<u64, ToolGroup>,
    tools: BTreeMap<u64, Tool>,
    tool_group: BTreeMap<u64, u64>,
    tool_order: BTreeMap<u64, u64>,
    materials: BTreeSet<String>,
    machines: BTreeSet<String>,
    active_material: String,
    name_formats: BTreeMap<String, String>,
}

/// How [`ToolDatabase::merge`] treats a tool both databases hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// The incoming tool and its cutting data replace what is there.
    Overwrite,
    /// Only cutting data for machine and material pairs the tool lacks is
    /// added; everything already there is kept.
    KeepExisting,
}

/// What a merge did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    /// Tools that were not there before.
    pub added: Vec<u64>,
    /// Tools that were there and took data from the other database.
    pub updated: Vec<u64>,
}

impl ToolDatabase {
    /// An empty database with no groups and no tools.
    pub fn empty() -> Self {
        ToolDatabase {
            next_id: 1,
            ..ToolDatabase::default()
        }
    }

    /// The database shipped with the app.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] if the embedded file is not readable,
    /// which would be a build problem rather than a user one.
    pub fn shipped_default() -> Result<Self, CoreError> {
        ToolDatabase::from_json(DEFAULT_DATABASE_JSON)
    }

    /// Reads a database from JSON.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the text is not a database.
    pub fn from_json(text: &str) -> Result<Self, CoreError> {
        let file: DatabaseFile = serde_json::from_str(text)
            .map_err(|error| CoreError::validation(format!("tool database: {error}")))?;
        if file.version > FORMAT_VERSION {
            return Err(CoreError::validation(format!(
                "tool database was written by a newer version of the app (format {})",
                file.version
            )));
        }
        let mut db = ToolDatabase {
            next_id: file.next_id.max(1),
            ..ToolDatabase::default()
        };
        for group in file.groups {
            db.next_id = db.next_id.max(group.id + 1);
            db.groups.insert(group.id, group);
        }
        for stored in file.tools {
            let StoredTool {
                group,
                order,
                mut tool,
            } = stored;
            db.tool_order.insert(tool.id, order);
            db.next_id = db.next_id.max(tool.id + 1);
            tool.sort_cutting_data();
            let group = if db.groups.contains_key(&group) {
                group
            } else {
                ROOT_GROUP
            };
            db.tool_group.insert(tool.id, group);
            db.tools.insert(tool.id, tool);
        }
        db.materials.extend(file.materials);
        db.machines.extend(file.machines);
        if db.materials.contains(&file.active_material) {
            db.active_material = file.active_material;
        }
        db.name_formats = file
            .name_formats
            .into_iter()
            .filter(|(_, format)| !format.trim().is_empty())
            .collect();
        db.repair_group_parents();
        Ok(db)
    }

    /// Writes the database as pretty printed JSON, in a stable order.
    ///
    /// # Errors
    /// Returns [`CoreError::Encode`] if serialisation fails.
    pub fn to_json(&self) -> Result<String, CoreError> {
        let file = DatabaseFile {
            version: FORMAT_VERSION,
            next_id: self.next_id,
            groups: self.groups.values().cloned().collect(),
            tools: self
                .tools
                .values()
                .map(|tool| StoredTool {
                    group: self.group_of(tool.id).unwrap_or(ROOT_GROUP),
                    order: self.tool_order.get(&tool.id).copied().unwrap_or(0),
                    tool: tool.clone(),
                })
                .collect(),
            materials: self.materials.iter().cloned().collect(),
            machines: self.machines.iter().cloned().collect(),
            active_material: self.active_material.clone(),
            name_formats: self.name_formats.clone(),
        };
        let mut text = serde_json::to_string_pretty(&file)?;
        text.push('\n');
        Ok(text)
    }

    /// Every group, ordered by id.
    pub fn groups(&self) -> impl Iterator<Item = &ToolGroup> {
        self.groups.values()
    }

    /// Every tool, ordered by id.
    pub fn tools(&self) -> impl Iterator<Item = &Tool> {
        self.tools.values()
    }

    /// How many tools the database holds.
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// The materials a tool can carry feeds and speeds for, sorted.
    pub fn materials(&self) -> impl Iterator<Item = &String> {
        self.materials.iter()
    }

    /// The machines a tool can carry feeds and speeds for, sorted.
    pub fn machines(&self) -> impl Iterator<Item = &String> {
        self.machines.iter()
    }

    /// One tool by id.
    pub fn tool(&self, id: u64) -> Option<&Tool> {
        self.tools.get(&id)
    }

    /// One group by id.
    pub fn group(&self, id: u64) -> Option<&ToolGroup> {
        self.groups.get(&id)
    }

    /// Which group a tool sits in.
    pub fn group_of(&self, tool_id: u64) -> Option<u64> {
        self.tool_group.get(&tool_id).copied()
    }

    /// Tools in one group, in list order.
    pub fn tools_in(&self, group: u64) -> impl Iterator<Item = &Tool> {
        let mut found: Vec<&Tool> = self
            .tools
            .values()
            .filter(|tool| self.group_of(tool.id) == Some(group))
            .collect();
        found.sort_by_key(|tool| (self.tool_order(tool.id), tool.id));
        found.into_iter()
    }

    /// Position of a tool among the tools of its group.
    pub fn tool_order(&self, id: u64) -> u64 {
        self.tool_order.get(&id).copied().unwrap_or(0)
    }

    /// Direct children of a group, in list order. Pass [`ROOT_GROUP`] for
    /// the groups at the top level.
    pub fn child_groups(&self, parent: u64) -> impl Iterator<Item = &ToolGroup> {
        let mut found: Vec<&ToolGroup> = self
            .groups
            .values()
            .filter(|group| group.parent.unwrap_or(ROOT_GROUP) == parent && group.id != parent)
            .collect();
        found.sort_by_key(|group| (group.order, group.id));
        found.into_iter()
    }

    /// The material toolpaths take their cutting data for, empty for none.
    pub fn active_material(&self) -> &str {
        &self.active_material
    }

    /// Makes a material the active one, or clears it with an empty name.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a material the list does not hold.
    pub fn set_active_material(&mut self, name: &str) -> Result<(), CoreError> {
        if !name.is_empty() && !self.materials.contains(name) {
            return Err(CoreError::validation(format!("no material named {name}")));
        }
        self.active_material = name.to_string();
        Ok(())
    }

    /// The name format tools of this shape start with: the user's, else the
    /// built in one.
    pub fn name_format(&self, type_name: &str) -> String {
        self.name_formats
            .get(type_name)
            .cloned()
            .unwrap_or_else(|| super::naming::builtin_format(type_name).to_string())
    }

    /// Every tool shape's name format, user formats over built in ones.
    pub fn name_formats(&self) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = [
            "endMill",
            "ballNose",
            "vBit",
            "engravingTool",
            "taperedBallNose",
            "radiusedEndMill",
            "formTool",
            "drill",
            "diamondDrag",
            "laserTool",
        ]
        .iter()
        .map(|kind| (kind.to_string(), self.name_format(kind)))
        .collect();
        for (kind, format) in &self.name_formats {
            out.insert(kind.clone(), format.clone());
        }
        out
    }

    /// Sets the default name format for a tool shape; an empty format goes
    /// back to the built in one. With `update_existing` every tool of that
    /// shape is renamed to the format. Answers the renamed tool ids.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown shape name.
    pub fn set_name_format(
        &mut self,
        type_name: &str,
        format: &str,
        update_existing: bool,
    ) -> Result<Vec<u64>, CoreError> {
        if !self.name_formats().contains_key(type_name) {
            return Err(CoreError::validation(format!(
                "no tool type named {type_name}"
            )));
        }
        let format = format.trim();
        if format.is_empty() {
            self.name_formats.remove(type_name);
        } else {
            self.name_formats
                .insert(type_name.to_string(), format.to_string());
        }
        let mut renamed = Vec::new();
        if update_existing {
            let format = self.name_format(type_name);
            for tool in self.tools.values_mut() {
                if tool.tool_type.type_name() == type_name {
                    tool.name = format.clone();
                    renamed.push(tool.id);
                }
            }
        }
        Ok(renamed)
    }

    /// A tool as a toolpath cuts with it on a machine and material: the
    /// cutting data of that pair applied and the name template filled in.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown tool.
    pub fn resolve(&self, id: u64, machine: &str, material: &str) -> Result<Tool, CoreError> {
        let tool = self
            .tools
            .get(&id)
            .ok_or_else(|| CoreError::validation(format!("no tool with id {id}")))?;
        Ok(self.resolve_tool(tool, machine, material))
    }

    /// [`ToolDatabase::resolve`] for a tool that need not be stored yet.
    pub fn resolve_tool(&self, tool: &Tool, machine: &str, material: &str) -> Tool {
        let mut resolved = tool.with_cutting_data(machine, material);
        let group = self
            .group_of(tool.id)
            .and_then(|g| self.groups.get(&g))
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let context = super::naming::NameContext {
            machine,
            material,
            group: &group,
        };
        resolved.name = super::naming::evaluate(&tool.name, &resolved, context);
        resolved
    }

    /// Path of group names from the top level down to `group`, which is what
    /// the tool list tooltip shows.
    pub fn group_path(&self, group: u64) -> Vec<String> {
        let mut names = Vec::new();
        let mut current = Some(group);
        let mut guard = 0;
        while let Some(id) = current {
            if id == ROOT_GROUP || guard > 64 {
                break;
            }
            match self.groups.get(&id) {
                Some(found) => {
                    names.push(found.name.clone());
                    current = found.parent;
                }
                None => break,
            }
            guard += 1;
        }
        names.reverse();
        names
    }

    /// Adds a group and answers its new id.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an empty name or an unknown
    /// parent.
    pub fn add_group(&mut self, name: &str, parent: Option<u64>) -> Result<u64, CoreError> {
        let name = check_name(name, "a group")?;
        let parent = self.check_parent(parent)?;
        let id = self.take_id();
        let order = self.next_group_order(parent.unwrap_or(ROOT_GROUP));
        self.groups.insert(
            id,
            ToolGroup {
                id,
                name,
                parent,
                order,
            },
        );
        Ok(id)
    }

    /// Renames a group.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group or empty name.
    pub fn rename_group(&mut self, id: u64, name: &str) -> Result<(), CoreError> {
        let name = check_name(name, "a group")?;
        let group = self
            .groups
            .get_mut(&id)
            .ok_or_else(|| CoreError::validation(format!("no tool group with id {id}")))?;
        group.name = name;
        Ok(())
    }

    /// Moves a group under a new parent, or to the top level with `None`.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group or a move that
    /// would put a group inside itself.
    pub fn move_group(&mut self, id: u64, parent: Option<u64>) -> Result<(), CoreError> {
        if !self.groups.contains_key(&id) {
            return Err(CoreError::validation(format!("no tool group with id {id}")));
        }
        let parent = self.check_parent(parent)?;
        if let Some(target) = parent {
            if target == id || self.ancestors(target).any(|a| a == id) {
                return Err(CoreError::validation(
                    "a group cannot be moved inside itself",
                ));
            }
        }
        if let Some(group) = self.groups.get_mut(&id) {
            group.parent = parent;
        }
        Ok(())
    }

    /// Deletes a group. Its tools and child groups move to `move_to` when one
    /// is given, and are deleted with it when it is `None`.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group or target.
    pub fn delete_group(&mut self, id: u64, move_to: Option<u64>) -> Result<(), CoreError> {
        if !self.groups.contains_key(&id) {
            return Err(CoreError::validation(format!("no tool group with id {id}")));
        }
        if let Some(target) = move_to {
            if target != ROOT_GROUP && !self.groups.contains_key(&target) {
                return Err(CoreError::validation(format!(
                    "no tool group with id {target}"
                )));
            }
            if target == id {
                return Err(CoreError::validation("a group cannot be moved into itself"));
            }
        }
        match move_to {
            Some(target) => {
                let parent = if target == ROOT_GROUP {
                    None
                } else {
                    Some(target)
                };
                let children: Vec<u64> = self.child_groups(id).map(|g| g.id).collect();
                for child in children {
                    if let Some(group) = self.groups.get_mut(&child) {
                        group.parent = parent;
                    }
                }
                let tools: Vec<u64> = self.tools_in(id).map(|t| t.id).collect();
                for tool in tools {
                    self.tool_group.insert(tool, target);
                }
            }
            None => {
                for group in self.subtree(id) {
                    let tools: Vec<u64> = self.tools_in(group).map(|t| t.id).collect();
                    for tool in tools {
                        self.tools.remove(&tool);
                        self.tool_group.remove(&tool);
                    }
                    self.groups.remove(&group);
                }
            }
        }
        self.groups.remove(&id);
        Ok(())
    }

    /// Creates or updates a tool and answers its id. A tool with id zero is
    /// created, any other id replaces the tool already there.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the tool is not machinable or
    /// the group does not exist.
    pub fn save_tool(&mut self, mut tool: Tool, group: u64) -> Result<u64, CoreError> {
        tool.validate()?;
        self.check_group(group)?;
        if tool.id == 0 {
            tool.id = self.take_id();
        } else {
            self.next_id = self.next_id.max(tool.id + 1);
        }
        tool.sort_cutting_data();
        let id = tool.id;
        if self.group_of(id) != Some(group) {
            let order = self.next_tool_order(group);
            self.tool_order.insert(id, order);
        }
        self.tool_group.insert(id, group);
        self.tools.insert(id, tool);
        Ok(id)
    }

    /// Deletes a tool. Answers whether one was there.
    pub fn delete_tool(&mut self, id: u64) -> bool {
        self.tool_group.remove(&id);
        self.tool_order.remove(&id);
        self.tools.remove(&id).is_some()
    }

    /// Puts a tool into `group` just before the tool `before`, or at the end
    /// of the group without one. This is what dragging a tool in the tree
    /// does.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown tool or group, or a `before`
    /// tool that is not in that group.
    pub fn place_tool(
        &mut self,
        id: u64,
        group: u64,
        before: Option<u64>,
    ) -> Result<(), CoreError> {
        if !self.tools.contains_key(&id) {
            return Err(CoreError::validation(format!("no tool with id {id}")));
        }
        self.check_group(group)?;
        if let Some(anchor) = before {
            if self.group_of(anchor) != Some(group) {
                return Err(CoreError::validation(
                    "a tool can only be placed next to a tool of the same group",
                ));
            }
        }
        let mut order: Vec<u64> = self
            .tools_in(group)
            .map(|t| t.id)
            .filter(|t| *t != id)
            .collect();
        let at = before
            .filter(|anchor| *anchor != id)
            .and_then(|anchor| order.iter().position(|t| *t == anchor))
            .unwrap_or(order.len());
        order.insert(at, id);
        self.tool_group.insert(id, group);
        for (index, tool) in order.into_iter().enumerate() {
            self.tool_order.insert(tool, index as u64 + 1);
        }
        Ok(())
    }

    /// Puts a group under `parent` just before the group `before`, or last.
    ///
    /// # Errors
    /// As [`ToolDatabase::move_group`], and for a `before` group that is not
    /// a child of `parent`.
    pub fn place_group(
        &mut self,
        id: u64,
        parent: Option<u64>,
        before: Option<u64>,
    ) -> Result<(), CoreError> {
        self.move_group(id, parent)?;
        let parent = self.check_parent(parent)?.unwrap_or(ROOT_GROUP);
        if let Some(anchor) = before {
            let anchor_parent = self
                .groups
                .get(&anchor)
                .map(|g| g.parent.unwrap_or(ROOT_GROUP));
            if anchor_parent != Some(parent) {
                return Err(CoreError::validation(
                    "a group can only be placed next to a group with the same parent",
                ));
            }
        }
        let mut order: Vec<u64> = self
            .child_groups(parent)
            .map(|g| g.id)
            .filter(|g| *g != id)
            .collect();
        let at = before
            .filter(|anchor| *anchor != id)
            .and_then(|anchor| order.iter().position(|g| *g == anchor))
            .unwrap_or(order.len());
        order.insert(at, id);
        for (index, group) in order.into_iter().enumerate() {
            if let Some(found) = self.groups.get_mut(&group) {
                found.order = index as u64 + 1;
            }
        }
        Ok(())
    }

    fn next_tool_order(&self, group: u64) -> u64 {
        self.tools_in(group)
            .map(|t| self.tool_order(t.id))
            .max()
            .map_or(0, |max| max + 1)
    }

    fn next_group_order(&self, parent: u64) -> u64 {
        self.child_groups(parent)
            .map(|g| g.order)
            .max()
            .map_or(0, |max| max + 1)
    }

    /// Moves a tool into another group.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown tool or group.
    pub fn move_tool(&mut self, id: u64, group: u64) -> Result<(), CoreError> {
        if !self.tools.contains_key(&id) {
            return Err(CoreError::validation(format!("no tool with id {id}")));
        }
        self.check_group(group)?;
        if self.group_of(id) != Some(group) {
            let order = self.next_tool_order(group);
            self.tool_order.insert(id, order);
        }
        self.tool_group.insert(id, group);
        Ok(())
    }

    /// Copies a tool into a group and answers the new tool's id. The copy
    /// keeps the name with " copy" appended when it lands in the same group.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown tool or group.
    pub fn copy_tool(&mut self, id: u64, group: Option<u64>) -> Result<u64, CoreError> {
        let source = self
            .tools
            .get(&id)
            .ok_or_else(|| CoreError::validation(format!("no tool with id {id}")))?;
        let from_group = self.group_of(id).unwrap_or(ROOT_GROUP);
        let target = group.unwrap_or(from_group);
        let mut copy = source.clone();
        copy.id = 0;
        if target == from_group {
            copy.name = format!("{} copy", copy.name);
        }
        self.save_tool(copy, target)
    }

    /// Merges another database into this one under `into_group`, giving every
    /// imported group and tool a fresh id. Answers the new tool ids.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the target group is unknown.
    /// Adds the tools of `defaults` that this database has no tool of the
    /// same name for, each into the group with the same path of names
    /// (created when missing), so that a database seeded by an older build
    /// gains the tools shipped since. Answers the new tool ids.
    ///
    /// # Errors
    /// Whatever [`ToolDatabase::add_group`] and [`ToolDatabase::save_tool`]
    /// report for a default that does not validate.
    pub fn add_missing_defaults(&mut self, defaults: &ToolDatabase) -> Result<Vec<u64>, CoreError> {
        let names: std::collections::BTreeSet<String> =
            self.tools.values().map(|t| t.name.clone()).collect();
        let mut added = Vec::new();
        for tool in defaults.tools.values() {
            if names.contains(&tool.name) {
                continue;
            }
            // Names of the tool's groups, outermost first.
            let mut path = Vec::new();
            let mut at = defaults.group_of(tool.id).unwrap_or(ROOT_GROUP);
            while let Some(group) = defaults.groups.get(&at) {
                path.push(group.name.clone());
                at = group.parent.unwrap_or(ROOT_GROUP);
            }
            path.reverse();
            let mut parent: Option<u64> = None;
            for name in &path {
                let found = self
                    .groups
                    .values()
                    .find(|g| g.parent == parent && &g.name == name)
                    .map(|g| g.id);
                parent = Some(match found {
                    Some(id) => id,
                    None => self.add_group(name, parent)?,
                });
            }
            let mut copy = tool.clone();
            copy.id = 0;
            added.push(self.save_tool(copy, parent.unwrap_or(ROOT_GROUP))?);
        }
        Ok(added)
    }

    pub fn import(&mut self, other: &ToolDatabase, into_group: u64) -> Result<Vec<u64>, CoreError> {
        self.check_group(into_group)?;
        let mut group_map: BTreeMap<u64, u64> = BTreeMap::new();
        for group in other.groups.values() {
            let id = self.take_id();
            group_map.insert(group.id, id);
        }
        for group in other.groups.values() {
            let Some(&id) = group_map.get(&group.id) else {
                continue;
            };
            let parent = match group.parent {
                Some(parent) => group_map.get(&parent).copied(),
                None if into_group == ROOT_GROUP => None,
                None => Some(into_group),
            };
            self.groups.insert(
                id,
                ToolGroup {
                    id,
                    name: group.name.clone(),
                    parent,
                    order: group.order,
                },
            );
        }
        let mut imported = Vec::new();
        for tool in other.tools.values() {
            let source_group = other.group_of(tool.id).unwrap_or(ROOT_GROUP);
            let target = group_map.get(&source_group).copied().unwrap_or(into_group);
            let mut copy = tool.clone();
            copy.id = 0;
            imported.push(self.save_tool(copy, target)?);
        }
        for material in &other.materials {
            self.materials.insert(material.clone());
        }
        for machine in &other.machines {
            self.machines.insert(machine.clone());
        }
        Ok(imported)
    }

    /// A database holding one group's subtree, ready to write out.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the group is unknown.
    pub fn export_group(&self, group: u64) -> Result<ToolDatabase, CoreError> {
        self.check_group(group)?;
        if group == ROOT_GROUP {
            return Ok(self.clone());
        }
        let mut out = ToolDatabase::empty();
        for id in self.subtree(group) {
            if let Some(found) = self.groups.get(&id) {
                let parent = if id == group { None } else { found.parent };
                out.groups.insert(
                    id,
                    ToolGroup {
                        id,
                        name: found.name.clone(),
                        parent,
                        order: found.order,
                    },
                );
                out.next_id = out.next_id.max(id + 1);
            }
            for tool in self.tools_in(id) {
                out.next_id = out.next_id.max(tool.id + 1);
                out.tool_group.insert(tool.id, id);
                out.tool_order.insert(tool.id, self.tool_order(tool.id));
                out.tools.insert(tool.id, tool.clone());
            }
        }
        out.materials = self.materials.clone();
        out.machines = self.machines.clone();
        out.name_formats = self.name_formats.clone();
        Ok(out)
    }

    /// A database holding just the given tools, at its top level, ready to
    /// write out.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown tool or an empty list.
    pub fn export_tools(&self, ids: &[u64]) -> Result<ToolDatabase, CoreError> {
        if ids.is_empty() {
            return Err(CoreError::validation("no tools were chosen to export"));
        }
        let mut out = ToolDatabase::empty();
        for (index, id) in ids.iter().enumerate() {
            let tool = self
                .tools
                .get(id)
                .ok_or_else(|| CoreError::validation(format!("no tool with id {id}")))?;
            out.next_id = out.next_id.max(tool.id + 1);
            out.tool_group.insert(tool.id, ROOT_GROUP);
            out.tool_order.insert(tool.id, index as u64 + 1);
            out.tools.insert(tool.id, tool.clone());
        }
        out.materials = self.materials.clone();
        out.machines = self.machines.clone();
        Ok(out)
    }

    /// Merges another database into this one. A tool is the same tool when
    /// it has the same name in a group with the same path of names; others
    /// are added in that path, creating groups as needed.
    ///
    /// # Errors
    /// Whatever [`ToolDatabase::save_tool`] reports for an incoming tool
    /// that does not validate.
    pub fn merge(
        &mut self,
        other: &ToolDatabase,
        mode: MergeMode,
    ) -> Result<MergeReport, CoreError> {
        let mut report = MergeReport::default();
        let incoming: Vec<u64> = other.tools.keys().copied().collect();
        for tool_id in incoming {
            let Some(tool) = other.tools.get(&tool_id) else {
                continue;
            };
            let path = other.group_path(other.group_of(tool_id).unwrap_or(ROOT_GROUP));
            let group = self.ensure_group_path(&path)?;
            let existing = self
                .tools_in(group)
                .find(|t| t.name == tool.name)
                .map(|t| t.id);
            match existing {
                Some(id) => {
                    let Some(mut current) = self.tools.get(&id).cloned() else {
                        continue;
                    };
                    match mode {
                        MergeMode::Overwrite => {
                            let kept = std::mem::take(&mut current.cutting_data);
                            current = tool.clone();
                            current.id = id;
                            current.cutting_data = kept;
                            for data in &tool.cutting_data {
                                current.set_cutting_data(data.clone());
                            }
                        }
                        MergeMode::KeepExisting => {
                            for data in &tool.cutting_data {
                                let there = current.cutting_data.iter().any(|d| {
                                    d.machine == data.machine && d.material == data.material
                                });
                                if !there {
                                    current.set_cutting_data(data.clone());
                                }
                            }
                        }
                    }
                    self.save_tool(current, group)?;
                    report.updated.push(id);
                }
                None => {
                    let mut copy = tool.clone();
                    copy.id = 0;
                    report.added.push(self.save_tool(copy, group)?);
                }
            }
        }
        self.materials.extend(other.materials.iter().cloned());
        self.machines.extend(other.machines.iter().cloned());
        Ok(report)
    }

    /// The group at a path of names from the top level, created where
    /// missing. An empty path is the top level.
    fn ensure_group_path(&mut self, path: &[String]) -> Result<u64, CoreError> {
        let mut parent: Option<u64> = None;
        for name in path {
            let found = self
                .groups
                .values()
                .find(|g| g.parent == parent && &g.name == name)
                .map(|g| g.id);
            parent = Some(match found {
                Some(id) => id,
                None => self.add_group(name, parent)?,
            });
        }
        Ok(parent.unwrap_or(ROOT_GROUP))
    }

    /// Adds a material holding a copy of another's cutting data for every
    /// tool when `copy_data` is set. Answers whether any data was copied.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown source or a name already
    /// taken.
    pub fn duplicate_material(
        &mut self,
        from: &str,
        to: &str,
        copy_data: bool,
    ) -> Result<bool, CoreError> {
        let to = check_name(to, "a material")?;
        if !self.materials.contains(from) {
            return Err(CoreError::validation(format!("no material named {from}")));
        }
        if self.materials.contains(&to) {
            return Err(CoreError::validation(format!(
                "two materials cannot both be named {to}"
            )));
        }
        self.materials.insert(to.clone());
        let mut copied = false;
        if copy_data {
            for tool in self.tools.values_mut() {
                let copies: Vec<_> = tool
                    .cutting_data
                    .iter()
                    .filter(|d| d.material == from)
                    .cloned()
                    .map(|mut d| {
                        d.material = to.clone();
                        d
                    })
                    .collect();
                for data in copies {
                    tool.set_cutting_data(data);
                    copied = true;
                }
            }
        }
        Ok(copied)
    }

    /// Adds a material to the list. Answers whether it was new.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an empty name.
    pub fn add_material(&mut self, name: &str) -> Result<bool, CoreError> {
        let name = check_name(name, "a material")?;
        Ok(self.materials.insert(name))
    }

    /// Removes a material and drops the cutting data that named it. Answers
    /// whether it was there.
    pub fn remove_material(&mut self, name: &str) -> bool {
        let found = self.materials.remove(name);
        if self.active_material == name {
            self.active_material = String::new();
        }
        for tool in self.tools.values_mut() {
            tool.cutting_data.retain(|data| data.material != name);
        }
        found
    }

    /// Renames a material, carrying the cutting data over.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an empty or unknown name.
    pub fn rename_material(&mut self, from: &str, to: &str) -> Result<(), CoreError> {
        let to = check_name(to, "a material")?;
        if to != from && self.materials.contains(&to) {
            return Err(CoreError::validation(format!(
                "two materials cannot both be named {to}"
            )));
        }
        if !self.materials.remove(from) {
            return Err(CoreError::validation(format!("no material named {from}")));
        }
        self.materials.insert(to.clone());
        if self.active_material == from {
            self.active_material = to.clone();
        }
        for tool in self.tools.values_mut() {
            for data in &mut tool.cutting_data {
                if data.material == from {
                    data.material = to.clone();
                }
            }
            tool.sort_cutting_data();
        }
        Ok(())
    }

    /// Adds a machine to the list. Answers whether it was new.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an empty name.
    pub fn add_machine(&mut self, name: &str) -> Result<bool, CoreError> {
        let name = check_name(name, "a machine")?;
        Ok(self.machines.insert(name))
    }

    /// Removes a machine and drops the cutting data that named it. Answers
    /// whether it was there.
    pub fn remove_machine(&mut self, name: &str) -> bool {
        let found = self.machines.remove(name);
        for tool in self.tools.values_mut() {
            tool.cutting_data.retain(|data| data.machine != name);
        }
        found
    }

    /// Renames a machine, carrying the cutting data over.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an empty or unknown name.
    pub fn rename_machine(&mut self, from: &str, to: &str) -> Result<(), CoreError> {
        let to = check_name(to, "a machine")?;
        if !self.machines.remove(from) {
            return Err(CoreError::validation(format!("no machine named {from}")));
        }
        self.machines.insert(to.clone());
        for tool in self.tools.values_mut() {
            for data in &mut tool.cutting_data {
                if data.machine == from {
                    data.machine = to.clone();
                }
            }
            tool.sort_cutting_data();
        }
        Ok(())
    }

    /// Group ids from `group` down through its children, deepest last.
    fn subtree(&self, group: u64) -> Vec<u64> {
        let mut out = vec![group];
        let mut index = 0;
        while index < out.len() {
            let current = out[index];
            for child in self.child_groups(current) {
                if !out.contains(&child.id) {
                    out.push(child.id);
                }
            }
            index += 1;
        }
        out
    }

    fn ancestors(&self, group: u64) -> impl Iterator<Item = u64> + '_ {
        let mut current = self.groups.get(&group).and_then(|g| g.parent);
        let mut guard = 0;
        std::iter::from_fn(move || {
            let id = current?;
            guard += 1;
            if guard > 64 {
                return None;
            }
            current = self.groups.get(&id).and_then(|g| g.parent);
            Some(id)
        })
    }

    fn check_group(&self, group: u64) -> Result<(), CoreError> {
        if group == ROOT_GROUP || self.groups.contains_key(&group) {
            Ok(())
        } else {
            Err(CoreError::validation(format!(
                "no tool group with id {group}"
            )))
        }
    }

    fn check_parent(&self, parent: Option<u64>) -> Result<Option<u64>, CoreError> {
        match parent {
            None | Some(ROOT_GROUP) => Ok(None),
            Some(id) if self.groups.contains_key(&id) => Ok(Some(id)),
            Some(id) => Err(CoreError::validation(format!("no tool group with id {id}"))),
        }
    }

    /// Drops parent links that point at a group that is not there, and breaks
    /// any cycle a hand edited file might carry.
    fn repair_group_parents(&mut self) {
        let known: BTreeSet<u64> = self.groups.keys().copied().collect();
        let ids: Vec<u64> = self.groups.keys().copied().collect();
        for id in &ids {
            let parent = self.groups.get(id).and_then(|g| g.parent);
            let broken = match parent {
                Some(parent) => !known.contains(&parent) || parent == *id,
                None => false,
            };
            if broken {
                if let Some(group) = self.groups.get_mut(id) {
                    group.parent = None;
                }
            }
        }
        for id in ids {
            if self.ancestors(id).any(|a| a == id) {
                if let Some(group) = self.groups.get_mut(&id) {
                    group.parent = None;
                }
            }
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }
}

/// Where the tool database file lives, and how it is read and written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStore {
    path: PathBuf,
}

impl ToolStore {
    /// A store that reads and writes the given file.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        ToolStore { path: path.into() }
    }

    /// The store in the user data directory, `CarveStudio/tools.json`. Falls
    /// back to the temporary directory on a platform that has no data
    /// directory, so that construction never fails.
    pub fn user_default() -> Self {
        let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        ToolStore::at(base.join("CarveStudio").join("tools.json"))
    }

    /// The file the store reads and writes.
    pub fn path(&self) -> &FsPath {
        &self.path
    }

    /// Reads the database, writing out the shipped default first if the file
    /// is not there yet.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be read or written and
    /// [`CoreError::Validation`] when it is not a database.
    pub fn load_or_seed(&self) -> Result<ToolDatabase, CoreError> {
        if self.path.exists() {
            self.load()
        } else {
            let db = ToolDatabase::shipped_default()?;
            self.save(&db)?;
            Ok(db)
        }
    }

    /// Reads the database.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be read and
    /// [`CoreError::Validation`] when it is not a database.
    pub fn load(&self) -> Result<ToolDatabase, CoreError> {
        let text = std::fs::read_to_string(&self.path)
            .map_err(|error| CoreError::io(self.path.display(), error))?;
        ToolDatabase::from_json(&text)
    }

    /// Writes the database, creating the directory if it is missing.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be written.
    pub fn save(&self, db: &ToolDatabase) -> Result<(), CoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CoreError::io(parent.display(), error))?;
        }
        let text = db.to_json()?;
        std::fs::write(&self.path, text).map_err(|error| CoreError::io(self.path.display(), error))
    }
}

impl Default for ToolStore {
    fn default() -> Self {
        ToolStore::user_default()
    }
}

fn check_name(name: &str, what: &str) -> Result<String, CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        Err(CoreError::validation(format!("{what} needs a name")))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::{CuttingData, FeedUnits, ToolGeometry};
    use crate::test_support::TempDir;

    fn tool(name: &str) -> Tool {
        Tool {
            name: name.to_string(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        }
    }

    fn small_db() -> ToolDatabase {
        let mut db = ToolDatabase::empty();
        let metric = db.add_group("Metric", None).expect("group");
        let mills = db.add_group("End Mills", Some(metric)).expect("group");
        db.save_tool(tool("End Mill 6 mm"), mills).expect("tool");
        db.save_tool(tool("End Mill 3 mm"), mills).expect("tool");
        db.save_tool(tool("Loose tool"), ROOT_GROUP).expect("tool");
        db
    }

    #[test]
    fn missing_default_tools_are_added_into_their_groups_once() {
        let defaults = ToolDatabase::shipped_default().expect("defaults");
        let mut db = small_db();
        let before = db.tool_count();
        let added = db.add_missing_defaults(&defaults).expect("adds");
        // The sample already has the two metric end mills by name.
        assert_eq!(added.len(), defaults.tool_count() - 2);
        assert_eq!(db.tool_count(), before + added.len());
        let laser = db
            .tools()
            .find(|t| t.name == "CO2 Laser 0.2 mm")
            .expect("the CO2 laser");
        let group = db.group_of(laser.id).expect("a group");
        let lasers = db.groups().find(|g| g.id == group).expect("group");
        assert_eq!(lasers.name, "Lasers");
        let parent = lasers.parent.and_then(|p| db.groups().find(|g| g.id == p));
        assert_eq!(parent.map(|g| g.name.as_str()), Some("Metric Tools"));
        assert!(db
            .add_missing_defaults(&defaults)
            .expect("again")
            .is_empty());
    }

    #[test]
    fn the_shipped_database_parses_and_every_tool_is_machinable() {
        let db = ToolDatabase::shipped_default().expect("shipped database parses");
        assert_eq!(db.tool_count(), 20);
        let lasers = db
            .tools()
            .filter(|t| {
                matches!(
                    t.tool_type,
                    crate::cam::tool::ToolGeometry::LaserTool { .. }
                )
            })
            .count();
        assert_eq!(lasers, 2, "a diode and a CO2 laser");
        for tool in db.tools() {
            tool.validate()
                .unwrap_or_else(|error| panic!("{}: {error}", tool.name));
            assert!(db.group_of(tool.id).is_some(), "{} has no group", tool.name);
        }
        assert!(db.materials().any(|m| m == "MDF"));
        assert!(db.machines().any(|m| m == "Default"));
    }

    #[test]
    fn the_shipped_database_covers_the_sizes_the_brief_asks_for() {
        let db = ToolDatabase::shipped_default().expect("shipped database parses");
        let names: Vec<&str> = db.tools().map(|t| t.name.as_str()).collect();
        for expected in [
            "End Mill 3 mm",
            "End Mill 6 mm",
            "End Mill 12 mm",
            "End Mill 1/8 inch",
            "End Mill 1/4 inch",
            "End Mill 1/2 inch",
            "Ball Nose 1.5 mm",
            "Ball Nose 3 mm",
            "Ball Nose 6 mm",
            "Ball Nose 1/8 inch",
            "Ball Nose 1/4 inch",
            "V Bit 60 deg",
            "V Bit 90 deg",
            "V Bit 120 deg",
            "Engraving 30 deg 0.1 mm tip",
            "Drill 3 mm",
            "Drill 6 mm",
            "Tapered Ball Nose 1/16 inch",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn groups_form_a_tree_with_a_readable_path() {
        let db = small_db();
        let metric = db
            .groups()
            .find(|g| g.name == "Metric")
            .expect("metric group");
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .expect("mills group");
        assert_eq!(mills.parent, Some(metric.id));
        assert_eq!(db.group_path(mills.id), vec!["Metric", "End Mills"]);
        assert_eq!(db.child_groups(ROOT_GROUP).count(), 1);
        assert_eq!(db.tools_in(mills.id).count(), 2);
        assert_eq!(db.tools_in(ROOT_GROUP).count(), 1);
    }

    #[test]
    fn saving_a_tool_gives_it_an_id_and_updating_keeps_it() {
        let mut db = ToolDatabase::empty();
        let id = db.save_tool(tool("cutter"), ROOT_GROUP).expect("tool");
        assert!(id > 0);
        let mut stored = db.tool(id).cloned().expect("tool is there");
        stored.name = "renamed".to_string();
        let again = db.save_tool(stored, ROOT_GROUP).expect("tool");
        assert_eq!(again, id);
        assert_eq!(db.tool_count(), 1);
        assert_eq!(db.tool(id).map(|t| t.name.as_str()), Some("renamed"));
    }

    #[test]
    fn saving_an_unmachinable_tool_is_refused() {
        let mut db = ToolDatabase::empty();
        let mut broken = tool("bad");
        broken.tool_type = ToolGeometry::EndMill { diameter: 0.0 };
        assert!(db.save_tool(broken, ROOT_GROUP).is_err());
        assert_eq!(db.tool_count(), 0);
        assert!(db.save_tool(tool("good"), 99).is_err());
    }

    #[test]
    fn tools_move_copy_and_delete() {
        let mut db = small_db();
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .map(|g| g.id)
            .expect("mills group");
        let loose = db
            .tools()
            .find(|t| t.name == "Loose tool")
            .map(|t| t.id)
            .expect("loose tool");
        db.move_tool(loose, mills).expect("move");
        assert_eq!(db.group_of(loose), Some(mills));
        let copy = db.copy_tool(loose, None).expect("copy");
        assert_eq!(
            db.tool(copy).map(|t| t.name.as_str()),
            Some("Loose tool copy")
        );
        let elsewhere = db.copy_tool(loose, Some(ROOT_GROUP)).expect("copy");
        assert_eq!(
            db.tool(elsewhere).map(|t| t.name.as_str()),
            Some("Loose tool")
        );
        assert!(db.delete_tool(loose));
        assert!(!db.delete_tool(loose));
        assert!(db.move_tool(loose, mills).is_err());
    }

    #[test]
    fn deleting_a_group_can_move_or_drop_its_contents() {
        let mut db = small_db();
        let metric = db
            .groups()
            .find(|g| g.name == "Metric")
            .map(|g| g.id)
            .expect("metric");
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .map(|g| g.id)
            .expect("mills");
        db.delete_group(mills, Some(metric)).expect("delete");
        assert_eq!(db.tools_in(metric).count(), 2);
        assert!(db.group(mills).is_none());
        db.delete_group(metric, None).expect("delete");
        assert_eq!(db.tool_count(), 1);
        assert!(db.delete_group(metric, None).is_err());
    }

    #[test]
    fn a_group_cannot_be_moved_inside_itself() {
        let mut db = small_db();
        let metric = db
            .groups()
            .find(|g| g.name == "Metric")
            .map(|g| g.id)
            .expect("metric");
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .map(|g| g.id)
            .expect("mills");
        assert!(db.move_group(metric, Some(mills)).is_err());
        assert!(db.move_group(metric, Some(metric)).is_err());
        db.move_group(mills, None).expect("move to the top level");
        assert_eq!(db.group_path(mills), vec!["End Mills"]);
    }

    #[test]
    fn materials_and_machines_carry_their_cutting_data() {
        let mut db = ToolDatabase::empty();
        let id = db.save_tool(tool("cutter"), ROOT_GROUP).expect("tool");
        assert!(db.add_material("Oak").expect("material"));
        assert!(!db.add_material("Oak").expect("material"));
        assert!(db.add_machine("Shark").expect("machine"));
        assert!(db.add_material("  ").is_err());
        if let Some(found) = db.tools.get_mut(&id) {
            found.set_cutting_data(CuttingData {
                machine: "Shark".to_string(),
                material: "Oak".to_string(),
                feed_units: FeedUnits::MmPerMin,
                feed_rate: 1500.0,
                plunge_rate: 400.0,
                spindle_speed: 16000.0,
                pass_depth: 2.0,
                stepover: 1.5,
                number: None,
            });
        }
        db.rename_material("Oak", "White Oak").expect("rename");
        assert_eq!(
            db.tool(id).map(|t| t.cutting_data[0].material.as_str()),
            Some("White Oak")
        );
        db.rename_machine("Shark", "Shark HD").expect("rename");
        assert_eq!(
            db.tool(id).map(|t| t.cutting_data[0].machine.as_str()),
            Some("Shark HD")
        );
        assert!(db.remove_material("White Oak"));
        assert_eq!(db.tool(id).map(|t| t.cutting_data.len()), Some(0));
        assert!(db.remove_machine("Shark HD"));
        assert!(db.rename_material("Gone", "Here").is_err());
        assert!(db.rename_machine("Gone", "Here").is_err());
    }

    #[test]
    fn json_round_trips_and_is_stable() {
        let db = small_db();
        let text = db.to_json().expect("json");
        let back = ToolDatabase::from_json(&text).expect("parses");
        assert_eq!(back, db);
        assert_eq!(back.to_json().expect("json"), text);
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn a_newer_format_version_is_refused() {
        let text = "{\"version\":99,\"nextId\":1,\"groups\":[],\"tools\":[]}";
        assert!(ToolDatabase::from_json(text).is_err());
        assert!(ToolDatabase::from_json("not json").is_err());
    }

    #[test]
    fn a_broken_group_tree_is_repaired_on_load() {
        let text = r#"{
            "version": 1, "nextId": 5,
            "groups": [
                {"id": 1, "name": "one", "parent": 2},
                {"id": 2, "name": "two", "parent": 1},
                {"id": 3, "name": "three", "parent": 99}
            ],
            "tools": []
        }"#;
        let db = ToolDatabase::from_json(text).expect("parses");
        assert_eq!(db.group_path(3), vec!["three"]);
        // One of the two in the cycle is cut loose, so no lookup can loop.
        assert!(db.group_path(1).len() <= 2);
        assert!(db.group_path(2).len() <= 2);
    }

    #[test]
    fn a_tool_in_an_unknown_group_lands_at_the_top_level() {
        let text = r#"{
            "version": 1, "nextId": 5, "groups": [], "tools": [
                {"group": 42, "id": 3, "name": "orphan",
                 "toolType": {"kind": "endMill", "diameter": 6.0}, "notes": "",
                 "units": "mm", "number": 1, "feedUnits": "mmPerMin",
                 "feedRate": 1000.0, "plungeRate": 300.0, "spindleSpeed": 12000.0,
                 "passDepth": 1.0, "stepover": 2.0, "stepoverPercent": 33.0,
                 "clearance": 0.0, "cuttingData": []}
            ]
        }"#;
        let db = ToolDatabase::from_json(text).expect("parses");
        assert_eq!(db.group_of(3), Some(ROOT_GROUP));
    }

    #[test]
    fn import_renumbers_and_export_takes_a_subtree() {
        let mut db = small_db();
        let other = small_db();
        let before = db.tool_count();
        let imported = db.import(&other, ROOT_GROUP).expect("import");
        assert_eq!(imported.len(), 3);
        assert_eq!(db.tool_count(), before + 3);
        // Nothing was overwritten, so every id is still unique.
        let ids: BTreeSet<u64> = db.tools().map(|t| t.id).collect();
        assert_eq!(ids.len(), db.tool_count());

        let metric = other
            .groups()
            .find(|g| g.name == "Metric")
            .map(|g| g.id)
            .expect("metric");
        let exported = other.export_group(metric).expect("export");
        assert_eq!(exported.tool_count(), 2);
        assert_eq!(exported.child_groups(ROOT_GROUP).count(), 1);
        assert!(other.export_group(99).is_err());
    }

    #[test]
    fn import_into_a_group_reparents_the_top_level() {
        let mut db = small_db();
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .map(|g| g.id)
            .expect("mills");
        let other = small_db();
        db.import(&other, mills).expect("import");
        let imported_metric = db
            .groups()
            .filter(|g| g.name == "Metric")
            .map(|g| g.parent)
            .collect::<Vec<_>>();
        assert!(imported_metric.contains(&Some(mills)));
    }

    #[test]
    fn the_store_seeds_the_default_database_on_first_run() {
        let temp = TempDir::new("tooldb-seed");
        let store = ToolStore::at(temp.path().join("nested").join("tools.json"));
        assert!(!store.path().exists());
        let db = store.load_or_seed().expect("seeds");
        assert!(store.path().exists());
        assert_eq!(db.tool_count(), 20);
        let again = store.load_or_seed().expect("loads");
        assert_eq!(again, db);
    }

    #[test]
    fn the_store_round_trips_edits() {
        let temp = TempDir::new("tooldb-save");
        let store = ToolStore::at(temp.path().join("tools.json"));
        let mut db = store.load_or_seed().expect("seeds");
        let id = db.save_tool(tool("Mine"), ROOT_GROUP).expect("tool");
        store.save(&db).expect("save");
        let back = store.load().expect("load");
        assert_eq!(back.tool(id).map(|t| t.name.as_str()), Some("Mine"));
        assert_eq!(back, db);
    }

    #[test]
    fn the_store_reports_a_missing_file() {
        let temp = TempDir::new("tooldb-missing");
        let store = ToolStore::at(temp.path().join("gone.json"));
        assert!(store.load().is_err());
    }

    #[test]
    fn the_user_store_sits_under_the_app_name() {
        let store = ToolStore::user_default();
        let text = store.path().display().to_string();
        assert!(text.ends_with("tools.json"), "{text}");
        assert!(text.contains("CarveStudio"), "{text}");
    }

    fn data(machine: &str, material: &str, feed: f64) -> CuttingData {
        CuttingData {
            machine: machine.to_string(),
            material: material.to_string(),
            feed_units: FeedUnits::MmPerMin,
            feed_rate: feed,
            plunge_rate: 300.0,
            spindle_speed: 16000.0,
            pass_depth: 2.0,
            stepover: 2.0,
            number: None,
        }
    }

    #[test]
    fn a_version_one_file_loads_with_the_new_fields_at_their_defaults() {
        let text = r#"{
            "version": 1, "nextId": 9,
            "groups": [{"id": 1, "name": "Mills", "parent": null}],
            "tools": [
                {"group": 1, "id": 3, "name": "old",
                 "toolType": {"kind": "endMill", "diameter": 6.0}, "notes": "",
                 "units": "mm", "number": 1, "feedUnits": "mmPerMin",
                 "feedRate": 1000.0, "plungeRate": 300.0, "spindleSpeed": 12000.0,
                 "passDepth": 1.0, "stepover": 2.0, "stepoverPercent": 33.0,
                 "clearance": 0.0, "cuttingData": [
                    {"machine": "A", "material": "Oak", "feedUnits": "mmPerMin",
                     "feedRate": 900.0, "plungeRate": 200.0, "spindleSpeed": 10000.0,
                     "passDepth": 1.0, "stepover": 1.0}
                 ]}
            ],
            "materials": ["Oak"], "machines": ["A"]
        }"#;
        let db = ToolDatabase::from_json(text).expect("a version 1 file loads");
        let tool = db.tool(3).expect("tool");
        assert_eq!(tool.flutes, 0);
        assert!(tool.variables.is_empty());
        assert_eq!(tool.cutting_data[0].number, None);
        assert_eq!(db.active_material(), "");
        let saved = db.to_json().expect("json");
        assert!(saved.contains("\"version\": 2"), "{saved}");
        assert_eq!(ToolDatabase::from_json(&saved).expect("again"), db);
    }

    #[test]
    fn tools_and_groups_can_be_placed_in_any_order() {
        let mut db = small_db();
        let mills = db
            .groups()
            .find(|g| g.name == "End Mills")
            .map(|g| g.id)
            .expect("mills");
        let names = |db: &ToolDatabase| -> Vec<String> {
            db.tools_in(mills).map(|t| t.name.clone()).collect()
        };
        assert_eq!(names(&db), vec!["End Mill 6 mm", "End Mill 3 mm"]);
        let three = db
            .tools()
            .find(|t| t.name == "End Mill 3 mm")
            .map(|t| t.id)
            .expect("three");
        let six = db
            .tools()
            .find(|t| t.name == "End Mill 6 mm")
            .map(|t| t.id)
            .expect("six");
        db.place_tool(three, mills, Some(six)).expect("place");
        assert_eq!(names(&db), vec!["End Mill 3 mm", "End Mill 6 mm"]);
        let loose = db
            .tools()
            .find(|t| t.name == "Loose tool")
            .map(|t| t.id)
            .expect("loose");
        db.place_tool(loose, mills, Some(six))
            .expect("drag into a group");
        assert_eq!(
            names(&db),
            vec!["End Mill 3 mm", "Loose tool", "End Mill 6 mm"]
        );
        assert!(db.place_tool(loose, ROOT_GROUP, Some(six)).is_err());
        // The order survives a round trip through the file.
        let back = ToolDatabase::from_json(&db.to_json().expect("json")).expect("parses");
        assert_eq!(names(&back), names(&db));

        let first = db.add_group("First", None).expect("group");
        let metric = db
            .groups()
            .find(|g| g.name == "Metric")
            .map(|g| g.id)
            .expect("metric");
        db.place_group(first, None, Some(metric)).expect("place");
        let top: Vec<&str> = db
            .child_groups(ROOT_GROUP)
            .map(|g| g.name.as_str())
            .collect();
        assert_eq!(top, vec!["First", "Metric"]);
        assert!(db.place_group(metric, Some(mills), None).is_err());
    }

    #[test]
    fn the_active_material_follows_renames_and_deletes() {
        let mut db = ToolDatabase::empty();
        db.add_material("Oak").expect("material");
        assert!(db.set_active_material("Pine").is_err());
        db.set_active_material("Oak").expect("activate");
        db.rename_material("Oak", "Red Oak").expect("rename");
        assert_eq!(db.active_material(), "Red Oak");
        db.add_material("Ash").expect("material");
        assert!(db.rename_material("Ash", "Red Oak").is_err());
        db.remove_material("Red Oak");
        assert_eq!(db.active_material(), "");
    }

    #[test]
    fn duplicating_a_material_can_copy_its_cutting_data() {
        let mut db = ToolDatabase::empty();
        let mut cutter = tool("cutter");
        cutter.set_cutting_data(data("Shop", "Oak", 1500.0));
        let id = db.save_tool(cutter, ROOT_GROUP).expect("tool");
        db.add_material("Oak").expect("material");
        assert!(db.duplicate_material("Oak", "Ash", true).expect("dup"));
        let copied = db.tool(id).expect("tool").cutting_data_for("Shop", "Ash");
        assert_eq!(copied.feed_rate, 1500.0);
        assert!(!db.duplicate_material("Oak", "Elm", false).expect("dup"));
        assert!(db.duplicate_material("Oak", "Ash", false).is_err());
        assert!(db.duplicate_material("Gone", "New", false).is_err());
    }

    #[test]
    fn a_tool_resolves_its_cutting_data_and_name_for_a_pair() {
        let mut db = ToolDatabase::empty();
        let group = db.add_group("Mills", None).expect("group");
        let mut cutter = tool("{Tool Type} {Diameter} {Material Name} {Group Name}");
        let mut oak = data("Shop", "Oak", 1500.0);
        oak.number = Some(7);
        cutter.set_cutting_data(oak);
        let id = db.save_tool(cutter, group).expect("tool");
        let resolved = db.resolve(id, "Shop", "Oak").expect("resolves");
        assert_eq!(resolved.name, "End Mill 6 Oak Mills");
        assert_eq!(resolved.feed_rate, 1500.0);
        assert_eq!(resolved.number, 7);
        let plain = db.resolve(id, "", "").expect("resolves");
        assert_eq!(plain.name, "End Mill 6 {Material Name} Mills");
        assert_eq!(plain.number, 1);
        assert!(db.resolve(999, "", "").is_err());
    }

    #[test]
    fn name_formats_default_per_type_and_can_rename_existing_tools() {
        let mut db = small_db();
        assert_eq!(
            db.name_format("endMill"),
            "{Tool Type} ({Diameter} {Units Short})"
        );
        let renamed = db
            .set_name_format("endMill", "{Tool Type} {Diameter|0.0}", true)
            .expect("set");
        assert_eq!(renamed.len(), 3);
        let id = renamed[0];
        assert_eq!(
            db.resolve(id, "", "").expect("resolves").name,
            "End Mill 6.0"
        );
        db.set_name_format("endMill", "", false).expect("reset");
        assert_eq!(
            db.name_format("endMill"),
            "{Tool Type} ({Diameter} {Units Short})"
        );
        assert!(db.set_name_format("spoon", "x", false).is_err());
        let text = db.to_json().expect("json");
        assert_eq!(ToolDatabase::from_json(&text).expect("parses"), db);
    }

    #[test]
    fn merging_overwrites_or_keeps_existing_cutting_data() {
        let mut ours = small_db();
        let mut theirs = small_db();
        let six = theirs
            .tools()
            .find(|t| t.name == "End Mill 6 mm")
            .map(|t| t.id)
            .expect("six");
        let group = theirs.group_of(six).expect("group");
        let mut changed = theirs.tool(six).cloned().expect("tool");
        changed.notes = "theirs".to_string();
        changed.set_cutting_data(data("Shop", "Oak", 2000.0));
        changed.set_cutting_data(data("Shop", "Pine", 2500.0));
        theirs.save_tool(changed, group).expect("save");
        let extra = theirs.add_group("Extra", None).expect("group");
        theirs.save_tool(tool("New one"), extra).expect("tool");

        let ours_six = ours
            .tools()
            .find(|t| t.name == "End Mill 6 mm")
            .map(|t| t.id)
            .expect("six");
        let mut mine = ours.tool(ours_six).cloned().expect("tool");
        mine.set_cutting_data(data("Shop", "Oak", 1000.0));
        let our_group = ours.group_of(ours_six).expect("group");
        ours.save_tool(mine, our_group).expect("save");

        let mut kept = ours.clone();
        let report = kept.merge(&theirs, MergeMode::KeepExisting).expect("merge");
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.updated.len(), 3);
        let tool = kept.tool(ours_six).expect("tool");
        assert_eq!(tool.cutting_data_for("Shop", "Oak").feed_rate, 1000.0);
        assert_eq!(tool.cutting_data_for("Shop", "Pine").feed_rate, 2500.0);
        assert_eq!(tool.notes, "");
        assert!(kept.groups().any(|g| g.name == "Extra"));

        let report = ours.merge(&theirs, MergeMode::Overwrite).expect("merge");
        assert_eq!(report.added.len(), 1);
        let tool = ours.tool(ours_six).expect("tool");
        assert_eq!(tool.cutting_data_for("Shop", "Oak").feed_rate, 2000.0);
        assert_eq!(tool.notes, "theirs");
        assert_eq!(tool.id, ours_six);
    }

    #[test]
    fn single_tools_can_be_exported() {
        let db = small_db();
        let ids: Vec<u64> = db.tools().take(2).map(|t| t.id).collect();
        let out = db.export_tools(&ids).expect("export");
        assert_eq!(out.tool_count(), 2);
        assert_eq!(out.tools_in(ROOT_GROUP).count(), 2);
        assert!(db.export_tools(&[]).is_err());
        assert!(db.export_tools(&[999]).is_err());
    }
}
