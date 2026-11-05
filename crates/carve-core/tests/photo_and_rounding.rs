//! Photo Carve and Rounding through the kernel's command interface: the
//! toolpath is calculated from the document, listed, recalculated,
//! previewed and written through a post processor.

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(label: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("carve-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
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

fn tool(kind: ToolGeometry) -> Value {
    let mut tool = Tool {
        name: "Tool".to_string(),
        number: 1,
        tool_type: kind,
        pass_depth: 3.0,
        ..Tool::default()
    };
    tool.set_stepover(2.5);
    serde_json::to_value(tool).expect("tool")
}

/// Values of the `letter` words of a G-code file, in order; words may run
/// together (`G1Z-2.000F600.0`) and comments are skipped.
fn words(text: &str, letter: char) -> Vec<f64> {
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('(')) {
        let mut rest = line;
        while let Some(at) = rest.find(letter) {
            let tail = &rest[at + 1..];
            let end = tail
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .unwrap_or(tail.len());
            if let Ok(value) = tail[..end].parse() {
                out.push(value);
            }
            rest = &tail[end..];
        }
    }
    out
}

#[test]
fn a_photo_is_carved_deeper_where_it_is_darker_and_posts_to_g_code() {
    let scratch = Scratch::new("photo-kernel");
    let mut kernel = kernel(&scratch);
    // Black on the left fading to white on the right.
    let png = scratch.0.join("fade.png");
    let mut image = image::GrayImage::new(60, 30);
    for (x, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Luma([(x * 255 / 59) as u8]);
    }
    image.save(&png).expect("png");
    let bitmap = run(
        &mut kernel,
        "io.importBitmap",
        json!({ "path": png.display().to_string() }),
    )["id"]
        .as_u64()
        .expect("a bitmap");
    let vbit = tool(ToolGeometry::VBit {
        diameter: 12.0,
        included_angle: 60.0,
    });
    let request = json!({
        "strategy": "photoCarve", "name": "Photo", "tool": vbit,
        "params": { "maxDepth": 2.0, "lineSpacing": 120.0 }, "ids": [bitmap],
    });
    let id = run(&mut kernel, "cam.compute", request.clone())["id"]
        .as_u64()
        .expect("a toolpath");
    let list = run(&mut kernel, "cam.list", json!(null));
    assert_eq!(list[0]["kind"], "photoCarve");
    assert_eq!(list[0]["stale"], false);
    assert!(list[0]["cuts"].as_bool().unwrap_or(false));

    // Deeper on the dark side than on the light side.
    let toolpath = kernel.document().toolpath(id).expect("toolpath").clone();
    let bbox = kernel
        .document()
        .entity(bitmap)
        .expect("bitmap")
        .bbox()
        .expect("bbox");
    let middle = (bbox.min.x + bbox.max.x) / 2.0;
    let deepest = |left: bool| {
        toolpath
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.x < middle) == left)
            .map(|m| -m.to.z)
            .fold(0.0, f64::max)
    };
    assert!(
        deepest(true) > deepest(false) + 0.5,
        "{} {}",
        deepest(true),
        deepest(false)
    );
    assert!(deepest(true) <= 2.0 + 1e-6);

    // It recalculates to the same moves, previews and posts.
    run(&mut kernel, "cam.recalculate", json!({ "id": id }));
    assert_eq!(
        kernel.document().toolpath(id).expect("toolpath").moves,
        toolpath.moves
    );
    let preview = run(&mut kernel, "sim.run", json!({}));
    assert!(preview["width"].as_u64().unwrap_or(0) > 0);
    let out = scratch.0.join("photo.nc");
    run(
        &mut kernel,
        "cam.save",
        json!({ "toolpathIds": [id], "post": "grbl_mm", "path": out.display().to_string() }),
    );
    let text = std::fs::read_to_string(&out).expect("g-code");
    let zs = words(&text, 'Z');
    assert!(zs.iter().any(|z| *z < -1.0), "deep grooves in the file");

    // Without a bitmap there is nothing to carve.
    let mut none = request;
    none["ids"] = json!([]);
    assert!(kernel.apply("cam.compute", &none.to_string()).is_err());
}

#[test]
fn rounding_turns_a_square_rotary_blank_and_posts_wrapped() {
    let scratch = Scratch::new("rounding-kernel");
    let mut kernel = kernel(&scratch);
    let mill = tool(ToolGeometry::EndMill { diameter: 6.0 });
    let request = json!({
        "strategy": "rounding", "name": "Round Down", "tool": mill,
        "params": { "blank": "square", "blankSize": 50.0, "finishDiameter": 40.0,
                    "method": "optimizedRaster" },
        "ids": [],
    });
    let flat = kernel
        .apply("cam.compute", &request.to_string())
        .expect_err("a flat job");
    assert!(flat.to_string().contains("rotary"), "{flat}");

    let mut job = run(&mut kernel, "job.get", json!({}));
    job["kind"] = json!("rotary");
    job["rotary"] = json!({ "diameter": 50.0, "axis": "x", "length": 120.0 });
    run(&mut kernel, "job.set", job);
    let id = run(&mut kernel, "cam.compute", request)["id"]
        .as_u64()
        .expect("a toolpath");
    let list = run(&mut kernel, "cam.list", json!(null));
    assert_eq!(list[0]["name"], "Round Down");
    assert!(list[0]["estimatedSeconds"].as_f64().unwrap_or(0.0) > 0.0);
    let toolpath = kernel.document().toolpath(id).expect("toolpath").clone();
    assert!(toolpath.moves.len() > 20);
    // Never below the finished radius above the axis.
    let surface = kernel.document().job().surface_z();
    let lowest = toolpath
        .moves
        .iter()
        .filter(|m| m.kind.is_cutting())
        .map(|m| m.to.z)
        .fold(f64::MAX, f64::min);
    assert!(lowest >= surface - 5.0 - 1e-6, "{lowest} {surface}");

    run(&mut kernel, "cam.recalculate", json!({ "id": id }));
    assert_eq!(
        kernel.document().toolpath(id).expect("toolpath").moves,
        toolpath.moves
    );
    run(&mut kernel, "sim.run", json!({}));
    let out = scratch.0.join("round.nc");
    run(
        &mut kernel,
        "cam.save",
        json!({ "toolpathIds": [id], "post": "grbl_rotary_a_mm",
                "path": out.display().to_string() }),
    );
    let text = std::fs::read_to_string(&out).expect("g-code");
    let angles = words(&text, 'A');
    assert!(angles.len() > 10, "the rotary post writes A words");
    assert!(angles.iter().any(|a| *a > 90.0), "{angles:?}");
}
