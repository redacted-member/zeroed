//! Commands that build a new relief component: shapes, imported meshes,
//! sweeps, extrusions, texture areas and turned or spun profiles.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::{self, Path, Tolerance};
use crate::io;
use crate::kernel::{edit, parse_args, parse_value, KernelReply};

use super::tools_cmd::undo_if_last;
use crate::model;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.createShape"
            | "model.importMesh"
            | "model.sweep"
            | "model.extrude"
            | "model.textureArea"
            | "model.turn"
            | "model.spin"
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
        "model.createShape" => create_shape(document, parse_args(name, args)?),
        "model.importMesh" => {
            let args: ImportMeshArgs = parse_args(name, args)?;
            let triangles = io::mesh::read(&args.path)?;
            let grid = model::grid_for(document.job());
            let bounds = document.job_bounds();
            let full = model::mesh::project(&triangles, &grid, &args.params, bounds.centre())?;
            let field = full
                .cropped()
                .ok_or_else(|| CoreError::validation("the model lands outside the material"))?;
            let fallback = args
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model")
                .to_string();
            let name_given = args
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(fallback);
            edit(document, "Import Component", |doc| {
                let id = doc.add_component(model::Component {
                    id: 0,
                    name: name_given,
                    visible: true,
                    combine: args.combine,
                    height_scale: 1.0,
                    level: 0,
                    extra: Default::default(),
                    field,
                });
                Ok(json!({ "id": id, "triangles": triangles.len() }))
            })
        }
        "model.sweep" => {
            let args: SweepArgs = parse_args(name, args)?;
            let rail = |id: u64| -> Result<Path, CoreError> {
                document
                    .entity(id)?
                    .path()
                    .ok_or_else(|| CoreError::validation("a rail must be a vector"))
            };
            let (rail_a, rail_b) = (rail(args.rails[0])?, rail(args.rails[1])?);
            let sections = args
                .sections
                .iter()
                .map(|id| rail(*id))
                .collect::<Result<Vec<_>, _>>()?;
            let grid = model::grid_for(document.job());
            let field = model::sweep::two_rail_sweep(
                &rail_a,
                &rail_b,
                &sections,
                &grid,
                &args.params,
                Tolerance::new(0.01)?,
            )?;
            apply_component(
                document,
                &Made {
                    label: "Two-Rail Sweep",
                    base: "Sweep",
                    combine: args.combine,
                    name: args.name,
                    apply: args.apply,
                    hide: None,
                    reply: Value::Null,
                },
                field,
                |_| {},
            )
        }
        "model.extrude" => {
            let args: ExtrudeArgs = parse_args(name, args)?;
            let vector = |id: u64| -> Result<Path, CoreError> {
                document.entity(id)?.path().ok_or_else(|| {
                    CoreError::validation("rails and cross sections must be vectors")
                })
            };
            let rails = args
                .rails
                .iter()
                .map(|id| vector(*id))
                .collect::<Result<Vec<_>, _>>()?;
            let grid = model::grid_for(document.job());
            let tol = Tolerance::new(0.01)?;
            let field = if let Some(id) = args.component {
                // The component as it shows, its dynamic properties baked in.
                let c = document.component(id)?;
                let e = &c.extra;
                let carried = model::dynamic::bake_field(
                    &c.field,
                    c.height_scale,
                    e.base_height,
                    e.fade.as_ref(),
                    e.tilt.as_ref(),
                );
                model::extrude::extrude(
                    &rails,
                    model::extrude::Profile::Component(&carried),
                    &grid,
                    &args.params,
                    tol,
                )?
            } else {
                let sections = args
                    .sections
                    .iter()
                    .map(|id| vector(*id))
                    .collect::<Result<Vec<_>, _>>()?;
                model::extrude::extrude(
                    &rails,
                    model::extrude::Profile::Sections(&sections),
                    &grid,
                    &args.params,
                    tol,
                )?
            };
            apply_component(
                document,
                &Made {
                    label: "Extrude and Weave",
                    base: "Extrusion",
                    combine: args.combine,
                    name: args.name,
                    apply: args.apply,
                    hide: None,
                    reply: Value::Null,
                },
                field,
                |_| {},
            )
        }
        "model.textureArea" => texture_area(document, parse_args(name, args)?),
        "model.turn" | "model.spin" => {
            let args: RevolveArgs = parse_args(name, args)?;
            let profile = document
                .entity(args.id)?
                .path()
                .ok_or_else(|| CoreError::validation("the profile must be a vector"))?;
            let grid = model::grid_for(document.job());
            let tol = Tolerance::new(0.01)?;
            let (label, base, field) = if name == "model.turn" {
                let params: model::turn::TurnParams = parse_value(&args.params)?;
                (
                    "Turn",
                    "Turned",
                    model::turn::turn(&profile, &grid, &params, tol)?,
                )
            } else {
                let params: model::turn::SpinParams = parse_value(&args.params)?;
                (
                    "Spin",
                    "Spun",
                    model::turn::spin(&profile, &grid, &params, tol)?,
                )
            };
            apply_component(
                document,
                &Made {
                    label,
                    base,
                    combine: args.combine,
                    name: args.name,
                    apply: args.apply,
                    hide: None,
                    reply: Value::Null,
                },
                field,
                |_| {},
            )
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// Undo labels of the creation commands whose Apply a form may take back
/// (Reset) or replace (Apply again).
pub const CREATE_LABELS: [&str; 6] = [
    CREATE_SHAPE,
    "Two-Rail Sweep",
    "Extrude and Weave",
    "Create Texture Area",
    "Turn",
    "Spin",
];

/// Undo label of the shape tools.
pub const CREATE_SHAPE: &str = "Create Shape";

/// How a creation form's Apply lands. With neither field it adds a new
/// component. `replace` is the revision the form's previous Apply
/// answered: when that step is still the newest it is taken back first,
/// so Apply after Apply leaves one undo step. `target` is the component
/// the previous Apply made (or the one being edited): when it still exists
/// it is updated in place, keeping its level and place in the tree.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyArgs {
    #[serde(default)]
    target: Option<u64>,
    #[serde(default)]
    replace: Option<u64>,
}

/// What a creation command made and how it is to be named and combined.
struct Made<'a> {
    label: &'a str,
    base: &'a str,
    combine: model::Combine,
    name: Option<String>,
    apply: ApplyArgs,
    /// A seed component to hide in the same step (Create Texture Area).
    hide: Option<u64>,
    /// More fields for the reply.
    reply: Value,
}

/// Crops a freshly built field and adds it as a new component, or puts it
/// into the target component, inside one undo step (see [`ApplyArgs`]).
/// `extra` sets the component's other properties. A new component is
/// named from `name` or `base` and the next number; an updated one keeps
/// its name unless `name` is given.
fn apply_component<F>(
    document: &mut Document,
    made: &Made<'_>,
    full: model::field::Field,
    extra: F,
) -> Result<KernelReply, CoreError>
where
    F: Fn(&mut model::tree::ComponentExtra),
{
    let field = full
        .cropped()
        .ok_or_else(|| CoreError::validation("the result covers no cell of the model"))?;
    let target = made
        .apply
        .target
        .filter(|id| document.component(*id).is_ok());
    // The previous Apply is still the newest step: an update in place folds
    // into it (so the component keeps its id); a new component replaces it.
    let last = document.journal().undo_label().unwrap_or("");
    let fresh = made.apply.replace == Some(document.revision()) && CREATE_LABELS.contains(&last);
    let fold = (fresh && target.is_some()).then(|| document.journal().undo_depth() - 1);
    let undone = fold.is_none() && undo_if_last(document, made.apply.replace, &CREATE_LABELS);
    let target = target.filter(|id| document.component(*id).is_ok());
    let taken = document.components().len();
    let given = made.name.clone().filter(|n| !n.trim().is_empty());
    let combine = made.combine;
    let result = edit(document, made.label, |doc| {
        let id = if let Some(id) = target {
            doc.update_component(id, |c| {
                c.field = field;
                c.combine = combine;
                if let Some(name) = given {
                    c.name = name;
                }
                extra(&mut c.extra);
            })?;
            id
        } else {
            let mut component = model::Component {
                id: 0,
                name: given.unwrap_or_else(|| format!("{} {}", made.base, taken + 1)),
                visible: true,
                combine,
                height_scale: 1.0,
                level: 0,
                extra: Default::default(),
                field,
            };
            extra(&mut component.extra);
            doc.add_component(component)
        };
        if let Some(seed) = made.hide.filter(|seed| *seed != id) {
            doc.update_component(seed, |c| c.visible = false)?;
        }
        let c = doc.component(id)?;
        let mut reply = json!({
            "id": id,
            "shapeHeight": c.field.max().map(f64::from),
            "totalHeight": c.field.max().map(|v| f64::from(v) * c.height_scale + c.extra.base_height),
        });
        if let (Some(reply), Value::Object(more)) = (reply.as_object_mut(), &made.reply) {
            reply.extend(more.clone());
        }
        Ok(reply)
    });
    if result.is_err() && undone {
        document.redo();
    }
    if let (Ok(_), Some(depth)) = (&result, fold) {
        document.squash_steps_since(depth, made.label);
    }
    result
}

/// Tiles the seed component across the chosen area (or the job), hides the
/// seed, and keeps the settings on the new component so the tool can edit
/// it again.
fn texture_area(document: &mut Document, args: TextureAreaArgs) -> Result<KernelReply, CoreError> {
    let tol = Tolerance::new(0.01)?;
    let seed = document.component(args.component)?.field.clone();
    let mut boundary = Vec::new();
    for id in &args.ids {
        document.entity(*id)?;
        boundary.extend(
            document
                .paths_of(*id)
                .into_iter()
                .map(|(_, p)| p)
                .filter(|p| p.subpaths.iter().all(|s| s.closed)),
        );
    }
    // Editing a texture area with no vectors chosen keeps its own outline.
    let editing = args
        .apply
        .target
        .and_then(|id| document.component(id).ok())
        .and_then(|c| c.extra.texture.clone());
    let mut params = args.params;
    if boundary.is_empty() {
        if let Some(source) = &editing {
            boundary.clone_from(&source.boundary);
        } else {
            let b = document.job_bounds();
            boundary.push(geom::rect(b.min, b.max)?);
        }
    }
    if params.tile.is_none() {
        params.tile = editing
            .as_ref()
            .and_then(|s| s.params.tile)
            .or_else(|| model::texture_area::seed_box(&seed));
    }
    let grid = model::grid_for(document.job());
    let full = model::texture_area::texture_area(&seed, &boundary, &grid, &params, tol)?;
    let source = model::texture_area::TextureSource {
        seed: args.component,
        boundary,
        params,
    };
    let tile = params.tile;
    apply_component(
        document,
        &Made {
            label: "Create Texture Area",
            base: "Texture",
            combine: args.combine,
            name: args.name,
            apply: args.apply,
            hide: Some(args.component),
            reply: json!({ "tile": tile }),
        },
        full,
        |extra| extra.texture = Some(Box::new(source.clone())),
    )
}

/// Makes or updates a Create Shape component.
fn create_shape(document: &mut Document, args: CreateShapeArgs) -> Result<KernelReply, CoreError> {
    let tol = Tolerance::new(0.01)?;
    let mut closed = Vec::new();
    let mut open = Vec::new();
    for id in &args.ids {
        document.entity(*id)?;
        for (_, path) in document.paths_of(*id) {
            if path.subpaths.iter().all(|s| s.closed) {
                closed.push(path);
            } else {
                open.push((*id, path));
            }
        }
    }
    let mut params = args.params;
    let custom = if params.profile == model::shape::Profile::Custom {
        let [(id, path)] = open.as_slice() else {
            return Err(CoreError::validation(
                "select the closed vectors and one open vector for the profile",
            ));
        };
        let sub = path
            .subpaths
            .first()
            .ok_or_else(|| CoreError::validation("the profile vector is empty"))?;
        let points = geom::flatten_subpath(sub, tol).points;
        Some((*id, model::shape::ProfileCurve::from_points(&points)?))
    } else {
        None
    };
    let grid = model::grid_for(document.job());
    let region = if closed.is_empty() {
        if params.profile != model::shape::Profile::Flat {
            return Err(CoreError::validation(
                "select the closed vectors to build the shape from",
            ));
        }
        // A plane with no vectors covers the whole material.
        let b = document.job_bounds();
        geom::Region::from_paths(&[geom::rect(b.min, b.max)?], tol)
    } else {
        geom::Region::from_paths(&closed, tol)
    };
    // The base height is the component's own, not baked into its heights.
    let base = args.base_height.unwrap_or(params.base_height);
    params.base_height = 0.0;
    if !base.is_finite() {
        return Err(CoreError::validation("the base height must be a number"));
    }
    if let Some(fade) = &args.fade {
        fade.check()?;
    }
    if let Some(tilt) = &args.tilt {
        tilt.check()?;
    }
    let mut source = model::shape::ShapeSource {
        params,
        vectors: args
            .ids
            .iter()
            .copied()
            .filter(|id| custom.as_ref().is_none_or(|(p, _)| p != id))
            .collect(),
        profile_vector: custom.as_ref().map(|(id, _)| *id),
        placement: None,
    };
    // Made again from the vectors it was made from, a shape that was moved
    // off them stays where it was put.
    let placement = args
        .apply
        .target
        .and_then(|id| document.component(id).ok())
        .and_then(|c| c.extra.shape.as_deref())
        .filter(|old| same_ids(&old.all_vectors(), &source.all_vectors()))
        .and_then(|old| old.placement);
    source.placement = placement;
    let profile = custom.as_ref().map(|(_, c)| c);
    let full = match placement {
        None => model::shape::create_shape_with(&region, &grid, &params, profile, tol)?,
        // A move, turn or mirror builds from the vectors where the component
        // is, so nothing is resampled.
        Some(m) if model::shape::is_rigid(&m) && !closed.is_empty() => {
            let moved: Vec<geom::Path> = closed
                .iter()
                .map(|path| geom::transform_path(path, &m))
                .collect();
            let region = geom::Region::from_paths(&moved, tol);
            model::shape::create_shape_with(&region, &grid, &params, profile, tol)?
        }
        Some(m) => {
            let built = model::shape::create_shape_with(&region, &grid, &params, profile, tol)?;
            model::transform::transform_affine(&built, &m)?
        }
    };
    let (fade, tilt) = (args.fade, args.tilt);
    apply_component(
        document,
        &Made {
            label: CREATE_SHAPE,
            base: "Shape",
            combine: args.combine,
            name: args.name,
            apply: args.apply,
            hide: None,
            reply: Value::Null,
        },
        full,
        |extra| {
            extra.base_height = base;
            extra.fade = fade;
            extra.tilt = tilt;
            extra.shape = Some(Box::new(source.clone()));
        },
    )
}

/// True when both lists hold the same ids, in any order.
fn same_ids(a: &[u64], b: &[u64]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateShapeArgs {
    #[serde(default)]
    ids: Vec<u64>,
    #[serde(default)]
    params: model::shape::ShapeParams,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
    /// The component's base height (mm); the params' own when absent.
    #[serde(default)]
    base_height: Option<f64>,
    #[serde(default)]
    fade: Option<model::dynamic::Fade>,
    #[serde(default)]
    tilt: Option<model::dynamic::Tilt>,
    #[serde(flatten)]
    apply: ApplyArgs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportMeshArgs {
    path: PathBuf,
    #[serde(default)]
    params: model::mesh::MeshParams,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SweepArgs {
    rails: [u64; 2],
    sections: Vec<u64>,
    #[serde(default)]
    params: model::sweep::SweepParams,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
    #[serde(flatten)]
    apply: ApplyArgs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtrudeArgs {
    rails: Vec<u64>,
    #[serde(default)]
    sections: Vec<u64>,
    /// Extrude this component instead of the sections.
    #[serde(default)]
    component: Option<u64>,
    #[serde(default)]
    params: model::extrude::ExtrudeParams,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
    #[serde(flatten)]
    apply: ApplyArgs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureAreaArgs {
    /// The seed component, tiled across the area.
    component: u64,
    /// Closed vectors bounding the area; none fills the job.
    #[serde(default)]
    ids: Vec<u64>,
    #[serde(default)]
    params: model::texture_area::TextureParams,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
    #[serde(flatten)]
    apply: ApplyArgs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevolveArgs {
    id: u64,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    combine: model::Combine,
    #[serde(default)]
    name: Option<String>,
    #[serde(flatten)]
    apply: ApplyArgs,
}
