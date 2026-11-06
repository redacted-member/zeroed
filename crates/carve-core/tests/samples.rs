//! The finished jobs on the welcome card save programs the independent checker
//! accepts, for the inch and millimeter posts and each kind of job.

use carve_core::cam::commands::CamState;
use carve_core::cam::tooldb::ToolStore;
use carve_core::cam::toolpath::MoveKind;
use carve_core::post::PostLibrary;
use carve_core::{Kernel, KernelReply};
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("carve-samples-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn kernel(scratch: &Scratch) -> Kernel {
    Kernel::with_cam(CamState::new(
        ToolStore::at(scratch.0.join("tools.json")),
        PostLibrary::at(scratch.0.join("posts")),
    ))
}

fn run(kernel: &mut Kernel, name: &str, args: &Value) -> Value {
    match kernel.apply(name, &args.to_string()).expect(name) {
        KernelReply::Json(text) => {
            let value: Value = serde_json::from_str(&text).expect("reply is JSON");
            value["result"].clone()
        }
        KernelReply::Bytes(_) => panic!("{name} answered bytes"),
    }
}

/// Each toolpath saved alone through `post`, so tools, sides and sheets do not
/// have to agree.
fn programs(kernel: &mut Kernel, scratch: &Scratch, post: &str) -> Vec<(String, String)> {
    let listed = run(kernel, "cam.list", &json!({}));
    let list = listed.as_array().expect("a list");
    assert!(!list.is_empty());
    list.iter()
        .map(|toolpath| {
            let path = scratch.0.join("out.txt");
            run(
                kernel,
                "cam.save",
                &json!({
                    "toolpathIds": [toolpath["id"]],
                    "post": post,
                    "path": path.to_string_lossy(),
                    "output": "oneFile",
                }),
            );
            let text = std::fs::read_to_string(&path).expect("posted file");
            (toolpath["name"].as_str().expect("a name").to_string(), text)
        })
        .collect()
}

fn assert_clean(sample: &str, name: &str, program: &str, setup: &gcode_check::Setup) {
    let report = gcode_check::check(program, setup);
    assert!(
        report.at_least(gcode_check::Severity::Error).is_empty(),
        "{sample} {name}: {}",
        report.summary()
    );
}

#[test]
fn the_flat_samples_save_clean_programs() {
    for sample in [
        "house-sign",
        "relief-plaque",
        "box-joints",
        "knockdown-table",
        "nested-sheet",
        "two-sided",
    ] {
        let scratch = Scratch::new(sample);
        let mut kernel = kernel(&scratch);
        run(&mut kernel, "samples.open", &json!({ "id": sample }));
        for (post, units) in [
            ("linuxcnc_inch", gcode_check::Units::Inch),
            ("linuxcnc_mm", gcode_check::Units::Mm),
        ] {
            let setup = gcode_check::Setup::new(units, 0.0);
            for (name, program) in programs(&mut kernel, &scratch, post) {
                assert_clean(sample, &name, &program, &setup);
            }
        }
    }
}

#[test]
fn the_rotary_sample_saves_clean_wrapped_programs() {
    let scratch = Scratch::new("rotary");
    let mut kernel = kernel(&scratch);
    run(
        &mut kernel,
        "samples.open",
        &json!({ "id": "rotary-sleeve" }),
    );
    let job = run(&mut kernel, "job.get", &json!({}));
    let along_x = job["rotary"]["axis"] == "x";
    assert!(
        !along_x,
        "the blank lies along Y, as the rotary spindle does"
    );
    let diameter = job["rotary"]["diameter"].as_f64().expect("a diameter");
    let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, 0.0);
    setup.rotary = Some(gcode_check::Rotary {
        axis: 'A',
        across: if along_x { 'Y' } else { 'X' },
        diameter,
        linear: false,
        reversed: false,
    });
    for (name, program) in programs(&mut kernel, &scratch, "linuxcnc_rotary_mm") {
        assert!(program.contains('A'), "{name} turns the blank");
        assert_clean("rotary-sleeve", &name, &program, &setup);
    }
}

#[test]
fn the_laser_sample_saves_clean_laser_programs() {
    let scratch = Scratch::new("laser");
    let mut kernel = kernel(&scratch);
    run(
        &mut kernel,
        "samples.open",
        &json!({ "id": "laser-coaster" }),
    );
    let mut setup = gcode_check::Setup::new(gcode_check::Units::Mm, 0.0);
    setup.laser = true;
    for (name, program) in programs(&mut kernel, &scratch, "laser_grbl") {
        assert_clean("laser-coaster", &name, &program, &setup);
    }
}

#[test]
fn a_rotary_pocket_cuts_along_the_blank_whatever_the_form_asked_for() {
    let scratch = Scratch::new("rotary-raster");
    let mut kernel = kernel(&scratch);
    run(
        &mut kernel,
        "samples.open",
        &json!({ "id": "rotary-sleeve" }),
    );
    let job = run(&mut kernel, "job.get", &json!({}));
    assert_eq!(job["rotary"]["axis"], "y", "the blank lies along Y");
    let listed = run(&mut kernel, "cam.list", &json!({}));
    let pocket = listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|t| t["strategy"] == "pocket")
        .expect("a pocket")["id"]
        .clone();
    let mut request = run(&mut kernel, "cam.get", &json!({ "id": pocket }))["request"].clone();
    request["params"]["strategy"] = json!("offset");
    request["params"]["rasterAngle"] = json!(30.0);
    request["params"]["profilePass"] = json!(false);
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": pocket, "request": request }),
    );
    let stored = run(&mut kernel, "cam.get", &json!({ "id": pocket }));
    assert_eq!(stored["request"]["params"]["strategy"], "raster");
    assert_eq!(stored["request"]["params"]["rasterAngle"], 90.0);

    let id = pocket.as_u64().expect("an id");
    let toolpath = kernel.document().toolpath(id).expect("the pocket");
    let (mut along, mut around) = (0.0_f64, 0.0_f64);
    for pair in toolpath.moves.windows(2) {
        if pair[1].kind == MoveKind::Feed {
            along += (pair[1].to.y - pair[0].to.y).abs();
            around += (pair[1].to.x - pair[0].to.x).abs();
        }
    }
    assert!(along > 3.0 * around, "along {along}, around {around}");
}
