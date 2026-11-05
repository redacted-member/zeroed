//! Every shipped post, run through an independent G-code checker.
//!
//! Jobs are built through the kernel's commands as the app builds them,
//! previewed with the app's own stock and posted with `cam.save` through
//! every shipped post. The checker in `tools/gcode-check` then reads each
//! file as a controller would and cuts it into a heightfield of its own,
//! with none of the CAM, post or preview code. Each program must:
//! - cut the same material the preview shows (within a cell and 0.05 mm),
//! - go no lower than the design asks,
//! - rapid across the job only at or above the safe Z,
//! - load each toolpath's tool before it cuts, with the spindle running
//!   at the toolpath's speed and the feeds the toolpath asks for,
//! - set the units the post declares, and
//! - end with the tool clear and the spindle off.
//!
//! Run `cargo test -p carve-core --test gcode_check -- --nocapture` to see
//! the table of every job and post; with `GSIM_DEBUG=1` a heightfield
//! difference also prints the toolpath and program moves near its worst cell.

use std::collections::BTreeMap;
use std::path::PathBuf;

use carve_core::cam::commands::CamState;
use carve_core::cam::playback::{Budget, Playback};
use carve_core::cam::sim::Stock;
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::tooldb::ToolStore;
use carve_core::cam::toolpath::Toolpath;
use carve_core::doc::Side;
use carve_core::geom::{circle, rect, Point};
use carve_core::post::{parse_post, PostLibrary, PostUnits, SHIPPED_POSTS};
use carve_core::{Kernel, KernelReply};
use gcode_check::{Cutter, Grid, Kind, Report, Setup, Severity, ToolLength, Units};
use serde_json::{json, Value};

/// Height a cell may differ by, beyond a one cell shift.
const HEIGHT_TOLERANCE: f64 = 0.05;

/// Size of a comparison cell, millimetres.
const CELL: f64 = 0.5;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "carve-gcode-check-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
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
            assert_eq!(value["ok"], true, "{name}: {value}");
            value["result"].clone()
        }
        KernelReply::Bytes(_) => panic!("{name} answered with bytes"),
    }
}

fn tool(name: &str, number: u32, geometry: ToolGeometry) -> Value {
    serde_json::to_value(Tool {
        name: name.to_string(),
        number,
        tool_type: geometry,
        feed_rate: 1500.0,
        plunge_rate: 500.0,
        spindle_speed: 16000.0 + f64::from(number) * 500.0,
        pass_depth: 3.0,
        stepover: 2.0,
        ..Tool::default()
    })
    .expect("tool serialises")
}

fn end_mill(number: u32, diameter: f64) -> Value {
    tool(
        &format!("End Mill {diameter} mm"),
        number,
        ToolGeometry::EndMill { diameter },
    )
}

fn vector(kernel: &mut Kernel, path: carve_core::geom::Path) -> u64 {
    run(
        kernel,
        "entities.create",
        json!({ "entity": { "kind": "path", "path": path } }),
    )["id"]
        .as_u64()
        .expect("an id")
}

fn compute(kernel: &mut Kernel, request: Value) {
    run(kernel, "cam.compute", request);
}

/// A job as the checker needs it: the kernel holding the design and the
/// depth each tool is meant to reach, from the design, not the toolpaths.
struct Job {
    name: &'static str,
    kernel: Kernel,
    /// Lowest Z each tool number should reach; `None` to take it from the
    /// calculated toolpath (V carving and 3D, whose depth follows the
    /// geometry).
    depths: BTreeMap<u32, Option<f64>>,
    _scratch: Scratch,
}

fn new_job(name: &'static str) -> (Kernel, Scratch) {
    let scratch = Scratch::new(name);
    let mut kernel = kernel(&scratch);
    let mut job = run(&mut kernel, "job.get", json!({}));
    job["width"] = json!(160.0);
    job["height"] = json!(120.0);
    job["thickness"] = json!(18.0);
    run(&mut kernel, "job.set", job);
    (kernel, scratch)
}

/// A rectangular pocket 6 mm deep in 2 mm passes, ramped in.
fn pocket_job() -> Job {
    let (mut kernel, scratch) = new_job("pocket");
    let id = vector(
        &mut kernel,
        rect(Point::new(30.0, 30.0), Point::new(90.0, 80.0)).expect("rect"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": end_mill(1, 6.0), "ids": [id],
                "params": { "cutDepth": 6.0, "passDepth": 2.0, "stepover": 2.4,
                            "ramp": { "enabled": true, "distance": 15.0, "kind": "zigZag" } } }),
    );
    let circle_id = vector(
        &mut kernel,
        circle(Point::new(125.0, 60.0), 15.0).expect("circle"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": end_mill(1, 6.0), "ids": [circle_id],
                "params": { "cutDepth": 4.0, "passDepth": 2.0, "stepover": 2.4,
                            "strategy": "raster" } }),
    );
    Job {
        name: "pocket",
        kernel,
        depths: BTreeMap::from([(1, Some(-6.0))]),
        _scratch: scratch,
    }
}

/// An outside profile through the board, with tabs and a smooth ramp.
fn profile_job() -> Job {
    let (mut kernel, scratch) = new_job("profile");
    let id = vector(
        &mut kernel,
        rect(Point::new(40.0, 30.0), Point::new(110.0, 85.0)).expect("rect"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "profile", "tool": end_mill(1, 6.0), "ids": [id],
                "params": { "cutDepth": 18.0, "passDepth": 6.0, "side": "outside",
                            "tabs": { "enabled": true, "count": 4, "length": 8.0,
                                      "thickness": 3.0 },
                            "ramp": { "enabled": true, "distance": 25.0, "kind": "smooth" } } }),
    );
    Job {
        name: "profile",
        kernel,
        depths: BTreeMap::from([(1, Some(-18.0))]),
        _scratch: scratch,
    }
}

/// Holes 12 mm deep in 3 mm pecks, with a dwell at the bottom.
fn drill_job() -> Job {
    let (mut kernel, scratch) = new_job("drill");
    let mut ids = Vec::new();
    for (x, y) in [(30.0, 30.0), (80.0, 40.0), (120.0, 90.0)] {
        ids.push(vector(
            &mut kernel,
            circle(Point::new(x, y), 3.0).expect("circle"),
        ));
    }
    compute(
        &mut kernel,
        json!({ "strategy": "drill",
                "tool": tool("Drill 5 mm", 2, ToolGeometry::Drill { diameter: 5.0, point_angle: 118.0 }),
                "ids": ids,
                "params": { "cutDepth": 12.0, "peckDepth": 3.0, "retractGap": 1.0,
                            "dwell": 0.5 } }),
    );
    Job {
        name: "drill",
        kernel,
        depths: BTreeMap::from([(2, Some(-12.0))]),
        _scratch: scratch,
    }
}

/// A star carved with a 60 degree V bit.
fn vgroove_job() -> Job {
    let (mut kernel, scratch) = new_job("vgroove");
    let star = carve_core::geom::star(
        Point::new(70.0, 60.0),
        5,
        25.0,
        carve_core::geom::StarInner::Percent(45.0),
        0.0,
    )
    .expect("star");
    let id = vector(&mut kernel, star);
    compute(
        &mut kernel,
        json!({ "strategy": "vgroove",
                "tool": tool("V-Bit 60 deg 12 mm", 3,
                             ToolGeometry::VBit { diameter: 12.0, included_angle: 60.0 }),
                "ids": [id], "params": { "startDepth": 0.0 } }),
    );
    Job {
        name: "vgroove",
        kernel,
        depths: BTreeMap::from([(3, None)]),
        _scratch: scratch,
    }
}

/// A dome in the model, roughed with an end mill and finished with a ball
/// nose.
fn finish_job() -> Job {
    let (mut kernel, scratch) = new_job("finish3d");
    let id = vector(
        &mut kernel,
        circle(Point::new(80.0, 60.0), 35.0).expect("circle"),
    );
    run(
        &mut kernel,
        "model.createShape",
        json!({ "ids": [id], "params": { "profile": "round", "height": 8.0 } }),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "rough3d", "tool": end_mill(1, 6.0), "ids": [],
                "params": { "stepover": 2.5 } }),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "finish3d",
                "tool": tool("Ball Nose 6 mm", 4, ToolGeometry::BallNose { diameter: 6.0 }),
                "ids": [], "params": { "stepover": 1.0 } }),
    );
    Job {
        name: "finish3d",
        kernel,
        depths: BTreeMap::from([(1, None), (4, None)]),
        _scratch: scratch,
    }
}

/// A V-Groove inlay: the pocket and the plug of a circle, with a flat depth
/// cleared by an end mill.
fn inlay_job() -> Job {
    let (mut kernel, scratch) = new_job("inlay");
    let id = vector(
        &mut kernel,
        circle(Point::new(50.0, 60.0), 18.0).expect("circle"),
    );
    for part in ["female", "male"] {
        compute(
            &mut kernel,
            json!({ "strategy": "inlay",
                    "tool": tool("V-Bit 60 deg 12 mm", 3,
                                 ToolGeometry::VBit { diameter: 12.0, included_angle: 60.0 }),
                    "ids": [id],
                    "params": { "pocketDepth": 4.0, "glueGap": 0.5, "part": part,
                                "clearanceTools": [ { "tool": end_mill(6, 3.0) } ] } }),
        );
    }
    Job {
        name: "inlay",
        kernel,
        depths: BTreeMap::from([(3, None), (6, None)]),
        _scratch: scratch,
    }
}

/// An internal M12 thread in a hole, cut with a single point thread mill.
fn thread_job() -> Job {
    let (mut kernel, scratch) = new_job("thread");
    let id = vector(
        &mut kernel,
        circle(Point::new(60.0, 60.0), 6.0).expect("circle"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "threadMilling",
                "tool": tool("Thread Mill 8 mm", 5, ToolGeometry::ThreadMill {
                    diameter: 8.0, pitch: 1.75, included_angle: 60.0,
                    tooth_offset: 0.0, teeth: 1 }),
                "ids": [id],
                "params": { "startDepth": 0.0, "maxDepth": 10.0, "pitch": 1.75,
                            "diameter": 12.0, "kind": "internal" } }),
    );
    Job {
        name: "thread",
        kernel,
        depths: BTreeMap::from([(5, Some(-10.0))]),
        _scratch: scratch,
    }
}

/// The saved projects under `docs/testing/projects`, each opened as the
/// app opens it, with every tool's depth taken from its toolpaths.
///
/// Each side of a two sided job and each sheet of a nested job is its own
/// setup on the machine, posted to its own file: such a project gives one
/// job per side and sheet, named `<file>-top`, `<file>-sheet2` and so on,
/// seen from its side with only its toolpaths shown.
fn saved_projects() -> Vec<(String, Job)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/testing/projects");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "carve"));
    files.sort();
    let open = |path: &PathBuf| {
        let scratch = Scratch::new("project");
        let mut kernel = kernel(&scratch);
        run(&mut kernel, "doc.open", json!({ "path": path }));
        (scratch, kernel)
    };
    let mut jobs = Vec::new();
    for path in files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_string();
        // Every side's toolpaths: the document holds the side shown.
        let (_scratch, mut first) = open(&path);
        let mut groups: Vec<(Side, u64)> = Vec::new();
        for t in run(&mut first, "cam.list", json!({}))
            .as_array()
            .expect("toolpaths")
        {
            if t["visible"] == false {
                continue;
            }
            let side = if t["side"] == "bottom" {
                Side::Bottom
            } else {
                Side::Top
            };
            let group = (side, t["sheet"].as_u64().unwrap_or(1));
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        let sides = groups.iter().any(|g| g.0 != groups[0].0);
        // Each side of a two sided job keeps its own sheet: sheets count
        // only within a side.
        let sheets = groups
            .iter()
            .any(|g| groups.iter().any(|h| h.0 == g.0 && h.1 != g.1));
        for (side, sheet) in groups {
            let (scratch, mut kernel) = open(&path);
            let mut label = stem.clone();
            if sheets {
                label.push_str(&format!("-sheet{sheet}"));
            }
            if sides {
                label.push_str(if side == Side::Top { "-top" } else { "-bottom" });
                run(&mut kernel, "job.setSide", json!({ "side": side }));
            }
            let others: Vec<u64> = kernel
                .document()
                .toolpaths()
                .iter()
                .filter(|t| t.visible && (t.side, t.sheet) != (side, sheet))
                .map(|t| t.id)
                .collect();
            if !others.is_empty() {
                run(
                    &mut kernel,
                    "cam.setVisible",
                    json!({ "ids": others, "visible": false }),
                );
            }
            let depths = kernel
                .document()
                .toolpaths()
                .iter()
                .filter(|t| t.visible)
                .map(|t| (t.tool_number, None))
                .collect();
            let job = Job {
                name: "project",
                kernel,
                depths,
                _scratch: scratch,
            };
            jobs.push((label, job));
        }
    }
    jobs
}

fn cutter_of(tool: &Tool) -> Option<Cutter> {
    Some(match &tool.tool_type {
        ToolGeometry::EndMill { diameter } | ToolGeometry::ThreadMill { diameter, .. } => {
            Cutter::Flat {
                diameter: *diameter,
            }
        }
        ToolGeometry::Drill {
            diameter,
            point_angle,
        } => Cutter::Drill {
            diameter: *diameter,
            point_angle: *point_angle,
        },
        ToolGeometry::BallNose { diameter } => Cutter::Ball {
            diameter: *diameter,
        },
        ToolGeometry::VBit {
            diameter,
            included_angle,
        }
        | ToolGeometry::DiamondDrag {
            diameter,
            angle: included_angle,
        } => Cutter::Vee {
            diameter: *diameter,
            included_angle: *included_angle,
        },
        ToolGeometry::EngravingTool {
            diameter,
            included_angle,
            flat_diameter,
        } => Cutter::Engraving {
            diameter: *diameter,
            included_angle: *included_angle,
            flat_diameter: *flat_diameter,
        },
        ToolGeometry::RadiusedEndMill {
            diameter,
            corner_radius,
        } => Cutter::Bull {
            diameter: *diameter,
            corner_radius: *corner_radius,
        },
        ToolGeometry::TaperedBallNose {
            diameter,
            tip_radius,
            taper_angle,
        } => Cutter::TaperedBall {
            diameter: *diameter,
            tip_radius: *tip_radius,
            taper_angle: *taper_angle,
        },
        ToolGeometry::FormTool { .. } | ToolGeometry::LaserTool { .. } => return None,
    })
}

/// The app's preview of the job, on a grid coarse enough for a debug test.
fn preview(kernel: &Kernel) -> Stock {
    let doc = kernel.document();
    let job = doc.job();
    let mut stock = Stock::with_cells(job, job.width * job.height / (CELL * CELL));
    let mut playback = Playback::new(doc, |t| t.visible);
    playback
        .step(doc, &mut stock, Budget::All)
        .expect("the preview runs");
    stock
}

fn grid_of(stock: &Stock) -> Grid {
    Grid {
        width: stock.width,
        height: stock.height,
        cell: stock.cell,
        origin_x: stock.origin_x,
        origin_y: stock.origin_y,
        top: stock.top,
        bottom: stock.bottom,
    }
}

fn post_units(id: &str) -> Units {
    let (_, source) = SHIPPED_POSTS
        .iter()
        .find(|(name, _)| *name == id)
        .expect("shipped");
    match parse_post(id, source).expect("parses").units {
        PostUnits::Mm => Units::Mm,
        PostUnits::Inch => Units::Inch,
    }
}

/// How the machine a post is written for measures each new tool: the
/// LinuxCNC and UCCNC posts leave it to an M6 that probes, their manual
/// variants to a touch off at the M0 pause. Other posts are not checked for
/// it (a tool table or a changer sets the length there).
fn tool_length_of(id: &str) -> ToolLength {
    match id {
        "linuxcnc_mm"
        | "linuxcnc_inch"
        | "uccnc_mm"
        | "linuxcnc_rotary_mm"
        | "linuxcnc_rotary_inch"
        | "uccnc_rotary_mm" => ToolLength::ProbedByM6,
        "linuxcnc_manual_mm"
        | "linuxcnc_manual_inch"
        | "uccnc_manual_mm"
        | "linuxcnc_manual_rotary_mm"
        | "linuxcnc_manual_rotary_inch"
        | "uccnc_manual_rotary_mm" => ToolLength::TouchOffAtPause,
        _ => ToolLength::Unchecked,
    }
}

/// The LinuxCNC and UCCNC posts set every modal state their moves rely on
/// in the header (both controls keep them from the program run before).
fn sets_its_modes(id: &str) -> bool {
    id.starts_with("linuxcnc") || id.starts_with("uccnc")
}

/// Posts the whole job through `post`, answering the program's text.
fn post(job: &mut Job, post: &str, ids: &[u64], rotary: Option<Value>) -> Result<String, String> {
    let path = job._scratch.0.join(format!("{}-{post}", job.name));
    let reply = job
        .kernel
        .apply(
            "cam.save",
            &json!({ "toolpathIds": ids, "post": post, "path": path, "rotary": rotary })
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
    let KernelReply::Json(text) = reply else {
        return Err("bytes".into());
    };
    let value: Value = serde_json::from_str(&text).expect("JSON");
    if value["ok"] != true {
        return Err(value.to_string());
    }
    let paths = value["result"]["paths"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if paths.len() != 1 {
        return Err(format!("{} files written", paths.len()));
    }
    std::fs::read_to_string(paths[0].as_str().expect("a path")).map_err(|e| e.to_string())
}

/// One job through one post: what the checker found that it should not.
struct Outcome {
    problems: Vec<String>,
    notes: Vec<String>,
    line: String,
}

/// A wrapped rotary save: the blank the design unwraps from.
#[derive(Clone, Copy)]
struct Wrap {
    axis: char,
    across: char,
    diameter: f64,
}

fn check_one(job: &mut Job, post_id: &str, stock: &Stock, wrap: Option<Wrap>) -> Outcome {
    let toolpaths: Vec<Toolpath> = job
        .kernel
        .document()
        .toolpaths()
        .into_iter()
        .filter(|t| t.visible)
        .cloned()
        .collect();
    let ids: Vec<u64> = toolpaths.iter().map(|t| t.id).collect();
    let rotary = wrap.map(
        |w| json!({ "diameter": w.diameter, "axis": if w.across == 'Y' { "x" } else { "y" } }),
    );
    let text = match post(job, post_id, &ids, rotary) {
        Ok(text) => text,
        Err(e) => {
            return Outcome {
                problems: Vec::new(),
                notes: vec![format!("not posted: {e}")],
                line: format!("{:<9} {post_id:<20} refused", job.name),
            }
        }
    };
    check_text(job, post_id, stock, wrap, &text, &toolpaths)
}

/// Checks one posted program against the job it was posted from.
fn check_text(
    job: &Job,
    post_id: &str,
    stock: &Stock,
    wrap: Option<Wrap>,
    text: &str,
    toolpaths: &[Toolpath],
) -> Outcome {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let units = post_units(post_id);
    let safe_z = toolpaths
        .iter()
        .map(|t| t.safe_z)
        .fold(f64::INFINITY, f64::min);
    // A laser burns at the surface with the beam for a spindle: no safe Z,
    // no tool changes, no depths, nothing cut into the material.
    let laser = post_id.starts_with("laser");
    let mut setup = Setup::new(units, if laser { 0.0 } else { safe_z });
    setup.tool_length = tool_length_of(post_id);
    setup.preamble = sets_its_modes(post_id);
    setup.laser = laser;
    for t in toolpaths {
        if let Some(c) = cutter_of(&t.tool) {
            setup.tools.insert(t.tool_number, c);
        }
    }
    setup.grid = Some(grid_of(stock));
    if post_id.starts_with("snapmaker") {
        setup.fitted_tool = toolpaths.first().map(|t| t.tool_number);
    }
    setup.rotary = wrap.map(|w| gcode_check::Rotary {
        axis: w.axis,
        across: w.across,
        diameter: w.diameter,
        linear: false,
        reversed: false,
    });
    let report = gcode_check::check(text, &setup);

    for issue in report.at_least(Severity::Warning) {
        let text = format!(
            "line {} {:?} {:?}: {}",
            issue.line, issue.severity, issue.kind, issue.text
        );
        // A spindle turning through a tool change is never accepted; a move
        // across before Z lifts after a change is an error of its own.
        if issue.severity == Severity::Error || issue.kind == Kind::SpindleOnAtChange {
            problems.push(text);
        } else {
            notes.push(text);
        }
    }
    if report.units != Some(units) && report.dialect != gcode_check::Dialect::ShopBot {
        problems.push(format!("units {:?}, the post says {units:?}", report.units));
    }

    if laser {
        laser_problems(&report, toolpaths, &mut problems);
    }

    // Each toolpath's tool is loaded before it, in toolpath order.
    let mut wanted: Vec<u32> = Vec::new();
    for t in toolpaths {
        if wanted.last() != Some(&t.tool_number) {
            wanted.push(t.tool_number);
        }
    }
    if !laser && report.tool_changes != wanted {
        problems.push(format!(
            "tools loaded {:?}, the toolpaths use {wanted:?}",
            report.tool_changes
        ));
    }

    // Depths: the lowest point per tool is what the design asks for.
    let lowest = report.lowest_by_tool();
    let resolution = if units == Units::Inch { 0.003 } else { 0.0011 };
    let depths = if laser {
        BTreeMap::new()
    } else {
        job.depths.clone()
    };
    for (number, depth) in &depths {
        let from_toolpaths = toolpaths
            .iter()
            .filter(|t| t.tool_number == *number)
            .filter_map(|t| t.bounds.map(|b| b.min.z))
            .fold(f64::INFINITY, f64::min);
        let want = depth.unwrap_or(from_toolpaths);
        match lowest.get(&Some(*number)) {
            Some(got) if (got - want).abs() <= resolution => {}
            Some(got) => problems.push(format!(
                "tool {number} goes down to Z {got:.4}, the design asks for {want:.4}"
            )),
            None => problems.push(format!("tool {number} never cuts")),
        }
        if depth.is_some() && (from_toolpaths - want).abs() > 1e-6 {
            problems.push(format!(
                "the toolpaths of tool {number} go down to Z {from_toolpaths:.4}, the design \
                 asks for {want:.4}"
            ));
        }
    }

    // Feeds and speeds: each cutting move runs at one of its toolpath's
    // rates, the spindle at the toolpath's speed.
    check_rates(&report, toolpaths, post_id, &mut problems);

    // Dwells: as long as the toolpaths ask (a Fanuc P counts milliseconds).
    let dwells: Vec<f64> = toolpaths
        .iter()
        .flat_map(|t| t.moves.iter())
        .filter_map(|m| match m.kind {
            carve_core::cam::toolpath::MoveKind::Dwell { seconds } => Some(seconds),
            _ => None,
        })
        .collect();
    let got: Vec<f64> = report
        .dwells
        .iter()
        .map(|d| {
            if post_id.starts_with("fanuc") {
                d / 1000.0
            } else {
                *d
            }
        })
        .collect();
    let same =
        got.len() == dwells.len() && got.iter().zip(&dwells).all(|(a, b)| (a - b).abs() < 0.011);
    if !same {
        problems.push(format!("dwells {got:?}, the toolpaths ask for {dwells:?}"));
    }

    // The material: the checker's cut against the app's preview.
    let field = report.field.as_ref().expect("a field");
    let ours: Vec<f64> = stock.z.iter().map(|z| f64::from(*z)).collect();
    let slope = setup
        .tools
        .values()
        .map(|c| c.steepest())
        .fold(0.0, f64::max);
    let cmp = gcode_check::compare(&field.grid, &field.z, &ours, HEIGHT_TOLERANCE, slope);
    if cmp.over > 0 {
        let (x, y, a, b, e) = cmp.worst.expect("a worst cell");
        if std::env::var("GSIM_DEBUG").is_ok() {
            for t in toolpaths {
                for (from, m) in t.iter_moves() {
                    let near =
                        |p: carve_core::cam::toolpath::Point3| (p.x - x).hypot(p.y - y) < 4.0;
                    if near(from) || near(m.to) {
                        eprintln!(
                            "DBG tp {} {} {:?} {:?} -> {:?} r {}",
                            t.name,
                            t.tool_number,
                            m.kind,
                            from,
                            m.to,
                            t.tool.radius()
                        );
                    }
                }
            }
            for m in &report.motions {
                let near = |p: [f64; 3]| (p[0] - x).hypot(p[1] - y) < 4.0;
                if near(m.from) || near(m.to) {
                    eprintln!(
                        "DBG gc line {} rapid {} {:?} -> {:?} tool {:?}",
                        m.line, m.rapid, m.from, m.to, m.tool
                    );
                }
            }
        }
        problems.push(format!(
            "{} cells differ from the preview beyond a cell and {HEIGHT_TOLERANCE} mm; worst at \
             X {x:.2} Y {y:.2}: G-code {a:.3}, preview {b:.3} ({e:.3} beyond)",
            cmp.over
        ));
    }
    let line = format!(
        "{:<9} {post_id:<20} {:>6} moves  max {:.3}  mean {:.4}  over {:>4}  errors {:>2}  \
         warnings {:>2}",
        job.name,
        report.motions.len(),
        cmp.max_abs,
        cmp.mean_abs,
        cmp.over,
        problems.len(),
        notes.len()
    );
    Outcome {
        problems,
        notes,
        line,
    }
}

/// A laser program burns what its toolpaths burn: every line the beam
/// crosses at a power above zero, as long in all as the toolpaths' own,
/// each at a power one of them sets (the post scales percent to Grbl's
/// 0 to 1000), and the beam never on across a rapid it could mark.
fn laser_problems(report: &Report, toolpaths: &[Toolpath], problems: &mut Vec<String>) {
    let mut wanted = 0.0;
    let mut powers: Vec<f64> = Vec::new();
    for t in toolpaths {
        let ToolGeometry::LaserTool { power, .. } = t.tool.tool_type else {
            problems.push(format!("{} is not a laser toolpath", t.name));
            continue;
        };
        let mut now = power;
        for (from, one) in t.iter_moves() {
            if let Some(set) = one.power {
                now = set;
            }
            if one.kind.is_cutting() && now > 0.0 {
                wanted += one.length(from);
                if !powers.iter().any(|p| (p - now * 10.0).abs() < 0.5) {
                    powers.push(now * 10.0);
                }
            }
        }
    }
    let mut burned = 0.0;
    for m in report.motions.iter().filter(|m| m.spindle && m.speed > 0.0) {
        let across = (m.to[0] - m.from[0]).hypot(m.to[1] - m.from[1]);
        if m.rapid {
            continue;
        }
        burned += across;
        if !powers.iter().any(|p| (p - m.speed).abs() < 0.51) {
            problems.push(format!(
                "line {} burns at S{}, the toolpaths set {powers:?}",
                m.line, m.speed
            ));
            return;
        }
    }
    if (burned - wanted).abs() > 0.005 * wanted.max(1.0) {
        problems.push(format!(
            "the beam burns {burned:.2} mm, the toolpaths {wanted:.2} mm"
        ));
    }
}

fn check_rates(report: &Report, toolpaths: &[Toolpath], post_id: &str, problems: &mut Vec<String>) {
    let mut feeds: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    let mut speeds: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for t in toolpaths {
        let f = feeds.entry(t.tool_number).or_default();
        f.extend([t.feed, t.plunge]);
        f.extend(t.rates.iter().flat_map(|r| [r.feed, r.plunge]));
        let s = speeds.entry(t.tool_number).or_default();
        s.push(t.spindle);
        s.extend(t.rates.iter().map(|r| r.spindle));
    }
    let mut bad_feed = 0;
    let mut first = None;
    for m in report.motions.iter().filter(|m| !m.rapid) {
        let Some(tool) = m.tool else { continue };
        let allowed = feeds.get(&tool).cloned().unwrap_or_default();
        let fits = allowed
            .iter()
            .any(|f| (m.feed - f).abs() <= (0.01 * f).max(8.0));
        if !fits {
            bad_feed += 1;
            first.get_or_insert((m.line, m.feed, allowed.clone()));
        }
        let speed_ok = speeds
            .get(&tool)
            .is_some_and(|s| s.iter().any(|v| (m.speed - v).abs() < 1.0));
        // The 50W module takes M3 P in percent: snapmaker_post.rs checks that word.
        let percent = post_id == "snapmaker_cnc_50w_mm";
        if !speed_ok && !percent && m.from[2].min(m.to[2]) < 0.0 {
            problems.push(format!(
                "{post_id}: line {} cuts at spindle speed {}, the toolpath asks for {:?}",
                m.line,
                m.speed,
                speeds.get(&tool)
            ));
            break;
        }
    }
    if let Some((line, feed, allowed)) = first {
        problems.push(format!(
            "{bad_feed} cutting moves at a feed the toolpaths do not use; first at line {line}: \
             {feed:.1} mm/min, allowed {allowed:?}"
        ));
    }
}

fn milling_posts() -> Vec<&'static str> {
    SHIPPED_POSTS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !id.starts_with("laser") && !id.starts_with("snapmaker"))
        .collect()
}

/// The posts a job's programs go through: a laser job through the laser
/// post, a rotary job through the rotary posts wrapped round its blank
/// (A about X, B about Y), any other through every milling post.
fn posts_for(job: &Job) -> Vec<(&'static str, Option<Wrap>)> {
    let doc = job.kernel.document();
    let shown: Vec<&Toolpath> = doc.toolpaths().into_iter().filter(|t| t.visible).collect();
    if !shown.is_empty()
        && shown
            .iter()
            .all(|t| matches!(t.tool.tool_type, ToolGeometry::LaserTool { .. }))
    {
        return vec![("laser_grbl", None)];
    }
    let setup = doc.job_setup();
    if setup.is_rotary() {
        return match wrap_of(job) {
            Some(wrap) if wrap.axis == 'A' => vec![
                ("grbl_rotary_a_mm", Some(wrap)),
                ("mach3_rotary_mm", Some(wrap)),
            ],
            wrap => vec![("mach3_rotary_mm", wrap)],
        };
    }
    milling_posts().into_iter().map(|p| (p, None)).collect()
}

/// The blank a rotary job's programs wrap round, `None` for a flat job.
fn wrap_of(job: &Job) -> Option<Wrap> {
    let setup = job.kernel.document().job_setup();
    setup.is_rotary().then(|| {
        let along_x = setup.rotary.axis == carve_core::doc::RotaryAxis::X;
        Wrap {
            axis: if along_x { 'A' } else { 'B' },
            across: if along_x { 'Y' } else { 'X' },
            diameter: setup.rotary.diameter,
        }
    })
}

fn check_job(mut job: Job) -> (Vec<String>, Vec<String>) {
    let stock = preview(&job.kernel);
    let mut lines = Vec::new();
    let mut failures = Vec::new();
    for (post_id, wrap) in posts_for(&job) {
        let outcome = check_one(&mut job, post_id, &stock, wrap);
        lines.push(outcome.line);
        for p in outcome.problems {
            failures.push(format!("{} / {post_id}: {p}", job.name));
        }
        for n in outcome.notes {
            lines.push(format!("    note: {n}"));
        }
    }
    (lines, failures)
}

fn assert_job(job: Job) {
    let (lines, failures) = check_job(job);
    println!("{}", lines.join("\n"));
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_pocket_posts_as_previewed_through_every_post() {
    assert_job(pocket_job());
}

#[test]
fn a_profile_with_tabs_and_ramps_posts_as_previewed_through_every_post() {
    assert_job(profile_job());
}

#[test]
fn pecked_holes_post_as_previewed_through_every_post() {
    assert_job(drill_job());
}

#[test]
fn a_v_carve_posts_as_previewed_through_every_post() {
    assert_job(vgroove_job());
}

#[test]
fn a_3d_rough_and_finish_post_as_previewed_through_every_post() {
    assert_job(finish_job());
}

#[test]
fn an_inlay_posts_as_previewed_through_every_post() {
    assert_job(inlay_job());
}

#[test]
fn a_thread_posts_as_previewed_through_every_post() {
    assert_job(thread_job());
}

#[test]
fn a_wrapped_rotary_job_unwraps_to_the_flat_preview() {
    // The pocket job's 120 mm of Y wrapped round a 40 mm blank (A, about
    // X), and its 160 mm of X round a 52 mm blank (B, about Y).
    let mut failures = Vec::new();
    for (post_id, wrap) in [
        (
            "grbl_rotary_a_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "mach3_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "mach3_rotary_mm",
            Wrap {
                axis: 'B',
                across: 'X',
                diameter: 52.0,
            },
        ),
        (
            "linuxcnc_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "linuxcnc_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'X',
                diameter: 52.0,
            },
        ),
        (
            "linuxcnc_rotary_inch",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "linuxcnc_manual_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "linuxcnc_manual_rotary_inch",
            Wrap {
                axis: 'A',
                across: 'X',
                diameter: 52.0,
            },
        ),
        (
            "uccnc_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
        (
            "uccnc_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'X',
                diameter: 52.0,
            },
        ),
        (
            "uccnc_manual_rotary_mm",
            Wrap {
                axis: 'A',
                across: 'Y',
                diameter: 40.0,
            },
        ),
    ] {
        let mut job = pocket_job();
        let stock = preview(&job.kernel);
        let outcome = check_one(&mut job, post_id, &stock, Some(wrap));
        println!("{} {}", outcome.line, wrap.axis);
        assert!(
            outcome.notes.iter().all(|n| !n.starts_with("not posted")),
            "{:?}",
            outcome.notes
        );
        failures.extend(
            outcome
                .problems
                .into_iter()
                .map(|p| format!("{post_id} {}: {p}", wrap.axis)),
        );
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// One tool to a file: Snapmaker firmware has no M6, so the save refuses a second tool.
#[test]
fn a_snapmaker_program_cuts_what_the_flat_preview_shows() {
    let mut failures = Vec::new();
    for post_id in ["snapmaker_cnc_200w_mm", "snapmaker_cnc_50w_mm"] {
        let mut job = pocket_job();
        let stock = preview(&job.kernel);
        let outcome = check_one(&mut job, post_id, &stock, None);
        println!("{} {:?}", outcome.line, outcome.notes);
        assert!(
            outcome.notes.iter().all(|n| !n.starts_with("not posted")),
            "{:?}",
            outcome.notes
        );
        failures.extend(
            outcome
                .problems
                .into_iter()
                .map(|p| format!("{post_id}: {p}")),
        );
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A picture's overscan runs at the cutting speed with the beam held at
/// zero power in laser mode: not a cut with the beam off. A feed with the
/// laser stopped (M5) is.
#[test]
fn a_laser_overscan_at_zero_power_is_not_a_cut_with_the_beam_off() {
    let mut setup = Setup::new(Units::Mm, 0.0);
    setup.laser = true;
    let overscan = "G90\nG21\nM5\nG0X5Y10\nS0\nM4\nG1X10F3000\nS800\nG1X20\nS0\nG1X25\nM5\nM30\n";
    let report = gcode_check::check(overscan, &setup);
    assert!(
        report.at_least(Severity::Error).is_empty(),
        "{}",
        report.summary()
    );
    let stopped = "G90\nG21\nM5\nG0X5Y10\nS800\nG1X20F3000\nM30\n";
    let report = gcode_check::check(stopped, &setup);
    assert_eq!(report.of_kind(Kind::Beam).len(), 1, "{}", report.summary());
}

#[test]
fn a_laser_cut_burns_every_line_with_the_beam_on_and_ends_off() {
    let (mut kernel, scratch) = new_job("laser");
    let ids = vec![
        vector(
            &mut kernel,
            rect(Point::new(20.0, 20.0), Point::new(80.0, 60.0)).expect("rect"),
        ),
        vector(
            &mut kernel,
            circle(Point::new(120.0, 60.0), 20.0).expect("circle"),
        ),
    ];
    let beam = serde_json::to_value(Tool {
        name: "Laser".to_string(),
        number: 1,
        tool_type: ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: carve_core::cam::tool::DEFAULT_MAX_BURN_RATE,
        },
        feed_rate: 1200.0,
        plunge_rate: 1200.0,
        ..Tool::default()
    })
    .expect("tool");
    compute(
        &mut kernel,
        json!({ "strategy": "laserCutFill", "tool": beam, "ids": ids,
                "params": { "mode": "cut", "repeats": 1 } }),
    );
    let mut job = Job {
        name: "laser",
        kernel,
        depths: BTreeMap::new(),
        _scratch: scratch,
    };
    let toolpaths: Vec<Toolpath> = job
        .kernel
        .document()
        .toolpaths()
        .into_iter()
        .cloned()
        .collect();
    let ids: Vec<u64> = toolpaths.iter().map(|t| t.id).collect();
    let text = post(&mut job, "laser_grbl", &ids, None).expect("posts");
    let mut setup = Setup::new(Units::Mm, 0.0);
    setup.laser = true;
    let report = gcode_check::check(&text, &setup);
    let errors = report.at_least(Severity::Error);
    assert!(errors.is_empty(), "{}", report.summary());
    // The burn is as long as the toolpaths' cutting moves.
    let burned: f64 = report
        .motions
        .iter()
        .filter(|m| !m.rapid && m.spindle)
        .map(|m| (m.to[0] - m.from[0]).hypot(m.to[1] - m.from[1]))
        .sum();
    let wanted: f64 = toolpaths.iter().map(Toolpath::cutting_length).sum();
    assert!(
        (burned - wanted).abs() < 0.01 * wanted,
        "burned {burned:.2} mm, the toolpaths cut {wanted:.2} mm"
    );
    // Grbl's laser mode (M4) never fires on a rapid, so the beam may stay
    // switched on across travel moves.
    assert!(text.contains("M4"), "the beam runs in laser mode:\n{text}");
}

#[test]
fn a_drill_bank_posts_only_through_a_bank_post_and_fires_as_designed() {
    let (mut kernel, scratch) = new_job("bank");
    // Holes on the 32 mm system: drills 1, 2 and 4 of the default nine drill
    // line bank in one row, 1 and 3 in the next.
    let mut ids = Vec::new();
    for (x, y) in [
        (20.0, 40.0),
        (52.0, 40.0),
        (116.0, 40.0),
        (20.0, 90.0),
        (84.0, 90.0),
    ] {
        ids.push(vector(
            &mut kernel,
            circle(Point::new(x, y), 2.5).expect("circle"),
        ));
    }
    compute(
        &mut kernel,
        json!({ "strategy": "drillBank",
                "tool": tool("Drill 5 mm", 2, ToolGeometry::Drill { diameter: 5.0, point_angle: 118.0 }),
                "ids": ids,
                "params": { "drill": { "cutDepth": 10.0 } } }),
    );
    let mut job = Job {
        name: "bank",
        kernel,
        depths: BTreeMap::new(),
        _scratch: scratch,
    };
    let toolpaths: Vec<Toolpath> = job
        .kernel
        .document()
        .toolpaths()
        .into_iter()
        .cloned()
        .collect();
    let ids: Vec<u64> = toolpaths.iter().map(|t| t.id).collect();
    for post_id in milling_posts() {
        let posted = post(&mut job, post_id, &ids, None);
        if post_id != "drill_bank_mm" {
            assert!(
                posted.is_err(),
                "{post_id} posts a drill bank it cannot select"
            );
            continue;
        }
        let text = posted.expect("the bank post writes it");
        let mut setup = Setup::new(Units::Mm, toolpaths[0].safe_z);
        setup
            .tools
            .insert(toolpaths[0].tool_number, Cutter::Flat { diameter: 5.0 });
        let report = gcode_check::check(&text, &setup);
        assert!(
            report.at_least(Severity::Error).is_empty(),
            "{}",
            report.summary()
        );
        let low = report.lowest_by_tool()[&Some(toolpaths[0].tool_number)];
        assert!((low + 10.0).abs() < 0.0011, "the drills go to Z {low}");
        // One pattern line per firing whose set of drills changes.
        let patterns: Vec<&str> = text.lines().filter(|l| l.contains("PATTERN")).collect();
        let firings = &toolpaths[0].firings;
        assert!(
            !patterns.is_empty() && patterns.len() <= firings.len(),
            "{patterns:?}"
        );
    }
}

#[test]
fn the_saved_projects_post_as_previewed_through_every_post() {
    let mut failures = Vec::new();
    for (stem, job) in saved_projects() {
        println!("{stem}");
        let (lines, found) = check_job(job);
        println!("{}", lines.join("\n"));
        failures.extend(found.into_iter().map(|f| format!("{stem}: {f}")));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The post a saved program under `docs/testing/projects` was written
/// with, from what follows the project's name (`grbl.gcode` in
/// `house-sign-grbl.gcode`).
fn post_of_saved(rest: &str) -> Option<&'static str> {
    let stem = rest.rsplit_once('.')?.0;
    [
        ("grbl-inch", "grbl_inch"),
        ("grbl", "grbl_mm"),
        ("laser", "laser_grbl"),
        ("grbl-rotary", "grbl_rotary_a_mm"),
        ("mach3-rotary", "mach3_rotary_mm"),
        ("linuxcnc-rotary", "linuxcnc_rotary_mm"),
        ("uccnc-rotary", "uccnc_rotary_mm"),
        ("generic", "generic_gcode_mm"),
        ("linuxcnc", "linuxcnc_mm"),
        ("mach3", "mach3_mm"),
        ("uccnc", "uccnc_mm"),
    ]
    .into_iter()
    .find(|(name, _)| stem == *name)
    .map(|(_, id)| id)
}

#[test]
fn the_saved_programs_cut_what_their_projects_preview() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/testing/projects");
    let mut failures = Vec::new();
    let mut checked: Vec<String> = Vec::new();
    for (stem, job) in saved_projects() {
        let stock = preview(&job.kernel);
        let toolpaths: Vec<Toolpath> = job
            .kernel
            .document()
            .toolpaths()
            .into_iter()
            .filter(|t| t.visible)
            .cloned()
            .collect();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .map(|e| {
                e.filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for name in names {
            let Some(rest) = name.strip_prefix(&format!("{stem}-")) else {
                continue;
            };
            let Some(post_id) = post_of_saved(rest) else {
                println!("{name}: no known post for {rest}");
                continue;
            };
            let text = std::fs::read_to_string(dir.join(&name)).expect("reads");
            let outcome = check_text(&job, post_id, &stock, wrap_of(&job), &text, &toolpaths);
            println!("{name}: {}", outcome.line);
            failures.extend(outcome.problems.into_iter().map(|p| format!("{name}: {p}")));
            checked.push(name);
        }
    }
    println!("{} saved programs checked", checked.len());
    // Every saved program belongs to a project, a side or a sheet of one,
    // and names a post: none is left unchecked.
    for entry in std::fs::read_dir(&dir).expect("the projects folder") {
        let name = entry.expect("an entry").file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".carve") && !checked.iter().any(|c| *c == name) {
            failures.push(format!(
                "{name}: no project or post of that name, not checked"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_checker_finds_what_it_is_meant_to_find() {
    let mut setup = Setup::new(Units::Mm, 5.0);
    setup.tools.insert(1, Cutter::Flat { diameter: 6.0 });
    let good = "G90\nG21\nT1M6\nS10000M3\nG0Z5\nG0X10Y10\nG1Z-2F300\nG1X20F1000\nG0Z5\nM5\nM30\n";
    let report = gcode_check::check(good, &setup);
    assert!(report.issues.is_empty(), "{}", report.summary());
    assert_eq!(report.lowest(), Some(-2.0));
    assert_eq!(report.tool_changes, vec![1]);

    let kinds = |text: &str| -> Vec<Kind> {
        gcode_check::check(text, &setup)
            .issues
            .iter()
            .map(|i| i.kind)
            .collect()
    };
    // No units, a rapid across the job below safe Z, no spindle.
    assert!(kinds("G90\nT1M6\nG0Z5\nG0X10Y10\nM30\n").contains(&Kind::NoUnits));
    assert!(
        kinds("G90G21\nT1M6\nG0Z5\nG0X0Y0\nG0Z1\nG0X10\nM30\n").contains(&Kind::RapidBelowSafeZ)
    );
    assert!(kinds("G90G21\nT1M6\nG0Z5\nG0X0Y0\nG1Z-1F100\nG0Z5\nM30\n").contains(&Kind::SpindleOff));
    assert!(
        kinds("G90G21\nG0Z5\nG0X0Y0\nM3S1000\nG1Z-1F100\nG0Z5\nM5\nM30\n").contains(&Kind::NoTool)
    );
    // A move across before Z lifts, at the start or after a change, is an
    // error: the new cutter may still be on the material it was zeroed on.
    let across_first = gcode_check::check(
        "G90G21\nT1M6\nS9000M3\nG0X0Y0Z5\nG1Z-1F100\nG0Z5\nM5\nM30\n",
        &setup,
    );
    let unknown = across_first.of_kind(Kind::UnknownHeight);
    assert!(
        unknown.len() == 1 && unknown[0].severity == Severity::Error,
        "{}",
        across_first.summary()
    );
    assert!(
        kinds("G90G21\nT1M6\nS9000M3\nG0Z5\nG0X0Y0\nG1Z-1F100\nM5\nM30\n")
            .contains(&Kind::UnsafeEnd)
    );
    assert!(
        kinds("G90G21\nT1M6\nS9000M3\nG0Z5\nG0X0Y0\nG1Z-1\nG0Z5\nM5\nM30\n")
            .contains(&Kind::NoFeed)
    );
    assert!(kinds("G90G21\nT1M6\nS9000M3\nZ5\nM5\nM30\n").contains(&Kind::NoMotionMode));
    // A pause for a manual tool change with the spindle running.
    assert!(kinds("G90G21\nT1M6\nS9000M3\nG0Z5\nT2\nM0\nM30\n").contains(&Kind::SpindleOnAtChange));
    // A rapid that plunges into the material.
    let mut cut = setup.clone();
    cut.grid = Some(Grid {
        width: 40,
        height: 40,
        cell: 1.0,
        origin_x: 0.5,
        origin_y: 0.5,
        top: 0.0,
        bottom: -18.0,
    });
    let crash = gcode_check::check(
        "G90G21\nT1M6\nS9000M3\nG0Z5\nG0X10Y10\nG0Z-3\nG0Z5\nM5\nM30\n",
        &cut,
    );
    assert!(
        !crash.of_kind(Kind::RapidIntoMaterial).is_empty(),
        "{}",
        crash.summary()
    );
}

#[test]
fn the_checker_reads_arcs_by_centre_and_by_radius_alike() {
    let mut setup = Setup::new(Units::Mm, 5.0);
    setup.tools.insert(1, Cutter::Flat { diameter: 2.0 });
    let head = "G90G21G17\nT1M6\nS9000M3\nG0Z5\nG0X10Y0\nG1Z-1F200\n";
    // A half circle of radius 10 about the origin, counter clockwise, as
    // I J and as R; then a clockwise quarter with a negative R taking the
    // long way round.
    for arc in ["G3X-10Y0I-10J0", "G3X-10Y0R10"] {
        let text = format!("{head}{arc}\nG0Z5\nM5\nM30\n");
        let report = gcode_check::check(&text, &setup);
        assert!(report.issues.is_empty(), "{arc}: {}", report.summary());
        let top = report
            .motions
            .iter()
            .map(|m| m.to[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (top - 10.0).abs() < 0.01,
            "{arc} passes through Y 10: {top}"
        );
    }
    let long = format!("{head}G2X0Y10R-10\nG0Z5\nM5\nM30\n");
    let report = gcode_check::check(&long, &setup);
    let low = report
        .motions
        .iter()
        .map(|m| m.to[1])
        .fold(f64::INFINITY, f64::min);
    assert!((low + 10.0).abs() < 0.01, "three quarters round: {low}");
    // An arc whose end is off its circle.
    let off = format!("{head}G3X-10Y0I-9J0\nG0Z5\nM5\nM30\n");
    assert!(!gcode_check::check(&off, &setup)
        .of_kind(Kind::ArcRadius)
        .is_empty());
    // Inches are read as inches.
    let mut inch = Setup::new(Units::Inch, 0.2 * 25.4);
    inch.tools.insert(1, Cutter::Flat { diameter: 2.0 });
    let text = "G90G20\nT1M6\nS9000M3\nG0Z0.2\nG0X1Y1\nG1Z-0.1F10\nG0Z0.2\nM5\nM30\n";
    let report = gcode_check::check(text, &inch);
    assert!(report.issues.is_empty(), "{}", report.summary());
    assert!((report.lowest().expect("moves") + 2.54).abs() < 1e-9);
    assert!((report.motions[2].feed - 254.0).abs() < 1e-9);
}

#[test]
fn the_checker_reads_shopbot_and_heidenhain_programs() {
    let mut setup = Setup::new(Units::Mm, 5.0);
    setup.tools.insert(1, Cutter::Flat { diameter: 6.0 });
    let sbp = "SA\nIF %(25)=0 THEN GOTO UNIT_ERROR\n&Tool = 1\nC9\nTR,12000\nC6\nMS,20,5\nJZ,5\n\
               J2,10,10\nMZ,-2\nM3,20,10,-2\nJZ,5\nC7\nEND\nUNIT_ERROR:\nEND\n";
    let report = gcode_check::check(sbp, &setup);
    assert_eq!(report.dialect, gcode_check::Dialect::ShopBot);
    assert!(report.issues.is_empty(), "{}", report.summary());
    assert_eq!(report.units, Some(Units::Mm));
    let cut = report.motions.iter().find(|m| !m.rapid).expect("a cut");
    assert!((cut.feed - 300.0).abs() < 1e-9, "MZ runs at the Z speed");
    // Without the unit check the file runs in whatever units the machine
    // is set to.
    let unchecked = sbp.replace("IF %(25)=0 THEN GOTO UNIT_ERROR\n", "");
    assert!(!gcode_check::check(&unchecked, &setup)
        .of_kind(Kind::NoUnits)
        .is_empty());

    let h = "0 BEGIN PGM T MM\n1 TOOL CALL 1 Z S12000\n2 M3\n3 L Z+5.000 R0 FMAX\n\
             4 L X+10.000 Y+0.000 R0 FMAX\n5 L Z-1.000 R0 F200\n6 CC X+0.000 Y+0.000\n\
             7 C X-10.000 Y+0.000 DR+ R0 F800\n8 L Z+5.000 R0 FMAX M5\n9 M30\n10 END PGM T MM\n";
    let report = gcode_check::check(h, &setup);
    assert_eq!(report.dialect, gcode_check::Dialect::Heidenhain);
    assert!(report.issues.is_empty(), "{}", report.summary());
    // A TOOL CALL with only a speed changes the speed, not the tool.
    let faster = h.replace("5 L Z-1.000", "5 TOOL CALL S18000\n5 L Z-1.000");
    let report2 = gcode_check::check(&faster, &setup);
    assert!(report2.issues.is_empty(), "{}", report2.summary());
    assert_eq!(report2.tool_changes, vec![1]);
    let arc = report2.motions.iter().rfind(|m| !m.rapid).expect("a cut");
    assert!((arc.speed - 18000.0).abs() < 1e-9);
    let top = report
        .motions
        .iter()
        .map(|m| m.to[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (top - 10.0).abs() < 0.01,
        "DR+ runs counter clockwise: {top}"
    );
}

/// Two toolpaths with different tools, homed at `home_z` with the safe Z
/// at `safe_z`, starting as the CAM starts them: a rapid across to the
/// home height, then down.
fn two_tool_job(home_z: f64, safe_z: f64) -> Vec<Toolpath> {
    use carve_core::cam::toolpath::{Move, Point3, StrategyKind};
    let mut out = Vec::new();
    for (number, x) in [(1_u32, 10.0), (2, 30.0)] {
        let tool = Tool {
            name: format!("End Mill {number}"),
            number,
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        let mut tp = Toolpath::new(number as u64, "cut", tool, StrategyKind::Profile);
        tp.home = Point3::new(0.0, 0.0, home_z);
        tp.safe_z = safe_z;
        tp.feed = 1200.0;
        tp.plunge = 300.0;
        tp.spindle = 16000.0;
        tp.moves = vec![
            Move::rapid(Point3::new(x, 10.0, home_z)),
            Move::rapid(Point3::new(x, 10.0, safe_z)),
            Move::plunge(Point3::new(x, 10.0, -2.0)),
            Move::feed(Point3::new(x + 10.0, 10.0, -2.0)),
            Move::rapid(Point3::new(x + 10.0, 10.0, safe_z)),
            Move::rapid(Point3::new(x + 10.0, 10.0, home_z)),
        ];
        tp.recompute_bounds();
        out.push(tp);
    }
    out
}

#[test]
fn every_post_lifts_straight_up_before_it_moves_across_after_each_tool_change() {
    use carve_core::post::{post_process, PostJob};
    // The home above the safe Z and the safe Z above the home.
    for (home_z, safe_z) in [(20.0, 5.0), (15.0, 30.0)] {
        let clear = f64::max(home_z, safe_z);
        let toolpaths = two_tool_job(home_z, safe_z);
        for post_id in milling_posts() {
            let (_, source) = SHIPPED_POSTS
                .iter()
                .find(|(id, _)| *id == post_id)
                .expect("shipped");
            let post = parse_post(post_id, source).expect("parses");
            let job = PostJob {
                home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, home_z),
                safe_z,
                ..PostJob::default()
            };
            let text = post_process(&post, &job, &toolpaths).expect("posts");
            let mut setup = Setup::new(post_units(post_id), safe_z);
            setup.tool_length = tool_length_of(post_id);
            setup.preamble = sets_its_modes(post_id);
            for number in [1, 2] {
                setup.tools.insert(number, Cutter::Flat { diameter: 6.0 });
            }
            let report = gcode_check::check(&text, &setup);
            let errors = report.at_least(Severity::Error);
            assert!(
                errors.is_empty(),
                "{post_id} home {home_z} safe {safe_z}:\n{}\n{text}",
                report.summary()
            );
            // With each tool: the first motion goes straight up to the
            // clearance height, the next crosses at that height.
            for number in [1, 2] {
                let moves: Vec<_> = report
                    .motions
                    .iter()
                    .filter(|m| m.tool == Some(number))
                    .collect();
                let (lift, across) = (moves[0], moves[1]);
                let tol = 2e-3 * post_units(post_id).mm();
                assert!(
                    lift.rapid && !lift.known_start && (lift.to[2] - clear).abs() < tol,
                    "{post_id} tool {number}: first motion {lift:?}\n{text}"
                );
                assert!(
                    (across.from[2] - clear).abs() < tol && (across.to[2] - clear).abs() < tol,
                    "{post_id} tool {number}: the move across {across:?}\n{text}"
                );
            }
        }
    }
}

#[test]
fn the_hand_change_posts_name_the_tool_and_wait_before_the_spindle_starts() {
    use carve_core::post::{post_process, PostJob};
    let toolpaths = two_tool_job(20.0, 5.0);
    for (post_id, message) in [
        (
            "linuxcnc_manual_mm",
            "(MSG,Fit tool 2: End Mill 2, then resume)",
        ),
        (
            "linuxcnc_manual_inch",
            "(MSG,Fit tool 2: End Mill 2, then resume)",
        ),
        (
            "uccnc_manual_mm",
            "(Fit tool T2: End Mill 2, set Z zero, then press Cycle start)",
        ),
    ] {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        let job = PostJob {
            home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, 20.0),
            ..PostJob::default()
        };
        let text = post_process(&post, &job, &toolpaths).expect("posts");
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| *l == message)
            .unwrap_or_else(|| panic!("{post_id}: no {message}\n{text}"));
        // After the message: the change, a stop, a lift, then the spindle.
        let bare = |l: &str| {
            l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit())
                .to_string()
        };
        let rest: Vec<String> = lines[at + 1..].iter().map(|l| bare(l)).collect();
        let change = rest.iter().position(|l| l.contains("T2M6")).expect("M6");
        let stop = rest.iter().position(|l| l == "M0").expect("M0");
        let lift = rest
            .iter()
            .position(|l| l.starts_with("G0Z"))
            .expect("a lift");
        let spindle = rest.iter().position(|l| l.ends_with("M3")).expect("M3");
        assert!(
            change < stop && stop < lift && lift < spindle,
            "{post_id}\n{text}"
        );
        // And the old tool was lifted clear before the change.
        let before: Vec<String> = lines[..at].iter().map(|l| bare(l)).collect();
        assert!(
            before.iter().rev().take(2).any(|l| l.starts_with("G0Z")),
            "{post_id}\n{text}"
        );
    }
}

#[test]
fn the_probing_posts_name_the_tool_and_leave_the_length_to_the_m6() {
    use carve_core::post::{post_process, PostJob};
    let toolpaths = two_tool_job(20.0, 5.0);
    for (post_id, message) in [
        (
            "linuxcnc_mm",
            "(MSG,Change to tool 2: End Mill 2, M6 probes its length)",
        ),
        (
            "linuxcnc_inch",
            "(MSG,Change to tool 2: End Mill 2, M6 probes its length)",
        ),
        (
            "uccnc_mm",
            "(Change to tool T2: End Mill 2, M6 probes its length)",
        ),
    ] {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        let job = PostJob {
            home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, 20.0),
            safe_z: 5.0,
            ..PostJob::default()
        };
        let text = post_process(&post, &job, &toolpaths).expect("posts");
        let bare = |l: &str| {
            l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit())
                .to_string()
        };
        let lines: Vec<String> = text.lines().map(bare).collect();
        let at = lines
            .iter()
            .position(|l| l == message)
            .unwrap_or_else(|| panic!("{post_id}: no {message}\n{text}"));
        // Spindle off and the old tool lifted before the message.
        assert_eq!(lines[at - 2], "M5", "{post_id}\n{text}");
        assert!(lines[at - 1].starts_with("G0Z"), "{post_id}\n{text}");
        // Then exactly: the change, the lift to the clearance height, the
        // spindle; no length offset from the table and no pause.
        let lift = if post_id == "linuxcnc_inch" {
            "G0Z0.7874"
        } else if post_id == "uccnc_mm" {
            "G0Z20.000"
        } else {
            "G0Z20.0000"
        };
        assert_eq!(
            lines[at + 1..at + 4],
            ["T2M6".to_string(), lift.to_string(), "S16000M3".to_string()],
            "{post_id}\n{text}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with("G43") || l == "M0" || l == "M1"),
            "{post_id}\n{text}"
        );
        // The checker, told that M6 probes: no finding, and each tool's
        // first motion is the lift straight up.
        let mut setup = Setup::new(post_units(post_id), 5.0);
        setup.tool_length = ToolLength::ProbedByM6;
        setup.preamble = true;
        for number in [1, 2] {
            setup.tools.insert(number, Cutter::Flat { diameter: 6.0 });
        }
        let report = gcode_check::check(&text, &setup);
        assert!(report.issues.is_empty(), "{post_id}\n{}", report.summary());
        for number in [1, 2] {
            let first = report
                .motions
                .iter()
                .find(|m| m.tool == Some(number))
                .expect("a move");
            assert!(
                first.rapid && !first.known_start && first.from[..2] == first.to[..2],
                "{post_id} tool {number}: {first:?}"
            );
        }
        // Read as a touch off machine, the same program cuts with tools no
        // one measured.
        setup.tool_length = ToolLength::TouchOffAtPause;
        let report = gcode_check::check(&text, &setup);
        assert!(
            !report.of_kind(Kind::ToolLength).is_empty(),
            "{post_id}\n{}",
            report.summary()
        );
    }
}

/// With "M6 macro applies tool length" off (a macro that probes into the
/// tool table and leaves the length to the program), the probing posts
/// write G43 H and the new tool's number right after each M6, and G49 in
/// the header and at the end. The checker, told the program applies the
/// table length, accepts it; told the M6 applies the length, it does not.
#[test]
fn the_probing_posts_apply_the_table_length_when_the_m6_does_not() {
    use carve_core::post::{post_process, PostJob};
    let toolpaths = two_tool_job(20.0, 5.0);
    for (post_id, end) in [
        ("linuxcnc_mm", "M2"),
        ("linuxcnc_inch", "M2"),
        ("uccnc_mm", "M30"),
    ] {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        assert!(post.summary().length_choice, "{post_id}");
        let on = PostJob {
            home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, 20.0),
            safe_z: 5.0,
            ..PostJob::default()
        };
        assert!(on.m6_applies_length, "the option defaults on");
        let off = PostJob {
            m6_applies_length: false,
            ..on.clone()
        };
        let bare = |text: &str| -> Vec<String> {
            text.lines()
                .map(|l| {
                    l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit())
                        .to_string()
                })
                .collect()
        };
        let text = post_process(&post, &off, &toolpaths).expect("posts");
        let lines = bare(&text);
        for number in [1, 2] {
            let at = lines
                .iter()
                .position(|l| *l == format!("T{number}M6"))
                .unwrap_or_else(|| panic!("{post_id}: no T{number}M6\n{text}"));
            assert_eq!(lines[at + 1], format!("G43H{number}"), "{post_id}\n{text}");
            assert!(lines[at + 2].starts_with("G0Z"), "{post_id}\n{text}");
        }
        let g49: Vec<usize> = (0..lines.len()).filter(|i| lines[*i] == "G49").collect();
        let first_change = lines.iter().position(|l| l == "T1M6").expect("a change");
        assert_eq!(g49.len(), 2, "{post_id}\n{text}");
        assert!(g49[0] < first_change, "{post_id}\n{text}");
        let last = lines
            .iter()
            .rposition(|l| !l.is_empty() && l != "%")
            .expect("an end");
        assert_eq!(lines[last], end, "{post_id}\n{text}");
        assert_eq!(g49[1], last - 1, "{post_id}\n{text}");
        if post_id == "uccnc_mm" {
            // Every line still carries its own number, counting up by ten.
            let numbers: Vec<u32> = text
                .lines()
                .filter_map(|l| l.strip_prefix('N'))
                .map(|l| {
                    l.chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                        .parse()
                        .expect("a number")
                })
                .collect();
            assert!(numbers.windows(2).all(|w| w[1] == w[0] + 10), "{text}");
        }
        // The default writes none of it.
        let default = post_process(&post, &on, &toolpaths).expect("posts");
        assert!(
            !bare(&default)
                .iter()
                .any(|l| l.starts_with("G43") || l == "G49"),
            "{post_id}\n{default}"
        );

        let mut setup = Setup::new(post_units(post_id), 5.0);
        setup.preamble = true;
        for number in [1, 2] {
            setup.tools.insert(number, Cutter::Flat { diameter: 6.0 });
        }
        setup.tool_length = ToolLength::TableAfterM6;
        let report = gcode_check::check(&text, &setup);
        assert!(report.issues.is_empty(), "{post_id}\n{}", report.summary());
        // The default program read as a table length machine cuts with
        // lengths nobody applied; this one read as a probing machine has
        // the table replace the probed length.
        let report = gcode_check::check(&default, &setup);
        assert!(!report.of_kind(Kind::ToolLength).is_empty(), "{post_id}");
        // A G43 H naming another tool is found.
        let wrong = text.replace("G43H2", "G43H1");
        let report = gcode_check::check(&wrong, &setup);
        assert!(
            report
                .of_kind(Kind::ToolLength)
                .iter()
                .any(|i| i.text == "G43 H1 does not name tool 2 in the spindle"),
            "{post_id}\n{}",
            report.summary()
        );
        setup.tool_length = ToolLength::ProbedByM6;
        let report = gcode_check::check(&text, &setup);
        assert!(!report.of_kind(Kind::ToolLength).is_empty(), "{post_id}");
    }
    // The posts with no such lines offer no choice.
    for id in [
        "linuxcnc_manual_mm",
        "uccnc_manual_mm",
        "grbl_mm",
        "mach3_mm",
    ] {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(n, _)| *n == id)
            .expect("shipped");
        assert!(
            !parse_post(id, source)
                .expect("parses")
                .summary()
                .length_choice,
            "{id}"
        );
    }
}

/// The LinuxCNC and UCCNC headers set every modal state the moves rely on
/// before the first tool change (review 031 finding 8: no G94, G40 or G49,
/// and both controls keep them from the program run before). The probing
/// posts never write G49: it would cancel the length the M6 probed, and a
/// routine that skips a change to the tool already fitted would not probe
/// it again. The manual posts cancel it, their G43 H or touch off sets it.
#[test]
fn the_linuxcnc_and_uccnc_headers_set_their_modes_and_keep_a_probed_length() {
    use carve_core::post::{post_process, PostJob};
    let toolpaths = two_tool_job(20.0, 5.0);
    let job = PostJob {
        home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, 20.0),
        safe_z: 5.0,
        ..PostJob::default()
    };
    let mut checked = 0;
    for post_id in milling_posts().into_iter().filter(|id| sets_its_modes(id)) {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        let text = post_process(&post, &job, &toolpaths).expect("posts");
        let bare = |l: &str| {
            l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit())
                .to_string()
        };
        let lines: Vec<String> = text.lines().map(bare).collect();
        let change = lines
            .iter()
            .position(|l| l.ends_with("M6"))
            .unwrap_or_else(|| panic!("{post_id}: no M6\n{text}"));
        let first_move = lines
            .iter()
            .position(|l| {
                l.starts_with("G0Z")
                    || l.starts_with("G0X")
                    || (l.starts_with("G1") && !l.starts_with("G17"))
            })
            .expect("a move");
        let header = &lines[..first_move.min(change)];
        let units = if post_units(post_id) == Units::Inch {
            "G20"
        } else {
            "G21"
        };
        for word in ["G90", "G94", units, "G17", "G40", "G64"] {
            assert!(
                header.iter().any(|l| l.contains(word)),
                "{post_id}: {word} is not set before the first move\n{text}"
            );
        }
        let probing = tool_length_of(post_id) == ToolLength::ProbedByM6;
        let g49 = lines.iter().filter(|l| l.contains("G49")).count();
        if probing {
            assert_eq!(g49, 0, "{post_id}: G49 cancels the probed length\n{text}");
        } else {
            assert!(
                header.iter().any(|l| l == "G49"),
                "{post_id}: the manual post cancels any old length first\n{text}"
            );
        }
        // UCCNC reads G64 P as its unify length, not a tolerance.
        if post_id.starts_with("uccnc") {
            assert!(header.iter().any(|l| l == "G64"), "{post_id}\n{text}");
        }

        let mut setup = Setup::new(post_units(post_id), 5.0);
        setup.tool_length = tool_length_of(post_id);
        setup.preamble = true;
        for number in [1, 2] {
            setup.tools.insert(number, Cutter::Flat { diameter: 6.0 });
        }
        let kinds = |text: &str| -> Vec<Kind> {
            gcode_check::check(text, &setup)
                .at_least(Severity::Error)
                .iter()
                .map(|i| i.kind)
                .collect()
        };
        assert_eq!(kinds(&text), vec![], "{post_id}\n{text}");
        // Without any one of the modes the checker finds it.
        let dropped = |word: &str| -> String {
            text.lines()
                .filter(|l| bare(l) != word)
                .map(|l| format!("{l}\n"))
                .collect()
        };
        let mut words = vec!["G40"];
        if !probing {
            words.push("G49");
        }
        if post_id.starts_with("linuxcnc") {
            words.push("G94");
        }
        for word in words {
            let without = dropped(word);
            assert_ne!(without, text, "{post_id}: {word}");
            assert_eq!(
                kinds(&without),
                vec![Kind::Modal],
                "{post_id} without {word}"
            );
        }
        // A G49 in a probing post's header is an error.
        if probing {
            let cancelled = text.replacen("G17", "G17\nG49", 1);
            assert!(kinds(&cancelled).contains(&Kind::ToolLength), "{post_id}");
        }
        checked += 1;
    }
    assert_eq!(checked, 12);
}

#[test]
fn the_checker_tells_a_probing_m6_from_a_touch_off_at_a_pause() {
    let checked = |text: &str, how: ToolLength| -> Vec<Kind> {
        let mut setup = Setup::new(Units::Mm, 5.0);
        setup.tool_length = how;
        setup.tools.insert(1, Cutter::Flat { diameter: 6.0 });
        gcode_check::check(text, &setup)
            .at_least(Severity::Error)
            .iter()
            .map(|i| i.kind)
            .collect()
    };
    let program = |change: &str| {
        format!(
            "G90G21\nM5\nG0Z20\n{change}\nG0Z20\nS9000M3\nG0X10Y10\nG0Z5\nG1Z-2F300\nG1X20F900\nG0Z20\nM5\nM30\n"
        )
    };
    let probing = program("T1M6");
    let manual = program("T1M6\nG43H1\nM0");
    let no_pause = program("T1M6\nG43H1");
    let pause_only = program("T1\nM0");
    // Each form is clean on its own machine.
    assert_eq!(checked(&probing, ToolLength::ProbedByM6), vec![]);
    assert_eq!(checked(&manual, ToolLength::TouchOffAtPause), vec![]);
    assert_eq!(checked(&pause_only, ToolLength::TouchOffAtPause), vec![]);
    // A probing M6 followed by the tool table's length throws the probe away.
    assert_eq!(
        checked(&manual, ToolLength::ProbedByM6),
        vec![Kind::ToolLength]
    );
    // A change with no probe and no pause cuts with an unmeasured tool.
    assert_eq!(
        checked(&probing, ToolLength::TouchOffAtPause),
        vec![Kind::ToolLength]
    );
    assert_eq!(
        checked(&no_pause, ToolLength::TouchOffAtPause),
        vec![Kind::ToolLength]
    );
    assert_eq!(
        checked(&pause_only, ToolLength::ProbedByM6),
        vec![Kind::ToolLength]
    );
    // Not checked: the old reading of every form stands.
    for text in [&probing, &manual, &no_pause, &pause_only] {
        assert_eq!(checked(text, ToolLength::Unchecked), vec![], "{text}");
    }
    // Both forms: a move across before the lift after the change is an
    // error, the macro or the operator having left Z anywhere.
    for (text, how) in [
        (&probing, ToolLength::ProbedByM6),
        (&manual, ToolLength::TouchOffAtPause),
    ] {
        let early = text.replacen("\nG0Z20\nS9000M3\nG0X10Y10", "\nS9000M3\nG0X10Y10", 1);
        assert_ne!(&early, text);
        assert!(
            checked(&early, how).contains(&Kind::UnknownHeight),
            "{early}"
        );
    }
}

#[test]
fn a_drilled_hole_shows_its_point_in_the_preview_and_the_checker() {
    // 5 mm holes 12 mm deep with a 118 degree point: the depth is the tip's,
    // at the centre, and the wall stops the point's length above it.
    let mut job = drill_job();
    let toolpaths: Vec<Toolpath> = job
        .kernel
        .document()
        .toolpaths()
        .into_iter()
        .cloned()
        .collect();
    assert!(
        toolpaths
            .iter()
            .all(|t| t.bounds.is_some_and(|b| (b.min.z + 12.0).abs() < 1e-6)),
        "the toolpath goes to the centre depth"
    );
    let stock = preview(&job.kernel);
    // The grid is 0.5 mm, so a sample lands up to a quarter cell off the axis.
    let point = |r: f64| -12.0 + r / 59.0f64.to_radians().tan();
    let (x, y) = (30.0, 30.0);
    let preview_at = |dx: f64| f64::from(stock.height_at(x + dx, y).expect("in the stock"));
    assert!((preview_at(0.0) + 12.0).abs() < 0.3, "{}", preview_at(0.0));
    let wall = preview_at(2.0);
    assert!(
        (wall - point(2.0)).abs() < 0.35,
        "preview {wall}, point {}",
        point(2.0)
    );
    assert!(wall > -11.5, "a flat drill would show {wall} near -12");

    let ids: Vec<u64> = toolpaths.iter().map(|t| t.id).collect();
    let text = post(&mut job, "linuxcnc_mm", &ids, None).expect("posts");
    let mut setup = Setup::new(Units::Mm, 5.0);
    for t in &toolpaths {
        setup
            .tools
            .insert(t.tool_number, cutter_of(&t.tool).expect("a cutter"));
    }
    setup.grid = Some(grid_of(&stock));
    let report = gcode_check::check(&text, &setup);
    let field = report.field.as_ref().expect("a field");
    let centre = field.height_at(x, y).expect("in the field");
    let checked = field.height_at(x + 2.0, y).expect("in the field");
    assert!((centre + 12.0).abs() < 0.3, "{centre}");
    assert!(
        (checked - point(2.0)).abs() < 0.35,
        "checker {checked}, point {}",
        point(2.0)
    );
}

#[test]
fn an_m6_in_inverse_time_is_an_error_and_in_units_per_minute_is_not() {
    let program = |before: &str| {
        format!(
            "G90 G93 G21 G17 G40\nG0 Z10\n{before}T1 M6\nG93\nG0 Z10\nS1000 M3\nG0 X0 Y0\nM5\nM2\n"
        )
    };
    let setup = Setup::new(Units::Mm, 5.0);
    let bad = gcode_check::check(&program(""), &setup);
    assert!(
        bad.issues
            .iter()
            .any(|i| i.kind == Kind::ToolChangeInverseTime && i.severity == Severity::Error),
        "{}",
        bad.summary()
    );
    let good = gcode_check::check(&program("G94\n"), &setup);
    assert!(
        !good
            .issues
            .iter()
            .any(|i| i.kind == Kind::ToolChangeInverseTime),
        "{}",
        good.summary()
    );
}

#[test]
fn a_rotary_turn_in_units_per_minute_is_an_error_and_inverse_time_is_not() {
    // A quarter turn of a 40 mm blank is 31.4 mm on the surface.
    let mut setup = Setup::new(Units::Mm, 5.0);
    setup.tools.insert(1, Cutter::Flat { diameter: 3.0 });
    setup.rotary = Some(gcode_check::Rotary {
        axis: 'A',
        across: 'Y',
        diameter: 40.0,
        linear: false,
        reversed: false,
    });
    let program = |mode: &str, f: &str| {
        format!(
            "G90{mode}\nG21\nT1M6\nG0Z30\nS10000M3\nG0X0Y0A0\nG0Z21\nG1Z19{f}\nG1A90{f}\nG0Z30\nM5\nG94\nM30\n"
        )
    };
    // G94 F1000: the control turns 1000 degrees a minute, 349 mm/min here.
    let units = gcode_check::check(&program("G94", "F1000"), &setup);
    let turn = units
        .issues
        .iter()
        .find(|i| i.kind == Kind::RotaryFeed)
        .unwrap_or_else(|| panic!("{}", units.summary()));
    assert_eq!(turn.severity, Severity::Error);
    assert!(turn.text.contains("349 mm/min"), "{}", turn.text);
    // G93: one over the minutes, 31.8 for 31.4 mm at 1000 mm/min.
    let inverse = gcode_check::check(
        &program("G93", "F31.831").replace("G1Z19F31.831", "G1Z19F500"),
        &setup,
    );
    assert!(
        !inverse.issues.iter().any(|i| i.kind == Kind::RotaryFeed),
        "{}",
        inverse.summary()
    );
    let cut = inverse
        .motions
        .iter()
        .find(|m| !m.rapid && (m.to[1] - m.from[1]).abs() > 1.0)
        .expect("the turn");
    assert!((cut.feed - 1000.0).abs() < 1.0, "{}", cut.feed);
}

#[test]
fn the_rotary_posts_feed_wrapped_cuts_in_inverse_time_and_flat_ones_per_minute() {
    use carve_core::post::interp::post_process_wrapped;
    use carve_core::post::{post_process, rotary::RotaryWord, PostJob};
    let toolpaths = two_tool_job(20.0, 5.0);
    for post_id in [
        "mach3_rotary_mm",
        "grbl_rotary_a_mm",
        "linuxcnc_rotary_mm",
        "linuxcnc_rotary_inch",
        "linuxcnc_manual_rotary_mm",
        "linuxcnc_manual_rotary_inch",
        "uccnc_rotary_mm",
        "uccnc_manual_rotary_mm",
    ] {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        let wrap = carve_core::post::rotary::Wrap {
            word: RotaryWord::A,
            blank_along: carve_core::doc::RotaryAxis::X,
            wiring: Default::default(),
            diameter: 40.0,
        };
        let wrapped = post_process_wrapped(&post, &PostJob::default(), &toolpaths, Some(wrap))
            .expect("posts");
        let bare = |l: &str| {
            l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit())
                .to_string()
        };
        let lines: Vec<String> = wrapped.lines().map(bare).collect();
        assert!(
            lines.iter().any(|l| l == "G90G93" || l == "G93"),
            "{post_id}\n{wrapped}"
        );
        // In G93 every cut carries its own rate.
        assert!(
            lines
                .iter()
                .filter(|l| l.starts_with("G1") && !l.starts_with("G17"))
                .all(|l| l.contains('F')),
            "{post_id}\n{wrapped}"
        );
        let end = lines
            .iter()
            .position(|l| l == "M30" || l == "M2")
            .expect("an end code");
        assert!(
            lines[..end].iter().rev().take(3).any(|l| l == "G94"),
            "{post_id}\n{wrapped}"
        );
        let flat = post_process(&post, &PostJob::default(), &toolpaths).expect("posts");
        assert!(!flat.contains("G93"), "{post_id}\n{flat}");
        assert!(
            flat.lines().map(bare).any(|l| l == "G90G94" || l == "G94"),
            "{post_id}\n{flat}"
        );
    }
}

#[test]
fn names_and_notes_with_brackets_or_line_breaks_post_as_programs_every_control_reads() {
    use carve_core::post::{post_process, PostJob};
    // Tool names such as "End Mill (1/4 inch)" are common, and a note may run
    // over several lines: neither may end a comment early or start a line the
    // controller reads as a command.
    let mut toolpaths = two_tool_job(20.0, 5.0);
    toolpaths[0].tool.name = "End Mill (1/4 inch)".to_string();
    toolpaths[0].name = "Profile (outside)".to_string();
    toolpaths[1].tool.name = "Ball Nose (3 mm)".to_string();
    toolpaths[1].tool.notes = "spare (in the drawer)".to_string();
    let job = PostJob {
        home: carve_core::cam::toolpath::Point3::new(0.0, 0.0, 20.0),
        safe_z: 5.0,
        notes: "Sign for Ann (front)\nclamp at the ends".to_string(),
        file_name: "sign (1).nc".to_string(),
        ..PostJob::default()
    };
    for post_id in milling_posts() {
        let (_, source) = SHIPPED_POSTS
            .iter()
            .find(|(id, _)| *id == post_id)
            .expect("shipped");
        let post = parse_post(post_id, source).expect("parses");
        let text = post_process(&post, &job, &toolpaths).expect("posts");
        let mut setup = Setup::new(post_units(post_id), 5.0);
        setup.tool_length = tool_length_of(post_id);
        setup.preamble = sets_its_modes(post_id);
        for number in [1, 2] {
            setup.tools.insert(number, Cutter::Flat { diameter: 6.0 });
        }
        let report = gcode_check::check(&text, &setup);
        let errors = report.at_least(Severity::Error);
        assert!(
            errors.is_empty(),
            "{post_id}:\n{}\n{text}",
            report.summary()
        );
        assert_eq!(report.tool_changes, vec![1, 2], "{post_id}\n{text}");
    }
}

#[test]
fn the_checker_draws_its_lines_at_the_edges() {
    // Each check fires a hundredth of a millimetre past its limit and not on
    // it: a loose limit would let a post that is slightly wrong through.
    let mut setup = Setup::new(Units::Mm, 5.0);
    setup.tools.insert(1, Cutter::Flat { diameter: 6.0 });
    let kinds = |text: &str| -> Vec<Kind> {
        gcode_check::check(text, &setup)
            .issues
            .iter()
            .map(|i| i.kind)
            .collect()
    };
    let across_at =
        |z: &str| format!("G90G21\nT1M6\nS9000M3\nG0Z5\nG0X0Y0\nG0Z{z}\nG0X10\nG0Z5\nM5\nM30\n");
    assert!(!kinds(&across_at("5")).contains(&Kind::RapidBelowSafeZ));
    assert!(kinds(&across_at("4.99")).contains(&Kind::RapidBelowSafeZ));
    let ends_at =
        |z: &str| format!("G90G21\nT1M6\nS9000M3\nG0Z5\nG0X0Y0\nG1Z-1F100\nG0Z{z}\nM5\nM30\n");
    assert!(!kinds(&ends_at("5")).contains(&Kind::UnsafeEnd));
    assert!(kinds(&ends_at("4.99")).contains(&Kind::UnsafeEnd));
    // A tool changed by hand with the spindle running is an error; an
    // automatic changer stops it itself, so that is a warning.
    let by_hand = gcode_check::check("G90G21\nT1M6\nS9000M3\nG0Z5\nT2\nM0\nM30\n", &setup);
    let found = by_hand.of_kind(Kind::SpindleOnAtChange);
    assert_eq!(found.len(), 1, "{}", by_hand.summary());
    assert_eq!(found[0].severity, Severity::Error);
    let automatic = gcode_check::check("G90G21\nT1M6\nS9000M3\nG0Z5\nT2M6\nM30\n", &setup);
    let found = automatic.of_kind(Kind::SpindleOnAtChange);
    assert_eq!(found.len(), 1, "{}", automatic.summary());
    assert_eq!(found[0].severity, Severity::Warning);
    // A rapid a fiftieth of a millimetre into the material is one into it.
    let mut cut = setup.clone();
    cut.grid = Some(Grid {
        width: 40,
        height: 40,
        cell: 1.0,
        origin_x: 0.5,
        origin_y: 0.5,
        top: 0.0,
        bottom: -18.0,
    });
    let into = |z: &str| {
        gcode_check::check(
            &format!("G90G21\nT1M6\nS9000M3\nG0Z5\nG0X10Y10\nG0Z{z}\nG0Z5\nM5\nM30\n"),
            &cut,
        )
        .of_kind(Kind::RapidIntoMaterial)
        .len()
    };
    assert_eq!(into("0"), 0);
    assert_eq!(into("-0.02"), 1);
}

// ---- review 031 (criticcore): more strategies, Z zero at the bottom, and
// the feeds and speed of merged and tiled toolpaths ----

fn probe_job(name: &'static str, z_zero_bottom: bool) -> (Kernel, Scratch) {
    let scratch = Scratch::new(name);
    let mut kernel = kernel(&scratch);
    let mut job = run(&mut kernel, "job.get", json!({}));
    job["width"] = json!(160.0);
    job["height"] = json!(120.0);
    job["thickness"] = json!(18.0);
    if z_zero_bottom {
        job["zZero"] = json!("bottom");
    }
    run(&mut kernel, "job.set", job);
    (kernel, scratch)
}

/// A pocket with an island, offset cleared with a profile pass and a ramp.
fn pocket_island_job() -> Job {
    let (mut kernel, scratch) = probe_job("island", false);
    let outer = vector(
        &mut kernel,
        rect(Point::new(30.0, 30.0), Point::new(90.0, 80.0)).expect("rect"),
    );
    let island = vector(
        &mut kernel,
        circle(Point::new(60.0, 55.0), 8.0).expect("circle"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": end_mill(1, 6.0), "ids": [outer, island],
                "params": { "cutDepth": 5.0, "passDepth": 2.5, "stepover": 2.4,
                            "profilePass": true, "strategy": "offset",
                            "ramp": { "enabled": true, "distance": 10.0, "kind": "smooth" } } }),
    );
    Job {
        name: "island",
        kernel,
        depths: BTreeMap::from([(1, Some(-5.0))]),
        _scratch: scratch,
    }
}

/// A V-groove with a flat floor, in passes, cleared with an end mill.
fn vgroove_flat_job() -> Job {
    let (mut kernel, scratch) = probe_job("vflat", false);
    let outer = vector(
        &mut kernel,
        rect(Point::new(30.0, 30.0), Point::new(100.0, 80.0)).expect("rect"),
    );
    let inner = vector(
        &mut kernel,
        rect(Point::new(45.0, 45.0), Point::new(85.0, 65.0)).expect("rect"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "vgroove",
                "tool": tool("V-Bit 90 deg 20 mm", 3,
                             ToolGeometry::VBit { diameter: 20.0, included_angle: 90.0 }),
                "ids": [outer, inner],
                "params": { "startDepth": 0.0, "flatDepth": 3.0, "maxPassDepth": 1.5,
                            "clearanceTool": end_mill(4, 6.0) } }),
    );
    Job {
        name: "vflat",
        kernel,
        depths: BTreeMap::from([(3, Some(-3.0)), (4, Some(-3.0))]),
        _scratch: scratch,
    }
}

/// A chamfer round a rectangle.
fn chamfer_job() -> Job {
    let (mut kernel, scratch) = probe_job("chamfer", false);
    let id = vector(
        &mut kernel,
        rect(Point::new(40.0, 30.0), Point::new(110.0, 85.0)).expect("rect"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "chamfer",
                "tool": tool("V-Bit 90 deg 20 mm", 3,
                             ToolGeometry::VBit { diameter: 20.0, included_angle: 90.0 }),
                "ids": [id], "params": { "width": 3.0, "side": "outside" } }),
    );
    Job {
        name: "chamfer",
        kernel,
        depths: BTreeMap::from([(3, None)]),
        _scratch: scratch,
    }
}

/// Three flutes with ramped ends.
fn fluting_job() -> Job {
    let (mut kernel, scratch) = probe_job("fluting", false);
    let mut ids = Vec::new();
    for y in [40.0, 60.0, 80.0] {
        let line =
            carve_core::geom::polyline_path(&[Point::new(30.0, y), Point::new(130.0, y)], false)
                .expect("line");
        ids.push(vector(&mut kernel, line));
    }
    compute(
        &mut kernel,
        json!({ "strategy": "fluting",
                "tool": tool("Ball Nose 6 mm", 5, ToolGeometry::BallNose { diameter: 6.0 }),
                "ids": ids,
                "params": { "depth": 4.0, "rampStart": true, "rampEnd": true,
                            "rampLength": 15.0 } }),
    );
    Job {
        name: "fluting",
        kernel,
        depths: BTreeMap::from([(5, Some(-4.0))]),
        _scratch: scratch,
    }
}

/// A keyhole slot.
fn keyhole_job() -> Job {
    let (mut kernel, scratch) = probe_job("keyhole", false);
    let id = vector(
        &mut kernel,
        circle(Point::new(70.0, 60.0), 2.0).expect("circle"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "keyhole", "tool": end_mill(1, 6.0), "ids": [id],
                "params": { "depth": 8.0, "length": 20.0, "direction": "bottomToTop" } }),
    );
    Job {
        name: "keyhole",
        kernel,
        depths: BTreeMap::from([(1, Some(-8.0))]),
        _scratch: scratch,
    }
}

/// A pocket and pecked holes with Z zero on the machine bed: job Z runs
/// from 18 at the surface down.
fn bottom_zero_job() -> Job {
    let (mut kernel, scratch) = probe_job("bottomzero", true);
    let id = vector(
        &mut kernel,
        rect(Point::new(30.0, 30.0), Point::new(90.0, 80.0)).expect("rect"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": end_mill(1, 6.0), "ids": [id],
                "params": { "cutDepth": 6.0, "passDepth": 2.0, "stepover": 2.4 } }),
    );
    let hole = vector(
        &mut kernel,
        circle(Point::new(120.0, 60.0), 3.0).expect("circle"),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "drill",
                "tool": tool("Drill 5 mm", 2,
                             ToolGeometry::Drill { diameter: 5.0, point_angle: 118.0 }),
                "ids": [hole], "params": { "cutDepth": 12.0, "peckDepth": 4.0 } }),
    );
    Job {
        name: "bottomzero",
        kernel,
        depths: BTreeMap::from([(1, Some(12.0)), (2, Some(6.0))]),
        _scratch: scratch,
    }
}

#[test]
fn a_pocket_with_an_island_posts_as_previewed_through_every_post() {
    assert_job(pocket_island_job());
}

#[test]
fn a_flat_bottomed_v_carve_in_passes_posts_as_previewed_through_every_post() {
    assert_job(vgroove_flat_job());
}

#[test]
fn a_chamfer_posts_as_previewed_through_every_post() {
    assert_job(chamfer_job());
}

#[test]
fn ramped_flutes_post_as_previewed_through_every_post() {
    assert_job(fluting_job());
}

#[test]
fn a_keyhole_posts_as_previewed_through_every_post() {
    assert_job(keyhole_job());
}

#[test]
fn a_job_with_z_zero_on_the_bed_posts_as_previewed_through_every_post() {
    assert_job(bottom_zero_job());
}

/// Two pockets cut with one cutter at different feeds and speeds, merged
/// into one toolpath: the second part asks for 2400 mm/min at 20000 rpm.
fn two_rate_pockets(name: &'static str) -> (Kernel, Scratch) {
    two_pockets(name, 20000.0, true)
}

/// Two pockets with one 6 mm end mill (tool 1), the left at 800 mm/min and
/// 12000 rpm, the right at 2400 mm/min and `fast_speed`, merged or left as
/// two toolpaths.
fn two_pockets(name: &'static str, fast_speed: f64, merge: bool) -> (Kernel, Scratch) {
    let (mut kernel, scratch) = probe_job(name, false);
    let a = vector(
        &mut kernel,
        rect(Point::new(30.0, 30.0), Point::new(60.0, 60.0)).expect("rect"),
    );
    let b = vector(
        &mut kernel,
        rect(Point::new(90.0, 30.0), Point::new(120.0, 60.0)).expect("rect"),
    );
    let mut slow = end_mill(1, 6.0);
    slow["feedRate"] = json!(800.0);
    slow["spindleSpeed"] = json!(12000.0);
    let mut fast = end_mill(1, 6.0);
    fast["feedRate"] = json!(2400.0);
    fast["spindleSpeed"] = json!(fast_speed);
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": slow, "ids": [a],
                "params": { "cutDepth": 3.0, "passDepth": 3.0, "stepover": 2.4 } }),
    );
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": fast, "ids": [b],
                "params": { "cutDepth": 3.0, "passDepth": 3.0, "stepover": 2.4 } }),
    );
    if merge {
        let ids: Vec<u64> = kernel.document().toolpaths().iter().map(|t| t.id).collect();
        run(
            &mut kernel,
            "cam.merge",
            json!({ "ids": ids, "name": "Both" }),
        );
    }
    (kernel, scratch)
}

fn visible_ids(kernel: &Kernel) -> Vec<u64> {
    kernel
        .document()
        .toolpaths()
        .iter()
        .filter(|t| t.visible)
        .map(|t| t.id)
        .collect()
}

/// The feed and spindle speed of every cut across X right of `x_min`,
/// below the surface.
fn cuts_right_of(text: &str, units: Units, x_min: f64) -> Vec<(f64, f64)> {
    let setup = Setup::new(units, 5.0);
    let report = gcode_check::check(text, &setup);
    report
        .motions
        .iter()
        .filter(|m| {
            !m.rapid && m.to[0] > x_min && m.to[2] < 0.0 && (m.to[0] - m.from[0]).abs() > 1e-6
        })
        .map(|m| (m.feed, m.speed))
        .collect()
}

/// Every milling post writes the speed and start of the spindle in its own
/// SPINDLE_ON block, so a merged toolpath whose parts run at different
/// speeds changes speed on the way to each part (review 031: the posts
/// wrote it in the tool change only, so the second part ran at the
/// first's speed). ShopBot changes it with TR, Heidenhain with a TOOL CALL
/// that gives only the speed (review 031 finding 6).
#[test]
fn a_merged_toolpath_changes_spindle_speed_between_its_parts_through_every_post() {
    let (kernel, scratch) = two_rate_pockets("mergespeed");
    let mut job = Job {
        name: "mergespeed",
        kernel,
        depths: BTreeMap::new(),
        _scratch: scratch,
    };
    let merged = visible_ids(&job.kernel);
    let mut checked = 0;
    for post_id in milling_posts() {
        let text = post(&mut job, post_id, &merged, None).expect("posts");
        let cuts = cuts_right_of(&text, post_units(post_id), 80.0);
        assert!(!cuts.is_empty(), "{post_id}");
        let (feed, speed) = cuts[0];
        assert!(
            (speed - 20000.0).abs() < 1.0,
            "{post_id}: the second part cuts at {speed} rpm, it asks for 20000\n{text}"
        );
        // ShopBot's speeds are two decimals of units per second: 1.57 in/s
        // is 2392.7 mm/min, within 1 %.
        assert!(
            (feed - 2400.0).abs() < 24.0,
            "{post_id}: the second part cuts at {feed} mm/min, it asks for 2400\n{text}"
        );
        // The speed changes before the travel to the part, not in the cut.
        let lines: Vec<&str> = text.lines().collect();
        let (speed_word, rapid_word) = if post_id.starts_with("shopbot") {
            ("TR,20000", "J")
        } else if post_id.starts_with("heidenhain") {
            ("TOOL CALL S20000", "FMAX")
        } else {
            ("S20000", "G0")
        };
        let change = lines
            .iter()
            .position(|l| l.contains(speed_word))
            .unwrap_or_else(|| panic!("{post_id}: no speed change\n{text}"));
        let after = &lines[change + 1..];
        assert!(
            after.iter().take(3).any(|l| l.contains(rapid_word)),
            "{post_id}: the speed change is not followed by the rapid to the part\n{text}"
        );
        // Only the speed changes: no tool change comes with it.
        let report = gcode_check::check(&text, &Setup::new(post_units(post_id), 5.0));
        assert_eq!(report.tool_changes, vec![1], "{post_id}\n{text}");
        checked += 1;
    }
    assert_eq!(checked, 26);
}

/// A merged toolpath split into tiles keeps each part's feeds and speed on
/// the moves that are left (review 031: the tiles carried the whole
/// toolpath's rate changes at their old move indices).
#[test]
fn a_tiled_merged_toolpath_keeps_each_parts_feeds_and_speed() {
    let (kernel, scratch) = two_rate_pockets("tilefeed");
    let mut job = Job {
        name: "tilefeed",
        kernel,
        depths: BTreeMap::new(),
        _scratch: scratch,
    };
    let merged = visible_ids(&job.kernel);
    let path = job._scratch.0.join("tiled");
    let reply = job
        .kernel
        .apply(
            "cam.save",
            &json!({ "toolpathIds": merged, "post": "linuxcnc_mm", "path": path,
                     "tiling": { "method": "feedX", "tileWidth": 80.0, "tileHeight": 0.0,
                                 "overlap": 0.0 },
                     "jobBounds": [0.0, 0.0, 160.0, 120.0] })
            .to_string(),
        )
        .expect("saves");
    let KernelReply::Json(text) = reply else {
        panic!("bytes");
    };
    let value: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(value["ok"], true, "{value}");
    let paths: Vec<String> = value["result"]["paths"]
        .as_array()
        .expect("paths")
        .iter()
        .map(|p| p.as_str().expect("a path").to_string())
        .collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
    // The second tile holds the fast pocket alone, shifted to X 10 to 40.
    let second = std::fs::read_to_string(&paths[1]).expect("reads");
    let cuts = cuts_right_of(&second, Units::Mm, 5.0);
    assert!(!cuts.is_empty());
    assert!(
        cuts.iter()
            .all(|(f, s)| (f - 2400.0).abs() < 1.0 && (s - 20000.0).abs() < 1.0),
        "tile 2 cuts at {:?}, the pocket asks for 2400 mm/min at 20000 rpm\n{second}",
        cuts[0]
    );
    let first = std::fs::read_to_string(&paths[0]).expect("reads");
    let cuts = cuts_right_of(&first, Units::Mm, 5.0);
    assert!(cuts
        .iter()
        .all(|(f, s)| (f - 800.0).abs() < 1.0 && (s - 12000.0).abs() < 1.0));
}

/// A toolpath merged from a merged toolpath keeps the inner parts' feeds
/// and speed (review 031: the inner changes were dropped for the merged
/// toolpath's own rates).
#[test]
fn a_merged_toolpath_merged_again_keeps_its_inner_parts_feeds_and_speed() {
    let (mut kernel, scratch) = two_rate_pockets("mergemerge");
    let c = vector(
        &mut kernel,
        rect(Point::new(30.0, 80.0), Point::new(60.0, 110.0)).expect("rect"),
    );
    let mut slow = end_mill(1, 6.0);
    slow["feedRate"] = json!(800.0);
    slow["spindleSpeed"] = json!(12000.0);
    compute(
        &mut kernel,
        json!({ "strategy": "pocket", "tool": slow, "ids": [c],
                "params": { "cutDepth": 3.0, "passDepth": 3.0, "stepover": 2.4 } }),
    );
    let ids = visible_ids(&kernel);
    run(
        &mut kernel,
        "cam.merge",
        json!({ "ids": ids, "name": "All" }),
    );
    let mut job = Job {
        name: "mergemerge",
        kernel,
        depths: BTreeMap::new(),
        _scratch: scratch,
    };
    let merged = visible_ids(&job.kernel);
    let text = post(&mut job, "linuxcnc_mm", &merged, None).expect("posts");
    let cuts = cuts_right_of(&text, Units::Mm, 80.0);
    assert!(!cuts.is_empty());
    assert!(
        cuts.iter()
            .all(|(f, s)| (f - 2400.0).abs() < 1.0 && (s - 20000.0).abs() < 1.0),
        "the fast pocket cuts at {:?}, it asks for 2400 mm/min at 20000 rpm\n{text}",
        cuts[0]
    );
}

/// The second of two pockets cut with one tool runs at its own feed and
/// speed through every milling post: when the two are merged and differ in
/// feed alone (ShopBot set its move speeds with MS only where the speed
/// changed, so the fast part ran at the slow part's MS), and when they are
/// two toolpaths with no tool change between them, differing in speed or in
/// feed alone.
#[test]
fn the_second_of_two_pockets_with_one_tool_cuts_at_its_own_feed_and_speed() {
    let cases: [(&'static str, f64, bool); 3] = [
        ("feedmerge", 12000.0, true),
        ("speedapart", 20000.0, false),
        ("feedapart", 12000.0, false),
    ];
    for (name, fast_speed, merge) in cases {
        let (kernel, scratch) = two_pockets(name, fast_speed, merge);
        let mut job = Job {
            name,
            kernel,
            depths: BTreeMap::new(),
            _scratch: scratch,
        };
        let ids = visible_ids(&job.kernel);
        let mut checked = 0;
        for post_id in milling_posts() {
            let text = post(&mut job, post_id, &ids, None).expect("posts");
            let cuts = cuts_right_of(&text, post_units(post_id), 80.0);
            assert!(!cuts.is_empty(), "{name} {post_id}");
            for (feed, speed) in &cuts {
                assert!(
                    (speed - fast_speed).abs() < 1.0,
                    "{name} {post_id}: the second pocket cuts at {speed} rpm, it asks for \
                     {fast_speed}\n{text}"
                );
                // ShopBot's speeds are two decimals of units per second.
                assert!(
                    (feed - 2400.0).abs() < 24.0,
                    "{name} {post_id}: the second pocket cuts at {feed} mm/min, it asks for \
                     2400\n{text}"
                );
            }
            let slow = cuts_right_of(&text, post_units(post_id), 0.0);
            assert!(
                slow.iter().any(|(f, _)| (f - 800.0).abs() < 8.0),
                "{name} {post_id}: the first pocket never cuts at 800 mm/min\n{text}"
            );
            let report = gcode_check::check(&text, &Setup::new(post_units(post_id), 5.0));
            assert_eq!(report.tool_changes, vec![1], "{name} {post_id}\n{text}");
            checked += 1;
        }
        assert_eq!(checked, 26);
    }
}
