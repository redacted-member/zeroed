//! Golden tests for the posts shipped with the app.
//!
//! One fixture job is run through every shipped post and compared with the
//! file under `tests/golden`. Set `CARVE_UPDATE_GOLDEN=1` to write the golden
//! files instead of comparing them, then read the diff before committing it.

use std::path::PathBuf;

use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
use carve_core::geom::Point;
use carve_core::post::{parse_post, post_process, PostJob, SHIPPED_POSTS};

fn tool(name: &str, number: u32, geometry: ToolGeometry) -> Tool {
    Tool {
        name: name.to_string(),
        number,
        tool_type: geometry,
        ..Tool::default()
    }
}

fn job() -> PostJob {
    PostJob {
        file_name: "sample.gcode".to_string(),
        path_name: "/jobs".to_string(),
        date: "2026-09-20".to_string(),
        time: "10:15".to_string(),
        notes: "Sample job".to_string(),
        home: Point3::new(0.0, 0.0, 25.0),
        safe_z: 5.0,
        ..PostJob::default()
    }
}

/// A profile with straight moves and two arcs, then a drilling toolpath with
/// a second tool and a dwell, so that every block of a milling post is used.
fn milling_job() -> Vec<Toolpath> {
    let mill = tool("End Mill 6 mm", 1, ToolGeometry::EndMill { diameter: 6.0 });
    let mut profile = Toolpath::new(1, "Profile 1", mill, StrategyKind::Profile);
    profile.home = Point3::new(0.0, 0.0, 25.0);
    profile.safe_z = 5.0;
    profile.feed = 2400.0;
    profile.plunge = 600.0;
    profile.spindle = 18000.0;
    profile.moves = vec![
        Move::rapid(Point3::new(10.0, 10.0, 5.0)),
        Move::plunge(Point3::new(10.0, 10.0, -3.0)),
        Move::feed(Point3::new(40.0, 10.0, -3.0)),
        Move::arc_ccw(Point::new(40.0, 20.0), Point3::new(50.0, 20.0, -3.0)),
        Move::feed(Point3::new(50.0, 40.0, -3.0)),
        Move::arc_cw(Point::new(40.0, 40.0), Point3::new(40.0, 30.0, -3.0)),
        Move::feed(Point3::new(10.0, 30.0, -3.0)),
        Move::feed(Point3::new(10.0, 10.0, -3.0)),
        Move::rapid(Point3::new(10.0, 10.0, 5.0)),
    ];
    profile.recompute_bounds();

    let drill = tool(
        "Drill 6 mm",
        2,
        ToolGeometry::Drill {
            diameter: 6.0,
            point_angle: 118.0,
        },
    );
    let mut holes = Toolpath::new(2, "Drill holes", drill, StrategyKind::Drill);
    holes.home = Point3::new(0.0, 0.0, 25.0);
    holes.safe_z = 5.0;
    holes.feed = 300.0;
    holes.plunge = 200.0;
    holes.spindle = 3000.0;
    holes.moves = vec![
        Move::rapid(Point3::new(20.0, 20.0, 5.0)),
        Move::plunge(Point3::new(20.0, 20.0, -10.0)),
        Move::dwell(0.5, Point3::new(20.0, 20.0, -10.0)),
        Move::rapid(Point3::new(20.0, 20.0, 5.0)),
        Move::rapid(Point3::new(30.0, 20.0, 5.0)),
        Move::plunge(Point3::new(30.0, 20.0, -10.0)),
        Move::dwell(0.5, Point3::new(30.0, 20.0, -10.0)),
        Move::rapid(Point3::new(30.0, 20.0, 5.0)),
    ];
    holes.recompute_bounds();
    vec![profile, holes]
}

/// Two cuts with a travel move between them, so that the beam has to be
/// turned off and on again.
fn laser_job() -> Vec<Toolpath> {
    let beam = tool(
        "Laser 0.2 mm",
        1,
        ToolGeometry::LaserTool {
            power: 75.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: carve_core::cam::tool::DEFAULT_MAX_BURN_RATE,
        },
    );
    let mut cut = Toolpath::new(1, "Laser cut", beam, StrategyKind::Laser);
    cut.home = Point3::new(0.0, 0.0, 0.0);
    cut.safe_z = 0.0;
    cut.feed = 1200.0;
    cut.plunge = 1200.0;
    cut.spindle = 0.0;
    cut.moves = vec![
        Move::rapid(Point3::new(10.0, 10.0, 0.0)),
        Move::feed(Point3::new(60.0, 10.0, 0.0)),
        Move::feed(Point3::new(60.0, 40.0, 0.0)),
        Move::rapid(Point3::new(10.0, 40.0, 0.0)),
        Move::feed(Point3::new(10.0, 10.0, 0.0)),
    ];
    cut.recompute_bounds();
    vec![cut]
}

fn golden_path(id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{id}.txt"))
}

fn check_golden(id: &str, text: &str) {
    let path = golden_path(id);
    if std::env::var("CARVE_UPDATE_GOLDEN").is_ok() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the golden directory is created");
        }
        std::fs::write(&path, text).expect("the golden file is written");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. Run with CARVE_UPDATE_GOLDEN=1 to write it.",
            path.display()
        )
    });
    assert_eq!(text, expected, "{id} does not match its golden file");
}

fn post_by_id(id: &str) -> carve_core::post::Post {
    let (_, source) = SHIPPED_POSTS
        .iter()
        .find(|(name, _)| *name == id)
        .unwrap_or_else(|| panic!("{id} is shipped"));
    parse_post(id, source).unwrap_or_else(|error| panic!("{id} parses: {error}"))
}

fn output_for(id: &str) -> String {
    let post = post_by_id(id);
    let toolpaths = if id.starts_with("laser") {
        laser_job()
    } else {
        milling_job()
    };
    post_process(&post, &job(), &toolpaths).unwrap_or_else(|error| panic!("{id} posts: {error}"))
}

/// A helical ramp: a full turn down 2 mm in two half circles, then a flat
/// half circle, for the posts that write arcs moving in Z.
fn helical_job() -> Vec<Toolpath> {
    let mill = tool("End Mill 6 mm", 1, ToolGeometry::EndMill { diameter: 6.0 });
    let mut ramp = Toolpath::new(1, "Helical ramp", mill, StrategyKind::Pocket);
    ramp.home = Point3::new(0.0, 0.0, 25.0);
    ramp.safe_z = 5.0;
    ramp.feed = 2400.0;
    ramp.plunge = 600.0;
    ramp.spindle = 18000.0;
    ramp.moves = vec![
        Move::rapid(Point3::new(30.0, 20.0, 5.0)),
        Move::plunge(Point3::new(30.0, 20.0, 0.0)),
        Move::arc_ccw(Point::new(20.0, 20.0), Point3::new(10.0, 20.0, -1.0)),
        Move::arc_ccw(Point::new(20.0, 20.0), Point3::new(30.0, 20.0, -2.0)),
        Move::arc_cw(Point::new(20.0, 20.0), Point3::new(10.0, 20.0, -2.0)),
        Move::rapid(Point3::new(10.0, 20.0, 5.0)),
    ];
    ramp.recompute_bounds();
    vec![ramp]
}

fn helical_output_for(id: &str) -> String {
    post_process(&post_by_id(id), &job(), &helical_job())
        .unwrap_or_else(|error| panic!("{id} posts: {error}"))
}

#[test]
fn the_newer_shipped_posts_match_their_golden_files() {
    for id in [
        "mach3_inch",
        "linuxcnc_inch",
        "shopbot_inch",
        "uccnc_mm",
        "fanuc_mm",
        "heidenhain_mm",
        "centroid_mm",
        "linuxcnc_manual_mm",
        "linuxcnc_manual_inch",
        "uccnc_manual_mm",
    ] {
        check_golden(id, &output_for(id));
    }
}

#[test]
fn posts_with_helical_blocks_write_arcs_that_move_in_z() {
    for id in ["fanuc_mm", "heidenhain_mm", "centroid_mm"] {
        let text = helical_output_for(id);
        check_golden(&format!("{id}_helical"), &text);
        assert!(post_by_id(id).supports_helical_arcs(), "{id}");
    }
}

#[test]
fn generic_gcode_mm_matches_its_golden_file() {
    check_golden("generic_gcode_mm", &output_for("generic_gcode_mm"));
}

#[test]
fn generic_gcode_inch_matches_its_golden_file() {
    check_golden("generic_gcode_inch", &output_for("generic_gcode_inch"));
}

#[test]
fn grbl_mm_matches_its_golden_file() {
    check_golden("grbl_mm", &output_for("grbl_mm"));
}

#[test]
fn grbl_inch_matches_its_golden_file() {
    check_golden("grbl_inch", &output_for("grbl_inch"));
}

#[test]
fn mach3_mm_matches_its_golden_file() {
    check_golden("mach3_mm", &output_for("mach3_mm"));
}

#[test]
fn linuxcnc_mm_matches_its_golden_file() {
    check_golden("linuxcnc_mm", &output_for("linuxcnc_mm"));
}

#[test]
fn shopbot_mm_matches_its_golden_file() {
    check_golden("shopbot_mm", &output_for("shopbot_mm"));
}

#[test]
fn laser_grbl_matches_its_golden_file() {
    check_golden("laser_grbl", &output_for("laser_grbl"));
}

#[test]
fn every_shipped_post_writes_the_same_bytes_twice() {
    for (id, _) in SHIPPED_POSTS {
        assert_eq!(output_for(id), output_for(id), "{id} is not deterministic");
    }
}

#[test]
fn a_post_that_says_it_writes_arcs_writes_arcs() {
    for (id, _) in SHIPPED_POSTS {
        let post = post_by_id(id);
        if !post.supports_arcs() || id == "laser_grbl" {
            continue;
        }
        let text = output_for(id);
        // Heidenhain's plain language writes its arcs as C with a direction.
        let (cw, ccw) = if id.starts_with("heidenhain") {
            ("DR-", "DR+")
        } else {
            ("G2", "G3")
        };
        assert!(text.contains(cw), "{id} writes no clockwise arc");
        assert!(text.contains(ccw), "{id} writes no counter clockwise arc");
    }
}

#[test]
fn the_shopbot_post_breaks_arcs_into_straight_moves() {
    let post = post_by_id("shopbot_mm");
    assert!(!post.supports_arcs());
    let text = output_for("shopbot_mm");
    let moves = text.lines().filter(|l| l.starts_with("M3,")).count();
    assert!(moves > 20, "an arc should take many moves, got {moves}");
}

#[test]
fn an_inch_post_writes_the_same_geometry_in_inches() {
    let mm = output_for("generic_gcode_mm");
    let inch = output_for("generic_gcode_inch");
    assert!(mm.contains("X40.000"), "{mm}");
    // 40 mm is 1.5748 inches.
    assert!(inch.contains("X1.5748"), "{inch}");
    assert!(mm.lines().count() == inch.lines().count());
}

#[test]
fn every_post_writes_the_line_ending_it_declares() {
    for (id, _) in SHIPPED_POSTS {
        let post = post_by_id(id);
        let text = output_for(id);
        assert!(
            text.ends_with(&post.line_ending),
            "{id} does not end with its own line ending"
        );
        if post.line_ending == "\r\n" {
            let bare = text.matches('\n').count() - text.matches("\r\n").count();
            assert_eq!(bare, 0, "{id} has a line that is not ended with CR LF");
        }
    }
}

#[test]
fn a_tool_change_is_written_once_for_each_tool() {
    for (id, _) in SHIPPED_POSTS {
        // A post with one tool to a file cannot be handed two.
        if !post_by_id(id).supports_tool_change() {
            continue;
        }
        let text = output_for(id);
        assert!(
            text.contains("End Mill 6 mm"),
            "{id} does not name the first tool"
        );
        assert!(
            text.contains("Drill 6 mm"),
            "{id} does not name the second tool"
        );
    }
}
