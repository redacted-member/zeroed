//! Double sided jobs through the kernel's command interface: switching
//! sides, previewing a through cut and posting each side in its own job
//! space.

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::geom::Point;
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("carve-double-sided-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creates the scratch folder");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn kernel(scratch: &Scratch) -> Kernel {
    Kernel::with_cam(CamState::new(
        ToolStore::at(scratch.0.join("tools.json")),
        PostLibrary::at(scratch.0.join("posts")),
    ))
}

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

fn double_sided(kernel: &mut Kernel) {
    let mut job = run(kernel, "job.get", json!({}));
    job["kind"] = json!("doubleSided");
    job["doubleSided"] = json!({ "flip": "horizontal", "bottomZZero": "top" });
    run(kernel, "job.set", job);
}

fn circle(kernel: &mut Kernel, x: f64, y: f64) -> u64 {
    let path = carve_core::geom::circle(Point::new(x, y), 10.0).expect("circle");
    let entity = json!({ "entity": { "kind": "path", "path": path } });
    run(kernel, "entities.create", entity)["id"]
        .as_u64()
        .expect("an id")
}

fn pocket(kernel: &mut Kernel, id: u64, depth: f64) -> u64 {
    let tool = serde_json::to_value(Tool {
        name: "End Mill 6 mm".to_string(),
        pass_depth: 20.0,
        tool_type: ToolGeometry::EndMill { diameter: 6.0 },
        ..Tool::default()
    })
    .expect("tool serialises");
    run(
        kernel,
        "cam.compute",
        json!({ "strategy": "pocket", "tool": tool, "ids": [id],
                "params": { "cutDepth": depth } }),
    )["id"]
        .as_u64()
        .expect("an id")
}

fn through_at(kernel: &mut Kernel, x: f64, y: f64) -> bool {
    run(kernel, "sim.run", json!({}));
    shown_through_at(kernel, x, y)
}

/// Whether the preview shown now is cut through at (x, y), without running
/// it again.
fn shown_through_at(kernel: &mut Kernel, x: f64, y: f64) -> bool {
    let KernelReply::Bytes(bytes) = kernel.apply("sim.heightfield", "").expect("heightfield")
    else {
        panic!("expected bytes");
    };
    let length = u32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")) as usize;
    let header: Value = serde_json::from_slice(&bytes[4..4 + length]).expect("header");
    let cell = header["cellSize"].as_f64().expect("cell");
    let cx = ((x - header["originX"].as_f64().expect("x")) / cell).round() as usize;
    let cy = ((y - header["originY"].as_f64().expect("y")) / cell).round() as usize;
    let index = cy * header["width"].as_u64().expect("width") as usize + cx;
    // Heights come as u16 steps, the top code standing for NaN.
    assert_eq!(header["sections"][0]["dtype"], "u16");
    let at = 4 + length + index * 2;
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("two bytes"))
        == carve_core::blob::HEIGHT_NAN
}

#[test]
fn a_through_cut_shows_from_both_sides_and_each_side_posts_in_its_own_space() {
    let scratch = Scratch::new("through");
    let mut kernel = kernel(&scratch);
    double_sided(&mut kernel);
    // An 18 mm board: 10 mm from each face meets in the middle.
    let top_circle = circle(&mut kernel, 60.0, 100.0);
    let top = pocket(&mut kernel, top_circle, 10.0);
    let info = run(&mut kernel, "doc.info", json!({}));
    assert_eq!(info["activeSide"], "top");

    run(&mut kernel, "job.setSide", json!({ "side": "bottom" }));
    assert_eq!(
        run(&mut kernel, "entities.list", json!({})),
        json!([]),
        "the bottom side starts empty"
    );
    // Turned over left to right, top X 60 is bottom X 240 on a 300 mm job.
    let bottom_circle = circle(&mut kernel, 240.0, 100.0);
    let bottom = pocket(&mut kernel, bottom_circle, 10.0);
    let list = run(&mut kernel, "cam.list", json!({}));
    assert_eq!(list[0]["side"], "top");
    assert_eq!(list[1]["side"], "bottom");
    assert_eq!(list[0]["stale"], false);

    assert!(
        through_at(&mut kernel, 240.0, 100.0),
        "seen from the bottom"
    );
    assert!(!through_at(&mut kernel, 60.0, 100.0));
    run(&mut kernel, "job.setSide", json!({ "side": "top" }));
    assert!(through_at(&mut kernel, 60.0, 100.0), "seen from the top");

    let path = scratch.0.join("side");
    let mixed = kernel.apply(
        "cam.save",
        &json!({ "toolpathIds": [top, bottom], "post": "grbl_mm", "path": path }).to_string(),
    );
    assert!(mixed.is_err(), "one file holds one side");
    for (id, centre) in [(top, 60.0), (bottom, 240.0)] {
        let saved = run(
            &mut kernel,
            "cam.save",
            json!({ "toolpathIds": [id], "post": "grbl_mm", "path": path }),
        );
        let file = saved["paths"][0].as_str().expect("a file").to_string();
        let gcode = std::fs::read_to_string(file).expect("reads");
        let xs: Vec<f64> = gcode
            .lines()
            .filter(|line| {
                line.starts_with("G1") || line.starts_with("G2") || line.starts_with("G3")
            })
            .filter_map(|line| line.split_once('X'))
            .filter_map(|(_, rest)| {
                let end = rest.find(char::is_alphabetic).unwrap_or(rest.len());
                rest[..end].parse().ok()
            })
            .collect();
        assert!(!xs.is_empty(), "{gcode}");
        assert!(
            xs.iter().all(|x| (x - centre).abs() <= 10.0),
            "side {id} cuts round X {centre}: {xs:?}"
        );
    }
}

#[test]
fn the_preview_turns_over_with_the_side_shown() {
    let scratch = Scratch::new("turn");
    let mut kernel = kernel(&scratch);
    double_sided(&mut kernel);
    let hole = circle(&mut kernel, 60.0, 100.0);
    pocket(&mut kernel, hole, 18.0);
    run(&mut kernel, "sim.run", json!({}));
    assert!(shown_through_at(&mut kernel, 60.0, 100.0));
    // Showing the bottom turns the preview over: the hole is at X 240 seen
    // from there, as the design is, without previewing again.
    run(&mut kernel, "job.setSide", json!({ "side": "bottom" }));
    assert!(shown_through_at(&mut kernel, 240.0, 100.0));
    assert!(!shown_through_at(&mut kernel, 60.0, 100.0));
    run(&mut kernel, "job.setSide", json!({ "side": "top" }));
    assert!(shown_through_at(&mut kernel, 60.0, 100.0));
    assert!(!shown_through_at(&mut kernel, 240.0, 100.0));
}

#[test]
fn a_toolpath_of_the_side_not_shown_recalculates_from_its_own_vectors() {
    let scratch = Scratch::new("recalc");
    let mut kernel = kernel(&scratch);
    double_sided(&mut kernel);
    let top_circle = circle(&mut kernel, 60.0, 100.0);
    let top = pocket(&mut kernel, top_circle, 5.0);
    run(&mut kernel, "job.setSide", json!({ "side": "bottom" }));
    let bottom_circle = circle(&mut kernel, 200.0, 50.0);
    let bottom = pocket(&mut kernel, bottom_circle, 5.0);
    // Shown the bottom, Recalculate All reaches the top's pocket too.
    let before = run(&mut kernel, "cam.get", json!({ "id": top }))["bounds"].clone();
    run(&mut kernel, "cam.recalculate", json!({ "id": top }));
    run(&mut kernel, "cam.recalculate", json!({ "id": bottom }));
    let info = run(&mut kernel, "doc.info", json!({}));
    assert_eq!(info["activeSide"], "bottom", "the side shown stays");
    let after = run(&mut kernel, "cam.get", json!({ "id": top }));
    assert_eq!(after["side"], "top");
    assert_eq!(after["stale"], false);
    assert_eq!(after["bounds"], before);
    assert_eq!(
        run(&mut kernel, "cam.get", json!({ "id": bottom }))["side"],
        "bottom"
    );
}

#[test]
fn swap_sides_is_one_undo_step() {
    let scratch = Scratch::new("swap");
    let mut kernel = kernel(&scratch);
    double_sided(&mut kernel);
    let id = circle(&mut kernel, 60.0, 100.0);
    run(&mut kernel, "job.swapSides", json!({}));
    assert_eq!(run(&mut kernel, "entities.list", json!({})), json!([]));
    run(&mut kernel, "job.setSide", json!({ "side": "bottom" }));
    assert_eq!(run(&mut kernel, "entities.list", json!({}))[0]["id"], id);
    // Looking at the bottom side is no step: Undo takes back the swap, on
    // the top side it was made on.
    assert_eq!(run(&mut kernel, "undo", json!({}))["label"], "Swap Sides");
    assert_eq!(run(&mut kernel, "entities.list", json!({}))[0]["id"], id);
    let info = run(&mut kernel, "doc.info", json!({}));
    assert_eq!(info["activeSide"], "top");
}

#[test]
fn a_single_sided_job_refuses_the_bottom_side() {
    let scratch = Scratch::new("single");
    let mut kernel = kernel(&scratch);
    assert!(kernel
        .apply("job.setSide", &json!({ "side": "bottom" }).to_string())
        .is_err());
    assert!(kernel.apply("job.swapSides", "{}").is_err());
}
