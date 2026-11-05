//! Whole projects built through the app as a user would build them, saved
//! under `docs/testing/projects` (see `docs/testing/wfA.md`), replayed
//! through the kernel's command interface: every toolpath recalculated,
//! posted through the GRBL, Mach3, LinuxCNC, UCCNC and generic posts, and the
//! posted G-code read back and swept through the preview stock, so what the
//! machine would cut is checked against the design.

use carve_core::cam::commands::CamState;
use carve_core::cam::gcode_import;
use carve_core::cam::sim::Stock;
use carve_core::cam::tooldb::ToolStore;
use carve_core::cam::toolpath::{MoveKind, Toolpath};
use carve_core::doc::JobSetup;
use carve_core::geom::{Path, Point, Region, Tolerance};
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("carve-workflow-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creates the scratch folder");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn project(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/testing/projects");
    dir.join(name).to_string_lossy().into_owned()
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

fn tol() -> Tolerance {
    Tolerance::new(0.01).expect("tolerance")
}

/// Opens a project and recalculates every toolpath with today's kernel;
/// answers the toolpath ids in list order.
fn open_and_recalculate(kernel: &mut Kernel, file: &str) -> Vec<u64> {
    run(kernel, "doc.open", json!({ "path": project(file) }));
    let list = run(kernel, "cam.list", json!({}));
    let ids: Vec<u64> = list
        .as_array()
        .expect("list")
        .iter()
        .map(|t| t["id"].as_u64().expect("id"))
        .collect();
    for &id in &ids {
        run(kernel, "cam.recalculate", json!({ "id": id }));
        let after = run(kernel, "cam.get", json!({ "id": id }));
        assert_eq!(after["stale"], false, "{after}");
    }
    ids
}

/// Posts the toolpaths into one file and answers its text.
fn post(kernel: &mut Kernel, scratch: &Scratch, ids: &[u64], post: &str) -> String {
    let path = scratch.0.join(format!("out-{post}.txt"));
    run(
        kernel,
        "cam.save",
        json!({
            "toolpathIds": ids,
            "post": post,
            "path": path.to_string_lossy(),
            "output": "oneFile",
        }),
    );
    std::fs::read_to_string(&path).expect("posted file")
}

/// The controller families the brief names and the two the shop cuts on
/// (LinuxCNC and UCCNC, changing tools by hand with an M6 that probes the
/// tool length), in millimetres.
const POSTS: [&str; 5] = [
    "grbl_mm",
    "mach3_mm",
    "linuxcnc_mm",
    "uccnc_mm",
    "generic_gcode_mm",
];

/// The end of a saved program's file name for each post.
fn saved_name(post: &str) -> &'static str {
    match post {
        "grbl_mm" => "grbl.gcode",
        "grbl_inch" => "grbl-inch.gcode",
        "mach3_mm" => "mach3.tap",
        "linuxcnc_mm" => "linuxcnc.ngc",
        "uccnc_mm" => "uccnc.tap",
        "laser_grbl" => "laser.gcode",
        "grbl_rotary_a_mm" => "grbl-rotary.gcode",
        "mach3_rotary_mm" => "mach3-rotary.tap",
        "linuxcnc_rotary_mm" => "linuxcnc-rotary.ngc",
        "uccnc_rotary_mm" => "uccnc-rotary.tap",
        _ => "generic.gcode",
    }
}

/// With `CARVE_KEEP_PROGRAMS` set, writes a posted program over its saved
/// copy under `docs/testing/projects` (after a post is changed on purpose;
/// the G-code check in `gcode_check.rs` then reads the saved copies).
fn keep(project_name: &str, post: &str, text: &str) {
    if std::env::var_os("CARVE_KEEP_PROGRAMS").is_some() {
        let name = format!("{project_name}-{}", saved_name(post));
        // The program names the file it was posted to: the saved copy's name.
        let text = text.replacen(&format!("(out-{post}.txt)"), &format!("({name})"), 1);
        std::fs::write(project(&name), text).expect("writes the program");
    }
}

/// Every word of the program, line by line, without comments or line numbers.
fn words(text: &str) -> Vec<Vec<(char, f64)>> {
    text.lines()
        .map(|line| {
            let mut code = String::new();
            let mut depth = 0;
            for c in line.chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ';' if depth == 0 => break,
                    _ if depth == 0 => code.push(c),
                    _ => {}
                }
            }
            let chars: Vec<char> = code.chars().filter(|c| !c.is_whitespace()).collect();
            let mut out = Vec::new();
            let mut i = 0;
            while i < chars.len() {
                let letter = chars[i].to_ascii_uppercase();
                i += 1;
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || "+-.".contains(chars[i])) {
                    i += 1;
                }
                let number: String = chars[start..i].iter().collect();
                if letter.is_ascii_alphabetic() && letter != 'N' {
                    if let Ok(value) = number.parse::<f64>() {
                        out.push((letter, value));
                    }
                }
            }
            out
        })
        .collect()
}

/// The machine's safety across the program: the spindle is stopped at every
/// tool change and turning at every cut, and every cut has a feed rate.
fn check_program(text: &str, post: &str) {
    let mut spindle = false;
    let mut feed = false;
    let mut motion = 0;
    let mut changes = 0;
    // After a tool change (M6, or the pause a manual change post writes) the
    // new cutter's length is unknown until the operator has touched it off:
    // its first move lifts Z on its own before anything moves across.
    let mut changed = false;
    for (n, line) in words(text).iter().enumerate() {
        let moves = line.iter().any(|&(l, _)| "XYZ".contains(l));
        if changed && moves {
            let across = line.iter().any(|&(l, _)| "XY".contains(l));
            assert!(
                !across,
                "{post} line {}: moves across before Z lifts after a tool change",
                n + 1
            );
            changed = false;
        }
        for &(letter, value) in line {
            match (letter, value as i64) {
                ('M', 3) | ('M', 4) => spindle = true,
                ('M', 5) | ('M', 2) | ('M', 30) => spindle = false,
                ('M', 0) | ('M', 1) => changed = true,
                ('M', 6) => {
                    changed = true;
                    changes += 1;
                    assert!(
                        !spindle,
                        "{post} line {}: tool change with the spindle on",
                        n + 1
                    );
                }
                ('G', g @ 0..=3) => motion = g,
                ('F', _) => feed = true,
                _ => {}
            }
        }
        if moves && motion != 0 {
            assert!(
                spindle,
                "{post} line {}: cutting with the spindle off",
                n + 1
            );
            assert!(feed, "{post} line {}: cutting with no feed rate", n + 1);
        }
    }
    if !post.starts_with("grbl") {
        assert!(changes > 0, "{post}: no tool change written");
    }
}

/// The posted program as toolpaths, one per tool, in job millimetres.
/// Moves that go nowhere (a footer repeating the home position) are left out,
/// and so is the lift straight up a manual tool change post writes before
/// the spindle starts; a lift before the first tool change, which cuts
/// nothing, is not a toolpath.
fn read_back(text: &str, post: &str) -> Vec<Toolpath> {
    let mut read = gcode_import::read(text, post).expect("reads the posted G-code");
    read.toolpaths
        .retain(|tp| tp.moves.iter().any(|m| m.kind.is_cutting()));
    for tp in &mut read.toolpaths {
        let lifts = tp
            .moves
            .iter()
            .take_while(|m| matches!(m.kind, MoveKind::Rapid) && m.to.x == 0.0 && m.to.y == 0.0)
            .count();
        tp.moves
            .drain(..lifts.min(tp.moves.len().saturating_sub(1)));
        let mut at: Option<carve_core::cam::toolpath::Point3> = None;
        tp.moves.retain(|m| {
            let keep = at.is_none_or(|p| p.distance(m.to) > 1e-6);
            at = Some(m.to);
            keep
        });
    }
    read.toolpaths
}

/// Rapids never travel sideways below the material surface.
fn check_rapids(toolpaths: &[Toolpath], post: &str) {
    for tp in toolpaths {
        for (from, one) in tp.iter_moves() {
            if matches!(one.kind, MoveKind::Rapid) {
                let sideways = from.xy().distance(one.to.xy()) > 1e-6;
                assert!(
                    !(sideways && from.z.min(one.to.z) < 0.5),
                    "{post}: rapid from {from:?} to {:?} in the material",
                    one.to
                );
            }
        }
    }
}

fn cutting_depth(tp: &Toolpath) -> f64 {
    tp.moves
        .iter()
        .filter(|m| m.kind.is_cutting())
        .map(|m| m.to.z)
        .fold(f64::MAX, f64::min)
}

fn text_region(kernel: &mut Kernel, id: u64) -> Region {
    let text = run(kernel, "entities.get", json!({ "id": id }));
    let paths: Vec<Path> = serde_json::from_value(text["paths"].clone()).expect("text outlines");
    Region::from_paths(&paths, tol())
}

/// The house sign (project 1): "THE MILLERS" in Georgia 30 mm, V-grooved with
/// a 60 degree V-bit to a 3 mm flat depth with a 3 mm end mill as the
/// clearance tool (too big for any flat, so the V clears them), the inner
/// border of a clipart frame engraved 2 mm deep on the line with the V-bit,
/// and the outer border cut out through the 19 mm board with a 6 mm end
/// mill, four tabs 10 mm long and 3 mm thick.
#[test]
fn the_house_sign_cuts_what_was_designed() {
    let scratch = Scratch::new("house-sign");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "house-sign.carve");
    assert_eq!(ids.len(), 3);
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    let text = text_region(&mut kernel, 1);

    let mut first: Option<Vec<Toolpath>> = None;
    let inch = post(&mut kernel, &scratch, &ids, "grbl_inch");
    check_program(&inch, "grbl_inch");
    keep("house-sign", "grbl_inch", &inch);
    for name in POSTS {
        let program = post(&mut kernel, &scratch, &ids, name);
        check_program(&program, name);
        keep("house-sign", name, &program);
        let toolpaths = read_back(&program, name);
        check_rapids(&toolpaths, name);
        // One tool for the V-Groove and the border, then the end mill.
        let numbers: Vec<u32> = toolpaths.iter().map(|t| t.tool.number).collect();
        assert_eq!(numbers, vec![7, 2], "{name}");
        assert!((cutting_depth(&toolpaths[0]) + 3.0).abs() < 2e-3, "{name}");
        assert!((cutting_depth(&toolpaths[1]) + 19.0).abs() < 2e-3, "{name}");
        // Every post writes the same moves.
        match &first {
            None => first = Some(toolpaths),
            Some(reference) => {
                for (a, b) in reference.iter().zip(&toolpaths) {
                    assert_eq!(a.moves.len(), b.moves.len(), "{name}");
                    for (m, n) in a.moves.iter().zip(&b.moves) {
                        assert!(m.to.distance(n.to) < 2e-3, "{name}: {m:?} and {n:?}");
                    }
                }
            }
        }
    }

    // Sweep what the machine would cut and compare it with the design.
    let toolpaths = first.expect("posted");
    let mut stock = Stock::with_cells(&job, 800_000.0);
    for (i, tp) in toolpaths.iter().enumerate() {
        stock.cut(tp, i as u16 + 1).expect("sweeps");
    }
    let height = |x: f64, y: f64| f64::from(stock.height_at(x, y).expect("on the stock"));
    // The letters: flat bottomed at 3 mm wherever the V would go deeper
    // (1.73 mm in from the edge), V walls rising to the edge elsewhere. The
    // V clears the flats in passes 1.39 mm apart (40% of its 3.46 mm width
    // there), which leaves ridges up to 0.69 / tan 30 = 1.2 mm between them.
    let flats = text.offset(3.0 * 30f64.to_radians().tan() + 0.3, tol());
    let walls = text.offset(0.5, tol());
    let outside = text.offset(-0.5, tol());
    let (mut flat_points, mut on_floor, mut wall_points) = (0, 0, 0);
    for i in 0..600 {
        for j in 0..70 {
            let p = Point::new(54.0 + i as f64 * 0.5, 92.0 + j as f64 * 0.5);
            let z = height(p.x, p.y);
            if flats.contains(p, tol()) {
                flat_points += 1;
                assert!(z > -3.05 && z < -3.0 + 1.25, "floor at {p:?} is at {z}");
                if z < -2.9 {
                    on_floor += 1;
                }
            } else if walls.contains(p, tol()) {
                wall_points += 1;
                assert!(z < -0.3 && z > -3.05, "wall at {p:?} is at {z}");
            } else if !outside.contains(p, tol()) {
                assert!(z > -0.05, "cut outside the letters at {p:?}: {z}");
            }
        }
    }
    assert!(
        on_floor * 2 > flat_points,
        "{on_floor} of {flat_points} on the floor"
    );
    assert!(
        flat_points > 200 && wall_points > 200,
        "{flat_points} {wall_points}"
    );
    // The engraved border line at 2 mm, the sign's face untouched inside.
    assert!((height(200.0, 28.0) + 2.0).abs() < 0.1);
    assert!((height(32.8, 100.0) + 2.0).abs() < 0.1);
    assert!(height(200.0, 60.0) > -0.01);
    // The cut-out goes through except at the tabs, 3 mm thick; the waste
    // outside the kerf is left.
    let mut tab_cells = 0;
    let mut through = 0;
    for k in 0..=280 {
        let x = 60.0 + k as f64;
        let z = height(x, 7.0);
        if (z + 16.0).abs() < 0.1 {
            tab_cells += 1;
        } else {
            assert!(z < -18.9, "kerf at x {x} is at {z}");
            through += 1;
        }
    }
    assert!((3..=6).contains(&tab_cells), "tab {tab_cells} mm");
    assert!(through > 250);
    assert!(height(200.0, 2.0) > -0.01);
}

/// The relief plaque (project 2): an ellipse 180 x 130 mm made a round
/// shape (45 degrees, limited to 8 mm), the Five Petal Rosette clipart
/// relief added in the middle, a stroke of the sculpting Deposit brush, then
/// 3D Roughing (6 mm end mill, Z level, 1 mm allowance, model boundary) and
/// 3D Finishing (offset rings, 3 mm ball nose, then the 1.5 mm ball nose
/// resting the rosette), in 25 mm board.
///
/// Run in release (`cargo test --release -p carve-core --test
/// workflow_projects`): in a debug build the offsetter's own checks trip on
/// the traced model outline and the finishing takes minutes.
#[test]
#[cfg_attr(debug_assertions, ignore = "release only: slow in a debug build")]
fn the_relief_plaque_cuts_what_was_modelled() {
    let scratch = Scratch::new("relief");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "relief-plaque.carve");
    assert_eq!(ids.len(), 3);
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    let doc =
        carve_core::io::native::open(std::path::Path::new(&project("relief-plaque.carve")), 1)
            .expect("opens");
    let model = doc.composite();
    let zero = doc.model_zero(&model);
    let model_z = |x: f64, y: f64| {
        let h = model.at(Point::new(x, y));
        zero + if h.is_nan() { 0.0 } else { f64::from(h) }
    };
    let inside = |x: f64, y: f64, margin: f64| {
        ((x - 100.0) / (90.0 - margin)).powi(2) + ((y - 75.0) / (65.0 - margin)).powi(2) < 1.0
    };

    let mut first: Option<Vec<Toolpath>> = None;
    for name in POSTS {
        let program = post(&mut kernel, &scratch, &ids, name);
        check_program(&program, name);
        // Only the GRBL program is kept: each is 3 to 4 MB.
        if name == "grbl_mm" {
            keep("relief-plaque", name, &program);
        }
        let toolpaths = read_back(&program, name);
        check_rapids(&toolpaths, name);
        let numbers: Vec<u32> = toolpaths.iter().map(|t| t.tool.number).collect();
        assert_eq!(numbers, vec![2, 5, 4], "{name}");
        // No finishing move leaves the model's outline for the flat around
        // it (offset rings once ran round the model's box, 12 mm down in the
        // uncut corners).
        for tp in &toolpaths[1..] {
            for m in tp.moves.iter().filter(|m| m.kind.is_cutting()) {
                assert!(
                    inside(m.to.x, m.to.y, -0.5),
                    "{name} T{}: {m:?} off the model",
                    tp.tool.number
                );
            }
        }
        if first.is_none() {
            first = Some(toolpaths);
        }
    }

    let toolpaths = first.expect("posted");
    let mut stock = Stock::with_cells(&job, 1_000_000.0);
    for (i, tp) in toolpaths.iter().enumerate() {
        stock.cut(tp, i as u16 + 1).expect("sweeps");
    }
    let height = |x: f64, y: f64| f64::from(stock.height_at(x, y).expect("on the stock"));
    let (mut checked, mut worst) = (0, 0.0f64);
    for i in 0..90 {
        for j in 0..65 {
            let (x, y) = (11.0 + i as f64 * 2.0, 11.0 + j as f64 * 2.0);
            let z = height(x, y);
            // The model's lowest and highest points within a stock cell and
            // a model cell, as the stock samples the cut a cell at a time and
            // the toolpaths follow the model a cell at a time.
            let c = stock.cell + model.cell;
            let near: Vec<f64> = [(0.0, 0.0), (c, 0.0), (-c, 0.0), (0.0, c), (0.0, -c)]
                .iter()
                .map(|(dx, dy)| model_z(x + dx, y + dy))
                .collect();
            let low = near.iter().copied().fold(f64::MAX, f64::min);
            let high = near.iter().copied().fold(f64::MIN, f64::max);
            if inside(x, y, 0.0) {
                // Never below the model, give or take half the rise across
                // those cells on a steep wall.
                let slack = 0.05 + 0.5 * (high - low);
                assert!(z > low - slack, "gouge at {x}, {y}: {z} below {low}");
            }
            if inside(x, y, 4.0) {
                checked += 1;
                worst = worst.max(z - high);
                assert!(z < high + 0.3, "left at {x}, {y}: {z} over {high}");
            } else if !inside(x, y, -3.5) {
                // The corners outside the model's outline stay uncut.
                assert!(z > -0.01, "cut outside the model at {x}, {y}: {z}");
            }
        }
    }
    assert!(checked > 3000, "{checked}");
    assert!(worst < 0.3, "{worst}");
}

/// The two sided job (project 3): 300 x 200 x 18 mm, datum in the middle,
/// turned over left to right. On top an 80 x 60 mm pocket 6 mm deep
/// (3 mm end mill); on the bottom a 40 mm circle cut out through the board
/// with a 6 mm end mill, which lands inside the pocket once turned over.
/// Each side posts to its own file in its own job space.
#[test]
fn the_two_sided_job_cuts_both_sides_where_they_were_drawn() {
    use carve_core::doc::Side;
    let scratch = Scratch::new("two-sided");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "two-sided.carve");
    assert_eq!(ids.len(), 2);
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    let list = run(&mut kernel, "cam.list", json!({}));
    assert_eq!(list[0]["side"], "top");
    assert_eq!(list[1]["side"], "bottom");
    // Both sides in one file are refused.
    let path = scratch.0.join("mixed.gcode");
    let mixed = kernel.apply(
        "cam.save",
        &json!({ "toolpathIds": ids, "post": "grbl_mm", "path": path, "output": "oneFile" })
            .to_string(),
    );
    assert!(mixed.is_err());

    let mut sides: Option<(Toolpath, Toolpath)> = None;
    for name in POSTS {
        let top = post(&mut kernel, &scratch, &ids[..1], name);
        let bottom = post(&mut kernel, &scratch, &ids[1..], name);
        check_program(&top, name);
        check_program(&bottom, name);
        keep("two-sided-top", name, &top);
        keep("two-sided-bottom", name, &bottom);
        let top = read_back(&top, name).remove(0);
        let mut bottom = read_back(&bottom, name).remove(0);
        check_rapids(std::slice::from_ref(&top), name);
        check_rapids(std::slice::from_ref(&bottom), name);
        assert!((cutting_depth(&top) + 6.0).abs() < 2e-3, "{name}");
        assert!((cutting_depth(&bottom) + 18.0).abs() < 2e-3, "{name}");
        // Each side in its own space: the pocket at X -120..-40 seen from the
        // top, the circle round X 80 seen from the bottom.
        for m in top.moves.iter().filter(|m| m.kind.is_cutting()) {
            assert!(m.to.x > -120.0 && m.to.x < -40.0, "{name}: {m:?}");
        }
        for m in bottom.moves.iter().filter(|m| m.kind.is_cutting()) {
            assert!(
                (Point::new(80.0, 0.0).distance(m.to.xy()) - 17.0).abs() < 0.01,
                "{name}: {m:?}"
            );
        }
        bottom.side = Side::Bottom;
        sides.get_or_insert((top, bottom));
    }

    // Both programs swept through one board, seen from the top.
    let (top, bottom) = sides.expect("posted");
    let mut stock = Stock::with_cells(&job, 600_000.0);
    stock
        .cut_for(&job, Side::Top, &top, 1)
        .expect("sweeps the top");
    stock
        .cut_for(&job, Side::Top, &bottom, 2)
        .expect("sweeps the bottom");
    let height = |x: f64, y: f64| f64::from(stock.height_at(x, y).expect("on the stock"));
    // The pocket floor, the hole through it (the bottom's X 80 is the top's
    // X -80), and the board untouched elsewhere.
    assert!((height(-110.0, 20.0) + 6.0).abs() < 0.05);
    assert!((height(-50.0, -25.0) + 6.0).abs() < 0.05);
    assert!(
        stock.is_through(-80.0 + 20.0 - 1.5, 0.0),
        "the ring cut at the hole's edge"
    );
    assert!(
        !stock.is_through(80.0 - 20.0 + 1.5, 0.0),
        "nothing at the unturned place"
    );
    assert!(height(80.0, 0.0) > -0.01);
    assert!(height(0.0, 60.0) > -0.01);
}

/// The nested sheet (project 4): a 180 x 110 mm plate with 10 mm corners,
/// a 120 mm square and a 100 mm disc, five copies of each nested (6 mm tool,
/// 1 mm clearance, 10 mm border) onto 600 x 400 x 12 mm sheets: ten parts on
/// sheet 1, the five discs on sheet 2. Each sheet is cut out through with a
/// 6 mm end mill and two tabs a part, posted to its own file; sheet 1 is
/// also tiled for a machine 350 mm wide.
#[test]
fn the_nested_sheets_cut_their_own_parts() {
    let scratch = Scratch::new("nested");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "nested-sheet.carve");
    assert_eq!(ids.len(), 2);
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    let list = run(&mut kernel, "cam.list", json!({}));
    let sheet_of = |i: usize| list[i]["sheet"].as_u64().expect("sheet");
    assert_eq!((sheet_of(0), sheet_of(1)), (2, 1));

    // Both sheets in one file are refused: the second board's parts would
    // be cut into the first.
    let mixed = kernel.apply(
        "cam.save",
        &json!({
            "toolpathIds": ids,
            "post": "grbl_mm",
            "path": scratch.0.join("mixed.gcode"),
            "output": "oneFile",
        })
        .to_string(),
    );
    assert!(mixed.is_err());
    // To multiple files, grouped: still one file a sheet.
    let grouped = run(
        &mut kernel,
        "cam.save",
        json!({
            "toolpathIds": ids,
            "post": "grbl_mm",
            "path": scratch.0.join("grouped.gcode"),
            "output": "multipleFiles",
            "group": true,
        }),
    );
    assert_eq!(
        grouped["paths"].as_array().map(Vec::len),
        Some(2),
        "{grouped}"
    );

    for (index, &id) in ids.iter().enumerate() {
        let sheet = sheet_of(index);
        let parts: Vec<[f64; 4]> = run(&mut kernel, "entities.list", json!({ "sheet": sheet }))
            .as_array()
            .expect("parts")
            .iter()
            .map(|e| serde_json::from_value(e["bbox"].clone()).expect("bbox"))
            .collect();
        assert_eq!(parts.len(), if sheet == 1 { 10 } else { 5 });
        for name in POSTS {
            let program = post(&mut kernel, &scratch, &[id], name);
            check_program(&program, name);
            if ["grbl_mm", "linuxcnc_mm", "uccnc_mm"].contains(&name) {
                keep(&format!("nested-sheet-sheet{sheet}"), name, &program);
            }
            let toolpaths = read_back(&program, name);
            check_rapids(&toolpaths, name);
            assert_eq!(toolpaths.len(), 1, "{name}");
            assert!((cutting_depth(&toolpaths[0]) + 12.0).abs() < 2e-3, "{name}");

            let mut stock = Stock::with_cells(&job, 600_000.0);
            stock.cut(&toolpaths[0], 1).expect("sweeps");
            let height = |x: f64, y: f64| f64::from(stock.height_at(x, y).expect("on the stock"));
            for b in &parts {
                let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
                // The part itself is left whole, the kerf round it goes
                // through or leaves a 3 mm tab.
                assert!(
                    height(cx, cy) > -0.01,
                    "{name} sheet {sheet}: part at {cx}, {cy}"
                );
                assert!(height(b[0] + 1.0, cy) > -0.01, "{name}: part edge {b:?}");
                for (x, y) in [
                    (b[0] - 3.0, cy),
                    (b[2] + 3.0, cy),
                    (cx, b[1] - 3.0),
                    (cx, b[3] + 3.0),
                ] {
                    let z = height(x, y);
                    assert!(
                        z < -11.9 || (z + 9.0).abs() < 0.1,
                        "{name} sheet {sheet}: kerf at {x}, {y} is at {z}"
                    );
                }
            }
        }
    }

    // Sheet 1 tiled for a 350 mm machine: the tiles, each moved back to its
    // place, cut what the whole sheet's program cuts.
    let whole = read_back(
        &post(&mut kernel, &scratch, &ids[1..], "linuxcnc_mm"),
        "linuxcnc_mm",
    );
    let mut whole_stock = Stock::with_cells(&job, 600_000.0);
    whole_stock.cut(&whole[0], 1).expect("sweeps");
    for name in ["linuxcnc_mm", "uccnc_mm"] {
        let reply = run(
            &mut kernel,
            "cam.save",
            json!({
                "toolpathIds": [ids[1]],
                "post": name,
                "path": scratch.0.join(format!("tiled-{name}.txt")),
                "output": "oneFile",
                "tiling": {
                    "method": "individual",
                    "tileWidth": 350.0,
                    "tileHeight": 400.0,
                    "overlap": 3.0,
                    "smallestFirst": false,
                },
                "jobBounds": [0.0, 0.0, 600.0, 400.0],
            }),
        );
        let tiles = reply["tiles"].as_array().expect("tiles");
        assert_eq!(tiles.len(), 2, "{reply}");
        let mut stock = Stock::with_cells(&job, 600_000.0);
        for (i, tile) in tiles.iter().enumerate() {
            let text = std::fs::read_to_string(tile["path"].as_str().expect("path"))
                .expect("tile program");
            check_program(&text, name);
            let shift = tile["shift"][0].as_f64().expect("shift");
            for mut tp in read_back(&text, name) {
                check_rapids(std::slice::from_ref(&tp), name);
                // Nothing cut beyond the tile and its overlap.
                for m in tp.moves.iter().filter(|m| m.kind.is_cutting()) {
                    assert!(
                        m.to.x > -3.001 && m.to.x < 350.001,
                        "{name} tile {i}: {m:?}"
                    );
                }
                for m in &mut tp.moves {
                    m.to.x -= shift;
                    if let MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } = &mut m.kind {
                        centre.x -= shift;
                    }
                }
                stock.cut(&tp, i as u16 + 1).expect("sweeps");
            }
        }
        let (mut worst, mut compared, mut at) = (0.0f64, 0, (0.0, 0.0, 0.0, 0.0));
        for i in 0..1200 {
            for j in 0..800 {
                let (x, y) = (0.25 + i as f64 * 0.5, 0.25 + j as f64 * 0.5);
                if let (Some(a), Some(b)) = (stock.height_at(x, y), whole_stock.height_at(x, y)) {
                    if f64::from(a - b).abs() > worst {
                        worst = f64::from(a - b).abs();
                        at = (x, y, a, b);
                    }
                    compared += 1;
                }
            }
        }
        assert!(compared > 900_000, "{compared}");
        assert!(
            worst < 0.05,
            "{name}: tiles differ from the whole sheet by {worst} at {at:?} (tiled, whole)"
        );
    }
}

/// The finger jointed box (project 1 of `docs/testing/wfB.md`): five panels
/// of 12 mm stock made into a box by Box from Selection with a press fit
/// (0.1 mm a side), a test coupon, all nested 1 mm apart and cut out by one
/// outside profile 12.2 mm deep with a 1/4 inch end mill and two tabs 3 mm
/// thick per part. Reopened, the parts go together in the Assembly View
/// with every joint at its clearance, and the posted programs cut through
/// all round but the tabs.
#[test]
fn the_box_goes_together_and_cuts_what_was_designed() {
    let scratch = Scratch::new("box-joints");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "box-joints.carve");
    assert_eq!(ids.len(), 1);

    // Seven pieces (the waste in the finger notches is not one), every
    // joint mate solved, and Verify Fit reads the fit to 0.01 mm.
    let pieces = run(&mut kernel, "assembly.pieces", json!({}));
    assert_eq!(
        pieces["pieces"].as_array().map(Vec::len),
        Some(7),
        "{pieces}"
    );
    let mates = pieces["mates"].as_array().expect("mates");
    assert_eq!(mates.len(), 9);
    assert!(mates.iter().all(|m| m["state"] == "solved"), "{mates:?}");
    let fit = run(
        &mut kernel,
        "assembly.verifyFit",
        json!({ "target": 0.1, "tolerance": 0.05 }),
    );
    let joints = fit["joints"].as_array().expect("joints");
    assert_eq!(joints.len(), 15, "{fit}");
    let mut sides = 0;
    for joint in joints {
        assert_eq!(joint["status"], "ok", "{joint}");
        for side in joint["sides"].as_array().expect("sides") {
            if side["fit"] == true {
                sides += 1;
                for end in ["min", "max"] {
                    let gap = side[end].as_f64().expect("gap");
                    assert!((gap - 0.1).abs() < 0.01, "{joint}");
                }
            }
        }
    }
    assert_eq!(sides, 30);

    let mut first: Option<Vec<Toolpath>> = None;
    for name in POSTS {
        let program = post(&mut kernel, &scratch, &ids, name);
        check_program(&program, name);
        keep("box-joints", name, &program);
        let toolpaths = read_back(&program, name);
        check_rapids(&toolpaths, name);
        let numbers: Vec<u32> = toolpaths.iter().map(|t| t.tool.number).collect();
        assert_eq!(numbers, vec![14], "{name}");
        assert!((cutting_depth(&toolpaths[0]) + 12.2).abs() < 2e-3, "{name}");
        // The tabs, 3 mm above the bottom of the cut: the last pass climbs
        // over 14 of them (two a part).
        let over_tabs = toolpaths[0]
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 9.2).abs() < 2e-3)
            .count();
        assert!(over_tabs >= 14, "{name}: {over_tabs} moves over tabs");
        match &first {
            None => first = Some(toolpaths),
            Some(reference) => {
                for (a, b) in reference.iter().zip(&toolpaths) {
                    assert_eq!(a.moves.len(), b.moves.len(), "{name}");
                    for (m, n) in a.moves.iter().zip(&b.moves) {
                        assert!(m.to.distance(n.to) < 2e-3, "{name}: {m:?} and {n:?}");
                    }
                }
            }
        }
    }
}

/// A knock-down table (see `docs/testing/wfB.md`, project 2): a top, two
/// legs drawn alike, a stretcher and a shelf, 18 mm, knock-down fit (0.25
/// mm a side). Each leg's two tabs stand in slots 30 mm in from the top's
/// end; the stretcher's wedged tenons go through both legs, a wedge in
/// each; the shelf sits in a 6 mm dado in each leg 60 mm up from its foot.
/// The joints made their toolpaths, the holes and outlines then cut 18.3
/// deep with two tabs 10 x 3 mm a part. Reopened, the table goes together
/// with its legs facing each other, every joint at its clearance, and the
/// posted programs cut what the preview shows.
#[test]
fn the_knock_down_table_goes_together_and_cuts_what_was_designed() {
    let scratch = Scratch::new("knockdown-table");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "knockdown-table.carve");
    assert_eq!(ids.len(), 3);

    // Top, two legs, stretcher, shelf and two wedges; every mate solved.
    let pieces = run(&mut kernel, "assembly.pieces", json!({}));
    assert_eq!(
        pieces["pieces"].as_array().map(Vec::len),
        Some(7),
        "{pieces}"
    );
    let mates = pieces["mates"].as_array().expect("mates");
    assert_eq!(mates.len(), 8);
    assert!(mates.iter().all(|m| m["state"] == "solved"), "{mates:?}");
    let fit = run(
        &mut kernel,
        "assembly.verifyFit",
        json!({ "target": 0.25, "tolerance": 0.05 }),
    );
    let joints = fit["joints"].as_array().expect("joints");
    assert_eq!(joints.len(), 14, "{fit}");
    let mut sides = 0;
    for joint in joints {
        assert_eq!(joint["status"], "ok", "{joint}");
        for side in joint["sides"].as_array().expect("sides") {
            if side["fit"] == true {
                sides += 1;
                for end in ["min", "max"] {
                    let gap = side[end].as_f64().expect("gap");
                    assert!((gap - 0.25).abs() < 0.01, "{joint}");
                }
            }
        }
    }
    assert!(sides >= 30, "{sides} fit sides");

    // The cut list: the stretcher drawn standing up reads 345.65 by 80.
    let list = run(&mut kernel, "assembly.pieceCutList", json!({}));
    let rows = list["rows"].as_array().expect("rows");
    assert!(
        rows.iter()
            .all(|r| r["length"].as_f64() >= r["width"].as_f64()),
        "{list}"
    );

    let mut first: Option<Vec<Toolpath>> = None;
    for name in POSTS {
        let program = post(&mut kernel, &scratch, &ids, name);
        check_program(&program, name);
        keep("knockdown-table", name, &program);
        let toolpaths = read_back(&program, name);
        check_rapids(&toolpaths, name);
        let numbers: Vec<u32> = toolpaths.iter().map(|t| t.tool.number).collect();
        assert_eq!(numbers, vec![14], "{name}");
        assert!((cutting_depth(&toolpaths[0]) + 18.3).abs() < 2e-3, "{name}");
        // Fourteen tabs 3 mm thick: the last pass climbs over them.
        let over_tabs = toolpaths[0]
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 15.3).abs() < 2e-3)
            .count();
        assert!(over_tabs >= 14, "{name}: {over_tabs} moves over tabs");
        match &first {
            None => first = Some(toolpaths),
            Some(reference) => {
                for (a, b) in reference.iter().zip(&toolpaths) {
                    assert_eq!(a.moves.len(), b.moves.len(), "{name}");
                    for (m, n) in a.moves.iter().zip(&b.moves) {
                        assert!(m.to.distance(n.to) < 2e-3, "{name}: {m:?} and {n:?}");
                    }
                }
            }
        }
    }
}

/// The program's burning moves between two comment lines (a toolpath's
/// name and the next one's), as (from, to, power S, feed) in millimetres.
fn burns_between(
    report: &gcode_check::Report,
    text: &str,
    start: &str,
    end: Option<&str>,
) -> Vec<([f64; 2], [f64; 2], f64, f64)> {
    let line_of = |name: &str| {
        text.lines()
            .position(|l| l.trim() == name)
            .map(|i| i + 1)
            .unwrap_or_else(|| panic!("no {name} in the program"))
    };
    let (first, last) = (line_of(start), end.map_or(usize::MAX, line_of));
    report
        .motions
        .iter()
        .filter(|m| m.line > first && m.line < last && !m.rapid && m.spindle && m.speed > 0.0)
        .map(|m| ([m.from[0], m.from[1]], [m.to[0], m.to[1]], m.speed, m.feed))
        .collect()
}

fn burnt_length(burns: &[([f64; 2], [f64; 2], f64, f64)]) -> f64 {
    burns
        .iter()
        .map(|(a, b, _, _)| (b[0] - a[0]).hypot(b[1] - a[1]))
        .sum()
}

/// The laser coaster (project 3): a 300 x 200 x 3 mm board. A 60 mm circle
/// (moved 5 mm left after the first calculation, then recalculated) is cut
/// and filled: cross hatched at 45 degrees every 0.2 mm with its outline, at
/// 60 % and 1500 mm/min. A 120 x 80 pixel gray bitmap placed at 150..240 x
/// 70..130 is burnt as a Laser Picture in raster lines 0.3 mm apart at
/// 3000 mm/min, 5 % overscan, black at the tool's 80 %. The 200 x 120 outline
/// is cut out outside the 0.1 mm kerf at full power and 300 mm/min, three
/// repeats. Posted through the GRBL laser post, the program burns exactly
/// that, and the preview chars the board where it does.
#[test]
fn the_laser_coaster_burns_what_was_designed() {
    let scratch = Scratch::new("laser-coaster");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "laser-coaster.carve");
    assert_eq!(ids.len(), 3);
    let list = run(&mut kernel, "cam.list", json!({}));
    let strategies: Vec<&str> = list
        .as_array()
        .expect("list")
        .iter()
        .map(|t| t["strategy"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(strategies, ["laserCutFill", "laserPicture", "laserCutFill"]);

    let program = post(&mut kernel, &scratch, &ids, "laser_grbl");
    keep("laser-coaster", "laser_grbl", &program);
    let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, 0.0);
    setup.laser = true;
    let report = gcode_check::check(&program, &setup);
    assert!(
        report.at_least(gcode_check::Severity::Warning).is_empty(),
        "{}",
        report.summary()
    );
    // The beam is off for every travel and at the end.
    assert!(program.trim_end().ends_with("M30"), "no end code");

    // The circle: every burn inside it, at 60 % and 1500 mm/min, about the
    // two hatches over its area and its outline.
    let circle = burns_between(
        &report,
        &program,
        "(Laser Cut 1)",
        Some("(Laser Picture 1)"),
    );
    for (a, b, s, f) in &circle {
        for p in [a, b] {
            assert!(
                (p[0] - 95.0).hypot(p[1] - 100.0) < 30.0 + 1e-3,
                "{p:?} outside the circle"
            );
        }
        assert_eq!((*s, *f), (600.0, 1500.0));
    }
    let area = std::f64::consts::PI * 30.0 * 30.0;
    let expected = 2.0 * area / 0.2 + std::f64::consts::TAU * 30.0;
    let burnt = burnt_length(&circle);
    assert!(
        (burnt - expected).abs() < 0.03 * expected,
        "{burnt} mm, about {expected}"
    );

    // The picture: inside the bitmap at 3000 mm/min, black at 80 %, the
    // gray ramp fading from black on the left to white on the right, the
    // black disc (pixel 90, 40 of 120 x 80) at full power.
    let picture = burns_between(
        &report,
        &program,
        "(Laser Picture 1)",
        Some("(Laser Cut 2)"),
    );
    assert!(!picture.is_empty());
    for (a, b, s, f) in &picture {
        for p in [a, b] {
            assert!(
                (150.0 - 1e-3..=240.0 + 1e-3).contains(&p[0]) && (70.0..=130.0).contains(&p[1]),
                "{p:?} outside the bitmap"
            );
        }
        assert!(*s <= 800.0 && *f == 3000.0, "S{s} F{f}");
    }
    let power_at = |x: f64, y: f64| -> f64 {
        picture
            .iter()
            .filter(|(a, b, _, _)| (a[1] - y).abs() < 0.2 && (b[1] - y).abs() < 0.2)
            .find(|(a, b, _, _)| a[0].min(b[0]) <= x && x <= a[0].max(b[0]))
            .map_or(0.0, |burn| burn.2)
    };
    assert_eq!(power_at(217.5, 100.0), 800.0, "the black disc");
    assert!(
        power_at(151.0, 80.0) >= 790.0,
        "black at the left of the ramp"
    );
    assert!(
        power_at(239.0, 80.0) <= 20.0,
        "white at the right of the ramp"
    );
    let ramp: Vec<f64> = (0..9)
        .map(|k| power_at(155.0 + 10.0 * f64::from(k), 80.0))
        .collect();
    assert!(ramp.windows(2).all(|w| w[1] <= w[0]), "{ramp:?}");

    // The cut out: half the 0.1 mm kerf outside the outline, full power,
    // 300 mm/min, three times round.
    let cut = burns_between(&report, &program, "(Laser Cut 2)", None);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for (a, b, s, f) in &cut {
        assert_eq!((*s, *f), (1000.0, 300.0));
        for p in [a, b] {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    for (got, want) in [(lo, [49.95, 39.95]), (hi, [250.05, 160.05])] {
        for k in 0..2 {
            assert!((got[k] - want[k]).abs() < 2e-3, "{lo:?} {hi:?}");
        }
    }
    let round = 2.0 * (200.1 + 120.1);
    assert!(
        (burnt_length(&cut) - 3.0 * round).abs() < 1.0,
        "{}",
        burnt_length(&cut)
    );

    // The preview chars the board where the beam burns, nowhere else, and
    // cuts nothing.
    let doc = kernel.document();
    let mut stock = carve_core::cam::sim::Stock::new(doc.job(), Default::default());
    let top = stock.z.clone();
    for (n, t) in doc.toolpaths().iter().enumerate() {
        stock
            .cut(t, u16::try_from(n + 1).unwrap_or(0))
            .expect("previews");
    }
    assert_eq!(stock.z, top, "a laser removes no material");
    let char_at = |x: f64, y: f64| {
        let i = ((y - stock.origin_y) / stock.cell).round() as usize * stock.width
            + ((x - stock.origin_x) / stock.cell).round() as usize;
        stock.burn[i]
    };
    assert_eq!(char_at(95.0, 100.0), 255, "the filled circle");
    assert_eq!(
        char_at(217.5, 100.0),
        204,
        "the disc at 80 % and 3000 mm/min"
    );
    assert_eq!(char_at(20.0, 20.0), 0, "outside every burn");
    assert_eq!(
        char_at(150.0, 150.0),
        0,
        "inside the cut out, clear of the burns"
    );
    assert_eq!(char_at(250.05, 100.0), 255, "the cut out line");
}

/// The rotary sleeve again with the job changed the ways a user changes it,
/// from the document through `cam.save` to the file: Z zero on the surface
/// writes depths under zero; a square blank's rapids clear its corners; a
/// blank along Y through a LinuxCNC post turns A and holds X on the axis line.
#[test]
fn the_rotary_sleeve_posts_through_every_job_setting() {
    use carve_core::doc::{RotaryAxis, RotaryBlank, RotaryZZero};
    let scratch = Scratch::new("rotary-settings");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "rotary-sleeve.carve");
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    let recalculate = |kernel: &mut Kernel, job: &JobSetup| {
        run(kernel, "job.set", serde_json::to_value(job).expect("job"));
        for &id in &ids {
            run(kernel, "cam.recalculate", json!({ "id": id }));
        }
    };
    let check = |program: &str, across: char, safe: f64, diameter: f64| {
        let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, safe);
        setup.rotary = Some(gcode_check::Rotary {
            axis: 'A',
            across,
            diameter,
            linear: false,
            reversed: false,
        });
        let report = gcode_check::check(program, &setup);
        assert!(
            report.at_least(gcode_check::Severity::Error).is_empty(),
            "{}\n{program}",
            report.summary()
        );
        report
    };

    // Z zero on the surface: the pocket 3 deep and the ring 2 deep are under
    // zero, and the clearance is 5 over it.
    let mut surface = job;
    surface.rotary.z_zero = RotaryZZero::Surface;
    recalculate(&mut kernel, &surface);
    let program = post(&mut kernel, &scratch, &ids, "linuxcnc_rotary_mm");
    let report = check(&program, 'Y', 5.0, 60.0);
    let lowest = report.lowest_by_tool();
    assert!((lowest[&Some(2)] + 3.0).abs() < 1e-3, "{lowest:?}");
    assert!((lowest[&Some(1)] + 2.0).abs() < 1e-3, "{lowest:?}");

    // A square blank 60 on a side, Z zero on the axis: its corners reach
    // 42.426 from the axis, so every turn happens 5 above that.
    let mut square = job;
    square.rotary.blank = RotaryBlank::Square;
    recalculate(&mut kernel, &square);
    let program = post(&mut kernel, &scratch, &ids, "linuxcnc_rotary_mm");
    let clear = 30.0 * std::f64::consts::SQRT_2 + 5.0;
    let report = check(&program, 'Y', clear - 1e-3, 60.0);
    let turning: Vec<_> = report
        .motions
        .iter()
        .filter(|m| m.rapid && (m.to[1] - m.from[1]).abs() > 1e-6)
        .collect();
    assert!(!turning.is_empty());
    for m in &turning {
        assert!(
            m.from[2] >= clear - 1e-3 && m.to[2] >= clear - 1e-3,
            "a turn below the corners: {m:?}"
        );
    }
    // The cuts stay where the job asks, measured from the round surface.
    let lowest = report.lowest_by_tool();
    assert!((lowest[&Some(2)] - 27.0).abs() < 1e-3, "{lowest:?}");

    // A blank along Y: the same posts turn A, X stays on the axis line.
    let mut along_y = job;
    along_y.rotary.axis = RotaryAxis::Y;
    recalculate(&mut kernel, &along_y);
    for name in ["linuxcnc_rotary_mm", "uccnc_rotary_mm"] {
        let program = post(&mut kernel, &scratch, &ids, name);
        assert!(program.contains('A'), "{name}\n{program}");
        assert!(!program.contains("B0"), "{name}\n{program}");
        check(&program, 'X', 35.0, 60.0);
    }
}

/// The rotary sleeve (project 4): a cylinder 60 mm across and 200 long
/// turning about X on the A axis, Z zero on its axis, unwrapped to 200 by
/// the 188.5 mm round. A 60 x 40 mm pocket 3 deep with a 6 mm end mill
/// halfway along, on the far side (74.25..114.25 round, A 142 to 218
/// degrees), and a ring groove 2 deep with a 3 mm end mill 20 mm from the
/// end, along a line all the way round. Posted through both rotary posts
/// the programs wrap round the blank in inverse time and cut what the
/// flat preview shows: the pocket and the ring where they were drawn, each
/// at its depth below the surface (radius 30), each cut at its toolpath's
/// surface feed.
#[test]
fn the_rotary_sleeve_wraps_round_its_blank_and_cuts_what_was_designed() {
    let scratch = Scratch::new("rotary-sleeve");
    let mut kernel = kernel(&scratch);
    let ids = open_and_recalculate(&mut kernel, "rotary-sleeve.carve");
    assert_eq!(ids.len(), 2);
    let job: JobSetup =
        serde_json::from_value(run(&mut kernel, "job.get", json!({}))).expect("job");
    assert!(job.is_rotary());
    assert_eq!(job.rotary.diameter, 60.0);
    let round = std::f64::consts::PI * 60.0;
    let toolpaths: Vec<Toolpath> = kernel.document().toolpaths().into_iter().cloned().collect();

    for name in [
        "grbl_rotary_a_mm",
        "mach3_rotary_mm",
        "linuxcnc_rotary_mm",
        "uccnc_rotary_mm",
    ] {
        let program = post(&mut kernel, &scratch, &ids, name);
        keep("rotary-sleeve", name, &program);
        check_program(&program, name);
        // Inverse time for the wrapped cuts, units per minute again at the end.
        let head: String = program.lines().take(8).collect::<Vec<_>>().join("\n");
        assert!(head.contains("G93"), "{name}: {head}");
        assert!(
            program.lines().rev().take(4).any(|l| l.contains("G94")),
            "{name}"
        );
        let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, 35.0);
        setup.rotary = Some(gcode_check::Rotary {
            axis: 'A',
            across: 'Y',
            diameter: 60.0,
            linear: false,
            reversed: false,
        });
        let report = gcode_check::check(&program, &setup);
        assert!(
            report.at_least(gcode_check::Severity::Error).is_empty(),
            "{name}: {}",
            report.summary()
        );
        // Each tool down to its depth below the 30 mm surface.
        let lowest = report.lowest_by_tool();
        assert!((lowest[&Some(2)] - 27.0).abs() < 2e-3, "{name}: {lowest:?}");
        assert!((lowest[&Some(1)] - 28.0).abs() < 2e-3, "{name}: {lowest:?}");
        // Unwrapped, the pocket's cuts lie in the rectangle less the tool's
        // radius and the ring's along X = 20 all the way round.
        let cuts: Vec<_> = report.motions.iter().filter(|m| !m.rapid).collect();
        let mut ring = 0.0;
        for m in &cuts {
            match m.tool {
                Some(2) if m.to[2] < 29.99 => {
                    assert!(
                        (73.0 - 1e-3..=127.0 + 1e-3).contains(&m.to[0])
                            && (77.25 - 1e-2..=111.25 + 1e-2).contains(&m.to[1]),
                        "{name}: pocket cut at {:?}",
                        m.to
                    );
                }
                Some(1) if m.to[2] < 29.99 => {
                    assert!((m.to[0] - 20.0).abs() < 1e-3, "{name}: {:?}", m.to);
                    ring += (m.to[1] - m.from[1]).abs();
                }
                _ => {}
            }
        }
        // Two passes round, 1 and 2 deep.
        assert!(
            (ring - 2.0 * round).abs() < 0.5,
            "{name}: {ring} of {round}"
        );
        // Every cut at its toolpath's feed or plunge rate at the surface.
        for m in &cuts {
            let t = toolpaths
                .iter()
                .find(|t| Some(t.tool_number) == m.tool)
                .expect("a toolpath for the tool");
            let fits = [t.feed, t.plunge]
                .iter()
                .any(|f| (m.feed - f).abs() <= 0.01 * f);
            assert!(fits, "{name}: line {} at {} mm/min", m.line, m.feed);
        }
    }
}
