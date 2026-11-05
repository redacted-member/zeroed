//! Toolpath, tool and preview commands through the kernel.

use super::*;

#[test]
fn tool_and_post_commands_are_routed_to_the_cam_state() {
    let temp = crate::test_support::TempDir::new("kernel-cam");
    let cam = CamState::new(
        crate::cam::tooldb::ToolStore::at(temp.path().join("tools.json")),
        crate::post::PostLibrary::at(temp.path().join("posts")),
    );
    let mut kernel = Kernel::with_cam(cam);
    let tools = run(&mut kernel, "tools.list", "");
    assert!(tools.to_string().contains("End Mill"), "{tools}");
    let posts = run(&mut kernel, "posts.list", "{}");
    assert!(
        posts.as_array().is_some_and(|list| list.len() >= 8),
        "{posts}"
    );
    assert!(temp.path().join("tools.json").exists());
}

#[test]
fn tools_resolve_for_the_active_machine_and_a_vector_makes_a_form_profile() {
    let temp = crate::test_support::TempDir::new("kernel-tools-context");
    let cam = CamState::new(
        crate::cam::tooldb::ToolStore::at(temp.path().join("tools.json")),
        crate::post::PostLibrary::at(temp.path().join("posts")),
    );
    let mut kernel = Kernel::with_cam(cam);
    let list = run(&mut kernel, "tools.list", "");
    assert_eq!(
        list["machine"],
        json!(kernel.prefs().values().machines.active)
    );
    let other = run(&mut kernel, "tools.list", r#"{"machine":"Elsewhere"}"#);
    assert_eq!(other["machine"], "Elsewhere");

    assert!(kernel.apply("tools.formProfile", "{}").is_err());
    let id = make_rect(&mut kernel, 40.0, 10.0, 43.0, 15.0);
    let reply = run(
        &mut kernel,
        "tools.formProfile",
        &json!({ "id": id }).to_string(),
    );
    let profile: crate::geom::Path =
        serde_json::from_value(reply["profile"].clone()).expect("a path");
    let tool = crate::cam::tool::Tool {
        name: "form".to_string(),
        tool_type: crate::cam::tool::ToolGeometry::FormTool { profile },
        ..crate::cam::tool::Tool::default()
    };
    assert!((tool.diameter() - 6.0).abs() < 1e-6);
}

#[test]
fn a_toolpath_is_calculated_listed_previewed_and_saved() {
    let (temp, mut kernel) = temp_kernel("kernel-toolpath");
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let reply = run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 6.0, "side": "outside" },
            "ids": [id],
        })
        .to_string(),
    );
    let toolpath = reply["id"].as_u64().expect("an id");
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["name"], "Profile 1");
    assert_eq!(list[0]["stale"], false);
    assert!(list[0]["estimatedSeconds"].as_f64().unwrap_or(0.0) > 0.0);

    let blob = match kernel.apply("cam.moves", "").expect("moves") {
        KernelReply::Bytes(bytes) => bytes,
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    };
    let header_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let header: Value = serde_json::from_slice(&blob[4..4 + header_len]).expect("header");
    let count = header["toolpaths"][0]["count"].as_u64().expect("count") as usize;
    assert!(count > 5);
    assert_eq!(header["sections"][1]["count"].as_u64(), Some(count as u64));

    run(
        &mut kernel,
        "cam.rename",
        &json!({ "id": toolpath, "name": "Cut out" }).to_string(),
    );
    run(
        &mut kernel,
        "cam.setVisible",
        &json!({ "ids": [toolpath], "visible": false }).to_string(),
    );
    let got = run(
        &mut kernel,
        "cam.get",
        &json!({ "id": toolpath }).to_string(),
    );
    assert_eq!(got["name"], "Cut out");
    assert_eq!(got["visible"], false);
    assert_eq!(got["request"]["strategy"], "profile");

    let path = temp.path().join("job");
    let saved = run(
        &mut kernel,
        "cam.save",
        &json!({ "toolpathIds": [toolpath], "post": "grbl_mm", "path": path }).to_string(),
    );
    let written = saved["paths"][0].as_str().expect("a file").to_string();
    let gcode = std::fs::read_to_string(written).expect("reads");
    assert!(gcode.contains("Z-6.000"), "{gcode}");
}

/// Save Toolpaths wraps a rotary job round its blank through a post that
/// drives the rotary axis, and through any other post writes it flat, as the
/// form warns, rather than refusing.
#[test]
fn a_rotary_job_saves_wrapped_through_a_rotary_post_and_flat_through_others() {
    let (temp, mut kernel) = temp_kernel("kernel-rotary-save");
    let base = JobSetup::default();
    let job = JobSetup {
        kind: crate::doc::JobKind::Rotary,
        rotary: crate::doc::RotarySetup {
            diameter: 60.0,
            length: 200.0,
            ..base.rotary
        },
        ..base
    };
    run(
        &mut kernel,
        "job.set",
        &serde_json::to_string(&job).expect("job"),
    );
    let id = make_rect(&mut kernel, 70.0, 70.0, 130.0, 110.0);
    let reply = run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 2.0, "side": "outside" },
            "ids": [id],
        })
        .to_string(),
    );
    let toolpath = reply["id"].as_u64().expect("an id");
    let save = |kernel: &mut Kernel, post: &str| {
        let path = temp.path().join(post);
        let saved = run(
            kernel,
            "cam.save",
            &json!({ "toolpathIds": [toolpath], "post": post, "path": path }).to_string(),
        );
        std::fs::read_to_string(saved["paths"][0].as_str().expect("a file")).expect("reads")
    };
    let wrapped = save(&mut kernel, "grbl_rotary_a_mm");
    assert!(wrapped.lines().any(|l| l.contains('A')), "{wrapped}");
    assert!(wrapped.contains("G93"), "wrapped cuts feed in inverse time");
    let flat = save(&mut kernel, "grbl_mm");
    assert!(
        !flat.lines().any(|l| l.starts_with('G') && l.contains('A')),
        "{flat}"
    );
    assert!(flat.contains("Y"), "flat Y stays Y");
}

#[test]
fn editing_a_vector_marks_its_toolpath_stale_until_recalculated() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let reply = run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "pocket", "tool": end_mill(), "ids": [id] }).to_string(),
    );
    let toolpath = reply["id"].as_u64().expect("an id");
    run(
        &mut kernel,
        "ops.move",
        &json!({ "ids": [id], "dx": 10.0, "dy": 0.0 }).to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], true);
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": toolpath }).to_string(),
    );
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["stale"], false);
    assert_eq!(list[0]["name"], "Pocket 1");
}

#[test]
fn a_material_setup_that_moves_the_heights_marks_toolpaths_stale() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "pocket", "tool": end_mill(), "ids": [id] }).to_string(),
    );
    let set_job = |kernel: &mut Kernel, job: &crate::doc::JobSetup| {
        run(kernel, "job.set", &serde_json::to_string(job).expect("job"));
    };
    let stale = |kernel: &mut Kernel| run(kernel, "cam.list", "")[0]["stale"].clone();
    let before = *kernel.document().job_setup();

    // The thickness moves nothing while Z zero is on the material surface.
    let mut thicker = before;
    thicker.thickness += 5.0;
    set_job(&mut kernel, &thicker);
    assert_eq!(stale(&mut kernel), false);

    // Z zero on the machine bed lifts every height by the thickness.
    let mut bed = thicker;
    bed.z_zero = crate::doc::ZZero::Bottom;
    set_job(&mut kernel, &bed);
    assert_eq!(stale(&mut kernel), true);
    set_job(&mut kernel, &thicker);
    assert_eq!(stale(&mut kernel), false);

    // So do the clearance and the home position of Material Setup.
    let mut clearance = thicker;
    clearance.machine.safe_z += 3.0;
    set_job(&mut kernel, &clearance);
    assert_eq!(stale(&mut kernel), true);
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": 1 }).to_string(),
    );
    assert_eq!(stale(&mut kernel), false);
    let mut home = clearance;
    home.machine.home_x += 10.0;
    set_job(&mut kernel, &home);
    assert_eq!(stale(&mut kernel), true);
}

#[test]
fn toolpath_changes_undo_like_everything_else() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let compute = json!({ "strategy": "drill", "tool": end_mill(), "ids": [id] }).to_string();
    let first = run(&mut kernel, "cam.compute", &compute)["id"]
        .as_u64()
        .expect("id");
    let second = run(&mut kernel, "cam.compute", &compute)["id"]
        .as_u64()
        .expect("id");
    run(
        &mut kernel,
        "cam.reorder",
        &json!({ "ids": [second, first] }).to_string(),
    );
    run(
        &mut kernel,
        "cam.delete",
        &json!({ "ids": [first] }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(1)
    );
    let undo = run(&mut kernel, "undo", "");
    assert_eq!(undo["label"], "Delete Toolpath");
    let names: Vec<Value> = run(&mut kernel, "cam.list", "")
        .as_array()
        .expect("list")
        .iter()
        .map(|t| t["id"].clone())
        .collect();
    assert_eq!(names, vec![json!(second), json!(first)]);
    let error = kernel
        .apply(
            "cam.compute",
            &json!({ "strategy": "profile", "tool": end_mill(), "ids": [] }).to_string(),
        )
        .expect_err("nothing selected");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn job_sheet_lists_each_toolpath_and_moves_mark_retracts() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 20.0, 20.0, 80.0, 60.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "pocket", "name": "Pocket <A>", "tool": end_mill(),
                 "ids": [id], "params": { "cutDepth": 4.0 } })
        .to_string(),
    );
    let blob = match kernel.apply("cam.moves", "").expect("moves") {
        KernelReply::Bytes(bytes) => bytes,
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    };
    let header_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let header: Value = serde_json::from_slice(&blob[4..4 + header_len]).expect("header");
    let kinds_at = header["sections"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["name"] == "kind"))
        .and_then(|s| s["offset"].as_u64())
        .expect("kind section") as usize;
    let kinds = &blob[4 + header_len + kinds_at..];
    assert!(
        kinds.contains(&MOVE_RETRACT),
        "a pocket lifts out at the end"
    );

    let path = std::env::temp_dir().join(format!("carve-jobsheet-{}.html", std::process::id()));
    run(
        &mut kernel,
        "cam.jobSheet",
        &json!({ "path": path, "title": "Sign" }).to_string(),
    );
    let html = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<h1>Sign</h1>"));
    assert!(html.contains("Pocket &lt;A&gt;"), "names are escaped");
    assert!(html.contains("4.00 mm"), "the cut depth");
    assert!(html.contains("Material Setup") && html.contains("<svg"));
}

#[test]
fn a_preview_cuts_the_visible_toolpaths_into_the_stock() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 20.0, 20.0, 80.0, 60.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "pocket", "tool": end_mill(), "ids": [id],
                 "params": { "cutDepth": 4.0 } })
        .to_string(),
    );
    assert!(
        kernel.apply("sim.heightfield", "").is_err(),
        "nothing run yet"
    );
    let reply = run(&mut kernel, "sim.run", "{}");
    assert!(reply["collisions"].as_array().is_some_and(Vec::is_empty));
    assert!(reply["seconds"].as_f64().unwrap_or(0.0) > 0.0);
    let blob = match kernel.apply("sim.heightfield", "").expect("heightfield") {
        KernelReply::Bytes(bytes) => bytes,
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    };
    let (header, z) = crate::blob::read_heights(&blob, "z");
    let header_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let (w, h) = (
        header["width"].as_u64().expect("w") as usize,
        header["height"].as_u64().expect("h") as usize,
    );
    // u16 heights and owners; a single sided job's underside is flat, so
    // it has no floor section.
    assert_eq!(
        blob.len(),
        4 + header_len + (w * h * 2).next_multiple_of(4) * 2
    );
    let stock = kernel.stock.as_ref().expect("stock");
    let step = header["heightStep"].as_f64().expect("a step");
    assert!(step * 0.5 <= stock.cell * 0.01);
    for (sent, kept) in z.iter().zip(&stock.z) {
        assert!(f64::from((sent - kept).abs()) <= step * 0.5 + 1e-5);
    }
    assert_eq!(stock.height_at(50.0, 40.0), Some(-4.0), "the pocket floor");
    assert_eq!(
        stock.height_at(5.0, 5.0),
        Some(0.0),
        "outside it is untouched"
    );
    run(&mut kernel, "sim.reset", "");
    assert!(kernel.apply("sim.heightfield", "").is_err());
}

#[test]
fn changing_the_model_marks_its_3d_toolpaths_stale() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 100.0, 80.0, 140.0, 120.0);
    let shape = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "finish3d", "tool": end_mill(), "ids": [],
                 "params": { "stepover": 2.0 } })
        .to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], false);
    run(
        &mut kernel,
        "model.update",
        &json!({ "id": shape, "heightScale": 2.0 }).to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], true);
    run(&mut kernel, "undo", "");
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], false);
}

#[test]
fn toolpaths_belong_to_their_sheet_and_come_along_when_it_is_copied() {
    let mut kernel = Kernel::new();
    let first = kernel.document().active_sheet();
    let id = make_rect(&mut kernel, 10.0, 10.0, 60.0, 40.0);
    let tool = crate::cam::tool::Tool {
        name: "End Mill 6 mm".to_string(),
        ..crate::cam::tool::Tool::default()
    };
    let compute = |kernel: &mut Kernel, id: u64| {
        run(
            kernel,
            "cam.compute",
            &json!({ "strategy": "profile", "tool": tool,
                     "params": { "cutDepth": 3.0 }, "ids": [id] })
            .to_string(),
        )["id"]
            .as_u64()
            .expect("a toolpath")
    };
    let tp = compute(&mut kernel, id);
    assert_eq!(kernel.document().toolpath(tp).expect("tp").sheet, first);
    let copied = run(
        &mut kernel,
        "sheets.duplicate",
        &json!({ "id": first }).to_string(),
    );
    let sheet = copied["id"].as_u64().expect("sheet");
    let new_vector = copied["ids"][0].as_u64().expect("copy");
    let list = run(&mut kernel, "cam.list", "");
    let rows = list
        .as_array()
        .or_else(|| list["toolpaths"].as_array())
        .expect("rows");
    assert_eq!(rows.len(), 2);
    let copy = rows
        .iter()
        .find(|r| r["sheet"] == json!(sheet))
        .expect("the copied toolpath");
    let copy_id = copy["id"].as_u64().expect("id");
    let request = Request::of(kernel.document().toolpath(copy_id).expect("tp")).expect("req");
    assert_eq!(request.ids, vec![new_vector]);
    // The copy only draws on its own sheet.
    assert_eq!(kernel.document().active_sheet(), sheet);
    run(
        &mut kernel,
        "sheets.delete",
        &json!({ "id": sheet }).to_string(),
    );
    assert_eq!(kernel.document().toolpaths().len(), 1);
}

#[test]
fn a_toolpath_template_carries_settings_to_another_job_by_layer() {
    let (temp, mut kernel) = temp_kernel("kernel-template");
    let cut = |kernel: &mut Kernel| {
        run(kernel, "layers.add", r#"{"name":"Cut Out"}"#)["id"]
            .as_u64()
            .expect("layer")
    };
    let rect_on = |kernel: &mut Kernel, layer: u64, x0: f64, y0: f64, x1: f64, y1: f64| {
        let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
        let args =
            json!({ "entity": { "kind": "path", "layer": layer, "sheet": 1, "path": path } });
        run(kernel, "entities.create", &args.to_string())["id"]
            .as_u64()
            .expect("id")
    };
    let layer = cut(&mut kernel);
    let id = rect_on(&mut kernel, layer, 10.0, 10.0, 60.0, 40.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "profile", "name": "Outline", "tool": end_mill(),
                 "params": { "cutDepth": 6.0, "side": "outside" }, "ids": [id] })
        .to_string(),
    );
    let path = temp.path().join("job.json");
    let saved = run(
        &mut kernel,
        "cam.saveTemplate",
        &json!({ "path": path }).to_string(),
    );
    assert_eq!(saved["count"], 1);

    run(&mut kernel, "doc.new", "");
    let error = kernel
        .apply("cam.loadTemplate", &json!({ "path": path }).to_string())
        .expect_err("no Cut Out layer yet");
    assert!(error.to_string().contains("Cut Out"), "{error}");
    let layers = run(
        &mut kernel,
        "cam.templateLayers",
        &json!({ "path": path }).to_string(),
    );
    assert_eq!(layers["missing"], json!(["Cut Out"]));
    // Loaded as it is, the toolpath waits to be calculated.
    let reply = run(
        &mut kernel,
        "cam.loadTemplate",
        &json!({ "path": path, "missing": "asIs" }).to_string(),
    );
    assert_eq!(reply["uncalculated"].as_array().map(Vec::len), Some(1));
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["stale"], true);
    assert_eq!(list[0]["moves"], 0);
    assert_eq!(list[0]["name"], "Outline");
    run(&mut kernel, "undo", "");
    // Creating the layers adds an empty Cut Out layer.
    run(
        &mut kernel,
        "cam.loadTemplate",
        &json!({ "path": path, "missing": "create" }).to_string(),
    );
    assert!(kernel
        .document()
        .layers()
        .iter()
        .any(|l| l.name == "Cut Out"));
    run(&mut kernel, "undo", "");
    assert!(!kernel
        .document()
        .layers()
        .iter()
        .any(|l| l.name == "Cut Out"));

    make_rect(&mut kernel, 0.0, 0.0, 5.0, 5.0);
    let layer = cut(&mut kernel);
    let other = rect_on(&mut kernel, layer, 100.0, 50.0, 180.0, 90.0);
    let reply = run(
        &mut kernel,
        "cam.loadTemplate",
        &json!({ "path": path }).to_string(),
    );
    assert_eq!(reply["ids"].as_array().map(Vec::len), Some(1), "{reply}");
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["name"], "Outline");
    let toolpath = list[0]["id"].as_u64().expect("id");
    let request = &run(
        &mut kernel,
        "cam.get",
        &json!({ "id": toolpath }).to_string(),
    )["request"];
    assert_eq!(request["ids"], json!([other]));
    assert_eq!(request["params"]["side"], "outside");
    assert_eq!(
        run(&mut kernel, "undo", "")["label"],
        "Apply Toolpath Template"
    );

    let error = kernel
        .apply(
            "cam.saveTemplate",
            &json!({ "path": temp.path().join("none.json") }).to_string(),
        )
        .expect_err("nothing to save");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn toolpaths_with_one_tool_merge_and_recalculate_together() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 10.0, 10.0, 60.0, 40.0);
    let b = make_rect(&mut kernel, 100.0, 10.0, 150.0, 40.0);
    let profile = |kernel: &mut Kernel, id: u64| {
        run(
            kernel,
            "cam.compute",
            &json!({ "strategy": "profile", "tool": end_mill(),
                     "params": { "cutDepth": 6.0, "side": "outside" }, "ids": [id] })
            .to_string(),
        )["id"]
            .as_u64()
            .expect("toolpath")
    };
    let (pa, pb) = (profile(&mut kernel, a), profile(&mut kernel, b));
    let merged = run(
        &mut kernel,
        "cam.merge",
        &json!({ "ids": [pb, pa] }).to_string(),
    )["id"]
        .as_u64()
        .expect("merged");
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["visible"], false);
    assert_eq!(list[1]["visible"], false);
    assert_eq!(list[2]["name"], "Merged Toolpath");
    let blob = |kernel: &mut Kernel, id: u64| match kernel
        .apply("cam.moves", &json!({ "ids": [id] }).to_string())
        .expect("moves")
    {
        KernelReply::Bytes(bytes) => bytes.len(),
        KernelReply::Json(_) => 0,
    };
    let merged_size = blob(&mut kernel, merged);
    assert!(merged_size > blob(&mut kernel, pa));

    run(
        &mut kernel,
        "ops.move",
        &json!({ "ids": [b], "dx": 20.0, "dy": 0.0 }).to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[2]["stale"], true);
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": merged }).to_string(),
    );
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[2]["stale"], false, "{list}");
    assert_eq!(list[1]["stale"], false, "the part was recalculated too");

    let other = run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "profile",
                 "tool": serde_json::to_value(crate::cam::tool::Tool {
                     tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter: 3.0 },
                     name: "End Mill 3 mm".to_string(),
                     ..crate::cam::tool::Tool::default()
                 }).expect("tool"),
                 "params": { "cutDepth": 6.0 }, "ids": [a] })
        .to_string(),
    )["id"]
        .as_u64()
        .expect("toolpath");
    let error = kernel
        .apply("cam.merge", &json!({ "ids": [pa, other] }).to_string())
        .expect_err("different tools");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

/// The XY bounds of every position in the `cam.moves` blob.
fn move_extent(kernel: &mut Kernel) -> [f64; 4] {
    let blob = match kernel.apply("cam.moves", "").expect("moves") {
        KernelReply::Bytes(bytes) => bytes,
        KernelReply::Json(text) => panic!("expected bytes, got {text}"),
    };
    let header_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let header: Value = serde_json::from_slice(&blob[4..4 + header_len]).expect("header");
    let section = header["sections"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["name"] == "xyz"))
        .expect("xyz section")
        .clone();
    let offset = section["offset"].as_u64().expect("offset") as usize;
    let count = section["count"].as_u64().expect("count") as usize;
    let data = &blob[4 + header_len + offset..];
    let mut extent = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for i in 0..count / 3 {
        let at = |k: usize| {
            let j = (i * 3 + k) * 4;
            f64::from(f32::from_le_bytes([
                data[j],
                data[j + 1],
                data[j + 2],
                data[j + 3],
            ]))
        };
        extent = [
            extent[0].min(at(0)),
            extent[1].min(at(1)),
            extent[2].max(at(0)),
            extent[3].max(at(1)),
        ];
    }
    extent
}

#[test]
fn tiling_shows_the_active_tile_and_saves_a_file_per_tile() {
    let (temp, mut kernel) = temp_kernel("kernel-tiling");
    let id = line_entity(&mut kernel, (20.0, 100.0), (280.0, 100.0));
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "profile", "tool": end_mill(),
                 "params": { "cutDepth": 3.0, "side": "on" }, "ids": [id] })
        .to_string(),
    );
    let whole = move_extent(&mut kernel);
    assert!(whole[2] > 270.0, "{whole:?}");
    let tiling = json!({ "enabled": true, "active": 1,
        "spec": { "method": "feedX", "tileWidth": 120.0, "overlap": 10.0 } });
    let set = run(
        &mut kernel,
        "cam.tiling.set",
        &json!({ "tiling": tiling }).to_string(),
    );
    assert_eq!(set["tiles"].as_array().map(Vec::len), Some(3));
    // The middle tile cuts from 110 to 240 in the job, drawn from -10 to 120
    // at the origin where it is machined.
    let tile = move_extent(&mut kernel);
    assert!(tile[2] <= 120.0 + 1e-3, "{tile:?}");
    let cut_from = tile[0];
    assert!(cut_from < 0.0, "{tile:?}");
    run(
        &mut kernel,
        "cam.tiling.view",
        &json!({ "originalPosition": true }).to_string(),
    );
    let placed = move_extent(&mut kernel);
    assert!(placed[2] > 230.0 && placed[2] <= 240.0 + 1e-3, "{placed:?}");

    // The preview cuts the active tile alone, where it sits in the job.
    run(&mut kernel, "sim.run", "{}");
    let stock = kernel.stock.as_ref().expect("stock");
    let depth = |x: f64| stock.height_at(x, 100.0).unwrap_or(0.0);
    assert!(depth(170.0) < -2.0, "inside the tile");
    assert!(depth(60.0) > -1e-3, "the first tile is not cut");
    assert!(depth(270.0) > -1e-3, "the last tile is not cut");
    run(&mut kernel, "sim.start", "{}");
    run(
        &mut kernel,
        "sim.step",
        &json!({ "toEnd": true }).to_string(),
    );
    let stock = kernel.stock.as_ref().expect("stock");
    assert!(stock.height_at(170.0, 100.0).unwrap_or(0.0) < -2.0);
    assert!(stock.height_at(60.0, 100.0).unwrap_or(0.0) > -1e-3);

    let path = temp.path().join("strip");
    let saved = run(
        &mut kernel,
        "cam.save",
        &json!({ "outputAllVisible": true, "post": "grbl_mm", "path": path,
                 "outputTiled": true })
        .to_string(),
    );
    assert_eq!(saved["tiles"].as_array().map(Vec::len), Some(3), "{saved}");
    run(&mut kernel, "undo", "");
    let off = kernel.apply(
        "cam.save",
        &json!({ "outputAllVisible": true, "post": "grbl_mm", "path": path,
                 "outputTiled": true })
        .to_string(),
    );
    assert!(off.is_err(), "tiling is off again");
}

#[test]
fn a_multi_tool_finish_makes_a_toolpath_per_tool_and_edits_them_together() {
    let mut kernel = Kernel::new();
    // Two 3 mm blocks with a 2 mm gap the big tool cannot get into, joined
    // at both ends so the gap is inside the model's outline (the Model
    // boundary is the outline, not the box).
    for (x0, y0, x1, y1) in [
        (100.0, 80.0, 119.0, 120.0),
        (121.0, 80.0, 140.0, 120.0),
        (100.0, 120.0, 140.0, 124.0),
        (100.0, 76.0, 140.0, 80.0),
    ] {
        let id = make_rect(&mut kernel, x0, y0, x1, y1);
        run(
            &mut kernel,
            "model.createShape",
            &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
        );
    }
    let mill = |name: &str, diameter: f64| {
        serde_json::to_value(crate::cam::tool::Tool {
            name: name.to_string(),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
            stepover: diameter * 0.4,
            ..crate::cam::tool::Tool::default()
        })
        .expect("tool")
    };
    let small = mill("End Mill 1 mm", 1.0);
    let request = |rest: Vec<Value>| {
        json!({ "strategy": "finish3d", "tool": mill("End Mill 12 mm", 12.0), "ids": [],
                "name": "Finish",
                "params": { "restTools": rest, "minimumDetail": 0.1,
                            "restBoundaryOffset": 0.0 } })
    };
    let reply = run(
        &mut kernel,
        "cam.compute",
        &request(vec![small.clone()]).to_string(),
    );
    let main = reply["id"].as_u64().expect("main");
    let rest = reply["restIds"][0]
        .as_u64()
        .expect("a toolpath for the small tool");
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list[0]["name"], "Finish [End Mill 12 mm]");
    assert_eq!(list[1]["name"], "Finish [End Mill 1 mm]");
    let cut = kernel.document().toolpath(rest).expect("rest");
    assert!(cut
        .moves
        .iter()
        .filter(|m| m.kind.is_cutting() && m.to.z < -1.0)
        .all(|m| (m.to.x - 120.0).abs() <= 1.5));
    // The later tool's form reads the whole request back.
    let got = run(&mut kernel, "cam.get", &json!({ "id": rest }).to_string());
    assert_eq!(got["request"]["name"], "Finish");
    assert_eq!(got["request"]["tool"]["name"], "End Mill 12 mm");
    assert_eq!(
        got["request"]["params"]["restTools"][0]["name"],
        "End Mill 1 mm"
    );
    // Recalculating the later tool on its own keeps it.
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": rest }).to_string(),
    );
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(2)
    );

    // Editing from the later tool with a second small tool adds its toolpath;
    // taking the tools out deletes theirs.
    let tiny = mill("End Mill 0.5 mm", 0.5);
    let edited = run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": rest, "request": request(vec![small, tiny]) }).to_string(),
    );
    assert_eq!(edited["restIds"][0], rest, "{edited}");
    let names: Vec<String> = run(&mut kernel, "cam.list", "")
        .as_array()
        .expect("list")
        .iter()
        .map(|t| t["name"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        names.len(),
        2 + edited["restIds"].as_array().map_or(0, |r| r.len() - 1)
    );
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": main, "request": request(vec![]) }).to_string(),
    );
    let list = run(&mut kernel, "cam.list", "");
    assert_eq!(list.as_array().map(Vec::len), Some(1));
    assert_eq!(list[0]["name"], "Finish");
    run(&mut kernel, "undo", "");
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(names.len())
    );
}

#[test]
fn machining_times_follow_the_scale_factor_and_machine_accelerations() {
    let (_temp, mut kernel) = temp_kernel("kernel-estimate");
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 6.0, "side": "outside" },
            "ids": [id],
        })
        .to_string(),
    );
    let seconds = |kernel: &mut Kernel| {
        run(kernel, "cam.list", "")[0]["estimatedSeconds"]
            .as_f64()
            .expect("seconds")
    };
    // The machine's rapid rate takes over once the preferences apply.
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"estimateScale":1.0}}"#,
    );
    let plain = seconds(&mut kernel);
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"estimateScale":2.0}}"#,
    );
    let doubled = seconds(&mut kernel);
    assert!((doubled - 2.0 * plain).abs() < 1e-6, "{plain} {doubled}");
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"estimateScale":1.0},"machines":{"active":"Default","list":[{"name":"Default","units":"mm","bedX":600,"bedY":600,"bedZ":100,"rapidRate":3000,"acceleration":[50,50,50]}]}}"#,
    );
    let accelerated = seconds(&mut kernel);
    assert!(accelerated > plain, "{plain} {accelerated}");
    // A toolpath calculated now is timed the same way.
    run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 6.0, "side": "outside" },
            "ids": [id],
        })
        .to_string(),
    );
    let list = run(&mut kernel, "cam.list", "");
    let second = list[1]["estimatedSeconds"].as_f64().expect("seconds");
    assert!(
        (second - accelerated).abs() < 1e-6,
        "{second} {accelerated}"
    );
    let error = kernel
        .apply("prefs.set", r#"{"toolpaths":{"estimateScale":0}}"#)
        .expect_err("a zero scale");
    assert!(matches!(error, CoreError::Validation(_)), "{error}");
}

#[test]
fn a_moulding_template_keeps_its_profile_after_its_rails() {
    let (temp, mut kernel) = temp_kernel("kernel-moulding-template");
    let layer = |kernel: &mut Kernel, name: &str| {
        run(kernel, "layers.add", &json!({ "name": name }).to_string())["id"]
            .as_u64()
            .expect("layer")
    };
    let add = |kernel: &mut Kernel, layer: u64, path: &crate::geom::Path| {
        let args =
            json!({ "entity": { "kind": "path", "layer": layer, "sheet": 1, "path": path } });
        run(kernel, "entities.create", &args.to_string())["id"]
            .as_u64()
            .expect("id")
    };
    let rail_path = crate::geom::line(Point::new(10.0, 10.0), Point::new(60.0, 10.0));
    let mut profile = crate::geom::SubPath::open(Point::new(0.0, 0.0));
    profile.line_to(Point::new(5.0, 3.0));
    profile.line_to(Point::new(10.0, 0.0));
    let profile_path = profile.into_path();
    let rails = layer(&mut kernel, "Rail");
    let profiles = layer(&mut kernel, "Profile");
    let rail = add(&mut kernel, rails, &rail_path);
    let shape = add(&mut kernel, profiles, &profile_path);
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "moulding", "name": "Trim", "tool": end_mill(),
                 "params": {}, "ids": [rail, shape] })
        .to_string(),
    );
    let path = temp.path().join("moulding.json");
    run(
        &mut kernel,
        "cam.saveTemplate",
        &json!({ "path": path }).to_string(),
    );

    // A new job drawn the other way round: the profile before the rail.
    run(&mut kernel, "doc.new", "");
    let profiles = layer(&mut kernel, "Profile");
    let rails = layer(&mut kernel, "Rail");
    let shape = add(&mut kernel, profiles, &profile_path);
    let rail = add(&mut kernel, rails, &rail_path);
    let reply = run(
        &mut kernel,
        "cam.loadTemplate",
        &json!({ "path": path }).to_string(),
    );
    let id = reply["ids"][0].as_u64().expect("a toolpath");
    let request = Request::of(kernel.document().toolpath(id).expect("tp")).expect("req");
    assert_eq!(request.ids, vec![rail, shape]);
    assert!(!kernel.document().toolpath(id).expect("tp").moves.is_empty());
}

#[test]
fn a_material_texture_is_read_for_the_view_and_chosen_per_job() {
    let (temp, mut kernel) = temp_kernel("kernel-texture");
    let png = temp.path().join("grain.png");
    image::RgbImage::from_pixel(8, 4, image::Rgb([180, 140, 90]))
        .save(&png)
        .expect("png");
    let reply = kernel
        .apply("sim.texture", &json!({ "path": png }).to_string())
        .expect("texture");
    let KernelReply::Bytes(blob) = reply else {
        panic!("expected bytes");
    };
    let length = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let header: Value = serde_json::from_slice(&blob[4..4 + length]).expect("header");
    assert_eq!(header["mime"], "image/png");
    assert_eq!(
        blob.len(),
        4 + length + header["sections"][0]["count"].as_u64().unwrap_or(0) as usize
    );
    let text = temp.path().join("notes.txt");
    std::fs::write(&text, "not a picture").expect("text");
    assert!(kernel
        .apply("sim.texture", &json!({ "path": text }).to_string())
        .is_err());

    run(
        &mut kernel,
        "prefs.set",
        &json!({ "view3d": { "textures": [
            { "id": 3, "name": "Grain", "category": "Wood", "path": png }
        ] } })
        .to_string(),
    );
    let twice = json!({ "view3d": { "textures": [
        { "id": 3, "name": "A", "category": "Wood", "path": png },
        { "id": 3, "name": "B", "category": "Wood", "path": png }
    ] } });
    assert!(kernel.apply("prefs.set", &twice.to_string()).is_err());
    let mut job = run(&mut kernel, "job.get", "");
    job["appearance"]["texture"] = json!(3);
    run(&mut kernel, "job.set", &job.to_string());
    assert_eq!(kernel.document().job_setup().appearance.texture, 3);
}

fn profile_on(kernel: &mut Kernel, id: u64, name: &str, extra: Value) -> u64 {
    let mut request = json!({
        "strategy": "profile", "tool": end_mill(), "name": name,
        "params": { "cutDepth": 6.0, "side": "outside" }, "ids": [id],
    });
    if let (Some(object), Value::Object(more)) = (request.as_object_mut(), extra) {
        object.extend(more);
    }
    run(kernel, "cam.compute", &request.to_string())["id"]
        .as_u64()
        .expect("an id")
}

fn list_order(kernel: &mut Kernel) -> Vec<(String, u64)> {
    run(kernel, "cam.list", "")
        .as_array()
        .expect("list")
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap_or("").to_string(),
                t["group"].as_u64().unwrap_or(0),
            )
        })
        .collect()
}

#[test]
fn new_toolpaths_join_the_selected_group_when_the_preference_says_so() {
    let (_temp, mut kernel) = temp_kernel("kernel-selected-group");
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let a = profile_on(&mut kernel, id, "A", json!({}));
    profile_on(&mut kernel, id, "B", json!({}));
    let group = run(
        &mut kernel,
        "cam.groups.create",
        &json!({ "ids": [a] }).to_string(),
    )["id"]
        .as_u64()
        .expect("group");
    profile_on(&mut kernel, id, "C", json!({ "selectedGroup": group }));
    let names: Vec<(String, u64)> = list_order(&mut kernel);
    assert_eq!(
        names,
        vec![
            ("A".to_string(), group),
            ("C".to_string(), group),
            ("B".to_string(), 0)
        ]
    );
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"appendToSelectedGroup":false}}"#,
    );
    profile_on(&mut kernel, id, "D", json!({ "selectedGroup": group }));
    assert_eq!(list_order(&mut kernel)[3], ("D".to_string(), 0));
}

#[test]
fn a_duplicate_sits_beside_its_original_unless_appended() {
    let (_temp, mut kernel) = temp_kernel("kernel-duplicate");
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let a = profile_on(&mut kernel, id, "A", json!({}));
    profile_on(&mut kernel, id, "B", json!({}));
    let group = run(
        &mut kernel,
        "cam.groups.create",
        &json!({ "ids": [a] }).to_string(),
    )["id"]
        .as_u64()
        .expect("group");
    let copy = run(
        &mut kernel,
        "cam.duplicate",
        &json!({ "id": a, "name": "A Copy" }).to_string(),
    );
    assert!(copy["id"].as_u64().is_some());
    let names = list_order(&mut kernel);
    assert_eq!(names[1], ("A Copy".to_string(), group), "{names:?}");
    assert_eq!(names[2].0, "B");

    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"appendDuplicated":true}}"#,
    );
    run(
        &mut kernel,
        "cam.duplicate",
        &json!({ "id": a, "name": "" }).to_string(),
    );
    let names = list_order(&mut kernel);
    assert_eq!(names.last(), Some(&("A Copy".to_string(), 0)), "{names:?}");
    assert_eq!(names.len(), 4);
}

#[test]
fn the_depth_check_follows_the_check_all_preference() {
    let (_temp, mut kernel) = temp_kernel("kernel-depth-check");
    let id = make_rect(&mut kernel, 0.0, 0.0, 50.0, 30.0);
    let a = profile_on(&mut kernel, id, "Deep", json!({}));
    let mut job = run(&mut kernel, "job.get", "");
    job["thickness"] = json!(4.0);
    run(&mut kernel, "job.set", &job.to_string());
    let check = run(&mut kernel, "cam.depthCheck", "");
    assert_eq!(check["all"], false);
    assert_eq!(check["toolpaths"][0]["name"], "Deep");
    run(
        &mut kernel,
        "cam.setVisible",
        &json!({ "ids": [a], "visible": false }).to_string(),
    );
    let check = run(&mut kernel, "cam.depthCheck", "");
    assert_eq!(check["toolpaths"].as_array().map(Vec::len), Some(0));
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"checkAllDepths":true}}"#,
    );
    let check = run(&mut kernel, "cam.depthCheck", "");
    assert_eq!(check["all"], true);
    assert_eq!(check["toolpaths"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_vector_that_crosses_itself_is_fixed_within_the_timeout() {
    let (_temp, mut kernel) = temp_kernel("kernel-geometry-fix");
    let args = json!({ "entity": { "kind": "path", "path": {
        "subpaths": [{ "start": { "x": 0.0, "y": 0.0 },
                       "segments": [
                           { "kind": "line", "to": { "x": 40.0, "y": 40.0 } },
                           { "kind": "line", "to": { "x": 40.0, "y": 0.0 } },
                           { "kind": "line", "to": { "x": 0.0, "y": 40.0 } }],
                       "closed": true }] } } });
    let bowtie = run(&mut kernel, "entities.create", &args.to_string())["id"]
        .as_u64()
        .expect("id");
    let request = json!({ "strategy": "pocket", "tool": end_mill(),
                          "params": { "cutDepth": 3.0 }, "ids": [bowtie] })
    .to_string();
    let fixed = run(&mut kernel, "cam.compute", &request);
    assert!(
        fixed["warnings"].to_string().contains("crossed itself"),
        "{fixed}"
    );
    // The reply carries a code and parameters the front end words itself.
    assert_eq!(fixed["warnings"][0]["code"], "fix.repaired", "{fixed}");
    assert_eq!(fixed["warnings"][0]["params"]["count"], 1, "{fixed}");
    run(
        &mut kernel,
        "prefs.set",
        r#"{"toolpaths":{"geometryFixSeconds":0}}"#,
    );
    // As drawn, the two lobes cancel out: there is no area to pocket.
    let as_drawn = kernel.apply("cam.compute", &request);
    assert!(as_drawn.is_err(), "the unrepaired shape has no inside");
}

#[test]
fn an_array_copy_hides_its_sources_groups_the_copies_and_recalculates() {
    let (_temp, mut kernel) = temp_kernel("kernel-array-copy");
    let id = make_rect(&mut kernel, 0.0, 0.0, 20.0, 10.0);
    let reply = run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 3.0, "side": "outside" },
            "ids": [id],
        })
        .to_string(),
    );
    let source = reply["id"].as_u64().expect("an id");
    let array = run(
        &mut kernel,
        "cam.arrayCopy",
        &json!({
            "ids": [source],
            "spec": { "rows": 2, "columns": 3, "spacing": "gap", "x": 5.0, "y": 5.0 },
        })
        .to_string(),
    );
    let copy = array["ids"][0].as_u64().expect("a copy");
    let list = run(&mut kernel, "cam.list", "");
    let row = |id: u64| {
        list.as_array()
            .and_then(|l| l.iter().find(|t| t["id"] == id).cloned())
            .expect("listed")
    };
    assert_eq!(row(source)["visible"], false);
    assert_eq!(row(copy)["name"], "Profile 1 Array");
    assert_eq!(row(copy)["group"], array["group"]);
    let moves = row(copy)["moves"].as_u64().unwrap_or(0);
    let again = run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": copy }).to_string(),
    );
    assert_eq!(again["id"], copy);
    let list = run(&mut kernel, "cam.list", "");
    let after = list
        .as_array()
        .and_then(|l| l.iter().find(|t| t["id"] == copy).cloned())
        .expect("listed");
    assert_eq!(after["moves"].as_u64().unwrap_or(0), moves);
}

mod array_copy_review_tests {
    use super::*;

    fn profile_of(kernel: &mut Kernel, id: u64, tool: Value) -> u64 {
        run(
            kernel,
            "cam.compute",
            &json!({
                "strategy": "profile",
                "tool": tool,
                "params": { "cutDepth": 3.0, "side": "outside" },
                "ids": [id],
            })
            .to_string(),
        )["id"]
            .as_u64()
            .expect("an id")
    }

    fn small_mill() -> Value {
        serde_json::to_value(crate::cam::tool::Tool {
            name: "End Mill 3 mm".to_string(),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter: 3.0 },
            pass_depth: 3.0,
            ..crate::cam::tool::Tool::default()
        })
        .expect("tool serialises")
    }

    fn moves_of(kernel: &Kernel, id: u64) -> Vec<crate::cam::toolpath::Move> {
        kernel
            .document()
            .toolpath(id)
            .expect("the toolpath")
            .moves
            .clone()
    }

    #[test]
    fn a_gap_array_recalculates_to_the_same_spacing_for_each_tool() {
        let (_temp, mut kernel) = temp_kernel("kernel-array-gap-review");
        let small = make_rect(&mut kernel, 0.0, 0.0, 20.0, 10.0);
        let large = make_rect(&mut kernel, 30.0, 0.0, 90.0, 10.0);
        let a = profile_of(&mut kernel, small, end_mill());
        let b = profile_of(&mut kernel, large, small_mill());
        let array = run(
            &mut kernel,
            "cam.arrayCopy",
            &json!({
                "ids": [a, b],
                "spec": { "rows": 1, "columns": 2, "spacing": "gap", "x": 5.0, "y": 5.0 },
            })
            .to_string(),
        );
        let copies: Vec<u64> = array["ids"]
            .as_array()
            .map(|l| l.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        assert_eq!(copies.len(), 2, "{array}");
        for copy in copies {
            let before = moves_of(&kernel, copy);
            run(
                &mut kernel,
                "cam.recalculate",
                &json!({ "id": copy }).to_string(),
            );
            assert_eq!(moves_of(&kernel, copy), before, "copy {copy} moved");
        }
    }

    #[test]
    fn a_source_given_another_tool_is_not_dropped_from_its_copies() {
        let (_temp, mut kernel) = temp_kernel("kernel-array-tool-review");
        let left = make_rect(&mut kernel, 0.0, 0.0, 20.0, 10.0);
        let right = make_rect(&mut kernel, 30.0, 0.0, 50.0, 10.0);
        let a = profile_of(&mut kernel, left, end_mill());
        let b = profile_of(&mut kernel, right, end_mill());
        let array = run(
            &mut kernel,
            "cam.arrayCopy",
            &json!({
                "ids": [a, b],
                "spec": { "rows": 1, "columns": 2, "spacing": "offset", "x": 100.0, "y": 0.0 },
            })
            .to_string(),
        );
        let copy = array["ids"][0].as_u64().expect("one joined copy");
        run(
            &mut kernel,
            "cam.recalculate",
            &json!({
                "id": b,
                "request": {
                    "strategy": "profile",
                    "tool": small_mill(),
                    "params": { "cutDepth": 3.0, "side": "outside" },
                    "ids": [right],
                },
            })
            .to_string(),
        );
        // One toolpath cuts with one tool: the copy is refused, not made
        // again without the right source's copies.
        let again = kernel.apply("cam.recalculate", &json!({ "id": copy }).to_string());
        assert!(again.is_err(), "recalculated without the right source");
        let reaches_right = moves_of(&kernel, copy)
            .iter()
            .any(|m| m.to.z < 0.0 && m.to.x > 125.0 && m.to.x < 155.0);
        assert!(reaches_right, "the right source's copy is gone");
    }

    #[test]
    fn an_array_too_large_to_cut_is_refused() {
        let (_temp, mut kernel) = temp_kernel("kernel-array-size-review");
        let id = make_rect(&mut kernel, 0.0, 0.0, 20.0, 10.0);
        let a = profile_of(&mut kernel, id, end_mill());
        for (rows, columns) in [(u64::from(u32::MAX), 2), (100_000, 100_000)] {
            let reply = kernel.apply(
                "cam.arrayCopy",
                &json!({
                    "ids": [a],
                    "spec": { "rows": rows, "columns": columns, "spacing": "offset", "x": 1.0, "y": 1.0 },
                })
                .to_string(),
            );
            assert!(reply.is_err(), "{rows} by {columns} was accepted");
        }
    }
}

mod tabs_review_tests {
    use super::*;

    /// Moves at the tabs' top (a 6 mm cut leaving 2 mm tabs).
    fn tab_moves(kernel: &Kernel, id: u64) -> usize {
        kernel
            .document()
            .toolpath(id)
            .expect("the toolpath")
            .moves
            .iter()
            .filter(|m| (m.to.z + 4.0).abs() < 1e-6)
            .count()
    }

    #[test]
    fn edited_tabs_follow_their_vector_when_it_moves() {
        let (_temp, mut kernel) = temp_kernel("kernel-tabs-review");
        let id = make_rect(&mut kernel, 0.0, 0.0, 100.0, 50.0);
        let spec = json!({ "enabled": true, "count": 4, "length": 10.0, "thickness": 2.0 });
        let placed = run(
            &mut kernel,
            "cam.tabs",
            &json!({ "ids": [id], "tabs": spec, "op": "auto", "reach": 1.0 }).to_string(),
        );
        let positions = placed["positions"].clone();
        assert_eq!(positions.as_array().map(Vec::len), Some(4));
        let toolpath = run(
            &mut kernel,
            "cam.compute",
            &json!({
                "strategy": "profile",
                "tool": end_mill(),
                "params": {
                    "cutDepth": 6.0,
                    "side": "outside",
                    "tabs": spec,
                    "tabPositions": positions,
                },
                "ids": [id],
            })
            .to_string(),
        )["id"]
            .as_u64()
            .expect("an id");
        let before = tab_moves(&kernel, toolpath);
        assert!(before > 0, "no tabs cut");
        run(
            &mut kernel,
            "ops.move",
            &json!({ "ids": [id], "dx": 200.0, "dy": 100.0 }).to_string(),
        );
        run(
            &mut kernel,
            "cam.recalculate",
            &json!({ "id": toolpath }).to_string(),
        );
        assert_eq!(tab_moves(&kernel, toolpath), before, "the tabs were lost");
    }

    #[test]
    fn an_inlay_insert_keeps_its_placed_tabs_on_its_vector_when_it_moves() {
        let (_temp, mut kernel) = temp_kernel("kernel-tabs-inlay");
        let id = make_rect(&mut kernel, 0.0, 0.0, 100.0, 50.0);
        let spec = json!({ "enabled": true, "count": 4, "length": 10.0, "thickness": 2.0 });
        let placed = run(
            &mut kernel,
            "cam.tabs",
            &json!({ "ids": [id], "tabs": spec, "op": "auto", "reach": 1.0 }).to_string(),
        );
        let positions = placed["positions"].clone();
        let toolpath = run(
            &mut kernel,
            "cam.compute",
            &json!({
                "strategy": "straightInlay",
                "tool": end_mill(),
                "params": {
                    "part": "male",
                    "male": "straight",
                    "cutDepth": 6.0,
                    "tabs": spec,
                    "profileOptions": {
                        "cutDepth": 6.0,
                        "side": "outside",
                        "tabs": spec,
                        "tabPositions": positions,
                    },
                },
                "ids": [id],
            })
            .to_string(),
        )["id"]
            .as_u64()
            .expect("an id");
        let stored = kernel
            .document()
            .toolpath(toolpath)
            .expect("toolpath")
            .params_json["params"]["profileOptions"]["tabAnchors"]
            .as_array()
            .map(Vec::len);
        assert_eq!(stored, Some(4), "the tabs were not anchored");
        let before = tab_moves(&kernel, toolpath);
        assert!(before > 0, "no tabs cut");
        run(
            &mut kernel,
            "ops.move",
            &json!({ "ids": [id], "dx": 200.0, "dy": 100.0 }).to_string(),
        );
        run(
            &mut kernel,
            "cam.recalculate",
            &json!({ "id": toolpath }).to_string(),
        );
        assert_eq!(tab_moves(&kernel, toolpath), before, "the tabs were lost");
    }

    #[test]
    fn a_tab_moved_in_the_form_is_not_put_back_by_its_old_anchor() {
        let (_temp, mut kernel) = temp_kernel("kernel-tabs-edit-review");
        let id = make_rect(&mut kernel, 0.0, 0.0, 100.0, 50.0);
        let spec = json!({ "enabled": true, "count": 2, "length": 10.0, "thickness": 2.0 });
        let request = |params: Value| json!({ "strategy": "profile", "tool": end_mill(), "params": params, "ids": [id] });
        let toolpath = run(
            &mut kernel,
            "cam.compute",
            &request(json!({
                "cutDepth": 6.0,
                "side": "outside",
                "tabs": spec,
                "tabPositions": [{ "x": 50.0, "y": 0.0 }, { "x": 50.0, "y": 50.0 }],
            }))
            .to_string(),
        )["id"]
            .as_u64()
            .expect("an id");
        let stored = |kernel: &Kernel| {
            kernel
                .document()
                .toolpath(toolpath)
                .expect("toolpath")
                .params_json["params"]
                .clone()
        };
        let mut params = stored(&kernel);
        assert_eq!(params["tabAnchors"].as_array().map(Vec::len), Some(2));
        // The form sends the stored anchors back with the first tab dragged.
        params["tabPositions"][0] = json!({ "x": 20.0, "y": 0.0 });
        run(
            &mut kernel,
            "cam.recalculate",
            &json!({ "id": toolpath, "request": request(params) }).to_string(),
        );
        let now = stored(&kernel);
        assert_eq!(now["tabPositions"][0], json!({ "x": 20.0, "y": 0.0 }));
        assert_eq!(now["tabPositions"][1], json!({ "x": 50.0, "y": 50.0 }));
    }
}

#[test]
fn an_older_two_tool_pocket_opens_in_cutting_order_and_edits_into_a_tool_list() {
    let (temp, mut kernel) = temp_kernel("kernel-legacy-pocket");
    let id = make_rect(&mut kernel, 0.0, 0.0, 80.0, 60.0);
    let mill = |name: &str, diameter: f64| {
        serde_json::to_value(crate::cam::tool::Tool {
            name: name.to_string(),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
            stepover: diameter * 0.4,
            ..crate::cam::tool::Tool::default()
        })
        .expect("tool")
    };
    let small = mill("End Mill 3 mm", 3.0);
    let big = mill("End Mill 12 mm", 12.0);
    // A pocket as saved before the tool list: a main tool and a larger
    // clearance tool, two toolpaths with the clearance one first.
    let made = run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "pocket", "tool": small, "ids": [id], "name": "Pocket",
                 "params": { "cutDepth": 3.0, "clearanceTool": big } })
        .to_string(),
    );
    let main = made["id"].as_u64().expect("main");
    let path = temp.path().join("legacy.carve");
    let file = json!({ "path": path.to_string_lossy() }).to_string();
    run(&mut kernel, "doc.save", &file);
    run(&mut kernel, "doc.new", "");
    run(&mut kernel, "doc.open", &file);
    let list = run(&mut kernel, "cam.list", "");
    let names: Vec<&str> = list
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, ["Pocket [End Mill 12 mm]", "Pocket [End Mill 3 mm]"]);
    let clearance = list[0]["id"].as_u64().expect("clearance");
    // Either toolpath opens the same request: the main tool with the
    // clearance tool beside it, which the form puts first in the list.
    for opened in [main, clearance] {
        let got = run(&mut kernel, "cam.get", &json!({ "id": opened }).to_string());
        assert_eq!(got["request"]["tool"]["name"], "End Mill 3 mm");
        assert_eq!(
            got["request"]["params"]["clearanceTool"]["name"],
            "End Mill 12 mm"
        );
    }

    // Calculated again from the form (the clearance tool first, then the
    // main tool), from the clearance toolpath: one family of two
    // toolpaths, and the old clearance toolpath is gone.
    let edited = json!({ "strategy": "pocket", "tool": big, "ids": [id], "name": "Pocket",
                         "params": { "cutDepth": 3.0, "clearanceTool": null,
                                     "restTools": [small] } });
    let reply = run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": clearance, "request": edited }).to_string(),
    );
    assert_eq!(reply["id"].as_u64(), Some(main));
    let list = run(&mut kernel, "cam.list", "");
    let rows: Vec<(u64, &str)> = list
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|t| Some((t["id"].as_u64()?, t["name"].as_str()?)))
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0], (main, "Pocket [End Mill 12 mm]"));
    assert_eq!(rows[1].1, "Pocket [End Mill 3 mm]");
    assert!(kernel.document().toolpath(clearance).is_err());
    let got = run(&mut kernel, "cam.get", &json!({ "id": main }).to_string());
    assert!(got["request"]["params"]["clearanceTool"].is_null());
    assert_eq!(
        got["request"]["params"]["restTools"][0]["name"],
        "End Mill 3 mm"
    );
    // One undo step puts the two older toolpaths back.
    run(&mut kernel, "undo", "");
    assert!(kernel.document().toolpath(clearance).is_ok());
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(2)
    );

    // Edited from the main toolpath it goes the same way.
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": main, "request": edited }).to_string(),
    );
    assert!(kernel.document().toolpath(clearance).is_err());
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(2)
    );
}
