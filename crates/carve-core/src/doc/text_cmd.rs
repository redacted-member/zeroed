//! Text and dimension commands: `text.*` and `dims.create`. Text is laid
//! out into glyph outlines with the kernel's fonts when it is created.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use crate::doc::entity::{CurvePosition, OnCurve};
use crate::doc::{
    DimensionEntity, DimensionSpec, Document, Entity, EntityKind, TextEntity, TextSpecJson,
};
use crate::error::CoreError;
use crate::geom::{
    self, transform_path, DimensionStyle, FontLibrary, Mat3, Path, Point, TextSide, TextSpec,
    Tolerance,
};
use crate::kernel::{edit, parse_args, KernelReply};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "text.create"
            | "text.update"
            | "text.onCurve"
            | "text.removeFromCurve"
            | "text.arc"
            | "text.fonts"
            | "text.preview"
            | "dims.create"
            | "dims.update"
            | "dims.style"
            | "text.toCurves"
            | "text.glyphs"
    ) || super::plate_cmd::handles(name)
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(
    document: &mut Document,
    fonts: &FontLibrary,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let style = &document.dimension_style().clone();
    match name {
        "text.create" => {
            let args: TextCreateArgs = parse_args(name, args)?;
            let origin = Point::new(args.x, args.y);
            let paths = plain_text_paths(fonts, &args)?;
            let layer = layer_or_active(document, args.layer)?;
            let sheet = document.active_sheet();
            edit(document, "Create Text", |doc| {
                let id = doc.insert_entity(Entity {
                    rotation: 0.0,
                    id: 0,
                    layer,
                    sheet,
                    revision: 0,
                    name: None,
                    parent: None,
                    kind: EntityKind::Text(TextEntity {
                        spec: args.spec,
                        origin,
                        paths,
                    }),
                });
                Ok(json!({ "id": id }))
            })
        }
        "text.update" => {
            let args: TextUpdateArgs = parse_args(name, args)?;
            let existing = document.entity(args.id)?.clone();
            let EntityKind::Text(current) = &existing.kind else {
                return Err(CoreError::validation(format!(
                    "entity {} is not text",
                    args.id
                )));
            };
            let mut spec = args.spec.unwrap_or_else(|| current.spec.clone());
            // Text whose curve is gone is edited as plain text where it is.
            let curve_gone = spec.on_curve.as_ref().is_some_and(|on| {
                document
                    .entity(on.path_id)
                    .map_or(true, |curve| curve.path().is_none())
            });
            if curve_gone {
                spec.on_curve = None;
            }
            let origin = match (args.x, args.y) {
                (Some(x), Some(y)) => Point::new(x, y),
                _ => current.origin,
            };
            let mut paths = match &spec.on_curve {
                Some(on) => curve_text(fonts, document, &spec, on)?,
                None => straight_text(fonts, &spec, origin)?,
            };
            if existing.rotation != 0.0 && spec.auto_layout.is_none() && spec.on_curve.is_none() {
                // Turned text stays turned when it is laid out again.
                let m = Mat3::rotate_about(existing.rotation.to_radians(), origin);
                paths = paths.iter().map(|p| transform_path(p, &m)).collect();
            }
            let mut entity = existing.clone();
            entity.kind = EntityKind::Text(TextEntity {
                spec,
                origin,
                paths,
            });
            if joins_last_text_step(document, args.merge, args.id) && document.reopen_newest_step()
            {
                document.replace_entity(args.id, entity)?;
                let revision = document.commit();
                return KernelReply::ok(&json!({ "id": args.id, "revision": revision }));
            }
            edit(document, "Edit Text", |doc| {
                doc.replace_entity(args.id, entity)?;
                Ok(json!({ "id": args.id }))
            })
        }
        "text.onCurve" => {
            let args: TextOnCurveArgs = parse_args(name, args)?;
            let (id, entity) = on_curve_entity(fonts, document, &args)?;
            edit(document, "Text On A Curve", |doc| {
                doc.replace_entity(id, entity)?;
                doc.set_selection(&[id]);
                Ok(json!({ "id": id }))
            })
        }
        "text.removeFromCurve" => {
            let args: IdArgs = parse_args(name, args)?;
            let mut entity = document.entity(args.id)?.clone();
            let EntityKind::Text(text) = &mut entity.kind else {
                return Err(CoreError::validation("select text that is on a curve"));
            };
            if text.spec.on_curve.is_none() {
                return Err(CoreError::validation("that text is not on a curve"));
            }
            text.spec.on_curve = None;
            text.paths = straight_text(fonts, &text.spec, text.origin)?;
            edit(document, "Remove Text From The Curve", |doc| {
                doc.replace_entity(args.id, entity)?;
                Ok(json!({ "id": args.id }))
            })
        }
        "text.arc" => {
            let args: TextArcArgs = parse_args(name, args)?;
            let centre = Point::new(args.cx, args.cy);
            let paths = arc_text_paths(fonts, &args)?;
            let layer = layer_or_active(document, args.layer)?;
            let sheet = document.active_sheet();
            edit(document, "Text On Arc", |doc| {
                let id = doc.insert_entity(Entity {
                    rotation: 0.0,
                    id: 0,
                    layer,
                    sheet,
                    revision: 0,
                    name: None,
                    parent: None,
                    kind: EntityKind::Text(TextEntity {
                        spec: args.spec,
                        origin: centre,
                        paths,
                    }),
                });
                Ok(json!({ "id": id }))
            })
        }
        "text.fonts" => KernelReply::ok(&json!({
            "families": fonts.families(),
            "singleLine": fonts.single_line_families(),
        })),
        "text.preview" => {
            let args: TextPreviewArgs = parse_args(name, args)?;
            let paths = match &args {
                TextPreviewArgs::Plain(plain) => plain_text_paths(fonts, plain)?,
                TextPreviewArgs::Arc(arc) => arc_text_paths(fonts, arc)?,
                TextPreviewArgs::OnCurve(curve) => {
                    match on_curve_entity(fonts, document, curve)?.1.kind {
                        EntityKind::Text(text) => text.paths,
                        _ => Vec::new(),
                    }
                }
            };
            KernelReply::ok(&json!({ "paths": paths }))
        }
        "dims.create" => {
            let args: DimsCreateArgs = parse_args(name, args)?;
            let styled = with_text(style, args.custom_text.clone());
            let (paths, text_box) = args.spec.build_parts(fonts, &styled, args.text_at)?;
            let sheet = document.active_sheet();
            let on_layer = style.use_layer.then(|| style.layer_name.clone());
            edit(document, "Create Dimension", |doc| {
                let layer = match &on_layer {
                    Some(name) => dimensions_layer(doc, name),
                    None => doc.active_layer(),
                };
                let id = doc.insert_entity(Entity {
                    rotation: 0.0,
                    id: 0,
                    layer,
                    sheet,
                    revision: 0,
                    name: None,
                    parent: None,
                    kind: EntityKind::Dimension(DimensionEntity {
                        spec: args.spec.clone(),
                        paths,
                        custom_text: args.custom_text.clone(),
                        text_at: args.text_at,
                        text_box,
                    }),
                });
                Ok(json!({ "id": id }))
            })
        }
        "dims.update" => {
            let args: DimsUpdateArgs = parse_args(name, args)?;
            let mut entity = document.entity(args.id)?.clone();
            let EntityKind::Dimension(dim) = &mut entity.kind else {
                return Err(CoreError::validation("select a dimension to edit"));
            };
            if let Some(spec) = args.spec {
                // A moved dimension carries its text position with it; none
                // puts the text back in the middle.
                dim.spec = spec;
                dim.text_at = args.text_at;
            }
            if let Some(custom) = args.custom_text {
                dim.custom_text = Some(custom).filter(|t| !t.trim().is_empty());
            }
            let (paths, text_box) = dim.spec.build_parts(
                fonts,
                &with_text(style, dim.custom_text.clone()),
                dim.text_at,
            )?;
            dim.paths = paths;
            dim.text_box = text_box;
            edit(document, "Edit Dimension", |doc| {
                doc.replace_entity(args.id, entity)?;
                Ok(json!({ "id": args.id }))
            })
        }
        "dims.style" => {
            let args: DimsStyleArgs = parse_args(name, args)?;
            let mut next = style.clone();
            if let Some(family) = args.font_family {
                next.text.font_family = family;
            }
            if let Some(height) = args.text_height {
                if !height.is_finite() || height <= 0.0 {
                    return Err(CoreError::validation("the text height must be above zero"));
                }
                next.text.size_mm = height;
            }
            if let Some(places) = args.precision {
                next.precision = places.min(6);
            }
            if let Some(places) = args.angle_precision {
                next.angle_precision = places.min(6);
            }
            if let Some(units) = args.units {
                next.units = units;
            }
            if let Some(show) = args.show_units {
                next.show_units = show;
            }
            if let Some(gap) = args.offset {
                if !gap.is_finite() || gap < 0.0 {
                    return Err(CoreError::validation("the offset cannot be negative"));
                }
                next.extension_gap = gap;
            }
            if let Some(size) = args.arrow_size {
                if !size.is_finite() || size <= 0.0 {
                    return Err(CoreError::validation("the arrow size must be above zero"));
                }
                next.arrow_length = size;
                next.arrow_width = size * 0.55;
            }
            if let Some(on) = args.use_layer {
                next.use_layer = on;
            }
            if let Some(name) = args.layer_name {
                next.layer_name = name;
            }
            let reply = style_json(&next);
            document.set_dimension_style(next);
            KernelReply::ok(&reply)
        }
        "text.glyphs" => {
            let args: GlyphsArgs = parse_args(name, args)?;
            let entity = document.entity(args.id)?;
            let EntityKind::Text(text) = &entity.kind else {
                return Err(CoreError::validation("select one piece of text"));
            };
            let spec = layout_spec(fonts, &text.spec)?;
            let (glyphs, arc) = super::text_adjust::glyph_boxes(
                fonts,
                &spec,
                &text.spec,
                text.origin,
                entity.rotation.to_radians(),
            )?;
            KernelReply::ok(&json!({
                "glyphs": glyphs,
                "height": text.spec.size_mm,
                "arc": arc,
                "rotation": entity.rotation,
            }))
        }
        "text.toCurves" => {
            let args: ToCurvesArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Convert Text to Curves", |doc| {
                let made = text_to_curves(doc, &ids)?;
                doc.set_selection(&made);
                Ok(json!({ "ids": made }))
            })
        }
        other if super::plate_cmd::handles(other) => {
            super::plate_cmd::dispatch(document, fonts, other, args)
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The layout spec for a text spec from the wire. The wire's `sizeMm` is
/// the height of the capital letters, as the Text Height field means it;
/// layout wants the em size, which depends on the font's proportions.
fn layout_spec(fonts: &FontLibrary, json: &TextSpecJson) -> Result<TextSpec, CoreError> {
    let mut spec: TextSpec = json.into();
    let font = fonts.load(&spec.font_family, spec.bold, spec.italic)?;
    let cap_per_em = geom::cap_height(&font, 1.0)?;
    if cap_per_em > 0.0 {
        spec.size_mm = json.size_mm / cap_per_em;
    }
    Ok(spec)
}

/// Glyph outlines of straight text, placed at its origin.
fn plain_text_paths(fonts: &FontLibrary, args: &TextCreateArgs) -> Result<Vec<Path>, CoreError> {
    straight_text(fonts, &args.spec, Point::new(args.x, args.y))
}

/// Glyph outlines of straight text or a text block at `origin`.
pub(crate) fn straight_text(
    fonts: &FontLibrary,
    json: &TextSpecJson,
    origin: Point,
) -> Result<Vec<Path>, CoreError> {
    let spec = layout_spec(fonts, json)?;
    if let Some(auto) = &json.auto_layout {
        // Already placed in the box, in job space.
        return geom::text::auto::layout_auto(fonts, &spec, auto);
    }
    if json.wrap_width.is_none() && super::text_adjust::is_adjusted(json) {
        return super::text_adjust::adjusted_text(fonts, &spec, json, origin);
    }
    let laid = match json.wrap_width {
        Some(width_mm) => geom::layout_text_block(
            fonts,
            &geom::TextBlockSpec {
                text: spec,
                width_mm,
            },
        )?,
        None => geom::layout_text(fonts, &spec)?,
    };
    Ok(place(anchored(laid, json.anchor), origin))
}

/// `paths`, laid out about the first baseline's start, moved so that the
/// anchor point of their box sits there instead.
fn anchored(paths: Vec<Path>, anchor: Option<super::entity::TextAnchor>) -> Vec<Path> {
    let Some(anchor) = anchor else {
        return paths;
    };
    let mut bounds: Option<geom::BBox> = None;
    for path in &paths {
        if let Some(b) = geom::path_bbox(path) {
            bounds = Some(match bounds {
                Some(all) => all.union(b),
                None => b,
            });
        }
    }
    let Some(b) = bounds else {
        return paths;
    };
    let (fx, fy) = anchor.fractions();
    let at = Point::new(
        b.min.x + (b.max.x - b.min.x) * fx,
        b.min.y + (b.max.y - b.min.y) * fy,
    );
    let m = geom::Mat3::translate(geom::Vec2::new(-at.x, -at.y));
    paths.iter().map(|p| geom::transform_path(p, &m)).collect()
}

/// Glyph outlines of text set along the vector its [`OnCurve`] names.
///
/// # Errors
/// [`CoreError::Validation`] when that vector is gone or is not a vector.
pub(crate) fn curve_text(
    fonts: &FontLibrary,
    document: &Document,
    json: &TextSpecJson,
    on: &OnCurve,
) -> Result<Vec<Path>, CoreError> {
    let spec = layout_spec(fonts, json)?;
    let guide = document
        .entity(on.path_id)
        .ok()
        .and_then(|e| e.path())
        .ok_or_else(|| CoreError::validation("the curve the text is on is gone"))?;
    let height = json.size_mm;
    let (offset, drop) = match on.position {
        CurvePosition::Above => (on.offset, 0.0),
        CurvePosition::On => (on.offset, height * 0.5),
        CurvePosition::Below => (-on.offset, height),
    };
    let layout = geom::CurveLayout {
        offset,
        side: if on.other_side {
            TextSide::Right
        } else {
            TextSide::Left
        },
        align: on.align,
        start_at: on.start_at,
        spacing: on.spacing / 100.0,
        fill: on.fill,
        keep_vertical: on.keep_vertical,
        drop,
    };
    geom::text_on_curve_with(fonts, &spec, &guide, &layout, Tolerance::default())
}

/// True when the newest undo step made or edited text `id` and nothing has
/// changed the document since `merge` was answered.
fn joins_last_text_step(document: &Document, merge: Option<u64>, id: u64) -> bool {
    let Some(revision) = merge else {
        return false;
    };
    let label = document.journal().undo_label().unwrap_or("");
    revision == document.revision()
        && matches!(label, "Create Text" | "Edit Text")
        && document.newest_step_touches(id)
}

/// Copies of text on a curve follow the copy of their curve when the curve
/// was copied by the same command; otherwise they become plain text where
/// they are, so a copy never jumps back onto the original's curve (or onto
/// an unrelated vector with that id in another job). The change joins the
/// step that made the copies.
pub(crate) fn relink_copies(document: &mut Document, copies: &BTreeMap<u64, u64>) {
    let mut fixes: Vec<(u64, Option<u64>)> = Vec::new();
    for copy in copies.values() {
        let Ok(entity) = document.entity(*copy) else {
            continue;
        };
        let EntityKind::Text(text) = &entity.kind else {
            continue;
        };
        let Some(on) = &text.spec.on_curve else {
            continue;
        };
        let target = copies.get(&on.path_id).copied();
        if target != Some(on.path_id) {
            fixes.push((*copy, target));
        }
    }
    if fixes.is_empty() || !document.reopen_newest_step() {
        return;
    }
    let revision = document.revision();
    for (id, target) in fixes {
        document.capture(id);
        if let Some(entity) = document.entities_mut().get_mut(&id) {
            if let EntityKind::Text(text) = &mut entity.kind {
                match (target, text.spec.on_curve.as_mut()) {
                    (Some(curve), Some(on)) => on.path_id = curve,
                    _ => text.spec.on_curve = None,
                }
            }
            entity.revision = revision;
        }
    }
    document.close_reopened();
}

/// Lays text out again along any curve that changed after the text was
/// last laid out on it (a node edit, a move or a size change of the curve
/// alone), folding the new layout into the step that changed the curve so
/// one Undo takes both back. Text whose curve is gone stays as it was.
pub(crate) fn follow_curves(document: &mut Document, fonts: &FontLibrary) {
    let mut relaid: Vec<(u64, Vec<Path>)> = Vec::new();
    for (id, entity) in document.entities() {
        let EntityKind::Text(text) = &entity.kind else {
            continue;
        };
        let Some(on) = &text.spec.on_curve else {
            continue;
        };
        let Ok(curve) = document.entity(on.path_id) else {
            continue;
        };
        if curve.revision <= entity.revision || !document.newest_step_touches(on.path_id) {
            continue;
        }
        if let Ok(paths) = curve_text(fonts, document, &text.spec, on) {
            relaid.push((*id, paths));
        }
    }
    if relaid.is_empty() || !document.reopen_newest_step() {
        return;
    }
    let revision = document.revision();
    for (id, paths) in relaid {
        document.capture(id);
        if let Some(entity) = document.entities_mut().get_mut(&id) {
            if let EntityKind::Text(text) = &mut entity.kind {
                text.paths = paths;
            }
            entity.revision = revision;
        }
    }
    document.close_reopened();
}

/// The text entity of `args` set along its curve, not yet stored.
fn on_curve_entity(
    fonts: &FontLibrary,
    document: &Document,
    args: &TextOnCurveArgs,
) -> Result<(u64, Entity), CoreError> {
    let mut entity = document.entity(args.text_id)?.clone();
    let EntityKind::Text(text) = &mut entity.kind else {
        return Err(CoreError::validation(
            "select a single line of text and the curve to set it on",
        ));
    };
    if args.curve.path_id == args.text_id {
        return Err(CoreError::validation("the text cannot follow itself"));
    }
    if let Some(spec) = &args.spec {
        text.spec = spec.clone();
    }
    text.spec.auto_layout = None;
    text.spec.wrap_width = None;
    text.spec.arch = None;
    text.spec.on_curve = Some(args.curve.clone());
    text.paths = curve_text(fonts, document, &text.spec, &args.curve)?;
    entity.rotation = 0.0;
    Ok((args.text_id, entity))
}

/// Glyph outlines of text bent round a circular arc.
fn arc_text_paths(fonts: &FontLibrary, args: &TextArcArgs) -> Result<Vec<Path>, CoreError> {
    let spec = layout_spec(fonts, &args.spec)?;
    geom::arc_text(
        fonts,
        &spec,
        Point::new(args.cx, args.cy),
        args.radius,
        args.start_angle_deg.to_radians(),
        args.sweep_deg.to_radians(),
        args.inside,
        Tolerance::default(),
    )
}

fn layer_or_active(document: &Document, layer: Option<u64>) -> Result<u64, CoreError> {
    match layer {
        Some(id) => {
            document.layer(id)?;
            Ok(id)
        }
        None => Ok(document.active_layer()),
    }
}

/// Replaces each text entity among `ids` with plain vectors: one per glyph,
/// grouped when there are several, on the text's layer and sheet with its
/// rotation. Answers the new entities (groups or single vectors).
///
/// # Errors
/// [`CoreError::Validation`] when none of the ids is text.
pub fn text_to_curves(doc: &mut Document, ids: &[u64]) -> Result<Vec<u64>, CoreError> {
    let mut made = Vec::new();
    for id in ids {
        let entity = doc.entity(*id)?.clone();
        let EntityKind::Text(text) = &entity.kind else {
            continue;
        };
        let parts: Vec<u64> = text
            .paths
            .iter()
            .filter(|path| !path.is_empty())
            .map(|path| {
                doc.insert_entity(Entity {
                    rotation: entity.rotation,
                    id: 0,
                    layer: entity.layer,
                    sheet: entity.sheet,
                    revision: 0,
                    name: None,
                    parent: None,
                    kind: EntityKind::Path { path: path.clone() },
                })
            })
            .collect();
        doc.delete_entities(&[*id]);
        match parts.as_slice() {
            [] => {}
            [one] => made.push(*one),
            _ => {
                let group = doc.group(&parts)?;
                if let Ok(g) = doc.entity(group) {
                    let mut g = g.clone();
                    g.rotation = entity.rotation;
                    g.name = entity.name.clone();
                    doc.replace_entity(group, g)?;
                }
                made.push(group);
            }
        }
    }
    if made.is_empty() {
        return Err(CoreError::validation(
            "select the text to convert to curves",
        ));
    }
    Ok(made)
}

/// The dimension style with a dimension's own text in place of its value.
fn with_text(style: &DimensionStyle, custom_text: Option<String>) -> DimensionStyle {
    DimensionStyle {
        custom_text,
        ..style.clone()
    }
}

/// Name of the layer dimensions go on, made (mid grey) when missing.
pub const DIMENSIONS_LAYER: &str = "Dimensions";

fn dimensions_layer(doc: &mut Document, name: &str) -> u64 {
    let name = if name.trim().is_empty() {
        DIMENSIONS_LAYER
    } else {
        name.trim()
    };
    if let Some(layer) = doc.layers().iter().find(|l| l.name == name) {
        return layer.id;
    }
    doc.add_layer(Some(name.to_string()), Some("#808080".to_string()))
}

fn style_json(style: &DimensionStyle) -> serde_json::Value {
    json!({
        "fontFamily": style.text.font_family,
        "textHeight": style.text.size_mm,
        "precision": style.precision,
        "anglePrecision": style.angle_precision,
        "units": style.units,
        "showUnits": style.show_units,
        "offset": style.extension_gap,
        "arrowSize": style.arrow_length,
        "useLayer": style.use_layer,
        "layerName": style.layer_name,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DimsCreateArgs {
    #[serde(flatten)]
    spec: DimensionSpec,
    #[serde(default)]
    custom_text: Option<String>,
    /// Where the value's text goes, when the final click placed it.
    #[serde(default)]
    text_at: Option<Point>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DimsUpdateArgs {
    id: u64,
    #[serde(default)]
    spec: Option<DimensionSpec>,
    #[serde(default)]
    custom_text: Option<String>,
    /// With `spec`: where the text was dragged to, none for the middle.
    #[serde(default)]
    text_at: Option<Point>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DimsStyleArgs {
    font_family: Option<String>,
    text_height: Option<f64>,
    precision: Option<usize>,
    angle_precision: Option<usize>,
    units: Option<geom::DimensionUnits>,
    show_units: Option<bool>,
    offset: Option<f64>,
    arrow_size: Option<f64>,
    use_layer: Option<bool>,
    layer_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlyphsArgs {
    id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToCurvesArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

/// Moves laid out glyph outlines to where the text sits.
fn place(paths: Vec<Path>, origin: Point) -> Vec<Path> {
    let m = geom::Mat3::translate(geom::Vec2::new(origin.x, origin.y));
    paths.iter().map(|p| geom::transform_path(p, &m)).collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextCreateArgs {
    spec: TextSpecJson,
    x: f64,
    y: f64,
    #[serde(default)]
    layer: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextUpdateArgs {
    id: u64,
    #[serde(default)]
    spec: Option<TextSpecJson>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    /// The revision the previous live edit of this text answered: when
    /// nothing else changed the document since and that step made or edited
    /// this text, the edit joins that step (Create Text applies live, so a
    /// whole editing session undoes in one go).
    #[serde(default)]
    merge: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextOnCurveArgs {
    /// The text to set along the curve (it keeps its id).
    text_id: u64,
    /// The curve and how the text sits on it.
    curve: OnCurve,
    /// New text settings, when the form changed them too.
    #[serde(default)]
    spec: Option<TextSpecJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArgs {
    id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextArcArgs {
    spec: TextSpecJson,
    cx: f64,
    cy: f64,
    radius: f64,
    #[serde(default)]
    start_angle_deg: f64,
    #[serde(default = "half_turn")]
    sweep_deg: f64,
    #[serde(default)]
    inside: bool,
    #[serde(default)]
    layer: Option<u64>,
}

fn half_turn() -> f64 {
    180.0
}

/// What `text.preview` lays out: the arguments of the matching create
/// command, tagged with `kind`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TextPreviewArgs {
    Plain(TextCreateArgs),
    Arc(TextArcArgs),
    OnCurve(TextOnCurveArgs),
}
