//! Golden tests for wrapped rotary output.
//!
//! A flat toolpath on the unwrapped surface of a 40 mm blank is written
//! through the shipped rotary posts with the across coordinate wrapped into
//! an angle. Set `CARVE_UPDATE_GOLDEN=1` to write the golden files instead
//! of comparing them.

use std::f64::consts::PI;
use std::path::PathBuf;

use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
use carve_core::doc::{RotaryAxis, RotarySetup};
use carve_core::geom::Point;
use carve_core::post::interp::post_process_wrapped;
use carve_core::post::rotary::Wrap;
use carve_core::post::{parse_post, PostJob, SHIPPED_POSTS};

const DIAMETER: f64 = 40.0;

fn job() -> PostJob {
    PostJob {
        file_name: "rotary.gcode".to_string(),
        path_name: "/jobs".to_string(),
        date: "2026-09-24".to_string(),
        time: "09:00".to_string(),
        notes: "Rotary sample".to_string(),
        home: Point3::new(0.0, 0.0, 40.0),
        safe_z: 25.0,
        ..PostJob::default()
    }
}

/// A groove along the axis, then a band a quarter of the way round, then a
/// quarter arc, all on a blank along X with Z zero on the axis (surface at
/// Z 20).
fn along_x() -> Vec<Toolpath> {
    let quarter = PI * DIAMETER / 4.0;
    let mill = Tool {
        name: "Ball Nose 3 mm".to_string(),
        number: 1,
        tool_type: ToolGeometry::BallNose { diameter: 3.0 },
        ..Tool::default()
    };
    let mut tp = Toolpath::new(1, "Wrapped groove", mill, StrategyKind::Profile);
    tp.home = Point3::new(0.0, 0.0, 40.0);
    tp.safe_z = 25.0;
    tp.feed = 1000.0;
    tp.plunge = 300.0;
    tp.spindle = 16000.0;
    tp.moves = vec![
        Move::rapid(Point3::new(10.0, 0.0, 25.0)),
        Move::plunge(Point3::new(10.0, 0.0, 18.0)),
        Move::feed(Point3::new(60.0, 0.0, 18.0)),
        Move::feed(Point3::new(60.0, quarter, 18.0)),
        Move::arc_ccw(
            Point::new(60.0 - 10.0, quarter),
            Point3::new(50.0, quarter + 10.0, 18.0),
        ),
        Move::feed(Point3::new(10.0, 2.0 * quarter, 18.0)),
        Move::rapid(Point3::new(10.0, 2.0 * quarter, 25.0)),
    ];
    tp.recompute_bounds();
    vec![tp]
}

/// The same groove turned for a blank along Y: flat X wraps into B.
fn along_y() -> Vec<Toolpath> {
    along_x()
        .into_iter()
        .map(|mut tp| {
            for one in &mut tp.moves {
                one.to = Point3::new(one.to.y, one.to.x, one.to.z);
            }
            tp.moves.truncate(4);
            tp.moves
                .push(Move::rapid(Point3::new(PI * DIAMETER / 4.0, 60.0, 25.0)));
            tp.recompute_bounds();
            tp
        })
        .collect()
}

fn post_by_id(id: &str) -> carve_core::post::Post {
    let (_, source) = SHIPPED_POSTS
        .iter()
        .find(|(name, _)| *name == id)
        .unwrap_or_else(|| panic!("{id} is shipped"));
    parse_post(id, source).unwrap_or_else(|error| panic!("{id} parses: {error}"))
}

fn wrapped(id: &str, axis: RotaryAxis, toolpaths: &[Toolpath]) -> String {
    let post = post_by_id(id);
    let rotary = RotarySetup {
        diameter: DIAMETER,
        axis,
        ..RotarySetup::default()
    };
    let wrap = Wrap::for_job(&post, &rotary).unwrap_or_else(|e| panic!("{id} wraps: {e}"));
    post_process_wrapped(&post, &job(), toolpaths, Some(wrap))
        .unwrap_or_else(|e| panic!("{id} posts: {e}"))
}

fn check_golden(name: &str, text: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{name}.txt"));
    if std::env::var("CARVE_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, text).expect("the golden file is written");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. Run with CARVE_UPDATE_GOLDEN=1 to write it.",
            path.display()
        )
    });
    assert_eq!(text, expected, "{name} does not match its golden file");
}

#[test]
fn grbl_rotary_wraps_y_into_a() {
    let text = wrapped("grbl_rotary_a_mm", RotaryAxis::X, &along_x());
    check_golden("grbl_rotary_a_mm_wrapped", &text);
    // A quarter of the circumference is 90 degrees, half is 180.
    assert!(text.contains("A90.000"), "{text}");
    assert!(text.contains("A180.000"), "{text}");
    // Y stays on the axis line and arcs become straight moves.
    let arcs = text
        .lines()
        .filter(|l| l.starts_with("G2X") || l.starts_with("G3X"))
        .count();
    assert_eq!(arcs, 0, "{text}");
    assert!(!text
        .lines()
        .any(|l| l.contains('Y') && !l.contains("Y0.000")));
}

#[test]
fn mach_rotary_wraps_x_into_b() {
    let text = wrapped("mach3_rotary_mm", RotaryAxis::Y, &along_y());
    check_golden("mach3_rotary_mm_wrapped_b", &text);
    assert!(text.contains("B90.000"), "{text}");
    assert!(!text.contains("A9"), "{text}");
}

#[test]
fn a_flat_post_cannot_write_a_rotary_job() {
    let post = post_by_id("grbl_mm");
    assert!(Wrap::for_job(&post, &RotarySetup::default()).is_err());
    let grbl = post_by_id("grbl_rotary_a_mm");
    let along = RotarySetup {
        axis: RotaryAxis::Y,
        ..RotarySetup::default()
    };
    assert!(Wrap::for_job(&grbl, &along).is_err());
}

#[test]
fn z_is_written_as_the_job_measures_it() {
    let text = wrapped("grbl_rotary_a_mm", RotaryAxis::X, &along_x());
    assert!(text.contains("Z18.000"), "{text}");
}

/// The rotary posts for LinuxCNC and UCCNC, with the words that set their
/// units.
const NEW_POSTS: [&str; 6] = [
    "linuxcnc_rotary_mm",
    "linuxcnc_rotary_inch",
    "linuxcnc_manual_rotary_mm",
    "linuxcnc_manual_rotary_inch",
    "uccnc_rotary_mm",
    "uccnc_manual_rotary_mm",
];

/// Lines of a program without their line numbers.
fn bare(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            let line = line.trim();
            match line.strip_prefix('N') {
                Some(rest) => rest
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .to_string(),
                None => line.to_string(),
            }
        })
        .collect()
}

/// True for a G1 line (not G17 or G18).
fn is_cut(line: &str) -> bool {
    line.starts_with("G1") && !line.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
}

/// A ring round the blank: from Y 0 once round at 1000 mm/min, 2 mm deep, X 10.
fn ring() -> Vec<Toolpath> {
    let mill = Tool {
        name: "End Mill 1/8 in".to_string(),
        number: 1,
        tool_type: ToolGeometry::EndMill { diameter: 3.175 },
        ..Tool::default()
    };
    let mut tp = Toolpath::new(1, "Ring", mill, StrategyKind::Profile);
    tp.home = Point3::new(0.0, 0.0, 40.0);
    tp.safe_z = 25.0;
    tp.feed = 1000.0;
    tp.plunge = 300.0;
    tp.spindle = 16000.0;
    tp.moves = vec![
        Move::rapid(Point3::new(10.0, 0.0, 25.0)),
        Move::plunge(Point3::new(10.0, 0.0, 18.0)),
        Move::feed(Point3::new(10.0, PI * DIAMETER, 18.0)),
        Move::rapid(Point3::new(10.0, PI * DIAMETER, 25.0)),
    ];
    tp.recompute_bounds();
    vec![tp]
}

#[test]
fn linuxcnc_rotary_writes_the_exact_a_moves_a_ring_needs() {
    let text = wrapped("linuxcnc_rotary_mm", RotaryAxis::X, &ring());
    let lines = bare(&text);
    let cuts: Vec<&str> = lines
        .iter()
        .filter(|l| is_cut(l))
        .map(String::as_str)
        .collect();
    // 7 mm plunged at 300 mm/min takes 7/300 min. Once round is 125.664 mm,
    // four turns of 90 degrees (MAXIMUM_ROTATION), each 31.416 mm at 1000
    // mm/min, so 31.831 times a minute.
    assert_eq!(
        cuts,
        [
            "G1Z18.0000F42.857",
            "G1A90.0000F31.831",
            "G1A180.0000F31.831",
            "G1A270.0000F31.831",
            "G1A360.0000F31.831",
        ],
        "{text}"
    );
    // The first move lifts, crosses to the start on the axis line, then turns.
    assert!(
        lines.contains(&"G0X10.0000Y0.0000A0.0000".to_string()),
        "{text}"
    );
    assert!(lines.contains(&"G21".to_string()), "{text}");
    check_golden("linuxcnc_rotary_mm_wrapped", &text);
}

#[test]
fn inch_rotary_posts_write_inches_and_leave_degrees_and_rates_alone() {
    for id in ["linuxcnc_rotary_inch", "linuxcnc_manual_rotary_inch"] {
        let text = wrapped(id, RotaryAxis::X, &ring());
        let lines = bare(&text);
        assert!(lines.contains(&"G20".to_string()), "{id}\n{text}");
        // 10 mm is 0.3937 in, and Z 18 mm is 0.7087 in.
        assert!(
            lines.contains(&"G0X0.3937Y0.0000A0.0000".to_string()),
            "{id}\n{text}"
        );
        // Degrees are not lengths and inverse time is one over minutes in
        // any units: the same words as the millimeter post.
        assert!(
            lines.contains(&"G1Z0.7087F42.857".to_string()),
            "{id}\n{text}"
        );
        assert!(
            lines.contains(&"G1A90.0000F31.831".to_string()),
            "{id}\n{text}"
        );
        assert!(
            lines.contains(&"G1A360.0000F31.831".to_string()),
            "{id}\n{text}"
        );
    }
}

#[test]
fn a_blank_along_y_turns_the_a_word_and_stays_on_the_x_axis_line() {
    let quarter = PI * DIAMETER / 4.0;
    let mut paths = ring();
    for tp in &mut paths {
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 30.0, 25.0)),
            Move::plunge(Point3::new(0.0, 30.0, 18.0)),
            Move::feed(Point3::new(quarter, 30.0, 18.0)),
            Move::feed(Point3::new(quarter, 60.0, 18.0)),
            Move::rapid(Point3::new(quarter, 60.0, 25.0)),
        ];
        tp.recompute_bounds();
    }
    for id in NEW_POSTS {
        let text = wrapped(id, RotaryAxis::Y, &paths);
        let lines = bare(&text);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("G1A90.0") && l.contains('F')),
            "{id}\n{text}"
        );
        assert!(!lines.iter().any(|l| l.contains("B")), "{id}\n{text}");
        // X stays on the axis line whatever the flat X was.
        assert!(
            lines
                .iter()
                .filter(|l| l.starts_with("G0") || l.starts_with("G1"))
                .all(|l| !l.contains('X') || l.contains("X0.0")),
            "{id}\n{text}"
        );
        // Flat Y runs along the blank as Y.
        let along = if id.ends_with("inch") {
            "Y2.3622"
        } else {
            "Y60.000"
        };
        assert!(lines.iter().any(|l| l.contains(along)), "{id}\n{text}");
    }
}

#[test]
fn tool_changes_run_in_units_per_minute_and_inverse_time_comes_back() {
    for id in NEW_POSTS.into_iter().chain(["mach3_rotary_mm"]) {
        let text = wrapped(id, RotaryAxis::X, &ring());
        let lines = bare(&text);
        let change = lines
            .iter()
            .position(|l| l.contains("M6") && !l.starts_with('('))
            .unwrap_or_else(|| panic!("{id}: no M6\n{text}"));
        let last_mode = |to: usize| {
            lines[..to]
                .iter()
                .rev()
                .find_map(|l| {
                    if l.contains("G93") {
                        Some("G93")
                    } else if l.contains("G94") {
                        Some("G94")
                    } else {
                        None
                    }
                })
                .unwrap_or("")
        };
        assert_eq!(last_mode(change), "G94", "{id} M6 in inverse time\n{text}");
        let first_cut = lines
            .iter()
            .position(|l| is_cut(l))
            .unwrap_or_else(|| panic!("{id}: no cut\n{text}"));
        assert!(change < first_cut, "{id}\n{text}");
        assert_eq!(last_mode(first_cut), "G93", "{id} cuts in G94\n{text}");
        assert_eq!(last_mode(lines.len()), "G94", "{id} ends in G93\n{text}");
    }
}

#[test]
fn every_new_post_lifts_before_it_turns_or_crosses() {
    for id in NEW_POSTS {
        let text = wrapped(id, RotaryAxis::X, &ring());
        let lines = bare(&text);
        let change = lines
            .iter()
            .position(|l| l.contains("M6") && !l.starts_with('('))
            .unwrap_or_else(|| panic!("{id}: no M6\n{text}"));
        let lift = lines[change..]
            .iter()
            .position(|l| l.starts_with("G0Z"))
            .unwrap_or_else(|| panic!("{id}: no lift\n{text}"));
        let across = lines[change..]
            .iter()
            .position(|l| l.starts_with("G0") && (l.contains('X') || l.contains('A')))
            .unwrap_or_else(|| panic!("{id}: no rapid across\n{text}"));
        assert!(lift < across, "{id} crosses before it lifts\n{text}");
    }
}

#[test]
fn home_at_the_end_stays_on_the_axis_line() {
    let mut paths = ring();
    let mut job = job();
    job.home = Point3::new(5.0, 77.0, 40.0);
    paths[0].home = job.home;
    let post = post_by_id("linuxcnc_rotary_mm");
    let rotary = RotarySetup {
        diameter: DIAMETER,
        axis: RotaryAxis::X,
        ..RotarySetup::default()
    };
    let wrap = Wrap::for_job(&post, &rotary).expect("wraps");
    let text = post_process_wrapped(&post, &job, &paths, Some(wrap)).expect("posts");
    assert!(
        bare(&text).contains(&"G0X5.0000Y0.0000".to_string()),
        "{text}"
    );
}

#[test]
fn a_flat_job_through_a_rotary_post_stays_flat_in_units_per_minute() {
    use carve_core::post::post_process;
    let post = post_by_id("linuxcnc_rotary_mm");
    let text = post_process(&post, &job(), &along_x()).expect("posts");
    let lines = bare(&text);
    assert!(lines.contains(&"G94".to_string()), "{text}");
    assert!(!text.contains("G93"), "{text}");
    assert!(text.contains("F1000.000"), "{text}");
    assert!(
        text.lines().all(|l| !l.contains('A') || !l.contains("A9")),
        "{text}"
    );
}

#[test]
fn a_multiplier_on_the_a_variable_reverses_an_axis_wired_the_other_way() {
    let (_, source) = SHIPPED_POSTS
        .iter()
        .find(|(name, _)| *name == "linuxcnc_rotary_mm")
        .expect("shipped");
    let reversed = source.replace("[A|A|A|1.4]", "[A|A|A|1.4|-1]");
    assert_ne!(*source, reversed);
    let post = parse_post("reversed", &reversed).expect("parses");
    let rotary = RotarySetup {
        diameter: DIAMETER,
        ..RotarySetup::default()
    };
    let wrap = Wrap::for_job(&post, &rotary).expect("wraps");
    let text = post_process_wrapped(&post, &job(), &ring(), Some(wrap)).expect("posts");
    assert!(
        bare(&text).contains(&"G1A-90.0000F31.831".to_string()),
        "{text}"
    );
}

#[test]
fn a_shared_post_wraps_only_the_way_it_says() {
    use carve_core::doc::RotaryAxis as Axis;
    use carve_core::post::rotary::RotaryWord;
    let grbl = post_by_id("grbl_rotary_a_mm");
    assert_eq!(grbl.wrap_word(Axis::X), Some(RotaryWord::A));
    assert_eq!(grbl.wrap_word(Axis::Y), None);
    let mach = post_by_id("mach3_rotary_mm");
    assert_eq!(mach.wrap_word(Axis::X), Some(RotaryWord::A));
    assert_eq!(mach.wrap_word(Axis::Y), Some(RotaryWord::B));
    let linux = post_by_id("linuxcnc_rotary_mm");
    assert_eq!(linux.wrap_word(Axis::X), Some(RotaryWord::A));
    assert_eq!(linux.wrap_word(Axis::Y), Some(RotaryWord::A));
    let summary = linux.summary();
    assert!(summary.rotary);
    assert_eq!(summary.rotary_wrap_x.as_deref(), Some("A"));
}

/// A wrapped ring through a post, on a machine wired as `wiring`.
fn wired(id: &str, wiring: carve_core::post::rotary::RotaryWiring) -> Vec<String> {
    let post = post_by_id(id);
    let rotary = RotarySetup {
        diameter: DIAMETER,
        axis: RotaryAxis::X,
        ..RotarySetup::default()
    };
    let wrap = Wrap::for_job(&post, &rotary)
        .expect("wraps")
        .with_wiring(wiring);
    bare(&post_process_wrapped(&post, &job(), &ring(), Some(wrap)).expect("posts"))
}

#[test]
fn machine_wiring_sets_the_letter_units_and_direction_in_the_file() {
    use carve_core::post::rotary::{RotaryUnits, RotaryWiring, RotaryWord};
    let has = |lines: &[String], line: &str| lines.iter().any(|l| l == line);

    // The default: what a machine wired as A in degrees reads, the same words as no wiring.
    let default = wired("linuxcnc_rotary_mm", RotaryWiring::machine_default());
    assert!(has(&default, "G1A90.0000F31.831"), "{default:?}");
    assert!(has(&default, "G1A360.0000F31.831"), "{default:?}");

    let reversed = wired(
        "linuxcnc_rotary_mm",
        RotaryWiring {
            reversed: true,
            ..RotaryWiring::machine_default()
        },
    );
    assert!(has(&reversed, "G1A-90.0000F31.831"), "{reversed:?}");
    assert!(has(&reversed, "G1A-360.0000F31.831"), "{reversed:?}");

    // On B: the post has only an A token, and the file names B alone.
    let on_b = wired(
        "linuxcnc_rotary_mm",
        RotaryWiring {
            letter: Some(RotaryWord::B),
            ..RotaryWiring::machine_default()
        },
    );
    assert!(has(&on_b, "G1B90.0000F31.831"), "{on_b:?}");
    assert!(has(&on_b, "G0X10.0000Y0.0000B0.0000"), "{on_b:?}");
    assert!(
        on_b.iter()
            .filter(|l| !l.starts_with('('))
            .all(|l| !l.contains('A')),
        "{on_b:?}"
    );

    // A post with both tokens names the axis once when the letter moves.
    let mach = wired(
        "mach3_rotary_mm",
        RotaryWiring {
            letter: Some(RotaryWord::B),
            ..RotaryWiring::machine_default()
        },
    );
    assert!(has(&mach, "G0X10.000Y0.000B0.000"), "{mach:?}");
    assert!(
        mach.iter()
            .filter(|l| !l.starts_with('('))
            .all(|l| !l.contains('A')),
        "{mach:?}"
    );

    // Linear: a quarter of 125.664 mm, in the post's own units, and the rate
    // is one over minutes whatever the word holds.
    let linear = RotaryWiring {
        units: RotaryUnits::Linear,
        ..RotaryWiring::machine_default()
    };
    let mm = wired("linuxcnc_rotary_mm", linear);
    assert!(has(&mm, "G1A31.4159F31.831"), "{mm:?}");
    assert!(has(&mm, "G1A125.6637F31.831"), "{mm:?}");
    let inch = wired("linuxcnc_rotary_inch", linear);
    assert!(has(&inch, "G1A1.2368F31.831"), "{inch:?}");
    assert!(has(&inch, "G1A4.9474F31.831"), "{inch:?}");
}

#[test]
fn every_wiring_reads_back_onto_the_flat_design() {
    use carve_core::post::rotary::{RotaryUnits, RotaryWiring, RotaryWord};
    let quarter = PI * DIAMETER / 4.0;
    for (letter, axis, units, linear, reversed) in [
        (RotaryWord::A, 'A', RotaryUnits::Degrees, false, false),
        (RotaryWord::A, 'A', RotaryUnits::Degrees, false, true),
        (RotaryWord::B, 'B', RotaryUnits::Degrees, false, false),
        (RotaryWord::A, 'A', RotaryUnits::Linear, true, false),
        (RotaryWord::B, 'B', RotaryUnits::Linear, true, true),
    ] {
        let post = post_by_id("linuxcnc_rotary_mm");
        let rotary = RotarySetup {
            diameter: DIAMETER,
            axis: RotaryAxis::X,
            ..RotarySetup::default()
        };
        let wrap = Wrap::for_job(&post, &rotary)
            .expect("wraps")
            .with_wiring(RotaryWiring {
                letter: Some(letter),
                units,
                reversed,
            });
        let text = post_process_wrapped(&post, &job(), &ring(), Some(wrap)).expect("posts");
        let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, 25.0);
        setup.rotary = Some(gcode_check::Rotary {
            axis,
            across: 'Y',
            diameter: DIAMETER,
            linear,
            reversed,
        });
        let report = gcode_check::check(&text, &setup);
        assert!(
            report.at_least(gcode_check::Severity::Error).is_empty(),
            "{axis} {units:?} {reversed}: {}\n{text}",
            report.summary()
        );
        // The first quarter ends a quarter of the way round the flat design.
        let reach = report
            .motions
            .iter()
            .filter(|m| !m.rapid)
            .map(|m| m.to[1])
            .fold(f64::MIN, f64::max);
        assert!(
            (reach - 4.0 * quarter).abs() < 1e-2,
            "{axis} {units:?} {reversed}: reaches {reach}\n{text}"
        );
    }
}
