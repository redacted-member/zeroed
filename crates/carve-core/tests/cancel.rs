//! Stopping a long command: the progress the host reads, and a nest or a
//! 3D finish asked to stop failing as cancelled with the job left as it
//! was. The stop request is kept for the whole process, so these checks
//! live in a test binary of their own and run one after the other.

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::geom::{circle, rect, Point};
use carve_core::post::PostLibrary;
use carve_core::progress;
use carve_core::{CoreError, Kernel, KernelReply};
use serde_json::{json, Value};

fn run(kernel: &mut Kernel, name: &str, args: Value) -> Value {
    match kernel.apply(name, &args.to_string()).expect(name) {
        KernelReply::Json(text) => {
            serde_json::from_str::<Value>(&text).expect("JSON")["result"].clone()
        }
        KernelReply::Bytes(_) => Value::Null,
    }
}

fn kernel(dir: &std::path::Path) -> Kernel {
    let mut kernel = Kernel::with_cam(CamState::new(
        ToolStore::at(dir.join("tools.json")),
        PostLibrary::at(dir.join("posts")),
    ));
    let job = carve_core::doc::JobSetup {
        width: 600.0,
        height: 400.0,
        thickness: 20.0,
        ..Default::default()
    };
    run(
        &mut kernel,
        "job.set",
        serde_json::to_value(job).expect("job"),
    );
    kernel
}

#[test]
fn a_stopped_command_fails_as_cancelled_and_changes_nothing() {
    let dir = std::env::temp_dir().join(format!("carve-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");

    // What the host reads while a command runs.
    assert!(!progress::status().running);
    assert!(!progress::request_stop(), "nothing is running");
    progress::begin();
    progress::report_steps(1, 4);
    assert_eq!(
        progress::status(),
        progress::Status {
            running: true,
            progress: 0.25
        }
    );
    assert!(progress::check().is_ok());
    assert!(progress::request_stop());
    assert!(matches!(progress::check(), Err(CoreError::Cancelled)));
    progress::end();
    assert!(!progress::status().running);
    assert!(
        progress::check().is_ok(),
        "a finished command drops the stop"
    );

    // Nest Parts asked to stop.
    let mut k = kernel(&dir);
    let mut ids = Vec::new();
    for n in 0..12 {
        let x = 20.0 + f64::from(n) * 45.0;
        let path = rect(Point::new(x, 20.0), Point::new(x + 30.0, 60.0)).expect("rect");
        ids.push(
            run(
                &mut k,
                "entities.create",
                json!({ "entity": { "kind": "path", "path": path } }),
            )["id"]
                .as_u64()
                .expect("id"),
        );
    }
    let before = run(&mut k, "entities.list", json!(null));
    let nest = json!({ "ids": ids }).to_string();
    progress::begin();
    progress::request_stop();
    let stopped = k.apply("ops.nest", &nest);
    progress::end();
    assert!(matches!(stopped, Err(CoreError::Cancelled)), "{stopped:?}");
    assert_eq!(run(&mut k, "entities.list", json!(null)), before);
    // The next command runs as usual.
    let reply = k.call("ops.nest", &nest).into_framed();
    assert!(String::from_utf8_lossy(&reply).contains("\"ok\":true"));

    // A 3D finish asked to stop.
    let mut k = kernel(&dir);
    let dome = circle(Point::new(300.0, 200.0), 80.0).expect("circle");
    let id = run(
        &mut k,
        "entities.create",
        json!({ "entity": { "kind": "path", "path": dome } }),
    )["id"]
        .as_u64()
        .expect("id");
    run(&mut k, "model.createShape", json!({ "ids": [id] }));
    let mut tool = Tool {
        name: "Ball".into(),
        tool_type: ToolGeometry::BallNose { diameter: 6.0 },
        ..Tool::default()
    };
    tool.set_stepover(3.0);
    let finish = json!({ "strategy": "finish3d", "tool": tool, "params": {}, "ids": [] });
    for strategy in ["raster", "rasterXY", "offset"] {
        let mut request = finish.clone();
        request["params"]["strategy"] = json!(strategy);
        progress::begin();
        progress::request_stop();
        let stopped = k.apply("cam.compute", &request.to_string());
        progress::end();
        assert!(
            matches!(stopped, Err(CoreError::Cancelled)),
            "{strategy}: {stopped:?}"
        );
        assert_eq!(run(&mut k, "cam.list", json!(null)), json!([]));
    }
    let reply = k.call("cam.compute", &finish.to_string()).into_framed();
    assert!(String::from_utf8_lossy(&reply).contains("\"ok\":true"));

    // The moves made after the points drop are part of the progress: it
    // never shows done while the finish still runs (it sat at 100 percent
    // for a third of a 0.1 mm finish, where Esc did nothing).
    let mut fine = tool.clone();
    fine.set_stepover(0.2);
    let request = json!({ "strategy": "finish3d", "tool": fine, "params": {}, "ids": [] });
    let worker = std::thread::spawn(move || {
        let reply = k.call("cam.compute", &request.to_string()).into_framed();
        String::from_utf8_lossy(&reply).contains("\"ok\":true")
    });
    let mut seen = 0.0f64;
    while !worker.is_finished() {
        let status = progress::status();
        if status.running {
            seen = seen.max(status.progress);
        }
        std::thread::yield_now();
    }
    assert!(worker.join().expect("finish thread"), "the finish answers");
    assert!(seen > 0.0, "the finish reports progress");
    assert!(seen < 1.0, "progress showed done while running: {seen}");
    std::fs::remove_dir_all(&dir).ok();
}
