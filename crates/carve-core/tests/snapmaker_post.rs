//! Exact output of the Snapmaker 2.0 posts, against the firmware's G-code
//! reference: what it lists is written, and what it does not is left out.

use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
use carve_core::geom::Point;
use carve_core::post::{parse_post, post_process, Post, PostJob, SHIPPED_POSTS};

fn post_of(id: &str) -> Post {
    let (_, source) = SHIPPED_POSTS
        .iter()
        .find(|(name, _)| *name == id)
        .unwrap_or_else(|| panic!("{id} is shipped"));
    parse_post(id, source).unwrap_or_else(|error| panic!("{id} parses: {error}"))
}

fn job() -> PostJob {
    PostJob {
        file_name: "sign.cnc".to_string(),
        home: Point3::new(0.0, 0.0, 20.0),
        safe_z: 5.0,
        ..PostJob::default()
    }
}

/// A pocket corner: down 2 mm, along, round a quarter arc, a dwell, then home.
fn cut(spindle: f64) -> Vec<Toolpath> {
    let mill = Tool {
        name: "End Mill 1/8 in".to_string(),
        number: 1,
        tool_type: ToolGeometry::EndMill { diameter: 3.175 },
        ..Tool::default()
    };
    let mut tp = Toolpath::new(1, "Corner", mill, StrategyKind::Profile);
    tp.home = Point3::new(0.0, 0.0, 20.0);
    tp.safe_z = 5.0;
    tp.feed = 1000.0;
    tp.plunge = 300.0;
    tp.spindle = spindle;
    tp.moves = vec![
        Move::rapid(Point3::new(10.0, 10.0, 5.0)),
        Move::plunge(Point3::new(10.0, 10.0, -2.0)),
        Move::feed(Point3::new(40.0, 10.0, -2.0)),
        Move::arc_ccw(Point::new(40.0, 20.0), Point3::new(50.0, 20.0, -2.0)),
        Move::dwell(1.5, Point3::new(50.0, 20.0, -2.0)),
        Move::rapid(Point3::new(50.0, 20.0, 5.0)),
        Move::rapid(Point3::new(50.0, 20.0, 20.0)),
        Move::rapid(Point3::new(0.0, 0.0, 20.0)),
    ];
    tp.recompute_bounds();
    vec![tp]
}

fn program(id: &str, spindle: f64) -> String {
    post_process(&post_of(id), &job(), &cut(spindle)).expect("posts")
}

#[test]
fn the_200w_post_writes_the_words_the_reference_lists() {
    let text = program("snapmaker_cnc_200w_mm", 12000.0);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        &lines[..12],
        [
            "; sign.cnc",
            "; Written by Zeroed",
            "; T1 = End Mill 1/8 in",
            "G90",
            "G21",
            "G0 Z20.000 F3000",
            "M3 S12000",
            "; Corner",
            "G0 X10.000 Y10.000 F3000",
            "G0 Z5.000 F3000",
            "G1 Z-2.000 F300.0",
            "G1 X40.000 F1000.0",
        ],
        "{text}"
    );
    assert!(lines.contains(&"G4 S1.50"), "{text}");
    assert_eq!(
        &lines[lines.len() - 4..],
        ["M5", "G0 Z20.000 F3000", "G0 X0.000 Y0.000 F3000", "M400"],
        "{text}"
    );
}

#[test]
fn nothing_the_reference_leaves_out_or_warns_of_is_written() {
    for id in [
        "snapmaker_cnc_200w_mm",
        "snapmaker_cnc_50w_mm",
        "laser_snapmaker_mm",
    ] {
        let text = program(id, 12000.0);
        for line in text.lines().filter(|l| !l.starts_with(';')) {
            let first = line.split_whitespace().next().unwrap_or_default();
            assert!(
                ["G0", "G1", "G4", "G20", "G21", "G90", "M3", "M4", "M5", "M400"].contains(&first),
                "{id}: {line:?} is not a verified word\n{text}"
            );
            assert!(!line.contains('('), "{id}: {line:?}\n{text}");
        }
    }
}

#[test]
fn every_rapid_carries_its_rate_and_every_run_of_cuts_its_feed() {
    let text = program("snapmaker_cnc_200w_mm", 12000.0);
    let moves: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("G0 ") || l.starts_with("G1 "))
        .collect();
    let mut last = "";
    for line in &moves {
        let word = line.split_whitespace().next().unwrap_or_default();
        if word == "G0" {
            assert!(line.ends_with(" F3000"), "{line}\n{text}");
        } else if last != "G1" {
            assert!(line.contains(" F"), "{line}\n{text}");
        }
        last = word;
    }
    // The arc goes out as straight moves at the feed set before it.
    assert!(moves.iter().any(|l| l.starts_with("G1 X50.000 Y20.000")));
}

#[test]
fn the_spindle_word_follows_the_toolhead() {
    let word = |id: &str, rpm: f64| {
        program(id, rpm)
            .lines()
            .find(|l| l.starts_with("M3"))
            .unwrap_or_default()
            .to_string()
    };
    let (big, small) = ("snapmaker_cnc_200w_mm", "snapmaker_cnc_50w_mm");
    assert_eq!(word(big, 12000.0), "M3 S12000");
    assert_eq!(word(big, 24000.0), "M3 S18000");
    assert_eq!(word(big, 5000.0), "M3 S8000");
    assert_eq!(word(small, 12000.0), "M3 P100");
    assert_eq!(word(small, 9000.0), "M3 P75");
    assert_eq!(word(small, 6000.0), "M3 P50");
    assert_eq!(word(small, 24000.0), "M3 P100");
}

#[test]
fn the_tool_is_lifted_before_anything_crosses_or_starts() {
    for id in ["snapmaker_cnc_200w_mm", "snapmaker_cnc_50w_mm"] {
        let text = program(id, 12000.0);
        let lines: Vec<&str> = text.lines().collect();
        let lift = lines
            .iter()
            .position(|l| l.starts_with("G0 Z20.000"))
            .unwrap_or_else(|| panic!("{id}: no lift\n{text}"));
        let spindle = lines
            .iter()
            .position(|l| l.starts_with("M3"))
            .unwrap_or_else(|| panic!("{id}: no spindle\n{text}"));
        let across = lines
            .iter()
            .position(|l| l.starts_with("G0 X"))
            .unwrap_or_else(|| panic!("{id}: no rapid\n{text}"));
        assert!(lift < spindle && spindle < across, "{id}\n{text}");
    }
}

#[test]
fn a_file_holds_one_tool_because_the_firmware_has_no_m6() {
    for id in [
        "snapmaker_cnc_200w_mm",
        "snapmaker_cnc_50w_mm",
        "laser_snapmaker_mm",
    ] {
        assert!(!post_of(id).summary().tool_change, "{id}");
    }
}

#[test]
fn the_laser_post_writes_m4_with_power_from_0_to_255_and_no_z() {
    let post = post_of("laser_snapmaker_mm");
    assert!(post.summary().jet);
    let beam = Tool {
        name: "Laser".to_string(),
        number: 1,
        tool_type: ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: 3000.0,
        },
        ..Tool::default()
    };
    let mut tp = Toolpath::new(1, "Burn", beam, StrategyKind::Profile);
    tp.feed = 1200.0;
    tp.moves = vec![
        Move::rapid(Point3::new(5.0, 5.0, 0.0)),
        Move::feed(Point3::new(25.0, 5.0, 0.0)),
        Move::rapid(Point3::new(25.0, 15.0, 0.0)),
    ];
    tp.recompute_bounds();
    let text = post_process(&post, &job(), &[tp]).expect("posts");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"M4 S204"), "{text}");
    assert!(lines.contains(&"G1 X25.000 F1200.0"), "{text}");
    assert!(lines.contains(&"G0 X5.000 Y5.000 F6000"), "{text}");
    assert!(
        !text.lines().any(|l| !l.starts_with(';') && l.contains('Z')),
        "{text}"
    );
    assert_eq!(lines.last(), Some(&"M400"), "{text}");
    // The beam goes out before the rapid that follows a burn.
    let burn = lines
        .iter()
        .position(|l| l.starts_with("G1 X25"))
        .expect("the burn");
    assert_eq!(lines[burn + 1], "M5", "{text}");
}
