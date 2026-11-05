//! Times for large, realistic jobs driven through the kernel commands the
//! UI sends: a full sheet nest of 200 parts, a 600 x 400 relief at the
//! Highest model resolution with 20 components, a V-grooved paragraph, a 3D
//! finish at a 0.1 mm stepover, a 30 piece assembly with Verify Fit, every
//! clipart item, and saving and opening each job. Each prints its times and
//! fails when an operation takes several times what it took when measured
//! (docs/testing/perf.md), so a large slowdown shows. They take seconds in
//! a release build, so they are left out of the gate:
//! `cargo test --release -p carve-core --test large_jobs_speed -- --ignored --nocapture`
//! Each job is its own test, so its peak memory can be read by running it
//! alone (see docs/testing/perf.md).

use std::time::Instant;

use carve_core::cam::commands::CamState;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::doc::{JobSetup, ModelResolution, XyOrigin};
use carve_core::geom::{circle, rect, Path, Point, SubPath, Tolerance};
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Bench {
    kernel: Kernel,
    dir: std::path::PathBuf,
    /// Operations that took longer than their limit.
    slow: Vec<String>,
}

impl Bench {
    fn new(label: &str) -> Bench {
        let dir = std::env::temp_dir().join(format!("carve-large-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let kernel = Kernel::with_cam(CamState::new(
            ToolStore::at(dir.join("tools.json")),
            PostLibrary::at(dir.join("posts")),
        ));
        Bench {
            kernel,
            dir,
            slow: Vec::new(),
        }
    }

    /// Runs one command; answers its result and the reply size in bytes.
    fn call(&mut self, name: &str, args: &Value) -> (Value, usize) {
        let text = if args.is_null() {
            String::new()
        } else {
            args.to_string()
        };
        match self
            .kernel
            .apply(name, &text)
            .unwrap_or_else(|error| panic!("{name} failed: {error}"))
        {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("JSON");
                assert_eq!(value["ok"], true, "{value}");
                (value["result"].clone(), text.len())
            }
            KernelReply::Bytes(bytes) => (Value::Null, bytes.len()),
        }
    }

    fn run(&mut self, name: &str, args: Value) -> Value {
        self.call(name, &args).0
    }

    /// Runs one command, printing its time and reply size; noted as slow
    /// when it takes more than `limit` milliseconds.
    fn time(&mut self, label: &str, name: &str, args: Value, limit: f64) -> Value {
        let start = Instant::now();
        let (value, size) = self.call(name, &args);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{label:<44} {ms:>10.1} ms {:>10.1} kB",
            size as f64 / 1000.0
        );
        self.within(label, ms, limit);
        value
    }

    fn within(&mut self, label: &str, ms: f64, limit: f64) {
        if ms > limit {
            self.slow
                .push(format!("{label}: {ms:.0} ms, over {limit:.0} ms"));
        }
    }

    /// Fails when anything was slow.
    fn check(&self) {
        assert!(self.slow.is_empty(), "too slow:\n{}", self.slow.join("\n"));
    }

    /// What the UI fetches after a change: the document mirror, the 2D
    /// display polylines, the model and the toolpath list and moves.
    fn refresh(&mut self, label: &str) -> f64 {
        let start = Instant::now();
        let mut bytes = 0;
        for (name, args) in [
            ("doc.info", Value::Null),
            ("job.get", Value::Null),
            ("layers.list", Value::Null),
            ("sheets.list", Value::Null),
            ("entities.list", Value::Null),
            ("selection.get", Value::Null),
            ("undo.info", Value::Null),
            ("entities.display", json!({ "tolerance": 0.05 })),
            ("model.list", Value::Null),
            ("model.composite", json!({})),
            ("cam.list", Value::Null),
            ("cam.moves", json!({})),
        ] {
            bytes += self.call(name, &args).1;
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let label = format!("{label}: UI refresh");
        println!(
            "{label:<44} {ms:>10.1} ms {:>10.1} kB",
            bytes as f64 / 1000.0
        );
        self.within(&label, ms, 400.0);
        ms
    }

    fn job(&mut self, width: f64, height: f64, thickness: f64, resolution: ModelResolution) {
        let job = JobSetup {
            width,
            height,
            thickness,
            xy_origin: XyOrigin::BottomLeft,
            resolution,
            ..JobSetup::default()
        };
        self.run("job.set", serde_json::to_value(job).expect("job"));
    }

    fn add(&mut self, path: &Path) -> u64 {
        self.run(
            "entities.create",
            json!({ "entity": { "kind": "path", "path": path } }),
        )["id"]
            .as_u64()
            .expect("an id")
    }

    /// Saves the job, starts a new one and opens it again. The file is
    /// kept in `CARVE_LARGE_JOBS_OUT` when that is set, for the browser
    /// measurements.
    fn save_and_open(&mut self, label: &str) {
        let dir = std::env::var_os("CARVE_LARGE_JOBS_OUT")
            .map_or_else(|| self.dir.clone(), std::path::PathBuf::from);
        let path = dir.join(format!("{label}.carve"));
        let path_text = path.to_string_lossy().to_string();
        self.time(
            &format!("{label}: save"),
            "doc.save",
            json!({ "path": path_text }),
            4000.0,
        );
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        println!("{label}: file {:.1} kB", size as f64 / 1000.0);
        self.run("doc.new", Value::Null);
        self.time(
            &format!("{label}: open"),
            "doc.open",
            json!({ "path": path_text }),
            2000.0,
        );
        self.refresh(&format!("{label} opened"));
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
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

/// An `n` point star round `c`.
fn star(c: Point, n: usize, outer: f64, inner: f64) -> Path {
    let mut sub = SubPath::open(Point::new(c.x + outer, c.y));
    for k in 1..2 * n {
        let a = std::f64::consts::PI * k as f64 / n as f64;
        let r = if k % 2 == 0 { outer } else { inner };
        sub.line_to(Point::new(c.x + r * a.cos(), c.y + r * a.sin()));
    }
    sub.close_with_line(Tolerance::new(0.001).expect("tol"));
    sub.into_path()
}

/// An L shaped part `w` by `h` with arms `t` wide, its corner at `c`.
fn ell(c: Point, w: f64, h: f64, t: f64) -> Path {
    let mut sub = SubPath::open(c);
    for (x, y) in [(w, 0.0), (w, t), (t, t), (t, h), (0.0, h)] {
        sub.line_to(Point::new(c.x + x, c.y + y));
    }
    sub.close_with_line(Tolerance::new(0.001).expect("tol"));
    sub.into_path()
}

/// 200 cabinet and sign parts of mixed shapes on a full 2440 x 1220 sheet,
/// nested with the default settings (90 degree turns).
#[test]
#[ignore = "a timing report, not a check"]
fn nest_200_parts() {
    let mut b = Bench::new("nest");
    b.job(2440.0, 1220.0, 18.0, ModelResolution::Standard);
    let mut ids = Vec::new();
    for k in 0..200 {
        let x = 20.0 + f64::from(k % 20) * 120.0;
        let y = 20.0 + f64::from(k / 20) * 120.0;
        let s = 40.0 + f64::from((k * 37) % 60);
        let path = match k % 5 {
            0 => rect(Point::new(x, y), Point::new(x + s, y + s * 0.6)).expect("rect"),
            1 => circle(Point::new(x + s / 2.0, y + s / 2.0), s / 2.0).expect("circle"),
            2 => star(Point::new(x + s / 2.0, y + s / 2.0), 5, s / 2.0, s / 4.0),
            3 => ell(Point::new(x, y), s, s * 0.8, s * 0.3),
            _ => rect(Point::new(x, y), Point::new(x + s * 0.4, y + s)).expect("rect"),
        };
        ids.push(b.add(&path));
    }
    b.refresh("200 parts drawn");
    let result = b.time(
        "Nest 200 parts, 2440 x 1220",
        "ops.nest",
        json!({ "ids": ids, "params": { "sheetWidth": 2440.0, "sheetHeight": 1220.0 } }),
        3500.0,
    );
    println!("nest reply: {}", summary(&result));
    b.refresh("nested");
    b.save_and_open("nest");
    b.check();
}

/// Twenty overlapping domes, stars and plateaus on a 600 x 400 job at the
/// Highest resolution, then the model view's fetches.
#[test]
#[ignore = "a timing report, not a check"]
fn relief_20_components_highest() {
    let mut b = Bench::new("relief");
    b.job(600.0, 400.0, 25.0, ModelResolution::Highest);
    relief(&mut b);
    b.refresh("relief");
    b.save_and_open("relief");
    b.check();
}

fn relief(b: &mut Bench) -> Vec<u64> {
    let start = Instant::now();
    let mut components = Vec::new();
    for k in 0..20 {
        let c = Point::new(
            60.0 + f64::from(k % 5) * 120.0,
            60.0 + f64::from(k / 5) * 95.0,
        );
        let path = if k % 2 == 0 {
            circle(c, 55.0).expect("circle")
        } else {
            star(c, 7, 60.0, 30.0)
        };
        let id = b.add(&path);
        // Circles alternate round and angular, stars smooth and flat.
        let profile =
            ["round", "smooth", "angular", "flat"][(k as usize / 2) % 2 * 2 + k as usize % 2];
        let reply = b.time(
            &format!("Create Shape {} ({profile})", k + 1),
            "model.createShape",
            json!({ "ids": [id], "params": { "profile": profile, "height": 8.0, "angle": 45.0 } }),
            100.0,
        );
        components.push(reply["id"].as_u64().expect("component"));
    }
    let list = b.run("model.list", Value::Null);
    for row in list["components"].as_array().expect("components") {
        let top = row["maxHeight"].as_f64().unwrap_or(0.0);
        assert!(top < 100.0, "a component {top} mm high: {row}");
    }
    println!(
        "{:<44} {:>10.1} ms",
        "20 components in all",
        start.elapsed().as_secs_f64() * 1000.0
    );
    b.time(
        "Move one component",
        "model.transform",
        json!({ "id": components[3], "params": { "offset": { "x": 5.0, "y": 0.0 },
                "scaleX": 1.0, "scaleY": 1.0, "rotateDeg": 0.0 } }),
        100.0,
    );
    components
}

/// A paragraph of body text V-grooved with a 90 degree bit.
#[test]
#[ignore = "a timing report, not a check"]
fn vgroove_paragraph() {
    let mut b = Bench::new("vgroove");
    b.job(600.0, 400.0, 18.0, ModelResolution::Standard);
    let words = "The quick brown fox jumps over the lazy dog while carving signs \
        for the workshop door, the garden gate and the old barn by the river. ";
    let text = words.repeat(3);
    let reply = b.time(
        &format!("Text, {} characters", text.len()),
        "text.create",
        json!({
            "spec": { "text": text, "fontFamily": "Helvetica", "sizeMm": 12.0,
                      "wrapWidth": 540.0 },
            "x": 30.0, "y": 360.0,
        }),
        100.0,
    );
    let id = reply["id"].as_u64().expect("a text id");
    let vbit = tool(
        ToolGeometry::VBit {
            diameter: 12.7,
            included_angle: 90.0,
        },
        0.5,
    );
    let reply = b.time(
        "V-Groove the paragraph",
        "cam.compute",
        json!({ "strategy": "vgroove", "tool": vbit, "params": {}, "ids": [id] }),
        1200.0,
    );
    println!("moves: {}", moves_of(&mut b, &reply));
    b.refresh("vgroove");
    b.time(
        "Preview (Standard)",
        "sim.run",
        json!({ "quality": "standard" }),
        2000.0,
    );
    b.save_and_open("vgroove");
    b.check();
}

/// The relief of `relief_20_components_highest` finished with a 3 mm ball
/// nose at a 0.1 mm stepover.
#[test]
#[ignore = "a timing report, not a check"]
fn finish_0_1_stepover() {
    let mut b = Bench::new("finish");
    b.job(600.0, 400.0, 25.0, ModelResolution::Highest);
    relief(&mut b);
    let ball = tool(ToolGeometry::BallNose { diameter: 3.0 }, 0.1);
    let reply = b.time(
        "3D Finish raster, 0.1 mm stepover",
        "cam.compute",
        json!({ "strategy": "finish3d", "tool": ball, "params": { "stepover": 0.1 }, "ids": [] }),
        6000.0,
    );
    println!("moves: {}", moves_of(&mut b, &reply));
    b.refresh("finish");
    b.save_and_open("finish");
    b.check();
}

/// Thirty boards cut out of one sheet, each a piece of the assembly, then
/// the Assembly View's list and Verify Fit.
#[test]
#[ignore = "a timing report, not a check"]
fn assembly_30_pieces() {
    let mut b = Bench::new("assembly");
    b.job(2440.0, 1220.0, 18.0, ModelResolution::Standard);
    let mut ids = Vec::new();
    for k in 0..30 {
        let x = 30.0 + f64::from(k % 10) * 240.0;
        let y = 30.0 + f64::from(k / 10) * 390.0;
        ids.push(b.add(&rect(Point::new(x, y), Point::new(x + 200.0, y + 350.0)).expect("rect")));
    }
    let end_mill = tool(ToolGeometry::EndMill { diameter: 6.35 }, 2.5);
    b.time(
        "Profile 30 boards",
        "cam.compute",
        json!({ "strategy": "profile", "tool": end_mill,
                "params": { "cutDepth": 18.5, "side": "outside" }, "ids": ids }),
        500.0,
    );
    let list = b.time("Assembly pieces", "assembly.pieces", json!({}), 300.0);
    let count = list["pieces"].as_array().map_or(0, Vec::len);
    println!("pieces: {count}");
    b.time("Assembly pieces again", "assembly.pieces", json!({}), 100.0);
    b.time(
        "Verify Fit",
        "assembly.verifyFit",
        json!({ "target": 0.2, "tolerance": 0.1 }),
        1500.0,
    );
    b.time(
        "Verify Fit again",
        "assembly.verifyFit",
        json!({ "target": 0.2, "tolerance": 0.1 }),
        1500.0,
    );
    b.refresh("assembly");
    b.save_and_open("assembly");
    b.check();
}

/// Every item of the shipped clipart library inserted into a new 600 x 400
/// job, listing the slowest and the time of each item's thumbnail.
#[test]
#[ignore = "a timing report, not a check"]
fn clipart_items() {
    use carve_core::clipart::{dispatch_in, ClipartDirs};
    use carve_core::doc::Document;
    let mut b = Bench::new("clipart");
    let dirs = ClipartDirs {
        user: b.dir.join("user"),
        library: b.dir.join(carve_core::clipart::library::dir_name()),
    };
    let json_of = |reply: KernelReply| match reply {
        KernelReply::Json(text) => {
            serde_json::from_str::<Value>(&text).expect("JSON")["result"].clone()
        }
        KernelReply::Bytes(_) => Value::Null,
    };
    let start = Instant::now();
    let mut doc = Document::new();
    let scan = json_of(dispatch_in(&dirs, &mut doc, "clipart.scan", "{}").expect("scan"));
    println!(
        "{:<44} {:>10.1} ms",
        "Clipart scan",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let items = scan["items"].as_array().cloned().unwrap_or_default();
    let mut times = Vec::new();
    let job = JobSetup {
        width: 600.0,
        height: 400.0,
        thickness: 25.0,
        ..JobSetup::default()
    };
    for item in &items {
        let path = item["path"].as_str().expect("path").to_string();
        let start = Instant::now();
        dispatch_in(
            &dirs,
            &mut doc,
            "clipart.thumbnail",
            &json!({ "path": path }).to_string(),
        )
        .expect("thumbnail");
        let thumb = start.elapsed().as_secs_f64() * 1000.0;
        let mut doc = Document::new();
        doc.set_job(job).expect("job");
        let start = Instant::now();
        dispatch_in(
            &dirs,
            &mut doc,
            "clipart.insert",
            &json!({ "path": path, "x": 300.0, "y": 200.0 }).to_string(),
        )
        .expect("insert");
        let insert = start.elapsed().as_secs_f64() * 1000.0;
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        b.within(&format!("thumbnail of {path}"), thumb, 1000.0);
        b.within(&format!("insert of {path}"), insert, 200.0);
        times.push((insert, thumb, size, path));
    }
    let total: f64 = times.iter().map(|t| t.0).sum();
    let thumbs: f64 = times.iter().map(|t| t.1).sum();
    println!(
        "{} items: insert {total:.0} ms in all, thumbnails {thumbs:.0} ms in all",
        times.len()
    );
    times.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (insert, thumb, size, path) in times.iter().take(8) {
        let name = std::path::Path::new(path)
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());
        println!(
            "  insert {insert:>8.1} ms  thumbnail {thumb:>7.1} ms  {:>8.1} kB  {name}",
            *size as f64 / 1000.0
        );
    }
    times.sort_by_key(|a| std::cmp::Reverse(a.2));
    println!("largest files:");
    for (insert, thumb, size, path) in times.iter().take(5) {
        let name = std::path::Path::new(path)
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());
        println!(
            "  insert {insert:>8.1} ms  thumbnail {thumb:>7.1} ms  {:>8.1} kB  {name}",
            *size as f64 / 1000.0
        );
    }
    b.check();
}

fn moves_of(b: &mut Bench, reply: &Value) -> u64 {
    let id = reply["id"].as_u64().expect("an id");
    let list = b.run("cam.list", Value::Null);
    list.as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == json!(id)))
        .map_or(0, |r| r["moves"].as_u64().unwrap_or(0))
}

fn summary(value: &Value) -> String {
    let text = value.to_string();
    text.chars().take(200).collect()
}
