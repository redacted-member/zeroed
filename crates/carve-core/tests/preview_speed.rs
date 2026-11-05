//! Toolpath preview sweep times at the finest quality for flat, sloped and
//! 3D moves, as a report rather than a check (timings depend on the
//! machine):
//! `cargo test --release -p carve-core --test preview_speed -- --ignored --nocapture`.

use std::time::Instant;

use carve_core::cam::sim::{Quality, Stock};
use carve_core::cam::tool::{Tool, ToolGeometry};
use carve_core::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
use carve_core::doc::JobSetup;

fn toolpath(tool: ToolGeometry, moves: Vec<Move>) -> Toolpath {
    let tool = Tool {
        name: "T".into(),
        tool_type: tool,
        ..Tool::default()
    };
    let mut tp = Toolpath::new(1, "speed", tool, StrategyKind::Profile);
    tp.home = Point3::new(0.0, 0.0, 20.0);
    tp.moves = moves;
    tp
}

/// V carving: zig-zag lines whose depth rises and falls every 2 mm.
fn vgroove() -> Vec<Move> {
    let mut moves = vec![Move::rapid(Point3::new(20.0, 20.0, 5.0))];
    for line in 0..20 {
        let y = 20.0 + f64::from(line) * 8.0;
        moves.push(Move::rapid(Point3::new(20.0, y, 5.0)));
        moves.push(Move::plunge(Point3::new(20.0, y, -0.3)));
        for k in 1..=50 {
            let x = 20.0 + f64::from(k) * 2.0;
            let z = if k % 2 == 0 { -0.3 } else { -3.0 };
            moves.push(Move::feed(Point3::new(x, y + f64::from(k % 3) * 0.7, z)));
        }
        moves.push(Move::rapid(Point3::new(120.0, y, 5.0)));
    }
    moves
}

/// Deep V carving: long moves at every angle, 4 to 12 mm down.
fn deep_vgroove() -> Vec<Move> {
    let mut moves = vec![Move::rapid(Point3::new(150.0, 100.0, 5.0))];
    moves.push(Move::plunge(Point3::new(150.0, 100.0, -4.0)));
    for k in 0..60 {
        let a = f64::from(k) * 2.399;
        let r = 20.0 + f64::from(k % 7) * 9.0;
        let z = -4.0 - f64::from(k % 5) * 2.0;
        moves.push(Move::feed(Point3::new(
            150.0 + r * a.cos(),
            100.0 + r * a.sin(),
            z,
        )));
    }
    moves
}

/// 3D finishing: raster lines over a dome in 0.5 mm steps.
fn finish() -> Vec<Move> {
    let dome = |x: f64, y: f64| {
        let r2 = (x - 150.0).powi(2) + (y - 100.0).powi(2);
        (4900.0 - r2).max(0.0).sqrt() * 0.2 - 16.0
    };
    let mut moves = vec![Move::rapid(Point3::new(100.0, 70.0, 5.0))];
    for line in 0..60 {
        let y = 70.0 + f64::from(line) * 1.0;
        let xs: Vec<f64> = (0..=200).map(|k| 100.0 + f64::from(k) * 0.5).collect();
        let xs: Vec<f64> = if line % 2 == 0 {
            xs
        } else {
            xs.into_iter().rev().collect()
        };
        for x in xs {
            moves.push(Move::feed(Point3::new(x, y, dome(x, y))));
        }
    }
    moves
}

/// Ramping in: an end mill zig-zags down 6 mm over 30 mm passes.
fn ramps() -> Vec<Move> {
    let mut moves = vec![
        Move::rapid(Point3::new(200.0, 50.0, 5.0)),
        Move::plunge(Point3::new(200.0, 50.0, 0.0)),
    ];
    for pass in 1..=20 {
        let x = if pass % 2 == 0 { 200.0 } else { 230.0 };
        moves.push(Move::feed(Point3::new(x, 50.0, -0.3 * f64::from(pass))));
    }
    moves
}

/// A flat pocket raster, for comparison.
fn flat() -> Vec<Move> {
    let mut moves = vec![
        Move::rapid(Point3::new(200.0, 100.0, 5.0)),
        Move::plunge(Point3::new(200.0, 100.0, -3.0)),
    ];
    for line in 0..20 {
        let y = 100.0 + f64::from(line) * 2.4;
        let x = if line % 2 == 0 { 280.0 } else { 200.0 };
        moves.push(Move::feed(Point3::new(x, y, -3.0)));
        moves.push(Move::feed(Point3::new(x, y + 2.4, -3.0)));
    }
    moves
}

#[test]
#[ignore = "a timing report, not a check"]
fn preview_sweep_times() {
    let job = JobSetup {
        width: 300.0,
        height: 200.0,
        thickness: 25.0,
        ..JobSetup::default()
    };
    let cases = [
        (
            "V carve, 12 mm 60 degree V-bit",
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 60.0,
            },
            vgroove(),
        ),
        (
            "deep V carve, 12 mm 60 degree",
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 60.0,
            },
            deep_vgroove(),
        ),
        (
            "3D finish, 6 mm ball nose",
            ToolGeometry::BallNose { diameter: 6.0 },
            finish(),
        ),
        (
            "ramps, 6 mm end mill",
            ToolGeometry::EndMill { diameter: 6.0 },
            ramps(),
        ),
        (
            "flat pocket, 6 mm end mill",
            ToolGeometry::EndMill { diameter: 6.0 },
            flat(),
        ),
    ];
    for quality in [Quality::Standard, Quality::Maximum] {
        for (label, tool, moves) in &cases {
            let tp = toolpath(tool.clone(), moves.clone());
            let length = tp.cutting_length();
            // The quickest of three, the machine being shared.
            let ms = (0..3)
                .map(|_| {
                    let mut stock = Stock::new(&job, quality);
                    let start = Instant::now();
                    stock.cut(&tp, 1).expect("cuts");
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .fold(f64::INFINITY, f64::min);
            println!(
                "{quality:?} {label:<32} {length:>7.0} mm {ms:>9.1} ms {:>8.4} ms/mm",
                ms / length
            );
        }
    }
}

/// The cost of the collision checks while the animation cuts: the same
/// toolpaths stepped a frame's worth at a time at Standard with a tool
/// whose flutes, shank and holder are not set (nothing checked), set clear
/// of the material (checked, the usual case), and set so short that every
/// sample scans the stock (the worst case):
/// `cargo test --release -p carve-core --test preview_speed -- --ignored --nocapture check_costs`.
#[test]
#[ignore = "a timing report, not a check"]
fn check_costs() {
    use carve_core::cam::playback::{Budget, Playback};
    use carve_core::doc::Document;

    let job = JobSetup {
        width: 300.0,
        height: 200.0,
        thickness: 25.0,
        ..JobSetup::default()
    };
    let cases = [
        (
            "V carve, 12 mm V-bit",
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 60.0,
            },
            vgroove(),
        ),
        (
            "3D finish, 6 mm ball nose",
            ToolGeometry::BallNose { diameter: 6.0 },
            finish(),
        ),
        (
            "ramps, 6 mm end mill",
            ToolGeometry::EndMill { diameter: 6.0 },
            ramps(),
        ),
        (
            "flat pocket, 6 mm end mill",
            ToolGeometry::EndMill { diameter: 6.0 },
            flat(),
        ),
    ];
    type Setup = (&'static str, fn(&mut Tool));
    let setups: [Setup; 3] = [
        ("unchecked", |_| {}),
        ("checked", |t| {
            t.flute_length = 30.0;
            t.shank_diameter = 8.0;
            t.length_below_holder = 40.0;
            t.holder_diameter = 35.0;
            t.holder_length = 50.0;
            t.center_cutting = false;
        }),
        ("worst", |t| {
            t.flute_length = 0.2;
            t.shank_diameter = 10.0;
            t.length_below_holder = 0.5;
            t.holder_diameter = 35.0;
            t.holder_length = 50.0;
        }),
    ];
    for (label, geometry, moves) in &cases {
        for (setup, apply) in &setups {
            let mut tp = toolpath(geometry.clone(), moves.clone());
            apply(&mut tp.tool);
            let length = tp.cutting_length();
            let mut doc = Document::new();
            doc.set_job(job).expect("job");
            doc.add_toolpath(tp);
            let ms = (0..3)
                .map(|_| {
                    let mut stock = Stock::new(&job, Quality::Standard);
                    let mut run = Playback::new(&doc, |_| true);
                    let start = Instant::now();
                    // Speed 5 moves about 30 mm a 16 ms frame.
                    while !run
                        .step(&doc, &mut stock, Budget::Distance(30.0))
                        .expect("steps")
                        .done
                    {}
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .fold(f64::INFINITY, f64::min);
            println!(
                "Standard {label:<28} {setup:<10} {length:>7.0} mm {ms:>8.1} ms {:>8.4} ms/mm",
                ms / length
            );
        }
    }
}
