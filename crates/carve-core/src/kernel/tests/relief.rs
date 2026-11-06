//! Relief component commands through the kernel.

use super::*;

#[test]
fn a_shape_becomes_a_component_that_bakes_into_the_composite() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let reply = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    );
    let component = reply["id"].as_u64().expect("an id");
    let reply = run(&mut kernel, "model.list", "");
    let list = &reply["components"];
    assert_eq!(list[0]["name"], "Shape 1");
    assert!((list[0]["maxHeight"].as_f64().unwrap_or(0.0) - 3.0).abs() < 1e-6);
    let stamp = reply["stamp"].as_u64().expect("stamp");
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [id] }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "model.list", "")["stamp"].as_u64(),
        Some(stamp)
    );
    let composite = kernel.document().composite();
    assert_eq!(composite.at(Point::new(120.0, 100.0)), 3.0);
    assert_eq!(composite.at(Point::new(10.0, 10.0)), 0.0);

    run(
        &mut kernel,
        "model.update",
        &json!({ "id": component, "heightScale": 2.0, "combine": "subtract" }).to_string(),
    );
    assert_eq!(
        kernel.document().composite().at(Point::new(120.0, 100.0)),
        -6.0
    );
    match kernel.apply("model.composite", "").expect("composite") {
        KernelReply::Bytes(bytes) => {
            let grid = model::grid_for(kernel.document().job());
            assert!(bytes.len() > grid.width * grid.height * 2);
        }
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    }
    run(&mut kernel, "undo", "");
    assert_eq!(
        kernel.document().composite().at(Point::new(120.0, 100.0)),
        3.0
    );
    run(
        &mut kernel,
        "model.delete",
        &json!({ "ids": [component] }).to_string(),
    );
    assert!(run(&mut kernel, "model.list", "")["components"]
        .as_array()
        .is_some_and(Vec::is_empty));
    let error = kernel
        .apply("model.createShape", &json!({ "ids": [] }).to_string())
        .expect_err("nothing selected");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn an_stl_file_becomes_a_component() {
    let temp = crate::test_support::TempDir::new("kernel-stl");
    let path = temp.path().join("pyramid.stl");
    let text = "solid p\n\
         facet normal 0 0 1\n outer loop\n vertex 0 0 0\n vertex 10 0 0\n vertex 5 5 4\n endloop\n endfacet\n\
         facet normal 0 0 1\n outer loop\n vertex 10 0 0\n vertex 10 10 0\n vertex 5 5 4\n endloop\n endfacet\n\
         facet normal 0 0 1\n outer loop\n vertex 10 10 0\n vertex 0 10 0\n vertex 5 5 4\n endloop\n endfacet\n\
         facet normal 0 0 1\n outer loop\n vertex 0 10 0\n vertex 0 0 0\n vertex 5 5 4\n endloop\n endfacet\n\
         endsolid p\n";
    std::fs::write(&path, text).expect("writes");
    let mut kernel = Kernel::new();
    let reply = run(
        &mut kernel,
        "model.importMesh",
        &json!({ "path": path, "params": { "width": 40.0 } }).to_string(),
    );
    assert_eq!(reply["triangles"], 4);
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["components"][0]["name"], "pyramid");
    let peak = list["components"][0]["maxHeight"].as_f64().unwrap_or(0.0);
    assert!(
        (peak - 16.0).abs() < 0.5,
        "a 4 high pyramid scaled by 4: {peak}"
    );
    let centre = kernel.document().job_bounds().centre();
    assert!(kernel.document().composite().at(centre) > 15.0);
    assert!(kernel
        .apply(
            "model.importMesh",
            &json!({ "path": temp.path().join("none.stl") }).to_string()
        )
        .is_err());
}

#[test]
fn extrusions_and_texture_areas_make_components() {
    let mut kernel = Kernel::new();
    let rail = line_entity(&mut kernel, (50.0, 100.0), (250.0, 100.0));
    let arc = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
        "start": { "x": 0.0, "y": 0.0 },
        "segments": [{ "kind": "arc", "to": { "x": 20.0, "y": 0.0 }, "bulge": -1.0 }],
        "closed": false }] } } });
    let section = run(&mut kernel, "entities.create", &arc.to_string())["id"]
        .as_u64()
        .expect("an id");
    let reply = run(
        &mut kernel,
        "model.extrude",
        &json!({ "rails": [rail], "sections": [section],
                 "params": { "stations": [{ "rail": 0, "section": 0, "at": 0.0 },
                                          { "rail": 0, "section": 0, "at": 0.5 }] } })
        .to_string(),
    );
    assert!(reply["id"].as_u64().is_some());
    let crest = kernel.document().composite().at(Point::new(100.0, 100.0));
    assert!((f64::from(crest) - 10.0).abs() < 0.5, "crest {crest}");
    let beyond = kernel.document().composite().at(Point::new(200.0, 100.0));
    assert!(beyond.abs() < 1e-6, "ends at the last section: {beyond}");

    // The extrusion carried along a second rail as a component, stretched.
    let second = line_entity(&mut kernel, (50.0, 160.0), (250.0, 160.0));
    let first = run(&mut kernel, "model.list", "")["components"][0]["id"]
        .as_u64()
        .expect("the extrusion");
    let reply = run(
        &mut kernel,
        "model.extrude",
        &json!({ "rails": [second], "component": first,
                 "params": { "fill": "stretch", "exactHeight": true, "height": 4.0 } })
        .to_string(),
    );
    let carried = reply["id"].as_u64().expect("a component");
    let top = kernel
        .document()
        .component(carried)
        .expect("carried")
        .field
        .max();
    assert!(top.is_some_and(|t| (t - 4.0).abs() < 1e-3), "{top:?}");
    run(&mut kernel, "undo", "");

    let square = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
        "start": { "x": 20.0, "y": 20.0 },
        "segments": [
            { "kind": "line", "to": { "x": 60.0, "y": 20.0 } },
            { "kind": "line", "to": { "x": 60.0, "y": 60.0 } },
            { "kind": "line", "to": { "x": 20.0, "y": 60.0 } }],
        "closed": true }] } } });
    let region = run(&mut kernel, "entities.create", &square.to_string())["id"]
        .as_u64()
        .expect("an id");
    let seed = run(&mut kernel, "model.list", "")["components"][0]["id"]
        .as_u64()
        .expect("the extrusion");
    let reply = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [region], "params": { "spacingX": 10.0 },
                 "name": "Pebbles" })
        .to_string(),
    );
    assert!(reply["id"].as_u64().is_some());
    assert!(
        reply["tile"]["width"].as_f64().unwrap_or(0.0) > 95.0,
        "{reply}"
    );
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["components"][1]["name"], "Pebbles");
    assert_eq!(
        list["components"][0]["visible"],
        Value::Bool(false),
        "the seed is hidden"
    );
    let peak = list["components"][1]["maxHeight"].as_f64().unwrap_or(0.0);
    assert!(peak > 0.0 && peak < 10.5, "{peak}");
    assert!(kernel
        .apply("model.textureArea", &json!({ "ids": [] }).to_string())
        .is_err());
    assert!(kernel
        .apply(
            "model.extrude",
            &json!({ "rails": [rail], "sections": [] }).to_string()
        )
        .is_err());
}

#[test]
fn sweeps_turns_and_edits_make_and_change_components() {
    let mut kernel = Kernel::new();
    let a = line_entity(&mut kernel, (50.0, 90.0), (250.0, 90.0));
    let b = line_entity(&mut kernel, (50.0, 110.0), (250.0, 110.0));
    let arc = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
        "start": { "x": 0.0, "y": 0.0 },
        "segments": [{ "kind": "arc", "to": { "x": 20.0, "y": 0.0 }, "bulge": -1.0 }],
        "closed": false }] } } });
    let section = run(&mut kernel, "entities.create", &arc.to_string())["id"]
        .as_u64()
        .expect("an id");
    let swept = run(
        &mut kernel,
        "model.sweep",
        &json!({ "rails": [a, b], "sections": [section] }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    let crest = kernel.document().composite().at(Point::new(150.0, 100.0));
    assert!(
        (f64::from(crest) - 10.0).abs() < 0.5,
        "half cylinder crest {crest}"
    );

    let profile = line_entity(&mut kernel, (100.0, 45.0), (120.0, 40.0));
    run(
        &mut kernel,
        "model.spin",
        &json!({ "id": profile, "params": {} }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    run(
        &mut kernel,
        "model.edit",
        &json!({ "id": swept, "op": "scale", "params": { "scale": 0.5 } }).to_string(),
    );
    let halved = kernel.document().composite().at(Point::new(150.0, 100.0));
    assert!(
        (f64::from(halved) - f64::from(crest) / 2.0).abs() < 0.05,
        "{halved}"
    );
    let undo = run(&mut kernel, "undo", "");
    assert_eq!(undo["label"], "Scale Component");
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": swept, "params": { "offset": { "x": 20.0, "y": 0.0 } } }).to_string(),
    );
    let moved = kernel.document().composite().at(Point::new(170.0, 100.0));
    assert!((f64::from(moved) - f64::from(crest)).abs() < 0.5, "{moved}");
    run(&mut kernel, "undo", "");
    run(
        &mut kernel,
        "model.sculpt",
        &json!({
            "id": swept,
            "points": [{ "x": 100.0, "y": 100.0 }, { "x": 120.0, "y": 100.0 }],
            "params": { "brush": "deposit", "diameter": 10.0, "strength": 1.0 },
        })
        .to_string(),
    );
    let raised = kernel.document().composite().at(Point::new(110.0, 100.0));
    assert!(
        raised > crest,
        "deposit raises the crest: {raised} > {crest}"
    );
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Sculpt");
    run(
        &mut kernel,
        "model.edit",
        &json!({ "id": swept, "op": "flip" }).to_string(),
    );
    let bbox = &run(&mut kernel, "model.list", "")["components"][0]["bbox"];
    assert!(bbox[0].as_f64().unwrap_or(0.0) < 60.0 && bbox[2].as_f64().unwrap_or(0.0) > 240.0);
    assert!(kernel
        .apply(
            "model.edit",
            &json!({ "id": swept, "op": "wobble" }).to_string()
        )
        .is_err());
}

#[test]
fn levels_bake_separately_and_combine_as_a_whole() {
    let mut kernel = Kernel::new();
    let base = make_rect(&mut kernel, 20.0, 20.0, 120.0, 80.0);
    let shape = |kernel: &mut Kernel, id: u64, height: f64| {
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": height } }).to_string(),
        )["id"]
            .as_u64()
            .expect("a component")
    };
    shape(&mut kernel, base, 10.0);
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["levels"].as_array().map(Vec::len), Some(1));
    let first = list["activeLevel"].as_u64().expect("active level");

    // A second level holding two overlapping 3 mm blocks, subtracted
    // from the base as one piece: the overlap is still 3 deep, not 6.
    let level = run(
        &mut kernel,
        "model.levels.add",
        &json!({ "combine": "subtract" }).to_string(),
    )["id"]
        .as_u64()
        .expect("a level");
    let a = make_rect(&mut kernel, 40.0, 40.0, 70.0, 60.0);
    let b = make_rect(&mut kernel, 60.0, 40.0, 90.0, 60.0);
    let pa = shape(&mut kernel, a, 3.0);
    let pb = shape(&mut kernel, b, 3.0);
    run(
        &mut kernel,
        "model.update",
        &json!({ "id": pb, "combine": "mergeHigh" }).to_string(),
    );
    let at =
        |kernel: &Kernel, x: f64| f64::from(kernel.document().composite().at(Point::new(x, 50.0)));
    assert!((at(&kernel, 30.0) - 10.0).abs() < 1e-3, "base untouched");
    assert!(
        (at(&kernel, 50.0) - 7.0).abs() < 1e-3,
        "{}",
        at(&kernel, 50.0)
    );
    assert!(
        (at(&kernel, 65.0) - 7.0).abs() < 1e-3,
        "overlap merged first: {}",
        at(&kernel, 65.0)
    );

    // Hiding the level brings the base back; moving a component to
    // the first level adds it there instead.
    run(
        &mut kernel,
        "model.levels.update",
        &json!({ "id": level, "visible": false }).to_string(),
    );
    assert!((at(&kernel, 50.0) - 10.0).abs() < 1e-3);
    run(
        &mut kernel,
        "model.update",
        &json!({ "id": pa, "level": first }).to_string(),
    );
    assert!(
        (at(&kernel, 50.0) - 13.0).abs() < 1e-3,
        "adds on the base level"
    );
    assert!(kernel
        .apply(
            "model.update",
            &json!({ "id": pa, "level": 99 }).to_string()
        )
        .is_err());

    // Deleting the level moves its components down; the last level stays.
    run(
        &mut kernel,
        "model.levels.delete",
        &json!({ "id": level }).to_string(),
    );
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["levels"].as_array().map(Vec::len), Some(1));
    assert!(list["components"]
        .as_array()
        .expect("components")
        .iter()
        .all(|c| c["level"].as_u64() == Some(first)));
    assert!(kernel
        .apply("model.levels.delete", &json!({ "id": first }).to_string())
        .is_err());
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Delete Level");
    assert_eq!(
        run(&mut kernel, "model.list", "")["levels"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn boundaries_and_stl_come_from_the_model() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 20.0, 20.0, 60.0, 50.0);
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id] }).to_string(),
    );
    let made = run(
        &mut kernel,
        "model.boundary",
        &json!({ "offset": 2.0 }).to_string(),
    );
    let outline = made["ids"][0].as_u64().expect("a vector");
    let bounds = kernel
        .document()
        .entity(outline)
        .ok()
        .and_then(|e| e.path())
        .and_then(|p| crate::geom::path_bbox(&p))
        .expect("bounds");
    assert!((bounds.min.x - 18.0).abs() < 0.6, "{bounds:?}");
    assert!((bounds.max.y - 52.0).abs() < 0.6, "{bounds:?}");
    assert_eq!(
        run(&mut kernel, "undo", "")["label"],
        "Create Boundary Vectors"
    );

    let path = std::env::temp_dir().join(format!("carve-export-{}.stl", std::process::id()));
    run(
        &mut kernel,
        "model.exportStl",
        &json!({ "path": path }).to_string(),
    );
    let bytes = std::fs::read(&path).expect("written");
    // The whole job grid would be about 100 MB; the bar alone is near 2 MB.
    assert!(
        bytes.len() < 5_000_000,
        "only the modelled area: {}",
        bytes.len()
    );
    let _ = std::fs::remove_file(&path);
    let triangles = io::mesh::stl(&bytes).expect("reads back");
    let top = triangles
        .iter()
        .flatten()
        .map(|v| v[2])
        .fold(f64::MIN, f64::max);
    assert!(
        (top - 15.0).abs() < 0.5,
        "a 30 wide bar rounds to 15 high: {top}"
    );
    assert!(Kernel::new()
        .apply("model.exportStl", &json!({ "path": path }).to_string())
        .is_err());
}

#[test]
fn baking_merges_components_and_replace_below_clips_one() {
    let mut kernel = Kernel::new();
    let low = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let high = make_rect(&mut kernel, 120.0, 80.0, 160.0, 120.0);
    let other = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let shape = |kernel: &mut Kernel, id: u64, height: f64| {
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": height } }).to_string(),
        )["id"]
            .as_u64()
            .expect("a component")
    };
    let a = shape(&mut kernel, low, 2.0);
    let spare = shape(&mut kernel, other, 1.0);
    let b = shape(&mut kernel, high, 3.0);
    let before = kernel.document().composite().at(Point::new(130.0, 100.0));
    let baked = run(
        &mut kernel,
        "model.bake",
        &json!({ "ids": [b, a] }).to_string(),
    )["id"]
        .as_u64()
        .expect("baked");
    let list = run(&mut kernel, "model.list", "");
    let ids: Vec<u64> = list["components"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|c| c["id"].as_u64())
        .collect();
    assert_eq!(
        ids,
        vec![baked, spare],
        "baked takes the lowest source's place"
    );
    let after = kernel.document().composite().at(Point::new(130.0, 100.0));
    assert!((after - before).abs() < 1e-4, "{before} became {after}");
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Bake Components");
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    run(&mut kernel, "redo", "");

    run(
        &mut kernel,
        "model.replaceBelow",
        &json!({ "ids": [baked], "height": 4.0 }).to_string(),
    );
    let field = &kernel.document().component(baked).expect("baked").field;
    assert!(
        field.at(Point::new(110.0, 100.0)).is_nan(),
        "the 2 mm part is gone"
    );
    assert!(
        !field.at(Point::new(130.0, 100.0)).is_nan(),
        "the 5 mm overlap stays"
    );
    let error = kernel
        .apply(
            "model.replaceBelow",
            &json!({ "ids": [baked], "height": 50.0 }).to_string(),
        )
        .expect_err("nothing above");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    let error = kernel
        .apply("model.bake", &json!({ "ids": [] }).to_string())
        .expect_err("no component");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn one_component_bakes_its_tilt_and_heights_in_and_keeps_its_name() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 },
                 "baseHeight": 2.0 })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    run(
        &mut kernel,
        "model.props",
        &json!({ "ids": [component], "heightScale": 2.0 }).to_string(),
    );
    let at = Point::new(120.0, 100.0);
    let before = kernel.document().composite().at(at);
    let baked = run(
        &mut kernel,
        "model.bake",
        &json!({ "ids": [component] }).to_string(),
    )["id"]
        .as_u64()
        .expect("baked");
    let after = kernel.document().composite().at(at);
    assert!((after - before).abs() < 1e-4, "{before} became {after}");
    let list = run(&mut kernel, "model.list", "");
    let items = list["components"].as_array().expect("list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_u64(), Some(baked));
    assert_eq!(
        items[0]["heightScale"].as_f64(),
        Some(1.0),
        "the scale is baked in"
    );
    let first = run(&mut kernel, "undo", "");
    assert_eq!(first["label"], "Bake Components");
    let name = |kernel: &mut Kernel| run(kernel, "model.list", "")["components"][0]["name"].clone();
    let original = name(&mut kernel);
    run(&mut kernel, "redo", "");
    assert_eq!(name(&mut kernel), original, "a lone bake keeps the name");
}

#[test]
fn the_undo_brush_restores_the_component_from_before_sculpting() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    let at = Point::new(120.0, 100.0);
    let stroke = |kernel: &mut Kernel, brush: &str| {
        run(
            kernel,
            "model.sculpt",
            &json!({ "id": component, "points": [{ "x": 120.0, "y": 100.0 }],
                     "params": { "brush": brush, "strength": 1.0, "falloff": 0.2 } })
            .to_string(),
        );
    };
    stroke(&mut kernel, "remove");
    stroke(&mut kernel, "remove");
    let dug = kernel.document().composite().at(at);
    assert!(dug < 2.5, "{dug}");
    stroke(&mut kernel, "restore");
    let back = kernel.document().composite().at(at);
    assert!((back - 3.0).abs() < 1e-4, "{back}");

    // After sculpting ends, the next stroke starts a new baseline.
    stroke(&mut kernel, "remove");
    run(&mut kernel, "model.sculptEnd", "");
    stroke(&mut kernel, "remove");
    stroke(&mut kernel, "restore");
    let partly = kernel.document().composite().at(at);
    assert!(
        (partly - 2.5).abs() < 1e-4,
        "restores only to after the first stroke: {partly}"
    );
}

#[test]
fn a_live_transform_drag_is_one_undo_step() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let depth = kernel.document().journal().undo_depth();
    let step = |kernel: &mut Kernel, dx: f64, replace: Option<u64>| {
        run(
            kernel,
            "model.transform",
            &json!({
                "id": component,
                "params": { "offset": { "x": dx, "y": 0.0 } },
                "replace": replace,
            })
            .to_string(),
        )["revision"]
            .as_u64()
    };
    let first = step(&mut kernel, 10.0, None);
    let second = step(&mut kernel, 20.0, first);
    let third = step(&mut kernel, 30.0, second);
    assert!(third.is_some());
    let at = |kernel: &Kernel, x: f64| kernel.document().composite().at(Point::new(x, 100.0));
    assert_eq!(at(&kernel, 165.0), 3.0, "moved 30 from the start, not 60");
    assert_eq!(at(&kernel, 125.0), 0.0);
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    // A stale revision stacks a new step instead of replacing.
    step(&mut kernel, 5.0, first);
    assert_eq!(kernel.document().journal().undo_depth(), depth + 2);
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    assert_eq!(at(&kernel, 120.0), 3.0, "back where it began");
}

#[test]
fn a_finer_modelling_resolution_makes_finer_components_and_model() {
    let shape_cell = |kernel: &mut Kernel| {
        let id = make_rect(kernel, 100.0, 80.0, 140.0, 120.0);
        let reply = run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "round", "angle": 45.0 } }).to_string(),
        );
        let component = reply["id"].as_u64().expect("an id");
        kernel
            .document()
            .component(component)
            .expect("made")
            .field
            .cell
    };
    let mut kernel = Kernel::new();
    let standard_grid = model::grid_for(kernel.document().job());
    let standard = shape_cell(&mut kernel);

    let mut job = *kernel.document().job_setup();
    job.resolution = crate::doc::ModelResolution::Highest;
    let reply = run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    assert_eq!(reply["resolution"], "highest");
    let finest_grid = model::grid_for(kernel.document().job());
    let finest = shape_cell(&mut kernel);
    assert!(finest < standard * 0.5, "{finest} against {standard}");
    let cells = |g: &model::shape::Grid| (g.width * g.height) as f64;
    assert!((cells(&finest_grid) / cells(&standard_grid) - 7.0).abs() < 0.2);
    match kernel.apply("model.composite", "").expect("composite") {
        KernelReply::Bytes(bytes) => {
            assert!(bytes.len() > finest_grid.width * finest_grid.height * 2);
        }
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    }
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    assert_eq!(
        kernel.document().job_setup().resolution,
        crate::doc::ModelResolution::Standard,
        "the resolution change undoes"
    );
}

#[test]
fn replace_below_works_on_one_component_several_or_a_level() {
    let mut kernel = Kernel::new();
    let shape = |kernel: &mut Kernel, x0: f64, x1: f64, height: f64| {
        let id = make_rect(kernel, x0, 80.0, x1, 120.0);
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": height } }).to_string(),
        )["id"]
            .as_u64()
            .expect("an id")
    };
    let low = shape(&mut kernel, 100.0, 140.0, 2.0);
    let high = shape(&mut kernel, 150.0, 190.0, 6.0);
    let at = |kernel: &Kernel, x: f64| kernel.document().composite().at(Point::new(x, 100.0));

    // One component, a plane at 3 mm, lowered: the 6 mm block becomes 3.
    run(
        &mut kernel,
        "model.replaceBelow",
        &json!({ "ids": [high], "height": 3.0, "with": "plane", "lower": true }).to_string(),
    );
    assert!((at(&kernel, 170.0) - 3.0).abs() < 1e-4);
    run(&mut kernel, "undo", "");

    // The active level: both are baked into one, the 2 mm block goes.
    let level = kernel.document().active_level();
    let reply = run(
        &mut kernel,
        "model.replaceBelow",
        &json!({ "level": level, "height": 4.0 }).to_string(),
    );
    let id = reply["id"].as_u64().expect("an id");
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["components"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["components"][0]["id"].as_u64(), Some(id));
    assert_eq!(at(&kernel, 120.0), 0.0, "below the height, gone");
    assert!((at(&kernel, 170.0) - 6.0).abs() < 1e-4);
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Replace Below");
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "one undo step brings both back"
    );
    let error = kernel
        .apply("model.replaceBelow", &json!({ "height": 1.0 }).to_string())
        .expect_err("no target");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    let _ = low;
}

#[test]
fn a_component_can_be_the_sculpting_brush() {
    let mut kernel = Kernel::new();
    let shape = |kernel: &mut Kernel, x0: f64, x1: f64, y0: f64, y1: f64| {
        let id = make_rect(kernel, x0, y0, x1, y1);
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
        )["id"]
            .as_u64()
            .expect("an id")
    };
    let slab = shape(&mut kernel, 50.0, 250.0, 50.0, 150.0);
    let bar = shape(&mut kernel, 10.0, 30.0, 10.0, 15.0);
    run(
        &mut kernel,
        "model.sculpt",
        &json!({
            "id": slab,
            "points": [{ "x": 150.0, "y": 100.0 }],
            "params": { "brush": "deposit", "diameter": 20.0, "strength": 1.0 },
            "custom": { "brush": bar, "angle": 90.0 },
        })
        .to_string(),
    );
    let field = &kernel.document().component(slab).expect("slab").field;
    // Turned upright, the 20 by 5 bar raises a strip 5 wide and 20 tall.
    assert!((field.at(Point::new(150.0, 108.0)) - 7.0).abs() < 0.2);
    assert!((field.at(Point::new(158.0, 100.0)) - 2.0).abs() < 1e-4);
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Sculpt");
    let error = kernel
        .apply(
            "model.sculpt",
            &json!({
                "id": slab,
                "points": [{ "x": 150.0, "y": 100.0 }],
                "params": { "brush": "smooth" },
                "custom": { "brush": bar },
            })
            .to_string(),
        )
        .expect_err("smooth has no custom brush");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn a_saved_brush_file_sculpts_like_its_component() {
    let temp = crate::test_support::TempDir::new("kernel-brush");
    let mut kernel = Kernel::new();
    let shape = |kernel: &mut Kernel, x0: f64, x1: f64, y0: f64, y1: f64| {
        let id = make_rect(kernel, x0, y0, x1, y1);
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
        )["id"]
            .as_u64()
            .expect("an id")
    };
    let slab = shape(&mut kernel, 50.0, 250.0, 50.0, 150.0);
    let bar = shape(&mut kernel, 10.0, 30.0, 10.0, 15.0);
    let depth = kernel.document().journal().undo_depth();
    let saved = run(
        &mut kernel,
        "model.createBrush",
        &json!({ "id": bar, "name": "Bar", "dir": temp.path() }).to_string(),
    );
    let path = saved["path"].as_str().expect("a path").to_string();
    assert!(path.ends_with("Bar.carveclip"), "{path}");
    assert_eq!(
        depth,
        kernel.document().journal().undo_depth(),
        "saving a brush is no undo step"
    );
    run(
        &mut kernel,
        "model.sculpt",
        &json!({
            "id": slab,
            "points": [{ "x": 150.0, "y": 100.0 }],
            "params": { "brush": "deposit", "diameter": 20.0, "strength": 1.0 },
            "custom": { "file": path, "angle": 90.0 },
        })
        .to_string(),
    );
    let field = &kernel.document().component(slab).expect("slab").field;
    assert!((field.at(Point::new(150.0, 108.0)) - 7.0).abs() < 0.2);
    assert!((field.at(Point::new(158.0, 100.0)) - 2.0).abs() < 1e-4);
    run(&mut kernel, "model.sculptEnd", "");
    let error = kernel
        .apply(
            "model.sculpt",
            &json!({
                "id": slab,
                "points": [{ "x": 150.0, "y": 100.0 }],
                "params": { "brush": "deposit", "diameter": 20.0 },
                "custom": { "angle": 0.0 },
            })
            .to_string(),
        )
        .expect_err("no brush chosen");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn the_other_side_model_turns_over_onto_this_side() {
    let mut kernel = Kernel::new();
    let job = crate::doc::JobSetup {
        kind: crate::doc::JobKind::DoubleSided,
        ..crate::doc::JobSetup::default()
    };
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    assert!(kernel
        .apply("model.composite", &json!({ "otherSide": true }).to_string())
        .is_err());
    run(
        &mut kernel,
        "job.setSide",
        &json!({ "side": "bottom" }).to_string(),
    );
    // A block near the left edge of the bottom side.
    let path = crate::geom::rect(Point::new(10.0, 80.0), Point::new(40.0, 120.0)).expect("rect");
    let id = run(
        &mut kernel,
        "entities.create",
        &json!({ "entity": { "kind": "path", "path": path } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 4.0 } }).to_string(),
    );
    run(
        &mut kernel,
        "job.setSide",
        &json!({ "side": "top" }).to_string(),
    );
    let blob = match kernel
        .apply("model.composite", &json!({ "otherSide": true }).to_string())
        .expect("the bottom model")
    {
        KernelReply::Bytes(bytes) => bytes,
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    };
    let (header, heights) = crate::blob::read_heights(&blob, "z");
    let width = header["width"].as_u64().expect("width") as usize;
    let cell = header["cellSize"].as_f64().expect("cell");
    let (ox, oy) = (
        header["originX"].as_f64().expect("x"),
        header["originY"].as_f64().expect("y"),
    );
    let z = |x: f64, y: f64| {
        let i = ((x - ox) / cell).round() as usize;
        let j = ((y - oy) / cell).round() as usize;
        heights[j * width + i]
    };
    assert_eq!(header["max"].as_f64(), Some(4.0));
    // Turned over about the vertical middle, it sits near the right edge.
    assert_eq!(z(275.0, 100.0), 4.0);
    assert!(z(25.0, 100.0).is_nan() || z(25.0, 100.0) == 0.0);
}

fn composite_at(kernel: &Kernel, x: f64, y: f64) -> f64 {
    f64::from(kernel.document().composite().at(Point::new(x, y)))
}

#[test]
fn applying_a_shape_again_updates_it_in_one_undo_step() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let depth = kernel.document().journal().undo_depth();
    let first = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "angular", "angle": 45.0 },
            "baseHeight": 2.0 })
        .to_string(),
    );
    let component = first["id"].as_u64().expect("an id");
    // A 40 mm square at 45 degrees peaks 20 mm up, on a 2 mm base.
    assert!(
        (first["totalHeight"].as_f64().unwrap_or(0.0) - 22.0).abs() < 0.3,
        "{first}"
    );
    let second = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "angular", "angle": 45.0,
                "scale": true, "scaleHeight": 5.0 },
            "baseHeight": 1.0, "target": component, "replace": first["revision"] })
        .to_string(),
    );
    assert_eq!(second["id"].as_u64(), Some(component), "the same component");
    assert!((second["shapeHeight"].as_f64().unwrap_or(0.0) - 5.0).abs() < 1e-6);
    assert!((composite_at(&kernel, 120.0, 100.0) - 6.0).abs() < 0.05);
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    let list = run(&mut kernel, "model.list", "");
    let shape = &list["components"][0]["shape"];
    assert_eq!(shape["params"]["scaleHeight"].as_f64(), Some(5.0));
    assert_eq!(shape["vectors"], json!([id]));
    assert_eq!(list["components"][0]["baseHeight"].as_f64(), Some(1.0));
    // Reset takes the shape back altogether.
    let reverted = run(
        &mut kernel,
        "model.revertPreview",
        &json!({ "revision": second["revision"] }).to_string(),
    );
    assert_eq!(reverted["reverted"], Value::Bool(true));
    assert!(run(&mut kernel, "model.list", "")["components"]
        .as_array()
        .is_some_and(Vec::is_empty));
}

#[test]
fn an_existing_shape_is_edited_in_place_as_a_new_step() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let other = make_rect(&mut kernel, 10.0, 10.0, 30.0, 30.0);
    let shape = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "name": "Keep" }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [other] }).to_string(),
    );
    let reply = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 0.0 },
            "baseHeight": 3.0, "target": shape })
        .to_string(),
    );
    assert_eq!(reply["id"].as_u64(), Some(shape));
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(
        list["components"][0]["name"], "Keep",
        "keeps its name and place"
    );
    assert!((composite_at(&kernel, 120.0, 100.0) - 3.0).abs() < 1e-6);
    run(&mut kernel, "undo", "");
    assert!(
        composite_at(&kernel, 120.0, 100.0) > 10.0,
        "a round shape again"
    );
}

#[test]
fn a_custom_shape_takes_its_profile_from_the_open_vector() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    // A 45 degree ramp with a leg at the ridge end: angular, 20 mm high.
    let ramp = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
        "start": { "x": 0.0, "y": 0.0 },
        "segments": [
            { "kind": "line", "to": { "x": 10.0, "y": 10.0 } },
            { "kind": "line", "to": { "x": 10.0, "y": 0.0 } }],
        "closed": false }] } } });
    let profile = run(&mut kernel, "entities.create", &ramp.to_string())["id"]
        .as_u64()
        .expect("an id");
    let error = kernel
        .apply(
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "custom" } }).to_string(),
        )
        .expect_err("needs its profile");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    let reply = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id, profile], "params": { "profile": "custom" } }).to_string(),
    );
    assert!(
        (reply["shapeHeight"].as_f64().unwrap_or(0.0) - 20.0).abs() < 0.3,
        "{reply}"
    );
    assert!((composite_at(&kernel, 105.0, 100.0) - 5.0).abs() < 0.3);
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(
        list["components"][0]["shape"]["profileVector"].as_u64(),
        Some(profile)
    );
    assert_eq!(list["components"][0]["shape"]["vectors"], json!([id]));
}

#[test]
fn a_flat_plane_without_vectors_covers_the_material() {
    let mut kernel = Kernel::new();
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [], "params": { "profile": "flat", "height": 0.0 },
            "baseHeight": 1.5 })
        .to_string(),
    );
    let bounds = kernel.document().job_bounds();
    for p in [bounds.min, bounds.max, bounds.centre()] {
        let inside = Point::new(
            p.x.clamp(bounds.min.x + 1.0, bounds.max.x - 1.0),
            p.y.clamp(bounds.min.y + 1.0, bounds.max.y - 1.0),
        );
        assert!((composite_at(&kernel, inside.x, inside.y) - 1.5).abs() < 1e-6);
    }
    assert!(kernel
        .apply("model.createShape", &json!({ "ids": [] }).to_string())
        .is_err());
}

#[test]
fn fade_and_tilt_are_dynamic_until_baked() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 4.0 },
            "fade": { "from": { "x": 100.0, "y": 100.0 }, "to": { "x": 140.0, "y": 100.0 },
                "amount": 0.5 } })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    assert!((composite_at(&kernel, 101.0, 100.0) - 3.95).abs() < 0.05);
    assert!((composite_at(&kernel, 139.0, 100.0) - 2.05).abs() < 0.05);
    let tilt = json!({ "pivot": { "x": 100.0, "y": 80.0 }, "toward": { "x": 100.0, "y": 90.0 },
        "angleDeg": 45.0 });
    run(
        &mut kernel,
        "model.props",
        &json!({ "ids": [component], "fade": null, "tilt": tilt }).to_string(),
    );
    assert!((composite_at(&kernel, 120.0, 110.0) - 34.0).abs() < 0.2);
    let listed = run(&mut kernel, "model.list", "");
    assert!((listed["components"][0]["maxHeight"].as_f64().unwrap_or(0.0) - 44.0).abs() < 0.3);
    assert_eq!(listed["components"][0]["fade"], Value::Null);
    // The anchors move with the component.
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": component, "params": { "offset": { "x": 0.0, "y": 50.0 } } }).to_string(),
    );
    assert!((composite_at(&kernel, 120.0, 160.0) - 34.0).abs() < 0.2);
    run(
        &mut kernel,
        "model.props",
        &json!({ "ids": [component], "bake": true }).to_string(),
    );
    let listed = run(&mut kernel, "model.list", "");
    let c = &listed["components"][0];
    assert_eq!(c["tilt"], Value::Null);
    assert_eq!(c["shape"], Value::Null);
    assert!(
        (composite_at(&kernel, 120.0, 160.0) - 34.0).abs() < 0.2,
        "baked in"
    );
    let bad = json!({ "ids": [component], "tilt": { "pivot": { "x": 1.0, "y": 1.0 },
        "toward": { "x": 1.0, "y": 1.0 }, "angleDeg": 10.0 } });
    assert!(kernel.apply("model.props", &bad.to_string()).is_err());
}

#[test]
fn a_sweep_takes_its_stations_and_applies_again_in_place() {
    let mut kernel = Kernel::new();
    let a = line_entity(&mut kernel, (50.0, 100.0), (250.0, 100.0));
    let b = line_entity(&mut kernel, (50.0, 120.0), (250.0, 120.0));
    let arc = json!({ "entity": { "kind": "path", "path": { "subpaths": [{
        "start": { "x": 0.0, "y": 0.0 },
        "segments": [{ "kind": "arc", "to": { "x": 20.0, "y": 0.0 }, "bulge": -1.0 }],
        "closed": false }] } } });
    let section = run(&mut kernel, "entities.create", &arc.to_string())["id"]
        .as_u64()
        .expect("an id");
    let args = |extra: Value| {
        let mut v = json!({ "rails": [a, b], "sections": [section], "params": {
            "stations": [
                { "section": 0, "a": 0.0, "b": 0.0, "smooth": true },
                { "section": 0, "a": 1.0, "b": 1.0 }
            ],
            "exactHeight": true, "height": 4.0 } });
        if let (Some(v), Value::Object(extra)) = (v.as_object_mut(), extra) {
            v.extend(extra);
        }
        v.to_string()
    };
    let first = run(&mut kernel, "model.sweep", &args(json!({})));
    let id = first["id"].as_u64().expect("an id");
    assert!(
        (first["shapeHeight"].as_f64().unwrap_or(0.0) - 4.0).abs() < 1e-4,
        "{first}"
    );
    let second = run(
        &mut kernel,
        "model.sweep",
        &args(json!({ "target": id, "replace": first["revision"], "name": "Rope" })),
    );
    assert_eq!(second["id"].as_u64(), Some(id));
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(list["components"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["components"][0]["name"], "Rope");
}

#[test]
fn a_texture_area_tiles_its_seed_and_resizing_keeps_the_tiles() {
    let mut kernel = Kernel::new();
    let square = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let seed = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [square], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a seed");
    let area = make_rect(&mut kernel, 0.0, 0.0, 100.0, 60.0);
    let reply = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [area], "params": { "spacingX": 100.0, "spacingY": 100.0 } })
            .to_string(),
    );
    let texture = reply["id"].as_u64().expect("an id");
    let at = |kernel: &Kernel, x: f64, y: f64| {
        f64::from(kernel.document().composite().at(Point::new(x, y)))
    };
    // Tiles 10 wide every 20 from the seed at 10..20: 30..40 and 50..60 too.
    assert!((at(&kernel, 35.0, 15.0) - 2.0).abs() < 1e-6);
    assert!(at(&kernel, 45.0, 15.0).abs() < 1e-6, "a gap");
    assert!((at(&kernel, 35.0, 35.0) - 2.0).abs() < 1e-6);
    // Doubling the width doubles the area, not the tiles.
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": texture, "params": { "scaleX": 2.0, "centre": { "x": 0.0, "y": 0.0 } } })
            .to_string(),
    );
    // The lattice starts at the tile corner, moved from 10 to 20.
    assert!(
        (at(&kernel, 185.0, 15.0) - 2.0).abs() < 1e-6,
        "tiles out to 200 now"
    );
    assert!(
        at(&kernel, 175.0, 15.0).abs() < 1e-6,
        "still 10 wide with gaps"
    );
    assert!((at(&kernel, 25.0, 15.0) - 2.0).abs() < 1e-6);
    let list = run(&mut kernel, "model.list", "");
    assert_eq!(
        list["components"][1]["texture"]["seed"].as_u64(),
        Some(seed)
    );
    // Editing it again from its component updates it in place.
    let again = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [area], "target": texture,
            "params": { "spacingX": 0.0, "spacingY": 0.0 } })
        .to_string(),
    );
    assert_eq!(again["id"].as_u64(), Some(texture));
    assert!((at(&kernel, 45.0, 15.0) - 2.0).abs() < 1e-6, "no gaps now");
}

#[test]
fn the_composite_goes_as_u16_heights_well_inside_the_model_resolution() {
    // The default 300 x 200 mm job at the finest resolution, with a round
    // dome beside a 25 mm deep subtracted pocket: over 60 mm of heights on
    // cells under 0.1 mm.
    let mut kernel = Kernel::new();
    let mut job = *kernel.document().job_setup();
    job.resolution = crate::doc::ModelResolution::Highest;
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    let dome = make_rect(&mut kernel, 20.0, 20.0, 140.0, 180.0);
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [dome], "params": { "profile": "round", "angle": 90.0, "height": 60.0,
                 "limit": true } })
        .to_string(),
    );
    let pocket = make_rect(&mut kernel, 180.0, 40.0, 280.0, 160.0);
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [pocket], "params": { "profile": "flat", "height": 25.0 },
                 "combine": "subtract" })
        .to_string(),
    );
    let KernelReply::Bytes(blob) = kernel.apply("model.composite", "").expect("composite") else {
        panic!("expected bytes");
    };
    let (header, sent) = crate::blob::read_heights(&blob, "z");
    assert_eq!(header["sections"][0]["dtype"], json!("u16"));
    let composite = kernel.document().composite();
    let (lo, hi) = (composite.min().expect("min"), composite.max().expect("max"));
    assert!(hi - lo > 60.0, "a wide range: {lo} to {hi}");
    let step = header["heightStep"].as_f64().expect("step");
    let worst = sent
        .iter()
        .zip(&composite.z)
        .filter(|(_, kept)| !kept.is_nan())
        .map(|(s, k)| f64::from((s - k).abs()))
        .fold(0.0, f64::max);
    assert!(
        sent.iter()
            .zip(&composite.z)
            .all(|(s, k)| s.is_nan() == k.is_nan()),
        "empty cells stay empty"
    );
    // Half a step plus the f32 rounding of the height itself.
    assert!(
        worst <= step * 0.5 + 1e-5,
        "{worst} against a step of {step}"
    );
    // Under a hundredth of a cell: nothing the grid can show.
    assert!(
        worst < composite.cell * 0.01,
        "{worst} mm against {} mm cells",
        composite.cell
    );
    assert_eq!(header["min"].as_f64(), Some(f64::from(lo)));
    assert_eq!(header["max"].as_f64(), Some(f64::from(hi)));
    // Half the bytes of f32 heights.
    let cells = composite.width * composite.height;
    assert!(blob.len() < cells * 2 + 1024);
}

#[test]
fn heights_too_tall_for_u16_steps_go_as_f32() {
    // A tiny job at the finest resolution has cells of about 4 um, too
    // small for 65534 steps over a 400 mm tall shape.
    let mut kernel = Kernel::new();
    let mut job = *kernel.document().job_setup();
    job.width = 10.0;
    job.height = 10.0;
    job.resolution = crate::doc::ModelResolution::Highest;
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    let id = make_rect(&mut kernel, 1.0, 1.0, 9.0, 9.0);
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 400.0 } }).to_string(),
    );
    let KernelReply::Bytes(blob) = kernel.apply("model.composite", "").expect("composite") else {
        panic!("expected bytes");
    };
    let (header, sent) = crate::blob::read_heights(&blob, "z");
    assert_eq!(header["sections"][0]["dtype"], json!("f32"));
    assert!(header.get("heightStep").is_none());
    assert_eq!(sent, kernel.document().composite().z);
}

#[test]
fn sculpting_keeps_discards_and_cancels() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let dab = |kernel: &mut Kernel| {
        run(
            kernel,
            "model.sculpt",
            &json!({ "id": component, "points": [{ "x": 120.0, "y": 100.0 }],
                "params": { "brush": "deposit", "diameter": 10.0, "strength": 1.0 } })
            .to_string(),
        );
    };
    let top =
        |kernel: &Kernel| f64::from(kernel.document().composite().at(Point::new(120.0, 100.0)));
    dab(&mut kernel);
    let once = top(&kernel);
    assert!(once > 2.0);
    run(
        &mut kernel,
        "model.sculptKeep",
        &json!({ "id": component }).to_string(),
    );
    dab(&mut kernel);
    assert!(top(&kernel) > once);
    // Discard goes back to the Keep.
    let reply = run(
        &mut kernel,
        "model.sculptRevert",
        &json!({ "id": component, "to": "kept" }).to_string(),
    );
    assert_eq!(reply["reverted"], Value::Bool(true));
    assert!((top(&kernel) - once).abs() < 1e-6);
    // Cancel goes back to before sculpting began.
    run(
        &mut kernel,
        "model.sculptRevert",
        &json!({ "id": component, "to": "start" }).to_string(),
    );
    assert!((top(&kernel) - 2.0).abs() < 1e-6);
    run(&mut kernel, "model.sculptEnd", "");
    let none = run(
        &mut kernel,
        "model.sculptRevert",
        &json!({ "id": component, "to": "start" }).to_string(),
    );
    assert_eq!(
        none["reverted"],
        Value::Bool(false),
        "a new session has nothing to undo"
    );
}

#[test]
fn a_vector_boundary_is_the_silhouette_grouped_when_it_takes_several_loops() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 20.0, 20.0, 40.0, 40.0);
    let b = make_rect(&mut kernel, 80.0, 20.0, 100.0, 40.0);
    // Flat planes at zero height: only the silhouette can outline them.
    let flat = |kernel: &mut Kernel, id: u64| {
        run(
            kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": 0.0 } }).to_string(),
        )["id"]
            .as_u64()
            .expect("a plane")
    };
    let (p, q) = (flat(&mut kernel, a), flat(&mut kernel, b));
    let made = run(
        &mut kernel,
        "model.boundary",
        &json!({ "ids": [p, q], "group": true }).to_string(),
    );
    assert_eq!(made["ids"].as_array().map(Vec::len), Some(1), "{made}");
    assert_eq!(made["vectors"].as_array().map(Vec::len), Some(2));
    let group = made["ids"][0].as_u64().expect("a group");
    assert!(matches!(
        kernel.document().entity(group).map(|e| &e.kind),
        Ok(crate::doc::EntityKind::Group { .. })
    ));
    let one = run(
        &mut kernel,
        "model.boundary",
        &json!({ "ids": [p], "group": true }).to_string(),
    );
    assert_eq!(one["ids"], one["vectors"], "a single loop is not grouped");
    // With a level above the planes there is nothing to outline.
    assert!(kernel
        .apply(
            "model.boundary",
            &json!({ "ids": [p], "level": 1.0 }).to_string()
        )
        .is_err());
}

#[test]
fn components_transform_with_the_selection_by_any_matrix() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "angular", "angle": 45.0 },
            "tilt": { "pivot": { "x": 100.0, "y": 100.0 }, "toward": { "x": 90.0, "y": 100.0 },
                "angleDeg": 10.0 } })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("a shape");
    let before = f64::from(kernel.document().composite().at(Point::new(110.0, 100.0)));
    // Mirror left to right about x = 150: the shape lands on 160 to 200.
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [component], "matrix": [-1.0, 0.0, 0.0, 1.0, 300.0, 0.0] }).to_string(),
    );
    let after = f64::from(kernel.document().composite().at(Point::new(190.0, 100.0)));
    assert!((after - before).abs() < 0.15, "{before} {after}");
    assert_eq!(
        kernel.document().composite().at(Point::new(110.0, 100.0)),
        0.0
    );
    let tilt = kernel
        .document()
        .component(component)
        .ok()
        .and_then(|c| c.extra.tilt)
        .expect("a tilt");
    assert!((tilt.pivot.x - 200.0).abs() < 1e-9 && (tilt.toward.x - 210.0).abs() < 1e-9);
    assert_eq!(
        run(&mut kernel, "undo", "")["label"],
        "Transform Components"
    );
}

#[test]
fn auto_scale_z_scales_the_heights_with_the_size() {
    let mut kernel = Kernel::new();
    let vector = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [vector], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a shape");
    let scale = |kernel: &Kernel| {
        kernel
            .document()
            .component(component)
            .expect("component")
            .height_scale
    };
    let before = scale(&kernel);
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [component], "matrix": [2.0, 0.0, 0.0, 2.0, -100.0, -80.0],
                 "scaleZ": true })
        .to_string(),
    );
    assert!((scale(&kernel) - before * 2.0).abs() < 1e-9);
    // Without it the heights stay.
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [component], "matrix": [0.5, 0.0, 0.0, 0.5, 50.0, 40.0] }).to_string(),
    );
    assert!((scale(&kernel) - before * 2.0).abs() < 1e-9);
}

#[test]
fn vectors_and_components_moved_in_an_undo_group_undo_together() {
    let mut kernel = Kernel::new();
    let vector = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [vector], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a shape");
    run(&mut kernel, "undo.beginGroup", "{}");
    run(
        &mut kernel,
        "ops.move",
        &json!({ "ids": [vector], "dx": 50.0, "dy": 0.0 }).to_string(),
    );
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [component], "matrix": [1.0, 0.0, 0.0, 1.0, 50.0, 0.0] }).to_string(),
    );
    assert_eq!(run(&mut kernel, "undo.endGroup", "")["merged"], true);
    assert_eq!(
        kernel.document().composite().at(Point::new(170.0, 100.0)),
        3.0
    );
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Move");
    assert_eq!(
        kernel.document().composite().at(Point::new(120.0, 100.0)),
        3.0
    );
    assert_eq!(
        kernel.document().composite().at(Point::new(170.0, 100.0)),
        0.0
    );
    let bbox = kernel.document().bbox_of_many(&[vector]).expect("a box");
    assert!((bbox.min.x - 100.0).abs() < 1e-9);
    // The shape itself is the step before.
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Create Shape");
}

#[test]
fn a_turned_texture_area_keeps_its_tiles_and_stays_editable() {
    let mut kernel = Kernel::new();
    // A 10 by 20 tile every 20 across and 40 up.
    let tall = make_rect(&mut kernel, 10.0, 10.0, 20.0, 30.0);
    let seed = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [tall], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a seed");
    let area = make_rect(&mut kernel, 0.0, 0.0, 100.0, 100.0);
    let texture = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [area], "params": { "spacingX": 100.0, "spacingY": 100.0 } })
            .to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": texture, "params": { "rotateDeg": 90.0, "centre": { "x": 50.0, "y": 50.0 } } })
            .to_string(),
    );
    let list = run(&mut kernel, "model.list", "");
    let source = &list["components"][1]["texture"];
    assert_eq!(source["seed"].as_u64(), Some(seed), "still a texture area");
    assert!((source["params"]["angle"].as_f64().unwrap_or(0.0) - 90.0).abs() < 1e-9);
    // The tile corner (10, 10) turned about (50, 50) lands on (90, 10); the
    // tile now runs 20 across to the left of it and 10 up.
    let at = |x: f64, y: f64| f64::from(kernel.document().composite().at(Point::new(x, y)));
    assert!((at(80.0, 15.0) - 2.0).abs() < 1e-6, "{}", at(80.0, 15.0));
    assert!(at(80.0, 25.0).abs() < 1e-6, "a gap above the turned tile");
    // Mirrored, it is a plain component.
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [texture], "matrix": [-1.0, 0.0, 0.0, 1.0, 100.0, 0.0] }).to_string(),
    );
    let list = run(&mut kernel, "model.list", "");
    assert!(list["components"][1]["texture"].is_null());
}

#[test]
fn components_copy_by_a_matrix_in_one_undo_step() {
    let mut kernel = Kernel::new();
    let square = make_rect(&mut kernel, 10.0, 10.0, 30.0, 30.0);
    let shape = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [square], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a shape");
    // Mirrored left to right about x = 100: the copy lands on 170 to 190.
    let reply = run(
        &mut kernel,
        "model.copyMatrix",
        &json!({ "ids": [shape], "matrix": [-1.0, 0.0, 0.0, 1.0, 200.0, 0.0] }).to_string(),
    );
    let copy = reply["ids"][0].as_u64().expect("a copy");
    assert_ne!(copy, shape);
    let at =
        |kernel: &Kernel, x: f64| f64::from(kernel.document().composite().at(Point::new(x, 20.0)));
    assert!((at(&kernel, 20.0) - 2.0).abs() < 1e-6, "the original stays");
    assert!((at(&kernel, 180.0) - 2.0).abs() < 1e-6, "the copy");
    // The copy keeps its Create Shape settings, on the original's vectors
    // and mirrored off them.
    let list = run(&mut kernel, "model.list", "");
    let source = &list["components"][1]["shape"];
    assert_eq!(source["vectors"], json!([square]));
    assert_eq!(source["params"]["profile"], "flat");
    assert!((source["placement"]["a"].as_f64().unwrap_or(0.0) + 1.0).abs() < 1e-9);
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Copy Components");
    assert!(at(&kernel, 180.0).abs() < 1e-6);
}

#[test]
fn a_live_matrix_drag_replaces_its_step_measured_from_where_it_began() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let depth = kernel.document().journal().undo_depth();
    let step = |kernel: &mut Kernel, dx: f64, replace: Option<u64>| {
        run(
            kernel,
            "model.transformMatrix",
            &json!({
                "ids": [component],
                "matrix": [1.0, 0.0, 0.0, 1.0, dx, 0.0],
                "replace": replace,
            })
            .to_string(),
        )["revision"]
            .as_u64()
    };
    let first = step(&mut kernel, 10.0, None);
    let second = step(&mut kernel, 20.0, first);
    step(&mut kernel, 30.0, second);
    let at = |kernel: &Kernel, x: f64| kernel.document().composite().at(Point::new(x, 100.0));
    assert_eq!(at(&kernel, 165.0), 3.0, "moved 30 from the start, not 60");
    assert_eq!(at(&kernel, 185.0), 0.0);
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    // A replacing step that fails keeps the step it would have replaced.
    let stale = kernel.document().revision();
    assert!(kernel
        .apply(
            "model.transformMatrix",
            &json!({ "ids": [component, 999_999], "matrix": [1.0, 0.0, 0.0, 1.0, 5.0, 0.0],
                "replace": stale })
            .to_string(),
        )
        .is_err());
    assert_eq!(at(&kernel, 165.0), 3.0);
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    run(&mut kernel, "undo", "");
    assert_eq!(at(&kernel, 120.0), 3.0, "back where it began");
}

#[test]
fn a_live_turn_of_a_texture_area_is_measured_from_where_it_began() {
    let mut kernel = Kernel::new();
    let square = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let seed = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [square], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a seed");
    let area = make_rect(&mut kernel, 0.0, 0.0, 100.0, 100.0);
    let texture = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [area], "params": { "spacingX": 100.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let depth = kernel.document().journal().undo_depth();
    let mut replace: Option<u64> = None;
    for angle in [10.0, 20.0, 30.0] {
        replace = run(
            &mut kernel,
            "model.transform",
            &json!({ "id": texture, "params": { "rotateDeg": angle }, "replace": replace })
                .to_string(),
        )["revision"]
            .as_u64();
    }
    let list = run(&mut kernel, "model.list", "");
    let angle = list["components"][1]["texture"]["params"]["angle"]
        .as_f64()
        .unwrap_or(0.0);
    assert!((angle - 30.0).abs() < 1e-6, "{angle}");
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
}

/// How near (mm) a moved or rebuilt round shape's top comes back: the top
/// of a square is where its corner hips meet, so a shape made again on a
/// moved square, sampled a part of a cell off that point, comes within
/// 0.1 mm of it.
const ON_TOP: f64 = 0.1;

/// Makes a round shape on a new rectangle; answers the rectangle and the
/// component.
fn round_shape(kernel: &mut Kernel, x0: f64, y0: f64, x1: f64, y1: f64) -> (u64, u64) {
    let vector = make_rect(kernel, x0, y0, x1, y1);
    let shape = run(
        kernel,
        "model.createShape",
        &json!({ "ids": [vector], "params": { "profile": "round", "angle": 45.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a shape");
    (vector, shape)
}

/// Makes shape `component` again from `vector` with a new edge angle, as
/// the shape tools do when it is edited.
fn rebuild(kernel: &mut Kernel, vector: u64, component: u64, angle: f64) {
    run(
        kernel,
        "model.createShape",
        &json!({
            "ids": [vector],
            "params": { "profile": "round", "angle": angle },
            "target": component,
        })
        .to_string(),
    );
}

#[test]
fn a_shape_moved_off_its_vectors_is_rebuilt_where_it_was_put() {
    let mut kernel = metric_kernel();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    assert!(top > 1.0);
    // Moved in 2D on its own: the vectors stay behind.
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [shape], "matrix": [1.0, 0.0, 0.0, 1.0, 50.0, 0.0] }).to_string(),
    );
    assert!(composite_at(&kernel, 120.0, 100.0).abs() < 1e-6);
    let placement = &run(&mut kernel, "model.list", "")["components"][0]["shape"]["placement"];
    assert!((placement["e"].as_f64().unwrap_or(0.0) - 50.0).abs() < 1e-9);
    rebuild(&mut kernel, vector, shape, 60.0);
    assert!(
        composite_at(&kernel, 120.0, 100.0).abs() < 1e-6,
        "not back on the vectors"
    );
    let moved = composite_at(&kernel, 170.0, 100.0);
    assert!(moved > top, "rebuilt steeper where it was put: {moved}");
    // Moved again with the 3D handles, and rebuilt: it stays there too.
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": shape, "params": { "offset": { "x": 0.0, "y": 40.0 } } }).to_string(),
    );
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!(composite_at(&kernel, 170.0, 100.0).abs() < 1e-6);
    assert!((composite_at(&kernel, 170.0, 140.0) - top).abs() < ON_TOP);
    // Picked on other vectors, the shape is made on them instead.
    let other = make_rect(&mut kernel, 10.0, 10.0, 50.0, 50.0);
    rebuild(&mut kernel, other, shape, 45.0);
    assert!((composite_at(&kernel, 30.0, 30.0) - top).abs() < ON_TOP);
    assert!(composite_at(&kernel, 170.0, 140.0).abs() < 1e-6);
    let list = run(&mut kernel, "model.list", "");
    assert!(list["components"][0]["shape"]["placement"].is_null());
}

#[test]
fn a_shape_moved_with_its_vectors_still_lies_on_them() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    // Turned a quarter about (100, 80) with the rectangle, as the
    // selection's handles do: vectors first, then the component.
    let quarter = [0.0, 1.0, -1.0, 0.0, 180.0, -20.0];
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [vector], "matrix": quarter }).to_string(),
    );
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [shape], "matrix": quarter, "vectors": [vector] }).to_string(),
    );
    let list = run(&mut kernel, "model.list", "");
    assert!(list["components"][0]["shape"]["placement"].is_null());
    // The rectangle now spans x 80..120, y 80..120; its middle is (80, 100).
    assert!((composite_at(&kernel, 80.0, 100.0) - top).abs() < ON_TOP);
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!((composite_at(&kernel, 80.0, 100.0) - top).abs() < ON_TOP);
    assert!(composite_at(&kernel, 130.0, 100.0).abs() < 1e-6);
}

#[test]
fn a_turned_or_scaled_shape_is_rebuilt_turned_or_scaled() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 180.0, 100.0);
    // Turned a quarter about the rectangle's middle (140, 90) on its own:
    // it now stands upright over x 130..150, y 50..130.
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": shape, "params": { "rotateDeg": 90.0 } }).to_string(),
    );
    let turned = composite_at(&kernel, 140.0, 120.0);
    assert!(turned > 1.0);
    assert!(composite_at(&kernel, 170.0, 90.0).abs() < 1e-6);
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!((composite_at(&kernel, 140.0, 120.0) - turned).abs() < 1e-3);
    assert!(composite_at(&kernel, 170.0, 90.0).abs() < 1e-6);
    // Stretched along Y to half its length: rebuilt, it keeps that size.
    run(
        &mut kernel,
        "model.transform",
        &json!({ "id": shape, "params": { "scaleY": 0.5 } }).to_string(),
    );
    let before = kernel
        .document()
        .component(shape)
        .expect("shape")
        .field
        .clone();
    rebuild(&mut kernel, vector, shape, 45.0);
    let after = &kernel.document().component(shape).expect("shape").field;
    assert_eq!((after.width, after.height), (before.width, before.height));
    assert_eq!(after.origin, before.origin);
    assert!(composite_at(&kernel, 140.0, 120.0).abs() < 1e-6);
}

/// The Create Shape source of component `id` as model.list answers it.
fn shape_source(kernel: &mut Kernel, id: u64) -> Value {
    let list = run(kernel, "model.list", "");
    list["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["id"].as_u64() == Some(id))
        .map(|c| c["shape"].clone())
        .expect("the component")
}

/// Makes a text entity at (x, y); answers its id.
fn make_text(kernel: &mut Kernel, x: f64, y: f64) -> u64 {
    let args = json!({
        "spec": {
            "text": "AB",
            "fontFamily": FontLibrary::FALLBACK_FAMILY,
            "sizeMm": 10.0,
        },
        "x": x,
        "y": y,
    });
    run(kernel, "text.create", &args.to_string())["id"]
        .as_u64()
        .expect("a text id")
}

#[test]
fn a_copied_shape_moved_off_its_vectors_is_edited_where_the_copy_lies() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let copy = run(
        &mut kernel,
        "model.copyMatrix",
        &json!({ "ids": [shape], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 60.0] }).to_string(),
    )["ids"][0]
        .as_u64()
        .expect("a copy");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], json!([vector]));
    rebuild(&mut kernel, vector, copy, 60.0);
    let steeper = composite_at(&kernel, 120.0, 160.0);
    assert!(steeper > top, "the copy was edited in place: {steeper}");
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
}

#[test]
fn vectors_text_and_groups_copy_with_their_components_in_one_step() {
    let mut kernel = metric_kernel();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let text = make_text(&mut kernel, 10.0, 10.0);
    let a = make_rect(&mut kernel, 10.0, 150.0, 20.0, 160.0);
    let b = make_rect(&mut kernel, 30.0, 150.0, 40.0, 160.0);
    let group = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [a, b] }).to_string(),
    )["id"]
        .as_u64()
        .expect("a group");
    let before = kernel.document().order().len();
    let reply = run(
        &mut kernel,
        "model.copyMatrix",
        &json!({
            "ids": [shape],
            "matrix": [1.0, 0.0, 0.0, 1.0, 100.0, 0.0],
            "vectors": [vector, text, group],
        })
        .to_string(),
    );
    let copies: Vec<u64> = reply["vectors"]
        .as_array()
        .expect("vector copies")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(copies.len(), 3);
    let doc = kernel.document();
    assert_eq!(doc.entity(copies[1]).expect("text").kind.tag(), "text");
    let crate::doc::EntityKind::Group { children } = &doc.entity(copies[2]).expect("group").kind
    else {
        panic!("the group is copied as a group");
    };
    assert_eq!(children.len(), 2);
    let moved = doc.bbox_of(copies[1]).expect("text box");
    assert!(moved.min.x > 100.0, "{moved:?}");
    assert_eq!(doc.selection(), copies.as_slice());
    // The copied shape is made from the copied rectangle and lies on it.
    let copy = reply["ids"][0].as_u64().expect("a component copy");
    let source = shape_source(&mut kernel, copy);
    assert_eq!(source["vectors"], json!([copies[0]]));
    assert!(source["placement"].is_null());
    rebuild(&mut kernel, copies[0], copy, 45.0);
    assert!((composite_at(&kernel, 220.0, 100.0) - top).abs() < ON_TOP);
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
    // One undo step takes every copy away.
    run(&mut kernel, "undo", "");
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Copy");
    assert_eq!(kernel.document().order().len(), before);
    assert_eq!(kernel.document().components().len(), 1);
}

#[test]
fn an_array_copy_takes_the_components_and_each_shape_its_own_vectors() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let reply = run(
        &mut kernel,
        "ops.arrayCopy",
        &json!({
            "ids": [vector],
            "components": [shape],
            "rows": 1,
            "cols": 3,
            "gapX": 10.0,
            "mode": "gap",
        })
        .to_string(),
    );
    let vectors: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    let components: Vec<u64> = reply["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!((vectors.len(), components.len()), (2, 2));
    for (i, (vector, component)) in vectors.iter().zip(&components).enumerate() {
        let source = shape_source(&mut kernel, *component);
        assert_eq!(source["vectors"], json!([vector]));
        assert!(source["placement"].is_null());
        let x = 120.0 + 50.0 * (i + 1) as f64;
        assert!(
            (composite_at(&kernel, x, 100.0) - top).abs() < ON_TOP,
            "{x}"
        );
    }
    // Components alone array too, over their own box.
    let reply = run(
        &mut kernel,
        "ops.arrayCopy",
        &json!({ "ids": [], "components": [shape], "rows": 2, "cols": 1, "gapY": 0.0 }).to_string(),
    );
    let copy = reply["components"][0].as_u64().expect("a copy");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], json!([vector]));
}

#[test]
fn a_cut_and_pasted_shape_is_made_from_the_pasted_vectors() {
    let mut kernel = metric_kernel();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let text = make_text(&mut kernel, 10.0, 10.0);
    let reply = run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [vector, text], "components": [shape] }).to_string(),
    );
    assert_eq!(reply["count"], 3);
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [vector, text] }).to_string(),
    );
    run(
        &mut kernel,
        "model.delete",
        &json!({ "ids": [shape] }).to_string(),
    );
    let reply = run(
        &mut kernel,
        "clipboard.paste",
        &json!({ "dx": 10.0, "dy": 10.0 }).to_string(),
    );
    let ids: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(
        kernel.document().entity(ids[1]).expect("text").kind.tag(),
        "text"
    );
    let copy = reply["components"][0].as_u64().expect("a component");
    let source = shape_source(&mut kernel, copy);
    assert_eq!(source["vectors"], json!([ids[0]]));
    assert!(source["placement"].is_null());
    assert!((composite_at(&kernel, 130.0, 110.0) - top).abs() < ON_TOP);
    rebuild(&mut kernel, ids[0], copy, 45.0);
    assert!((composite_at(&kernel, 130.0, 110.0) - top).abs() < ON_TOP);
    // Pasted again: new copies, the clipboard is kept.
    let again = run(
        &mut kernel,
        "clipboard.paste",
        &json!({ "dx": 20.0 }).to_string(),
    );
    assert_ne!(again["ids"][0], json!(ids[0]));
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Paste");
}

/// The ids in `reply[key]`.
fn ids_in(reply: &Value, key: &str) -> Vec<u64> {
    reply[key]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

#[test]
fn a_circular_array_takes_the_components_and_each_shape_its_own_vectors() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    // Four round (170, 100), kept upright: the copies' middles land at
    // (170, 150), (220, 100) and (170, 50).
    let reply = run(
        &mut kernel,
        "ops.circularArray",
        &json!({
            "ids": [vector],
            "components": [shape],
            "cx": 170.0,
            "cy": 100.0,
            "count": 4,
            "angle": 360.0,
            "rotateCopies": false,
        })
        .to_string(),
    );
    let (vectors, components) = (ids_in(&reply, "ids"), ids_in(&reply, "components"));
    assert_eq!((vectors.len(), components.len()), (3, 3));
    for (vector, component) in vectors.iter().zip(&components) {
        let source = shape_source(&mut kernel, *component);
        assert_eq!(source["vectors"], json!([vector]));
        assert!(source["placement"].is_null());
    }
    for (x, y) in [(170.0, 150.0), (220.0, 100.0), (170.0, 50.0)] {
        assert!(
            (composite_at(&kernel, x, y) - top).abs() < ON_TOP,
            "{x} {y}"
        );
    }
    // Components alone go round too, one undo step with the rest.
    let reply = run(
        &mut kernel,
        "ops.circularArray",
        &json!({ "ids": [], "components": [shape], "cx": 120.0, "cy": 60.0, "count": 2,
                 "angle": 180.0 })
        .to_string(),
    );
    assert!(ids_in(&reply, "ids").is_empty());
    let copy = reply["components"][0].as_u64().expect("a copy");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], json!([vector]));
    assert!((composite_at(&kernel, 120.0, 20.0) - top).abs() < ON_TOP);
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Circular Array");
}

#[test]
fn copy_along_vectors_takes_the_components_and_deletes_them_with_the_originals() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let guide = line_entity(&mut kernel, (50.0, 150.0), (250.0, 150.0));
    let reply = run(
        &mut kernel,
        "ops.copyAlong",
        &json!({
            "ids": [vector],
            "components": [shape],
            "pathId": guide,
            "count": 2,
            "alignToCurve": false,
            "deleteOriginal": true,
        })
        .to_string(),
    );
    let (vectors, components) = (ids_in(&reply, "ids"), ids_in(&reply, "components"));
    assert_eq!((vectors.len(), components.len()), (2, 2));
    for (vector, component) in vectors.iter().zip(&components) {
        assert_eq!(
            shape_source(&mut kernel, *component)["vectors"],
            json!([vector])
        );
    }
    assert!((composite_at(&kernel, 50.0, 150.0) - top).abs() < ON_TOP);
    assert!((composite_at(&kernel, 250.0, 150.0) - top).abs() < ON_TOP);
    // The originals, vector and component, are gone.
    assert!(kernel.document().entity(vector).is_err());
    assert!(kernel.document().component(shape).is_err());
    assert!(composite_at(&kernel, 120.0, 100.0).abs() < 1e-6);
    run(&mut kernel, "undo", "");
    assert!(kernel.document().component(shape).is_ok());
    assert_eq!(kernel.document().components().len(), 1);
}

#[test]
fn delete_and_cut_take_the_selected_components_with_the_vectors() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let other = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let depth = kernel.document().journal().undo_depth();
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [vector], "components": [shape] }).to_string(),
    );
    assert!(kernel.document().entity(vector).is_err());
    assert!(kernel.document().components().is_empty());
    assert!(kernel.document().entity(other).is_ok());
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Delete");
    assert!(kernel.document().entity(vector).is_ok());
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
    // Components alone: nothing else goes.
    run(&mut kernel, "selection.clear", "{}");
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [], "components": [shape] }).to_string(),
    );
    assert!(kernel.document().components().is_empty());
    assert!(kernel.document().entity(vector).is_ok());
    assert!(kernel.document().entity(other).is_ok());
    run(&mut kernel, "undo", "");
    // Cut (copy, then delete) pastes back as it was.
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [vector], "components": [shape] }).to_string(),
    );
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [vector], "components": [shape] }).to_string(),
    );
    let pasted = run(
        &mut kernel,
        "clipboard.paste",
        &json!({ "dx": 0.0, "dy": 0.0 }).to_string(),
    );
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], pasted["ids"]);
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
    // An unknown component refuses the whole delete.
    let refused = kernel.apply(
        "entities.delete",
        &json!({ "ids": [other], "components": [9999] }).to_string(),
    );
    assert!(refused.is_err());
    assert!(kernel.document().entity(other).is_ok());
}

#[test]
fn a_shape_pasted_without_its_vectors_keeps_them_only_in_the_same_job() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "components": [shape] }).to_string(),
    );
    // Same job, vectors still there: the copy is still made from them.
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], json!([vector]));
    // Same job, vectors deleted: the copy is a plain component.
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [vector] }).to_string(),
    );
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert!(shape_source(&mut kernel, copy).is_null());
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    // Another job, where the same id holds an unrelated vector: the copy
    // stays where it was and is not linked to that vector.
    run(&mut kernel, "doc.new", "{}");
    let mut stranger = make_rect(&mut kernel, 10.0, 10.0, 30.0, 30.0);
    while stranger < vector {
        stranger = make_rect(&mut kernel, 10.0, 10.0, 30.0, 30.0);
    }
    assert!(kernel.document().entity(vector).is_ok());
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert!(shape_source(&mut kernel, copy).is_null());
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < 1e-2);
    // Pasted with its vectors into the other job, it is made from them.
    run(&mut kernel, "doc.new", "{}");
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [vector], "components": [shape] }).to_string(),
    );
    run(&mut kernel, "doc.new", "{}");
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert_eq!(shape_source(&mut kernel, copy)["vectors"], pasted["ids"]);
}

#[test]
fn vectors_moved_alone_leave_their_shape_where_it_lies() {
    let mut kernel = metric_kernel();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    // A shape on its vectors: the vectors move up on their own.
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [vector], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 60.0] }).to_string(),
    );
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!(
        (composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP,
        "the rebuild stays where the component lies"
    );
    assert!(composite_at(&kernel, 120.0, 160.0).abs() < 1e-6);
    // Moved off its vectors too, then the vectors move again: the rebuild
    // still lands where the component was put.
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [shape], "matrix": [1.0, 0.0, 0.0, 1.0, 50.0, 0.0] }).to_string(),
    );
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [vector], "matrix": [0.0, 1.0, -1.0, 0.0, 300.0, 0.0] }).to_string(),
    );
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!((composite_at(&kernel, 170.0, 100.0) - top).abs() < ON_TOP);
    assert!(composite_at(&kernel, 120.0, 100.0).abs() < 1e-6);
    // Undo takes the vectors' move and its record together.
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!((composite_at(&kernel, 170.0, 100.0) - top).abs() < ON_TOP);
    // Moved with the component in one user step, both calls in either
    // order, the shape lies where it lay relative to them.
    let right = [1.0, 0.0, 0.0, 1.0, 0.0, -40.0];
    run(
        &mut kernel,
        "model.transformMatrix",
        &json!({ "ids": [shape], "matrix": right }).to_string(),
    );
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [vector], "matrix": right }).to_string(),
    );
    rebuild(&mut kernel, vector, shape, 45.0);
    assert!((composite_at(&kernel, 170.0, 60.0) - top).abs() < ON_TOP);
}

// Review 023.

#[test]
fn copy_along_of_components_alone_leaves_the_guide_alone() {
    let mut kernel = Kernel::new();
    let (_vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    let guide = line_entity(&mut kernel, (50.0, 150.0), (250.0, 150.0));
    // The form picks the guide last; with only components besides it the
    // app sends no other vectors.
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [guide] }).to_string(),
    );
    let reply = run(
        &mut kernel,
        "ops.copyAlong",
        &json!({ "ids": [], "components": [shape], "pathId": guide, "count": 2,
                 "alignToCurve": false, "deleteOriginal": true })
        .to_string(),
    );
    assert!(ids_in(&reply, "ids").is_empty(), "the guide was copied");
    assert!(
        kernel.document().entity(guide).is_ok(),
        "the guide was deleted"
    );
    assert!((composite_at(&kernel, 50.0, 150.0) - top).abs() < ON_TOP);
    assert!((composite_at(&kernel, 250.0, 150.0) - top).abs() < ON_TOP);
}

#[test]
fn a_texture_area_pasted_into_another_job_keeps_its_own_tiles() {
    let mut kernel = Kernel::new();
    let square = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let seed = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [square], "params": { "profile": "flat", "height": 2.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a seed");
    let area = make_rect(&mut kernel, 0.0, 0.0, 100.0, 60.0);
    let texture = run(
        &mut kernel,
        "model.textureArea",
        &json!({ "component": seed, "ids": [area],
                 "params": { "spacingX": 100.0, "spacingY": 100.0 } })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("an area");
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "components": [texture] }).to_string(),
    );
    // Another job where the seed's id holds an unrelated, taller shape.
    run(&mut kernel, "doc.new", "{}");
    let mut other = 0;
    while other < seed {
        let r = make_rect(&mut kernel, 150.0, 150.0, 190.0, 190.0);
        other = run(
            &mut kernel,
            "model.createShape",
            &json!({ "ids": [r], "params": { "profile": "flat", "height": 9.0 } }).to_string(),
        )["id"]
            .as_u64()
            .expect("a shape");
    }
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert!(
        (composite_at(&kernel, 35.0, 15.0) - 2.0).abs() < 1e-6,
        "tiled from the stranger"
    );
    let list = run(&mut kernel, "model.list", "");
    let listed = list["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["id"].as_u64() == Some(copy))
        .expect("the copy");
    assert!(listed["texture"].is_null(), "still linked to the stranger");
}

#[test]
fn a_shape_copied_alone_whose_vectors_moved_is_pasted_where_it_was() {
    let mut kernel = Kernel::new();
    let (vector, shape) = round_shape(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let top = composite_at(&kernel, 120.0, 100.0);
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "components": [shape] }).to_string(),
    );
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [vector], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 60.0] }).to_string(),
    );
    run(
        &mut kernel,
        "model.delete",
        &json!({ "ids": [shape] }).to_string(),
    );
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert!((composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP);
    // Edited again, the copy stays where it was pasted.
    if !shape_source(&mut kernel, copy).is_null() {
        rebuild(&mut kernel, vector, copy, 45.0);
    }
    assert!(
        (composite_at(&kernel, 120.0, 100.0) - top).abs() < ON_TOP,
        "the copy jumped"
    );
    assert!(composite_at(&kernel, 120.0, 160.0).abs() < 1e-6);
}

#[test]
fn a_component_pasted_into_another_job_joins_no_group_there() {
    let mut kernel = Kernel::new();
    let (_v, a) = round_shape(&mut kernel, 10.0, 10.0, 30.0, 30.0);
    let (_w, b) = round_shape(&mut kernel, 50.0, 10.0, 70.0, 30.0);
    run(
        &mut kernel,
        "model.groups.create",
        &json!({ "ids": [a, b] }).to_string(),
    );
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "components": [a] }).to_string(),
    );
    run(&mut kernel, "doc.new", "{}");
    let (_x, c) = round_shape(&mut kernel, 110.0, 10.0, 130.0, 30.0);
    let (_y, d) = round_shape(&mut kernel, 150.0, 10.0, 170.0, 30.0);
    let group = run(
        &mut kernel,
        "model.groups.create",
        &json!({ "ids": [c, d] }).to_string(),
    )["id"]
        .as_u64()
        .expect("a group");
    let pasted = run(&mut kernel, "clipboard.paste", "{}");
    let copy = pasted["components"][0].as_u64().expect("pasted");
    assert_eq!(
        kernel
            .document()
            .component(copy)
            .map(|c| c.extra.group)
            .ok(),
        Some(0),
        "joined group {group} of the other job"
    );
}

#[test]
fn a_level_mirror_mode_mirrors_its_model_live_and_undoes() {
    let mut kernel = Kernel::new();
    // Job 300 x 200 from the bottom left: the middle is x = 150.
    let id = make_rect(&mut kernel, 40.0, 80.0, 80.0, 120.0);
    run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    );
    let level = run(&mut kernel, "model.list", "")["levels"][0]["id"]
        .as_u64()
        .expect("a level");
    let mirrored = Point::new(240.0, 100.0);
    assert!(kernel.document().composite().at(mirrored).abs() < 1e-6);
    run(
        &mut kernel,
        "model.levels.update",
        &json!({ "id": level, "mirror": "leftToRight" }).to_string(),
    );
    let at = kernel.document().composite().at(mirrored);
    assert!((at - 3.0).abs() < 1e-3, "{at}");
    assert_eq!(
        run(&mut kernel, "model.list", "")["levels"][0]["mirror"],
        "leftToRight"
    );
    // Only one component: the mirror is the level's, not a copy.
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    run(
        &mut kernel,
        "model.levels.update",
        &json!({ "id": level, "mirror": null }).to_string(),
    );
    assert!(kernel.document().composite().at(mirrored).abs() < 1e-6);
    run(&mut kernel, "undo", "");
    assert!((kernel.document().composite().at(mirrored) - 3.0).abs() < 1e-3);
}

#[test]
fn a_level_base_height_raises_its_components_and_scales_with_the_model() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 40.0, 80.0, 80.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    let level = run(&mut kernel, "model.list", "")["levels"][0]["id"]
        .as_u64()
        .expect("a level");
    let at = Point::new(60.0, 100.0);
    let outside = Point::new(200.0, 100.0);
    run(
        &mut kernel,
        "model.levels.update",
        &json!({ "id": level, "baseHeight": 2.0 }).to_string(),
    );
    let raised = kernel.document().composite().at(at);
    assert!((raised - 5.0).abs() < 1e-3, "{raised}");
    assert!(kernel.document().composite().at(outside).abs() < 1e-6);
    assert_eq!(
        run(&mut kernel, "model.list", "")["levels"][0]["baseHeight"],
        2.0
    );
    run(
        &mut kernel,
        "model.scaleHeight",
        &json!({ "percent": 200.0 }).to_string(),
    );
    let scaled = kernel.document().composite().at(at);
    assert!((scaled - 10.0).abs() < 1e-3, "{scaled}");
    // A bake takes the component without the level's base.
    let baked = run(
        &mut kernel,
        "model.bake",
        &json!({ "ids": [component] }).to_string(),
    )["id"]
        .as_u64()
        .expect("baked");
    let top = kernel
        .document()
        .component(baked)
        .expect("baked")
        .field
        .max();
    assert!(top.is_some_and(|t| (t - 6.0).abs() < 1e-3), "{top:?}");
    assert!(kernel
        .apply(
            "model.levels.update",
            &json!({ "id": level, "baseHeight": "x" }).to_string()
        )
        .is_err());
}

#[test]
fn components_move_or_copy_to_the_other_side_turned_over() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 10.0, 80.0, 40.0, 120.0);
    let component = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    // A single sided job has no other side.
    assert!(kernel
        .apply(
            "model.toOtherSide",
            &json!({ "ids": [component] }).to_string()
        )
        .is_err());
    let job = crate::doc::JobSetup {
        kind: crate::doc::JobKind::DoubleSided,
        ..crate::doc::JobSetup::default()
    };
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    run(
        &mut kernel,
        "model.toOtherSide",
        &json!({ "ids": [component], "copy": true }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the copy leaves the original"
    );
    run(
        &mut kernel,
        "model.toOtherSide",
        &json!({ "ids": [component] }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "the move takes it away"
    );
    run(
        &mut kernel,
        "job.setSide",
        &json!({ "side": "bottom" }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    // Turned over left to right on a 300 wide job: x 10..40 lands on
    // 260..290, the copy and the moved one adding up there.
    let composite = kernel.document().composite();
    let both = composite.at(Point::new(275.0, 100.0));
    assert!((both - 6.0).abs() < 1e-3, "{both}");
    assert!(composite.at(Point::new(25.0, 100.0)).abs() < 1e-6);
    run(
        &mut kernel,
        "job.setSide",
        &json!({ "side": "top" }).to_string(),
    );
    // Switching sides is a view change: Undo takes back the move.
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Move to Other Side");
    assert_eq!(
        run(&mut kernel, "model.list", "")["components"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
}
