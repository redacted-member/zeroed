//! Joints and the toolpaths that cut their parts, through the kernel.

use super::*;

fn mates(kernel: &mut Kernel) -> Vec<Value> {
    run(kernel, "assembly.pieces", "")["mates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Every fit side's smallest and largest gap, from Verify Fit.
fn fit_gaps(kernel: &mut Kernel, target: f64) -> Vec<(f64, f64)> {
    let reply = run(
        kernel,
        "assembly.verifyFit",
        &json!({ "target": target, "tolerance": 0.05 }).to_string(),
    );
    reply["joints"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .filter(|s| s["fit"] == true)
        .map(|s| {
            (
                s["min"].as_f64().unwrap_or(f64::NAN),
                s["max"].as_f64().unwrap_or(f64::NAN),
            )
        })
        .collect()
}

/// The Joint Creator without Make the toolpaths too, then a profile of the
/// user's own: the parts are cut free only by that profile, and its step
/// mates them, so the Assembly View puts them together as the joint does.
#[test]
fn a_profile_made_after_a_joint_mates_its_parts_in_the_same_step() {
    let (_temp, mut kernel) = temp_kernel("kernel-joint-then-profile");
    let mut job = run(&mut kernel, "job.get", "");
    job["width"] = json!(300.0);
    job["height"] = json!(200.0);
    job["thickness"] = json!(12.0);
    run(&mut kernel, "job.set", &job.to_string());
    let a = make_rect(&mut kernel, 10.0, 10.0, 110.0, 60.0);
    let b = make_rect(&mut kernel, 10.0, 100.0, 110.0, 150.0);
    let spec = json!({
        "kind": "finger",
        "a": { "kind": "edge", "vector": a, "from": [110.0, 60.0], "to": [10.0, 60.0] },
        "b": { "kind": "edge", "vector": b, "from": [10.0, 100.0], "to": [110.0, 100.0] },
        "params": {},
    });
    let added = run(&mut kernel, "joints.add", &spec.to_string());
    assert_eq!(added["mates"], 0, "no toolpaths yet: {added}");
    assert!(mates(&mut kernel).is_empty());

    run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": end_mill(),
            "params": { "cutDepth": 12.5, "side": "outside" },
            "ids": [a, b],
        })
        .to_string(),
    );
    let rows = mates(&mut kernel);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["kind"], "joint");
    assert_eq!(rows[0]["state"], "solved", "{rows:?}");
    let gaps = fit_gaps(&mut kernel, 0.1);
    assert!(!gaps.is_empty());
    for (min, max) in gaps {
        assert!(
            (min - 0.1).abs() < 0.01 && (max - 0.1).abs() < 0.01,
            "{min} {max}"
        );
    }

    // One undo takes the profile and its mate, redo brings both back.
    run(&mut kernel, "undo", "");
    assert!(run(&mut kernel, "cam.list", "")
        .as_array()
        .is_some_and(|l| l.is_empty()));
    assert!(mates(&mut kernel).is_empty());
    run(&mut kernel, "redo", "");
    assert_eq!(
        run(&mut kernel, "cam.list", "").as_array().map(Vec::len),
        Some(1)
    );
    let rows = mates(&mut kernel);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "solved");
}

/// The 6.35 mm end mill of the box project, as the tool database has it.
fn quarter_inch() -> Value {
    serde_json::to_value(crate::cam::tool::Tool {
        name: "End Mill 1/4 inch".to_string(),
        tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter: 6.35 },
        pass_depth: 3.175,
        ..crate::cam::tool::Tool::default()
    })
    .expect("tool serialises")
}

/// The box project (`docs/testing/wfB.md`): five panels 12 mm thick, Box
/// from Selection with finger joints, a test coupon, the parts nested 1 mm
/// apart and cut out by one outside profile with tabs.
#[test]
fn a_nested_finger_jointed_box_is_five_pieces_that_go_together() {
    box_project(true);
}

#[test]
fn a_finger_jointed_box_cut_where_it_was_drawn_goes_together() {
    box_project(false);
}

fn box_project(nest: bool) {
    let (_temp, mut kernel) = temp_kernel("kernel-box-project");
    let mut job = run(&mut kernel, "job.get", "");
    job["width"] = json!(600.0);
    job["height"] = json!(500.0);
    job["thickness"] = json!(12.0);
    run(&mut kernel, "job.set", &job.to_string());
    let panels = [
        make_rect(&mut kernel, 20.0, 20.0, 220.0, 170.0),
        make_rect(&mut kernel, 20.0, 200.0, 220.0, 300.0),
        make_rect(&mut kernel, 20.0, 330.0, 220.0, 430.0),
        make_rect(&mut kernel, 260.0, 20.0, 410.0, 120.0),
        make_rect(&mut kernel, 260.0, 150.0, 410.0, 250.0),
    ];
    let made = run(
        &mut kernel,
        "joints.box",
        &json!({ "ids": panels, "kind": "finger", "params": {} }).to_string(),
    );
    assert_eq!(made["ids"].as_array().map(Vec::len), Some(8), "{made}");
    let coupon = run(
        &mut kernel,
        "joints.coupon",
        &json!({ "kind": "finger", "params": {} }).to_string(),
    );
    let mut ids: Vec<u64> = panels.to_vec();
    ids.extend(
        coupon["parts"]
            .as_array()
            .expect("parts")
            .iter()
            .filter_map(Value::as_u64),
    );
    if nest {
        run(
            &mut kernel,
            "ops.nest",
            &json!({ "ids": ids, "params": { "toolDiameter": 6.35, "gap": 1.0, "border": 10.0,
                 "rotationStepDeg": 90.0, "sheetWidth": 600.0, "sheetHeight": 500.0 } })
            .to_string(),
        );
    }
    if nest {
        // A joint change after the nest keeps the parts where the nest put
        // them.
        let nested: Vec<_> = panels
            .iter()
            .map(|id| kernel.document().bbox_of(*id))
            .collect();
        let stays = |kernel: &Kernel| {
            for (id, was) in panels.iter().zip(&nested) {
                let (Some(now), Some(was)) = (kernel.document().bbox_of(*id), was) else {
                    panic!("panel {id} has no box");
                };
                assert!(close(now, *was), "{id}: {now:?} was {was:?}");
            }
        };
        // Commands that make no undo step of their own (selecting, placing
        // tabs) leave the nest alone.
        run(
            &mut kernel,
            "selection.set",
            &json!({ "ids": ids }).to_string(),
        );
        run(
            &mut kernel,
            "cam.tabs",
            &json!({ "ids": ids, "tabs": { "enabled": true, "count": 2, "length": 10.0,
                     "thickness": 3.0 }, "op": "auto", "reach": 1.0 })
            .to_string(),
        );
        stays(&kernel);
        let mut settings = kernel.document().assembly().joint_set.settings.clone();
        settings.fit = crate::joints::Fit::Slip;
        run(
            &mut kernel,
            "joints.setSettings",
            &json!({ "settings": settings }).to_string(),
        );
        settings.fit = crate::joints::Fit::Press;
        run(
            &mut kernel,
            "joints.setSettings",
            &json!({ "settings": settings }).to_string(),
        );
        stays(&kernel);
    }
    let tabs = json!({ "enabled": true, "count": 2, "length": 10.0, "thickness": 3.0 });
    let placed = run(
        &mut kernel,
        "cam.tabs",
        &json!({ "ids": ids, "tabs": tabs, "op": "auto", "reach": 1.0 }).to_string(),
    );
    run(
        &mut kernel,
        "cam.compute",
        &json!({
            "strategy": "profile",
            "tool": quarter_inch(),
            "params": { "cutDepth": 12.2, "side": "outside", "tabs": tabs,
                        "tabPositions": placed["positions"] },
            "ids": ids,
        })
        .to_string(),
    );
    let pieces = run(&mut kernel, "assembly.pieces", "");
    let sizes: Vec<Value> = pieces["pieces"]
        .as_array()
        .expect("pieces")
        .iter()
        .map(|p| p["size"].clone())
        .collect();
    // The five panels and the coupon's two parts; the slugs left in the
    // finger notches between two kerfs are waste.
    assert_eq!(sizes.len(), 7, "{sizes:?}");
    let rows = mates(&mut kernel);
    let states: Vec<Value> = rows.iter().map(|m| m["state"].clone()).collect();
    assert_eq!(rows.len(), 9, "{rows:?}");
    assert!(states.iter().all(|s| s == "solved"), "{rows:?}");
    let gaps = fit_gaps(&mut kernel, 0.1);
    assert!(gaps.len() >= 9, "{gaps:?}");
    for (min, max) in gaps {
        assert!(
            (min - 0.1).abs() < 0.01 && (max - 0.1).abs() < 0.01,
            "{min} {max}"
        );
    }
}

fn part_area(kernel: &Kernel, id: u64) -> f64 {
    match &kernel.document().entity(id).expect("part").kind {
        crate::doc::EntityKind::Path { path } => geom::path_area(path, geom::Tolerance::FLATTEN),
        _ => panic!("not a vector"),
    }
}

fn finger_pair(thickness: f64) -> (Kernel, u64) {
    let mut kernel = Kernel::new();
    let mut job = run(&mut kernel, "job.get", "");
    job["width"] = json!(300.0);
    job["height"] = json!(200.0);
    job["thickness"] = json!(thickness);
    run(&mut kernel, "job.set", &job.to_string());
    let a = make_rect(&mut kernel, 10.0, 10.0, 150.0, 90.0);
    let b = make_rect(&mut kernel, 10.0, 110.0, 150.0, 190.0);
    let spec = json!({
        "kind": "finger",
        "a": { "kind": "edge", "vector": a, "from": [150.0, 90.0], "to": [10.0, 90.0] },
        "b": { "kind": "edge", "vector": b, "from": [10.0, 110.0], "to": [150.0, 110.0] },
        "params": {},
    });
    run(&mut kernel, "joints.add", &spec.to_string());
    (kernel, a)
}

// Review 033 row 2: a new material thickness remakes the joints at it.
#[test]
fn a_new_material_thickness_remakes_the_joints_at_that_depth() {
    let (mut kernel, a) = finger_pair(18.0);
    let at_18 = part_area(&kernel, a);
    let (fresh, _) = finger_pair(24.0);
    let at_24 = part_area(&fresh, a);
    assert!((at_18 - at_24).abs() > 50.0, "{at_18} {at_24}");
    let mut job = run(&mut kernel, "job.get", "");
    job["thickness"] = json!(24.0);
    run(&mut kernel, "job.set", &job.to_string());
    assert!(
        (part_area(&kernel, a) - at_24).abs() < 1e-6,
        "{}",
        part_area(&kernel, a)
    );
    let listed = run(&mut kernel, "joints.list", "");
    assert!(listed.to_string().contains("24"), "{listed}");
    // One Undo puts the job and the joint back together.
    run(&mut kernel, "undo", "");
    assert_eq!(run(&mut kernel, "job.get", "")["thickness"], 18.0);
    assert!((part_area(&kernel, a) - at_18).abs() < 1e-6);
    run(&mut kernel, "redo", "");
    assert!((part_area(&kernel, a) - at_24).abs() < 1e-6);
}
