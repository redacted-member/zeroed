//! Create Vector Texture: fills the selected closed vectors, or the job
//! when none are selected, with rows of textured lines.
//!
//! Command:
//! - `vectors.texture {ids?, params, preview?, layerName?}` makes the
//!   texture. With `preview` it answers `{paths}` and changes nothing;
//!   otherwise the vectors are added, selected, on the active layer or on the
//!   layer named `layerName` (made when missing), as one undo step.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::entity::{Entity, EntityKind};
use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::vector_texture::{vector_texture, TextureParams};
use crate::geom::{flatten_subpath, path_is_closed, Path, Point, Tolerance};
use crate::kernel::KernelReply;

/// Distance boundaries are flattened to and curves are fitted to, in mm.
const TEXTURE_TOLERANCE_MM: f64 = 0.02;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "vectors.texture"
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default)]
    params: TextureParams,
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    layer_name: Option<String>,
}

/// Runs the Vector Texture command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for another name, [`CoreError::InvalidArgs`]
/// for arguments that do not parse and [`CoreError::Validation`] for bad
/// settings or a texture with nothing in it.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let trimmed = args.trim();
    let args: TextureArgs = if trimmed.is_empty() {
        TextureArgs::default()
    } else {
        serde_json::from_str(trimmed).map_err(|error| CoreError::invalid_args(name, error))?
    };
    let ids = document.target_ids(args.ids.as_deref());
    let paths = texture_paths(document, &ids, &args.params)?;
    if args.preview {
        return KernelReply::ok(&json!({ "paths": paths }));
    }
    if paths.is_empty() {
        return Err(CoreError::validation(
            "the texture has no vectors; make the spacing smaller than the boundary",
        ));
    }
    let layer_name = args
        .layer_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    in_step(document, "Vector Texture", |doc| {
        let layer = match layer_name {
            Some(name) => match doc.layers().iter().find(|l| l.name == name) {
                Some(existing) => existing.id,
                None => doc.add_layer(Some(name), None),
            },
            None => doc.active_layer(),
        };
        let sheet = doc.active_sheet();
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
        doc.set_selection(&ids);
        Ok(json!({ "ids": ids }))
    })
}

/// The texture for the closed vectors among `ids`, or for the job's
/// rectangle when there are none.
///
/// # Errors
/// The errors of [`vector_texture`].
pub fn texture_paths(
    document: &Document,
    ids: &[u64],
    params: &TextureParams,
) -> Result<Vec<Path>, CoreError> {
    let tol = Tolerance::new(TEXTURE_TOLERANCE_MM)?;
    let mut rings: Vec<Vec<Point>> = Vec::new();
    for id in ids {
        for (_, path) in document.paths_of(*id) {
            if !path_is_closed(&path) {
                continue;
            }
            for sub in &path.subpaths {
                let flat = flatten_subpath(sub, tol);
                if flat.points.len() >= 3 {
                    rings.push(flat.points);
                }
            }
        }
    }
    if rings.is_empty() {
        let job = document.job_bounds();
        rings.push(vec![
            job.min,
            Point::new(job.max.x, job.min.y),
            job.max,
            Point::new(job.min.x, job.max.y),
        ]);
    }
    vector_texture(&rings, params, tol)
}

/// Runs `action` as one undo step called `label`, rolling back on failure.
fn in_step<F>(document: &mut Document, label: &str, action: F) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(mut value) => {
            let revision = document.commit();
            value["revision"] = json!(revision);
            KernelReply::ok(&value)
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, path_bbox};

    fn with_circle() -> (Document, u64) {
        let mut doc = Document::new();
        doc.begin("Draw");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(50.0, 50.0), 20.0).expect("circle"),
            },
        });
        doc.commit();
        (doc, id)
    }

    #[test]
    fn the_texture_fills_the_selected_boundary() {
        let (mut doc, id) = with_circle();
        doc.set_selection(&[id]);
        let args = json!({ "params": { "maxSpacing": 4.0 } }).to_string();
        dispatch(&mut doc, "vectors.texture", &args).expect("texture");
        let made = doc.selection().to_vec();
        assert_eq!(made.len(), 10);
        for id in made {
            let path = doc.entity(id).expect("entity").path().expect("path");
            let bbox = path_bbox(&path).expect("bbox");
            assert!(bbox.min.x >= 29.9 && bbox.max.x <= 70.1);
        }
        assert_eq!(doc.undo().as_deref(), Some("Vector Texture"));
    }

    #[test]
    fn a_preview_changes_nothing() {
        let (mut doc, id) = with_circle();
        let before = doc.revision();
        let args = json!({ "ids": [id], "preview": true }).to_string();
        dispatch(&mut doc, "vectors.texture", &args).expect("preview");
        assert_eq!(doc.revision(), before);
    }

    #[test]
    fn without_a_boundary_the_job_is_filled_and_a_layer_can_be_named() {
        let mut doc = Document::new();
        let args = json!({ "params": { "maxSpacing": 50.0 }, "layerName": "Texture" }).to_string();
        dispatch(&mut doc, "vectors.texture", &args).expect("texture");
        let layer = doc
            .layers()
            .iter()
            .find(|l| l.name == "Texture")
            .expect("layer")
            .id;
        let first = doc.selection()[0];
        assert_eq!(doc.entity(first).expect("entity").layer, layer);
    }
}
