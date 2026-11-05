//! Entity and selection commands: `entities.*` and `selection.*`.

use serde::Deserialize;
use serde_json::json;

use crate::doc::display::{DisplayArgs, DisplayCache};
use crate::doc::ops;
use crate::doc::probe::{self, HitTestArgs, ProbeArgs};
use crate::doc::{Document, Entity};
use crate::error::CoreError;
use crate::geom;
use crate::kernel::{edit, parse_args, IdArgs, IdsArgs, KernelReply};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "entities.setSheet"
            | "entities.list"
            | "entities.display"
            | "entities.get"
            | "entities.summary"
            | "entities.create"
            | "entities.replace"
            | "entities.delete"
            | "entities.transform"
            | "entities.setLayer"
            | "entities.group"
            | "entities.ungroup"
            | "entities.probe"
            | "entities.hitTest"
            | "selection.get"
            | "selection.set"
            | "selection.add"
            | "selection.remove"
            | "selection.toggle"
            | "selection.clear"
            | "selection.all"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(
    document: &mut Document,
    display: &mut DisplayCache,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "entities.setSheet" => {
            let args: SetSheetArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let components = args.components.unwrap_or_default();
            if ids.is_empty() && components.is_empty() {
                return Err(CoreError::validation("select the objects to move"));
            }
            edit(document, "Move to Sheet", |doc| {
                doc.set_entity_sheet(&ids, args.sheet)?;
                doc.set_component_sheet(&components, args.sheet)?;
                let active = doc.active_sheet();
                let kept: Vec<u64> = doc
                    .selection()
                    .iter()
                    .copied()
                    .filter(|id| doc.entity(*id).is_ok_and(|e| e.sheet == active))
                    .collect();
                doc.set_selection(&kept);
                Ok(json!({}))
            })
        }
        "entities.list" => {
            let args: SheetArgs = parse_args(name, args)?;
            KernelReply::ok(&document.list(args.sheet))
        }
        "entities.display" => {
            let args: DisplayArgs = parse_args(name, args)?;
            Ok(KernelReply::Bytes(display.build(document, &args)))
        }
        "entities.get" => {
            let args: IdArgs = parse_args(name, args)?;
            KernelReply::ok(&document.entity(args.id)?)
        }
        "entities.summary" => {
            let args: IdArgs = parse_args(name, args)?;
            KernelReply::ok(&document.summary(args.id)?)
        }
        "entities.create" => {
            let args: CreateArgs = parse_args(name, args)?;
            let layer = args.layer.unwrap_or(args.entity.layer);
            let layer = if layer == 0 {
                document.active_layer()
            } else {
                layer
            };
            document.layer(layer)?;
            let sheet = if args.entity.sheet == 0 {
                document.active_sheet()
            } else {
                args.entity.sheet
            };
            document.sheet(sheet)?;
            let mut entity = args.entity;
            entity.layer = layer;
            entity.sheet = sheet;
            entity.parent = None;
            edit(document, "Create", |doc| {
                let id = doc.insert_entity(entity);
                Ok(json!({ "id": id }))
            })
        }
        "entities.replace" => {
            let args: ReplaceArgs = parse_args(name, args)?;
            edit(document, "Edit", |doc| {
                doc.replace_entity(args.id, args.entity)?;
                Ok(json!({}))
            })
        }
        "entities.delete" => {
            let args: DeleteArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            if args.components.is_empty() {
                ops::require(document, &ids, "delete")?;
            }
            for id in &args.components {
                document.component(*id)?;
            }
            edit(document, "Delete", |doc| {
                doc.delete_entities(&ids);
                doc.delete_components(&args.components)?;
                Ok(json!({}))
            })
        }
        "entities.transform" => {
            let args: TransformArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            ops::require(document, &ids, "transform")?;
            let m = matrix(&args.matrix)?;
            edit(document, "Transform", |doc| {
                doc.transform_entities(&ids, &m);
                Ok(json!({}))
            })
        }
        "entities.setLayer" => {
            let args: SetLayerArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            ops::require(document, &ids, "move to another layer")?;
            edit(document, "Move To Layer", |doc| {
                doc.set_entity_layer(&ids, args.layer)?;
                Ok(json!({}))
            })
        }
        "entities.group" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Group", |doc| {
                let id = doc.group(&ids)?;
                doc.set_selection(&[id]);
                Ok(json!({ "id": id }))
            })
        }
        "entities.ungroup" => {
            let args: UngroupArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            ops::require(document, &ids, "ungroup")?;
            edit(document, "Ungroup", |doc| {
                let mut released = Vec::new();
                for id in ids {
                    let layer = doc.entity(id)?.layer;
                    let mut out = doc.ungroup(&[id]);
                    // Deep ungroup takes nested groups apart too, in place.
                    while args.deep && out.iter().any(|child| is_group(doc, *child)) {
                        let mut flat = Vec::with_capacity(out.len());
                        for child in out {
                            if is_group(doc, child) {
                                flat.extend(doc.ungroup(&[child]));
                            } else {
                                flat.push(child);
                            }
                        }
                        out = flat;
                    }
                    if args.to_group_layer {
                        doc.set_entity_layer(&out, layer)?;
                    }
                    released.extend(out);
                }
                doc.set_selection(&released);
                Ok(json!({ "ids": released }))
            })
        }
        "entities.probe" => {
            let args: ProbeArgs = parse_args(name, args)?;
            KernelReply::ok(&probe::probe(document, &args))
        }
        "entities.hitTest" => {
            let args: HitTestArgs = parse_args(name, args)?;
            KernelReply::ok(&json!({ "ids": probe::hit_test(document, &args) }))
        }
        "selection.get" => KernelReply::ok(&document.selection_state()),
        "selection.set" | "selection.add" | "selection.remove" | "selection.toggle"
        | "selection.clear" | "selection.all" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = args.ids.unwrap_or_default();
            match name {
                "selection.set" => document.set_selection(&ids),
                "selection.add" => document.add_selection(&ids),
                "selection.remove" => document.remove_selection(&ids),
                "selection.toggle" => document.toggle_selection(&ids),
                "selection.clear" => document.clear_selection(),
                _ => document.select_all(),
            }
            // Selection is not an undoable edit of its own, but the view
            // has to redraw, so the revision still moves.
            document.bump();
            KernelReply::ok(&document.selection_state())
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// Turns the six numbers of an API matrix into a transform.
fn matrix(values: &[f64; 6]) -> Result<geom::Mat3, CoreError> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation(
            "a transform matrix must be six finite numbers",
        ));
    }
    Ok(geom::Mat3 {
        a: values[0],
        b: values[1],
        c: values[2],
        d: values[3],
        e: values[4],
        f: values[5],
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SheetArgs {
    sheet: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSheetArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    sheet: u64,
    #[serde(default)]
    components: Option<Vec<u64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    entity: Entity,
    #[serde(default)]
    layer: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplaceArgs {
    id: u64,
    entity: Entity,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransformArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    matrix: [f64; 6],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UngroupArgs {
    ids: Option<Vec<u64>>,
    /// Also takes apart groups inside the groups.
    #[serde(default)]
    deep: bool,
    /// Puts the released objects on the group's layer instead of their own.
    #[serde(default)]
    to_group_layer: bool,
}

fn is_group(document: &Document, id: u64) -> bool {
    matches!(
        document.entity(id),
        Ok(Entity {
            kind: crate::doc::EntityKind::Group { .. },
            ..
        })
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetLayerArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    layer: u64,
}

/// `entities.delete`: the entities (the selection when absent) and the
/// relief components selected with them, removed in one step.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    components: Vec<u64>,
}
