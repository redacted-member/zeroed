//! Kernel commands for the model tools that cut the relief up or read it
//! back as vectors: `model.crossSection`, `model.slicePreview`,
//! `model.slice` and `model.split`. Each change is one undo step.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Region, SubPath, Tolerance};
use crate::kernel::KernelReply;

use super::contour;
use super::cross_section::{profile, section_path};
use super::field::Field;
use super::slice::{plan, plan_count, plan_from_top, plan_heights, slab_of, Slab};
use super::split::{inside_mask, open_cut, split, SplitMode};
use super::{bake, grid_for, Combine, Component};

/// Gap between the top of the job and a cross section placed by default,
/// in millimetres.
pub const SECTION_GAP: f64 = 10.0;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "model.crossSection" | "model.slicePreview" | "model.slice" | "model.split"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] when they are not legal values or the model has
/// nothing to work on.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "model.crossSection" => cross_section(document, parse(name, args)?),
        "model.slicePreview" => {
            let args: SliceArgs = parse(name, args)?;
            let (field, _) = slice_source(document, args.component_ids.as_deref())?;
            let max = field.max().map_or(0.0, f64::from);
            let slabs = args.slabs(max)?;
            KernelReply::ok(&json!({ "maxHeight": max, "slabs": slabs }))
        }
        "model.slice" => slice(document, parse(name, args)?),
        "model.split" => split_component(document, parse(name, args)?),
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

fn tolerance() -> Result<Tolerance, CoreError> {
    Tolerance::new(0.01)
}

// ------------------------------------------------------------ cross section

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrossSectionArgs {
    from: Point,
    to: Point,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default)]
    origin: Option<Point>,
}

fn one() -> f64 {
    1.0
}

fn cross_section(
    document: &mut Document,
    args: CrossSectionArgs,
) -> Result<KernelReply, CoreError> {
    let composite = document.composite();
    let points = profile(&composite, args.from, args.to)?;
    let origin = args.origin.unwrap_or_else(|| {
        let job = document.job_bounds();
        Point::new(job.min.x, job.max.y + SECTION_GAP)
    });
    let path = section_path(&points, origin, args.scale)?;
    let length = args.from.distance(args.to);
    let max_height = points.iter().map(|p| p.y).fold(0.0, f64::max);
    let (layer, sheet) = (document.active_layer(), document.active_sheet());
    edit(document, "Create Cross Section", |doc| {
        let id = insert_path(doc, layer, sheet, path);
        Ok(json!({ "id": id, "length": length, "maxHeight": max_height }))
    })
}

fn insert_path(doc: &mut Document, layer: u64, sheet: u64, path: Path) -> u64 {
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
}

// -------------------------------------------------------------------- slice

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SliceArgs {
    #[serde(default)]
    thickness: Option<f64>,
    #[serde(default)]
    thicknesses: Option<Vec<f64>>,
    /// Components to slice; none or empty for the whole visible model.
    #[serde(default)]
    component_ids: Option<Vec<u64>>,
    /// Name the slices are numbered after, "Slice" when absent.
    #[serde(default)]
    name: Option<String>,
    /// Equal slices, this many, in place of the thicknesses.
    #[serde(default)]
    count: Option<usize>,
    /// Takes the thicknesses down from the model's top, leaving the part
    /// slice at the bottom, instead of up from the zero plane.
    #[serde(default)]
    from_top: bool,
    /// Outlines each slice with closed vectors on a layer of its own.
    #[serde(default)]
    boundary_vectors: bool,
    /// The slice heights (Z values to cut at) as the form's Slice Heights
    /// list holds them, in place of the thicknesses and the count.
    #[serde(default)]
    heights: Option<Vec<f64>>,
}

impl SliceArgs {
    fn thickness_list(&self) -> Result<Vec<f64>, CoreError> {
        match (&self.thicknesses, self.thickness) {
            (Some(list), _) if !list.is_empty() => Ok(list.clone()),
            (_, Some(t)) => Ok(vec![t]),
            _ => Err(CoreError::validation("give a slice thickness")),
        }
    }

    /// The slabs for a model `max` millimetres high.
    fn slabs(&self, max: f64) -> Result<Vec<Slab>, CoreError> {
        if let Some(heights) = &self.heights {
            return plan_heights(max, heights);
        }
        match self.count {
            Some(count) => plan_count(max, count),
            None if self.from_top => plan_from_top(max, &self.thickness_list()?),
            None => plan(max, &self.thickness_list()?),
        }
    }
}

/// The heights to slice on the model grid, and the components they came
/// from.
fn slice_source(document: &Document, ids: Option<&[u64]>) -> Result<(Field, Vec<u64>), CoreError> {
    let chosen: Vec<Component> = match ids {
        None | Some([]) => document
            .components()
            .into_iter()
            .filter(|c| c.visible)
            .cloned()
            .collect(),
        Some(ids) => ids
            .iter()
            .map(|id| {
                document.component(*id).map(|c| Component {
                    visible: true,
                    ..c.clone()
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    if chosen.is_empty() {
        return Err(CoreError::validation("there is no model to slice"));
    }
    let sources: Vec<u64> = chosen.iter().map(|c| c.id).collect();
    let field = match ids {
        None | Some([]) => document.composite(),
        Some(_) => {
            // Chosen components bake in model order.
            let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
            let mut sorted = chosen;
            sorted.sort_by_key(|c| order.iter().position(|id| *id == c.id));
            bake(&sorted, &grid_for(document.job()))
        }
    };
    Ok((field, sources))
}

fn slice(document: &mut Document, args: SliceArgs) -> Result<KernelReply, CoreError> {
    let (field, sources) = slice_source(document, args.component_ids.as_deref())?;
    let max = field.max().map_or(0.0, f64::from);
    let slabs = args.slabs(max)?;
    let base = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Slice")
        .to_string();
    let parts: Vec<(Slab, Field)> = slabs
        .iter()
        .filter_map(|slab| slab_of(&field, slab).map(|f| (*slab, f)))
        .collect();
    if parts.is_empty() {
        return Err(CoreError::validation(
            "there is no model above the zero plane to slice",
        ));
    }
    let tol = tolerance()?;
    let outlines: Vec<Vec<Path>> = if args.boundary_vectors {
        parts
            .iter()
            .map(|(_, field)| contour::boundary(field, f32::NEG_INFINITY, 0.0, tol))
            .collect::<Result<_, _>>()?
    } else {
        Vec::new()
    };
    let sheet = document.active_sheet();
    edit(document, "Slice Model", |doc| {
        // The sources stay in the list, hidden, so the slices (which add
        // back up to the model) are what shows.
        for id in &sources {
            doc.update_component(*id, |c| c.visible = false)?;
        }
        let mut ids = Vec::with_capacity(parts.len());
        let mut vectors = Vec::new();
        let mut layers = Vec::new();
        for (k, (slab, field)) in parts.into_iter().enumerate() {
            let name = format!("{base} {}", slab.index);
            // Each slice's outline goes on a layer named after the slice,
            // so one slice's vectors can be shown and machined alone.
            if let Some(paths) = outlines.get(k).filter(|p| !p.is_empty()) {
                let layer = doc.add_layer(Some(name.clone()), None);
                layers.push(layer);
                for path in paths {
                    vectors.push(insert_path(doc, layer, sheet, path.clone()));
                }
            }
            ids.push(doc.add_component(Component {
                id: 0,
                name,
                visible: true,
                combine: Combine::Add,
                height_scale: 1.0,
                level: 0,
                extra: Default::default(),
                field,
            }));
        }
        Ok(json!({
            "ids": ids,
            "hidden": sources,
            "slabs": slabs,
            "vectors": vectors,
            "layers": layers,
        }))
    })
}

// -------------------------------------------------------------------- split

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SplitArgs {
    component_id: u64,
    vector_ids: Vec<u64>,
    #[serde(default)]
    mode: SplitMode,
}

fn split_component(document: &mut Document, args: SplitArgs) -> Result<KernelReply, CoreError> {
    if args.vector_ids.is_empty() {
        return Err(CoreError::validation(
            "select the vectors to split the component with",
        ));
    }
    let component = document.component(args.component_id)?.clone();
    let mut paths = Vec::new();
    for id in &args.vector_ids {
        document.entity(*id)?;
        paths.extend(document.paths_of(*id).into_iter().map(|(_, p)| p));
    }
    let tol = tolerance()?;
    let open: Vec<&SubPath> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter(|s| !s.closed && !s.segments.is_empty())
        .collect();
    let region = match open.as_slice() {
        [] => Region::from_paths(&paths, tol),
        // An open vector is carried on to the edges of the model.
        [one] if paths.iter().all(|p| p.subpaths.iter().all(|s| !s.closed)) => {
            let points = flatten_subpath(one, tol).points;
            let cut = open_cut(&component.field, &points).ok_or_else(|| {
                CoreError::validation("the open vector has no length to split along")
            })?;
            Region::from_paths(&[cut], tol)
        }
        _ => {
            return Err(CoreError::validation(
                "split with closed vectors or with one open vector, not both",
            ))
        }
    };
    if region.is_empty() {
        return Err(CoreError::validation(
            "select closed vectors or one open vector to split the component with",
        ));
    }
    let mask = inside_mask(&component.field, &region, tol);
    let (inside, outside) = split(&component.field, &mask);
    let (inside, outside) = match (inside, outside) {
        (Some(i), Some(o)) => (i, o),
        (None, _) => {
            return Err(CoreError::validation(
                "the vectors do not cover any of the component",
            ))
        }
        (_, None) => {
            return Err(CoreError::validation(
                "the vectors cover the whole component, so there is nothing to split off",
            ))
        }
    };
    let id = component.id;
    match args.mode {
        SplitMode::ClearInside => edit(document, "Clear Component", |doc| {
            doc.update_component(id, |c| c.field = outside)?;
            Ok(json!({ "id": id }))
        }),
        SplitMode::ClearOutside => edit(document, "Clear Component", |doc| {
            doc.update_component(id, |c| c.field = inside)?;
            Ok(json!({ "id": id }))
        }),
        SplitMode::Split => {
            let order: Vec<u64> = document.components().iter().map(|c| c.id).collect();
            // The two parts are named "<name> - A" and
            // "<name> - B": the outside keeps the component (A), the
            // inside is the new one (B).
            edit(document, "Split Component", |doc| {
                doc.update_component(id, |c| {
                    c.field = outside;
                    c.name = format!("{} - A", component.name);
                })?;
                let added = doc.add_component(Component {
                    id: 0,
                    name: format!("{} - B", component.name),
                    field: inside,
                    ..component.clone()
                });
                // The new part sits just above the one it came from.
                let mut new_order = Vec::with_capacity(order.len() + 1);
                for c in &order {
                    new_order.push(*c);
                    if *c == id {
                        new_order.push(added);
                    }
                }
                doc.reorder_components(&new_order)?;
                Ok(json!({ "id": id, "insideId": added }))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rect;

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
        result(dispatch(doc, name, &args.to_string()).unwrap_or_else(|e| panic!("{name}: {e}")))
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

    fn add_rect(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
        let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
        let (layer, sheet) = (doc.active_layer(), doc.active_sheet());
        doc.begin("Rect");
        let id = insert_path(doc, layer, sheet, path);
        doc.commit();
        id
    }

    #[test]
    fn handles_only_its_own_commands() {
        assert!(handles("model.slice"));
        assert!(!handles("model.list"));
        let mut doc = Document::new();
        assert!(dispatch(&mut doc, "model.list", "").is_err());
    }

    #[test]
    fn the_kernel_routes_the_commands_here() {
        let mut kernel = crate::Kernel::with_cam(crate::cam::commands::CamState::default());
        let reply = kernel
            .apply(
                "model.slicePreview",
                &json!({ "thickness": 1.0 }).to_string(),
            )
            .expect_err("no model yet");
        assert!(reply.to_string().contains("no model"), "{reply}");
    }

    #[test]
    fn a_cross_section_draws_the_profile_above_the_job() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 8.0);
        let before = doc.revision();
        let reply = run(
            &mut doc,
            "model.crossSection",
            json!({ "from": {"x": 50.0, "y": 150.0}, "to": {"x": 250.0, "y": 150.0}, "scale": 2.0 }),
        );
        assert!(reply["revision"].as_u64() > Some(before));
        assert!((reply["maxHeight"].as_f64().unwrap_or(0.0) - 8.0).abs() < 1e-6);
        let id = reply["id"].as_u64().expect("id");
        let path = doc.entity(id).expect("entity").path().expect("path");
        let job = doc.job_bounds();
        let bbox = path.subpaths[0].nodes();
        let top = bbox.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        let bottom = bbox.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        assert!((bottom - (job.max.y + SECTION_GAP)).abs() < 1e-6);
        assert!((top - bottom - 16.0).abs() < 1e-3, "scaled by 2");
        doc.undo();
        assert!(doc.entity(id).is_err(), "one undo step");
    }

    #[test]
    fn slicing_makes_stacked_components_that_add_back_up() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 10.0);
        let preview = run(&mut doc, "model.slicePreview", json!({ "thickness": 4.0 }));
        assert_eq!(preview["slabs"].as_array().map(Vec::len), Some(3));
        let before = doc.composite();
        let reply = run(&mut doc, "model.slice", json!({ "thickness": 4.0 }));
        let ids: Vec<u64> = serde_json::from_value(reply["ids"].clone()).expect("ids");
        assert_eq!(ids.len(), 3);
        assert!(!doc.component(block).expect("kept").visible);
        let names: Vec<String> = ids
            .iter()
            .map(|id| doc.component(*id).expect("slice").name.clone())
            .collect();
        assert_eq!(names, ["Slice 1", "Slice 2", "Slice 3"]);
        let top = doc.component(ids[2]).expect("top");
        assert_eq!(top.field.max(), Some(2.0));
        assert_eq!(before, doc.composite(), "the slices add back to the model");
        doc.undo();
        assert_eq!(doc.components().len(), 1);
        assert!(doc.component(block).expect("back").visible);
    }

    #[test]
    fn variable_thickness_slices_follow_the_list() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 10.0);
        let reply = run(
            &mut doc,
            "model.slice",
            json!({ "thicknesses": [2.0, 5.0], "name": "Layer" }),
        );
        let ids: Vec<u64> = serde_json::from_value(reply["ids"].clone()).expect("ids");
        let tops: Vec<Option<f32>> = ids
            .iter()
            .map(|id| doc.component(*id).expect("slice").field.max())
            .collect();
        assert_eq!(tops, vec![Some(2.0), Some(5.0), Some(3.0)]);
        assert_eq!(doc.component(ids[0]).expect("first").name, "Layer 1");
    }

    #[test]
    fn slices_at_the_heights_of_the_slice_heights_list() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 10.0);
        // The heights win over the thickness and the count.
        let preview = run(
            &mut doc,
            "model.slicePreview",
            json!({ "thicknesses": [4.0], "count": 2, "heights": [7.0, 3.0] }),
        );
        let tops: Vec<f64> = preview["slabs"]
            .as_array()
            .expect("slabs")
            .iter()
            .filter_map(|s| s["top"].as_f64())
            .collect();
        assert_eq!(tops, vec![3.0, 7.0, 10.0]);
        let reply = run(
            &mut doc,
            "model.slice",
            json!({ "thicknesses": [], "heights": [7.0, 3.0] }),
        );
        let ids: Vec<u64> = serde_json::from_value(reply["ids"].clone()).expect("ids");
        let tops: Vec<Option<f32>> = ids
            .iter()
            .map(|id| doc.component(*id).expect("slice").field.max())
            .collect();
        assert_eq!(tops, vec![Some(3.0), Some(4.0), Some(3.0)]);
    }

    #[test]
    fn slices_from_the_top_or_by_number_with_boundary_vectors() {
        let mut doc = Document::new();
        add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 10.0);
        let preview = run(
            &mut doc,
            "model.slicePreview",
            json!({ "thicknesses": [4.0], "fromTop": true }),
        );
        let bottoms: Vec<f64> = preview["slabs"]
            .as_array()
            .expect("slabs")
            .iter()
            .filter_map(|s| s["bottom"].as_f64())
            .collect();
        assert_eq!(bottoms, vec![0.0, 2.0, 6.0]);
        let preview = run(
            &mut doc,
            "model.slicePreview",
            json!({ "thicknesses": [1.0], "count": 4 }),
        );
        assert_eq!(preview["slabs"].as_array().map(Vec::len), Some(4));
        let layers_before = doc.layers().len();
        let reply = run(
            &mut doc,
            "model.slice",
            json!({ "thicknesses": [4.0], "fromTop": true, "boundaryVectors": true }),
        );
        let ids: Vec<u64> = serde_json::from_value(reply["ids"].clone()).expect("ids");
        let tops: Vec<Option<f32>> = ids
            .iter()
            .map(|id| doc.component(*id).expect("slice").field.max())
            .collect();
        assert_eq!(tops, vec![Some(2.0), Some(4.0), Some(4.0)]);
        let layers: Vec<u64> = serde_json::from_value(reply["layers"].clone()).expect("layers");
        assert_eq!(layers.len(), 3);
        assert_eq!(doc.layers().len(), layers_before + 3);
        assert_eq!(doc.layer(layers[0]).expect("layer").name, "Slice 1");
        let vectors: Vec<u64> = serde_json::from_value(reply["vectors"].clone()).expect("vectors");
        assert_eq!(vectors.len(), 3, "one outline round each block slice");
        let outline = doc.entity(vectors[0]).expect("outline");
        assert_eq!(outline.layer, layers[0]);
        assert!(outline.path().expect("path").subpaths[0].closed);
        doc.undo();
        assert_eq!(doc.layers().len(), layers_before);
        assert!(doc.entity(vectors[0]).is_err(), "one undo step");
    }

    #[test]
    fn slicing_chosen_components_leaves_the_others() {
        let mut doc = Document::new();
        let a = add_block(&mut doc, 20.0, 20.0, 70.0, 70.0, 6.0);
        let b = add_block(&mut doc, 200.0, 20.0, 250.0, 70.0, 6.0);
        let reply = run(
            &mut doc,
            "model.slice",
            json!({ "thickness": 3.0, "componentIds": [a] }),
        );
        assert_eq!(reply["ids"].as_array().map(Vec::len), Some(2));
        assert!(!doc.component(a).expect("a").visible);
        assert!(doc.component(b).expect("b").visible);
        assert!(dispatch(
            &mut doc,
            "model.slice",
            &json!({ "thickness": 0.0 }).to_string()
        )
        .is_err());
        assert!(dispatch(&mut doc, "model.slice", "{}").is_err());
    }

    #[test]
    fn split_divides_a_component_in_two() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let cut = add_rect(&mut doc, 90.0, 90.0, 150.0, 210.0);
        let before = doc.composite();
        let reply = run(
            &mut doc,
            "model.split",
            json!({ "componentId": block, "vectorIds": [cut] }),
        );
        let inside = reply["insideId"].as_u64().expect("inside id");
        let order: Vec<u64> = doc.components().iter().map(|c| c.id).collect();
        assert_eq!(order, vec![block, inside]);
        assert_eq!(doc.component(inside).expect("inside").name, "Block - B");
        assert_eq!(doc.component(block).expect("outside").name, "Block - A");
        assert!(doc.component(inside).expect("inside").field.origin.x < 150.0);
        assert!(doc.component(block).expect("outside").field.origin.x > 150.0);
        assert_eq!(before, doc.composite(), "the two parts make the same model");
        // The outside part no longer reaches into the vectors.
        let again = json!({ "componentId": block, "vectorIds": [cut], "mode": "clearInside" });
        let error = dispatch(&mut doc, "model.split", &again.to_string()).expect_err("misses");
        assert!(error.to_string().contains("do not cover"), "{error}");
        doc.undo();
        assert_eq!(doc.components().len(), 1);
    }

    #[test]
    fn an_open_vector_splits_the_component_to_its_edges() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        // A short line up the middle, inside the block: carried on to the
        // edges, the left half is B.
        let mut sub = SubPath::open(Point::new(150.0, 140.0));
        sub.line_to(Point::new(150.0, 160.0));
        let (layer, sheet) = (doc.active_layer(), doc.active_sheet());
        doc.begin("Line");
        let line = insert_path(&mut doc, layer, sheet, sub.into_path());
        doc.commit();
        let before = doc.composite();
        let reply = run(
            &mut doc,
            "model.split",
            json!({ "componentId": block, "vectorIds": [line] }),
        );
        let inside = reply["insideId"].as_u64().expect("inside id");
        let left = doc.component(inside).expect("left");
        let right = doc.component(block).expect("right");
        assert_eq!(left.name, "Block - B");
        assert!(left.field.origin.x < 150.0);
        assert!(right.field.origin.x > 150.0);
        let top = |f: &Field| f.origin.y + f.cell * (f.height as f64 - 1.0);
        assert!(
            top(&left.field) > 195.0,
            "the split runs past the line's ends"
        );
        assert_eq!(before, doc.composite());
        // An open vector with a closed one is refused.
        let square = add_rect(&mut doc, 120.0, 120.0, 130.0, 130.0);
        let both = json!({ "componentId": block, "vectorIds": [line, square] });
        assert!(dispatch(&mut doc, "model.split", &both.to_string()).is_err());
    }

    #[test]
    fn clear_inside_only_removes_the_inside() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let cut = add_rect(&mut doc, 90.0, 90.0, 150.0, 210.0);
        run(
            &mut doc,
            "model.split",
            json!({ "componentId": block, "vectorIds": [cut], "mode": "clearInside" }),
        );
        assert_eq!(doc.components().len(), 1);
        let composite = doc.composite();
        assert_eq!(composite.at(Point::new(120.0, 150.0)), 0.0);
        assert_eq!(composite.at(Point::new(180.0, 150.0)), 5.0);
    }

    #[test]
    fn split_refuses_vectors_that_miss_or_swallow_the_component() {
        let mut doc = Document::new();
        let block = add_block(&mut doc, 100.0, 100.0, 200.0, 200.0, 5.0);
        let far = add_rect(&mut doc, 300.0, 300.0, 350.0, 350.0);
        let all = add_rect(&mut doc, 50.0, 50.0, 250.0, 250.0);
        for ids in [vec![far], vec![all], vec![]] {
            let args = json!({ "componentId": block, "vectorIds": ids });
            assert!(dispatch(&mut doc, "model.split", &args.to_string()).is_err());
        }
        assert_eq!(doc.components().len(), 1);
    }
}
