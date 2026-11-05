//! Damaged files never panic a reader. Every file reader the app opens a
//! user's file with is fed seeds (written by the kernel itself or taken from
//! the fixtures) with deterministic damage: flipped bytes, cut short, bytes
//! inserted, spans repeated, numbers swapped for extreme ones. Each must end
//! in `Ok` or `Err`; a panic fails the test with the seed and case to
//! reproduce it.
//!
//! `CARVE_FUZZ_CASES` sets the cases per seed (40 by default, which keeps
//! the run short); a longer run is `CARVE_FUZZ_CASES=3000 cargo test -p
//! carve-core --release --test fuzz_readers`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use carve_core::geom::{circle, rect, Point};
use carve_core::io::{self, ImportOptions};
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const EXTREMES: [&[u8]; 8] = [
    b"1e308",
    b"-1e308",
    b"0",
    b"-0",
    b"nan",
    b"4294967296",
    b"-1",
    b"99999999999",
];

/// `seed` with one kind of damage chosen by `rng`.
fn damage(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    match rng.below(6) {
        0 => {
            for _ in 0..=rng.below(8) {
                let at = rng.below(bytes.len());
                if let Some(byte) = bytes.get_mut(at) {
                    *byte ^= 1 << rng.below(8);
                }
            }
        }
        1 => bytes.truncate(rng.below(bytes.len())),
        2 => {
            let at = rng.below(bytes.len() + 1);
            let extra: Vec<u8> = (0..=rng.below(64)).map(|_| rng.next() as u8).collect();
            bytes.splice(at..at, extra);
        }
        3 => {
            let from = rng.below(bytes.len());
            let to = (from + 1 + rng.below(256)).min(bytes.len());
            let span = bytes[from..to].to_vec();
            let at = rng.below(bytes.len() + 1);
            bytes.splice(at..at, span);
        }
        4 => {
            // A run of digits becomes an extreme number.
            let digits: Vec<usize> = (0..bytes.len())
                .filter(|&i| {
                    bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_digit())
                })
                .collect();
            if !digits.is_empty() {
                let start = digits[rng.below(digits.len())];
                let end = (start..bytes.len())
                    .find(|&i| !bytes[i].is_ascii_digit())
                    .unwrap_or(bytes.len());
                let extreme = EXTREMES[rng.below(EXTREMES.len())];
                bytes.splice(start..end, extreme.iter().copied());
            }
        }
        _ => {
            // A four byte word (a length or count in a binary format)
            // becomes all ones or zero.
            if bytes.len() >= 4 {
                let at = rng.below(bytes.len() - 3);
                let fill = if rng.below(2) == 0 { 0xff } else { 0 };
                bytes[at..at + 4].fill(fill);
            }
        }
    }
    bytes
}

fn cases() -> usize {
    std::env::var("CARVE_FUZZ_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(40)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("carve-fuzz-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// Runs `read` on damaged copies of `seed`, written to a file with
/// extension `ext`. Answers the cases that panicked.
fn fuzz(label: &str, seed: &[u8], ext: &str, read: impl Fn(&Path)) -> Vec<String> {
    let dir = scratch(label);
    let path = dir.join(format!("case.{ext}"));
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ label.len() as u64 ^ (seed.len() as u64) << 16);
    let mut panics = Vec::new();
    for case in 0..cases() {
        let bytes = damage(seed, &mut rng);
        std::fs::write(&path, &bytes).expect("case written");
        if catch_unwind(AssertUnwindSafe(|| read(&path))).is_err() {
            let kept = dir.join(format!("panic-{case}.{ext}"));
            std::fs::write(&kept, &bytes).ok();
            panics.push(format!("{label} case {case}: {}", kept.display()));
        }
    }
    panics
}

fn run(kernel: &mut Kernel, name: &str, args: Value) -> Value {
    match kernel.apply(name, &args.to_string()) {
        Ok(KernelReply::Json(text)) => {
            let value: Value = serde_json::from_str(&text).expect("JSON");
            assert_eq!(value["ok"], true, "{name}: {value}");
            value["result"].clone()
        }
        Ok(KernelReply::Bytes(_)) => panic!("{name} answered with bytes"),
        Err(error) => panic!("{name} failed: {error}"),
    }
}

/// A job with a rectangle and a circle, written by the kernel in every
/// vector format and as a job.
fn kernel_seeds() -> Vec<(String, Vec<u8>)> {
    let mut kernel = Kernel::new();
    let square = rect(Point::new(10.0, 10.0), Point::new(60.0, 40.0)).expect("rect");
    let round = circle(Point::new(80.0, 60.0), 12.5).expect("circle");
    for path in [square, round] {
        run(
            &mut kernel,
            "entities.create",
            json!({ "entity": { "kind": "path", "path": path } }),
        );
    }
    let dir = scratch("seeds");
    let mut seeds = Vec::new();
    for ext in ["svg", "dxf", "eps", "ai", "pdf"] {
        let path = dir.join(format!("seed.{ext}"));
        run(&mut kernel, "io.exportVectors", json!({ "path": path }));
        seeds.push((ext.to_string(), std::fs::read(&path).expect("exported")));
    }
    let job = dir.join("seed.carve");
    run(&mut kernel, "doc.save", json!({ "path": job }));
    seeds.push(("carve".to_string(), std::fs::read(&job).expect("saved")));
    seeds
}

fn binary_stl() -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    let triangles: [[[f32; 3]; 3]; 2] = [
        [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 5.0]],
        [[10.0, 0.0, 0.0], [10.0, 10.0, 5.0], [0.0, 10.0, 5.0]],
    ];
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for triangle in triangles {
        bytes.extend_from_slice(&[0u8; 12]);
        for vertex in triangle {
            for value in vertex {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
    }
    bytes
}

fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(body.len() as u32 + 6).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

/// A 3DS file with one object of two triangles.
fn three_ds() -> Vec<u8> {
    let mut vertices = 4u16.to_le_bytes().to_vec();
    for v in [
        [0.0f32, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 2.0],
        [0.0, 10.0, 2.0],
    ] {
        for value in v {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut faces = 2u16.to_le_bytes().to_vec();
    for f in [[0u16, 1, 2, 0], [0, 2, 3, 0]] {
        for value in f {
            faces.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mesh = [chunk(0x4110, &vertices), chunk(0x4120, &faces)].concat();
    let mut object = b"plate\0".to_vec();
    object.extend(chunk(0x4100, &mesh));
    chunk(0x4D4D, &chunk(0x3D3D, &chunk(0x4000, &object)))
}

fn bitmaps() -> Vec<(String, Vec<u8>)> {
    let image = image::RgbaImage::from_fn(12, 9, |x, y| {
        image::Rgba([(x * 20) as u8, (y * 25) as u8, 90, 255])
    });
    let mut seeds = Vec::new();
    for (ext, format) in [
        ("png", image::ImageFormat::Png),
        ("bmp", image::ImageFormat::Bmp),
        ("gif", image::ImageFormat::Gif),
        ("tif", image::ImageFormat::Tiff),
        ("jpg", image::ImageFormat::Jpeg),
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let written = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgba8(image.clone())
                .to_rgb8()
                .write_to(&mut bytes, format)
        } else {
            image.write_to(&mut bytes, format)
        };
        written.expect("encoded");
        seeds.push((ext.to_string(), bytes.into_inner()));
    }
    seeds
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(relative),
    )
    .expect("fixture")
}

fn read_vectors(path: &Path) {
    let _ = io::import_vectors(path, &ImportOptions::default());
}

#[test]
fn damaged_vector_and_job_files_end_in_an_error_not_a_panic() {
    let mut panics = Vec::new();
    for (ext, seed) in kernel_seeds() {
        if ext == "carve" {
            panics.extend(fuzz("carve", &seed, "carve", |path| {
                let _ = io::native::open(path, 1);
            }));
        } else {
            panics.extend(fuzz(&ext, &seed, &ext, read_vectors));
        }
    }
    assert!(panics.is_empty(), "{panics:#?}");
}

#[test]
fn damaged_meshes_end_in_an_error_not_a_panic() {
    let ascii = b"solid s\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 10 5\nendloop\nendfacet\nendsolid s\n".to_vec();
    let obj = b"v 0 0 0\nv 10 0 0\nv 10 10 2\nv 0 10 2\nvt 0 0\nvn 0 0 1\nf 1/1/1 2/1/1 3/1/1\nf 1 3 4\nf -4 -2 -1\n".to_vec();
    let seeds = [
        ("stl-ascii", ascii, "stl"),
        ("stl-binary", binary_stl(), "stl"),
        ("obj", obj, "obj"),
        ("3ds", three_ds(), "3ds"),
        ("3dm-v5", fixture("3dm/cube-v5.3dm"), "3dm"),
        ("3dm-v8", fixture("3dm/dome-v8.3dm"), "3dm"),
    ];
    let mut panics = Vec::new();
    for (label, seed, ext) in &seeds {
        panics.extend(fuzz(label, seed, ext, |path| {
            let _ = io::mesh::read(path);
        }));
    }
    assert!(panics.is_empty(), "{panics:#?}");
}

#[test]
fn damaged_bitmaps_and_clipart_end_in_an_error_not_a_panic() {
    let mut panics = Vec::new();
    for (ext, seed) in bitmaps() {
        panics.extend(fuzz(&format!("bitmap-{ext}"), &seed, &ext, |path| {
            let _ = io::bitmap::read(path);
        }));
    }
    let dir = scratch("clip-seed");
    let clip_path = dir.join("rosette.carveclip");
    let clip = carve_core::clipart::samples::sample("Rosette").expect("a sample");
    carve_core::clipart::clip_file::write(&clip, &clip_path).expect("written");
    let seed = std::fs::read(&clip_path).expect("read back");
    panics.extend(fuzz("carveclip", &seed, "carveclip", |path| {
        let _ = carve_core::clipart::clip_file::read(path);
    }));
    assert!(panics.is_empty(), "{panics:#?}");
}

#[test]
fn damaged_dwg_files_end_in_an_error_not_a_panic() {
    let mut panics = Vec::new();
    for name in [
        "everything-r14.dwg",
        "everything-2004.dwg",
        "everything-2007.dwg",
        "polylines-2018.dwg",
    ] {
        panics.extend(fuzz(
            name,
            &fixture(&format!("dwg/{name}")),
            "dwg",
            |path| {
                if let Ok(bytes) = std::fs::read(path) {
                    let _ = io::dwg::read(&bytes);
                }
            },
        ));
    }
    assert!(panics.is_empty(), "{panics:#?}");
}
