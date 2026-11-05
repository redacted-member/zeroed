//! The devhost decodes DWG files in a child copy of itself, so a file that
//! would abort the decoder ends as an error while the host keeps running.

use std::path::Path;

use carve_core::io::dwg::{import_with, Helper};

fn fixture(name: &str) -> String {
    format!(
        "{}/../carve-core/tests/fixtures/dwg/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn helper() -> Helper {
    Helper::new(env!("CARGO_BIN_EXE_carve-devhost"))
}

#[test]
fn a_drawing_comes_in_through_the_helper() {
    let file = fixture("circle-and-line-2018.dwg");
    let (paths, _) = import_with(Path::new(&file), 1.0, None, Some(&helper())).expect("imports");
    let (direct, _) = import_with(Path::new(&file), 1.0, None, None).expect("imports");
    assert_eq!(paths.len(), 2);
    assert_eq!(paths, direct);
    // Scale and units reach the child.
    let (inches, _) =
        import_with(Path::new(&file), 2.0, Some("inch"), Some(&helper())).expect("imports");
    let (direct, _) = import_with(Path::new(&file), 2.0, Some("inch"), None).expect("imports");
    assert_eq!(inches, direct);
}

/// This file once made the third party decoder ask for a 66 PB buffer,
/// which aborted any process that decoded it. Our reader gets past the
/// damage or refuses the file; either way the host lives on.
#[test]
fn a_damaged_file_is_read_or_refused_and_the_host_lives_on() {
    let file = fixture("damaged-section-size-2018.dwg");
    let through_helper = import_with(Path::new(&file), 1.0, None, Some(&helper()));
    let in_process = import_with(Path::new(&file), 1.0, None, None);
    match (&through_helper, &in_process) {
        (Ok((paths, _)), Ok((direct, _))) => assert_eq!(paths, direct),
        (Err(_), Err(_)) => {}
        _ => panic!("{through_helper:?} but in process {in_process:?}"),
    }
    // A file cut short is refused through the helper.
    let whole = std::fs::read(fixture("circle-and-line-2018.dwg")).expect("reads");
    let cut = std::env::temp_dir().join(format!("carve-dwg-cut-{}.dwg", std::process::id()));
    std::fs::write(&cut, &whole[..whole.len() / 3]).expect("writes");
    let error = import_with(&cut, 1.0, None, Some(&helper())).expect_err("cut short");
    let _ = std::fs::remove_file(&cut);
    assert!(error.to_string().contains("DWG"), "{error}");
    // Still here, and still reading good files.
    a_drawing_comes_in_through_the_helper();
}

#[test]
fn files_that_are_not_dwg_are_refused_before_the_helper_runs() {
    let file = fixture("../3dm/cube-v5.3dm");
    let error = import_with(Path::new(&file), 1.0, None, Some(&helper())).expect_err("not dwg");
    assert!(error.to_string().contains("not a DWG"), "{error}");
}
