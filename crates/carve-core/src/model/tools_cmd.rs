//! Kernel commands of the Model menu's refinement tools:
//! `model.addZeroPlane`, `model.smoothComponent`, `model.embossComponent`,
//! `model.scaleHeight`, `model.offsetModel`, `model.addDraft`,
//! `model.fromPreview`, `model.revertPreview` and `model.volume`.
//!
//! Every change is one undo step. The forms that preview live (smoothing,
//! embossing, scaling the height) pass back the revision their last call
//! answered as `replace`: when nothing else changed the document since, that
//! step is undone before the new one is made, so however often a slider
//! moves the form leaves a single step. Offset Model previews add up
//! instead: `merge` folds the next offset into the previous step.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::cam::sim::Stock;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

use super::field::Field;
use super::refine::{ball_offset, draft_skirt, emboss_detail, smooth_amount, zero_plane};
use super::tree::ComponentExtra;
use super::{grid_for, Combine, Component};

/// Volume of a model, in cubic millimetres: the space between its surface
/// and its zero plane, every cell's height times the cell area. A part of
/// the model below the plane (a dish or a mould cavity, negative heights)
/// counts its depth, as the part above counts its height.
pub fn model_volume(composite: &Field) -> f64 {
    let area = composite.cell * composite.cell;
    composite
        .z
        .iter()
        .filter(|h| h.is_finite())
        .map(|h| f64::from(h.abs()) * area)
        .sum()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VolumeArgs {
    /// Components to measure on their own; none for the composite model.
    #[serde(default)]
    ids: Vec<u64>,
}

/// Undo label of Add Zero Plane.
pub const ZERO_PLANE: &str = "Add Zero Plane";
/// Undo label of Smooth Selected Components.
pub const SMOOTH: &str = "Smooth Component";
/// Undo label of Emboss Component.
pub const EMBOSS: &str = "Emboss Component";
/// Undo label of Scale Model Z Height.
pub const SCALE_HEIGHT: &str = "Scale Model Height";
/// Undo label of Create an Offset Model.
pub const OFFSET: &str = "Offset Model";
/// Undo label of Add Draft To Model.
pub const DRAFT: &str = "Add Draft to Model";
/// Undo label of Create Component from Toolpath Preview.
pub const FROM_PREVIEW: &str = "Component from Toolpath Preview";

/// The steps `model.revertPreview` may take back.
const PREVIEW_LABELS: [&str; 4] = [SMOOTH, EMBOSS, SCALE_HEIGHT, OFFSET];

/// Cut through material in the toolpath preview, as the simulation marks it.
const THROUGH: f32 = 1e-4;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.addZeroPlane"
            | "model.smoothComponent"
            | "model.embossComponent"
            | "model.scaleHeight"
            | "model.offsetModel"
            | "model.addDraft"
            | "model.fromPreview"
            | "model.revertPreview"
            | "model.volume"
    )
}

/// Runs one of the commands [`handles`] accepts. `stock` is the last
/// toolpath preview, which only `model.fromPreview` reads.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] for unknown ids, values out of range or a
/// model with nothing to work on.
pub fn dispatch(
    document: &mut Document,
    stock: Option<&Stock>,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "model.addZeroPlane" => add_zero_plane(document),
        "model.volume" => {
            let args: VolumeArgs = parse(name, args)?;
            if args.ids.is_empty() {
                let composite = document.composite();
                return KernelReply::ok(&json!({
                    "volume": model_volume(&composite),
                    "empty": !document.components().iter().any(|c| c.visible),
                    "selected": 0,
                }));
            }
            // Selected components are measured baked together in model
            // order, shown or not.
            let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
            let mut chosen = Vec::with_capacity(args.ids.len());
            for id in &args.ids {
                chosen.push(Component {
                    visible: true,
                    ..document.component(*id)?.clone()
                });
            }
            chosen.sort_by_key(|c| order.iter().position(|id| *id == c.id));
            let field = super::bake(&chosen, &grid_for(document.job()));
            KernelReply::ok(&json!({
                "volume": model_volume(&field),
                "empty": false,
                "selected": chosen.len(),
            }))
        }
        "model.smoothComponent" => {
            let args: SmoothArgs = parse(name, args)?;
            replacing(document, args.replace, SMOOTH, |doc| {
                smooth_component(doc, &args)
            })
        }
        "model.embossComponent" => {
            let args: EmbossArgs = parse(name, args)?;
            replacing(document, args.replace, EMBOSS, |doc| {
                emboss_component(doc, &args)
            })
        }
        "model.scaleHeight" => {
            let args: ScaleArgs = parse(name, args)?;
            replacing(document, args.replace, SCALE_HEIGHT, |doc| {
                scale_height(doc, &args)
            })
        }
        "model.offsetModel" => offset_model(document, parse(name, args)?),
        "model.addDraft" => add_draft(document, parse(name, args)?),
        "model.fromPreview" => from_preview(document, stock, parse(name, args)?),
        "model.revertPreview" => {
            let args: RevertArgs = parse(name, args)?;
            // A preview taken back is gone for good: Redo must not replay it.
            let last = document.journal().undo_label().unwrap_or("");
            let reverted = document.revision() == args.revision
                && (PREVIEW_LABELS.contains(&last)
                    || super::create_cmd::CREATE_LABELS.contains(&last))
                && document.discard_last().is_some();
            KernelReply::ok(&json!({ "reverted": reverted, "revision": document.revision() }))
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

/// Runs `action` as one undo step, answering its result with the new
/// revision, or putting everything back when it fails.
fn edit<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(mut result) => {
            let revision = document.commit();
            if let Some(object) = result.as_object_mut() {
                object.insert("revision".to_string(), json!(revision));
            } else {
                let mut object = Map::new();
                object.insert("result".to_string(), result);
                object.insert("revision".to_string(), json!(revision));
                result = Value::Object(object);
            }
            KernelReply::ok(&result)
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

/// Undoes the newest step when the document is still at `revision` and
/// that step is one of `labels`. Answers whether it did.
pub(crate) fn undo_if_last(
    document: &mut Document,
    revision: Option<u64>,
    labels: &[&str],
) -> bool {
    let Some(revision) = revision else {
        return false;
    };
    let last = document.journal().undo_label().unwrap_or("");
    if document.revision() != revision || !labels.contains(&last) {
        return false;
    }
    document.undo().is_some()
}

/// Takes back the step made at `replace` (when it is still the newest and
/// is a `label` step), then runs `action`. A failing action puts the old
/// step back.
fn replacing<F>(
    document: &mut Document,
    replace: Option<u64>,
    label: &str,
    action: F,
) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<KernelReply, CoreError>,
{
    let undone = undo_if_last(document, replace, &[label]);
    match action(document) {
        Ok(reply) => Ok(reply),
        Err(error) => {
            if undone {
                document.redo();
            }
            Err(error)
        }
    }
}

/// The level a whole-model component goes on: the lowest visible level
/// that adds up, since it stands on the zero plane.
fn base_level(document: &Document) -> u64 {
    document
        .levels()
        .iter()
        .find(|l| l.visible && l.combine == Combine::Add)
        .map_or(document.active_level(), |l| l.id)
}

fn new_component(name: String, combine: Combine, level: u64, field: Field) -> Component {
    Component {
        id: 0,
        name,
        visible: true,
        combine,
        height_scale: 1.0,
        level,
        extra: ComponentExtra::default(),
        field,
    }
}

// --------------------------------------------------------------- zero plane

fn add_zero_plane(document: &mut Document) -> Result<KernelReply, CoreError> {
    let field = zero_plane(&grid_for(document.job()));
    let level = document.active_level();
    edit(document, ZERO_PLANE, |doc| {
        let id = doc.add_component(new_component(
            "Zero Plane".to_string(),
            Combine::Add,
            level,
            field,
        ));
        Ok(json!({ "id": id }))
    })
}

// ------------------------------------------------------------------ smooth

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmoothArgs {
    id: u64,
    /// Smoothing, 0 to 100 %.
    amount: f64,
    #[serde(default = "yes")]
    preserve_transparency: bool,
    #[serde(default)]
    replace: Option<u64>,
}

fn yes() -> bool {
    true
}

fn smooth_component(document: &mut Document, args: &SmoothArgs) -> Result<KernelReply, CoreError> {
    let current = document.component(args.id)?.field.clone();
    let smoothed = smooth_amount(&current, args.amount, args.preserve_transparency)?;
    let id = args.id;
    edit(document, SMOOTH, |doc| {
        doc.update_component(id, |c| c.field = smoothed)?;
        Ok(json!({ "id": id }))
    })
}

// ------------------------------------------------------------------ emboss

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbossArgs {
    id: u64,
    /// Height the component keeps, 0 to 100 % of what it has.
    scale: f64,
    /// Smoothing of the kept detail, 0 to 100 %.
    #[serde(default)]
    detail_smoothing: f64,
    #[serde(default)]
    replace: Option<u64>,
}

fn emboss_component(document: &mut Document, args: &EmbossArgs) -> Result<KernelReply, CoreError> {
    let current = document.component(args.id)?.field.clone();
    let embossed = emboss_detail(&current, args.scale / 100.0, args.detail_smoothing)?;
    let id = args.id;
    edit(document, EMBOSS, |doc| {
        doc.update_component(id, |c| c.field = embossed)?;
        Ok(json!({ "id": id }))
    })
}

// ------------------------------------------------------------ scale height

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScaleArgs {
    /// Percent of the model's height, above 0.
    #[serde(default)]
    percent: Option<f64>,
    /// Exact height (mm) for the model's highest point, above 0.
    #[serde(default)]
    height: Option<f64>,
    /// A double sided job scales the other side's components too.
    #[serde(default)]
    both_sides: bool,
    #[serde(default)]
    replace: Option<u64>,
}

/// Scales the composite model's heights: every component's height scale
/// and base height multiply by the factor, so the components keep their
/// own shapes and the model they bake to is the old one times the factor.
/// Level base heights scale with the model too.
fn scale_level_bases(doc: &mut Document, factor: f64) -> Result<(), CoreError> {
    let raised: Vec<u64> = doc
        .levels()
        .iter()
        .filter(|l| l.base_height != 0.0)
        .map(|l| l.id)
        .collect();
    for id in raised {
        doc.update_level(id, |l| l.base_height *= factor)?;
    }
    Ok(())
}

fn scale_height(document: &mut Document, args: &ScaleArgs) -> Result<KernelReply, CoreError> {
    let peak = f64::from(document.composite().max().unwrap_or(0.0));
    let factor = match (args.height, args.percent) {
        (Some(height), _) => {
            if !(height.is_finite() && height > 0.0) {
                return Err(CoreError::validation("the model height must be above 0"));
            }
            if peak <= 0.0 {
                return Err(CoreError::validation(
                    "the model has no height above the zero plane to scale",
                ));
            }
            height / peak
        }
        (None, Some(percent)) => {
            if !(percent.is_finite() && percent > 0.0) {
                return Err(CoreError::validation("the height scale must be above 0 %"));
            }
            percent / 100.0
        }
        (None, None) => {
            return Err(CoreError::validation(
                "give a height scale or an exact height",
            ))
        }
    };
    let ids: Vec<u64> = document.components().iter().map(|c| c.id).collect();
    if ids.is_empty() {
        return Err(CoreError::validation("there is no model to scale"));
    }
    edit(document, SCALE_HEIGHT, |doc| {
        for id in &ids {
            doc.update_component(*id, |c| {
                c.height_scale *= factor;
                c.extra.base_height *= factor;
            })?;
        }
        scale_level_bases(doc, factor)?;
        if args.both_sides && doc.job().is_double_sided() {
            // The other side's components change while it is shown, inside
            // this undo step, so undo brings both sides back together.
            let here = doc.active_side();
            doc.set_active_side(here.other())?;
            let others: Vec<u64> = doc.components().iter().map(|c| c.id).collect();
            for id in &others {
                doc.update_component(*id, |c| {
                    c.height_scale *= factor;
                    c.extra.base_height *= factor;
                })?;
            }
            scale_level_bases(doc, factor)?;
            doc.set_active_side(here)?;
        }
        Ok(json!({ "factor": factor, "maxHeight": peak * factor }))
    })
}

// ------------------------------------------------------------------ offset

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OffsetArgs {
    /// Millimetres; positive grows the model, negative shrinks it.
    distance: f64,
    #[serde(default)]
    clip_to_zero: bool,
    /// The revision and component of the previous preview to add onto.
    #[serde(default)]
    merge: Option<OffsetMerge>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OffsetMerge {
    revision: u64,
    id: u64,
}

/// Offsets the visible model into a new component; the components it came
/// from stay in the tree, hidden, so the offset is what shows. A `merge`
/// that still matches the newest step offsets that step's component again
/// and folds the change into the same step.
fn offset_model(document: &mut Document, args: OffsetArgs) -> Result<KernelReply, CoreError> {
    if let Some(merge) = &args.merge {
        let last = document.journal().undo_label().unwrap_or("");
        if document.revision() == merge.revision
            && last == OFFSET
            && document.component(merge.id).is_ok()
        {
            let current = document.component(merge.id)?.field.clone();
            let field = ball_offset(&current, args.distance, args.clip_to_zero)?;
            let depth = document.journal().undo_depth().saturating_sub(1);
            let id = merge.id;
            let reply = edit(document, OFFSET, |doc| {
                doc.update_component(id, |c| c.field = field)?;
                Ok(json!({ "id": id }))
            })?;
            document.squash_steps_since(depth, OFFSET);
            return Ok(reply);
        }
    }
    let sources: Vec<u64> = document
        .components()
        .iter()
        .filter(|c| c.visible)
        .map(|c| c.id)
        .collect();
    if sources.is_empty() {
        return Err(CoreError::validation("there is no visible model to offset"));
    }
    let composite = document.composite();
    let field = ball_offset(&composite, args.distance, args.clip_to_zero)?;
    let level = base_level(document);
    edit(document, OFFSET, |doc| {
        for id in &sources {
            doc.update_component(*id, |c| c.visible = false)?;
        }
        let id = doc.add_component(new_component(
            OFFSET.to_string(),
            Combine::Add,
            level,
            field,
        ));
        Ok(json!({ "id": id, "hidden": sources }))
    })
}

// ------------------------------------------------------------------- draft

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftArgs {
    angle_deg: f64,
}

/// The draft component's name, with the angle to at most two decimals.
fn draft_name(angle: f64) -> String {
    let text = format!("{angle:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("Draft {text}\u{b0}")
}

/// Bakes the visible model into a new component with a draft skirt round
/// every wall. It merges high over the model, so the originals stay
/// visible under it.
fn add_draft(document: &mut Document, args: DraftArgs) -> Result<KernelReply, CoreError> {
    if !document.components().iter().any(|c| c.visible) {
        return Err(CoreError::validation(
            "there is no visible model to add draft to",
        ));
    }
    let field = draft_skirt(&document.composite(), args.angle_deg)?;
    let level = base_level(document);
    let name = draft_name(args.angle_deg);
    edit(document, DRAFT, |doc| {
        let id = doc.add_component(new_component(name, Combine::MergeHigh, level, field));
        Ok(json!({ "id": id }))
    })
}

// ------------------------------------------------------- toolpath preview

/// Which part of the toolpath preview becomes the component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewMode {
    /// All of it, the uncut waste included.
    #[default]
    Whole,
    /// Only where the model has components.
    OverModel,
    /// Only the material the tools left above the model, over the model.
    LeftOver,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FromPreviewArgs {
    #[serde(default)]
    mode: PreviewMode,
}

/// Heights of the preview above the model's zero plane, placed in the
/// material the way 3D toolpaths place the model, with empty cells where
/// the preview cut through.
pub fn preview_heights(stock: &Stock, model: &Field, mode: PreviewMode, covered: &[bool]) -> Field {
    let top = stock.top;
    let peak = f64::from(model.max().unwrap_or(0.0)).max(0.0);
    let zero = (top - peak).max(stock.bottom);
    let origin = crate::geom::Point::new(stock.origin_x, stock.origin_y);
    let mut out = Field::empty(stock.width, stock.height, stock.cell, origin);
    for j in 0..stock.height {
        for i in 0..stock.width {
            let index = j * stock.width + i;
            let z = stock.z[index];
            if z.is_nan() || stock.floor[index] >= z - THROUGH {
                continue;
            }
            let over = covered.get(index).copied().unwrap_or(false);
            let height = (f64::from(z) - zero) as f32;
            out.z[index] = match mode {
                PreviewMode::Whole => height,
                PreviewMode::OverModel if over => height,
                PreviewMode::LeftOver if over => {
                    let below = model.at(out.centre(i, j));
                    let below = if below.is_nan() { 0.0 } else { below };
                    (height - below).max(0.0)
                }
                _ => f32::NAN,
            };
        }
    }
    out
}

fn from_preview(
    document: &mut Document,
    stock: Option<&Stock>,
    args: FromPreviewArgs,
) -> Result<KernelReply, CoreError> {
    let stock = stock.ok_or_else(|| {
        CoreError::validation("run a toolpath preview before making a component from it")
    })?;
    let model = document.composite();
    let visible: Vec<&Component> = document
        .components()
        .into_iter()
        .filter(|c| c.visible)
        .collect();
    let mut covered = vec![false; stock.width * stock.height];
    if args.mode != PreviewMode::Whole {
        for j in 0..stock.height {
            for i in 0..stock.width {
                let p = crate::geom::Point::new(
                    stock.origin_x + i as f64 * stock.cell,
                    stock.origin_y + j as f64 * stock.cell,
                );
                covered[j * stock.width + i] = visible.iter().any(|c| !c.field.at(p).is_nan());
            }
        }
    }
    let field = preview_heights(stock, &model, args.mode, &covered)
        .cropped()
        .ok_or_else(|| {
            CoreError::validation("the toolpath preview leaves nothing to keep there")
        })?;
    let level = document.active_level();
    edit(document, FROM_PREVIEW, |doc| {
        let id = doc.add_component(new_component(
            "Toolpath Preview".to_string(),
            Combine::Add,
            level,
            field,
        ));
        Ok(json!({ "id": id }))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevertArgs {
    revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::sim::Quality;
    use crate::geom::Point;

    fn result(reply: KernelReply) -> Value {
        match reply {
            KernelReply::Json(text) => {
                let v: Value = serde_json::from_str(&text).expect("json");
                v["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected json"),
        }
    }

    fn run(doc: &mut Document, name: &str, args: Value) -> Value {
        result(
            dispatch(doc, None, name, &args.to_string()).unwrap_or_else(|e| panic!("{name}: {e}")),
        )
    }

    /// A flat block `height` mm high over a job rectangle, on the model grid.
    fn add_block(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64, height: f32) -> u64 {
        let grid = grid_for(doc.job());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for j in 0..grid.height {
            for i in 0..grid.width {
                let c = field.centre(i, j);
                if c.x >= x0 && c.x <= x1 && c.y >= y0 && c.y <= y1 {
                    field.z[j * grid.width + i] = height;
                }
            }
        }
        doc.begin("Add");
        let id = doc.add_component(new_component(
            "Block".to_string(),
            Combine::Add,
            0,
            field.cropped().expect("cells"),
        ));
        doc.commit();
        id
    }

    fn peak(doc: &Document) -> f32 {
        doc.composite().max().unwrap_or(0.0)
    }

    #[test]
    fn handles_only_its_own_commands() {
        assert!(handles("model.addDraft"));
        assert!(!handles("model.list"));
        let mut doc = Document::new();
        assert!(dispatch(&mut doc, None, "model.list", "").is_err());
    }

    #[test]
    fn the_kernel_routes_the_commands_here() {
        let mut kernel = crate::Kernel::with_cam(crate::cam::commands::CamState::default());
        let error = kernel
            .apply("model.fromPreview", "{}")
            .expect_err("no preview yet");
        assert!(error.to_string().contains("toolpath preview"), "{error}");
        kernel.apply("model.addZeroPlane", "{}").expect("adds");
    }

    #[test]
    fn a_zero_plane_covers_the_job_in_one_step() {
        let mut doc = Document::new();
        let reply = run(&mut doc, "model.addZeroPlane", json!({}));
        let id = reply["id"].as_u64().expect("id");
        let plane = doc.component(id).expect("plane");
        assert_eq!(plane.name, "Zero Plane");
        let grid = grid_for(doc.job());
        assert_eq!(
            (plane.field.width, plane.field.height),
            (grid.width, grid.height)
        );
        assert_eq!(plane.field.max(), Some(0.0));
        assert_eq!(doc.journal().undo_label(), Some(ZERO_PLANE));
        doc.undo();
        assert!(doc.components().is_empty());
    }

    #[test]
    fn live_smoothing_replaces_its_own_step() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let original = doc.component(block).expect("block").field.clone();
        let first = run(
            &mut doc,
            "model.smoothComponent",
            json!({ "id": block, "amount": 50, "preserveTransparency": false }),
        );
        let depth = doc.journal().undo_depth();
        let again = run(
            &mut doc,
            "model.smoothComponent",
            json!({ "id": block, "amount": 100, "preserveTransparency": false,
                    "replace": first["revision"] }),
        );
        assert_eq!(doc.journal().undo_depth(), depth, "replaced, not stacked");
        // A stale token stacks a new step instead.
        run(
            &mut doc,
            "model.smoothComponent",
            json!({ "id": block, "amount": 10, "replace": first["revision"] }),
        );
        assert_eq!(doc.journal().undo_depth(), depth + 1);
        doc.undo();
        doc.undo();
        assert_eq!(doc.component(block).expect("block").field, original);
        assert!(again["revision"].as_u64() > first["revision"].as_u64());
    }

    #[test]
    fn a_failing_replacement_keeps_the_old_step() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let first = run(
            &mut doc,
            "model.embossComponent",
            json!({ "id": block, "scale": 50 }),
        );
        let bad = json!({ "id": block, "scale": 0, "replace": first["revision"] });
        assert!(dispatch(&mut doc, None, "model.embossComponent", &bad.to_string()).is_err());
        assert_eq!(doc.journal().undo_label(), Some(EMBOSS));
        assert!((peak(&doc) - 2.5).abs() < 0.05, "{}", peak(&doc));
    }

    #[test]
    fn scale_both_sides_scales_the_other_side_in_the_same_step() {
        use crate::doc::{JobKind, JobSetup, Side};
        let mut doc = Document::new();
        doc.begin("Setup");
        doc.set_job(JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        })
        .expect("job");
        doc.commit();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 4.0);
        doc.begin("Bottom");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 6.0);
        doc.begin("Top");
        doc.set_active_side(Side::Top).expect("top");
        doc.commit();
        run(
            &mut doc,
            "model.scaleHeight",
            json!({ "percent": 50, "bothSides": true }),
        );
        assert!((peak(&doc) - 2.0).abs() < 1e-5);
        assert_eq!(doc.active_side(), Side::Top);
        let other = doc.other_side_composite().expect("bottom model");
        assert!((other.max().unwrap_or(0.0) - 3.0).abs() < 1e-5);
        doc.undo();
        let other = doc.other_side_composite().expect("bottom model");
        assert!((other.max().unwrap_or(0.0) - 6.0).abs() < 1e-5);
        assert!((peak(&doc) - 4.0).abs() < 1e-5);
        assert_eq!(doc.active_side(), Side::Top);
    }

    #[test]
    fn the_volume_is_the_model_over_its_zero_plane() {
        let mut doc = Document::new();
        let empty = run(&mut doc, "model.volume", json!({}));
        assert_eq!(empty["empty"], true);
        assert_eq!(empty["volume"], 0.0);
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 4.0);
        let cell = grid_for(doc.job()).cell;
        let cells = doc.composite().z.iter().filter(|h| **h > 0.0).count() as f64;
        let reply = run(&mut doc, "model.volume", json!({}));
        let volume = reply["volume"].as_f64().expect("volume");
        assert!(
            (volume - cells * cell * cell * 4.0).abs() < 1e-6,
            "{volume}"
        );
        // About the 100 by 100 by 4 block, give or take the edge cells.
        assert!((volume - 40_000.0).abs() < 40_000.0 * 0.05, "{volume}");
        assert_eq!(reply["empty"], false);
    }

    #[test]
    fn the_volume_counts_below_the_plane_and_can_measure_a_selection() {
        let mut field = Field::empty(4, 1, 2.0, crate::geom::Point::new(0.0, 0.0));
        field.z = vec![3.0, -2.0, f32::NAN, 0.0];
        assert!((model_volume(&field) - 5.0 * 4.0).abs() < 1e-9);
        let mut doc = Document::new();
        let a = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 4.0);
        add_block(&mut doc, 220.0, 100.0, 270.0, 150.0, 2.0);
        let whole = run(&mut doc, "model.volume", json!({}));
        let one = run(&mut doc, "model.volume", json!({ "ids": [a] }));
        assert_eq!(one["selected"], 1);
        assert_eq!(whole["selected"], 0);
        let (whole, one) = (
            whole["volume"].as_f64().expect("whole"),
            one["volume"].as_f64().expect("one"),
        );
        assert!((one - 40_000.0).abs() < 40_000.0 * 0.05, "{one}");
        assert!(
            (whole - one - 5_000.0).abs() < 5_000.0 * 0.1,
            "{whole} {one}"
        );
        let missing = json!({ "ids": [a + 1000] }).to_string();
        assert!(dispatch(&mut doc, None, "model.volume", &missing).is_err());
    }

    #[test]
    fn both_sides_stand_on_one_shared_modelling_plane() {
        use crate::doc::{JobKind, JobSetup, Side, ZZero};
        let mut doc = Document::new();
        doc.begin("Setup");
        let mut job = JobSetup {
            kind: JobKind::DoubleSided,
            thickness: 20.0,
            z_zero: ZZero::Top,
            ..JobSetup::default()
        };
        job.machine.model_gap = 2.0;
        job.double_sided.bottom_z_zero = ZZero::Top;
        doc.set_job(job).expect("job");
        doc.commit();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 4.0);
        // Top side: the model peak 2 under the top face, the plane 6 down.
        assert!((doc.model_zero(&doc.composite()) + 6.0).abs() < 1e-5);
        doc.begin("Bottom");
        doc.set_active_side(Side::Bottom).expect("bottom");
        doc.commit();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 7.0);
        // Seen from the bottom face the same plane is 20 - 6 = 14 down, so
        // the bottom model reaches 7 above it and the sides never overlap.
        assert!((doc.model_zero(&doc.composite()) + 14.0).abs() < 1e-5);
        assert!((doc.other_side_peak() - 4.0).abs() < 1e-5);
        let surface = crate::cam::surface::Surface::of(&doc);
        assert!((surface.zero() + 14.0).abs() < 1e-5);
    }

    #[test]
    fn scaling_the_height_scales_the_composite() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 4.0);
        add_block(&mut doc, 150.0, 150.0, 180.0, 180.0, 2.0);
        let before = doc.composite();
        let first = run(&mut doc, "model.scaleHeight", json!({ "percent": 50 }));
        assert!((peak(&doc) - 3.0).abs() < 1e-5);
        let exact = run(
            &mut doc,
            "model.scaleHeight",
            json!({ "height": 12, "replace": first["revision"] }),
        );
        assert!((exact["factor"].as_f64().unwrap_or(0.0) - 2.0).abs() < 1e-9);
        assert!((peak(&doc) - 12.0).abs() < 1e-4);
        let reverted = run(
            &mut doc,
            "model.revertPreview",
            json!({ "revision": exact["revision"] }),
        );
        assert_eq!(reverted["reverted"], json!(true));
        assert_eq!(doc.composite(), before);
        assert!(
            !doc.journal().can_redo(),
            "a taken back preview cannot be redone"
        );
        assert!(dispatch(&mut doc, None, "model.scaleHeight", "{}").is_err());
    }

    #[test]
    fn offset_previews_add_up_in_one_step() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let depth = doc.journal().undo_depth();
        let first = run(
            &mut doc,
            "model.offsetModel",
            json!({ "distance": 1.0, "clipToZero": false }),
        );
        let id = first["id"].as_u64().expect("id");
        assert!(!doc.component(block).expect("source").visible);
        assert!((peak(&doc) - 6.0).abs() < 1e-4);
        run(
            &mut doc,
            "model.offsetModel",
            json!({ "distance": 1.0, "merge": { "revision": first["revision"], "id": id } }),
        );
        assert!(
            (peak(&doc) - 7.0).abs() < 1e-4,
            "the second preview adds on"
        );
        assert_eq!(doc.journal().undo_depth(), depth + 1);
        doc.undo();
        assert!(doc.component(block).expect("source").visible);
        assert!((peak(&doc) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn a_negative_offset_can_clip_to_the_zero_plane() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        run(
            &mut doc,
            "model.offsetModel",
            json!({ "distance": -1.0, "clipToZero": true }),
        );
        let composite = doc.composite();
        assert!(composite.min().expect("cells") >= 0.0);
        assert!((composite.at(Point::new(150.0, 150.0)) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn draft_adds_a_skirt_component_over_the_model() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 10.0);
        let reply = run(&mut doc, "model.addDraft", json!({ "angleDeg": 45 }));
        let id = reply["id"].as_u64().expect("id");
        let draft = doc.component(id).expect("draft");
        assert_eq!(draft.name, "Draft 45\u{b0}");
        assert_eq!(draft.combine, Combine::MergeHigh);
        assert!(doc.component(block).expect("block").visible);
        let composite = doc.composite();
        assert!((composite.at(Point::new(150.0, 150.0)) - 10.0).abs() < 1e-4);
        let skirt = composite.at(Point::new(95.0, 150.0));
        assert!(skirt > 3.0 && skirt < 7.0, "{skirt}");
        assert_eq!(draft_name(12.5), "Draft 12.5\u{b0}");
        let empty = &mut Document::new();
        assert!(dispatch(empty, None, "model.addDraft", r#"{"angleDeg":10}"#).is_err());
    }

    #[test]
    fn preview_components_follow_the_mode() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        // A pocket 2 mm deep cut everywhere but over the block.
        let model = doc.composite();
        let mut covered = vec![false; stock.width * stock.height];
        for j in 0..stock.height {
            for i in 0..stock.width {
                let index = j * stock.width + i;
                let p = Point::new(
                    stock.origin_x + i as f64 * stock.cell,
                    stock.origin_y + j as f64 * stock.cell,
                );
                let over = !model.at(p).is_nan() && model.at(p) > 0.0;
                covered[index] = over;
                if !over {
                    stock.z[index] = (stock.top - 2.0) as f32;
                }
            }
        }
        let whole = preview_heights(&stock, &model, PreviewMode::Whole, &covered);
        let inside = Point::new(150.0, 150.0);
        let outside = Point::new(50.0, 50.0);
        assert!(
            (whole.at(inside) - 5.0).abs() < 1e-4,
            "top of the model at 5"
        );
        assert!((whole.at(outside) - 3.0).abs() < 1e-4);
        let over = preview_heights(&stock, &model, PreviewMode::OverModel, &covered);
        assert!(over.at(outside).is_nan());
        let left = preview_heights(&stock, &model, PreviewMode::LeftOver, &covered);
        assert!(
            left.at(inside).abs() < 1e-4,
            "nothing left above the flat top"
        );
        let reply = dispatch(
            &mut doc,
            Some(&stock),
            "model.fromPreview",
            r#"{"mode":"overModel"}"#,
        )
        .expect("makes it");
        let id = result(reply)["id"].as_u64().expect("id");
        assert_eq!(doc.component(id).expect("preview").name, "Toolpath Preview");
    }
}
