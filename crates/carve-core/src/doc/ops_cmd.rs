//! Vector operation commands: `ops.*`, from joining and offsetting to
//! nesting and node editing. Each is one undo step.

use serde::Deserialize;
use serde_json::json;

use crate::doc::ops::{
    self, AlignMode, AlignTarget, Anchor, ArrayArgs, Boolean, BoundaryKind, CircularArrayArgs,
    CopyAlongArgs, DistributeMode, MirrorAxis, TrimSide,
};
use crate::doc::{node_join, Document};
use crate::error::CoreError;
use crate::geom::{FilletKind, OffsetCorner, OffsetSide, Point, Region, Tolerance};
use crate::kernel::{edit, parse_args, IdsArgs, KernelReply};
use crate::model::split::{inside_mask, split};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "ops.join"
            | "ops.close"
            | "ops.offset"
            | "ops.weld"
            | "ops.subtract"
            | "ops.intersect"
            | "ops.xor"
            | "ops.crop"
            | "ops.trimObjects"
            | "ops.fillet"
            | "ops.filletProbe"
            | "ops.chamfer"
            | "ops.trim"
            | "ops.trimProbe"
            | "ops.extend"
            | "ops.extendTo"
            | "ops.move"
            | "ops.setSize"
            | "ops.rotate"
            | "ops.mirror"
            | "ops.align"
            | "ops.distribute"
            | "ops.nest"
            | "ops.nestMarks"
            | "ops.arrayCopy"
            | "ops.circularArray"
            | "ops.copyAlong"
            | "ops.validate"
            | "ops.fixZeroLength"
            | "ops.crossings"
            | "ops.simplify"
            | "ops.fitCurves"
            | "ops.boundary"
            | "ops.reverse"
            | "ops.duplicate"
            | "ops.node.insert"
            | "ops.node.delete"
            | "ops.node.convert"
            | "ops.node.smooth"
            | "ops.node.setStart"
            | "ops.node.cut"
            | "ops.node.split"
            | "ops.node.join"
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
        "ops.join" => {
            let args: JoinArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Join", |doc| {
                let out = ops::join(doc, &ids, args.tolerance, args.smooth)?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.close" => {
            let args: CloseArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Close", |doc| {
                let out = ops::close(doc, &ids, &args.mode)?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.offset" => {
            let args: OffsetArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Offset", |doc| {
                let out = ops::offset(
                    doc,
                    &ids,
                    args.distance,
                    args.side,
                    args.corner,
                    args.delete_original,
                )?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.weld" | "ops.subtract" | "ops.intersect" | "ops.xor" | "ops.crop" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let (label, which) = match name {
                "ops.weld" => ("Weld", Boolean::Weld),
                "ops.subtract" => ("Subtract", Boolean::Subtract),
                "ops.intersect" => ("Intersect", Boolean::Intersect),
                "ops.xor" => ("Exclusive Or", Boolean::Xor),
                _ => ("Crop", Boolean::Crop),
            };
            edit(document, label, |doc| {
                let out = ops::boolean(doc, &ids, which)?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.trimObjects" => {
            let args: TrimObjectsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Trim Objects", |doc| {
                let components = trim_components(doc, &ids, &args)?;
                let out = if ids.len() > 1 {
                    ops::trim_objects(doc, &ids, args.side, args.refit)?
                } else {
                    Vec::new()
                };
                let mut selected = out.clone();
                selected.extend(ids.last());
                doc.set_selection(&selected);
                Ok(json!({ "ids": out, "componentIds": components }))
            })
        }
        "ops.fillet" => {
            let args: FilletArgs = parse_args(name, args)?;
            edit(document, "Fillet", |doc| {
                ops::fillet(
                    doc,
                    args.id,
                    Point::new(args.x, args.y),
                    args.radius,
                    args.kind,
                )?;
                Ok(json!({}))
            })
        }
        "ops.chamfer" => {
            let args: ChamferArgs = parse_args(name, args)?;
            edit(document, "Chamfer", |doc| {
                ops::chamfer(doc, args.id, Point::new(args.x, args.y), args.distance)?;
                Ok(json!({}))
            })
        }
        "ops.filletProbe" => {
            // What a click here would do, for the tool's cursor: "remove" a
            // fillet, "fillet" a corner, or "none". Changes nothing.
            let args: FilletArgs = parse_args(name, args)?;
            let path = document.entity(args.id)?.path();
            let at = Point::new(args.x, args.y);
            let action = match path.as_ref().and_then(|p| ops::fillet_target(p, at)) {
                Some(ops::FilletTarget::Remove { .. }) => "remove",
                Some(ops::FilletTarget::Corner { sub, node }) => {
                    let sub_path = path.as_ref().map(|p| &p.subpaths[sub]);
                    let side = sub_path.and_then(|s| crate::geom::tbone_side_near(s, node, at));
                    let fits = sub_path.is_some_and(|s| {
                        crate::geom::fillet_corner_on(s, node, args.radius, args.kind, side).is_ok()
                    });
                    if fits {
                        "fillet"
                    } else {
                        "none"
                    }
                }
                None => "none",
            };
            KernelReply::ok(&json!({ "action": action }))
        }
        "ops.trimProbe" => {
            // Whether a click here would trim, for the scissors cursor. The
            // trim is tried inside a step that is then thrown away.
            let args: PickArgs = parse_args(name, args)?;
            document.entity(args.id)?;
            document.begin("Trim");
            let trims = ops::trim(document, args.id, Point::new(args.x, args.y)).is_ok();
            document.rollback();
            KernelReply::ok(&json!({ "trims": trims }))
        }
        "ops.trim" => {
            let args: PickArgs = parse_args(name, args)?;
            edit(document, "Trim", |doc| {
                ops::trim(doc, args.id, Point::new(args.x, args.y))?;
                Ok(json!({}))
            })
        }
        "ops.extend" => {
            let args: PickArgs = parse_args(name, args)?;
            edit(document, "Extend", |doc| {
                ops::extend(doc, args.id, Point::new(args.x, args.y))?;
                Ok(json!({}))
            })
        }
        "ops.extendTo" => {
            let args: ExtendToArgs = parse_args(name, args)?;
            edit(document, "Extend", |doc| {
                ops::extend_ends_to(doc, &args.ends, Point::new(args.x, args.y))?;
                Ok(json!({}))
            })
        }
        "ops.move" => {
            let args: MoveArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Move", |doc| {
                match (args.dx, args.dy, args.x, args.y) {
                    (Some(dx), Some(dy), _, _) => ops::move_by(doc, &ids, dx, dy)?,
                    (_, _, Some(x), Some(y)) => {
                        ops::move_to(doc, &ids, x, y, args.anchor.unwrap_or_default())?
                    }
                    _ => {
                        return Err(CoreError::validation(
                            "a move needs either dx and dy or x and y",
                        ))
                    }
                }
                Ok(json!({}))
            })
        }
        "ops.setSize" => {
            let args: SetSizeArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Set Size", |doc| {
                ops::set_size(
                    doc,
                    &ids,
                    args.width,
                    args.height,
                    args.link_xy,
                    args.anchor.unwrap_or_default(),
                )?;
                Ok(json!({}))
            })
        }
        "ops.rotate" => {
            let args: RotateArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let (cx, cy) = match (args.cx, args.cy) {
                (Some(cx), Some(cy)) => (cx, cy),
                _ => {
                    let centre = document
                        .bbox_of_many(&ids)
                        .map(|b| b.centre())
                        .unwrap_or(Point::ORIGIN);
                    (centre.x, centre.y)
                }
            };
            edit(document, "Rotate", |doc| {
                ops::rotate(doc, &ids, args.angle_deg, cx, cy)?;
                Ok(json!({}))
            })
        }
        "ops.mirror" => {
            let args: MirrorArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let line = match (args.line, args.line_to) {
                (Some([ax, ay]), Some([bx, by])) => Some((Point::new(ax, ay), Point::new(bx, by))),
                _ => None,
            };
            edit(document, "Mirror", |doc| {
                let out = ops::mirror(doc, &ids, args.axis, line, args.copy)?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.align" => {
            let args: AlignArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Align", |doc| {
                ops::align(doc, &ids, args.mode, args.target)?;
                Ok(json!({}))
            })
        }
        "ops.distribute" => {
            let args: DistributeArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Distribute", |doc| {
                ops::distribute(doc, &ids, args.mode, args.spacing)?;
                Ok(json!({}))
            })
        }
        "ops.nest" => super::nest_cmd::run(document, name, args),
        "ops.nestMarks" => super::nest_cmd::run_marks(document, name, args),
        "ops.arrayCopy" => {
            let args: WithComponents<ArrayArgs> = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let components = args.components;
            let more = crate::model::component_cmd::components_box(document, &components)?;
            edit(document, "Array Copy", |doc| {
                let mut made = Vec::new();
                let out = ops::array_copy_with(doc, &ids, &args.rest, more, &mut |doc, m, map| {
                    made.extend(crate::model::component_cmd::copy_components(
                        doc,
                        &components,
                        m,
                        map,
                    )?);
                    Ok(())
                })?;
                Ok(json!({ "ids": out, "components": made }))
            })
        }
        "ops.circularArray" => {
            let args: WithComponents<CircularArrayArgs> = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let components = args.components;
            let more = crate::model::component_cmd::components_box(document, &components)?;
            edit(document, "Circular Array", |doc| {
                let mut made = Vec::new();
                let out =
                    ops::circular_array_with(doc, &ids, &args.rest, more, &mut |doc, m, map| {
                        made.extend(crate::model::component_cmd::copy_components(
                            doc,
                            &components,
                            m,
                            map,
                        )?);
                        Ok(())
                    })?;
                Ok(json!({ "ids": out, "components": made }))
            })
        }
        "ops.copyAlong" => {
            let args: WithComponents<CopyAlongArgs> = parse_args(name, args)?;
            // The guide is picked with the selection but is never copied.
            let mut ids = document.target_ids(args.ids.as_deref());
            let guides = args.rest.guides();
            ids.retain(|id| !guides.contains(id));
            let components = args.components;
            let more = crate::model::component_cmd::components_box(document, &components)?;
            edit(document, "Copy Along Vector", |doc| {
                let mut made = Vec::new();
                let out = ops::copy_along_with(doc, &ids, &args.rest, more, &mut |doc, m, map| {
                    made.extend(crate::model::component_cmd::copy_components(
                        doc,
                        &components,
                        m,
                        map,
                    )?);
                    Ok(())
                })?;
                if args.rest.delete_original {
                    doc.delete_components(&components)?;
                }
                Ok(json!({ "ids": out, "components": made }))
            })
        }
        "ops.validate" => {
            let args: ValidateArgs = parse_args(name, args)?;
            let mut ids = document.target_ids(args.ids.as_deref());
            if ids.is_empty() {
                // Nothing selected: the visible vectors of the sheet shown,
                // on visible layers (the current side is the one loaded).
                ids = document
                    .order()
                    .iter()
                    .copied()
                    .filter(|id| {
                        document.entity(*id).is_ok_and(|e| {
                            e.sheet == document.active_sheet()
                                && document.layer(e.layer).is_ok_and(|l| l.visible)
                        })
                    })
                    .collect();
                if ids.is_empty() {
                    return KernelReply::ok(&json!({ "issues": [] }));
                }
            }
            let issues = ops::validate_with(document, &ids, args.vgroove)?;
            KernelReply::ok(&json!({ "issues": issues }))
        }
        "ops.fixZeroLength" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Fix Zero Length Spans", |doc| {
                let removed = ops::fix_zero_length(doc, &ids)?;
                Ok(json!({ "removed": removed }))
            })
        }
        "ops.crossings" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let crossings = ops::crossings(document, &ids)?;
            KernelReply::ok(&json!({ "crossings": crossings }))
        }
        "ops.simplify" => {
            let args: SimplifyArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Simplify", |doc| {
                ops::simplify(doc, &ids, args.tolerance, args.keep_arcs)?;
                Ok(json!({}))
            })
        }
        "ops.fitCurves" => {
            let args: FitArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            let options = ops::FitOptions {
                sharp_angle: args.sharp_angle,
                keep_originals: !args.replace,
            };
            edit(document, "Fit Curves", |doc| {
                let out = ops::fit_curves_with(doc, &ids, args.tolerance, &args.kind, options)?;
                Ok(json!({ "ids": out }))
            })
        }
        "ops.boundary" => {
            let args: BoundaryArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Boundary", |doc| {
                let out = ops::boundary(doc, &ids, args.offset, args.kind, args.rubber_band)?;
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.reverse" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            edit(document, "Reverse", |doc| {
                ops::reverse(doc, &ids)?;
                Ok(json!({}))
            })
        }
        "ops.duplicate" => {
            let args: IdsArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            ops::require(document, &ids, "duplicate")?;
            edit(document, "Duplicate", |doc| {
                let mut out = Vec::with_capacity(ids.len());
                for id in &ids {
                    out.push(ops::duplicate(doc, *id)?);
                }
                doc.set_selection(&out);
                Ok(json!({ "ids": out }))
            })
        }
        "ops.node.insert" => {
            let args: PickArgs = parse_args(name, args)?;
            edit(document, "Insert Node", |doc| {
                ops::node_insert(doc, args.id, Point::new(args.x, args.y))?;
                Ok(json!({}))
            })
        }
        "ops.node.delete" => {
            let args: NodeArgs = parse_args(name, args)?;
            edit(document, "Delete Node", |doc| {
                ops::node_delete(doc, args.id, args.subpath, args.node)?;
                Ok(json!({}))
            })
        }
        "ops.node.convert" => {
            let args: ConvertArgs = parse_args(name, args)?;
            edit(document, "Convert Segment", |doc| {
                ops::node_convert(doc, args.id, args.subpath, args.segment, &args.to)?;
                Ok(json!({}))
            })
        }
        "ops.node.smooth" => {
            let args: NodeArgs = parse_args(name, args)?;
            edit(document, "Smooth Node", |doc| {
                ops::node_smooth(doc, args.id, args.subpath, args.node)?;
                Ok(json!({}))
            })
        }
        "ops.node.setStart" => {
            let args: NodeArgs = parse_args(name, args)?;
            edit(document, "Set Start Point", |doc| {
                ops::node_set_start(doc, args.id, args.subpath, args.node)?;
                Ok(json!({}))
            })
        }
        "ops.node.cut" | "ops.node.split" => {
            let args: NodeArgs = parse_args(name, args)?;
            edit(document, "Cut Vector", |doc| {
                ops::node_cut(doc, args.id, args.subpath, args.node)?;
                Ok(json!({}))
            })
        }
        "ops.node.join" => {
            let args: NodeJoinArgs = parse_args(name, args)?;
            edit(document, "Join Vectors", |doc| {
                let id = node_join::join_end(doc, args.id, args.subpath, args.at_start)?;
                Ok(json!({ "id": id }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeJoinArgs {
    id: u64,
    #[serde(default)]
    subpath: usize,
    at_start: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default = "default_join_tolerance")]
    tolerance: f64,
    #[serde(default)]
    smooth: bool,
}

fn default_join_tolerance() -> f64 {
    0.01
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrimObjectsArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Components to trim with the boundary as well.
    #[serde(default)]
    component_ids: Vec<u64>,
    #[serde(default)]
    side: TrimSide,
    #[serde(default = "yes")]
    refit: bool,
}

fn yes() -> bool {
    true
}

/// Trim Objects on components: the cells on the cleared side of the
/// boundary (the last vector id) are removed. A component with nothing
/// left is deleted. Answers the components that remain.
fn trim_components(
    doc: &mut Document,
    ids: &[u64],
    args: &TrimObjectsArgs,
) -> Result<Vec<u64>, CoreError> {
    if args.component_ids.is_empty() {
        if ids.len() < 2 {
            return Err(CoreError::validation(
                "select the objects to trim, then the boundary",
            ));
        }
        return Ok(Vec::new());
    }
    let last = ids
        .last()
        .ok_or_else(|| CoreError::validation("select the boundary vector last"))?;
    let boundary = ops::trim_boundary(doc, *last)?;
    let tol = Tolerance::new(0.01)?;
    let region = Region::from_paths(&boundary, tol);
    let mut kept = Vec::new();
    for id in &args.component_ids {
        let field = doc.component(*id)?.field.clone();
        let mask = inside_mask(&field, &region, tol);
        let (inside, outside) = split(&field, &mask);
        let left = match args.side {
            TrimSide::Inside => outside,
            TrimSide::Outside => inside,
        };
        match left {
            Some(next) => {
                doc.update_component(*id, |c| c.field = next)?;
                kept.push(*id);
            }
            None => doc.delete_components(&[*id])?,
        }
    }
    Ok(kept)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default = "straight")]
    mode: String,
}

fn straight() -> String {
    "straight".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OffsetArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    distance: f64,
    #[serde(default = "outward")]
    side: OffsetSide,
    #[serde(default = "round_corner")]
    corner: OffsetCorner,
    #[serde(default)]
    delete_original: bool,
}

fn outward() -> OffsetSide {
    OffsetSide::Outward
}

fn round_corner() -> OffsetCorner {
    OffsetCorner::Round
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilletArgs {
    id: u64,
    x: f64,
    y: f64,
    radius: f64,
    #[serde(default = "normal_fillet")]
    kind: FilletKind,
}

fn normal_fillet() -> FilletKind {
    FilletKind::Normal
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChamferArgs {
    id: u64,
    x: f64,
    y: f64,
    distance: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickArgs {
    id: u64,
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtendToArgs {
    ends: Vec<ops::OpenEnd>,
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeArgs {
    id: u64,
    #[serde(default)]
    subpath: usize,
    node: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertArgs {
    id: u64,
    #[serde(default)]
    subpath: usize,
    segment: usize,
    to: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    dx: Option<f64>,
    #[serde(default)]
    dy: Option<f64>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    anchor: Option<Anchor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSizeArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    link_xy: bool,
    #[serde(default)]
    anchor: Option<Anchor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RotateArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    angle_deg: f64,
    #[serde(default)]
    cx: Option<f64>,
    #[serde(default)]
    cy: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    axis: MirrorAxis,
    #[serde(default)]
    line: Option<[f64; 2]>,
    #[serde(default)]
    line_to: Option<[f64; 2]>,
    #[serde(default)]
    copy: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlignArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    mode: AlignMode,
    #[serde(default)]
    target: AlignTarget,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DistributeArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    mode: DistributeMode,
    #[serde(default)]
    spacing: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimplifyArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default = "default_simplify_tolerance")]
    tolerance: f64,
    #[serde(default)]
    keep_arcs: bool,
}

fn default_simplify_tolerance() -> f64 {
    0.05
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FitArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default = "default_simplify_tolerance")]
    tolerance: f64,
    kind: String,
    /// Keep Sharp Corners: the Max Angle in degrees; absent, off.
    #[serde(default)]
    sharp_angle: Option<f64>,
    /// Replace selected vectors (on unless sent false).
    #[serde(default = "yes")]
    replace: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// V-Groove Mode: crossings involving text are left out.
    #[serde(default)]
    vgroove: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundaryArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    offset: f64,
    #[serde(default)]
    kind: BoundaryKind,
    /// The band stretched round everything instead of the outline.
    #[serde(default)]
    rubber_band: bool,
}

/// The array copies (`ops.arrayCopy`, `ops.circularArray`, `ops.copyAlong`):
/// their settings, the entities (the selection when absent) and the relief
/// components copied with them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WithComponents<T> {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    components: Vec<u64>,
    #[serde(flatten)]
    rest: T,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::rect;
    use crate::model::field::Field;
    use crate::model::{grid_for, Combine, Component};

    fn add_block(doc: &mut Document, x0: f64, x1: f64) -> u64 {
        let grid = grid_for(doc.job());
        let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for j in 0..grid.height {
            for i in 0..grid.width {
                let c = field.centre(i, j);
                if c.x >= x0 && c.x <= x1 && c.y >= 10.0 && c.y <= 40.0 {
                    field.z[j * grid.width + i] = 2.0;
                }
            }
        }
        doc.begin("Add");
        let id = doc.add_component(Component {
            id: 0,
            name: "Block".to_string(),
            visible: true,
            combine: Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field: field.cropped().expect("cells"),
        });
        doc.commit();
        id
    }

    fn add_square(doc: &mut Document, x0: f64, x1: f64) -> u64 {
        let path = rect(Point::new(x0, 0.0), Point::new(x1, 50.0)).expect("rect");
        doc.begin("Rect");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        id
    }

    #[test]
    fn trim_objects_clears_a_component_outside_the_boundary() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 10.0, 60.0);
        let boundary = add_square(&mut doc, 30.0, 80.0);
        let args = json!({ "ids": [boundary], "componentIds": [block], "side": "outside" });
        dispatch(&mut doc, "ops.trimObjects", &args.to_string()).expect("trims");
        let field = &doc.component(block).expect("component").field;
        let left = field.origin.x - field.cell * 0.5;
        assert!(
            left >= 29.0,
            "the part left of the boundary is gone: {left}"
        );
        assert!(doc.entity(boundary).is_ok(), "the boundary stays");
    }

    #[test]
    fn trim_objects_deletes_a_component_left_with_nothing() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 40.0, 60.0);
        let boundary = add_square(&mut doc, 30.0, 80.0);
        let args = json!({ "ids": [boundary], "componentIds": [block], "side": "inside" });
        dispatch(&mut doc, "ops.trimObjects", &args.to_string()).expect("trims");
        assert!(doc.component(block).is_err());
    }
}
