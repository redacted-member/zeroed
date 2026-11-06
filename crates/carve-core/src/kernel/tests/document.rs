//! Document, drawing, vector and file commands through the kernel.

use super::*;

#[test]
fn duplicating_a_layer_copies_its_objects_onto_a_new_layer() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let copy = run(&mut kernel, "layers.duplicate", r#"{"id":1}"#)["id"]
        .as_u64()
        .expect("an id came back");
    let layers = run(&mut kernel, "layers.list", "");
    let added = layers
        .as_array()
        .and_then(|list| list.iter().find(|l| l["id"] == copy))
        .expect("the copy is listed")
        .clone();
    assert_eq!(added["name"], "Layer 1 copy");
    assert_eq!(added["count"], 2);
    assert_eq!(kernel.document().order().len(), 4);
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().order().len(), 2);
    assert_eq!(kernel.document().layers().len(), 1);
    let error = kernel
        .apply("layers.duplicate", r#"{"id":99}"#)
        .expect_err("no such layer");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn text_can_be_listed_previewed_and_created() {
    let mut kernel = Kernel::new();
    let fonts = run(&mut kernel, "text.fonts", "");
    let families = fonts["families"].as_array().expect("a list of families");
    assert!(!families.is_empty());
    let family = families[0].as_str().expect("a name").to_string();
    // The single line fonts are listed and draw their letters as open strokes.
    let single = fonts["singleLine"]
        .as_array()
        .expect("single line families");
    assert!(single.contains(&json!("Single Line Sans")), "{single:?}");
    let engraved = run(
        &mut kernel,
        "text.preview",
        &json!({ "kind": "plain", "spec": { "text": "AB", "fontFamily": "Single Line Sans", "sizeMm": 20.0 }, "x": 0.0, "y": 0.0 }).to_string(),
    );
    let subpaths: Vec<&serde_json::Value> = engraved["paths"]
        .as_array()
        .expect("paths")
        .iter()
        .flat_map(|p| p["subpaths"].as_array().expect("subpaths").iter())
        .collect();
    assert!(!subpaths.is_empty());
    assert!(
        subpaths.iter().all(|s| s["closed"] == json!(false)),
        "{subpaths:?}"
    );
    let spec = json!({ "text": "H", "fontFamily": family, "sizeMm": 20.0 });
    let before = kernel.document().revision();
    let preview = run(
        &mut kernel,
        "text.preview",
        &json!({ "kind": "plain", "spec": spec, "x": 10.0, "y": 5.0 }).to_string(),
    );
    assert!(!preview["paths"].as_array().expect("paths").is_empty());
    let heights: Vec<f64> = serde_json::from_value::<Vec<Path>>(preview["paths"].clone())
        .expect("paths parse")
        .iter()
        .filter_map(geom::path_bbox)
        .map(|b| b.height())
        .collect();
    let tallest = heights.iter().copied().fold(0.0, f64::max);
    assert!(
        (tallest - 20.0).abs() < 1.0,
        "an H of 20 mm text height is 20 mm tall, got {tallest}"
    );
    assert_eq!(
        kernel.document().revision(),
        before,
        "a preview changes nothing"
    );
    let arc = run(
        &mut kernel,
        "text.preview",
        &json!({ "kind": "arc", "spec": spec, "cx": 0.0, "cy": 0.0, "radius": 50.0 }).to_string(),
    );
    assert!(!arc["paths"].as_array().expect("paths").is_empty());
    let created = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec, "x": 10.0, "y": 5.0 }).to_string(),
    );
    let id = created["id"].as_u64().expect("an id");
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["kind"], "text");
    let error = kernel
        .apply("text.preview", r#"{"kind":"sideways"}"#)
        .expect_err("unknown kind");
    assert!(matches!(error, CoreError::InvalidArgs { .. }), "{error}");
}

#[test]
fn ping_reports_the_kernel_version() {
    let mut kernel = Kernel::new();
    let result = run(&mut kernel, "ping", "");
    assert_eq!(result["pong"], Value::Bool(true));
    assert_eq!(result["version"], crate::VERSION);
}

#[test]
fn job_get_returns_the_default_job() {
    let mut kernel = Kernel::new();
    let result = run(&mut kernel, "job.get", "null");
    let width = result["width"].as_f64().expect("a width");
    assert!((width - 12.0 * 25.4).abs() < 1e-9, "{width}");
    assert_eq!(result["units"], "inch");
    assert_eq!(result["rotary"]["axis"], "y");
    let diameter = result["rotary"]["diameter"].as_f64().expect("a diameter");
    assert!((diameter - 4.0 * 25.4).abs() < 1e-9, "{diameter}");
    let safe = result["machine"]["safeZ"].as_f64().expect("a safe Z");
    assert!((safe - 0.25 * 25.4).abs() < 1e-9, "{safe}");
}

#[test]
fn job_set_stores_the_job_and_echoes_it_with_the_revision() {
    let mut kernel = Kernel::new();
    let job = JobSetup {
        units: Units::Inch,
        width: 609.6,
        height: 304.8,
        thickness: 19.05,
        z_zero: ZZero::Bottom,
        xy_origin: XyOrigin::Centre,
        offset_x: 1.5,
        offset_y: -2.5,
        kind: JobKind::DoubleSided,
        ..JobSetup::default()
    };
    let args = serde_json::to_string(&job).expect("job serialises");
    let result = run(&mut kernel, "job.set", &args);
    assert_eq!(result["width"], 609.6);
    assert_eq!(result["zZero"], "bottom");
    assert_eq!(result["kind"], "doubleSided");
    assert_eq!(result["revision"], 1);
    assert_eq!(*kernel.document().job(), job);
}

#[test]
fn job_set_rejects_an_unmachinable_job() {
    let mut kernel = Kernel::new();
    let args = r#"{"units":"mm","width":0,"height":200,"thickness":18,"zZero":"top",
        "xyOrigin":"bottomLeft","offsetX":0,"offsetY":0,"kind":"singleSided",
        "rotary":{"diameter":100,"axis":"x"}}"#;
    let error = kernel
        .apply("job.set", args)
        .expect_err("zero width is rejected");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    assert!((kernel.document().job().width - 12.0 * 25.4).abs() < 1e-9);
    assert_eq!(kernel.document().revision(), 0);
}

#[test]
fn job_set_reports_bad_json() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("job.set", "{\"width\":}")
        .expect_err("broken JSON is rejected");
    assert!(matches!(error, CoreError::InvalidArgs { .. }), "{error}");
}

#[test]
fn doc_new_resets_the_document_and_keeps_revisions_monotonic() {
    let mut kernel = Kernel::new();
    let job = JobSetup {
        width: 1000.0,
        ..JobSetup::default()
    };
    let args = serde_json::to_string(&job).expect("job serialises");
    run(&mut kernel, "job.set", &args);
    let result = run(&mut kernel, "doc.new", "");
    assert_eq!(result["revision"], 2);
    assert_eq!(kernel.document().job(), Kernel::new().document().job());
}

#[test]
fn unknown_commands_are_reported() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("job.explode", "null")
        .expect_err("unknown command is rejected");
    assert!(matches!(error, CoreError::UnknownCommand(_)), "{error}");
}

#[test]
fn call_wraps_errors_in_the_failure_envelope() {
    let mut kernel = Kernel::new();
    let value = envelope(kernel.call("job.explode", "null"));
    assert_eq!(value["ok"], Value::Bool(false));
    assert_eq!(value["error"]["code"], "unknown_command");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("job.explode"),
        "{value}"
    );
}

#[test]
fn doc_info_reports_what_the_shell_mirrors() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let info = run(&mut kernel, "doc.info", "");
    assert_eq!(info["activeLayer"], 1);
    assert_eq!(info["activeSheet"], 1);
    assert_eq!(info["counts"]["entities"], 1);
    assert_eq!(info["dirty"], Value::Bool(true));
    assert_eq!(info["path"], Value::Null);
}

#[test]
fn layers_list_marks_the_active_layer_and_counts_its_objects() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let layers = run(&mut kernel, "layers.list", "");
    assert_eq!(layers[0]["active"], Value::Bool(true));
    assert_eq!(layers[0]["count"], 1);
    assert_eq!(layers[0]["colour"], "#000000");
}

#[test]
fn a_layer_can_be_added_updated_and_deleted() {
    let mut kernel = Kernel::new();
    let id = run(&mut kernel, "layers.add", r#"{"name":"Cut"}"#)["id"]
        .as_u64()
        .expect("id");
    run(
        &mut kernel,
        "layers.update",
        &json!({ "id": id, "colour": "#112233", "locked": true }).to_string(),
    );
    let layers = run(&mut kernel, "layers.list", "");
    assert_eq!(layers[1]["colour"], "#112233");
    assert_eq!(layers[1]["locked"], Value::Bool(true));
    run(
        &mut kernel,
        "layers.delete",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(kernel.document().layers().len(), 1);
}

#[test]
fn a_layer_can_be_added_hidden_active_and_before_another_in_one_step() {
    let mut kernel = Kernel::new();
    let second = run(&mut kernel, "layers.add", r#"{"name":"Second"}"#)["id"]
        .as_u64()
        .expect("id");
    let id = run(
        &mut kernel,
        "layers.add",
        &json!({ "name": "Inserted", "visible": false, "active": true, "before": second })
            .to_string(),
    )["id"]
        .as_u64()
        .expect("id");
    let layers = run(&mut kernel, "layers.list", "");
    let names: Vec<&str> = layers
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|l| l["name"].as_str())
        .collect();
    assert_eq!(names, ["Layer 1", "Inserted", "Second"]);
    assert_eq!(layers[1]["id"], json!(id));
    assert_eq!(layers[1]["visible"], Value::Bool(false));
    assert_eq!(layers[1]["active"], Value::Bool(true));
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().layers().len(), 2);
    assert!(kernel.apply("layers.add", r#"{"before":99}"#).is_err());
}

#[test]
fn a_bad_layer_colour_is_refused() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("layers.add", r#"{"name":"Cut","colour":"red"}"#)
        .expect_err("a name is not a colour");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    assert_eq!(kernel.document().layers().len(), 1);
}

#[test]
fn sheets_can_be_listed_added_and_made_active() {
    let mut kernel = Kernel::new();
    let id = run(&mut kernel, "sheets.add", r#"{"name":"Offcut"}"#)["id"]
        .as_u64()
        .expect("id");
    run(
        &mut kernel,
        "sheets.setActive",
        &json!({ "id": id }).to_string(),
    );
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(sheets[1]["name"], "Offcut");
    assert_eq!(sheets[1]["active"], Value::Bool(true));
}

#[test]
fn creating_an_entity_answers_with_its_id_and_lists_it() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 20.0);
    let list = run(&mut kernel, "entities.list", "");
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["kind"], "path");
    assert_eq!(list[0]["closed"], Value::Bool(true));
    assert_eq!(list[0]["bbox"], json!([0.0, 0.0, 10.0, 20.0]));
}

#[test]
fn creating_on_a_layer_that_does_not_exist_is_refused() {
    let mut kernel = Kernel::new();
    let path = rect(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).expect("rect");
    let args = json!({ "entity": { "kind": "path", "layer": 99, "sheet": 1, "path": path } });
    let error = kernel
        .apply("entities.create", &args.to_string())
        .expect_err("no such layer");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    assert!(kernel.document().order().is_empty());
}

#[test]
fn entities_get_answers_with_the_full_entity() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["kind"], "path");
    assert_eq!(entity["id"], id);
    assert!(entity["path"]["subpaths"].is_array());
}

#[test]
fn the_display_reply_is_a_binary_blob() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let reply = kernel.apply("entities.display", "{}").expect("display");
    let KernelReply::Bytes(body) = reply else {
        panic!("expected a binary reply");
    };
    let (header, xy) = crate::doc::display::decode(&body).expect("decodes");
    assert_eq!(header.entities.len(), 1);
    assert_eq!(xy.len(), 8);
}

#[test]
fn selection_commands_answer_with_the_ids_and_the_box() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let b = make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let state = run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [a] }).to_string(),
    );
    assert_eq!(state["ids"], json!([a]));
    let state = run(
        &mut kernel,
        "selection.add",
        &json!({ "ids": [b] }).to_string(),
    );
    assert_eq!(state["ids"], json!([a, b]));
    assert_eq!(state["bbox"], json!([0.0, 0.0, 30.0, 10.0]));
    let state = run(&mut kernel, "selection.clear", "");
    assert_eq!(state["bbox"], Value::Null);
}

#[test]
fn hit_test_finds_the_vector_under_a_click() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let hit = run(
        &mut kernel,
        "entities.hitTest",
        &json!({ "x": 0.1, "y": 5.0, "tolerance": 0.5 }).to_string(),
    );
    assert_eq!(hit["ids"], json!([id]));
    let miss = run(
        &mut kernel,
        "entities.hitTest",
        &json!({ "x": 5.0, "y": 5.0, "tolerance": 0.5 }).to_string(),
    );
    assert_eq!(miss["ids"], json!([]));
}

#[test]
fn a_rubber_band_catches_what_it_covers() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    make_rect(&mut kernel, 50.0, 0.0, 60.0, 10.0);
    let inside = run(
        &mut kernel,
        "entities.hitTest",
        &json!({ "rect": [-1.0, -1.0, 11.0, 11.0], "mode": "inside" }).to_string(),
    );
    assert_eq!(inside["ids"], json!([a]));
    let touch = run(
        &mut kernel,
        "entities.hitTest",
        &json!({ "rect": [5.0, 5.0, 55.0, 6.0], "mode": "touch" }).to_string(),
    );
    assert_eq!(touch["ids"].as_array().map(|a| a.len()), Some(2));
}

#[test]
fn probe_answers_in_the_shape_the_frontend_sends() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 100.0, 50.0);
    let args = json!({
        "x": 0.2, "y": 0.2, "radius": 2.0,
        "snap": { "grid": true, "guides": true, "nodes": true, "midpoints": true,
                  "centres": true, "intersections": true, "jobPoints": false,
                  "smart": false },
        "gridSpacing": 10.0,
        "guides": [],
        "exclude": [],
    });
    let reply = run(&mut kernel, "entities.probe", &args.to_string());
    assert_eq!(reply["hits"][0]["kind"], "node");
    assert!(reply["nearestEntity"].is_number());
}

#[test]
fn grouping_and_ungrouping_move_the_selection() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let b = make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let group = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [a, b] }).to_string(),
    );
    let id = group["id"].as_u64().expect("id");
    assert_eq!(kernel.document().selection(), &[id]);
    let out = run(
        &mut kernel,
        "entities.ungroup",
        &json!({ "ids": [id] }).to_string(),
    );
    assert_eq!(out["ids"], json!([a, b]));
}

#[test]
fn deep_ungroup_and_ungroup_to_the_group_layer() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let b = make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let c = make_rect(&mut kernel, 40.0, 0.0, 50.0, 10.0);
    let inner = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [a, b] }).to_string(),
    )["id"]
        .as_u64()
        .expect("inner");
    let outer = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [inner, c] }).to_string(),
    )["id"]
        .as_u64()
        .expect("outer");
    let cut = run(&mut kernel, "layers.add", r#"{"name":"Cut"}"#)["id"]
        .as_u64()
        .expect("layer");
    run(
        &mut kernel,
        "entities.setLayer",
        &json!({ "ids": [outer], "layer": cut }).to_string(),
    );
    // Children back on their own layer first: plain ungroup keeps them there.
    run(
        &mut kernel,
        "entities.setLayer",
        &json!({ "ids": [a, b, c], "layer": 1 }).to_string(),
    );
    let deep = run(
        &mut kernel,
        "entities.ungroup",
        &json!({ "ids": [outer], "deep": true, "toGroupLayer": true }).to_string(),
    );
    let mut released: Vec<u64> = deep["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    released.sort_unstable();
    assert_eq!(released, [a, b, c]);
    for id in [a, b, c] {
        assert_eq!(kernel.document().entity(id).expect("entity").layer, cut);
    }
    run(&mut kernel, "undo", "");
    let shallow = run(
        &mut kernel,
        "entities.ungroup",
        &json!({ "ids": [outer] }).to_string(),
    );
    let shallow = shallow["ids"].as_array().expect("ids");
    assert!(shallow.contains(&json!(inner)) && shallow.contains(&json!(c)));
    assert_eq!(shallow.len(), 2);
    assert_eq!(kernel.document().entity(c).expect("entity").layer, 1);
}

#[test]
fn undo_and_redo_walk_the_journal_with_their_labels() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    run(
        &mut kernel,
        "ops.move",
        &json!({ "ids": [id], "dx": 5.0, "dy": 0.0 }).to_string(),
    );
    let info = run(&mut kernel, "undo.info", "");
    assert_eq!(info["canUndo"], Value::Bool(true));
    assert_eq!(info["undoLabel"], "Move");
    let undone = run(&mut kernel, "undo", "");
    assert_eq!(undone["label"], "Move");
    let bbox = kernel.document().bbox_of(id).expect("bbox");
    assert!((bbox.min.x).abs() < 1e-9, "{bbox:?}");
    let redone = run(&mut kernel, "redo", "");
    assert_eq!(redone["label"], "Move");
    let bbox = kernel.document().bbox_of(id).expect("bbox");
    assert!((bbox.min.x - 5.0).abs() < 1e-9, "{bbox:?}");
}

#[test]
fn undo_with_nothing_to_undo_answers_with_a_null_label() {
    let mut kernel = Kernel::new();
    let result = run(&mut kernel, "undo", "");
    assert_eq!(result["label"], Value::Null);
}

#[test]
fn a_failed_command_leaves_the_document_alone() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let before = kernel.document().revision();
    let error = kernel
        .apply("ops.weld", &json!({ "ids": [id] }).to_string())
        .expect_err("one vector cannot be welded");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
    assert_eq!(kernel.document().revision(), before);
    assert!(kernel.document().entity(id).is_ok());
    assert_eq!(kernel.document().journal().undo_label(), Some("Create"));
}

#[test]
fn ops_mirror_answers_with_the_ids_it_mirrored() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let result = run(
        &mut kernel,
        "ops.mirror",
        &json!({ "ids": [id], "axis": "h", "copy": true }).to_string(),
    );
    let ids = result["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 1);
    assert_ne!(ids[0].as_u64(), Some(id));
    assert_eq!(kernel.document().order().len(), 2);
}

#[test]
fn ops_rotate_turns_about_the_point_it_is_given() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 20.0, 10.0);
    run(
        &mut kernel,
        "ops.rotate",
        &json!({ "ids": [id], "angleDeg": 90.0, "cx": 0.0, "cy": 0.0 }).to_string(),
    );
    let bbox = kernel.document().bbox_of(id).expect("bbox");
    assert!((bbox.width() - 10.0).abs() < 1e-6, "{bbox:?}");
}

#[test]
fn ops_act_on_the_selection_when_no_ids_are_given() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [id] }).to_string(),
    );
    run(
        &mut kernel,
        "ops.move",
        &json!({ "dx": 3.0, "dy": 4.0 }).to_string(),
    );
    let bbox = kernel.document().bbox_of(id).expect("bbox");
    assert!((bbox.min.x - 3.0).abs() < 1e-9, "{bbox:?}");
}

#[test]
fn ops_offset_answers_with_the_new_ids() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 40.0, 40.0);
    let result = run(
        &mut kernel,
        "ops.offset",
        &json!({ "ids": [id], "distance": 3.0, "side": "outward",
                 "corner": "round", "deleteOriginal": false })
        .to_string(),
    );
    let ids: Vec<u64> = result["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 1);
    let bbox = kernel.document().bbox_of(ids[0]).expect("bbox");
    assert!((bbox.width() - 46.0).abs() < 1e-3, "{bbox:?}");
}

#[test]
fn ops_validate_reports_an_open_vector() {
    let mut kernel = Kernel::new();
    let path = json!({
        "subpaths": [{
            "start": { "x": 0.0, "y": 0.0 },
            "segments": [{ "kind": "line", "to": { "x": 10.0, "y": 0.0 } }],
            "closed": false
        }]
    });
    let args = json!({ "entity": { "kind": "path", "layer": 1, "sheet": 1, "path": path } });
    let id = run(&mut kernel, "entities.create", &args.to_string())["id"]
        .as_u64()
        .expect("id");
    let issues = run(
        &mut kernel,
        "ops.validate",
        &json!({ "ids": [id] }).to_string(),
    );
    assert_eq!(issues["issues"][0]["kind"], "open");
    assert_eq!(issues["issues"][0]["id"], id);
}

#[test]
fn ops_crossings_finds_two_vectors_that_cross_and_a_bow_tie() {
    let mut kernel = Kernel::new();
    let line = |kernel: &mut Kernel, points: &[(f64, f64)], closed: bool| {
        let segments: Vec<Value> = points[1..]
            .iter()
            .map(|(x, y)| json!({ "kind": "line", "to": { "x": x, "y": y } }))
            .collect();
        let path = json!({ "subpaths": [{
            "start": { "x": points[0].0, "y": points[0].1 },
            "segments": segments,
            "closed": closed
        }] });
        let args = json!({ "entity": { "kind": "path", "layer": 1, "sheet": 1, "path": path } });
        run(kernel, "entities.create", &args.to_string())["id"]
            .as_u64()
            .expect("id")
    };
    let a = line(&mut kernel, &[(0.0, 0.0), (10.0, 10.0)], false);
    let b = line(&mut kernel, &[(0.0, 10.0), (10.0, 0.0)], false);
    let apart = line(&mut kernel, &[(50.0, 0.0), (60.0, 0.0)], false);
    let found = run(
        &mut kernel,
        "ops.crossings",
        &json!({ "ids": [a, b] }).to_string(),
    );
    let list = found["crossings"].as_array().expect("crossings");
    assert_eq!(list.len(), 1, "{found}");
    assert!((list[0]["x"].as_f64().expect("x") - 5.0).abs() < 1e-6);
    assert_eq!(list[0]["ids"], json!([a, b]));
    let none = run(
        &mut kernel,
        "ops.crossings",
        &json!({ "ids": [a, apart] }).to_string(),
    );
    assert!(none["crossings"].as_array().expect("list").is_empty());
    let bow = line(
        &mut kernel,
        &[
            (100.0, 0.0),
            (110.0, 10.0),
            (110.0, 0.0),
            (100.0, 10.0),
            (100.0, 0.0),
        ],
        true,
    );
    let own = run(
        &mut kernel,
        "ops.crossings",
        &json!({ "ids": [bow] }).to_string(),
    );
    assert!(
        !own["crossings"].as_array().expect("list").is_empty(),
        "{own}"
    );
}

#[test]
fn text_is_laid_out_into_glyph_outlines() {
    let mut kernel = Kernel::new();
    let args = json!({
        "spec": {
            "text": "CARVE",
            "fontFamily": FontLibrary::FALLBACK_FAMILY,
            "sizeMm": 20.0,
        },
        "x": 10.0,
        "y": 10.0,
    });
    let result = run(&mut kernel, "text.create", &args.to_string());
    let id = result["id"].as_u64().expect("id");
    let entity = kernel.document().entity(id).expect("entity");
    assert_eq!(entity.kind.tag(), "text");
    let bbox = entity.bbox().expect("text has a box");
    assert!(bbox.width() > 10.0, "{bbox:?}");
    assert!(bbox.min.x >= 9.0, "{bbox:?}");
}

#[test]
fn a_text_anchor_puts_that_point_of_its_box_at_the_origin() {
    let mut kernel = Kernel::new();
    let bbox_of = |kernel: &mut Kernel, anchor: &str| {
        let args = json!({
            "spec": {
                "text": "CARVE",
                "fontFamily": FontLibrary::FALLBACK_FAMILY,
                "sizeMm": 20.0,
                "anchor": anchor,
            },
            "x": 100.0,
            "y": 50.0,
        });
        let id = run(kernel, "text.create", &args.to_string())["id"]
            .as_u64()
            .expect("id");
        kernel
            .document()
            .entity(id)
            .ok()
            .and_then(|e| e.bbox())
            .expect("a box")
    };
    let centre = bbox_of(&mut kernel, "centre");
    assert!(
        ((centre.min.x + centre.max.x) / 2.0 - 100.0).abs() < 1e-6,
        "{centre:?}"
    );
    assert!(
        ((centre.min.y + centre.max.y) / 2.0 - 50.0).abs() < 1e-6,
        "{centre:?}"
    );
    let top_right = bbox_of(&mut kernel, "topRight");
    assert!((top_right.max.x - 100.0).abs() < 1e-6, "{top_right:?}");
    assert!((top_right.max.y - 50.0).abs() < 1e-6, "{top_right:?}");
}

#[test]
fn a_text_block_wraps_to_its_width() {
    let mut kernel = Kernel::new();
    let family = kernel.fonts.families().first().cloned().expect("a font");
    let spec = |wrap: Option<f64>| {
        json!({
            "text": "one two three four five six seven eight",
            "fontFamily": family,
            "sizeMm": 5.0,
            "wrapWidth": wrap,
        })
    };
    let bbox_of = |kernel: &mut Kernel, wrap: Option<f64>| {
        let id = run(
            kernel,
            "text.create",
            &json!({ "spec": spec(wrap), "x": 0.0, "y": 0.0 }).to_string(),
        )["id"]
            .as_u64()
            .expect("id");
        kernel
            .document()
            .entity(id)
            .ok()
            .and_then(|e| e.bbox())
            .expect("a box")
    };
    let line = bbox_of(&mut kernel, None);
    let block = bbox_of(&mut kernel, Some(40.0));
    assert!(line.width() > 60.0, "{line:?}");
    assert!(block.width() <= 40.5, "{block:?}");
    assert!(block.height() > line.height() * 2.0, "{block:?}");
}

#[test]
fn a_dimension_is_created_with_its_parts() {
    let mut kernel = Kernel::new();
    let args = json!({
        "kind": "horizontal",
        "from": { "x": 0.0, "y": 0.0 },
        "to": { "x": 80.0, "y": 0.0 },
        "offset": 15.0,
    });
    let result = run(&mut kernel, "dims.create", &args.to_string());
    let id = result["id"].as_u64().expect("id");
    let entity = kernel.document().entity(id).expect("entity");
    assert_eq!(entity.kind.tag(), "dimension");
    assert!(entity.bbox().is_some());
}

#[test]
fn dimensions_go_on_the_named_layer_with_their_text_where_clicked() {
    let mut kernel = Kernel::new();
    let args = json!({
        "kind": "horizontal",
        "from": { "x": 0.0, "y": 0.0 },
        "to": { "x": 80.0, "y": 0.0 },
        "offset": 15.0,
        "textAt": { "x": 70.0, "y": 15.0 },
    });
    let id = run(&mut kernel, "dims.create", &args.to_string())["id"]
        .as_u64()
        .expect("id");
    let layer_name = |kernel: &Kernel, id: u64| {
        let layer = kernel.document().entity(id).expect("entity").layer;
        kernel.document().layer(layer).expect("layer").name.clone()
    };
    assert_eq!(layer_name(&kernel, id), "Dimensions");
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["textAt"]["x"], 70.0);

    let style = run(
        &mut kernel,
        "dims.style",
        &json!({ "layerName": "Notes" }).to_string(),
    );
    assert_eq!(style["useLayer"], true);
    let named = run(&mut kernel, "dims.create", &args.to_string())["id"]
        .as_u64()
        .expect("id");
    assert_eq!(layer_name(&kernel, named), "Notes");

    run(
        &mut kernel,
        "dims.style",
        &json!({ "useLayer": false }).to_string(),
    );
    let active = kernel.document().active_layer();
    let plain = run(&mut kernel, "dims.create", &args.to_string())["id"]
        .as_u64()
        .expect("id");
    assert_eq!(
        kernel.document().entity(plain).expect("entity").layer,
        active
    );
}

#[test]
fn bitmap_commands_report_that_they_are_not_in_this_build() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("bitmaps.load", r#"{"path":"x.png"}"#)
        .expect_err("not in this build");
    assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
}

#[test]
fn a_document_survives_save_and_open_through_the_kernel() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let mut path = std::env::temp_dir();
    path.push(format!("carve-kernel-{}.carve", std::process::id()));
    let args = json!({ "path": path.to_string_lossy() }).to_string();
    run(&mut kernel, "doc.save", &args);
    assert!(!kernel.document().is_dirty());
    let saved = kernel.document().clone();
    run(&mut kernel, "doc.new", "");
    assert!(kernel.document().order().is_empty());
    run(&mut kernel, "doc.open", &args);
    assert_eq!(*kernel.document(), saved);
    std::fs::remove_file(&path).ok();
}

#[test]
fn save_copy_writes_a_backup_and_leaves_the_document_unsaved() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let dir = std::env::temp_dir().join(format!("carve-backup-{}", std::process::id()));
    let own = dir.join("job.carve");
    std::fs::create_dir_all(&dir).expect("dir");
    run(
        &mut kernel,
        "doc.save",
        &json!({ "path": own.to_string_lossy() }).to_string(),
    );
    make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let reply = run(&mut kernel, "doc.saveCopy", "{}");
    let backup = dir.join("job.backup.carve");
    assert_eq!(reply["path"], json!(backup.display().to_string()));
    assert!(kernel.document().is_dirty());
    assert_eq!(kernel.document().file_path(), Some(own.as_path()));
    let copy = io::native::open(&backup, 1).expect("the backup opens");
    assert_eq!(copy.order().len(), 2);
    let other = dir.join("nested").join("elsewhere.carve");
    run(
        &mut kernel,
        "doc.saveCopy",
        &json!({ "path": other.to_string_lossy() }).to_string(),
    );
    assert!(other.exists());
    assert!(!dir.join("nested").join("elsewhere.carve.partial").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn document_variables_are_set_undone_and_saved() {
    let mut kernel = Kernel::new();
    run(
        &mut kernel,
        "doc.variables.set",
        &json!({ "variables": { "width": 120.5, "gap_2": 3.0 } }).to_string(),
    );
    let got = run(&mut kernel, "doc.variables.get", "{}");
    assert_eq!(got["variables"]["width"], json!(120.5));
    assert!(kernel
        .apply(
            "doc.variables.set",
            &json!({ "variables": { "2x": 1.0 } }).to_string()
        )
        .is_err());
    let path = std::env::temp_dir().join(format!("carve-vars-{}.carve", std::process::id()));
    io::native::save(kernel.document(), &path).expect("saves");
    let opened = io::native::open(&path, 1).expect("opens");
    assert_eq!(opened.variables().get("gap_2"), Some(&3.0));
    std::fs::remove_file(&path).ok();
    run(&mut kernel, "undo", "");
    assert!(kernel.document().variables().is_empty());
}

#[test]
fn variable_formulas_stay_live_through_edits_undo_and_files() {
    let mut kernel = metric_kernel();
    let set = |kernel: &mut Kernel, vars: serde_json::Value| {
        run(
            kernel,
            "doc.variables.set",
            &json!({ "variables": vars }).to_string(),
        )
    };
    set(
        &mut kernel,
        json!({ "w": 10.0, "half": "{w}/2+1", "twice": "{half}*2", "edge": "W/3" }),
    );
    let got = run(&mut kernel, "doc.variables.get", "{}");
    assert_eq!(got["variables"]["half"], json!(6.0));
    assert_eq!(got["variables"]["twice"], json!(12.0));
    assert_eq!(got["variables"]["edge"], json!(100.0));
    assert_eq!(got["formulas"]["half"], json!("{w}/2+1"));
    assert!(got["formulas"].get("w").is_none());

    // Changing w works out everything that reads it, formulas kept.
    set(
        &mut kernel,
        json!({ "w": 20.0, "half": "{w}/2+1", "twice": "{half}*2", "edge": "W/3" }),
    );
    assert_eq!(kernel.document().variables()["twice"], 22.0);
    assert_eq!(kernel.document().variable_formulas()["twice"], "{half}*2");

    // A formula reading the material follows the job, in one undo step with it.
    let job = JobSetup {
        width: 600.0,
        ..*kernel.document().job()
    };
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    assert_eq!(kernel.document().variables()["edge"], 200.0);
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().variables()["edge"], 100.0);
    assert_eq!(kernel.document().job().width, 300.0);

    // Loops and unknown names are refused with a plain message; nothing changes.
    for (vars, expected) in [
        (
            json!({ "a": "{b}+1", "b": "{a}" }),
            "refer to each other in a loop",
        ),
        (json!({ "a": "{a}+1" }), "a refers to itself"),
        (json!({ "a": "{nope}*2" }), "which is not a variable"),
    ] {
        let error = kernel
            .apply(
                "doc.variables.set",
                &json!({ "variables": vars }).to_string(),
            )
            .expect_err("refused");
        assert!(error.to_string().contains(expected), "{error}");
    }
    assert_eq!(kernel.document().variables()["half"], 11.0);

    // Saved in the file with the formulas; export and import keep them too.
    let dir = std::env::temp_dir().join(format!("carve-formulas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("job.carve");
    io::native::save(kernel.document(), &path).expect("saves");
    let opened = io::native::open(&path, 1).expect("opens");
    assert_eq!(opened.variable_formulas()["half"], "{w}/2+1");
    assert_eq!(opened.variables()["half"], 11.0);
    let text = dir.join("vars.txt");
    run(
        &mut kernel,
        "doc.variables.export",
        &json!({ "path": text }).to_string(),
    );
    let written = std::fs::read_to_string(&text).expect("read");
    assert!(written.contains("half = {w}/2+1"), "{written}");
    std::fs::write(&text, "w = 4\nquarter = {w}/4\n").expect("write");
    run(
        &mut kernel,
        "doc.variables.import",
        &json!({ "path": text }).to_string(),
    );
    assert_eq!(kernel.document().variables()["quarter"], 1.0);
    assert_eq!(kernel.document().variables()["half"], 3.0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_older_file_with_plain_variables_loads_them_as_numbers() {
    let mut file = Kernel::new().document().to_file();
    file.variables.insert("depth".to_string(), 3.0);
    let mut value = serde_json::to_value(&file).expect("file");
    assert!(value.get("variableFormulas").is_none());
    value["variables"] = json!({ "depth": 3.0 });
    let file: crate::doc::DocumentFile = serde_json::from_value(value).expect("reads");
    let doc = crate::doc::Document::from_file(file, 0).expect("loads");
    assert_eq!(doc.variables()["depth"], 3.0);
    assert!(doc.variable_formulas().is_empty());
}

#[test]
fn an_untitled_document_backs_up_into_the_fallback_folder() {
    let fallback = std::path::Path::new("/backups");
    assert_eq!(
        io::native::backup_path(None, fallback),
        fallback.join("Untitled.backup.carve")
    );
    assert_eq!(
        io::native::backup_path(Some(std::path::Path::new("/jobs/sign.carve")), fallback),
        std::path::Path::new("/jobs/sign.backup.carve")
    );
}

#[test]
fn vectors_survive_an_svg_export_and_import() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 10.0, 10.0, 60.0, 40.0);
    let before = kernel.document().bbox_of(id).expect("bbox");
    let mut path = std::env::temp_dir();
    path.push(format!("carve-kernel-{}.svg", std::process::id()));
    run(
        &mut kernel,
        "io.exportVectors",
        &json!({ "path": path.to_string_lossy(), "format": "svg", "ids": [id] }).to_string(),
    );
    run(&mut kernel, "doc.new", "");
    let result = run(
        &mut kernel,
        "io.importVectors",
        &json!({ "path": path.to_string_lossy() }).to_string(),
    );
    std::fs::remove_file(&path).ok();
    let ids: Vec<u64> = result["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 1);
    let after = kernel.document().bbox_of(ids[0]).expect("bbox");
    assert!(close(before, after), "{before:?} {after:?}");
}

#[test]
fn vectors_survive_a_pdf_eps_and_ai_export_and_import() {
    for extension in ["pdf", "eps", "ai"] {
        let mut kernel = Kernel::new();
        let id = make_rect(&mut kernel, 10.0, 10.0, 60.0, 40.0);
        let before = kernel.document().bbox_of(id).expect("bbox");
        let mut path = std::env::temp_dir();
        path.push(format!("carve-kernel-{}.{extension}", std::process::id()));
        let path_text = path.to_string_lossy();
        run(
            &mut kernel,
            "io.exportVectors",
            &json!({ "path": path_text, "ids": [id] }).to_string(),
        );
        run(&mut kernel, "doc.new", "");
        let pages = run(
            &mut kernel,
            "io.vectorPages",
            &json!({ "path": path_text }).to_string(),
        );
        assert_eq!(pages["pages"], 1, "{extension}");
        let result = run(
            &mut kernel,
            "io.importVectors",
            &json!({ "path": path_text, "options": { "page": 1 } }).to_string(),
        );
        if extension == "pdf" {
            let error = kernel
                .apply(
                    "io.importVectors",
                    &json!({ "path": path_text, "options": { "page": 2 } }).to_string(),
                )
                .expect_err("there is one page");
            assert!(matches!(error, CoreError::Validation(_)), "{error}");
        }
        std::fs::remove_file(&path).ok();
        let ids = result["ids"].as_array().expect("ids");
        assert_eq!(ids.len(), 1, "{extension}");
        let after = kernel
            .document()
            .bbox_of(ids[0].as_u64().expect("id"))
            .expect("bbox");
        assert!(close(before, after), "{extension}: {before:?} {after:?}");
    }
}

#[test]
fn an_svg_brings_its_pictures_in_as_bitmaps() {
    use image::{ImageBuffer, Luma};
    let temp = crate::test_support::TempDir::new("svg-bitmap");
    ImageBuffer::from_pixel(8, 4, Luma([90u8]))
        .save(temp.path().join("wood.png"))
        .expect("writes the png");
    std::fs::write(temp.path().join("broken.png"), b"not a picture").expect("writes");
    let svg = temp.path().join("sign.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="80mm" height="40mm" viewBox="0 0 80 40">
          <rect x="0" y="0" width="80" height="40"/>
          <image href="wood.png" x="0" y="0" width="80" height="40"/>
          <image href="broken.png" x="0" y="0" width="80" height="40"/></svg>"#,
    )
    .expect("writes the svg");
    let mut kernel = Kernel::new();
    let result = run(
        &mut kernel,
        "io.importVectors",
        &json!({ "path": svg.to_string_lossy() }).to_string(),
    );
    let ids: Vec<u64> = result["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(result["warnings"][0]["kind"], "imageSkipped");
    assert_eq!(result["warnings"][0]["code"], "import.imageSkipped");
    let picture = kernel.document().entity(ids[1]).expect("entity");
    let crate::doc::EntityKind::Bitmap(bitmap) = &picture.kind else {
        panic!("not a bitmap");
    };
    let bbox = picture.bbox().expect("bbox");
    assert!((bbox.width() - 80.0).abs() < 1e-9 && (bbox.height() - 40.0).abs() < 1e-9);
    let stored = kernel
        .document()
        .images()
        .get(bitmap.image_id)
        .expect("stored");
    assert_eq!((stored.width, stored.height), (8, 4));
    run(&mut kernel, "undo", "");
    assert!(kernel.document().order().is_empty());
}

#[test]
fn a_dwg_drawing_imports_through_the_vector_command() {
    let temp = crate::test_support::TempDir::new("kernel-dwg");
    let path = temp.path().join("plate.DWG");
    // An inch drawing with a closed 2D polyline and a 3D polyline.
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/dwg/polylines-2000.dwg"
    );
    std::fs::copy(fixture, &path).expect("copies");
    let mut kernel = Kernel::new();
    let pages = run(
        &mut kernel,
        "io.vectorPages",
        &json!({ "path": path.to_string_lossy() }).to_string(),
    );
    assert_eq!(pages["pages"], 1);
    let result = run(
        &mut kernel,
        "io.importVectors",
        &json!({ "path": path.to_string_lossy() }).to_string(),
    );
    let ids = result["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 2);
    let polyline = kernel
        .document()
        .bbox_of(ids[1].as_u64().expect("id"))
        .expect("bbox");
    // Inches become millimetres: 9 inches across.
    assert!((polyline.width() - 228.6).abs() < 1e-6, "{polyline:?}");
}

#[test]
fn importing_a_file_that_is_not_a_vector_file_is_refused() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("io.importVectors", r#"{"path":"model.stl"}"#)
        .expect_err("not a vector file");
    assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
}

#[test]
fn exporting_with_nothing_in_the_document_is_refused() {
    let mut kernel = Kernel::new();
    let error = kernel
        .apply("io.exportVectors", r#"{"path":"out.svg"}"#)
        .expect_err("nothing to export");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn a_bitmap_traces_into_vectors_fitted_to_the_job() {
    use image::{DynamicImage, ImageBuffer, Luma};
    let temp = crate::test_support::TempDir::new("trace");
    let path = temp.path().join("square.png");
    // A dark 10 by 10 square in the middle of a light 20 by 20 image.
    let buffer: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(20, 20, |x, y| {
        Luma([if (5..15).contains(&x) && (5..15).contains(&y) {
            0
        } else {
            255
        }])
    });
    DynamicImage::ImageLuma8(buffer)
        .save(&path)
        .expect("writes the png");
    let mut kernel = Kernel::new();
    let info = run(
        &mut kernel,
        "bitmap.info",
        &json!({ "path": path }).to_string(),
    );
    assert_eq!(
        (info["width"].as_u64(), info["height"].as_u64()),
        (Some(20), Some(20))
    );

    let reply = run(
        &mut kernel,
        "bitmap.trace",
        &json!({ "path": path, "params": { "cornerFit": "lines" },
                 "origin": { "x": 0.0, "y": 0.0 }, "pixelSize": 2.0 })
        .to_string(),
    );
    let ids = reply["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 1);
    let id = ids[0].as_u64().expect("id");
    let bbox = kernel
        .document()
        .entity(id)
        .expect("entity")
        .path()
        .and_then(|p| geom::path_bbox(&p))
        .expect("bbox");
    assert!(
        (bbox.min.x - 10.0).abs() < 1e-6 && (bbox.max.x - 30.0).abs() < 1e-6,
        "{bbox:?}"
    );
    assert!(
        (bbox.min.y - 10.0).abs() < 1e-6 && (bbox.max.y - 30.0).abs() < 1e-6,
        "{bbox:?}"
    );
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Trace Bitmap");

    // Without a placement the image fills the job, centred.
    let reply = run(
        &mut kernel,
        "bitmap.trace",
        &json!({ "path": path }).to_string(),
    );
    let id = reply["ids"][0].as_u64().expect("id");
    let bbox = kernel
        .document()
        .entity(id)
        .expect("entity")
        .path()
        .and_then(|p| geom::path_bbox(&p))
        .expect("bbox");
    let job = kernel.document().job_bounds();
    let centre = bbox.centre();
    assert!((centre.x - job.centre().x).abs() < 1e-3, "{bbox:?} {job:?}");
    assert!((centre.y - job.centre().y).abs() < 1e-3, "{bbox:?} {job:?}");
    let error = kernel
        .apply(
            "bitmap.trace",
            &json!({ "path": path, "params": { "threshold": 0 } }).to_string(),
        )
        .expect_err("nothing darker than black");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn nesting_onto_a_chosen_sheet_adds_no_sheets_when_asked() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 500.0, 500.0, 560.0, 540.0);
    let second = kernel.document_mut().add_sheet(None);
    let reply = run(
        &mut kernel,
        "ops.nest",
        &json!({
            "ids": [a],
            "params": { "sheetWidth": 150.0, "sheetHeight": 100.0, "border": 5.0,
                        "gap": 0.0, "toolDiameter": 4.0 },
            "quantities": { a.to_string(): 5 },
            "sheet": second,
            "addSheets": false,
        })
        .to_string(),
    );
    assert_eq!(reply["sheetCount"], 1, "{reply}");
    assert!(
        !reply["unplaced"].as_array().expect("unplaced").is_empty(),
        "{reply}"
    );
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(
        sheets.as_array().map(Vec::len),
        Some(2),
        "no sheet added: {sheets}"
    );
    for id in reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
    {
        assert_eq!(kernel.document().entity(id).expect("entity").sheet, second);
    }
}

#[test]
fn nesting_packs_parts_onto_sheets_with_copies() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 500.0, 500.0, 560.0, 540.0);
    let b = make_rect(&mut kernel, 600.0, 500.0, 630.0, 520.0);
    let reply = run(
        &mut kernel,
        "ops.nest",
        &json!({
            "ids": [a, b],
            "params": { "sheetWidth": 150.0, "sheetHeight": 100.0, "border": 5.0,
                        "gap": 0.0, "toolDiameter": 4.0 },
            "quantities": { a.to_string(): 5 },
        })
        .to_string(),
    );
    let ids: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 6, "{reply}");
    assert!(ids.contains(&a) && ids.contains(&b), "originals are placed");
    assert_eq!(reply["sheetCount"], 2, "{reply}");
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(sheets.as_array().map(Vec::len), Some(2), "{sheets}");

    let job = kernel.document().job_bounds();
    let boxes: Vec<(u64, BBox)> = ids
        .iter()
        .map(|id| {
            let entity = kernel.document().entity(*id).expect("entity");
            let bbox = entity
                .path()
                .and_then(|p| geom::path_bbox(&p))
                .expect("bbox");
            (entity.sheet, bbox)
        })
        .collect();
    for (i, (sheet_i, bi)) in boxes.iter().enumerate() {
        assert!(bi.min.x >= job.min.x + 5.0 - 1e-6 && bi.max.x <= job.min.x + 145.0 + 1e-6);
        assert!(bi.min.y >= job.min.y + 5.0 - 1e-6 && bi.max.y <= job.min.y + 95.0 + 1e-6);
        for (sheet_j, bj) in boxes.iter().skip(i + 1) {
            if sheet_i != sheet_j {
                continue;
            }
            let apart = bi.max.x + 4.0 <= bj.min.x + 1e-6
                || bj.max.x + 4.0 <= bi.min.x + 1e-6
                || bi.max.y + 4.0 <= bj.min.y + 1e-6
                || bj.max.y + 4.0 <= bi.min.y + 1e-6;
            assert!(apart, "{bi:?} and {bj:?} are closer than the tool");
        }
    }
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Nest Parts");
    assert_eq!(
        run(&mut kernel, "sheets.list", "").as_array().map(Vec::len),
        Some(1)
    );
    let error = kernel
        .apply("ops.nest", &json!({ "ids": [] }).to_string())
        .expect_err("nothing chosen");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn sheets_count_objects_duplicate_and_take_moved_objects() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 10.0, 10.0, 20.0, 20.0);
    let b = make_rect(&mut kernel, 30.0, 10.0, 40.0, 20.0);
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [a, b] }).to_string(),
    );
    let second = run(
        &mut kernel,
        "sheets.add",
        r#"{"name":"Offcut","activate":true}"#,
    )["id"]
        .as_u64()
        .expect("sheet");
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(sheets[1]["active"], true, "{sheets}");
    assert_eq!(sheets[0]["count"], 2);
    assert_eq!(run(&mut kernel, "selection.get", "")["ids"], json!([]));
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Add Sheet");
    assert_eq!(
        run(&mut kernel, "sheets.list", "").as_array().map(Vec::len),
        Some(1)
    );
    run(&mut kernel, "redo", "");

    run(&mut kernel, "sheets.setActive", r#"{"id":1}"#);
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [a, b] }).to_string(),
    );
    run(
        &mut kernel,
        "entities.setSheet",
        &json!({ "ids": [b], "sheet": second }).to_string(),
    );
    assert_eq!(run(&mut kernel, "selection.get", "")["ids"], json!([a]));
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(
        (sheets[0]["count"].as_u64(), sheets[1]["count"].as_u64()),
        (Some(1), Some(1))
    );

    let copy = run(
        &mut kernel,
        "sheets.duplicate",
        &json!({ "id": second }).to_string(),
    );
    let sheets = run(&mut kernel, "sheets.list", "");
    assert_eq!(sheets[2]["name"], "Offcut copy");
    assert_eq!(sheets[2]["count"], 1);
    assert_eq!(sheets[2]["active"], true);
    assert_ne!(copy["ids"][0], json!(b));
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Duplicate Sheet");
}

#[test]
fn save_text_writes_the_file_as_given() {
    let temp = crate::test_support::TempDir::new("kernel-save-text");
    let path = temp.path().join("cut list.csv");
    let text = "Part,Quantity\nSide,2\n";
    let mut kernel = Kernel::new();
    run(
        &mut kernel,
        "io.saveText",
        &json!({ "path": path, "text": text }).to_string(),
    );
    assert_eq!(std::fs::read_to_string(&path).expect("written"), text);
    let back = run(
        &mut kernel,
        "io.readText",
        &json!({ "path": path }).to_string(),
    );
    assert_eq!(back["text"], text);
}

#[test]
fn plug_vectors_are_one_undo_step() {
    let mut kernel = Kernel::new();
    let outline = make_rect(&mut kernel, 0.0, 0.0, 40.0, 20.0);
    let made = run(
        &mut kernel,
        "cam.inlayPlugVectors",
        &json!({ "ids": [outline] }).to_string(),
    );
    assert_eq!(made["ids"].as_array().map(Vec::len), Some(1));
    assert_eq!(run(&mut kernel, "undo", "")["label"], "Create Plug Vectors");
}

#[test]
fn the_window_layout_remembers_the_assembly_tab_and_view() {
    let window: crate::prefs::WindowPrefs =
        serde_json::from_value(json!({ "activeTab": "assembly", "viewMode": "assembly" }))
            .expect("parses");
    let back = serde_json::to_value(&window).expect("serializes");
    assert_eq!(back["activeTab"], "assembly");
    assert_eq!(back["viewMode"], "assembly");
}

#[test]
fn plate_production_fills_variables_and_lays_plates_out() {
    let mut kernel = Kernel::new();
    let outline = make_rect(&mut kernel, 0.0, 0.0, 40.0, 20.0);
    let text = run(
        &mut kernel,
        "text.create",
        &json!({
            "spec": { "text": "!!name!! #!!n!!", "fontFamily": FontLibrary::FALLBACK_FAMILY, "sizeMm": 5.0 },
            "x": 5.0,
            "y": 5.0,
        })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("text");
    let names = run(
        &mut kernel,
        "plates.variables",
        &json!({ "ids": [outline, text] }).to_string(),
    );
    assert_eq!(names["variables"], json!(["name", "n"]));
    let data = run(
        &mut kernel,
        "plates.data",
        &json!({ "text": "Name\nAnn\nBob\nCid\n", "header": true }).to_string(),
    );
    assert_eq!(data["columns"], json!(["Name"]));
    assert_eq!(data["rows"], 3);
    let reply = run(
        &mut kernel,
        "plates.produce",
        &json!({
            "ids": [outline, text],
            "text": "Name\nAnn\nBob\nCid\n",
            "header": true,
            "fields": {
                "name": { "kind": "column", "column": 0 },
                "n": { "kind": "counter", "start": 1, "step": 1, "digits": 2 },
            },
            "margins": { "left": 10.0, "right": 10.0, "top": 10.0, "bottom": 10.0 },
            "spacing": { "x": 5.0, "y": 5.0 },
        })
        .to_string(),
    );
    assert_eq!(reply["plates"], 3);
    assert_eq!(reply["sheets"], 1);
    let ids: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 6);
    let doc = kernel.document();
    let texts: Vec<String> = ids
        .iter()
        .filter_map(|id| match &doc.entity(*id).expect("entity").kind {
            crate::doc::EntityKind::Text(t) => Some(t.spec.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["Ann #01", "Bob #02", "Cid #03"]);
    // The first plate sits at the top left inside the margins, the second
    // beside it with the spacing.
    let job = doc.job_bounds();
    let first = doc.bbox_of(ids[0]).expect("first plate");
    let plate = doc.bbox_of_many(&[outline, text]).expect("template");
    assert!((first.min.x - (job.min.x + 10.0)).abs() < 1e-6, "{first:?}");
    assert!((first.max.y - (job.max.y - 10.0)).abs() < 1e-6, "{first:?}");
    let second = doc.bbox_of(ids[2]).expect("second plate");
    assert!(
        (second.min.x - first.min.x - (plate.width() + 5.0)).abs() < 1e-6,
        "{second:?}"
    );
    // Every plate on a sheet of its own.
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().order().len(), 2);
    let own = run(
        &mut kernel,
        "plates.produce",
        &json!({ "ids": [outline, text], "count": 3, "ownSheets": true,
                 "fields": { "n": { "kind": "counter", "start": 7, "step": 1 } } })
        .to_string(),
    );
    assert_eq!(own["sheets"], 3);

    // Layers mode: a sheet worth of plates per layer, all on one sheet.
    run(&mut kernel, "undo", "");
    let sheets_before = kernel.document().sheets().len();
    let layered = run(
        &mut kernel,
        "plates.produce",
        &json!({ "ids": [outline, text], "count": 400, "mode": "layers",
                 "fields": { "n": { "kind": "counter", "start": 1, "step": 1 } } })
        .to_string(),
    );
    assert_eq!(layered["sheets"], 1);
    assert_eq!(kernel.document().sheets().len(), sheets_before);
    let layers = layered["layers"].as_u64().expect("layers");
    assert!(layers >= 2, "400 plates overflow one sheet: {layered}");
    let names: Vec<String> = kernel
        .document()
        .layers()
        .iter()
        .map(|l| l.name.clone())
        .collect();
    assert!(names.contains(&"Plates 2".to_string()), "{names:?}");
}

#[test]
fn text_converts_to_a_group_of_letter_curves() {
    let mut kernel = Kernel::new();
    let text = run(
        &mut kernel,
        "text.create",
        &json!({
            "spec": { "text": "AB", "fontFamily": FontLibrary::FALLBACK_FAMILY, "sizeMm": 10.0 },
            "x": 0.0,
            "y": 0.0,
        })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("text");
    let before = kernel.document().bbox_of(text).expect("text box");
    let reply = run(
        &mut kernel,
        "text.toCurves",
        &json!({ "ids": [text] }).to_string(),
    );
    let group = reply["ids"][0].as_u64().expect("group");
    let doc = kernel.document();
    assert!(doc.entity(text).is_err());
    assert_eq!(doc.leaves(group).len(), 2);
    let after = doc.bbox_of(group).expect("group box");
    assert!((after.min.x - before.min.x).abs() < 1e-9 && (after.max.y - before.max.y).abs() < 1e-9);
    run(&mut kernel, "undo", "");
    assert!(kernel.document().entity(text).is_ok());
    assert!(kernel
        .apply("text.toCurves", &json!({ "ids": [] }).to_string())
        .is_err());
}

#[test]
fn auto_layout_text_fills_its_box_and_travels_with_moves() {
    let mut kernel = Kernel::new();
    let spec = json!({
        "text": "NAME\nPLATE",
        "fontFamily": FontLibrary::FALLBACK_FAMILY,
        "sizeMm": 5.0,
        "alignment": "centre",
        "autoLayout": {
            "bounds": [0.0, 0.0, 100.0, 40.0],
            "margin": "normal",
            "vertical": "lineSpacing",
            "horizontal": "characterWidth",
        },
    });
    let id = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec, "x": 0.0, "y": 0.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text");
    let bbox = kernel.document().bbox_of(id).expect("box");
    assert!(
        (bbox.min.x - 10.0).abs() < 1e-6 && (bbox.max.x - 90.0).abs() < 1e-6,
        "{bbox:?}"
    );
    assert!(
        bbox.min.y >= 4.0 - 1e-6 && bbox.max.y <= 36.0 + 1e-6,
        "{bbox:?}"
    );
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [id], "matrix": [1, 0, 0, 1, 50, 0] }).to_string(),
    );
    run(&mut kernel, "text.update", &json!({ "id": id }).to_string());
    let moved = kernel.document().bbox_of(id).expect("box");
    assert!((moved.min.x - 60.0).abs() < 1e-6, "{moved:?}");
}

#[test]
fn text_selection_gaps_and_arch_change_the_layout() {
    let mut kernel = Kernel::new();
    let spec = |extra: Value| {
        let mut spec = json!({
            "text": "AB\nCD",
            "fontFamily": FontLibrary::FALLBACK_FAMILY,
            "sizeMm": 10.0,
        });
        if let (Some(target), Some(more)) = (spec.as_object_mut(), extra.as_object()) {
            target.extend(more.clone());
        }
        spec
    };
    let id = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec(json!({})), "x": 0.0, "y": 0.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text");
    let glyphs = |kernel: &mut Kernel| run(kernel, "text.glyphs", &json!({ "id": id }).to_string());
    let before = glyphs(&mut kernel);
    assert_eq!(before["glyphs"].as_array().map(Vec::len), Some(4));
    assert_eq!(before["glyphs"][2]["line"], 1);
    let b0 = before["glyphs"][1]["bbox"][0].as_f64().expect("B");
    let c1 = before["glyphs"][2]["bbox"][3].as_f64().expect("C");
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec(json!({ "letterGaps": [[0.5]], "lineGaps": [0.3] })) })
            .to_string(),
    );
    let after = glyphs(&mut kernel);
    let b1 = after["glyphs"][1]["bbox"][0].as_f64().expect("B");
    let c2 = after["glyphs"][2]["bbox"][3].as_f64().expect("C");
    assert!((b1 - b0 - 5.0).abs() < 1e-6, "{b0} {b1}");
    assert!((c1 - c2 - 3.0).abs() < 1e-6, "{c1} {c2}");
    let flat = kernel.document().bbox_of(id).expect("box");
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec(json!({ "text": "ARCHED", "arch": 8.0 })) }).to_string(),
    );
    let arched = kernel.document().bbox_of(id).expect("box");
    assert!(arched.height() > 10.0 && arched != flat, "{arched:?}");
    // The arc comes back for the handles, and letter gaps are kept while
    // arched: opening the first gap widens the arched text.
    let reply = glyphs(&mut kernel);
    assert!(reply["arc"]["radius"].as_f64().expect("radius") > 0.0);
    assert_eq!(reply["arc"]["outward"], true);
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec(json!({ "text": "ARCHED", "arch": 8.0,
                 "letterGaps": [[0.5]] })) })
        .to_string(),
    );
    let spaced = kernel.document().bbox_of(id).expect("box");
    assert!(
        spaced.width() > arched.width() + 1.0,
        "{arched:?} {spaced:?}"
    );
    // Turned text stays turned when it is laid out again.
    run(
        &mut kernel,
        "ops.rotate",
        &json!({ "ids": [id], "angleDeg": 90.0 }).to_string(),
    );
    let turned = kernel.document().bbox_of(id).expect("box");
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec(json!({ "text": "ARCHED", "arch": 8.0,
                 "letterGaps": [[0.5]] })) })
        .to_string(),
    );
    let again = kernel.document().bbox_of(id).expect("box");
    assert!(
        (again.width() - turned.width()).abs() < 1e-6,
        "{turned:?} {again:?}"
    );
    assert!(again.height() > again.width());
}

#[test]
fn guides_belong_to_the_job_and_a_new_job_has_none() {
    let mut kernel = Kernel::new();
    run(
        &mut kernel,
        "doc.guides.set",
        &json!({ "guides": [{ "axis": "y", "position": 42.0, "locked": false }] }).to_string(),
    );
    let got = run(&mut kernel, "doc.guides.get", "{}");
    assert_eq!(got["guides"][0]["position"], 42.0);
    assert_eq!(got["guides"][0]["axis"], "y");
    assert_eq!(run(&mut kernel, "doc.info", "{}")["dirty"], true);
    run(&mut kernel, "doc.new", "");
    let fresh = run(&mut kernel, "doc.guides.get", "{}");
    assert_eq!(fresh["guides"], json!([]));
}

#[test]
fn text_spacing_leaves_anchored_text_where_it_was() {
    let mut kernel = Kernel::new();
    let spec = |gaps: Value| {
        json!({ "text": "Hello", "fontFamily": "Arial", "sizeMm": 20.0,
                "anchor": "bottomLeft", "letterGaps": gaps })
    };
    let id = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec(json!([])), "x": 40.0, "y": 60.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text");
    let first = |kernel: &mut Kernel| {
        let reply = run(kernel, "text.glyphs", &json!({ "id": id }).to_string());
        let b = &reply["glyphs"][0]["bbox"];
        (b[0].as_f64().expect("x"), b[1].as_f64().expect("y"))
    };
    let plain = kernel.document().bbox_of(id).expect("box");
    assert!((plain.min.x - 40.0).abs() < 1e-6 && (plain.min.y - 60.0).abs() < 1e-6);
    let before = first(&mut kernel);
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec(json!([[0.0, -0.02]])) }).to_string(),
    );
    let after = first(&mut kernel);
    assert!(
        (after.0 - before.0).abs() < 1e-6 && (after.1 - before.1).abs() < 1e-6,
        "{before:?} {after:?}"
    );
    let spaced = kernel.document().bbox_of(id).expect("box");
    assert!((spaced.min.x - 40.0).abs() < 1e-6 && (spaced.min.y - 60.0).abs() < 1e-6);
    assert!(spaced.width() < plain.width(), "{plain:?} {spaced:?}");
}

#[test]
fn dimensions_measure_radii_take_custom_text_and_a_style() {
    let mut kernel = Kernel::new();
    let style = run(
        &mut kernel,
        "dims.style",
        &json!({ "textHeight": 4.0, "precision": 1, "showUnits": false }).to_string(),
    );
    assert_eq!(style["precision"], 1);
    let id = run(
        &mut kernel,
        "dims.create",
        &json!({ "kind": "radius", "centre": { "x": 0.0, "y": 0.0 }, "radius": 12.5,
                 "at": { "x": 30.0, "y": 0.0 }, "diameter": true })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("dimension");
    let doc = kernel.document();
    let entity = doc.entity(id).expect("entity");
    let layer = doc.layer(entity.layer).expect("layer");
    assert_eq!(layer.name, "Dimensions");
    let bbox = entity.bbox().expect("box");
    assert!(bbox.min.x < -12.0 && bbox.max.x > 29.0, "{bbox:?}");
    run(
        &mut kernel,
        "dims.update",
        &json!({ "id": id, "customText": "BORE" }).to_string(),
    );
    match &kernel.document().entity(id).expect("entity").kind {
        crate::doc::EntityKind::Dimension(dim) => {
            assert_eq!(dim.custom_text.as_deref(), Some("BORE"));
        }
        _ => panic!("not a dimension"),
    }
    // A second dimension goes on the same layer.
    let other = run(
        &mut kernel,
        "dims.create",
        &json!({ "kind": "linear", "from": { "x": 0.0, "y": 0.0 }, "to": { "x": 10.0, "y": 0.0 },
                 "offset": 5.0, "customText": "TEN" })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("linear");
    let doc = kernel.document();
    assert_eq!(
        doc.entity(other).expect("e").layer,
        doc.entity(id).expect("e").layer
    );
}

#[test]
fn dimension_text_is_dragged_along_its_line_and_rides_a_move() {
    let mut kernel = Kernel::new();
    let id = run(
        &mut kernel,
        "dims.create",
        &json!({ "kind": "horizontal", "from": { "x": 0.0, "y": 0.0 },
                 "to": { "x": 100.0, "y": 0.0 }, "offset": 20.0 })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("dimension");
    let text_box = |kernel: &Kernel| match &kernel.document().entity(id).expect("e").kind {
        crate::doc::EntityKind::Dimension(dim) => dim.text_box.expect("text box"),
        _ => panic!("not a dimension"),
    };
    let middle = text_box(&kernel);
    assert!((middle[0] + middle[2]) * 0.5 > 45.0 && (middle[0] + middle[2]) * 0.5 < 55.0);
    // Dragged with the line moved up to 30 and the text slid to x = 80.
    run(
        &mut kernel,
        "dims.update",
        &json!({ "id": id, "spec": { "kind": "horizontal", "from": { "x": 0.0, "y": 0.0 },
                 "to": { "x": 100.0, "y": 0.0 }, "offset": 30.0 },
                 "textAt": { "x": 80.0, "y": 31.0 } })
        .to_string(),
    );
    let slid = text_box(&kernel);
    let cx = (slid[0] + slid[2]) * 0.5;
    assert!((cx - 80.0).abs() < 1.0, "{slid:?}");
    assert!(slid[1] > 30.0, "{slid:?}");
    // Moving the dimension carries the text position along.
    run(
        &mut kernel,
        "ops.move",
        &json!({ "ids": [id], "dx": 10.0, "dy": 0.0 }).to_string(),
    );
    match &kernel.document().entity(id).expect("e").kind {
        crate::doc::EntityKind::Dimension(dim) => {
            let at = dim.text_at.expect("text position");
            assert!((at.x - 90.0).abs() < 1e-9);
        }
        _ => panic!("not a dimension"),
    }
}

#[test]
fn plate_production_calculates_the_template_toolpaths_per_sheet() {
    let mut kernel = Kernel::new();
    let job = kernel.document().job_bounds();
    // A plate a little over half the material wide and tall: one per sheet.
    let (w, h) = (job.width() * 0.6, job.height() * 0.6);
    let outline = make_rect(&mut kernel, 0.0, 0.0, w, h);
    let other = make_rect(&mut kernel, -50.0, -50.0, -40.0, -40.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "profile", "tool": end_mill(), "name": "Cut Out",
                 "params": { "cutDepth": 3.0, "side": "outside" }, "ids": [outline] })
        .to_string(),
    );
    // A toolpath that also cuts something else is not the template's.
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "profile", "tool": end_mill(), "name": "Both",
                 "params": { "cutDepth": 3.0, "side": "outside" }, "ids": [outline, other] })
        .to_string(),
    );
    let names = run(
        &mut kernel,
        "plates.variables",
        &json!({ "ids": [outline] }).to_string(),
    );
    assert_eq!(names["toolpaths"], 1);
    let reply = run(
        &mut kernel,
        "plates.produce",
        &json!({ "ids": [outline], "count": 2, "toolpaths": true }).to_string(),
    );
    assert_eq!(reply["sheets"], 2);
    let made: Vec<u64> = reply["toolpaths"]
        .as_array()
        .expect("toolpaths")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(made.len(), 2);
    let doc = kernel.document();
    let first = doc.toolpath(made[0]).expect("first");
    let second = doc.toolpath(made[1]).expect("second");
    assert_eq!(first.name, "S1 - Cut Out");
    assert_eq!(second.name, "S2 - Cut Out");
    assert_ne!(first.sheet, second.sheet);
    assert!(!first.moves.is_empty());
    let plates: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(
        first.sources.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![plates[0]]
    );
    assert_eq!(
        second.sources.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![plates[1]]
    );
    // One undo takes the plates and their toolpaths back.
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().toolpaths().len(), 2);
}

#[test]
fn text_on_a_curve_stays_linked_and_comes_off_again() {
    let mut kernel = Kernel::new();
    let fonts = run(&mut kernel, "text.fonts", "");
    let family = fonts["families"][0].as_str().expect("a family").to_string();
    let spec = json!({ "text": "HHHH", "fontFamily": family, "sizeMm": 5.0 });
    let text = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec, "x": 0.0, "y": 100.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text id");
    let line = make_rect(&mut kernel, 0.0, 0.0, 200.0, 50.0);
    let bbox = |kernel: &mut Kernel, id: u64| {
        let entity = run(kernel, "entities.get", &json!({ "id": id }).to_string());
        let paths: Vec<Path> = serde_json::from_value(entity["paths"].clone()).expect("paths");
        paths
            .iter()
            .filter_map(geom::path_bbox)
            .reduce(|a, b| a.union(b))
            .expect("bbox")
    };
    let curve = json!({ "pathId": line, "position": "above", "align": "left" });
    run(
        &mut kernel,
        "text.onCurve",
        &json!({ "textId": text, "curve": curve }).to_string(),
    );
    let on = bbox(&mut kernel, text);
    assert!(
        on.min.y >= -1e-6 && on.max.y < 10.0,
        "sits on the bottom edge: {on:?}"
    );
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": text }).to_string(),
    );
    assert_eq!(entity["spec"]["onCurve"]["pathId"], line);

    // Editing the text keeps it on the curve; filling scales it to the curve.
    let filled = json!({ "pathId": line, "position": "above", "align": "left", "fill": true });
    run(
        &mut kernel,
        "text.onCurve",
        &json!({ "textId": text, "curve": filled }).to_string(),
    );
    let wide = bbox(&mut kernel, text);
    assert!(wide.width() > on.width() * 2.0, "{wide:?} against {on:?}");

    run(
        &mut kernel,
        "text.removeFromCurve",
        &json!({ "id": text }).to_string(),
    );
    let off = bbox(&mut kernel, text);
    assert!(off.min.y > 95.0, "back at its origin: {off:?}");
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": text }).to_string(),
    );
    assert!(entity["spec"].get("onCurve").is_none());
}

#[test]
fn text_on_a_curve_follows_the_curve_and_undoes_with_it() {
    let mut kernel = Kernel::new();
    let fonts = run(&mut kernel, "text.fonts", "");
    let family = fonts["families"][0].as_str().expect("a family").to_string();
    let spec = json!({ "text": "HHHH", "fontFamily": family, "sizeMm": 5.0 });
    let text = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec, "x": 0.0, "y": 100.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text id");
    let line = make_rect(&mut kernel, 0.0, 0.0, 200.0, 50.0);
    let curve = json!({ "pathId": line, "position": "above", "align": "left" });
    run(
        &mut kernel,
        "text.onCurve",
        &json!({ "textId": text, "curve": curve }).to_string(),
    );
    let bottom = |kernel: &mut Kernel| {
        let entity = run(kernel, "entities.get", &json!({ "id": text }).to_string());
        let paths: Vec<Path> = serde_json::from_value(entity["paths"].clone()).expect("paths");
        paths
            .iter()
            .filter_map(geom::path_bbox)
            .reduce(|a, b| a.union(b))
            .expect("bbox")
            .min
            .y
    };
    assert!(bottom(&mut kernel).abs() < 1.0);
    let depth = kernel.document().journal().undo_depth();

    // Moving the curve alone takes the text with it, in the same step.
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [line], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 30.0] }).to_string(),
    );
    let moved = bottom(&mut kernel);
    assert!((moved - 30.0).abs() < 1.0, "text followed: {moved}");
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);

    run(&mut kernel, "undo", "");
    let back = bottom(&mut kernel);
    assert!(back.abs() < 1.0, "one undo puts both back: {back}");

    // Moving both together keeps the text where the move put it.
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [line, text], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 10.0] }).to_string(),
    );
    assert!((bottom(&mut kernel) - 10.0).abs() < 1.0);
}

#[test]
fn live_text_edits_join_the_create_step() {
    let mut kernel = Kernel::new();
    let fonts = run(&mut kernel, "text.fonts", "");
    let family = fonts["families"][0].as_str().expect("a family").to_string();
    let spec = |text: &str| json!({ "text": text, "fontFamily": family, "sizeMm": 5.0 });
    let depth = kernel.document().journal().undo_depth();
    let made = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec("A"), "x": 0.0, "y": 0.0 }).to_string(),
    );
    let id = made["id"].as_u64().expect("id");
    let mut revision = made["revision"].as_u64().expect("revision");
    for word in ["AB", "ABC"] {
        let reply = run(
            &mut kernel,
            "text.update",
            &json!({ "id": id, "spec": spec(word), "x": 1.0, "y": 2.0, "merge": revision })
                .to_string(),
        );
        revision = reply["revision"].as_u64().expect("revision");
    }
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);
    assert_eq!(
        kernel.document().journal().undo_label(),
        Some("Create Text")
    );
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["spec"]["text"], "ABC");

    // Something else in between starts a step of its own.
    make_rect(&mut kernel, 0.0, 0.0, 5.0, 5.0);
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": id, "spec": spec("ABCD"), "merge": revision }).to_string(),
    );
    assert_eq!(kernel.document().journal().undo_label(), Some("Edit Text"));
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    assert!(
        kernel.document().entity(id).is_err(),
        "the create step undid"
    );
}

#[test]
fn redo_replays_a_step_that_changed_one_entity_twice() {
    let mut kernel = Kernel::new();
    let fonts = run(&mut kernel, "text.fonts", "");
    let family = fonts["families"][0].as_str().expect("a family").to_string();
    let spec = |text: &str| json!({ "text": text, "fontFamily": family, "sizeMm": 5.0 });
    let made = run(
        &mut kernel,
        "text.create",
        &json!({ "spec": spec("A"), "x": 0.0, "y": 0.0 }).to_string(),
    );
    let id = made["id"].as_u64().expect("id");
    let mut revision = made["revision"].as_u64().expect("revision");
    for word in ["AB", "ABC"] {
        revision = run(
            &mut kernel,
            "text.update",
            &json!({ "id": id, "spec": spec(word), "merge": revision }).to_string(),
        )["revision"]
            .as_u64()
            .expect("revision");
    }
    run(&mut kernel, "undo", "");
    assert!(kernel.document().entity(id).is_err());
    run(&mut kernel, "redo", "");
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["spec"]["text"], "ABC", "redo ends on the last edit");
    run(&mut kernel, "undo", "");
    assert!(kernel.document().entity(id).is_err(), "and undoes again");
    run(&mut kernel, "redo", "");
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": id }).to_string(),
    );
    assert_eq!(entity["spec"]["text"], "ABC");
}

/// A text set on a line, for the copy and delete tests: (text, line).
fn text_on_a_line(kernel: &mut Kernel) -> (u64, u64) {
    let fonts = run(kernel, "text.fonts", "");
    let family = fonts["families"][0].as_str().expect("a family").to_string();
    let spec = json!({ "text": "HHHH", "fontFamily": family, "sizeMm": 5.0 });
    let text = run(
        kernel,
        "text.create",
        &json!({ "spec": spec, "x": 0.0, "y": 100.0 }).to_string(),
    )["id"]
        .as_u64()
        .expect("text id");
    let line = make_rect(kernel, 0.0, 0.0, 200.0, 50.0);
    let curve = json!({ "pathId": line, "position": "above", "align": "left" });
    run(
        kernel,
        "text.onCurve",
        &json!({ "textId": text, "curve": curve }).to_string(),
    );
    (text, line)
}

fn curve_of(kernel: &mut Kernel, id: u64) -> Option<u64> {
    let entity = run(kernel, "entities.get", &json!({ "id": id }).to_string());
    entity["spec"]["onCurve"]["pathId"].as_u64()
}

#[test]
fn copied_curve_text_follows_its_copied_curve_or_goes_free() {
    let mut kernel = Kernel::new();
    let (text, line) = text_on_a_line(&mut kernel);
    let copies = run(
        &mut kernel,
        "ops.duplicate",
        &json!({ "ids": [text, line] }).to_string(),
    );
    let (text_copy, line_copy) = (
        copies["ids"][0].as_u64().expect("text copy"),
        copies["ids"][1].as_u64().expect("line copy"),
    );
    assert_eq!(curve_of(&mut kernel, text_copy), Some(line_copy));
    assert_eq!(
        curve_of(&mut kernel, text),
        Some(line),
        "the original keeps its own"
    );
    let depth = kernel.document().journal().undo_depth();

    // Moving the original curve leaves the copy where it is.
    let before = kernel.document().bbox_of(text_copy).expect("box");
    run(
        &mut kernel,
        "entities.transform",
        &json!({ "ids": [line], "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 40.0] }).to_string(),
    );
    let after = kernel.document().bbox_of(text_copy).expect("box");
    assert!(
        (after.min.y - before.min.y).abs() < 1e-9,
        "the copy stayed put"
    );
    assert_eq!(kernel.document().journal().undo_depth(), depth + 1);

    // A text copied without its curve is plain text where it was.
    let alone = run(
        &mut kernel,
        "ops.duplicate",
        &json!({ "ids": [text] }).to_string(),
    );
    let lone = alone["ids"][0].as_u64().expect("copy");
    assert_eq!(curve_of(&mut kernel, lone), None);

    // Copy and Paste link to the pasted curve too.
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [text, line] }).to_string(),
    );
    let pasted = run(
        &mut kernel,
        "clipboard.paste",
        &json!({ "dx": 10.0, "dy": 10.0 }).to_string(),
    );
    let (pasted_text, pasted_line) = (
        pasted["ids"][0].as_u64().expect("text"),
        pasted["ids"][1].as_u64().expect("line"),
    );
    assert_eq!(curve_of(&mut kernel, pasted_text), Some(pasted_line));
}

#[test]
fn text_whose_curve_is_deleted_stays_editable() {
    let mut kernel = Kernel::new();
    let (text, line) = text_on_a_line(&mut kernel);
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [line] }).to_string(),
    );
    assert_eq!(curve_of(&mut kernel, text), None);
    let entity = run(
        &mut kernel,
        "entities.get",
        &json!({ "id": text }).to_string(),
    );
    let mut spec = entity["spec"].clone();
    spec["text"] = json!("HH");
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": text, "spec": spec }).to_string(),
    );
    // Undo brings the curve and the link back together.
    run(&mut kernel, "undo", "");
    run(&mut kernel, "undo", "");
    assert_eq!(curve_of(&mut kernel, text), Some(line));

    // A text kept on a curve that has gone (an older file) is still edited.
    let mut spec = entity["spec"].clone();
    spec["onCurve"] = json!({ "pathId": 9999, "position": "above", "align": "left" });
    run(
        &mut kernel,
        "text.update",
        &json!({ "id": text, "spec": spec }).to_string(),
    );
    assert_eq!(curve_of(&mut kernel, text), None);
}

#[test]
fn document_variables_export_and_import_replace_same_names() {
    let (temp, mut kernel) = temp_kernel("variables");
    run(
        &mut kernel,
        "doc.variables.set",
        &json!({ "variables": { "depth": 6.5, "width": 100.0 } }).to_string(),
    );
    let path = temp.path().join("vars.txt");
    let file = path.display().to_string();
    run(
        &mut kernel,
        "doc.variables.export",
        &json!({ "path": file }).to_string(),
    );
    let text = std::fs::read_to_string(&path).expect("written");
    assert_eq!(text, "depth = 6.5\nwidth = 100\n");
    std::fs::write(&path, "# plates\nwidth = 120\ngap,3\n").expect("write");
    run(
        &mut kernel,
        "doc.variables.set",
        &json!({ "variables": { "depth": 6.5, "width": 100.0, "keep": 1.0 } }).to_string(),
    );
    let reply = run(
        &mut kernel,
        "doc.variables.import",
        &json!({ "path": file }).to_string(),
    );
    assert_eq!(reply["count"], 2);
    let vars = kernel.document().variables();
    assert_eq!(vars.get("width"), Some(&120.0));
    assert_eq!(vars.get("gap"), Some(&3.0));
    assert_eq!(vars.get("keep"), Some(&1.0));
    std::fs::write(&path, "width 120\n").expect("write");
    assert!(kernel
        .apply("doc.variables.import", &json!({ "path": file }).to_string())
        .is_err());
}

#[test]
fn a_copied_part_keeps_its_nest_marks_but_nested_copies_do_not() {
    let mut kernel = Kernel::new();
    let part = make_rect(&mut kernel, 0.0, 0.0, 20.0, 20.0);
    run(
        &mut kernel,
        "ops.nestMarks",
        &json!({ "ids": [part], "copies": 3, "filler": true }).to_string(),
    );
    let mark = kernel.document().nest_marks().get(&part).copied();
    assert!(mark.is_some());
    let depth = kernel.document().journal().undo_depth();
    let copy = run(
        &mut kernel,
        "ops.duplicate",
        &json!({ "ids": [part] }).to_string(),
    )["ids"][0]
        .as_u64()
        .expect("a copy");
    assert_eq!(kernel.document().nest_marks().get(&copy).copied(), mark);
    assert_eq!(
        kernel.document().journal().undo_depth(),
        depth + 1,
        "one undo step"
    );
    run(&mut kernel, "undo", "");
    assert!(kernel.document().nest_marks().get(&copy).is_none());
    assert_eq!(kernel.document().nest_marks().get(&part).copied(), mark);

    // Copy and Paste carries the marks too.
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [part] }).to_string(),
    );
    let pasted = run(&mut kernel, "clipboard.paste", "");
    let pasted = pasted["ids"][0].as_u64().expect("a pasted copy");
    assert_eq!(kernel.document().nest_marks().get(&pasted).copied(), mark);

    // The copies Nest Parts places are the ones the mark asked for.
    let nested = run(
        &mut kernel,
        "ops.nest",
        &json!({ "ids": [part], "params": { "rotationStepDeg": 0 } }).to_string(),
    );
    let placed: Vec<u64> = nested["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .filter(|id| *id != part)
        .collect();
    assert!(!placed.is_empty(), "{nested}");
    for id in placed {
        assert!(kernel.document().nest_marks().get(&id).is_none(), "{id}");
    }
}

#[test]
fn a_backup_restores_as_its_job_with_unsaved_changes() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let dir = std::env::temp_dir().join(format!("carve-restore-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let own = dir.join("job.carve");
    run(
        &mut kernel,
        "doc.save",
        &json!({ "path": own.to_string_lossy() }).to_string(),
    );
    make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    let backup = run(&mut kernel, "doc.saveCopy", "{}")["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let mut next = Kernel::new();
    run(
        &mut next,
        "doc.restore",
        &json!({ "path": backup, "as": own.to_string_lossy() }).to_string(),
    );
    assert_eq!(next.document().order().len(), 2, "the backup's content");
    assert_eq!(next.document().file_path(), Some(own.as_path()));
    assert!(next.document().is_dirty(), "not yet saved to the job");
    // The backup of a job never saved restores untitled.
    run(
        &mut next,
        "doc.restore",
        &json!({ "path": backup }).to_string(),
    );
    assert_eq!(next.document().file_path(), None);
    assert!(next.document().is_dirty());
    // A plain open of the same file is clean and names the backup.
    run(
        &mut next,
        "doc.open",
        &json!({ "path": backup }).to_string(),
    );
    assert!(!next.document().is_dirty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_panic_in_a_command_is_answered_as_an_internal_error() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let reply = envelope(kernel.call("test.panic", ""));
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "internal");
    assert_eq!(
        reply["error"]["message"],
        "something went wrong inside the app: test.panic failed: the test asked for a panic"
    );
    // The kernel still answers, with the document as it was.
    assert_eq!(kernel.document().order().len(), 1);
    assert_eq!(envelope(kernel.call("ping", ""))["ok"], true);
}

// Review 033 row 8: a command that panics part way leaves the document as
// it was, with no step left open for the next command to commit.
#[test]
fn a_command_that_panics_leaves_the_document_as_it_was() {
    let mut kernel = Kernel::new();
    make_rect(&mut kernel, 0.0, 0.0, 10.0, 10.0);
    let revision = kernel.document().revision();
    let count = kernel.document().order().len();
    let reply = kernel.call("test.panicMidway", "");
    let KernelReply::Json(text) = reply else {
        panic!("json")
    };
    assert!(text.contains("test.panicMidway failed"), "{text}");
    assert_eq!(kernel.document().revision(), revision);
    assert_eq!(kernel.document().order().len(), count);
    make_rect(&mut kernel, 20.0, 0.0, 30.0, 10.0);
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().order().len(), count);
}
