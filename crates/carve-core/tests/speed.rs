//! Toolpath calculation times for the heavier strategies on typical jobs,
//! as a report rather than a check (timings depend on the machine):
//! `cargo test --release -p carve-core --test speed -- --ignored --nocapture`.

use std::time::Instant;

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::geom::{circle, rect, Point, SubPath};
use carve_core::post::PostLibrary;
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

fn add(kernel: &mut Kernel, path: &carve_core::geom::Path) -> u64 {
    run(
        kernel,
        "entities.create",
        json!({ "entity": { "kind": "path", "path": path } }),
    )["id"]
        .as_u64()
        .expect("an id")
}

fn tool(kind: ToolGeometry, stepover: f64) -> Value {
    let mut tool = Tool {
        name: "T".to_string(),
        tool_type: kind,
        pass_depth: 3.0,
        ..Tool::default()
    };
    tool.set_stepover(stepover);
    serde_json::to_value(tool).expect("tool")
}

fn time(kernel: &mut Kernel, label: &str, request: Value) {
    let start = Instant::now();
    let reply = run(kernel, "cam.compute", request);
    let id = reply["id"].as_u64().expect("an id");
    let list = run(kernel, "cam.list", json!(null));
    let moves = list
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == json!(id)))
        .map_or(0, |r| r["moves"].as_u64().unwrap_or(0));
    println!(
        "{label:<40} {:>9.1} ms {moves:>8} moves",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

/// A 20 point star, radius 60 and 30, round `c`.
fn star(c: Point) -> carve_core::geom::Path {
    let mut sub = SubPath::open(Point::new(c.x + 60.0, c.y));
    for k in 1..40 {
        let a = std::f64::consts::PI * k as f64 / 20.0;
        let r = if k % 2 == 0 { 60.0 } else { 30.0 };
        sub.line_to(Point::new(c.x + r * a.cos(), c.y + r * a.sin()));
    }
    sub.close_with_line(carve_core::geom::Tolerance::new(0.001).expect("tol"));
    sub.into_path()
}

#[test]
#[ignore = "timing report, run in release with --ignored --nocapture"]
fn report_calculation_times() {
    let dir = std::env::temp_dir().join(format!("carve-speed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let mut kernel = Kernel::with_cam(CamState::new(
        ToolStore::at(dir.join("tools.json")),
        PostLibrary::at(dir.join("posts")),
    ));
    let job = carve_core::doc::JobSetup {
        width: 300.0,
        height: 200.0,
        thickness: 25.0,
        xy_origin: carve_core::doc::XyOrigin::BottomLeft,
        ..Default::default()
    };
    run(
        &mut kernel,
        "job.set",
        serde_json::to_value(job).expect("job"),
    );
    let end_mill = tool(ToolGeometry::EndMill { diameter: 6.0 }, 2.4);
    let ball = tool(ToolGeometry::BallNose { diameter: 3.0 }, 0.3);
    let vbit = tool(
        ToolGeometry::VBit {
            diameter: 25.0,
            included_angle: 90.0,
        },
        1.0,
    );

    let shapes: Vec<u64> = (0..6)
        .map(|k| {
            let c = Point::new(40.0 + 45.0 * k as f64, 100.0);
            let outer = circle(c, 20.0).expect("circle");
            add(&mut kernel, &outer)
        })
        .collect();
    let big = add(&mut kernel, &star(Point::new(150.0, 100.0)));
    let frame = add(
        &mut kernel,
        &rect(Point::new(5.0, 5.0), Point::new(295.0, 195.0)).expect("rect"),
    );

    time(
        &mut kernel,
        "Pocket, star, offset",
        json!({ "strategy": "pocket", "tool": end_mill,
                "params": { "cutDepth": 6.0 }, "ids": [big] }),
    );
    time(
        &mut kernel,
        "Pocket, frame, raster",
        json!({ "strategy": "pocket", "tool": end_mill,
                "params": { "cutDepth": 3.0, "strategy": "raster" }, "ids": [frame] }),
    );
    time(
        &mut kernel,
        "V-Groove, star",
        json!({ "strategy": "vgroove", "tool": vbit, "params": {}, "ids": [big] }),
    );
    time(
        &mut kernel,
        "V-Groove, six circles",
        json!({ "strategy": "vgroove", "tool": vbit, "params": {}, "ids": shapes }),
    );
    time(
        &mut kernel,
        "Profile, star",
        json!({ "strategy": "profile", "tool": end_mill,
                "params": { "cutDepth": 6.0, "side": "outside" }, "ids": [big] }),
    );

    // A 100 mm circle rail and a half round profile for the moulding.
    let rail = add(
        &mut kernel,
        &circle(Point::new(150.0, 100.0), 50.0).expect("rail"),
    );
    let mut sub = SubPath::open(Point::new(0.0, 0.0));
    sub.arc_to(Point::new(20.0, 0.0), -1.0);
    let profile = add(&mut kernel, &sub.into_path());
    time(
        &mut kernel,
        "Moulding Along, circle rail",
        json!({ "strategy": "moulding", "tool": ball,
                "params": { "strategy": "along" }, "ids": [rail, profile] }),
    );
    time(
        &mut kernel,
        "Moulding Across, circle rail",
        json!({ "strategy": "moulding", "tool": ball,
                "params": { "strategy": "across" }, "ids": [rail, profile] }),
    );

    // A domed model over the star for the 3D strategies.
    run(
        &mut kernel,
        "model.createShape",
        json!({ "ids": [big], "params": { "profile": "round", "height": 12.0 } }),
    );
    time(
        &mut kernel,
        "3D Roughing",
        json!({ "strategy": "rough3d", "tool": end_mill, "params": {}, "ids": [] }),
    );
    time(
        &mut kernel,
        "3D Finishing, raster",
        json!({ "strategy": "finish3d", "tool": ball, "params": {}, "ids": [] }),
    );
    time(
        &mut kernel,
        "3D Finishing, offset",
        json!({ "strategy": "finish3d", "tool": ball,
                "params": { "strategy": "offset" }, "ids": [] }),
    );
    // The same model at the highest modelling resolution.
    let job = carve_core::doc::JobSetup {
        resolution: carve_core::doc::ModelResolution::Highest,
        ..job
    };
    run(
        &mut kernel,
        "job.set",
        serde_json::to_value(job).expect("job"),
    );
    time(
        &mut kernel,
        "3D Roughing, highest resolution",
        json!({ "strategy": "rough3d", "tool": end_mill, "params": {}, "ids": [] }),
    );
    time(
        &mut kernel,
        "3D Finishing raster, highest resolution",
        json!({ "strategy": "finish3d", "tool": ball, "params": {}, "ids": [] }),
    );
    time(
        &mut kernel,
        "3D Finishing offset, highest resolution",
        json!({ "strategy": "finish3d", "tool": ball,
                "params": { "strategy": "offset" }, "ids": [] }),
    );
    std::fs::remove_dir_all(&dir).ok();
}
