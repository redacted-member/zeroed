//! Toolpath commands on the document: calculate, merge, recalculate, list,
//! edit and delete toolpaths, and save or apply toolpath templates.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cam::compute::{self as cam_compute, Request};
use crate::cam::template::{Template, TemplateEntry};
use crate::cam::toolpath::Toolpath;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::{edit, parse_args, IdArgs, IdsArgs, KernelReply, PathArgs, RenameArgs};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "cam.compute"
            | "cam.duplicate"
            | "cam.depthCheck"
            | "cam.merge"
            | "cam.arrayCopy"
            | "cam.recalculate"
            | "cam.list"
            | "cam.sketchPreview"
            | "cam.prismDepth"
            | "cam.tabs"
            | "cam.selectVectors"
            | "cam.get"
            | "cam.delete"
            | "cam.rename"
            | "cam.setVisible"
            | "cam.reorder"
            | "cam.saveTemplate"
            | "cam.loadTemplate"
            | "cam.templateLayers"
            | "cam.inlayPlugVectors"
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
        "cam.compute" => {
            let request: Request = parse_args(name, args)?;
            let placement: PlacementArgs = parse_args(name, args)?;
            edit(document, "Calculate Toolpath", |doc| {
                let done = cam_compute::calculate_parts(doc, &request)?;
                let added = add_parts(doc, done.clearance, done.main, done.rest)?;
                if let Some(group) = placement.selected_group {
                    into_selected_group(doc, &added.all(), group)?;
                }
                Ok(json!({
                    "id": added.id,
                    "clearanceId": added.clearance_id,
                    "restIds": added.rest_ids,
                    "warnings": done.warnings,
                }))
            })
        }
        "cam.duplicate" => {
            let args: DuplicateArgs = parse_args(name, args)?;
            let original = document.toolpath(args.id)?.clone();
            let mut request = Request::of(&original)?;
            let trimmed = args.name.trim();
            request.name = Some(if trimmed.is_empty() {
                format!("{} Copy", original.name)
            } else {
                trimmed.to_string()
            });
            edit(document, "Duplicate Toolpath", |doc| {
                let done = cam_compute::calculate_parts(doc, &request)?;
                let added = add_parts(doc, done.clearance, done.main, done.rest)?;
                let ids = added.all();
                for id in &ids {
                    doc.update_toolpath(*id, |t| t.sheet = original.sheet)?;
                }
                if !doc.cam_settings().append_duplicated {
                    beside(doc, &ids, &original)?;
                }
                Ok(json!({
                    "id": added.id,
                    "clearanceId": added.clearance_id,
                    "restIds": added.rest_ids,
                    "warnings": done.warnings,
                }))
            })
        }
        "cam.inlayPlugVectors" => {
            let args: PlugVectorsArgs = parse_args(name, args)?;
            edit(document, "Create Plug Vectors", |doc| {
                let ids = crate::cam::inlay::plug_vectors(doc, &args.ids, args.sheet)?;
                Ok(json!({ "ids": ids }))
            })
        }
        "cam.depthCheck" => {
            let all = document.cam_settings().check_all_depths;
            let found = crate::cam::depth::too_deep(document, all);
            KernelReply::ok(&json!({ "all": all, "toolpaths": found }))
        }
        "cam.merge" => {
            let args: MergeArgs = parse_args(name, args)?;
            let chosen: Vec<u64> = document
                .toolpaths()
                .iter()
                .map(|t| t.id)
                .filter(|id| args.ids.contains(id))
                .collect();
            for id in &args.ids {
                document.toolpath(*id)?;
            }
            let parts: Vec<&Toolpath> = chosen
                .iter()
                .filter_map(|id| document.toolpath(*id).ok())
                .collect();
            let name_given = args
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Merged Toolpath".to_string());
            let merged = crate::cam::merge::merge_ordered(&parts, &name_given, &args.options)?;
            edit(document, "Merge Toolpaths", |doc| {
                for id in &chosen {
                    doc.update_toolpath(*id, |t| t.visible = false)?;
                }
                let id = doc.add_toolpath(merged);
                Ok(json!({ "id": id }))
            })
        }
        "cam.arrayCopy" => {
            let args: ArrayCopyArgs = parse_args(name, args)?;
            let chosen: Vec<u64> = document
                .toolpaths()
                .iter()
                .map(|t| t.id)
                .filter(|id| args.ids.contains(id))
                .collect();
            for id in &args.ids {
                document.toolpath(*id)?;
            }
            let sources: Vec<&Toolpath> = chosen
                .iter()
                .filter_map(|id| document.toolpath(*id).ok())
                .collect();
            let arrays = crate::cam::array_copy::array_copies(&sources, &args.spec)?;
            let group_name = args
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Array Copy".to_string());
            edit(document, "Array Copy Toolpath", |doc| {
                for id in &chosen {
                    doc.update_toolpath(*id, |t| t.visible = false)?;
                }
                let sheet = doc.active_sheet();
                let ids: Vec<u64> = arrays.into_iter().map(|t| doc.add_toolpath(t)).collect();
                let group = doc.add_toolpath_group(&group_name, sheet, &ids)?;
                Ok(json!({ "ids": ids, "group": group }))
            })
        }
        "cam.recalculate" if other_side_of(document, args).is_some() => {
            // A toolpath of the side not shown is calculated from that
            // side's vectors: show it for the calculation, then come back.
            let (side, back) = (
                other_side_of(document, args).unwrap_or_default(),
                document.active_side(),
            );
            document.show_side(side)?;
            let done = dispatch(document, name, args);
            document.show_side(back)?;
            done
        }
        "cam.recalculate" if is_array_copy(document, args) => {
            let args: RecalculateArgs = parse_args(name, args)?;
            let existing = document.toolpath(args.id)?.clone();
            let record = &existing.params_json["arrayCopy"];
            let ids_in = |key: &str| -> Vec<u64> {
                record[key]
                    .as_array()
                    .map(|list| list.iter().filter_map(Value::as_u64).collect())
                    .unwrap_or_default()
            };
            let parts = ids_in("sources");
            // Every source of the array, which a Gap is measured from
            // (arrays made before it was recorded have only their own).
            let span = ids_in("span");
            let spec: crate::cam::array_copy::ArrayCopySpec =
                serde_json::from_value(record["spec"].clone())
                    .map_err(|e| CoreError::invalid_args(name, e))?;
            edit(document, "Recalculate Toolpath", |doc| {
                for id in &parts {
                    let part = doc.toolpath(*id).map_err(|_| {
                        CoreError::validation(
                            "a toolpath this array was copied from is gone; copy it again",
                        )
                    })?;
                    if doc.toolpath_is_stale(part) {
                        let request = Request::of(part)?;
                        let visible = part.visible;
                        let mut done = cam_compute::calculate_parts(doc, &request)?.main;
                        done.visible = visible;
                        doc.replace_toolpath(*id, done)?;
                    }
                }
                for id in span.iter().filter(|id| !parts.contains(id)) {
                    let stale = doc
                        .toolpath(*id)
                        .ok()
                        .filter(|t| doc.toolpath_is_stale(t) && Request::of(t).is_ok());
                    if let Some(part) = stale {
                        let request = Request::of(part)?;
                        let visible = part.visible;
                        let mut done = cam_compute::calculate_parts(doc, &request)?.main;
                        done.visible = visible;
                        doc.replace_toolpath(*id, done)?;
                    }
                }
                let fresh: Vec<&Toolpath> = parts
                    .iter()
                    .filter_map(|id| doc.toolpath(*id).ok())
                    .collect();
                let spread: Vec<&Toolpath> = span
                    .iter()
                    .filter_map(|id| doc.toolpath(*id).ok())
                    .collect();
                // One copy toolpath comes back as one, even when its sources
                // no longer share a tool.
                let mut copy = crate::cam::array_copy::array_copy_again(&fresh, &spread, &spec)?;
                copy.name = existing.name.clone();
                copy.visible = existing.visible;
                doc.replace_toolpath(args.id, copy)?;
                Ok(json!({ "id": args.id, "warnings": [] }))
            })
        }
        "cam.recalculate" if is_imported(document, args) => {
            // A toolpath read from a file has nothing to calculate from.
            let args: RecalculateArgs = parse_args(name, args)?;
            KernelReply::ok(&json!({ "id": args.id, "warnings": [] }))
        }
        "cam.recalculate" if is_merged(document, args) => {
            let args: RecalculateArgs = parse_args(name, args)?;
            let existing = document.toolpath(args.id)?.clone();
            let parts: Vec<u64> = existing.params_json["merged"]
                .as_array()
                .map(|list| list.iter().filter_map(|p| p["id"].as_u64()).collect())
                .unwrap_or_default();
            edit(document, "Recalculate Toolpath", |doc| {
                for id in &parts {
                    let part = doc.toolpath(*id).map_err(|_| {
                        CoreError::validation(
                            "a toolpath this one was merged from is gone; merge again",
                        )
                    })?;
                    if doc.toolpath_is_stale(part) {
                        let request = Request::of(part)?;
                        let visible = part.visible;
                        let mut done = cam_compute::calculate_parts(doc, &request)?.main;
                        done.visible = visible;
                        doc.replace_toolpath(*id, done)?;
                    }
                }
                let fresh: Vec<&Toolpath> = parts
                    .iter()
                    .filter_map(|id| doc.toolpath(*id).ok())
                    .collect();
                let options = crate::cam::merge::options_of(&existing);
                let mut merged =
                    crate::cam::merge::merge_ordered(&fresh, &existing.name, &options)?;
                merged.visible = existing.visible;
                doc.replace_toolpath(args.id, merged)?;
                Ok(json!({ "id": args.id, "warnings": [] }))
            })
        }
        "cam.recalculate" => {
            let args: RecalculateArgs = parse_args(name, args)?;
            let existing = document.toolpath(args.id)?.clone();
            if existing.side != document.active_side() && !document.job_setup().is_double_sided() {
                return Err(CoreError::validation(format!(
                    "{} cuts the bottom side of a double sided job; make the job double sided again in Job Setup to recalculate it",
                    existing.name
                )));
            }
            let edited = args.request.is_some();
            let mut request = match args.request {
                Some(request) => request,
                None => Request::of(&existing)?,
            };
            if request.name.is_none() {
                request.name = Request::of(&existing)
                    .ok()
                    .and_then(|r| r.name)
                    .or_else(|| Some(existing.name.clone()));
            }
            let clearance = cam_compute::is_clearance(&existing);
            let rest_part = cam_compute::rest_index(&existing);
            // Toolpaths made before tool inclusive names keep theirs.
            let legacy_name = existing.params_json.get("baseName").is_none();
            // An older two tool pocket edited into a tool list: its first
            // toolpath takes the list and the clearance toolpath goes.
            let legacy_pair = (edited && tool_list_from_clearance(&existing, &request))
                .then(|| clearance_pair(document, &existing));
            edit(document, "Recalculate Toolpath", |doc| {
                let done = cam_compute::calculate_parts(doc, &request)?;
                let warnings = done.warnings;
                if let Some((main_id, clearance_id)) = legacy_pair {
                    let visible = doc.toolpath(main_id)?.visible;
                    let mut main = done.main;
                    main.visible = visible;
                    doc.replace_toolpath(main_id, main)?;
                    let ids = sync_rest(doc, main_id, done.rest)?;
                    if let Some(gone) = clearance_id {
                        doc.delete_toolpaths(&[gone])?;
                    }
                    for id in std::iter::once(main_id).chain(ids.iter().copied()) {
                        doc.remeasure_through_cut(id)?;
                    }
                    return Ok(json!({ "id": main_id, "restIds": ids, "warnings": warnings }));
                }
                // Edited settings of a multi tool toolpath redo every tool.
                let family = cam_compute::family(&existing);
                if !clearance && edited && (family.is_some() || !done.rest.is_empty()) {
                    let main_id = family.unwrap_or(args.id);
                    let visible = doc.toolpath(main_id).map(|t| t.visible).map_err(|_| {
                        CoreError::validation(
                            "the first toolpath of this multi tool toolpath is gone; calculate it again",
                        )
                    })?;
                    let mut main = done.main;
                    main.visible = visible;
                    doc.replace_toolpath(main_id, main)?;
                    let ids = sync_rest(doc, main_id, done.rest)?;
                    // New settings set a new through cut allowance.
                    for id in std::iter::once(main_id).chain(ids.iter().copied()) {
                        doc.remeasure_through_cut(id)?;
                    }
                    return Ok(json!({ "id": args.id, "restIds": ids, "warnings": warnings }));
                }
                let mut toolpath = if let Some(index) = rest_part {
                    let mut part = done
                        .rest
                        .into_iter()
                        .find(|(i, _)| *i == index)
                        .map(|(_, t)| t)
                        .ok_or_else(|| {
                            CoreError::validation(
                                "this tool no longer has anything left to machine; delete this toolpath",
                            )
                        })?;
                    part.params_json["family"] = existing.params_json["family"].clone();
                    part
                } else if clearance {
                    let mut cleared = done.clearance.ok_or_else(|| {
                        CoreError::validation(
                            "the clearance tool no longer fits anywhere; delete this toolpath",
                        )
                    })?;
                    if legacy_name {
                        cleared.name = existing.name.clone();
                    }
                    cleared
                } else {
                    let mut main = done.main;
                    if let Some(family) = family {
                        main.params_json["family"] = json!(family);
                    }
                    main
                };
                toolpath.visible = existing.visible;
                doc.replace_toolpath(args.id, toolpath)?;
                if edited {
                    doc.remeasure_through_cut(args.id)?;
                }
                Ok(json!({ "id": args.id, "warnings": warnings }))
            })
        }
        "cam.list" => KernelReply::ok(&toolpath_list(document)),
        "cam.prismDepth" => KernelReply::ok(&crate::cam::prism::depth_command(document, args)?),
        "cam.tabs" => KernelReply::ok(&crate::cam::tabs::tabs_command(document, args)?),
        "cam.sketchPreview" => {
            KernelReply::ok(&crate::cam::sketch::preview_command(document, args)?)
        }
        "cam.selectVectors" => {
            KernelReply::ok(&crate::cam::selector::select_command(document, args)?)
        }
        "cam.get" => {
            let args: IdArgs = parse_args(name, args)?;
            let toolpath = document.toolpath(args.id)?;
            KernelReply::ok(&json!({
                "id": toolpath.id,
                "name": toolpath.name,
                "kind": toolpath.kind,
                "visible": toolpath.visible,
                "stale": document.toolpath_is_stale(toolpath),
                "tool": toolpath.tool,
                "request": Request::of(toolpath).ok(),
                "estimatedSeconds": toolpath.estimated_seconds,
                "bounds": toolpath.bounds,
                "side": toolpath.side,
                "sheet": toolpath.sheet,
                "gadget": toolpath.gadget.as_ref().map(|g| json!({
                    "name": g.name,
                    "values": g.values,
                })),
            }))
        }
        "cam.delete" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = args.ids.unwrap_or_default();
            if ids.is_empty() {
                return Err(CoreError::validation("no toolpaths were chosen to delete"));
            }
            edit(document, "Delete Toolpath", |doc| {
                doc.delete_toolpaths(&ids)?;
                Ok(json!({}))
            })
        }
        "cam.rename" => {
            let args: RenameArgs = parse_args(name, args)?;
            let trimmed = args.name.trim().to_string();
            if trimmed.is_empty() {
                return Err(CoreError::validation("a toolpath needs a name"));
            }
            edit(document, "Rename Toolpath", |doc| {
                doc.update_toolpath(args.id, |t| t.name = trimmed)?;
                Ok(json!({}))
            })
        }
        "cam.setVisible" => {
            let args: VisibleArgs = parse_args(name, args)?;
            edit(document, "Show Toolpath", |doc| {
                for id in &args.ids {
                    doc.update_toolpath(*id, |t| t.visible = args.visible)?;
                }
                Ok(json!({}))
            })
        }
        "cam.reorder" => {
            let args: IdsArgs = parse_args(name, args)?;
            edit(document, "Reorder Toolpaths", |doc| {
                doc.reorder_toolpaths(&args.ids.unwrap_or_default())?;
                Ok(json!({}))
            })
        }
        "cam.saveTemplate" => {
            let args: SaveTemplateArgs = parse_args(name, args)?;
            let chosen = args.ids.unwrap_or_default();
            let mut entries = Vec::new();
            for toolpath in document.toolpaths() {
                let wanted = chosen.is_empty() || chosen.contains(&toolpath.id);
                // A clearance or later tool part comes back with its main toolpath.
                if !wanted
                    || cam_compute::is_clearance(toolpath)
                    || cam_compute::rest_index(toolpath).is_some()
                {
                    continue;
                }
                let request = Request::of(toolpath)?;
                let mut layers: Vec<String> = Vec::new();
                for id in &request.ids {
                    let Ok(entity) = document.entity(*id) else {
                        continue;
                    };
                    if let Ok(layer) = document.layer(entity.layer) {
                        if !layers.contains(&layer.name) {
                            layers.push(layer.name.clone());
                        }
                    }
                }
                entries.push(TemplateEntry::of(&request, toolpath.name.clone(), layers));
            }
            if entries.is_empty() {
                return Err(CoreError::validation("there are no toolpaths to save"));
            }
            let count = entries.len();
            Template::new(entries).write(&args.path)?;
            KernelReply::ok(&json!({ "count": count }))
        }
        "cam.templateLayers" => {
            let args: PathArgs = parse_args(name, args)?;
            let template = Template::read(&args.path)?;
            let mut missing: Vec<String> = Vec::new();
            for entry in &template.toolpaths {
                for layer in &entry.layers {
                    let known = document.layers().iter().any(|l| &l.name == layer);
                    if !known && !missing.contains(layer) {
                        missing.push(layer.clone());
                    }
                }
            }
            KernelReply::ok(&json!({ "missing": missing, "count": template.toolpaths.len() }))
        }
        "cam.loadTemplate" => {
            let args: LoadTemplateArgs = parse_args(name, args)?;
            let template = Template::read(&args.path)?;
            edit(document, "Apply Toolpath Template", |doc| {
                let mut created = Vec::new();
                let mut skipped = Vec::new();
                let mut waiting = Vec::new();
                for entry in &template.toolpaths {
                    let missing: Vec<&String> = entry
                        .layers
                        .iter()
                        .filter(|name| !doc.layers().iter().any(|l| &l.name == *name))
                        .collect();
                    if !missing.is_empty() {
                        match args.missing {
                            MissingLayers::Delete => {
                                skipped.push(json!({
                                    "name": entry.name,
                                    "reason": format!(
                                        "no vectors on the layer{} {}",
                                        if missing.len() == 1 { "" } else { "s" },
                                        missing
                                            .iter()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    ),
                                }));
                                continue;
                            }
                            MissingLayers::Create => {
                                for name in missing {
                                    doc.add_layer(Some(name.clone()), None);
                                }
                            }
                            MissingLayers::AsIs => {}
                        }
                    }
                    let layer_ids: Vec<u64> = doc
                        .layers()
                        .iter()
                        .filter(|l| entry.layers.contains(&l.name))
                        .map(|l| l.id)
                        .collect();
                    // Vectors come in the order their layers were first used,
                    // so a moulding's profile layer stays after its rails.
                    let rank = |layer: u64| {
                        doc.layers()
                            .iter()
                            .find(|l| l.id == layer)
                            .and_then(|l| entry.layers.iter().position(|n| *n == l.name))
                            .unwrap_or(usize::MAX)
                    };
                    let mut ids: Vec<(usize, u64)> = doc
                        .list(None)
                        .iter()
                        .filter(|v| layer_ids.contains(&v.layer))
                        .map(|v| (rank(v.layer), v.id))
                        .collect();
                    ids.sort_by_key(|(r, _)| *r);
                    let ids: Vec<u64> = ids.into_iter().map(|(_, id)| id).collect();
                    let needs_vectors = entry.strategy.needs_vectors();
                    let done = if ids.is_empty() && needs_vectors {
                        None
                    } else {
                        cam_compute::calculate_parts(doc, &entry.request(ids)).ok()
                    };
                    match done {
                        Some(done) => {
                            if let Some(clearance) = done.clearance {
                                created.push(doc.add_toolpath(clearance));
                            }
                            let id = doc.add_toolpath(done.main);
                            created.push(id);
                            created.extend(add_rest(doc, id, done.rest)?);
                        }
                        None => {
                            // Ready to calculate once its vectors are picked.
                            let id = doc.add_toolpath(uncalculated(entry));
                            created.push(id);
                            waiting.push(id);
                        }
                    }
                }
                if created.is_empty() {
                    return Err(CoreError::validation(format!(
                        "no toolpath in the template could be loaded here: {}",
                        skipped
                            .iter()
                            .map(|s| format!(
                                "{} ({})",
                                s["name"].as_str().unwrap_or(""),
                                s["reason"].as_str().unwrap_or("")
                            ))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )));
                }
                Ok(json!({ "ids": created, "skipped": skipped, "uncalculated": waiting }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// What loading a template does with toolpaths whose layers the job lacks.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum MissingLayers {
    /// Leave those toolpaths out.
    #[default]
    Delete,
    /// Add the layers, empty, and the toolpaths ready to calculate.
    Create,
    /// Add the toolpaths ready to calculate without the layers.
    AsIs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadTemplateArgs {
    path: PathBuf,
    #[serde(default)]
    missing: MissingLayers,
}

/// A template toolpath with its settings but no moves, marked as needing
/// a calculation: edit it, pick its vectors and calculate.
fn uncalculated(entry: &TemplateEntry) -> Toolpath {
    let request = entry.request(Vec::new());
    let mut toolpath = Toolpath::new(
        0,
        entry.name.clone(),
        entry.tool.clone(),
        entry.strategy.kind(),
    );
    toolpath.params_json = json!({
        "strategy": request.strategy,
        "params": request.params,
        "ids": [],
        "uncalculated": true,
    });
    toolpath
}

/// `cam.compute`'s placement: the toolpath group selected in the list,
/// which new toolpaths join when the preference says so.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlacementArgs {
    #[serde(default)]
    selected_group: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlugVectorsArgs {
    ids: Vec<u64>,
    #[serde(default)]
    sheet: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateArgs {
    id: u64,
    #[serde(default)]
    name: String,
}

/// The ids of a calculation's toolpaths once added to the list.
struct Added {
    id: u64,
    clearance_id: Option<u64>,
    rest_ids: Vec<u64>,
}

impl Added {
    /// Every id, in list order.
    fn all(&self) -> Vec<u64> {
        self.clearance_id
            .into_iter()
            .chain(std::iter::once(self.id))
            .chain(self.rest_ids.iter().copied())
            .collect()
    }
}

/// Adds a calculation's toolpaths at the end of the list. The clearance
/// tool cuts first, so it goes in first; a multi tool toolpath's later
/// tools follow the first.
fn add_parts(
    doc: &mut Document,
    clearance: Option<Toolpath>,
    main: Toolpath,
    rest: Vec<(usize, Toolpath)>,
) -> Result<Added, CoreError> {
    let clearance_id = clearance.map(|t| doc.add_toolpath(t));
    let id = doc.add_toolpath(main);
    let rest_ids = add_rest(doc, id, rest)?;
    Ok(Added {
        id,
        clearance_id,
        rest_ids,
    })
}

/// With Append new toolpaths to selected group on, moves new toolpaths to
/// the end of `group` when it is shown on the active sheet. A group that is
/// gone (the list was stale) leaves them at the end of the list.
fn into_selected_group(doc: &mut Document, ids: &[u64], group: u64) -> Result<(), CoreError> {
    if group == 0 || !doc.cam_settings().append_to_selected_group {
        return Ok(());
    }
    let active = doc.active_sheet();
    let shown = doc
        .toolpath_groups()
        .iter()
        .any(|g| g.id == group && (g.sheet == 0 || g.sheet == active));
    if shown {
        doc.place_toolpaths(ids, group, None)?;
    }
    Ok(())
}

/// Moves a duplicate's toolpaths straight after `original` (after its
/// later tools, for a multi tool toolpath), into the original's group.
fn beside(doc: &mut Document, ids: &[u64], original: &Toolpath) -> Result<(), CoreError> {
    let order = doc.toolpath_order().to_vec();
    let Some(at) = order.iter().position(|id| *id == original.id) else {
        return Ok(());
    };
    let before = order[at + 1..].iter().copied().find(|id| {
        !ids.contains(id)
            && doc
                .toolpath(*id)
                .map(|t| cam_compute::family(t) != Some(original.id))
                .unwrap_or(true)
    });
    doc.place_toolpaths(ids, original.group, before)
}

/// Whether an edit turns a toolpath of an older two tool pocket (a pocket
/// with a clearance tool) into one with a tool list.
fn tool_list_from_clearance(existing: &Toolpath, request: &Request) -> bool {
    let Ok(before) = Request::of(existing) else {
        return false;
    };
    let had = |params: &Value| params.get("clearanceTool").is_some_and(|t| !t.is_null());
    before.strategy == cam_compute::Strategy::Pocket
        && request.strategy == cam_compute::Strategy::Pocket
        && had(&before.params)
        && !had(&request.params)
}

/// The two toolpaths of an older two tool pocket that `existing` is one
/// of: the main tool's (which the edited toolpath becomes) and the
/// clearance tool's, found by the request they were both made from. When
/// the other half is gone, `existing` is the one kept.
fn clearance_pair(doc: &Document, existing: &Toolpath) -> (u64, Option<u64>) {
    let clearance = cam_compute::is_clearance(existing);
    let same = |t: &Toolpath| {
        t.id != existing.id
            && cam_compute::is_clearance(t) != clearance
            && t.params_json.get("strategy") == existing.params_json.get("strategy")
            && t.params_json.get("params") == existing.params_json.get("params")
            && t.params_json.get("ids") == existing.params_json.get("ids")
            && t.params_json.get("baseName") == existing.params_json.get("baseName")
    };
    let other = doc.toolpaths().iter().find(|t| same(t)).map(|t| t.id);
    match (clearance, other) {
        (true, Some(main)) => (main, Some(existing.id)),
        (true, None) => (existing.id, None),
        (false, other) => (existing.id, other),
    }
}

/// Adds a multi tool toolpath's later tools after its first toolpath
/// `main_id`, all marked as one family, and answers their ids.
fn add_rest(
    doc: &mut Document,
    main_id: u64,
    rest: Vec<(usize, Toolpath)>,
) -> Result<Vec<u64>, CoreError> {
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    doc.update_toolpath(main_id, |t| t.params_json["family"] = json!(main_id))?;
    let group = doc.toolpath(main_id)?.group;
    let mut ids = Vec::with_capacity(rest.len());
    for (_, mut toolpath) in rest {
        toolpath.params_json["family"] = json!(main_id);
        let id = doc.add_toolpath(toolpath);
        doc.update_toolpath(id, |t| t.group = group)?;
        ids.push(id);
    }
    Ok(ids)
}

/// Brings a multi tool family in line with a new calculation: later tools
/// still in it are replaced in place, new ones added after the family,
/// dropped ones deleted. Answers the later tools' ids in tool order.
fn sync_rest(
    doc: &mut Document,
    main_id: u64,
    rest: Vec<(usize, Toolpath)>,
) -> Result<Vec<u64>, CoreError> {
    let members: Vec<(usize, u64, bool)> = doc
        .toolpaths()
        .iter()
        .filter(|t| cam_compute::family(t) == Some(main_id) && t.id != main_id)
        .filter_map(|t| cam_compute::rest_index(t).map(|i| (i, t.id, t.visible)))
        .collect();
    let last_member = members.last().map_or(main_id, |(_, id, _)| *id);
    let mut kept = Vec::new();
    let mut added = Vec::new();
    let mut ids = Vec::new();
    if rest.is_empty() {
        doc.update_toolpath(main_id, |t| {
            if let Some(record) = t.params_json.as_object_mut() {
                record.remove("family");
            }
        })?;
    } else {
        doc.update_toolpath(main_id, |t| t.params_json["family"] = json!(main_id))?;
    }
    let group = doc.toolpath(main_id)?.group;
    for (index, mut toolpath) in rest {
        toolpath.params_json["family"] = json!(main_id);
        match members.iter().find(|(i, _, _)| *i == index) {
            Some((_, id, visible)) => {
                toolpath.visible = *visible;
                doc.replace_toolpath(*id, toolpath)?;
                kept.push(*id);
                ids.push(*id);
            }
            None => {
                let id = doc.add_toolpath(toolpath);
                doc.update_toolpath(id, |t| t.group = group)?;
                added.push(id);
                ids.push(id);
            }
        }
    }
    let gone: Vec<u64> = members
        .iter()
        .map(|(_, id, _)| *id)
        .filter(|id| !kept.contains(id))
        .collect();
    if !added.is_empty() {
        // New tools cut straight after the rest of their family.
        let mut order: Vec<u64> = doc
            .toolpaths()
            .iter()
            .map(|t| t.id)
            .filter(|id| !added.contains(id))
            .collect();
        let at = order
            .iter()
            .position(|id| *id == last_member)
            .map_or(order.len(), |p| p + 1);
        order.splice(at..at, added.iter().copied());
        doc.reorder_toolpaths(&order)?;
    }
    if !gone.is_empty() {
        doc.delete_toolpaths(&gone)?;
    }
    Ok(ids)
}

fn toolpath_list(document: &Document) -> Vec<Value> {
    document
        .toolpaths()
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "kind": t.kind,
                // The strategy that made it, which picks its edit form
                // where the kind is shared (the three laser strategies).
                "strategy": t.params_json.get("strategy").cloned(),
                "visible": t.visible,
                "stale": document.toolpath_is_stale(t),
                "toolName": t.tool.name,
                "toolNumber": t.tool_number,
                "estimatedSeconds": t.estimated_seconds,
                "moves": t.moves.len(),
                "side": t.side,
                "sheet": t.sheet,
                "group": t.group,
                "toolType": t.tool.tool_type.type_name(),
                "cuts": t.moves.iter().any(|m| m.kind.is_cutting()),
                "gadget": t.gadget.as_ref().map(|g| g.name.clone()),
            })
        })
        .collect()
}

/// Whether a `cam.recalculate` names a merged toolpath and asks for
/// no new settings.
fn is_imported(document: &Document, args: &str) -> bool {
    serde_json::from_str::<Value>(args)
        .ok()
        .and_then(|value| value["id"].as_u64())
        .is_some_and(|id| crate::cam::import_cmd::is_imported(document, id))
}

/// Whether a `cam.recalculate` names an array copy toolpath and asks for
/// no new settings.
/// The side of the toolpath a `cam.recalculate` names when it is not the
/// side shown.
fn other_side_of(document: &Document, args: &str) -> Option<crate::doc::Side> {
    let value = serde_json::from_str::<Value>(args).ok()?;
    let side = document.toolpath(value["id"].as_u64()?).ok()?.side;
    (side != document.active_side() && document.job().is_double_sided()).then_some(side)
}

fn is_array_copy(document: &Document, args: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(args) else {
        return false;
    };
    value.get("request").is_none_or(Value::is_null)
        && value["id"]
            .as_u64()
            .and_then(|id| document.toolpath(id).ok())
            .is_some_and(|t| t.params_json.get("arrayCopy").is_some())
}

fn is_merged(document: &Document, args: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(args) else {
        return false;
    };
    value.get("request").is_none_or(Value::is_null)
        && value["id"]
            .as_u64()
            .and_then(|id| document.toolpath(id).ok())
            .is_some_and(|t| t.params_json.get("merged").is_some())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecalculateArgs {
    id: u64,
    #[serde(default)]
    request: Option<Request>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArrayCopyArgs {
    ids: Vec<u64>,
    #[serde(default)]
    spec: crate::cam::array_copy::ArrayCopySpec,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeArgs {
    ids: Vec<u64>,
    #[serde(default)]
    name: Option<String>,
    /// The orders to try and Merge By Part; none joins in list order.
    #[serde(default)]
    options: crate::cam::merge::MergeOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveTemplateArgs {
    path: PathBuf,
    /// Toolpaths to save; all of them when left out or empty.
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisibleArgs {
    ids: Vec<u64>,
    visible: bool,
}
