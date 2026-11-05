//! Toolpath list commands: groups (folders) in the list, dragging toolpaths
//! in and out of them, and moving toolpaths to another sheet.
//!
//! - `cam.groups`: the groups, `[{id, name, sheet, expanded}]`.
//! - `cam.groups.create {name, ids, sheet?}`: a group holding `ids` (none
//!   for an empty group) on `sheet` (the active sheet when absent).
//! - `cam.groups.update {id, name?, expanded?, sheet?}`.
//! - `cam.groups.delete {id, deleteToolpaths}`: ungroups, or deletes the
//!   group's toolpaths with it.
//! - `cam.place {ids, group, before?}`: moves toolpaths into a group (0 for
//!   none) in front of toolpath `before`, or at the group's end.
//! - `cam.setSheet {ids, sheet}`: moves toolpaths to a sheet.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

/// Commands this module answers.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "cam.groups"
            | "cam.groups.create"
            | "cam.groups.update"
            | "cam.groups.delete"
            | "cam.place"
            | "cam.setSheet"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ids: Vec<u64>,
    #[serde(default)]
    sheet: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    expanded: Option<bool>,
    #[serde(default)]
    sheet: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteArgs {
    id: u64,
    #[serde(default)]
    delete_toolpaths: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceArgs {
    ids: Vec<u64>,
    #[serde(default)]
    group: u64,
    #[serde(default)]
    before: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SheetArgs {
    ids: Vec<u64>,
    sheet: u64,
}

fn parse<T: serde::de::DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        trimmed
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

/// Runs `action` as one undo step, answering its result with the new
/// revision, or putting everything back when it fails.
fn edit<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(result) => {
            let revision = document.commit();
            let mut object = match result {
                Value::Object(map) => map,
                _ => Map::new(),
            };
            object.insert("revision".to_string(), json!(revision));
            KernelReply::ok(&Value::Object(object))
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

/// A group name not yet taken: "Group 1", "Group 2", ...
fn next_name(document: &Document) -> String {
    (1..)
        .map(|n| format!("Group {n}"))
        .find(|name| document.toolpath_groups().iter().all(|g| &g.name != name))
        .unwrap_or_else(|| "Group".to_string())
}

/// Runs one of this module's commands.
///
/// # Errors
/// Returns [`CoreError::UnknownCommand`] for another command,
/// [`CoreError::InvalidArgs`] for bad arguments and
/// [`CoreError::Validation`] for unknown toolpaths, groups or sheets.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "cam.groups" => KernelReply::ok(&document.toolpath_groups()),
        "cam.groups.create" => {
            let args: CreateArgs = parse(name, args)?;
            let sheet = args.sheet.unwrap_or_else(|| document.active_sheet());
            let title = args
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| next_name(document));
            edit(document, "Group Toolpaths", |doc| {
                let id = doc.add_toolpath_group(&title, sheet, &args.ids)?;
                Ok(json!({ "id": id }))
            })
        }
        "cam.groups.update" => {
            let args: UpdateArgs = parse(name, args)?;
            if let Some(sheet) = args.sheet {
                document.sheet(sheet)?;
            }
            edit(document, "Edit Toolpath Group", |doc| {
                doc.update_toolpath_group(
                    args.id,
                    args.name.as_deref(),
                    args.expanded,
                    args.sheet,
                )?;
                Ok(json!({}))
            })
        }
        "cam.groups.delete" => {
            let args: DeleteArgs = parse(name, args)?;
            let label = if args.delete_toolpaths {
                "Delete Toolpath Group"
            } else {
                "Ungroup Toolpaths"
            };
            edit(document, label, |doc| {
                doc.remove_toolpath_group(args.id, args.delete_toolpaths)?;
                Ok(json!({}))
            })
        }
        "cam.place" => {
            let args: PlaceArgs = parse(name, args)?;
            if args.ids.is_empty() {
                return Err(CoreError::validation("no toolpaths were chosen to move"));
            }
            edit(document, "Move Toolpaths", |doc| {
                doc.place_toolpaths(&args.ids, args.group, args.before)?;
                Ok(json!({}))
            })
        }
        "cam.setSheet" => {
            let args: SheetArgs = parse(name, args)?;
            edit(document, "Move Toolpaths to Sheet", |doc| {
                doc.set_toolpath_sheet(&args.ids, args.sheet)?;
                Ok(json!({}))
            })
        }
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::{StrategyKind, Toolpath};

    fn json(reply: KernelReply) -> Value {
        match reply {
            KernelReply::Json(text) => {
                let envelope: Value = serde_json::from_str(&text).expect("json");
                envelope.get("result").cloned().unwrap_or(envelope)
            }
            KernelReply::Bytes(_) => panic!("json reply"),
        }
    }

    #[test]
    fn groups_are_made_named_and_listed() {
        let mut doc = Document::new();
        doc.begin("add");
        let a = doc.add_toolpath(Toolpath::new(
            0,
            "a",
            Tool::default(),
            StrategyKind::Profile,
        ));
        let b = doc.add_toolpath(Toolpath::new(0, "b", Tool::default(), StrategyKind::Pocket));
        doc.commit();
        let made = json(
            dispatch(
                &mut doc,
                "cam.groups.create",
                &format!(r#"{{"ids":[{b}]}}"#),
            )
            .expect("creates"),
        );
        let id = made["id"].as_u64().expect("id");
        dispatch(&mut doc, "cam.groups.create", "{}").expect("an empty group");
        let groups = json(dispatch(&mut doc, "cam.groups", "").expect("lists"));
        assert_eq!(groups[0]["name"], "Group 1");
        assert_eq!(groups[1]["name"], "Group 2");
        assert_eq!(doc.toolpath(b).expect("b").group, id);
        dispatch(
            &mut doc,
            "cam.place",
            &format!(r#"{{"ids":[{a}],"group":{id}}}"#),
        )
        .expect("places");
        assert_eq!(doc.group_members(id), vec![b, a]);
        dispatch(
            &mut doc,
            "cam.groups.update",
            &format!(r#"{{"id":{id},"name":"Rough"}}"#),
        )
        .expect("renames");
        dispatch(
            &mut doc,
            "cam.groups.delete",
            &format!(r#"{{"id":{id},"deleteToolpaths":false}}"#),
        )
        .expect("ungroups");
        assert_eq!(doc.toolpaths().len(), 2);
        assert!(dispatch(&mut doc, "cam.place", r#"{"ids":[]}"#).is_err());
        assert!(dispatch(
            &mut doc,
            "cam.setSheet",
            &format!(r#"{{"ids":[{a}],"sheet":99}}"#)
        )
        .is_err());
        let sheet = doc.active_sheet();
        dispatch(
            &mut doc,
            "cam.setSheet",
            &format!(r#"{{"ids":[{a}],"sheet":{sheet}}}"#),
        )
        .expect("moves to a sheet");
    }
}
