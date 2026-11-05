//! Lists the built-in furniture templates (`src/furniture/library/*.json`)
//! so the kernel bundles every file there without a list kept by hand.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
        .join("src")
        .join("furniture")
        .join("library");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut out = String::from("&[\n");
    for name in &files {
        let path = dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    out.push(']');
    let target =
        Path::new(&std::env::var("OUT_DIR").unwrap_or_default()).join("furniture_library.rs");
    if let Err(error) = std::fs::write(&target, out) {
        panic!("cannot write {}: {error}", target.display());
    }
}
