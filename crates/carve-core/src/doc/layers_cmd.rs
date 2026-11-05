//! Layer and sheet commands: `layers.*` and `sheets.*`.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cam::toolpath::Toolpath;
use crate::doc::ops;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::{edit, parse_args, IdArgs, IdsArgs, KernelReply, RenameArgs};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "layers.list"
            | "layers.add"
            | "layers.update"
            | "layers.delete"
            | "layers.duplicate"
            | "layers.setActive"
            | "layers.reorder"
            | "sheets.list"
            | "sheets.add"
            | "sheets.duplicate"
            | "sheets.delete"
            | "sheets.setActive"
            | "sheets.edit"
            | "sheets.rename"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "layers.list" => KernelReply::ok(&layer_list(document)),
        "layers.add" => {
            let args: LayerAddArgs = parse_args(name, args)?;
            if let Some(colour) = args.colour.as_deref() {
                if !crate::doc::is_hex_colour(colour) {
                    return Err(CoreError::validation(format!(
                        "a layer color must look like #rrggbb, got {colour}"
                    )));
                }
            }
            if let Some(before) = args.before {
                document.layer(before)?;
            }
            edit(document, "Add Layer", |doc| {
                let id = doc.add_layer(args.name, args.colour);
                if args.visible == Some(false) {
                    doc.update_layer(id, None, None, Some(false), None)?;
                }
                if args.active {
                    doc.set_active_layer(id)?;
                }
                if let Some(before) = args.before {
                    let mut ids: Vec<u64> = doc
                        .layers()
                        .iter()
                        .map(|l| l.id)
                        .filter(|l| *l != id)
                        .collect();
                    let at = ids.iter().position(|l| *l == before).unwrap_or(ids.len());
                    ids.insert(at, id);
                    doc.reorder_layers(&ids)?;
                }
                Ok(json!({ "id": id }))
            })
        }
        "layers.update" => {
            let args: LayerUpdateArgs = parse_args(name, args)?;
            edit(document, "Edit Layer", |doc| {
                doc.update_layer(args.id, args.name, args.colour, args.visible, args.locked)?;
                if let Some(keep_out) = args.keep_out {
                    doc.set_layer_keep_out(args.id, keep_out)?;
                }
                Ok(json!({}))
            })
        }
        "layers.delete" => {
            let args: LayerDeleteArgs = parse_args(name, args)?;
            edit(document, "Delete Layer", |doc| {
                doc.delete_layer(args.id, args.move_to)?;
                Ok(json!({}))
            })
        }
        "layers.duplicate" => {
            let args: IdArgs = parse_args(name, args)?;
            let source = document.layer(args.id)?.clone();
            let members: Vec<u64> = document
                .order()
                .iter()
                .copied()
                .filter(|id| {
                    document
                        .entities()
                        .get(id)
                        .is_some_and(|e| e.layer == source.id)
                })
                .collect();
            edit(document, "Duplicate Layer", |doc| {
                let id = doc.add_layer(
                    Some(format!("{} copy", source.name)),
                    Some(source.colour.clone()),
                );
                let mut copies = Vec::with_capacity(members.len());
                for member in &members {
                    copies.push(ops::duplicate(doc, *member)?);
                }
                if !copies.is_empty() {
                    doc.set_entity_layer(&copies, id)?;
                }
                doc.update_layer(id, None, None, Some(source.visible), Some(source.locked))?;
                Ok(json!({ "id": id }))
            })
        }
        "layers.setActive" => {
            let args: IdArgs = parse_args(name, args)?;
            edit(document, "Active Layer", |doc| {
                doc.set_active_layer(args.id)?;
                Ok(json!({}))
            })
        }
        "layers.reorder" => {
            let args: IdsArgs = parse_args(name, args)?;
            edit(document, "Reorder Layers", |doc| {
                doc.reorder_layers(&args.ids.unwrap_or_default())?;
                Ok(json!({}))
            })
        }
        "sheets.list" => KernelReply::ok(&sheet_list(document)),
        "sheets.add" => {
            let args: SheetAddArgs = parse_args(name, args)?;
            edit(document, "Add Sheet", |doc| {
                let id = doc.add_sheet(args.name);
                if args.activate {
                    doc.set_active_sheet(id)?;
                    doc.set_selection(&[]);
                }
                Ok(json!({ "id": id }))
            })
        }
        "sheets.duplicate" => {
            let args: IdArgs = parse_args(name, args)?;
            let source = document.sheet(args.id)?.name.clone();
            let taken: Vec<String> = document.sheets().iter().map(|s| s.name.clone()).collect();
            let mut copy_name = format!("{source} copy");
            let mut n = 2;
            while taken.contains(&copy_name) {
                copy_name = format!("{source} copy {n}");
                n += 1;
            }
            let contents: Vec<u64> = document
                .list(Some(args.id))
                .into_iter()
                .map(|e| e.id)
                .collect();
            let toolpaths: Vec<Toolpath> = document
                .toolpaths()
                .into_iter()
                .filter(|t| t.sheet == args.id)
                .cloned()
                .collect();
            edit(document, "Duplicate Sheet", |doc| {
                let id = doc.add_sheet(Some(copy_name));
                let mut copies = Vec::with_capacity(contents.len());
                for entity in &contents {
                    copies.push(ops::duplicate(doc, *entity)?);
                }
                doc.set_entity_sheet(&copies, id)?;
                doc.copy_sheet_contents(args.id, id)?;
                // The sheet's toolpaths come along, pointed at the copies
                // of their vectors, so they recalculate from the new sheet.
                let map: std::collections::HashMap<u64, u64> = contents
                    .iter()
                    .copied()
                    .zip(copies.iter().copied())
                    .collect();
                for mut toolpath in toolpaths {
                    if let Some(list) = toolpath.params_json["ids"].as_array_mut() {
                        for value in list.iter_mut() {
                            if let Some(new) = value.as_u64().and_then(|old| map.get(&old)) {
                                *value = json!(new);
                            }
                        }
                    }
                    for source in &mut toolpath.sources {
                        if let Some(new) = map.get(&source.id) {
                            source.id = *new;
                        }
                    }
                    toolpath.id = 0;
                    toolpath.sheet = id;
                    doc.add_toolpath(toolpath);
                }
                doc.set_active_sheet(id)?;
                doc.set_selection(&[]);
                Ok(json!({ "id": id, "ids": copies }))
            })
        }
        "sheets.delete" => {
            let args: IdArgs = parse_args(name, args)?;
            edit(document, "Delete Sheet", |doc| {
                doc.delete_sheet(args.id)?;
                Ok(json!({}))
            })
        }
        "sheets.setActive" => {
            let args: IdArgs = parse_args(name, args)?;
            edit(document, "Active Sheet", |doc| {
                doc.set_active_sheet(args.id)?;
                // A selection never spans sheets.
                let kept: Vec<u64> = doc
                    .selection()
                    .iter()
                    .copied()
                    .filter(|id| doc.entity(*id).is_ok_and(|e| e.sheet == args.id))
                    .collect();
                doc.set_selection(&kept);
                Ok(json!({}))
            })
        }
        "sheets.edit" => {
            let args: crate::doc::SheetEditArgs = parse_args(name, args)?;
            edit(document, "Edit Sheet", |doc| {
                for id in &args.ids {
                    let old = doc.sheet_material(*id)?.applied_to(doc.job_setup());
                    doc.set_sheet_material(*id, args.material)?;
                    if args.scale_design {
                        let new = doc.sheet_material(*id)?.applied_to(doc.job_setup());
                        crate::job_scale::scale_sheet_design(
                            doc,
                            *id,
                            crate::job_scale::material_bounds(&old),
                            crate::job_scale::material_bounds(&new),
                        )?;
                    }
                }
                Ok(json!({}))
            })
        }
        "sheets.rename" => {
            let args: RenameArgs = parse_args(name, args)?;
            edit(document, "Rename Sheet", |doc| {
                doc.rename_sheet(args.id, args.name)?;
                Ok(json!({}))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

pub(crate) fn layer_list(document: &Document) -> Vec<Value> {
    document
        .layers()
        .iter()
        .map(|layer| {
            json!({
                "id": layer.id,
                "name": layer.name,
                "colour": layer.colour,
                "visible": layer.visible,
                "locked": layer.locked,
                "keepOut": layer.keep_out,
                "active": layer.id == document.active_layer(),
                "count": document.layer_count(layer.id),
            })
        })
        .collect()
}

pub(crate) fn sheet_list(document: &Document) -> Vec<Value> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            json!({
                "id": sheet.id,
                "name": sheet.name,
                "active": sheet.id == document.active_sheet(),
                "count": document.list(Some(sheet.id)).len(),
                "components": document.components_on(sheet.id),
                "material": document.sheet_material(sheet.id).ok(),
            })
        })
        .collect()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LayerAddArgs {
    name: Option<String>,
    colour: Option<String>,
    /// Hidden when false; a new layer shows by default.
    visible: Option<bool>,
    /// Makes the new layer the active one.
    #[serde(default)]
    active: bool,
    /// Inserts the new layer just before this one in the list, else last.
    before: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerUpdateArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    colour: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    locked: Option<bool>,
    #[serde(default)]
    keep_out: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerDeleteArgs {
    id: u64,
    #[serde(default)]
    move_to: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SheetAddArgs {
    name: Option<String>,
    /// Make the new sheet the active one in the same undo step.
    activate: bool,
}
