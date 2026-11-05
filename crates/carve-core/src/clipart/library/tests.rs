use std::collections::BTreeSet;

use rayon::prelude::*;
use serde_json::{json, Value};

use super::*;
use crate::clipart::{insert, thumbnail, thumbnail_png, InsertArgs};
use crate::doc::Document;
use crate::geom::{flatten_path, BBox, Path, Tolerance};
use crate::io::{import_vectors, ImportOptions};
use crate::model::field::Field;
use crate::test_support::TempDir;

/// The top level categories the library files its items under.
const TOP: [&str; 19] = [
    "Animals",
    "Architectural",
    "Borders and Corners",
    "Frames and Plaques",
    "Geometric and Celtic Knots",
    "Faces and Figures",
    "Hearts and Stars",
    "Holidays",
    "Leaves and Floral",
    "Letters and Monograms",
    "Molding and Edge Profiles",
    "Nautical",
    "Rosettes and Medallions",
    "Scrolls and Ornaments",
    "Shapes",
    "Signs and Arrows",
    "Sports",
    "Textures",
    "Western",
];

const MANIFEST: &str = include_str!("../../../assets/clipart/MANIFEST.json");

/// What building an item tells the manifest: its size and cell.
#[derive(Debug, Clone)]
struct Built {
    width: f64,
    height: f64,
    cell: Option<f64>,
}

fn manifest_text(built: &[Built]) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!(
        "  \"version\": {LIBRARY_VERSION},\n  \"author\": \"{AUTHOR}\",\n  \"license\": \"{LICENSE}\",\n  \"sources\": \"crates/carve-core/src/clipart/library/\",\n  \"items\": [\n"
    ));
    let count = catalogue().len();
    for (k, (e, b)) in catalogue().iter().zip(built).enumerate() {
        let mut item = json!({
            "file": e.relative_path().to_string_lossy().replace('\\', "/"),
            "kind": match e.kind { ClipKind::Relief => "relief", ClipKind::Vector => "vector" },
            "size": format!("{:.1} x {:.1} mm", b.width, b.height),
            "tags": e.tags,
            "source": e.source,
            "license": e.license(),
        });
        if let Some(cell) = b.cell {
            item["cell"] = json!(format!("{cell} mm"));
        }
        if e.kind == ClipKind::Vector {
            item["closed"] = json!(!e.line_art);
        }
        if let Some(origin) = e.origin {
            item["origin"] = json!(origin.url);
            item["author"] = json!(origin.author);
            item["downloaded"] = json!(origin.downloaded);
        }
        let comma = if k + 1 < count { "," } else { "" };
        out.push_str(&format!("    {item}{comma}\n"));
    }
    out.push_str("  ]\n}\n");
    out
}

/// Every problem with a relief a carver would see: spikes and pits one
/// cell wide, and a surface that is mostly cliffs.
fn relief_problems(field: &Field) -> Vec<String> {
    let mut out = Vec::new();
    let (w, h) = (field.width, field.height);
    let mut spikes = 0;
    let (mut steps, mut cliffs) = (0usize, 0usize);
    for j in 1..h.saturating_sub(1) {
        for i in 1..w.saturating_sub(1) {
            let z = field.z[j * w + i];
            if z.is_nan() {
                continue;
            }
            let around = [
                field.z[j * w + i - 1],
                field.z[j * w + i + 1],
                field.z[(j - 1) * w + i],
                field.z[(j + 1) * w + i],
            ];
            if around.iter().any(|v| v.is_nan()) {
                continue;
            }
            let hi = around.iter().copied().fold(f32::MIN, f32::max);
            let lo = around.iter().copied().fold(f32::MAX, f32::min);
            if z - hi > 0.15 || lo - z > 0.15 {
                spikes += 1;
            }
            for v in [around[1], around[3]] {
                steps += 1;
                if (v - z).abs() > 0.5 {
                    cliffs += 1;
                }
            }
        }
    }
    if spikes > 0 {
        out.push(format!("{spikes} one cell spikes or pits"));
    }
    if steps > 0 && cliffs * 100 > steps * 3 {
        out.push(format!(
            "{cliffs} of {steps} neighbor steps are over 0.5 mm"
        ));
    }
    out
}

/// Every problem with vectors meant to be cut as shapes: the validator's
/// findings, loops that cross each other, and loops with too many nodes.
fn vector_problems(paths: &[Path]) -> Vec<String> {
    use crate::geom::contain::validate_set;
    let tol = Tolerance::new(0.001).expect("tol");
    let mut out: Vec<String> = validate_set(paths, tol)
        .into_iter()
        .map(|i| i.message)
        .collect();
    let loops: Vec<Vec<crate::geom::Point>> = paths
        .iter()
        .flat_map(|p| flatten_path(p, Tolerance::new(0.02).expect("tol")))
        .map(|l| l.points)
        .collect();
    for sub in paths.iter().flat_map(|p| &p.subpaths) {
        if sub.segments.len() > 400 {
            out.push(format!("a loop has {} segments", sub.segments.len()));
        }
    }
    let boxes: Vec<Option<BBox>> = loops
        .iter()
        .map(|l| BBox::of_points(l.iter().copied()))
        .collect();
    for a in 0..loops.len() {
        for b in (a + 1)..loops.len() {
            let (Some(ba), Some(bb)) = (boxes[a], boxes[b]) else {
                continue;
            };
            if ba.max.x < bb.min.x
                || bb.max.x < ba.min.x
                || ba.max.y < bb.min.y
                || bb.max.y < ba.min.y
            {
                continue;
            }
            if loops_cross(&loops[a], &loops[b]) {
                out.push(format!("loops {a} and {b} cross"));
            }
        }
    }
    out
}

fn loops_cross(a: &[crate::geom::Point], b: &[crate::geom::Point]) -> bool {
    let seg = |l: &[crate::geom::Point], k: usize| (l[k], l[(k + 1) % l.len()]);
    for i in 0..a.len() {
        let (p0, p1) = seg(a, i);
        for j in 0..b.len() {
            let (q0, q1) = seg(b, j);
            if p0.x.max(p1.x) < q0.x.min(q1.x) || q0.x.max(q1.x) < p0.x.min(p1.x) {
                continue;
            }
            if p0.y.max(p1.y) < q0.y.min(q1.y) || q0.y.max(q1.y) < p0.y.min(p1.y) {
                continue;
            }
            let d = |a: crate::geom::Point, b: crate::geom::Point, c: crate::geom::Point| {
                (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
            };
            let (d1, d2) = (d(p0, p1, q0), d(p0, p1, q1));
            let (d3, d4) = (d(q0, q1, p0), d(q0, q1, p1));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                return true;
            }
        }
    }
    false
}

#[test]
fn every_item_builds_cleanly_and_the_manifest_records_it() {
    let results: Vec<Result<Built, String>> = catalogue()
        .par_iter()
        .map(|entry| {
            let label = format!("{} / {}", entry.category, entry.name);
            match entry.build() {
                Ok(Art::Relief(field)) => {
                    let problems = relief_problems(&field);
                    let (w, h) = (
                        field.width as f64 * field.cell,
                        field.height as f64 * field.cell,
                    );
                    let limit = if w.max(h) < 100.0 { 0.1 } else { 0.25 };
                    let mut problems = problems;
                    if field.cell > limit + 1e-9 {
                        problems.push(format!("cell {} is coarser than {limit}", field.cell));
                    }
                    if problems.is_empty() {
                        Ok(Built {
                            width: w,
                            height: h,
                            cell: Some(field.cell),
                        })
                    } else {
                        Err(format!("{label}: {}", problems.join("; ")))
                    }
                }
                Ok(Art::Vectors(paths)) => {
                    let problems = if entry.line_art {
                        Vec::new()
                    } else {
                        vector_problems(&paths)
                    };
                    let bbox = paths
                        .iter()
                        .flat_map(|p| flatten_path(p, art::tol()))
                        .filter_map(|l| BBox::of_points(l.points))
                        .reduce(BBox::union);
                    match (bbox, problems.is_empty()) {
                        (Some(b), true) => Ok(Built {
                            width: b.width(),
                            height: b.height(),
                            cell: None,
                        }),
                        (None, _) => Err(format!("{label}: no vectors")),
                        (_, false) => Err(format!("{label}: {}", problems.join("; "))),
                    }
                }
                Err(error) => Err(format!("{label}: {error}")),
            }
        })
        .collect();
    let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
    assert!(
        failures.is_empty(),
        "{} items fail:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let built: Vec<Built> = results.into_iter().filter_map(Result::ok).collect();
    let expected = manifest_text(&built);
    if std::env::var_os("CARVE_UPDATE_CLIPART_MANIFEST").is_some() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/clipart/MANIFEST.json");
        std::fs::write(&path, &expected).expect("writes the manifest");
        return;
    }
    assert!(
        MANIFEST == expected,
        "assets/clipart/MANIFEST.json is out of date; run the library tests with \
         CARVE_UPDATE_CLIPART_MANIFEST=1 to rewrite it"
    );
    let parsed: Value = serde_json::from_str(MANIFEST).expect("the manifest is JSON");
    assert_eq!(parsed["author"], AUTHOR);
    for item in parsed["items"].as_array().expect("items") {
        if item.get("origin").is_some() {
            // Outside art: only licenses that allow shipping it, each with
            // its page, author and download day.
            let license = item["license"].as_str().expect("license");
            assert!(
                ["CC0-1.0", "PDM", "CC-BY-3.0", "CC-BY-4.0"].contains(&license),
                "{license} is not a license the library may ship"
            );
            assert!(item["origin"]
                .as_str()
                .is_some_and(|u| u.starts_with("https://")));
            assert!(item["author"].as_str().is_some_and(|a| !a.is_empty()));
            assert!(item["downloaded"].as_str().is_some_and(|d| d.len() == 10));
        } else {
            assert!(item["license"] == LICENSE || item["license"] == FONT_LICENSE);
        }
        let source = item["source"].as_str().expect("source");
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/clipart/library")
            .join(source);
        assert!(file.is_file(), "{source} is not a file");
    }
}

#[test]
fn textures_repeat_without_a_seam() {
    let bad: Vec<String> = catalogue()
        .par_iter()
        .filter(|e| e.category.starts_with("Textures/"))
        .filter_map(|e| {
            let Ok(Art::Relief(f)) = e.build() else {
                return Some(format!("{}: does not build", e.name));
            };
            let (w, h) = (f.width, f.height);
            let at = |i: usize, j: usize| f64::from(f.z[j * w + i]);
            if (0..w * h).any(|k| f.z[k].is_nan()) {
                return Some(format!("{}: has empty cells", e.name));
            }
            // The step across each seam must look like any step inside.
            let mut inside: f64 = 0.0;
            for j in 0..h {
                for i in 1..w {
                    inside = inside.max((at(i, j) - at(i - 1, j)).abs());
                }
            }
            for i in 0..w {
                for j in 1..h {
                    inside = inside.max((at(i, j) - at(i, j - 1)).abs());
                }
            }
            let across = (0..h)
                .map(|j| (at(0, j) - at(w - 1, j)).abs())
                .chain((0..w).map(|i| (at(i, 0) - at(i, h - 1)).abs()))
                .fold(0.0, f64::max);
            (across > inside * 1.5 + 0.02).then(|| {
                format!(
                    "{}: seam step {across:.3} against {inside:.3} inside",
                    e.name
                )
            })
        })
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn names_and_categories_are_tidy() {
    let mut seen = BTreeSet::new();
    for entry in catalogue() {
        let top = entry.category.split('/').next().unwrap_or("");
        assert!(TOP.contains(&top), "{} files under {top}", entry.name);
        assert!(
            entry.category.split('/').count() == 2,
            "{} needs a subcategory",
            entry.category
        );
        assert!(
            seen.insert((entry.category.clone(), entry.name.clone())),
            "{} / {} is listed twice",
            entry.category,
            entry.name
        );
        assert!(!entry.tags.is_empty(), "{} has no tags", entry.name);
        for text in [&entry.name, &entry.category] {
            assert!(!text.contains('\u{2014}'));
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_alphanumeric() || " /-'&".contains(c)),
                "{text} has a character a file name should not"
            );
        }
        let lower = entry.name.to_lowercase();
        for british in ["colour", "centre", "moulding", "grey"] {
            assert!(!lower.contains(british), "{} uses UK spelling", entry.name);
        }
    }
}

#[test]
fn the_library_meets_its_size_targets() {
    let count = |kind: ClipKind, top: Option<&str>| {
        catalogue()
            .iter()
            .filter(|e| e.kind == kind)
            .filter(|e| top.is_none_or(|t| e.category.starts_with(t)))
            .count()
    };
    let reliefs = count(ClipKind::Relief, None) - count(ClipKind::Relief, Some("Textures"));
    let vectors = count(ClipKind::Vector, None) - count(ClipKind::Vector, Some("Molding"));
    let textures = count(ClipKind::Relief, Some("Textures"));
    let profiles = count(ClipKind::Vector, Some("Molding"));
    assert!(reliefs >= 150, "{reliefs} reliefs");
    assert!(vectors >= 300, "{vectors} vectors");
    assert!(textures >= 40, "{textures} textures");
    assert!(profiles >= 40, "{profiles} profiles");
}

#[test]
fn every_item_builds_writes_reads_back_draws_and_inserts() {
    let temp = TempDir::new("clipart-library-all");
    let dir = temp.path().join(dir_name());
    let failures: Vec<String> = items(&dir)
        .par_iter()
        .filter_map(|item| {
            let path = std::path::PathBuf::from(&item.path);
            let check = || -> Result<(), String> {
                let png = thumbnail_png(&path, 64).map_err(|e| format!("thumbnail: {e}"))?;
                let image = image::load_from_memory(&png).map_err(|e| e.to_string())?;
                let filled = image
                    .to_rgba8()
                    .pixels()
                    .filter(|p| p.0[3] > 0 && p.0[0..3] != [255, 255, 255])
                    .count();
                if filled < 40 {
                    return Err(format!("the thumbnail is nearly empty ({filled} pixels)"));
                }
                match item.kind {
                    ClipKind::Relief => {
                        let clip = clip_file::read(&path).map_err(|e| e.to_string())?;
                        let field = &clip.field;
                        let max = field.max().ok_or("no material")?;
                        let min = field.min().ok_or("no material")?;
                        if !(min >= 0.0 && max > 0.2 && max < 40.0) {
                            return Err(format!("heights {min} to {max}"));
                        }
                        if field.cell < art::CELL - 1e-9 {
                            return Err(format!("cell {}", field.cell));
                        }
                        if field.width * field.height > 2000 * 2000 {
                            return Err(format!("{} by {} cells", field.width, field.height));
                        }
                    }
                    ClipKind::Vector => {
                        let read = import_vectors(&path, &ImportOptions::default())
                            .map_err(|e| e.to_string())?;
                        if read.paths.is_empty() {
                            return Err("no vectors".to_string());
                        }
                    }
                }
                let mut doc = Document::new();
                let args = InsertArgs {
                    path: path.clone(),
                    x: 150.0,
                    y: 100.0,
                    scale: 1.0,
                    combine: Default::default(),
                };
                insert(&mut doc, &args).map_err(|e| format!("insert: {e}"))?;
                Ok(())
            };
            check()
                .err()
                .map(|e| format!("{} / {}: {e}", item.category, item.name))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn only_library_paths_are_made() {
    let temp = TempDir::new("clipart-library-made");
    let stray = temp.path().join("Shapes/Domes/Round Dome.carveclip");
    materialize(&stray).expect("leaves it alone");
    assert!(!stray.exists());
    let dir = temp.path().join(dir_name());
    let unknown = dir.join("Shapes/Domes/Teapot.carveclip");
    materialize(&unknown).expect("leaves it alone");
    assert!(!unknown.exists());
    let known = dir.join("Shapes/Domes/Round Dome.carveclip");
    materialize(&known).expect("writes");
    assert!(known.is_file());
    assert!(entry_for(&known).is_some_and(|e| e.name == "Round Dome"));
}

/// Draws every item into one contact sheet per top level category, for
/// judging the art by eye: `CARVE_CLIPART_SHEETS=<folder> cargo test -p
/// carve-core --lib contact_sheets -- --ignored`.
#[test]
#[ignore = "writes contact sheets for a person to look at"]
fn contact_sheets() {
    let Some(out) = std::env::var_os("CARVE_CLIPART_SHEETS") else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("folder");
    let only = std::env::var("CARVE_CLIPART_ONLY").unwrap_or_default();
    let tile = 160u32;
    for top in TOP {
        let entries: Vec<&Entry> = catalogue()
            .iter()
            .filter(|e| e.category.starts_with(top) && e.category.contains(only.as_str()))
            .collect();
        if entries.is_empty() {
            continue;
        }
        let cols = 8u32;
        let rows = (entries.len() as u32).div_ceil(cols);
        let mut sheet = image::RgbaImage::from_pixel(
            cols * tile,
            rows * tile,
            image::Rgba([255, 255, 255, 255]),
        );
        let images: Vec<image::RgbaImage> = entries
            .par_iter()
            .map(|e| match e.build() {
                Ok(Art::Relief(field)) => thumbnail::relief(&field, tile - 8),
                Ok(Art::Vectors(paths)) => thumbnail::vectors(&paths, tile - 8),
                Err(_) => {
                    image::RgbaImage::from_pixel(tile - 8, tile - 8, image::Rgba([255, 0, 0, 255]))
                }
            })
            .collect();
        for (k, img) in images.iter().enumerate() {
            let (cx, cy) = ((k as u32 % cols) * tile + 4, (k as u32 / cols) * tile + 4);
            for (x, y, px) in img.enumerate_pixels() {
                if px.0[3] > 0 {
                    sheet.put_pixel(cx + x, cy + y, *px);
                }
            }
        }
        let name = top.replace(' ', "_");
        sheet.save(out.join(format!("{name}.png"))).expect("saves");
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        std::fs::write(out.join(format!("{name}.txt")), names.join("\n")).expect("saves");
    }
}

#[test]
fn a_band_is_an_outer_ring_and_its_hole() {
    for ring in [
        art::rounded_rect(art::p(0.0, 0.0), 24.0, 12.0, 6.0),
        art::circle(art::p(5.0, 5.0), 10.0, 64),
        super::stars::heart(50.0),
    ] {
        let band = art::band(&ring, 3.0).expect("band");
        assert_eq!(band.len(), 2, "{band:?}");
        let areas: Vec<f64> = band.iter().map(|r| art::signed_area2(r)).collect();
        assert!(areas[0] * areas[1] < 0.0, "{areas:?}");
    }
}
