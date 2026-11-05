//! Furniture designs through the kernel: the Modeling tools on a furniture
//! panel, and what they make staying on that panel when the design
//! changes.

use super::*;

fn json_args(value: &Value) -> String {
    value.to_string()
}

/// The design `id` as stored.
fn design(kernel: &Kernel, id: u64) -> crate::furniture::FurnitureDesign {
    kernel
        .document()
        .assembly()
        .designs
        .iter()
        .find(|d| d.id == id)
        .cloned()
        .expect("design")
}

/// A closed rectangle on `sheet`.
fn rect_on(kernel: &mut Kernel, sheet: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
    let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
    let layer = kernel.document().active_layer();
    run(
        kernel,
        "entities.create",
        &json_args(
            &json!({ "entity": { "kind": "path", "layer": layer, "sheet": sheet, "path": path } }),
        ),
    )["id"]
        .as_u64()
        .expect("an id")
}

fn component_box(kernel: &Kernel, id: u64) -> BBox {
    crate::furniture::attach::component_box(kernel.document(), id).expect("component")
}

#[test]
fn modeling_tools_on_a_furniture_panel_stay_on_it_when_the_design_changes() {
    let (_temp, mut kernel) = temp_kernel("furniture-sweep");
    {
        let doc = kernel.document_mut();
        doc.begin("job");
        doc.set_job(JobSetup {
            width: 2440.0,
            height: 1220.0,
            thickness: 18.0,
            ..JobSetup::default()
        })
        .expect("job");
        doc.commit();
    }
    let reply = run(
        &mut kernel,
        "furniture.create",
        &json_args(
            &json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
        ),
    );
    let id = reply["design"].as_u64().expect("id");
    let made = design(&kernel, id).made["left"].clone();
    run(
        &mut kernel,
        "sheets.setActive",
        &json_args(&json!({ "id": made.sheet })),
    );
    let o = kernel.document().bbox_of(made.vector).expect("left");
    // A vector drawn on the panel, and a shape made from it.
    let (cx, cy) = ((o.min.x + o.max.x) / 2.0, (o.min.y + o.max.y) / 2.0);
    let (hw, hh) = (
        o.width().min(o.height()) / 4.0,
        o.width().min(o.height()) / 4.0,
    );
    let outline = rect_on(&mut kernel, made.sheet, cx - hw, cy - hh, cx + hw, cy + hh);
    let shape = run(
        &mut kernel,
        "model.createShape",
        &json_args(&json!({ "ids": [outline], "params": { "profile": "round", "angle": 30.0 } })),
    )["id"]
        .as_u64()
        .expect("shape");
    // Edits that keep the component: smooth, tilt, fade and scale.
    for (op, params) in [
        ("smooth", json!({})),
        (
            "tilt",
            json!({ "angleDeg": 5.0, "directionDeg": 0.0, "pivot": { "x": cx, "y": cy } }),
        ),
        ("fade", json!({})),
        ("scale", json!({ "scale": 0.8, "baseOffset": 0.0 })),
    ] {
        run(
            &mut kernel,
            "model.edit",
            &json_args(&json!({ "id": shape, "op": op, "params": params })),
        );
    }
    // Position and orientate: a quarter turn about its middle.
    run(
        &mut kernel,
        "model.transform",
        &json_args(
            &json!({ "id": shape, "params": { "offset": { "x": 0.0, "y": 0.0 }, "scaleX": 1.0, "scaleY": 1.0, "rotateDeg": 90.0 } }),
        ),
    );
    // A second shape, flat, then the two baked into one.
    let small = rect_on(
        &mut kernel,
        made.sheet,
        cx - 10.0,
        cy - 10.0,
        cx + 10.0,
        cy + 10.0,
    );
    let flat = run(
        &mut kernel,
        "model.createShape",
        &json_args(&json!({ "ids": [small], "params": { "profile": "flat", "height": 2.0 } })),
    )["id"]
        .as_u64()
        .expect("flat");
    // A texture area of the flat shape over the drawn rectangle.
    let texture = run(
        &mut kernel,
        "model.textureArea",
        &json_args(&json!({ "component": flat, "ids": [outline] })),
    )["id"]
        .as_u64()
        .expect("texture");
    run(
        &mut kernel,
        "model.bake",
        &json_args(&json!({ "ids": [shape, flat] })),
    );
    // What is on the panel now: components and the maker's vectors.
    let doc = kernel.document();
    let comps: Vec<u64> = doc
        .all_components()
        .iter()
        .filter(|c| doc.sheet_of_component(c) == made.sheet)
        .map(|c| c.id)
        .collect();
    assert!(comps.len() >= 2, "{comps:?}");
    assert!(comps.contains(&texture));
    // Wider and taller: everything is captured and follows the panel.
    let before: Vec<(u64, [f64; 2])> = comps
        .iter()
        .map(|c| {
            let b = component_box(&kernel, *c);
            (*c, [b.width(), b.height()])
        })
        .collect();
    let texture_before = component_box(&kernel, texture);
    let reply = run(
        &mut kernel,
        "furniture.update",
        &json_args(&json!({ "design": id, "values": { "width": 1100, "height": 2100 } })),
    );
    let d = design(&kernel, id);
    let left = d.made["left"].clone();
    let o = kernel.document().bbox_of(left.vector).expect("left");
    for (c, size) in before {
        let a = d
            .attachments
            .iter()
            .find(|a| a.id == c && a.kind == crate::furniture::attach::AttachKind::Component)
            .unwrap_or_else(|| panic!("component {c} not attached: {:?}", d.attachments));
        assert_eq!(a.panel, "left");
        let comp = kernel.document().component(c).expect("component");
        assert_eq!(kernel.document().sheet_of_component(comp), left.sheet);
        let b = component_box(&kernel, c);
        let cell = comp.field.cell;
        assert!(
            b.min.x >= o.min.x - cell
                && b.max.x <= o.max.x + cell
                && b.min.y >= o.min.y - cell
                && b.max.y <= o.max.y + cell,
            "component {c} {b:?} off the panel {o:?}"
        );
        assert!(
            (b.width() - size[0]).abs() <= 2.0 * cell && (b.height() - size[1]).abs() <= 2.0 * cell
        );
    }
    // The drawn vectors went with it.
    for v in [outline, small] {
        assert!(d.attachments.iter().any(|a| a.id == v), "vector {v}");
        let b = kernel.document().bbox_of(v).expect("vector");
        assert!(
            b.min.x >= o.min.x && b.max.x <= o.max.x && b.min.y >= o.min.y && b.max.y <= o.max.y
        );
    }
    // No model is said to be off its panel; the left side's inside is
    // still cut from the top.
    let issues = reply["summary"]["issues"].as_array().expect("issues");
    assert!(issues.is_empty(), "{issues:?}");
    let list = run(
        &mut kernel,
        "furniture.attachments",
        &json_args(&json!({ "design": id })),
    );
    assert!(list["attachments"]
        .as_array()
        .expect("rows")
        .iter()
        .all(|r| r["face"] == "top"));
    // The preview shows the models on the side's inside face.
    let preview = run(
        &mut kernel,
        "furniture.preview",
        &json_args(&json!({ "design": id })),
    );
    let p = preview["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .find(|p| p["key"] == "left")
        .expect("left")
        .clone();
    // Carving only reaches the panel's own stock: the models' relief never
    // stands past its thickness (18).
    let reach = p["box"]["max"][0].as_f64().expect("box");
    assert!(reach <= 18.0 + 1e-6, "{reach}");
    // One undo takes the change back with everything on the panel.
    let moved = component_box(&kernel, texture);
    assert_ne!(moved, texture_before, "the panel moved on its sheet");
    run(&mut kernel, "undo", "");
    assert_eq!(component_box(&kernel, texture), texture_before);
    assert_eq!(design(&kernel, id).values["width"], json!(800.0));
}
