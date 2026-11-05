//! Relief component and level commands: edit, sculpt, move, bake, flatten,
//! list, reorder and delete components, manage levels, and read the model
//! back as a composite, boundary vectors or an STL file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::blob;
use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{Point, Tolerance};
use crate::io;
use crate::kernel::{edit, parse_args, IdArgs, IdsArgs, KernelReply};
use crate::model;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.edit"
            | "model.boundary"
            | "model.exportStl"
            | "model.sculpt"
            | "model.sculptKeep"
            | "model.sculptRevert"
            | "model.sculptEnd"
            | "model.createBrush"
            | "model.brushFolder"
            | "model.transform"
            | "model.transformMatrix"
            | "model.copyMatrix"
            | "model.toOtherSide"
            | "model.bake"
            | "model.replaceBelow"
            | "model.list"
            | "model.levels.add"
            | "model.levels.update"
            | "model.levels.delete"
            | "model.levels.reorder"
            | "model.levels.setActive"
            | "model.update"
            | "model.delete"
            | "model.reorder"
            | "model.composite"
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
    session: &mut model::sculpt::SculptSession,
    brushes: &mut model::brush_file::BrushCache,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "model.edit" => {
            let args: EditArgs = parse_args(name, args)?;
            let current = document.component(args.id)?.field.clone();
            use model::edits;
            let edited = match &args.edit {
                ComponentEdit::Smooth(p) => edits::smooth(&current, p)?,
                ComponentEdit::Draft(p) => edits::add_draft(&current, p)?,
                ComponentEdit::Tilt(p) => edits::tilt(&current, p)?,
                ComponentEdit::Fade(p) => edits::fade(&current, p)?,
                ComponentEdit::Mirror(p) => edits::mirror(&current, p),
                ComponentEdit::Flip => edits::flip_vertical(&current),
                ComponentEdit::Scale(p) => edits::scale_heights(&current, p)?,
                ComponentEdit::Emboss(p) => edits::emboss(&current, p)?,
            };
            let field = edited
                .cropped()
                .ok_or_else(|| CoreError::validation("the edit leaves the component empty"))?;
            edit(document, args.edit.label(), |doc| {
                doc.update_component(args.id, |c| c.field = field)?;
                Ok(json!({}))
            })
        }
        "model.boundary" => {
            let args: ModelBoundaryArgs = parse_args(name, args)?;
            let field = covered_field(document, args.ids.as_deref())?;
            // No level: the silhouette, round every cell with material.
            let level = args.level.map_or(f32::NEG_INFINITY, |l| l as f32);
            let paths =
                model::contour::boundary(&field, level, args.offset, Tolerance::new(0.01)?)?;
            if paths.is_empty() {
                return Err(CoreError::validation(
                    "there is no model above that level to outline",
                ));
            }
            let layer = document.active_layer();
            let sheet = document.active_sheet();
            edit(document, "Create Boundary Vectors", |doc| {
                let ids: Vec<u64> = paths
                    .into_iter()
                    .map(|path| {
                        doc.insert_entity(Entity {
                            rotation: 0.0,
                            id: 0,
                            layer,
                            sheet,
                            revision: 0,
                            name: None,
                            parent: None,
                            kind: EntityKind::Path { path },
                        })
                    })
                    .collect();
                // Several loops come as one group, easier to pick together.
                if args.group && ids.len() > 1 {
                    let group = doc.group(&ids)?;
                    return Ok(json!({ "ids": [group], "vectors": ids }));
                }
                Ok(json!({ "ids": ids, "vectors": ids }))
            })
        }
        "model.exportStl" => {
            let args: ExportStlArgs = parse_args(name, args)?;
            let field = model_field(document, args.ids.as_deref())?;
            let lowest = f64::from(field.min().unwrap_or(0.0)).min(0.0);
            io::stl_export::write_stl(&field, lowest - args.base, &args.path)?;
            KernelReply::ok(&json!({}))
        }
        "model.sculpt" => {
            let args: SculptArgs = parse_args(name, args)?;
            let current = document.component(args.id)?.field.clone();
            let baseline = session
                .baselines
                .entry(args.id)
                .or_insert_with(|| current.clone());
            // A graphics tablet's pen pressure scales the strength: the stroke
            // runs as pieces of even pressure, one after the other.
            let mut sculpted = current;
            for (points, pressure) in pressure_runs(&args.points, args.pressures.as_deref()) {
                let params = model::sculpt::BrushParams {
                    strength: args.params.strength * pressure,
                    ..args.params
                };
                sculpted = if let Some(custom) = &args.custom {
                    let owned;
                    let shape = match (&custom.file, custom.brush) {
                        (Some(file), _) => brushes.get(file)?,
                        (None, Some(id)) => {
                            let brush = document.component(id)?;
                            owned = model::custom_brush::BrushShape::from_field(&brush.field)?;
                            &owned
                        }
                        (None, None) => {
                            return Err(CoreError::validation("choose a brush first"));
                        }
                    };
                    model::custom_brush::custom_stroke(
                        &sculpted,
                        baseline,
                        &points,
                        &params,
                        shape,
                        &custom.hold,
                    )?
                } else if params.brush == model::sculpt::Brush::Restore {
                    model::sculpt::stroke_restoring(&sculpted, baseline, &points, &params)?
                } else {
                    model::sculpt::stroke(&sculpted, &points, &params)?
                };
            }
            let field = sculpted.cropped().ok_or_else(|| {
                CoreError::validation("that would erase the whole component; delete it instead")
            })?;
            edit(document, "Sculpt", |doc| {
                doc.update_component(args.id, |c| c.field = field)?;
                Ok(json!({}))
            })
        }
        "model.sculptKeep" => {
            let args: IdArgs = parse_args(name, args)?;
            let current = document.component(args.id)?.field.clone();
            session.kept.insert(args.id, current);
            KernelReply::ok(&json!({}))
        }
        "model.sculptRevert" => {
            let args: SculptRevertArgs = parse_args(name, args)?;
            document.component(args.id)?;
            let to_start = args.to == "start";
            let target = if to_start {
                session.baselines.get(&args.id)
            } else {
                session
                    .kept
                    .get(&args.id)
                    .or_else(|| session.baselines.get(&args.id))
            }
            .cloned();
            if to_start {
                session.kept.remove(&args.id);
            }
            let Some(field) = target else {
                return KernelReply::ok(
                    &json!({ "reverted": false, "revision": document.revision() }),
                );
            };
            let reply = edit(document, "Discard Sculpting", |doc| {
                doc.update_component(args.id, |c| c.field = field)?;
                Ok(json!({ "reverted": true }))
            })?;
            Ok(reply)
        }
        "model.sculptEnd" => {
            session.clear();
            brushes.clear();
            KernelReply::ok(&json!({}))
        }
        "model.createBrush" => {
            let args: CreateBrushArgs = parse_args(name, args)?;
            let component = document.component(args.id)?;
            let label = args.name.unwrap_or_else(|| component.name.clone());
            let dir = args.dir.unwrap_or_else(model::brush_file::brush_dir);
            let path = model::brush_file::save_brush(&component.field, &label, &dir)?;
            KernelReply::ok(&json!({ "path": path.to_string_lossy() }))
        }
        "model.brushFolder" => {
            let dir = model::brush_file::brush_dir();
            std::fs::create_dir_all(&dir).map_err(|e| CoreError::io(dir.display(), e))?;
            KernelReply::ok(&json!({ "path": dir.to_string_lossy() }))
        }
        "model.transform" => {
            let args: ModelTransformArgs = parse_args(name, args)?;
            document.component(args.id)?;
            // A live drag in the 3D view replaces its own previous step, so
            // the whole drag is one undo step measured from where it began.
            let undone = args.replace.is_some_and(|revision| {
                document.revision() == revision
                    && document.journal().undo_label() == Some(TRANSFORM)
                    && document.undo().is_some()
            });
            let result = transform_one(document, &args);
            if result.is_err() && undone {
                document.redo();
            }
            result
        }
        "model.transformMatrix" => {
            let args: MatrixArgs = parse_args(name, args)?;
            if args.ids.is_empty() {
                return Err(CoreError::validation("choose one or more components"));
            }
            let [a, b, c, d, e, f] = args.matrix;
            let m = crate::geom::Mat3::new(a, b, c, d, e, f);
            // A live drag replaces its own previous step, so the matrix is
            // measured from where the drag began.
            let undone = args.replace.is_some_and(|revision| {
                document.revision() == revision
                    && document.journal().undo_label() == Some(TRANSFORM_MANY)
                    && document.undo().is_some()
            });
            let moved = args
                .ids
                .iter()
                .map(|id| {
                    let (field, mut extra) = moved_by(document, *id, &m)?;
                    follow_shape(&mut extra, &m);
                    Ok((*id, field, extra))
                })
                .collect::<Result<Vec<_>, CoreError>>();
            let moved = match moved {
                Ok(moved) => moved,
                Err(error) => {
                    if undone {
                        document.redo();
                    }
                    return Err(error);
                }
            };
            let z = if args.scale_z {
                (a * d - b * c).abs().sqrt()
            } else {
                1.0
            };
            let result = edit(document, TRANSFORM_MANY, |doc| {
                for (id, field, extra) in moved {
                    doc.update_component(id, |c| {
                        c.field = field;
                        c.extra = extra;
                        c.height_scale *= z;
                    })?;
                }
                Ok(json!({}))
            });
            if result.is_err() && undone {
                document.redo();
            }
            result
        }
        "model.toOtherSide" => to_other_side(document, parse_args(name, args)?),
        "model.copyMatrix" => {
            let args: MatrixArgs = parse_args(name, args)?;
            if args.ids.is_empty() && args.vectors.is_empty() {
                return Err(CoreError::validation("choose one or more objects to copy"));
            }
            let [a, b, c, d, e, f] = args.matrix;
            let m = crate::geom::Mat3::new(a, b, c, d, e, f);
            for id in &args.vectors {
                document.entity(*id)?;
            }
            let label = if args.vectors.is_empty() {
                "Copy Components"
            } else {
                "Copy"
            };
            edit(document, label, |doc| {
                let mut map = BTreeMap::new();
                let mut vectors = Vec::with_capacity(args.vectors.len());
                for id in &args.vectors {
                    vectors.push(crate::doc::ops::duplicate_mapped(doc, *id, &mut map)?);
                }
                doc.transform_entities(&vectors, &m);
                let ids = copy_components(doc, &args.ids, &m, &map)?;
                if !vectors.is_empty() {
                    doc.set_selection(&vectors);
                }
                Ok(json!({ "ids": ids, "vectors": vectors }))
            })
        }
        "model.bake" => {
            let args: BakeArgs = parse_args(name, args)?;
            if args.ids.is_empty() {
                return Err(CoreError::validation("choose the components to bake"));
            }
            for id in &args.ids {
                document.component(*id)?;
            }
            let chosen: Vec<&model::Component> = document
                .components()
                .into_iter()
                .filter(|c| args.ids.contains(&c.id))
                .collect();
            let level = chosen[0].level;
            let field = model::combine_ops::merge(&chosen, document.levels()).ok_or_else(|| {
                CoreError::validation("none of those components is visible to bake")
            })?;
            let lowest = chosen[0].id;
            let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
            let taken = document.components().len();
            // One component baked alone keeps its name: only its fade, tilt,
            // heights and combine mode are made part of its shape.
            let own_name = (chosen.len() == 1).then(|| chosen[0].name.clone());
            let name_given = args
                .name
                .filter(|n| !n.trim().is_empty())
                .or(own_name)
                .unwrap_or_else(|| format!("Baked {}", taken + 1));
            let sources = args.ids;
            edit(document, "Bake Components", |doc| {
                let id = doc.add_component(model::Component {
                    id: 0,
                    name: name_given,
                    visible: true,
                    combine: model::Combine::Add,
                    height_scale: 1.0,
                    level,
                    extra: Default::default(),
                    field,
                });
                let new_order: Vec<u64> = order
                    .iter()
                    .filter_map(|c| {
                        if *c == lowest {
                            Some(id)
                        } else if sources.contains(c) {
                            None
                        } else {
                            Some(*c)
                        }
                    })
                    .collect();
                doc.delete_components(&sources)?;
                doc.reorder_components(&new_order)?;
                Ok(json!({ "id": id }))
            })
        }
        "model.replaceBelow" => replace_below(document, parse_args(name, args)?),
        "model.list" => {
            let list: Vec<Value> = document
                .components()
                .into_iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "visible": c.visible,
                        "combine": c.combine,
                        "heightScale": c.height_scale,
                        "level": c.level,
                        "baseHeight": c.extra.base_height,
                        "color": c.extra.color,
                        "material": c.extra.material,
                        "group": c.extra.group,
                        "imported": c.extra.import.is_some(),
                        "maxHeight": top_of(c),
                        "shape": c.extra.shape,
                        "texture": c.extra.texture,
                        "fade": c.extra.fade,
                        "tilt": c.extra.tilt,
                        "bbox": [
                            c.field.origin.x - c.field.cell / 2.0,
                            c.field.origin.y - c.field.cell / 2.0,
                            c.field.origin.x + (c.field.width as f64 - 0.5) * c.field.cell,
                            c.field.origin.y + (c.field.height as f64 - 0.5) * c.field.cell,
                        ],
                    })
                })
                .collect();
            KernelReply::ok(&json!({
                "stamp": document.model_stamp(),
                "components": list,
                "levels": document.levels(),
                "activeLevel": document.active_level(),
            }))
        }
        "model.levels.add" => {
            let args: LevelAddArgs = parse_args(name, args)?;
            edit(document, "Add Level", |doc| {
                let id = doc.add_level(args.name, args.combine);
                Ok(json!({ "id": id }))
            })
        }
        "model.levels.update" => {
            let args: LevelUpdateArgs = parse_args(name, args)?;
            if let Some(color) = &args.color {
                super::tree_cmd::check_color(color.as_deref())?;
            }
            super::tree_cmd::check_material(args.material.as_ref())?;
            if args.base_height.is_some_and(|b| !b.is_finite()) {
                return Err(CoreError::validation("the base height must be a number"));
            }
            edit(document, "Edit Level", |doc| {
                doc.update_level(args.id, |l| {
                    if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
                        l.name = name;
                    }
                    if let Some(visible) = args.visible {
                        l.visible = visible;
                    }
                    if let Some(combine) = args.combine {
                        l.combine = combine;
                    }
                    if let Some(color) = args.color {
                        l.color = color;
                    }
                    if let Some(material) = args.material {
                        l.material = material;
                    }
                    if let Some(mirror) = args.mirror {
                        l.mirror = mirror;
                    }
                    if let Some(base) = args.base_height {
                        l.base_height = base;
                    }
                })?;
                Ok(json!({}))
            })
        }
        "model.levels.delete" => {
            let args: IdArgs = parse_args(name, args)?;
            edit(document, "Delete Level", |doc| {
                doc.delete_level(args.id)?;
                Ok(json!({}))
            })
        }
        "model.levels.reorder" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = args.ids.unwrap_or_default();
            edit(document, "Reorder Levels", |doc| {
                doc.reorder_levels(&ids)?;
                Ok(json!({}))
            })
        }
        "model.levels.setActive" => {
            let args: IdArgs = parse_args(name, args)?;
            edit(document, "Set Active Level", |doc| {
                doc.set_active_level(args.id)?;
                Ok(json!({}))
            })
        }
        "model.update" => {
            let args: ComponentUpdateArgs = parse_args(name, args)?;
            if let Some(scale) = args.height_scale {
                if !scale.is_finite() {
                    return Err(CoreError::validation("the height scale must be a number"));
                }
            }
            if let Some(level) = args.level {
                if !document.levels().iter().any(|l| l.id == level) {
                    return Err(CoreError::validation(format!("no level with id {level}")));
                }
            }
            edit(document, "Edit Component", |doc| {
                doc.update_component(args.id, |c| {
                    if let Some(level) = args.level {
                        c.level = level;
                    }
                    if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
                        c.name = name;
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
                })?;
                Ok(json!({}))
            })
        }
        "model.delete" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = args.ids.unwrap_or_default();
            if ids.is_empty() {
                return Err(CoreError::validation("no components were chosen to delete"));
            }
            edit(document, "Delete Component", |doc| {
                doc.delete_components(&ids)?;
                Ok(json!({}))
            })
        }
        "model.reorder" => {
            let args: IdsArgs = parse_args(name, args)?;
            edit(document, "Reorder Components", |doc| {
                doc.reorder_components(&args.ids.unwrap_or_default())?;
                Ok(json!({}))
            })
        }
        "model.composite" => {
            let args: CompositeArgs = parse_args(name, args)?;
            let composite = if args.other_side {
                other_side_turned(document)?
            } else {
                match &args.ids {
                    Some(ids) => document.composite_of(ids),
                    None => document.composite(),
                }
            };
            Ok(KernelReply::Bytes(composite_blob(document, &composite)))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The `model.composite` reply: heights as `u16` steps between the lowest
/// and highest cell when that rounds them by no more than
/// [`blob::HEIGHT_TOLERANCE`] of a cell, as `f32` otherwise.
fn composite_blob(document: &Document, composite: &model::field::Field) -> Vec<u8> {
    let (min, max) = (composite.min(), composite.max());
    let code = min.zip(max).and_then(|(lo, hi)| {
        blob::HeightCode::fit(lo, hi, composite.cell * blob::HEIGHT_TOLERANCE)
    });
    let mut header = json!({
        "revision": document.revision(),
        "width": composite.width,
        "height": composite.height,
        "cellSize": composite.cell,
        "originX": composite.origin.x,
        "originY": composite.origin.y,
        "min": min,
        "max": max,
        // The other side's model peak, so the view can place both sides on
        // their shared modelling plane.
        "otherPeak": document.other_side_peak(),
    });
    blob::describe_heights(&mut header, code.as_ref());
    blob::frame(header, &[blob::heights("z", &composite.z, code.as_ref())])
}

/// The model on its grid, cut down to the box the components cover plus
/// a cell of margin: every visible component, or only the chosen ones,
/// baked with empty cells at the zero plane.
fn model_field(document: &Document, ids: Option<&[u64]>) -> Result<model::field::Field, CoreError> {
    let chosen = chosen_components(document, ids)?;
    if chosen.is_empty() {
        return Err(CoreError::validation("there is no model yet"));
    }
    let grid = model::grid_for(document.job());
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &chosen {
        let f = &c.field;
        lo_x = lo_x.min(f.origin.x);
        lo_y = lo_y.min(f.origin.y);
        hi_x = hi_x.max(f.origin.x + (f.width as f64 - 1.0) * f.cell);
        hi_y = hi_y.max(f.origin.y + (f.height as f64 - 1.0) * f.cell);
    }
    let cell_of = |v: f64, o: f64, n: usize| {
        ((v - o) / grid.cell)
            .round()
            .clamp(0.0, n.saturating_sub(1) as f64) as usize
    };
    let i0 = cell_of(lo_x, grid.origin.x, grid.width).saturating_sub(1);
    let j0 = cell_of(lo_y, grid.origin.y, grid.height).saturating_sub(1);
    let i1 = (cell_of(hi_x, grid.origin.x, grid.width) + 1).min(grid.width - 1);
    let j1 = (cell_of(hi_y, grid.origin.y, grid.height) + 1).min(grid.height - 1);
    let local = model::shape::Grid {
        width: i1 - i0 + 1,
        height: j1 - j0 + 1,
        cell: grid.cell,
        origin: Point::new(
            grid.origin.x + i0 as f64 * grid.cell,
            grid.origin.y + j0 as f64 * grid.cell,
        ),
    };
    Ok(model::bake(&chosen, &local))
}

/// The components `ids` names (shown whatever their visibility), or every
/// visible one when it names none.
fn chosen_components(
    document: &Document,
    ids: Option<&[u64]>,
) -> Result<Vec<model::Component>, CoreError> {
    Ok(match ids {
        None | Some([]) => document
            .components()
            .into_iter()
            .filter(|c| c.visible)
            .cloned()
            .collect(),
        Some(ids) => ids
            .iter()
            .map(|id| {
                document.component(*id).map(|c| model::Component {
                    visible: true,
                    ..c.clone()
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// As [`model_field`], but empty (NaN) wherever no component reaches, so a
/// silhouette follows the components rather than the box round them.
fn covered_field(
    document: &Document,
    ids: Option<&[u64]>,
) -> Result<model::field::Field, CoreError> {
    let full = model_field(document, ids)?;
    let chosen = chosen_components(document, ids)?;
    let local = model::shape::Grid {
        width: full.width,
        height: full.height,
        cell: full.cell,
        origin: full.origin,
    };
    let (mut field, covered) = model::bake_covering(&chosen, &local);
    for (z, reached) in field.z.iter_mut().zip(covered) {
        if !reached {
            *z = f32::NAN;
        }
    }
    Ok(field)
}

/// One component edit and its settings, tagged `op` on the wire.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "params", rename_all = "camelCase")]
enum ComponentEdit {
    Smooth(model::edits::SmoothParams),
    Draft(model::edits::DraftParams),
    Tilt(model::edits::TiltParams),
    Fade(model::edits::FadeParams),
    Mirror(model::edits::MirrorParams),
    Flip,
    Scale(model::edits::ScaleParams),
    Emboss(model::edits::EmbossParams),
}

impl ComponentEdit {
    fn label(&self) -> &'static str {
        match self {
            ComponentEdit::Smooth(_) => "Smooth Component",
            ComponentEdit::Draft(_) => "Add Draft",
            ComponentEdit::Tilt(_) => "Tilt Component",
            ComponentEdit::Fade(_) => "Fade Component",
            ComponentEdit::Mirror(_) => "Mirror Component",
            ComponentEdit::Flip => "Flip Component",
            ComponentEdit::Scale(_) => "Scale Component",
            ComponentEdit::Emboss(_) => "Emboss Component",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditArgs {
    id: u64,
    #[serde(flatten)]
    edit: ComponentEdit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SculptArgs {
    id: u64,
    points: Vec<Point>,
    #[serde(default)]
    params: model::sculpt::BrushParams,
    /// Sculpt with another component's shape instead of the round brush.
    #[serde(default)]
    custom: Option<CustomBrushArgs>,
    /// Pen pressure at each point, 0 to 1, from a graphics tablet.
    #[serde(default)]
    pressures: Option<Vec<f64>>,
}

/// Steps pen pressure is rounded to when a stroke is split by it.
const PRESSURE_STEPS: f64 = 10.0;

/// `points` split into runs of even pen pressure, each with its pressure
/// factor; each run starts at the end of the one before. Without pressures,
/// one run at full strength.
fn pressure_runs(points: &[Point], pressures: Option<&[f64]>) -> Vec<(Vec<Point>, f64)> {
    let Some(pressures) = pressures.filter(|p| p.len() == points.len()) else {
        return vec![(points.to_vec(), 1.0)];
    };
    let level = |p: f64| {
        let p = if p.is_finite() {
            p.clamp(0.0, 1.0)
        } else {
            1.0
        };
        ((p * PRESSURE_STEPS).round() / PRESSURE_STEPS).max(1.0 / PRESSURE_STEPS)
    };
    let mut runs: Vec<(Vec<Point>, f64)> = Vec::new();
    for (point, pressure) in points.iter().zip(pressures) {
        let level = level(*pressure);
        match runs.last_mut() {
            Some((run, at)) if (*at - level).abs() < 1e-9 => run.push(*point),
            Some((run, _)) => {
                let from = run.last().copied().unwrap_or(*point);
                runs.push((vec![from, *point], level));
            }
            None => runs.push((vec![*point], level)),
        }
    }
    runs
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomBrushArgs {
    /// The component whose shape is the brush.
    #[serde(default)]
    brush: Option<u64>,
    /// A brush file instead: a clip or a picture.
    #[serde(default)]
    file: Option<std::path::PathBuf>,
    #[serde(flatten)]
    hold: model::custom_brush::CustomBrush,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBrushArgs {
    id: u64,
    /// The brush's name; the component's when absent.
    #[serde(default)]
    name: Option<String>,
    /// Where to write it; the brushes folder when absent.
    #[serde(default)]
    dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompositeArgs {
    /// Only these components, as they show on their own; the whole visible
    /// model when absent.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// The other side's model of a double sided job, turned over onto the
    /// active side's grid (Multi Sided View), instead.
    #[serde(default)]
    other_side: bool,
}

/// The other side's visible model turned over onto the active side's model
/// grid: each cell holds the other side's height (above that side's zero
/// plane) at the same point of the material.
///
/// # Errors
/// [`CoreError::Validation`] when the job is not double sided or the other
/// side has no model.
fn other_side_turned(document: &Document) -> Result<model::field::Field, CoreError> {
    let job = document.job();
    let other = document
        .other_side_composite()
        .filter(|_| job.is_double_sided())
        .ok_or_else(|| CoreError::validation("the other side has no model"))?;
    let mut turned = document.composite();
    let width = turned.width;
    for j in 0..turned.height {
        for i in 0..width {
            let c = turned.centre(i, j);
            let (x, y) = job.flip_xy(c.x, c.y);
            let oi = ((x - other.origin.x) / other.cell).round() as i64;
            let oj = ((y - other.origin.y) / other.cell).round() as i64;
            turned.z[j * width + i] = other.get(oi, oj);
        }
    }
    Ok(turned)
}

/// Undo label of `model.transform`.
const TRANSFORM: &str = "Transform Component";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelTransformArgs {
    id: u64,
    #[serde(default)]
    params: model::transform::TransformParams,
    /// The revision the previous step of the same drag answered; that step
    /// is taken back first while it is still the newest.
    #[serde(default)]
    replace: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelBoundaryArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    offset: f64,
    /// Outline only what stands above this height; none for the whole
    /// silhouette.
    #[serde(default)]
    level: Option<f64>,
    /// Put several loops into one group.
    #[serde(default)]
    group: bool,
}

fn one_millimetre() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportStlArgs {
    path: PathBuf,
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Thickness of the solid base under the lowest point, millimetres.
    #[serde(default = "one_millimetre")]
    base: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BakeArgs {
    ids: Vec<u64>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplaceBelowArgs {
    /// Components to work on; when empty, the visible components of
    /// `level`.
    #[serde(default)]
    ids: Vec<u64>,
    #[serde(default)]
    level: Option<u64>,
    /// Height below which the model is replaced, mm, as the chosen
    /// components show on their own.
    height: f64,
    #[serde(default)]
    with: model::combine_ops::ReplaceWith,
    /// Drop what is left by `height`.
    #[serde(default)]
    lower: bool,
}

/// Replace Below: one component is replaced in place; several (or a
/// level's) are baked into one component first, which takes the place of
/// the lowest of them. One undo step.
fn replace_below(
    document: &mut Document,
    args: ReplaceBelowArgs,
) -> Result<KernelReply, CoreError> {
    use model::combine_ops::{merge, replace_below};
    if !args.height.is_finite() {
        return Err(CoreError::validation("the height must be a number"));
    }
    let ids: Vec<u64> = if args.ids.is_empty() {
        let level = args.level.ok_or_else(|| {
            CoreError::validation("select components or a level to replace below")
        })?;
        document
            .components()
            .into_iter()
            .filter(|c| c.level == level && c.visible)
            .map(|c| c.id)
            .collect()
    } else {
        for id in &args.ids {
            document.component(*id)?;
        }
        args.ids.clone()
    };
    let nothing = || CoreError::validation("nothing of the model is above that height");
    match ids.as_slice() {
        [] => Err(CoreError::validation(
            "the level has no visible components to replace below",
        )),
        [id] => {
            let id = *id;
            let c = document.component(id)?;
            if c.height_scale == 0.0 {
                return Err(CoreError::validation(
                    "a component with a height scale of zero has nothing to replace",
                ));
            }
            let field = replace_below(
                &c.field,
                c.height_scale,
                c.extra.base_height,
                args.height,
                args.with,
                args.lower,
            )
            .ok_or_else(nothing)?;
            edit(document, "Replace Below", |doc| {
                doc.update_component(id, |c| c.field = field)?;
                Ok(json!({ "id": id }))
            })
        }
        _ => {
            let chosen: Vec<&model::Component> = document
                .components()
                .into_iter()
                .filter(|c| ids.contains(&c.id))
                .collect();
            let first = chosen
                .first()
                .ok_or_else(|| CoreError::validation("those components are on another sheet"))?;
            let level = first.level;
            let lowest = first.id;
            let name = format!("{} (replaced below)", first.name);
            let baked = merge(&chosen, document.levels())
                .ok_or_else(|| CoreError::validation("none of those components is visible"))?;
            let field = replace_below(&baked, 1.0, 0.0, args.height, args.with, args.lower)
                .ok_or_else(nothing)?;
            let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
            edit(document, "Replace Below", |doc| {
                let id = doc.add_component(model::Component {
                    id: 0,
                    name,
                    visible: true,
                    combine: model::Combine::Add,
                    height_scale: 1.0,
                    level,
                    extra: Default::default(),
                    field,
                });
                let new_order: Vec<u64> = order
                    .iter()
                    .filter_map(|c| {
                        if *c == lowest {
                            Some(id)
                        } else if ids.contains(c) {
                            None
                        } else {
                            Some(*c)
                        }
                    })
                    .collect();
                doc.delete_components(&ids)?;
                doc.reorder_components(&new_order)?;
                Ok(json!({ "id": id }))
            })
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentUpdateArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    combine: Option<model::Combine>,
    #[serde(default)]
    height_scale: Option<f64>,
    #[serde(default)]
    level: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LevelAddArgs {
    name: Option<String>,
    combine: model::Combine,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LevelUpdateArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    combine: Option<model::Combine>,
    /// Preview colour for the whole level; null takes the components'.
    #[serde(default, deserialize_with = "super::tree_cmd::present")]
    color: Option<Option<String>>,
    /// Name of the preview material the colour comes from; null for none.
    #[serde(default, deserialize_with = "super::tree_cmd::present")]
    material: Option<Option<String>>,
    /// Mirror mode; null for none.
    #[serde(default, deserialize_with = "super::tree_cmd::present")]
    mirror: Option<Option<model::MirrorMode>>,
    /// Base height of the level, millimetres.
    #[serde(default)]
    base_height: Option<f64>,
}

/// The highest point of a component as it bakes, dynamic properties and
/// all; none for an empty one.
pub(crate) fn top_of(c: &model::Component) -> Option<f64> {
    let e = &c.extra;
    if e.fade.is_none() && e.tilt.is_none() {
        return c
            .field
            .max()
            .map(|v| f64::from(v) * c.height_scale + e.base_height);
    }
    model::dynamic::bake_field(
        &c.field,
        c.height_scale,
        e.base_height,
        e.fade.as_ref(),
        e.tilt.as_ref(),
    )
    .max()
    .map(f64::from)
}

/// Adds copies of the components `ids` moved by `m`, in that order, and
/// answers their ids. Each keeps everything its original has, its Create
/// Shape settings too: `copied` maps the vectors copied with them (original
/// id to copy), and a shape whose vectors were all copied is made from the
/// copies and lies on them; any other copy keeps its original's vectors
/// and records where it lies from them.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown component or a copy that would
/// be empty.
pub(crate) fn copy_components(
    doc: &mut Document,
    ids: &[u64],
    m: &crate::geom::Mat3,
    copied: &BTreeMap<u64, u64>,
) -> Result<Vec<u64>, CoreError> {
    let mut copies = Vec::with_capacity(ids.len());
    for id in ids {
        copies.push(copy_of(doc, doc.component(*id)?, m, copied)?);
    }
    Ok(copies.into_iter().map(|c| doc.add_component(c)).collect())
}

/// A copy of `component` (in the document or not) moved by `m`, not yet
/// added; see [`copy_components`] for its shape settings.
///
/// # Errors
/// [`CoreError::Validation`] when the copy would be empty.
pub(crate) fn copy_of(
    doc: &Document,
    component: &model::Component,
    m: &crate::geom::Mat3,
    copied: &BTreeMap<u64, u64>,
) -> Result<model::Component, CoreError> {
    let (field, mut extra) = moved_of(doc, component, m)?;
    if let Some(shape) = extra.shape.as_deref_mut() {
        let own = shape.all_vectors();
        let with_copies = !own.is_empty() && own.iter().all(|v| copied.contains_key(v));
        shape.moved(m, with_copies);
        if with_copies {
            *shape = shape.remapped(&|v| copied.get(&v).copied());
        }
    }
    let mut copy = component.clone();
    copy.field = field;
    copy.extra = extra;
    Ok(copy)
}

/// The box round the components `ids`, none for no ids.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown component.
pub(crate) fn components_box(
    doc: &Document,
    ids: &[u64],
) -> Result<Option<crate::geom::BBox>, CoreError> {
    let mut out: Option<crate::geom::BBox> = None;
    for id in ids {
        let f = &doc.component(*id)?.field;
        let half = f.cell / 2.0;
        let b = crate::geom::BBox::new(
            Point::new(f.origin.x - half, f.origin.y - half),
            Point::new(
                f.origin.x + (f.width as f64 - 0.5) * f.cell,
                f.origin.y + (f.height as f64 - 0.5) * f.cell,
            ),
        );
        out = Some(out.map_or(b, |o| o.union(b)));
    }
    Ok(out)
}

/// Records on a shape component's placement that it moved by `m`. Its
/// vectors, if they moved in the same user step, already took their own
/// move back (see `Document::transform_entities`), so a shape moved with
/// its vectors still lies on them.
fn follow_shape(extra: &mut model::tree::ComponentExtra, m: &crate::geom::Mat3) {
    if let Some(shape) = extra.shape.as_deref_mut() {
        shape.moved(m, false);
    }
}

/// Component `id`'s field and properties after the affine `m`: a texture
/// area keeps its tiles (see [`retiled`]); any other component, or a texture
/// area mirrored or slanted, is resampled as a plain component. A shape's
/// placement is left to the caller (see [`follow_shape`]).
fn moved_by(
    document: &Document,
    id: u64,
    m: &crate::geom::Mat3,
) -> Result<(model::field::Field, model::tree::ComponentExtra), CoreError> {
    moved_of(document, document.component(id)?, m)
}

/// [`moved_by`] for a component that need not be in the document.
fn moved_of(
    document: &Document,
    component: &model::Component,
    m: &crate::geom::Mat3,
) -> Result<(model::field::Field, model::tree::ComponentExtra), CoreError> {
    let mut extra = component.extra.clone();
    if let Some((field, source)) = retiled_of(document, component, m)? {
        extra.texture = Some(Box::new(source));
        return Ok((field, extra));
    }
    let field = model::transform::transform_affine(&component.field, m)?
        .cropped()
        .ok_or_else(|| CoreError::validation("the component would be empty"))?;
    model::dynamic::follow_affine(&mut extra, m);
    extra.texture = None;
    Ok((field, extra))
}

/// A texture area tiled again after its component is moved, sized or
/// turned by `m`: its area follows and the tiles keep their size. Answers
/// none when the component is not a texture area whose seed still exists,
/// or the change is one the tiles cannot follow.
fn retiled(
    document: &Document,
    id: u64,
    m: &crate::geom::Mat3,
) -> Result<Option<(model::field::Field, model::texture_area::TextureSource)>, CoreError> {
    retiled_of(document, document.component(id)?, m)
}

/// [`retiled`] for a component that need not be in the document (one on the
/// clipboard); its seed is looked up in the document.
fn retiled_of(
    document: &Document,
    component: &model::Component,
    m: &crate::geom::Mat3,
) -> Result<Option<(model::field::Field, model::texture_area::TextureSource)>, CoreError> {
    let Some(source) = component.extra.texture.as_deref() else {
        return Ok(None);
    };
    let Ok(seed) = document.component(source.seed) else {
        return Ok(None);
    };
    let Some(moved) = source.transformed(m) else {
        return Ok(None);
    };
    let grid = model::grid_for(document.job());
    let full = model::texture_area::texture_area(
        &seed.field,
        &moved.boundary,
        &grid,
        &moved.params,
        Tolerance::new(0.01)?,
    )?;
    let field = full
        .cropped()
        .ok_or_else(|| CoreError::validation("the texture area would be empty"))?;
    Ok(Some((field, moved)))
}

/// Moves, scales and turns one component as `model.transform` asks. A
/// texture area whose tiles can follow is tiled again over its moved area
/// (see [`retiled`]); anything else is resampled as a plain component.
fn transform_one(
    document: &mut Document,
    args: &ModelTransformArgs,
) -> Result<KernelReply, CoreError> {
    let component = document.component(args.id)?;
    // As model::transform moves the cells: scale, turn, then move.
    let m = args.params.matrix(&component.field);
    if let Some((field, moved)) = retiled(document, args.id, &m)? {
        return edit(document, TRANSFORM, |doc| {
            doc.update_component(args.id, |c| {
                c.field = field;
                c.extra.texture = Some(Box::new(moved));
            })?;
            Ok(json!({}))
        });
    }
    let mut extra = component.extra.clone();
    model::dynamic::follow_transform(&mut extra, &component.field, &args.params);
    follow_shape(&mut extra, &m);
    // Tiles that cannot follow leave a plain component.
    extra.texture = None;
    let field = model::transform::transform(&component.field, &args.params)?
        .cropped()
        .ok_or_else(|| CoreError::validation("the component would be empty"))?;
    edit(document, TRANSFORM, |doc| {
        doc.update_component(args.id, |c| {
            c.field = field;
            c.extra = extra;
        })?;
        Ok(json!({}))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SculptRevertArgs {
    id: u64,
    /// "kept" (Discard: back to the last Keep) or "start" (Cancel).
    to: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_pressure_splits_a_stroke_into_even_runs() {
        let points: Vec<Point> = (0..5).map(|i| Point::new(f64::from(i), 0.0)).collect();
        assert_eq!(pressure_runs(&points, None), vec![(points.clone(), 1.0)]);
        let runs = pressure_runs(&points, Some(&[0.5, 0.52, 1.0, 1.0, 0.0]));
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0], (points[0..2].to_vec(), 0.5));
        assert_eq!(runs[1], (points[1..4].to_vec(), 1.0));
        assert_eq!(runs[2], (points[3..5].to_vec(), 0.1));
        // A mismatched list is ignored rather than guessed at.
        assert_eq!(pressure_runs(&points, Some(&[0.5])).len(), 1);
    }
}

/// Undo label of transforming components with the selection.
const TRANSFORM_MANY: &str = "Transform Components";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OtherSideArgs {
    ids: Vec<u64>,
    /// Keep the originals on this side.
    #[serde(default)]
    copy: bool,
}

/// Moves or copies components to the other side of a double sided job,
/// turned over so they lie over the same place of the material, onto
/// that side's active level. One undo step.
fn to_other_side(document: &mut Document, args: OtherSideArgs) -> Result<KernelReply, CoreError> {
    if !document.job().is_double_sided() {
        return Err(CoreError::validation(
            "components go to the other side only in a double sided job",
        ));
    }
    if args.ids.is_empty() {
        return Err(CoreError::validation("choose one or more components"));
    }
    let job = document.job();
    let (e, f) = job.flip_xy(0.0, 0.0);
    let (ax, ay) = job.flip_xy(1.0, 0.0);
    let (cx, cy) = job.flip_xy(0.0, 1.0);
    let turn = crate::geom::Mat3::new(ax - e, ay - f, cx - e, cy - f, e, f);
    let mut turned = Vec::with_capacity(args.ids.len());
    for id in &args.ids {
        let mut c = copy_of(document, document.component(*id)?, &turn, &BTreeMap::new())?;
        // Links to this side's vectors, seed and group do not travel.
        c.extra.shape = None;
        c.extra.texture = None;
        c.extra.group = 0;
        turned.push(c);
    }
    let label = if args.copy {
        "Copy to Other Side"
    } else {
        "Move to Other Side"
    };
    edit(document, label, |doc| {
        if !args.copy {
            doc.delete_components(&args.ids)?;
        }
        let here = doc.active_side();
        doc.set_active_side(here.other())?;
        let level = doc.active_level();
        let ids: Vec<u64> = turned
            .into_iter()
            .map(|mut c| {
                c.level = level;
                doc.add_component(c)
            })
            .collect();
        doc.set_active_side(here)?;
        Ok(json!({ "ids": ids }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatrixArgs {
    ids: Vec<u64>,
    /// `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f.
    matrix: [f64; 6],
    /// A live drag's previous step, replaced by this one.
    #[serde(default)]
    replace: Option<u64>,
    /// transformMatrix: accepted and not needed (vectors moved on their
    /// own already keep their shapes where they lie, see
    /// `Document::transform_entities`, so a shape moved after its vectors
    /// by the same matrix lies on them again).
    /// copyMatrix: vectors copied with the components, moved by the same
    /// matrix, and selected; a shape whose vectors are among them is made
    /// from their copies.
    #[serde(default)]
    vectors: Vec<u64>,
    /// transformMatrix: Auto Scale Z, the heights scale with the area the
    /// matrix scales by (the square root of its determinant).
    #[serde(default)]
    scale_z: bool,
}
