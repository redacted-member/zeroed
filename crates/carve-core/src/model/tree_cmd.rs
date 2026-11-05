//! Kernel commands of the component tree and the component properties:
//! `model.props`, `model.groups.*`, `model.tree.move`,
//! `model.levels.setClip`, `model.levels.updateClip`, `model.fromVisible`, `model.clear` and
//! `model.colors` and `model.highlight`. Every change is one undo step.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::Point;
use crate::kernel::KernelReply;

use super::combine_ops::merge;
use super::field::Field;
use super::transform::{transform, TransformParams};
use super::tree::{parse_color, ComponentExtra, Group};
use super::{grid_for, Combine, Component};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.props"
            | "model.groups.create"
            | "model.groups.update"
            | "model.groups.ungroup"
            | "model.groups.transform"
            | "model.tree.move"
            | "model.levels.setClip"
            | "model.levels.updateClip"
            | "model.fromVisible"
            | "model.clear"
            | "model.colors"
            | "model.highlight"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] for unknown ids or values out of range.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "model.props" => props(document, parse(name, args)?),
        "model.groups.create" => create_group(document, parse(name, args)?),
        "model.groups.update" => update_group(document, parse(name, args)?),
        "model.groups.ungroup" => {
            let args: IdArgs = parse(name, args)?;
            ungroup(document, args.id)
        }
        "model.groups.transform" => transform_group(document, parse(name, args)?),
        "model.tree.move" => move_components(document, parse(name, args)?),
        "model.levels.setClip" => set_clip(document, parse(name, args)?),
        "model.levels.updateClip" => update_clip(document, parse(name, args)?),
        "model.fromVisible" => from_visible(document, parse(name, args)?),
        "model.clear" => clear(document),
        "model.colors" => colors(document),
        "model.highlight" => {
            let args: HighlightArgs = parse(name, args)?;
            highlight(document, &args.ids)
        }
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
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

/// Checks a preview material name: short and not blank.
pub(super) fn check_material(material: Option<&Option<String>>) -> Result<(), CoreError> {
    match material {
        Some(Some(name)) if name.trim().is_empty() || name.len() > 40 => Err(
            CoreError::validation(format!("{name:?} is not a material name")),
        ),
        _ => Ok(()),
    }
}

/// A present key, even when its value is null: `Some(None)` clears.
pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
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

pub(super) fn check_color(color: Option<&str>) -> Result<(), CoreError> {
    match color {
        Some(text) => parse_color(text).map(|_| ()),
        None => Ok(()),
    }
}

fn level_of_group(document: &Document, id: u64) -> Result<u64, CoreError> {
    document
        .levels()
        .iter()
        .find(|l| l.groups.iter().any(|g| g.id == id))
        .map(|l| l.id)
        .ok_or_else(|| CoreError::validation(format!("no group with id {id}")))
}

fn members_of(document: &Document, group: u64) -> Vec<u64> {
    document
        .components()
        .iter()
        .filter(|c| c.extra.group == group)
        .map(|c| c.id)
        .collect()
}

/// Drops groups no component belongs to any more.
fn drop_empty_groups(doc: &mut Document) -> Result<(), CoreError> {
    let used: BTreeSet<(u64, u64)> = doc
        .components()
        .iter()
        .map(|c| (c.level, c.extra.group))
        .collect();
    let stale: Vec<u64> = doc
        .levels()
        .iter()
        .filter(|l| l.groups.iter().any(|g| !used.contains(&(l.id, g.id))))
        .map(|l| l.id)
        .collect();
    for level in stale {
        doc.update_level(level, |l| {
            let id = l.id;
            l.groups.retain(|g| used.contains(&(id, g.id)));
        })?;
    }
    Ok(())
}

/// The component order with `ids` taken out and put back together, in
/// their own order, just below `before` (or on top when `None`).
fn placed(order: &[u64], ids: &[u64], before: Option<u64>) -> Vec<u64> {
    let moving: Vec<u64> = order.iter().copied().filter(|c| ids.contains(c)).collect();
    let mut rest: Vec<u64> = order.iter().copied().filter(|c| !ids.contains(c)).collect();
    let at = before
        .and_then(|b| rest.iter().position(|c| *c == b))
        .unwrap_or(rest.len());
    rest.splice(at..at, moving);
    rest
}

fn check_ids(document: &Document, ids: &[u64]) -> Result<(), CoreError> {
    if ids.is_empty() {
        return Err(CoreError::validation("choose one or more components"));
    }
    for id in ids {
        document.component(*id)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

// -------------------------------------------------------------- properties

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropsArgs {
    ids: Vec<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    combine: Option<Combine>,
    #[serde(default)]
    height_scale: Option<f64>,
    #[serde(default)]
    base_height: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    color: Option<Option<String>>,
    /// Name of the preview material the colour comes from; null for none.
    #[serde(default, deserialize_with = "present")]
    material: Option<Option<String>>,
    /// Dynamic fade; null turns it off.
    #[serde(default, deserialize_with = "present")]
    fade: Option<Option<super::dynamic::Fade>>,
    /// Dynamic tilt; null turns it off.
    #[serde(default, deserialize_with = "present")]
    tilt: Option<Option<super::dynamic::Tilt>>,
    /// Bake the dynamic properties (shape height, base height, fade and
    /// tilt, after the other changes) into the heights and reset them.
    #[serde(default)]
    bake: bool,
    /// A live height drag in the 3D view: the revision its previous step
    /// answered. That step is replaced, so the drag is one undo step.
    #[serde(default)]
    replace: Option<u64>,
}

fn props(document: &mut Document, args: PropsArgs) -> Result<KernelReply, CoreError> {
    check_ids(document, &args.ids)?;
    let name = args.name.map(|n| n.trim().to_string());
    if let Some(name) = &name {
        if args.ids.len() > 1 {
            return Err(CoreError::validation(
                "a name can only be given to one component at a time",
            ));
        }
        if name.is_empty() {
            return Err(CoreError::validation("the name cannot be empty"));
        }
    }
    for (value, what) in [
        (args.height_scale, "shape height"),
        (args.base_height, "base height"),
    ] {
        if value.is_some_and(|v| !v.is_finite()) {
            return Err(CoreError::validation(format!(
                "the {what} must be a number"
            )));
        }
    }
    if let Some(color) = &args.color {
        check_color(color.as_deref())?;
    }
    check_material(args.material.as_ref())?;
    if let Some(Some(fade)) = &args.fade {
        fade.check()?;
    }
    if let Some(Some(tilt)) = &args.tilt {
        tilt.check()?;
    }
    let label = if args.ids.len() > 1 {
        "Edit Components"
    } else {
        "Edit Component"
    };
    let undone = args.replace.is_some_and(|revision| {
        document.revision() == revision
            && document.journal().undo_label() == Some(label)
            && document.undo().is_some()
    });
    let result = edit(document, label, |doc| {
        for id in &args.ids {
            doc.update_component(*id, |c| {
                if let Some(name) = &name {
                    c.name.clone_from(name);
                }
                if let Some(visible) = args.visible {
                    c.visible = visible;
                }
                if let Some(combine) = args.combine {
                    c.combine = combine;
                }
                if let Some(scale) = args.height_scale {
                    c.height_scale = scale;
                }
                if let Some(base) = args.base_height {
                    c.extra.base_height = base;
                }
                if let Some(color) = &args.color {
                    c.extra.color.clone_from(color);
                }
                if let Some(material) = &args.material {
                    c.extra.material.clone_from(material);
                }
                if let Some(fade) = args.fade {
                    c.extra.fade = fade;
                }
                if let Some(tilt) = args.tilt {
                    c.extra.tilt = tilt;
                }
                if args.bake {
                    c.field = super::dynamic::bake_field(
                        &c.field,
                        c.height_scale,
                        c.extra.base_height,
                        c.extra.fade.as_ref(),
                        c.extra.tilt.as_ref(),
                    );
                    c.height_scale = 1.0;
                    c.extra.base_height = 0.0;
                    c.extra.fade = None;
                    c.extra.tilt = None;
                    // Its heights are no longer what the shape tools made.
                    c.extra.shape = None;
                }
            })?;
        }
        Ok(json!({}))
    });
    if result.is_err() && undone {
        document.redo();
    }
    result
}

// ------------------------------------------------------------------ groups

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupCreateArgs {
    ids: Vec<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    combine: Combine,
}

fn create_group(document: &mut Document, args: GroupCreateArgs) -> Result<KernelReply, CoreError> {
    check_ids(document, &args.ids)?;
    let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
    // The group goes where its topmost member is, on that member's level.
    let top = order
        .iter()
        .rev()
        .find(|c| args.ids.contains(c))
        .copied()
        .unwrap_or(args.ids[0]);
    let level = document.component(top)?.level;
    let level = if document.levels().iter().any(|l| l.id == level) {
        level
    } else {
        document.levels().first().map_or(level, |l| l.id)
    };
    let before = order
        .iter()
        .skip_while(|c| **c != top)
        .find(|c| !args.ids.contains(c))
        .copied();
    let count: usize = document.levels().iter().map(|l| l.groups.len()).sum();
    let name = args
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Group {}", count + 1));
    edit(document, "Group Components", |doc| {
        let id = doc.add_group(
            level,
            Group {
                id: 0,
                name,
                visible: true,
                combine: args.combine,
                expanded: true,
                color: None,
                material: None,
            },
        )?;
        for member in &args.ids {
            doc.update_component(*member, |c| {
                c.level = level;
                c.extra.group = id;
            })?;
        }
        doc.reorder_components(&placed(&order, &args.ids, before))?;
        drop_empty_groups(doc)?;
        Ok(json!({ "id": id }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupUpdateArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    combine: Option<Combine>,
    #[serde(default)]
    expanded: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    color: Option<Option<String>>,
    /// Name of the preview material the colour comes from; null for none.
    #[serde(default, deserialize_with = "present")]
    material: Option<Option<String>>,
}

fn update_group(document: &mut Document, args: GroupUpdateArgs) -> Result<KernelReply, CoreError> {
    let level = level_of_group(document, args.id)?;
    if let Some(color) = &args.color {
        check_color(color.as_deref())?;
    }
    check_material(args.material.as_ref())?;
    let name = args
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    edit(document, "Edit Group", |doc| {
        doc.update_level(level, |l| {
            if let Some(group) = l.groups.iter_mut().find(|g| g.id == args.id) {
                if let Some(name) = name {
                    group.name = name;
                }
                if let Some(visible) = args.visible {
                    group.visible = visible;
                }
                if let Some(combine) = args.combine {
                    group.combine = combine;
                }
                if let Some(expanded) = args.expanded {
                    group.expanded = expanded;
                }
                if let Some(color) = args.color {
                    group.color = color;
                }
                if let Some(material) = args.material {
                    group.material = material;
                }
            }
        })?;
        Ok(json!({}))
    })
}

fn ungroup(document: &mut Document, id: u64) -> Result<KernelReply, CoreError> {
    let level = level_of_group(document, id)?;
    let members = members_of(document, id);
    edit(document, "Ungroup Components", |doc| {
        for member in &members {
            doc.update_component(*member, |c| c.extra.group = 0)?;
        }
        doc.update_level(level, |l| l.groups.retain(|g| g.id != id))?;
        Ok(json!({ "ids": members }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupTransformArgs {
    id: u64,
    params: TransformParams,
}

/// The job space box of a field's cells: min and max corners.
fn extent(field: &Field) -> (Point, Point) {
    let half = field.cell / 2.0;
    (
        Point::new(field.origin.x - half, field.origin.y - half),
        Point::new(
            field.origin.x + (field.width as f64 - 0.5) * field.cell,
            field.origin.y + (field.height as f64 - 0.5) * field.cell,
        ),
    )
}

fn transform_group(
    document: &mut Document,
    args: GroupTransformArgs,
) -> Result<KernelReply, CoreError> {
    level_of_group(document, args.id)?;
    let members = members_of(document, args.id);
    if members.is_empty() {
        return Err(CoreError::validation("the group has no components"));
    }
    let mut params = args.params;
    if params.centre.is_none() {
        // Scale and turn about the middle of the whole group's box.
        let (mut lo, mut hi) = (
            Point::new(f64::MAX, f64::MAX),
            Point::new(f64::MIN, f64::MIN),
        );
        for id in &members {
            let (a, b) = extent(&document.component(*id)?.field);
            lo = Point::new(lo.x.min(a.x), lo.y.min(a.y));
            hi = Point::new(hi.x.max(b.x), hi.y.max(b.y));
        }
        params.centre = Some(Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0));
    }
    let mut fields = Vec::with_capacity(members.len());
    for id in &members {
        let component = document.component(*id)?;
        let moved = transform(&component.field, &params)?;
        let mut extra = component.extra.clone();
        super::dynamic::follow_transform(&mut extra, &component.field, &params);
        if let Some(shape) = extra.shape.as_deref_mut() {
            shape.moved(&params.matrix(&component.field), false);
        }
        let field = moved
            .cropped()
            .ok_or_else(|| CoreError::validation("a component of the group would be empty"))?;
        fields.push((*id, field, extra));
    }
    edit(document, "Transform Group", |doc| {
        for (id, field, extra) in fields {
            doc.update_component(id, |c| {
                c.field = field;
                c.extra = extra;
            })?;
        }
        Ok(json!({}))
    })
}

// -------------------------------------------------------------------- move

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveArgs {
    ids: Vec<u64>,
    level: u64,
    /// Group to join inside `level`; absent or 0 for none.
    #[serde(default)]
    group: Option<u64>,
    /// Land just below this component.
    #[serde(default)]
    before: Option<u64>,
    /// Land just above this component.
    #[serde(default)]
    after: Option<u64>,
}

fn move_components(document: &mut Document, args: MoveArgs) -> Result<KernelReply, CoreError> {
    check_ids(document, &args.ids)?;
    let level = document
        .levels()
        .iter()
        .find(|l| l.id == args.level)
        .ok_or_else(|| CoreError::validation(format!("no level with id {}", args.level)))?;
    let group = args.group.unwrap_or(0);
    if group != 0 && !level.groups.iter().any(|g| g.id == group) {
        return Err(CoreError::validation(format!(
            "no group with id {group} on level {}",
            level.name
        )));
    }
    for anchor in [args.before, args.after].into_iter().flatten() {
        document.component(anchor)?;
        if args.ids.contains(&anchor) {
            return Err(CoreError::validation(
                "a component cannot be placed next to itself",
            ));
        }
    }
    let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
    let rest: Vec<u64> = order
        .iter()
        .copied()
        .filter(|c| !args.ids.contains(c))
        .collect();
    let before = if let Some(b) = args.before {
        Some(b)
    } else if let Some(a) = args.after {
        rest.iter().skip_while(|c| **c != a).nth(1).copied()
    } else {
        // On top of the group, or of the level, it joins.
        let top = document
            .components()
            .into_iter()
            .rev()
            .find(|c| {
                !args.ids.contains(&c.id)
                    && c.level == args.level
                    && (group == 0 || c.extra.group == group)
            })
            .map(|c| c.id);
        top.and_then(|t| rest.iter().skip_while(|c| **c != t).nth(1).copied())
    };
    let target_level = args.level;
    edit(document, "Move Components", |doc| {
        for id in &args.ids {
            doc.update_component(*id, |c| {
                c.level = target_level;
                c.extra.group = group;
            })?;
        }
        doc.reorder_components(&placed(&order, &args.ids, before))?;
        drop_empty_groups(doc)?;
        Ok(json!({}))
    })
}

// -------------------------------------------------------------------- clip

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipArgs {
    id: u64,
    #[serde(default)]
    vector_ids: Vec<u64>,
}

fn set_clip(document: &mut Document, args: ClipArgs) -> Result<KernelReply, CoreError> {
    for id in &args.vector_ids {
        document.entity(*id)?;
    }
    if !document.levels().iter().any(|l| l.id == args.id) {
        return Err(CoreError::validation(format!(
            "no level with id {}",
            args.id
        )));
    }
    if args.vector_ids.is_empty() {
        return edit(document, "Remove Level Clipping", |doc| {
            doc.update_level(args.id, |l| {
                l.clip.clear();
                l.clip_shape.clear();
            })?;
            Ok(json!({}))
        });
    }
    let shape = closed_shape(document, &args.vector_ids)?;
    edit(document, "Apply Level Clipping", |doc| {
        doc.update_level(args.id, |l| {
            l.clip = args.vector_ids;
            l.clip_shape = shape;
        })?;
        Ok(json!({}))
    })
}

/// The closed paths of `ids` as a clipping boundary.
///
/// # Errors
/// [`CoreError::Validation`] when they hold no closed vector.
fn closed_shape(document: &Document, ids: &[u64]) -> Result<Vec<crate::geom::Path>, CoreError> {
    let shape: Vec<_> = document
        .clip_paths(ids)
        .into_iter()
        .filter(|p| p.subpaths.iter().any(|s| s.closed))
        .collect();
    if shape.is_empty() {
        return Err(CoreError::validation(
            "select closed vectors to clip the level to",
        ));
    }
    Ok(shape)
}

/// Reads the clipping boundary again: from the vectors given, or when none
/// is given from the vectors the clipping was last taken from, while they
/// are still in the job.
fn update_clip(document: &mut Document, args: ClipArgs) -> Result<KernelReply, CoreError> {
    let level = document
        .levels()
        .iter()
        .find(|l| l.id == args.id)
        .ok_or_else(|| CoreError::validation(format!("no level with id {}", args.id)))?;
    if !level.is_clipped() {
        return Err(CoreError::validation("the level has no clipping to update"));
    }
    let ids = if args.vector_ids.is_empty() {
        level
            .clip
            .iter()
            .copied()
            .filter(|id| document.entity(*id).is_ok())
            .collect()
    } else {
        for id in &args.vector_ids {
            document.entity(*id)?;
        }
        args.vector_ids
    };
    if ids.is_empty() {
        return Err(CoreError::validation(
            "the clipping vectors are gone: select closed vectors to clip to",
        ));
    }
    let shape = closed_shape(document, &ids)?;
    edit(document, "Update Level Clipping", |doc| {
        doc.update_level(args.id, |l| {
            l.clip = ids;
            l.clip_shape = shape;
        })?;
        Ok(json!({}))
    })
}

// ----------------------------------------------------------- from visible

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FromVisibleArgs {
    /// Components to copy; none or empty for the whole visible model.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    name: Option<String>,
}

fn from_visible(document: &mut Document, args: FromVisibleArgs) -> Result<KernelReply, CoreError> {
    let ids = args.ids.unwrap_or_default();
    let levels = document.levels().to_vec();
    let visible_level = |id: u64| {
        let first = levels.first().map_or(0, |l| l.id);
        let level = if levels.iter().any(|l| l.id == id) {
            id
        } else {
            first
        };
        levels.iter().any(|l| l.id == level && l.visible)
    };
    let (field, sources, level) = if ids.is_empty() {
        let sources: Vec<u64> = document
            .components()
            .iter()
            .filter(|c| c.visible && visible_level(c.level))
            .map(|c| c.id)
            .collect();
        let mut composite = document.composite();
        // Cells no component raised or lowered hold no material.
        for v in &mut composite.z {
            if *v == 0.0 {
                *v = f32::NAN;
            }
        }
        // The copy adds onto the zero plane, so it goes on the lowest
        // level that adds up, the others no longer showing anything.
        let level = levels
            .iter()
            .find(|l| l.visible && l.combine == Combine::Add)
            .map_or(document.active_level(), |l| l.id);
        (composite.cropped(), sources, level)
    } else {
        check_ids(document, &ids)?;
        let chosen: Vec<&Component> = document
            .components()
            .into_iter()
            .filter(|c| ids.contains(&c.id))
            .collect();
        let level = chosen.first().map_or(document.active_level(), |c| c.level);
        let sources = chosen
            .iter()
            .filter(|c| c.visible && visible_level(c.level))
            .map(|c| c.id)
            .collect();
        (merge(&chosen, &levels), sources, level)
    };
    let field = field.ok_or_else(|| {
        CoreError::validation("there is no visible model to make a component from")
    })?;
    let taken = document.components().len();
    let name = args
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Visible Model {}", taken + 1));
    edit(document, "Create Component from Visible Model", |doc| {
        // The sources stay in the tree, hidden, so the new component is
        // what shows and the model looks the same.
        for id in &sources {
            doc.update_component(*id, |c| c.visible = false)?;
        }
        let id = doc.add_component(Component {
            id: 0,
            name,
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level,
            extra: ComponentExtra::default(),
            field,
        });
        Ok(json!({ "id": id, "hidden": sources }))
    })
}

// ------------------------------------------------------------------ clear

/// Removes every component and every group, keeping the levels, as one undo
/// step. Answers the ids of the removed components.
fn clear(document: &mut Document) -> Result<KernelReply, CoreError> {
    let ids: Vec<u64> = document.components().iter().map(|c| c.id).collect();
    let grouped: Vec<u64> = document
        .levels()
        .iter()
        .filter(|l| !l.groups.is_empty())
        .map(|l| l.id)
        .collect();
    if ids.is_empty() && grouped.is_empty() {
        return Err(CoreError::validation("there is no model to clear"));
    }
    edit(document, "Clear Model", |doc| {
        if !ids.is_empty() {
            doc.delete_components(&ids)?;
        }
        for level in &grouped {
            doc.update_level(*level, |l| l.groups.clear())?;
        }
        Ok(json!({ "deleted": ids }))
    })
}

// ------------------------------------------------------------------ colors

/// The component colours over the model grid as a `palette` of RGBA
/// entries (entry 0 clear) and the `index` of each cell's entry; see
/// [`crate::blob::palette`].
fn colors(document: &Document) -> Result<KernelReply, CoreError> {
    let grid = grid_for(document.job());
    let map = document.composite_appearance();
    let any = map.rgba.chunks_exact(4).any(|px| px[3] != 0);
    let (palette, index, tags) = crate::blob::tagged_palette(&map.rgba, &map.material);
    // The preview material of each palette entry, null for a plain colour.
    let materials: Vec<Option<&str>> = tags
        .iter()
        .map(|tag| {
            usize::from(*tag)
                .checked_sub(1)
                .and_then(|at| map.materials.get(at))
                .map(String::as_str)
        })
        .collect();
    let header = json!({
        "revision": document.revision(),
        "width": grid.width,
        "height": grid.height,
        "any": any,
        "materials": materials,
    });
    Ok(KernelReply::Bytes(crate::blob::frame(
        header,
        &[palette, index],
    )))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HighlightArgs {
    #[serde(default)]
    ids: Vec<u64>,
}

/// The selection highlight of `ids` over the model grid as a blob with one
/// `mask` byte per cell; see [`Document::highlight_mask`].
fn highlight(document: &Document, ids: &[u64]) -> Result<KernelReply, CoreError> {
    let (grid, mask) = document.highlight_mask(ids);
    let count = mask.len();
    let header = json!({
        "revision": document.revision(),
        "width": grid.width,
        "height": grid.height,
        "sections": [{ "name": "mask", "dtype": "u8", "offset": 0, "count": count }],
    })
    .to_string()
    .into_bytes();
    let mut out = Vec::with_capacity(4 + header.len() + count);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&mask);
    Ok(KernelReply::Bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::rect;

    fn block(name: &str, x: f64, height: f32) -> Component {
        let mut field = Field::empty(10, 10, 1.0, Point::new(x, 10.0));
        field.z.fill(height);
        Component {
            id: 0,
            name: name.to_string(),
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: ComponentExtra::default(),
            field,
        }
    }

    fn doc_with(blocks: &[(&str, f64, f32)]) -> (Document, Vec<u64>) {
        let mut doc = Document::new();
        doc.begin("Add");
        let ids = blocks
            .iter()
            .map(|(n, x, h)| doc.add_component(block(n, *x, *h)))
            .collect();
        doc.commit();
        (doc, ids)
    }

    fn run(doc: &mut Document, name: &str, args: Value) -> Value {
        match dispatch(doc, name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                let v: Value = serde_json::from_str(&text).expect("json");
                v["result"].clone()
            }
            KernelReply::Bytes(_) => Value::Null,
        }
    }

    fn at(doc: &Document, x: f64) -> f32 {
        doc.composite().at(Point::new(x, 14.0))
    }

    #[test]
    fn a_live_height_drag_replaces_its_own_step() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0)]);
        let depth = doc.journal().undo_depth();
        let step = |doc: &mut Document, base: f64, replace: Option<u64>| {
            run(
                doc,
                "model.props",
                json!({ "ids": [ids[0]], "baseHeight": base, "replace": replace }),
            )["revision"]
                .as_u64()
        };
        let first = step(&mut doc, 1.0, None);
        let second = step(&mut doc, 2.0, first);
        step(&mut doc, 3.0, second);
        assert_eq!(at(&doc, 14.0), 5.0);
        assert_eq!(doc.journal().undo_depth(), depth + 1);
        // A stale revision stacks a new step.
        step(&mut doc, 4.0, first);
        assert_eq!(doc.journal().undo_depth(), depth + 2);
        doc.undo();
        doc.undo();
        assert_eq!(at(&doc, 14.0), 2.0, "back where the drag began");
    }

    #[test]
    fn props_edit_many_components_in_one_step() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 40.0, 2.0)]);
        run(
            &mut doc,
            "model.props",
            json!({ "ids": ids, "heightScale": 1.5, "baseHeight": 1.0, "color": "#336699" }),
        );
        assert_eq!(at(&doc, 14.0), 4.0);
        assert_eq!(at(&doc, 44.0), 4.0);
        assert_eq!(
            doc.component(ids[1]).map(|c| c.extra.color.clone()).ok(),
            Some(Some("#336699".into()))
        );
        doc.undo();
        assert_eq!(at(&doc, 14.0), 2.0, "one undo step puts both back");
        assert_eq!(at(&doc, 44.0), 2.0);
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[0]], "color": "#112233" }),
        );
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[0]], "color": null }),
        );
        assert_eq!(
            doc.component(ids[0]).map(|c| c.extra.color.clone()).ok(),
            Some(None)
        );
        assert!(dispatch(
            &mut doc,
            "model.props",
            &json!({ "ids": ids, "name": "x" }).to_string()
        )
        .is_err());
        assert!(dispatch(
            &mut doc,
            "model.props",
            &json!({ "ids": [ids[0]], "color": "red" }).to_string()
        )
        .is_err());
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[0]], "name": " Rose " }),
        );
        assert_eq!(
            doc.component(ids[0]).map(|c| c.name.clone()).ok(),
            Some("Rose".into())
        );
    }

    #[test]
    fn groups_bake_as_one_and_undo_in_one_step() {
        // A slab, then two bumps grouped and subtracted as one.
        let (mut doc, ids) = doc_with(&[("slab", 10.0, 10.0), ("a", 10.0, 2.0), ("b", 15.0, 3.0)]);
        let slab = ids[0];
        let reply = run(
            &mut doc,
            "model.groups.create",
            json!({ "ids": [ids[1], ids[2]], "combine": "subtract" }),
        );
        let group = reply["id"].as_u64().expect("id");
        assert_eq!(members_of(&doc, group), vec![ids[1], ids[2]]);
        // Where a and b overlap they add first (5), then come off the slab.
        assert_eq!(at(&doc, 16.0), 5.0);
        assert_eq!(at(&doc, 11.0), 8.0, "a alone comes off the slab");
        run(
            &mut doc,
            "model.groups.update",
            json!({ "id": group, "visible": false }),
        );
        assert_eq!(at(&doc, 16.0), 10.0, "a hidden group hides its members");
        doc.undo();
        doc.undo();
        assert!(doc.levels()[0].groups.is_empty(), "creating was one step");
        assert_eq!(doc.component(ids[1]).map(|c| c.extra.group).ok(), Some(0));
        doc.redo();
        run(&mut doc, "model.groups.ungroup", json!({ "id": group }));
        assert!(doc.levels()[0].groups.is_empty());
        assert!(doc.components().iter().all(|c| c.extra.group == 0));
        assert_eq!(at(&doc, 16.0), 15.0, "ungrouped, the bumps add again");
        assert!(doc.component(slab).is_ok());
    }

    #[test]
    fn a_group_transforms_as_one() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 30.0, 2.0)]);
        let group = run(&mut doc, "model.groups.create", json!({ "ids": ids }))["id"]
            .as_u64()
            .expect("id");
        run(
            &mut doc,
            "model.groups.transform",
            json!({ "id": group, "params": { "offset": { "x": 5.0, "y": 0.0 } } }),
        );
        let xs: Vec<f64> = doc.components().iter().map(|c| c.field.origin.x).collect();
        assert_eq!(xs, vec![15.0, 35.0]);
        doc.undo();
        let xs: Vec<f64> = doc.components().iter().map(|c| c.field.origin.x).collect();
        assert_eq!(xs, vec![10.0, 30.0]);
    }

    #[test]
    fn moving_into_levels_and_groups() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 10.0, 3.0), ("c", 10.0, 4.0)]);
        doc.begin("Level");
        let upper = doc.add_level(None, Combine::Subtract);
        doc.commit();
        // c moves up to the subtracting level: 2 + 3 - 4.
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[2]], "level": upper }),
        );
        assert_eq!(at(&doc, 14.0), 1.0);
        // a goes above b on the first level.
        let first = doc.levels()[0].id;
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[0]], "level": first, "after": ids[1] }),
        );
        let order: Vec<u64> = doc.components().iter().map(|c| c.id).collect();
        assert_eq!(order[..2], [ids[1], ids[0]]);
        // and back below b.
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[0]], "level": first, "before": ids[1] }),
        );
        let order: Vec<u64> = doc.components().iter().map(|c| c.id).collect();
        assert_eq!(order[..2], [ids[0], ids[1]]);
        // Joining a group, then leaving it, drops the emptied group.
        let group = run(&mut doc, "model.groups.create", json!({ "ids": [ids[1]] }))["id"]
            .as_u64()
            .expect("id");
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[0]], "level": first, "group": group }),
        );
        assert_eq!(members_of(&doc, group), vec![ids[1], ids[0]]);
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[0], ids[1]], "level": first }),
        );
        assert!(doc.levels()[0].groups.is_empty());
        assert!(dispatch(
            &mut doc,
            "model.tree.move",
            &json!({ "ids": [ids[0]], "level": first, "group": 999 }).to_string()
        )
        .is_err());
    }

    #[test]
    fn a_level_clips_to_its_vectors() {
        let (mut doc, ids) = doc_with(&[("slab", 10.0, 5.0), ("cut", 10.0, 2.0)]);
        doc.begin("Level");
        let upper = doc.add_level(None, Combine::Subtract);
        let vector = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(15.0, 100.0)).expect("rect"),
            },
        });
        doc.commit();
        run(
            &mut doc,
            "model.tree.move",
            json!({ "ids": [ids[1]], "level": upper }),
        );
        run(
            &mut doc,
            "model.levels.setClip",
            json!({ "id": upper, "vectorIds": [vector] }),
        );
        assert_eq!(at(&doc, 12.0), 3.0, "inside the clip the level cuts");
        assert_eq!(at(&doc, 18.0), 5.0, "outside it does not");

        // Editing the vector leaves the clipping as it was applied...
        doc.begin("Widen");
        let mut wider = doc.entity(vector).expect("vector").clone();
        wider.kind = EntityKind::Path {
            path: rect(Point::new(0.0, 0.0), Point::new(17.0, 100.0)).expect("rect"),
        };
        doc.replace_entity(vector, wider).expect("vector");
        doc.commit();
        assert_eq!(at(&doc, 16.0), 5.0, "the boundary is kept, not live");
        // ...until Update reads the same vector again.
        run(&mut doc, "model.levels.updateClip", json!({ "id": upper }));
        assert_eq!(at(&doc, 16.0), 3.0, "updated to the wider vector");
        assert_eq!(at(&doc, 18.0), 5.0);
        // Deleting the vector keeps the clipping too.
        doc.begin("Delete");
        doc.delete_entities(&[vector]);
        doc.commit();
        assert_eq!(at(&doc, 18.0), 5.0, "still clipped");
        assert!(
            dispatch(
                &mut doc,
                "model.levels.updateClip",
                &json!({ "id": upper }).to_string()
            )
            .is_err(),
            "nothing to read again"
        );

        run(
            &mut doc,
            "model.levels.setClip",
            json!({ "id": upper, "vectorIds": [] }),
        );
        assert_eq!(at(&doc, 18.0), 3.0, "removed");
        doc.undo();
        assert_eq!(at(&doc, 18.0), 5.0, "undo brings the clipping back");
    }

    #[test]
    fn from_visible_copies_the_model_and_hides_the_sources() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 15.0, 3.0)]);
        let before = doc.composite();
        let reply = run(&mut doc, "model.fromVisible", json!({}));
        let id = reply["id"].as_u64().expect("id");
        assert_eq!(doc.composite(), before, "the model looks the same");
        assert!(ids
            .iter()
            .all(|i| doc.component(*i).map(|c| !c.visible).unwrap_or(false)));
        assert!(doc.component(id).is_ok());
        doc.undo();
        assert_eq!(doc.components().len(), 2);
        assert!(doc.components().iter().all(|c| c.visible));
        let reply = run(
            &mut doc,
            "model.fromVisible",
            json!({ "ids": [ids[1]], "name": "B copy" }),
        );
        let copy = doc
            .component(reply["id"].as_u64().expect("id"))
            .expect("copy");
        assert_eq!(copy.name, "B copy");
        assert_eq!(copy.field.max(), Some(3.0));
    }

    #[test]
    fn clear_removes_every_component_in_one_undo_step() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 40.0, 3.0)]);
        run(&mut doc, "model.groups.create", json!({ "ids": ids }));
        let before = doc.composite();
        let reply = run(&mut doc, "model.clear", json!({}));
        assert_eq!(reply["deleted"], json!(ids));
        assert!(doc.components().is_empty());
        assert!(doc.levels().iter().all(|l| l.groups.is_empty()));
        assert_eq!(at(&doc, 14.0), 0.0);
        assert!(dispatch(&mut doc, "model.clear", "").is_err());
        doc.undo();
        assert_eq!(doc.components().len(), 2);
        assert_eq!(doc.levels()[0].groups.len(), 1);
        assert_eq!(doc.composite(), before);
    }

    #[test]
    fn a_level_or_group_colour_covers_its_components_and_materials_are_kept() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0)]);
        let has = |doc: &Document, rgb: [u8; 3]| {
            doc.composite_colors()
                .chunks(4)
                .any(|px| px == [rgb[0], rgb[1], rgb[2], 255])
        };
        run(
            &mut doc,
            "model.props",
            json!({ "ids": ids, "color": "#ff0000", "material": "oak" }),
        );
        assert_eq!(
            doc.component(ids[0]).map(|c| c.extra.material.clone()).ok(),
            Some(Some("oak".into()))
        );
        assert!(has(&doc, [255, 0, 0]));
        let level = doc.levels()[0].id;
        let mut session = crate::model::sculpt::SculptSession::default();
        let mut brushes = crate::model::brush_file::BrushCache::default();
        let mut level_update = |doc: &mut Document, args: Value| {
            crate::model::component_cmd::dispatch(
                doc,
                &mut session,
                &mut brushes,
                "model.levels.update",
                &args.to_string(),
            )
        };
        assert!(level_update(
            &mut doc,
            json!({ "id": level, "color": "#0000ff", "material": "walnut" })
        )
        .is_ok());
        assert_eq!(doc.levels()[0].material.as_deref(), Some("walnut"));
        assert!(has(&doc, [0, 0, 255]), "the level's colour shows");
        assert!(!has(&doc, [255, 0, 0]));
        assert!(level_update(
            &mut doc,
            json!({ "id": level, "color": null, "material": null })
        )
        .is_ok());
        assert!(has(&doc, [255, 0, 0]), "colour from the components again");
        run(&mut doc, "model.groups.create", json!({ "ids": ids }));
        let group = doc.levels()[0].groups[0].id;
        run(
            &mut doc,
            "model.groups.update",
            json!({ "id": group, "color": "#00ff00", "material": "maple" }),
        );
        assert_eq!(doc.levels()[0].groups[0].material.as_deref(), Some("maple"));
        assert!(has(&doc, [0, 255, 0]));
        assert!(dispatch(
            &mut doc,
            "model.props",
            &json!({ "ids": ids, "material": " " }).to_string()
        )
        .is_err());
        assert!(level_update(&mut doc, json!({ "id": level, "color": "blue" })).is_err());
    }

    #[test]
    fn colors_come_back_as_a_palette_and_an_index() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0)]);
        run(
            &mut doc,
            "model.props",
            json!({ "ids": ids, "color": "#ff8000" }),
        );
        let Ok(KernelReply::Bytes(bytes)) = dispatch(&mut doc, "model.colors", "") else {
            panic!("a blob");
        };
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
        assert_eq!(header["any"], json!(true));
        let sections = header["sections"].as_array().expect("sections");
        assert_eq!(sections[0]["name"], json!("palette"));
        assert_eq!(sections[1]["name"], json!("index"));
        assert_eq!(sections[1]["dtype"], json!("u8"));
        let payload = &bytes[4 + len..];
        let at = |i: usize| sections[i]["offset"].as_u64().expect("offset") as usize;
        let palette = &payload[at(0)..at(0) + 8];
        assert_eq!(palette, &[0, 0, 0, 0, 255, 128, 0, 255]);
        let grid = grid_for(doc.job());
        let cells = grid.width * grid.height;
        assert_eq!(sections[1]["count"].as_u64(), Some(cells as u64));
        let index = &payload[at(1)..at(1) + cells];
        assert!(index.contains(&1) && index.contains(&0));
        assert!(index.iter().all(|e| *e <= 1));
        // A quarter of the RGBA bytes, give or take the palette.
        assert!(bytes.len() < cells + 1024);
    }

    #[test]
    fn each_palette_entry_names_the_material_it_came_from() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 30.0, 2.0)]);
        // The same colour as a plain colour and as a material: two entries.
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[0]], "color": "#b98c57", "material": "oak" }),
        );
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[1]], "color": "#b98c57" }),
        );
        let Ok(KernelReply::Bytes(bytes)) = dispatch(&mut doc, "model.colors", "") else {
            panic!("a blob");
        };
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
        let mut materials = header["materials"].as_array().expect("materials").clone();
        assert_eq!(materials.len(), 3);
        assert_eq!(materials.remove(0), Value::Null);
        materials.sort_by_key(|m| m.is_null());
        assert_eq!(materials, vec![json!("oak"), Value::Null]);
    }

    #[test]
    fn the_highlight_marks_seen_and_hidden_cells() {
        use crate::doc::{HIGHLIGHT_HIDDEN, HIGHLIGHT_SEEN};
        // "low" sits under "tall" where they overlap (x 15 to 20), so it is
        // hidden there and seen on its own (x 10 to 15).
        let (mut doc, ids) = doc_with(&[("low", 10.0, 2.0), ("tall", 15.0, 5.0)]);
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[1]], "combine": "mergeHigh" }),
        );
        let Ok(KernelReply::Bytes(bytes)) = dispatch(
            &mut doc,
            "model.highlight",
            &json!({ "ids": [ids[0]] }).to_string(),
        ) else {
            panic!("a blob");
        };
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
        let mask = &bytes[4 + len..];
        let grid = grid_for(doc.job());
        assert_eq!(header["width"], json!(grid.width));
        assert_eq!(mask.len(), grid.width * grid.height);
        let cell = |x: f64| {
            let col = ((x - grid.origin.x) / grid.cell).round() as usize;
            let row = ((14.0 - grid.origin.y) / grid.cell).round() as usize;
            mask[row * grid.width + col]
        };
        assert_eq!(cell(12.0), HIGHLIGHT_SEEN);
        assert_eq!(cell(17.0), HIGHLIGHT_HIDDEN);
        assert_eq!(cell(22.0), 0, "only the chosen component");
        assert_eq!(cell(60.0), 0);
        let (_, none) = doc.highlight_mask(&[]);
        assert!(none.iter().all(|m| *m == 0));

        // The low component on its own shows everywhere it reaches.
        let alone = doc.composite_of(&[ids[0]]);
        assert_eq!(alone.at(Point::new(17.0, 14.0)), 2.0);
        assert_eq!(alone.at(Point::new(22.0, 14.0)), 0.0);
        assert_eq!(doc.composite().at(Point::new(17.0, 14.0)), 5.0);
    }

    #[test]
    fn tree_data_survives_a_save_and_open() {
        let (mut doc, ids) = doc_with(&[("a", 10.0, 2.0), ("b", 15.0, 3.0)]);
        run(
            &mut doc,
            "model.groups.create",
            json!({ "ids": ids, "combine": "multiply" }),
        );
        run(
            &mut doc,
            "model.props",
            json!({ "ids": [ids[0]], "baseHeight": 2.5, "color": "#010203" }),
        );
        let mut path = std::env::temp_dir();
        path.push(format!("carve-tree-{}.carve", std::process::id()));
        crate::io::native::save(&doc, &path).expect("saves");
        let back = crate::io::native::open(&path, doc.revision()).expect("opens");
        std::fs::remove_file(&path).ok();
        assert_eq!(back, doc);
        assert_eq!(back.levels()[0].groups[0].combine, Combine::Multiply);
        assert_eq!(
            back.component(ids[0]).map(|c| c.extra.base_height).ok(),
            Some(2.5)
        );
        assert_eq!(back.composite(), doc.composite());
    }
}
