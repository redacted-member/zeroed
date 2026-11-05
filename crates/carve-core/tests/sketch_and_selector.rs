//! Sketch Carving, Laser Sketch and the Vector Selector through the
//! kernel's command interface: previewing sketch lines from an image file,
//! calculating both sketch toolpaths, and a laser toolpath whose vectors
//! are chosen by an associated filter surviving recalculation and a
//! template.

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::geom::{rect, Point};
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("carve-sketch-{}-{name}", std::process::id()));
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
        KernelReply::Bytes(_) => panic!("{name} answered bytes"),
    }
}

fn laser() -> Value {
    json!(Tool {
        name: "Diode".to_string(),
        tool_type: ToolGeometry::LaserTool {
            power: 60.0,
            speed: 1500.0,
            kerf: 0.1,
            max_burn_rate: carve_core::cam::tool::DEFAULT_MAX_BURN_RATE,
        },
        ..Tool::default()
    })
}

fn vbit() -> Value {
    json!(Tool {
        name: "V-Bit 60".to_string(),
        tool_type: ToolGeometry::VBit {
            diameter: 6.0,
            included_angle: 60.0,
        },
        pass_depth: 2.0,
        ..Tool::default()
    })
}

/// A white image with a black square in the middle, written as a PNG.
fn square_png(scratch: &Scratch) -> std::path::PathBuf {
    let image = image::GrayImage::from_fn(60, 60, |x, y| {
        if (15..45).contains(&x) && (15..45).contains(&y) {
            image::Luma([0u8])
        } else {
            image::Luma([255u8])
        }
    });
    let path = scratch.0.join("square.png");
    image.save(&path).expect("writes the png");
    path
}

fn rect_on(kernel: &mut Kernel, layer: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
    let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
    let args = json!({ "entity": { "kind": "path", "layer": layer, "sheet": 1, "path": path } });
    run(kernel, "entities.create", args)["id"]
        .as_u64()
        .expect("id")
}

#[test]
fn a_bitmap_is_previewed_carved_and_burnt_as_a_sketch() {
    let scratch = Scratch::new("bitmap");
    let mut kernel = kernel(&scratch);
    let png = square_png(&scratch);
    let trace = json!({ "source": "bitmap", "path": png, "origin": { "x": 10.0, "y": 10.0 },
                        "width": 60.0, "height": 0.0, "lineThickness": 30.0 });

    let preview = run(
        &mut kernel,
        "cam.sketchPreview",
        json!({ "params": trace, "ids": [] }),
    );
    let paths = preview["paths"].as_array().expect("paths");
    assert!(!paths.is_empty(), "{preview}");

    let mut carve = trace.clone();
    carve["startDepth"] = json!(0.0);
    carve["flatDepth"] = json!(1.0);
    let done = run(
        &mut kernel,
        "cam.compute",
        json!({ "strategy": "sketchCarve", "tool": vbit(), "params": carve, "ids": [] }),
    );
    let id = done["id"].as_u64().expect("id");
    let detail = run(&mut kernel, "cam.get", json!({ "id": id }));
    assert_eq!(detail["name"], "Sketch Carve 1");
    assert_eq!(detail["request"]["strategy"], "sketchCarve");

    let mut burn = trace.clone();
    burn["stepover"] = json!(0.25);
    burn["fill"] = json!("together");
    let done = run(
        &mut kernel,
        "cam.compute",
        json!({ "strategy": "laserSketch", "tool": laser(), "params": burn, "ids": [] }),
    );
    let id = done["id"].as_u64().expect("id");
    let detail = run(&mut kernel, "cam.get", json!({ "id": id }));
    assert_eq!(detail["kind"], "laser");

    let error = kernel
        .apply(
            "cam.compute",
            &json!({ "strategy": "laserSketch", "tool": vbit(), "params": burn, "ids": [] })
                .to_string(),
        )
        .expect_err("a V-bit is no laser");
    assert!(error.to_string().contains("laser"), "{error}");
}

#[test]
fn a_placed_bitmap_is_sketched_where_it_lies() {
    let scratch = Scratch::new("placed");
    let mut kernel = kernel(&scratch);
    let png = square_png(&scratch);
    let bitmap = run(&mut kernel, "io.importBitmap", json!({ "path": png }))["id"]
        .as_u64()
        .expect("bitmap");
    let params = json!({ "source": "bitmap", "bitmap": bitmap, "lineThickness": 30.0 });
    let preview = run(
        &mut kernel,
        "cam.sketchPreview",
        json!({ "params": params, "ids": [bitmap] }),
    );
    assert!(!preview["paths"].as_array().expect("paths").is_empty());
    // The bitmap's own outline in the selection is not taken as a boundary.
    let mut carve = params.clone();
    carve["boundary"] = json!("vector");
    let error = kernel
        .apply(
            "cam.compute",
            &json!({ "strategy": "sketchCarve", "tool": vbit(), "params": carve,
                     "ids": [bitmap] })
            .to_string(),
        )
        .expect_err("no boundary vector");
    assert!(error.to_string().contains("boundary"), "{error}");
    let done = run(
        &mut kernel,
        "cam.compute",
        json!({ "strategy": "sketchCarve", "tool": vbit(), "params": params, "ids": [bitmap] }),
    );
    assert!(done["id"].as_u64().is_some());
}

#[test]
fn an_associated_filter_picks_the_vectors_again_on_recalculation_and_from_a_template() {
    let scratch = Scratch::new("template");
    let mut kernel = kernel(&scratch);
    let engrave = run(&mut kernel, "layers.add", json!({ "name": "Engrave" }))["id"]
        .as_u64()
        .expect("layer");
    let first = rect_on(&mut kernel, engrave, 0.0, 0.0, 10.0, 10.0);
    let picked = run(
        &mut kernel,
        "cam.selectVectors",
        json!({ "filter": { "layers": ["Engrave"], "open": false } }),
    );
    assert_eq!(picked["ids"], json!([first]));
    assert_eq!(picked["closed"], 1);

    let params = json!({ "mode": "fill", "lineInterval": 0.5,
        "selector": { "associate": true, "open": false, "layers": ["Engrave"] } });
    let done = run(
        &mut kernel,
        "cam.compute",
        json!({ "strategy": "laserCutFill", "name": "Fill", "tool": laser(),
                "params": params, "ids": [] }),
    );
    let id = done["id"].as_u64().expect("id");
    let second = rect_on(&mut kernel, engrave, 20.0, 0.0, 30.0, 10.0);
    run(&mut kernel, "cam.recalculate", json!({ "id": id }));
    let request = &run(&mut kernel, "cam.get", json!({ "id": id }))["request"];
    assert_eq!(request["ids"], json!([first, second]));

    let file = scratch.0.join("laser.json");
    run(&mut kernel, "cam.saveTemplate", json!({ "path": file }));
    run(&mut kernel, "doc.new", json!(null));
    let engrave = run(&mut kernel, "layers.add", json!({ "name": "Engrave" }))["id"]
        .as_u64()
        .expect("layer");
    let other = rect_on(&mut kernel, engrave, 50.0, 50.0, 70.0, 60.0);
    run(&mut kernel, "cam.loadTemplate", json!({ "path": file }));
    let list = run(&mut kernel, "cam.list", json!(null));
    let loaded = list[0]["id"].as_u64().expect("loaded");
    let request = &run(&mut kernel, "cam.get", json!({ "id": loaded }))["request"];
    assert_eq!(request["ids"], json!([other]));
    assert_eq!(request["params"]["selector"]["associate"], true);
}
