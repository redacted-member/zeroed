//! DWG reader tests. The fixtures in `tests/fixtures/dwg` were written by
//! the third party reader this one replaced, for every release from R13 to
//! 2018; the `.json` next to each holds the vectors that reader drew from
//! it, which this one must reproduce.

use super::*;
use crate::geom::{path_bbox, path_length, Tolerance};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn length(path: &crate::geom::Path) -> f64 {
    path_length(path, Tolerance::new(1e-6).expect("tolerance"))
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dwg")
        .join(name)
}

/// The vectors of a fixture, imported as the app does.
fn imported(name: &str, units: Option<&str>) -> (Vec<ImportedPath>, Vec<ImportWarning>) {
    import(&fixture(name), 1.0, units).expect("imports")
}

#[test]
fn plain_entities_come_in_on_their_layers() {
    let (paths, warnings) = imported("everything-2018.dwg", None);
    assert_eq!(warnings, vec![ImportWarning::TextSkipped]);
    assert_eq!(paths.len(), 29, "{paths:#?}");
    let line = &paths[0];
    assert_eq!(line.layer, "Cut");
    assert!(close(length(&line.path), 50.0 * 5f64.sqrt()));
    let circle = path_bbox(&paths[2].path).expect("bbox");
    assert!(close(circle.width(), 20.0) && close(circle.min.x, 40.0));
    assert_eq!(paths[2].layer, "0");
    assert!(close(length(&paths[3].path), 20.0 * 1.7));
    let shape = &paths[6];
    assert_eq!(shape.layer, "Cut");
    assert!(shape.path.subpaths[0].closed);
    // The bulge of 1 puts a half circle of radius 5 on the right side.
    let bbox = path_bbox(&shape.path).expect("bbox");
    assert!(close(bbox.max.x, 15.0), "{bbox:?}");
    assert!(!paths[7].path.subpaths[0].closed);
}

#[test]
fn upside_down_arcs_and_circles_are_turned_over() {
    let (paths, _) = imported("everything-2004.dwg", None);
    // Seen from above the arc's centre is at x = -10 and the circle's at 30.
    let arc = path_bbox(&paths[4].path).expect("bbox");
    assert!(close(arc.min.x, -15.0) && close(arc.max.x, -5.0), "{arc:?}");
    let circle = path_bbox(&paths[5].path).expect("bbox");
    assert!(
        close(circle.min.x, 26.0) && close(circle.max.x, 34.0),
        "{circle:?}"
    );
}

#[test]
fn inch_drawings_are_scaled_to_millimetres() {
    let (paths, _) = imported("polylines-2000.dwg", None);
    // The 3D polyline runs from x 0 to 9 inches.
    let bbox = path_bbox(&paths[1].path).expect("bbox");
    assert!(close(bbox.width(), 228.6), "{bbox:?}");
    let (paths, _) = imported("polylines-2000.dwg", Some("mm"));
    let bbox = path_bbox(&paths[1].path).expect("bbox");
    assert!(close(bbox.width(), 9.0), "{bbox:?}");
    // The 2D polyline is closed and keeps its bulges.
    let square = &paths[0].path.subpaths[0];
    assert!(square.closed);
    assert_eq!(square.segments.len(), 4);
}

#[test]
fn blocks_are_exploded_where_they_are_placed() {
    let (paths, _) = imported("everything-2013.dwg", None);
    let peg: Vec<&ImportedPath> = paths.iter().filter(|p| p.layer == "Parts").collect();
    assert_eq!(peg.len(), 1, "{paths:#?}");
    let bbox = path_bbox(&peg[0].path).expect("bbox");
    // Base (5, 0) lands on (100, 100); the 10 long line, doubled and
    // turned a quarter, runs up to (100, 120).
    assert!(
        close(bbox.min.x, 100.0) && close(bbox.max.x, 100.0),
        "{bbox:?}"
    );
    assert!(
        close(bbox.min.y, 100.0) && close(bbox.max.y, 120.0),
        "{bbox:?}"
    );
    // The block's circle keeps its own layer; the MINSERT places the
    // block six times, three columns by two rows turned 0.25 rad.
    assert!(paths[13].layer == "Cut");
    let array: Vec<&ImportedPath> = paths[14..26].iter().step_by(2).collect();
    assert_eq!(array.len(), 6);
    let second = array[1].path.subpaths[0].start;
    assert!(close(second.x, -100.0 + 30.0 * 0.25f64.cos()), "{second:?}");
    assert!(close(second.y, -100.0 + 30.0 * 0.25f64.sin()), "{second:?}");
    // Nested blocks come in on the outer reference's layer.
    assert!(paths[26..].iter().all(|p| p.layer == "Cut"));
}

#[test]
fn splines_and_ellipses_are_curves() {
    let (paths, _) = imported("everything-2007.dwg", None);
    let curve = &paths[8].path.subpaths[0];
    assert_eq!(curve.segments.len(), 2);
    assert!(close(curve.end().x, 50.0) && close(curve.end().y, -10.0));
    assert!(paths[10].path.subpaths[0].closed);
    assert!(!paths[11].path.subpaths[0].closed);
}

#[test]
fn partial_ellipses_run_from_start_to_end() {
    let sub = walk::ellipse_subpath(
        &[0.0, 0.0, 0.0],
        &[10.0, 0.0, 0.0],
        &[0.0, 0.0, 1.0],
        0.5,
        0.0,
        std::f64::consts::PI,
    )
    .expect("half");
    assert!(!sub.closed);
    assert!(sub.end().distance(crate::geom::Point::new(-10.0, 0.0)) < 1e-9);
    let top = path_bbox(&sub.into_path()).expect("bbox");
    assert!(close(top.max.y, 5.0) && close(top.min.y, 0.0), "{top:?}");
    // Values no file should hold are not drawn.
    assert!(walk::ellipse_subpath(
        &[0.0, 0.0, 0.0],
        &[10.0, 0.0, 0.0],
        &[0.0, 0.0, 1.0],
        f64::NAN,
        0.0,
        1.0
    )
    .is_none());
}

#[test]
fn files_that_are_not_dwg_are_refused() {
    let temp = crate::test_support::TempDir::new("dwg-bad");
    let path = temp.path().join("not.dwg");
    std::fs::write(&path, b"0\nSECTION\n").expect("writes");
    let error = import(&path, 1.0, None).expect_err("not a dwg");
    assert!(matches!(error, CoreError::Format { .. }), "{error}");
    std::fs::write(&path, b"AC1032\0\0\0garbage that is not a drawing").expect("writes");
    assert!(import(&path, 1.0, None).is_err());
    std::fs::write(&path, b"AC1009\0\0\0an R12 drawing").expect("writes");
    assert!(import(&path, 1.0, None).is_err());
    assert!(import(&temp.path().join("missing.dwg"), 1.0, None).is_err());
    assert_eq!(insunits_to_mm(1), 25.4);
    assert_eq!(insunits_to_mm(0), 1.0);
}

/// Compares two JSON values, numbers to within 1e-6.
fn same_json(a: &serde_json::Value, b: &serde_json::Value, at: &str) -> Result<(), String> {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            if (x - y).abs() <= 1e-6 * (1.0 + x.abs().max(y.abs())) {
                Ok(())
            } else {
                Err(format!("{at}: {x} != {y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{at}: {} items != {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same_json(p, q, &format!("{at}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(x), Value::Object(y)) => {
            for (key, p) in x {
                let q = y.get(key).ok_or(format!("{at}.{key} missing"))?;
                same_json(p, q, &format!("{at}.{key}"))?;
            }
            if x.len() == y.len() {
                Ok(())
            } else {
                Err(format!("{at}: keys differ"))
            }
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{at}: {a} != {b}")),
    }
}

#[test]
fn every_release_draws_what_was_recorded() {
    let mut failures = Vec::new();
    for tag in ["r13", "r14", "2000", "2004", "2007", "2010", "2013", "2018"] {
        for name in ["everything", "polylines"] {
            let file = fixture(&format!("{name}-{tag}.dwg"));
            let bytes = std::fs::read(&file).expect("fixture");
            let expected: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(file.with_extension("json")).expect("summary"),
            )
            .expect("json");
            let result = read(&bytes).map(|d| vectors(&d, 1.0, None));
            let Ok((paths, warnings)) = result else {
                failures.push(format!("{name}-{tag}: {:?}", result.err()));
                continue;
            };
            let got = serde_json::json!({
                "paths": paths.iter().map(|p| serde_json::json!({"layer": p.layer, "path": p.path})).collect::<Vec<_>>(),
                "warnings": warnings,
            });
            if let Err(e) = same_json(&got, &expected, "") {
                failures.push(format!("{name}-{tag}: {e}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Decodes and draws `bytes`; false on a panic.
fn survives(bytes: &[u8]) -> bool {
    std::panic::catch_unwind(|| {
        if let Ok(drawing) = read(bytes) {
            let _ = vectors(&drawing, 1.0, None);
        }
    })
    .is_ok()
}

const RELEASES: [&str; 8] = ["r13", "r14", "2000", "2004", "2007", "2010", "2013", "2018"];

#[test]
fn cut_short_files_are_refused_without_a_panic() {
    for tag in RELEASES {
        let sample = std::fs::read(fixture(&format!("everything-{tag}.dwg"))).expect("fixture");
        let step = (sample.len() / 150).max(1);
        for end in (0..sample.len()).step_by(step) {
            assert!(survives(&sample[..end]), "{tag} cut at {end}");
        }
    }
}

#[test]
fn scrambled_files_never_panic() {
    let started = std::time::Instant::now();
    for tag in RELEASES {
        let sample = std::fs::read(fixture(&format!("everything-{tag}.dwg"))).expect("fixture");
        let mut seed = 0x2f6b_1c3d_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for round in 0..60 {
            let mut bytes = sample.clone();
            for _ in 0..1 + next() as usize % 6 {
                let at = 6 + next() as usize % (bytes.len() - 6);
                bytes[at] = next() as u8;
            }
            assert!(survives(&bytes), "{tag} round {round}");
        }
    }
    // Damaged files end quickly too.
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
}

#[test]
fn the_file_that_aborted_the_old_decoder_does_no_harm() {
    let bytes = std::fs::read(fixture("damaged-section-size-2018.dwg")).expect("fixture");
    assert!(survives(&bytes));
    let bytes = std::fs::read(fixture("circle-and-line-2018.dwg")).expect("fixture");
    let (paths, _) = vectors(&read(&bytes).expect("reads"), 1.0, None);
    assert_eq!(paths.len(), 2);
}

mod review_tests {
    use super::*;
    use crate::io::dwg::objects::{Block, Entity, Kind};

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut s = seed | 1;
        move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        }
    }

    /// Changes one to eight places of `bytes` past `from`: a bit, a byte,
    /// or a long set to a random or a huge value.
    fn mutate(bytes: &mut [u8], next: &mut impl FnMut() -> u64, from: usize) {
        if bytes.len() <= from + 8 {
            return;
        }
        for _ in 0..1 + next() as usize % 8 {
            let at = from + next() as usize % (bytes.len() - from - 4);
            match next() % 4 {
                0 => bytes[at] ^= 1 << (next() % 8),
                1 => bytes[at] = next() as u8,
                2 => bytes[at..at + 4].copy_from_slice(&(next() as u32).to_le_bytes()),
                _ => bytes[at..at + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]),
            }
        }
    }

    #[test]
    fn a_back_reference_of_no_distance_is_refused() {
        // Eight literal bytes, then a copy code 0x20 whose offset is 0.
        let data = [0x00, 1, 2, 3, 4, 5, 6, 7, 8, 0x20, 0x00, 0x00, 0x08];
        assert!(std::panic::catch_unwind(|| r2007::decompress(&data, 64)).is_ok());
        assert!(r2007::decompress(&data, 64).is_err());
    }

    #[test]
    fn mutated_files_and_sections_never_panic() {
        // The file level mutations mostly end in the page and compression
        // checks, so the decoded sections are mutated as well: that reaches
        // the object map, the classes, the header and every entity reader.
        let started = std::time::Instant::now();
        for name in ["everything", "polylines"] {
            for (n, tag) in RELEASES.into_iter().enumerate() {
                let sample = std::fs::read(fixture(&format!("{name}-{tag}.dwg"))).expect("fixture");
                let mut next = rng(0x9E37_79B9_7F4A_7C15 ^ (n as u64 * 7919) ^ name.len() as u64);
                for round in 0..150 {
                    let mut bytes = sample.clone();
                    mutate(&mut bytes, &mut next, 6);
                    assert!(survives(&bytes), "{name}-{tag} file round {round}");
                }
                let sections = file::sections(&sample).expect("sections");
                for round in 0..300 {
                    let mut s = file::Sections {
                        version: sections.version,
                        maintenance: sections.maintenance,
                        header: sections.header.clone(),
                        classes: sections.classes.clone(),
                        handles: sections.handles.clone(),
                        objects: sections.objects.clone(),
                        objects_base: sections.objects_base,
                    };
                    match round % 4 {
                        0 => mutate(&mut s.header, &mut next, 0),
                        1 => mutate(&mut s.classes, &mut next, 0),
                        2 => mutate(&mut s.handles, &mut next, 0),
                        _ => mutate(&mut s.objects, &mut next, 0),
                    }
                    let ok = std::panic::catch_unwind(move || {
                        if let Ok(drawing) = objects::drawing(&s) {
                            let _ = vectors(&drawing, 1.0, None);
                        }
                    })
                    .is_ok();
                    assert!(ok, "{name}-{tag} section round {round}");
                }
            }
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(60));
    }

    #[test]
    fn a_small_array_of_a_large_block_stops_at_a_bound() {
        // A block holding one polyline of 1,000 edges, placed by a MINSERT
        // of 60 by 100: 6 million segments from a file of a few kilobytes.
        // The instance limit alone let an array of 200,000 draw 200 million.
        let points: Vec<(f64, f64, f64)> = (0..1001).map(|i| (f64::from(i), 0.0, 0.0)).collect();
        let mut drawing = Drawing::default();
        drawing.blocks.insert(
            7,
            Block {
                base: [0.0; 3],
                entities: vec![Entity {
                    layer: "0".to_string(),
                    kind: Kind::Polyline {
                        points,
                        closed: false,
                        normal: [0.0, 0.0, 1.0],
                    },
                }],
            },
        );
        drawing.model.push(Entity {
            layer: "0".to_string(),
            kind: Kind::Insert {
                block: 7,
                at: [0.0; 3],
                scale: [1.0; 3],
                rotation: 0.0,
                normal: [0.0, 0.0, 1.0],
                array: Some((60, 100, 2000.0, 10.0)),
            },
        });
        let (paths, warnings) = walk::paths(&drawing);
        let segments: usize = paths
            .iter()
            .flat_map(|p| &p.path.subpaths)
            .map(|s| s.segments.len())
            .sum();
        assert!(segments <= walk::MAX_SEGMENTS, "{segments} segments");
        assert!(segments > 0);
        assert!(warnings.contains(&ImportWarning::BlockTooDeep));
    }
}
