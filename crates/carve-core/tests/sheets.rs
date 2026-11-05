//! Sheets through the kernel's command interface: each sheet owns its
//! components and its material, Duplicate Sheet copies both, and Move to
//! Sheet takes components along with vectors.

use carve_core::geom::Point;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

fn run(kernel: &mut Kernel, name: &str, args: Value) -> Value {
    match kernel
        .apply(name, &args.to_string())
        .unwrap_or_else(|error| panic!("{name} failed: {error}"))
    {
        KernelReply::Json(text) => {
            let value: Value = serde_json::from_str(&text).expect("JSON");
            assert_eq!(value["ok"], true, "{value}");
            value["result"].clone()
        }
        KernelReply::Bytes(_) => panic!("{name} answered with bytes"),
    }
}

fn square(kernel: &mut Kernel, x: f64) -> u64 {
    let path =
        carve_core::geom::rect(Point::new(x, 20.0), Point::new(x + 40.0, 60.0)).expect("rect");
    let entity = json!({ "entity": { "kind": "path", "path": path } });
    run(kernel, "entities.create", entity)["id"]
        .as_u64()
        .expect("an id")
}

fn shape(kernel: &mut Kernel, vector: u64) -> u64 {
    run(
        kernel,
        "model.createShape",
        json!({ "ids": [vector], "params": { "profile": "flat", "height": 3.0 } }),
    )["id"]
        .as_u64()
        .expect("an id")
}

fn names(kernel: &mut Kernel) -> Vec<String> {
    run(kernel, "model.list", json!({}))["components"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|c| c["name"].as_str().map(str::to_string))
        .collect()
}

fn height_at(kernel: &Kernel, x: f64) -> f32 {
    kernel.document().composite().at(Point::new(x, 40.0))
}

#[test]
fn switching_sheets_shows_that_sheets_components() {
    let mut kernel = Kernel::new();
    let vector = square(&mut kernel, 20.0);
    shape(&mut kernel, vector);
    let second = run(
        &mut kernel,
        "sheets.add",
        json!({ "name": "Offcut", "activate": true }),
    )["id"]
        .as_u64()
        .expect("sheet");
    assert!(names(&mut kernel).is_empty(), "the new sheet has no model");
    assert_eq!(height_at(&kernel, 40.0), 0.0);
    let other = square(&mut kernel, 100.0);
    shape(&mut kernel, other);
    // Names count the components of the sheet on show.
    assert_eq!(names(&mut kernel), vec!["Shape 1".to_string()]);
    assert_eq!(height_at(&kernel, 120.0), 3.0);
    let sheets = run(&mut kernel, "sheets.list", json!({}));
    assert_eq!(sheets[0]["components"], 1, "{sheets}");
    assert_eq!(sheets[1]["components"], 1, "{sheets}");
    run(&mut kernel, "sheets.setActive", json!({ "id": 1 }));
    assert_eq!(names(&mut kernel), vec!["Shape 1".to_string()]);
    assert_eq!(height_at(&kernel, 40.0), 3.0);
    assert_eq!(height_at(&kernel, 120.0), 0.0);
    run(&mut kernel, "sheets.delete", json!({ "id": second }));
    assert_eq!(kernel.document().all_components().len(), 1);
}

#[test]
fn duplicate_sheet_copies_components_and_move_takes_them_along() {
    let mut kernel = Kernel::new();
    let vector = square(&mut kernel, 20.0);
    let component = shape(&mut kernel, vector);
    let copy = run(&mut kernel, "sheets.duplicate", json!({ "id": 1 }))["id"]
        .as_u64()
        .expect("sheet");
    assert_eq!(names(&mut kernel), vec!["Shape 1".to_string()]);
    assert_eq!(height_at(&kernel, 40.0), 3.0);
    assert_eq!(kernel.document().all_components().len(), 2);

    run(&mut kernel, "sheets.setActive", json!({ "id": 1 }));
    run(
        &mut kernel,
        "entities.setSheet",
        json!({ "ids": [vector], "sheet": copy, "components": [component] }),
    );
    assert!(names(&mut kernel).is_empty());
    assert!(run(&mut kernel, "entities.list", json!({}))
        .as_array()
        .is_some_and(Vec::is_empty));
    run(&mut kernel, "undo", json!({}));
    assert_eq!(names(&mut kernel), vec!["Shape 1".to_string()]);
}

#[test]
fn each_sheet_keeps_its_own_size() {
    let mut kernel = Kernel::new();
    let job = run(&mut kernel, "job.get", json!({}));
    let second = run(
        &mut kernel,
        "sheets.add",
        json!({ "name": "Small", "activate": false }),
    )["id"]
        .as_u64()
        .expect("sheet");
    let mut material = run(&mut kernel, "sheets.list", json!({}))[1]["material"].clone();
    assert_eq!(material["width"], job["width"]);
    material["width"] = json!(120.0);
    material["thickness"] = json!(6.0);
    run(
        &mut kernel,
        "sheets.edit",
        json!({ "ids": [second], "material": material }),
    );
    assert_eq!(
        run(&mut kernel, "job.get", json!({}))["width"],
        job["width"]
    );
    run(&mut kernel, "sheets.setActive", json!({ "id": second }));
    let small = run(&mut kernel, "job.get", json!({}));
    assert_eq!(small["width"], 120.0);
    assert_eq!(small["thickness"], 6.0);
    run(&mut kernel, "undo", json!({}));
    assert_eq!(
        run(&mut kernel, "job.get", json!({}))["width"],
        job["width"]
    );
    let bad = json!({ "ids": [second], "material": { "width": -1.0 } });
    assert!(kernel.apply("sheets.edit", &bad.to_string()).is_err());
}
