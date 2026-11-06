//! The finished jobs the welcome card opens, for a new user to look through.
//!
//! They are built into the app. `tools/make-samples.mjs` makes them from the
//! jobs the test suite replays (`docs/testing/projects`), with inch tools. A
//! sample opens as an untitled job in the preferred units, so saving it asks
//! where to put it and never touches the app itself.

use serde::Deserialize;
use serde_json::json;

use crate::error::CoreError;
use crate::io::native;
use crate::kernel::{Kernel, KernelReply};

/// Each sample's id and its `.carve` file.
const SAMPLES: [(&str, &[u8]); 8] = [
    (
        "house-sign",
        include_bytes!("../assets/samples/house-sign.carve"),
    ),
    (
        "relief-plaque",
        include_bytes!("../assets/samples/relief-plaque.carve"),
    ),
    (
        "box-joints",
        include_bytes!("../assets/samples/box-joints.carve"),
    ),
    (
        "knockdown-table",
        include_bytes!("../assets/samples/knockdown-table.carve"),
    ),
    (
        "nested-sheet",
        include_bytes!("../assets/samples/nested-sheet.carve"),
    ),
    (
        "two-sided",
        include_bytes!("../assets/samples/two-sided.carve"),
    ),
    (
        "rotary-sleeve",
        include_bytes!("../assets/samples/rotary-sleeve.carve"),
    ),
    (
        "laser-coaster",
        include_bytes!("../assets/samples/laser-coaster.carve"),
    ),
];

#[derive(Deserialize)]
struct OpenArgs {
    id: String,
}

/// Whether `name` is a samples command.
pub fn handles(name: &str) -> bool {
    name == "samples.open"
}

/// Runs a samples command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] for arguments that do not parse and
/// [`CoreError::Validation`] for an id that is not a sample.
pub fn dispatch(kernel: &mut Kernel, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let args: OpenArgs =
        serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
    let (id, bytes) = SAMPLES
        .iter()
        .find(|(id, _)| *id == args.id)
        .ok_or_else(|| CoreError::validation(format!("there is no sample {}", args.id)))?;
    // Clears the display cache and the sculpt baselines, as opening any
    // document does.
    kernel.apply("doc.new", "")?;
    let revision = kernel.document().revision() + 1;
    let mut document = native::read_bytes(bytes, id, revision)?;
    kernel.prefs().values().apply_to_document(&mut document);
    document.show_units(kernel.prefs().values().general.default_units);
    *kernel.document_mut() = document;
    KernelReply::ok(&json!({ "revision": revision }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::commands::CamState;
    use crate::doc::Units;
    use serde_json::Value;

    fn kernel() -> Kernel {
        Kernel::with_cam(CamState::default())
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

    #[test]
    fn every_sample_opens_untitled_clean_and_ready_to_preview() {
        for (id, _) in SAMPLES {
            let mut kernel = kernel();
            run(&mut kernel, "samples.open", &json!({ "id": id }));
            let info = run(&mut kernel, "doc.info", &json!({}));
            assert!(info["path"].is_null(), "{id}: {info}");
            assert_eq!(info["dirty"], false, "{id}");
            assert!(!kernel.document().journal().can_undo(), "{id} has history");
            let toolpaths = run(&mut kernel, "cam.list", &json!({}));
            let toolpaths = toolpaths.as_array().expect("a list");
            assert!(!toolpaths.is_empty(), "{id} has toolpaths");
            for toolpath in toolpaths {
                assert_eq!(toolpath["stale"], false, "{id}: {toolpath}");
                assert!(
                    toolpath["moves"].as_u64().expect("moves") > 0,
                    "{id}: {toolpath}"
                );
                for name in [&toolpath["name"], &toolpath["toolName"]] {
                    let name = name.as_str().expect("a name");
                    assert!(!name.contains(" mm"), "{id} shows metric: {name}");
                }
            }
        }
    }

    #[test]
    fn a_sample_shows_the_preferred_units_with_its_sizes_kept() {
        let mut kernel = kernel();
        run(
            &mut kernel,
            "prefs.set",
            &json!({ "general": { "defaultUnits": "inch" } }),
        );
        run(&mut kernel, "samples.open", &json!({ "id": "house-sign" }));
        let job = kernel.document().job();
        assert_eq!(job.units, Units::Inch);
        assert_eq!(kernel.document().job_setup().units, Units::Inch);
        assert!((job.width - 16.0 * 25.4).abs() < 1e-9);
        assert!((job.height - 8.0 * 25.4).abs() < 1e-9);
        assert!((job.thickness - 0.75 * 25.4).abs() < 1e-9);
        assert_eq!(run(&mut kernel, "doc.info", &json!({}))["dirty"], false);
    }

    #[test]
    fn an_unknown_sample_is_refused_and_leaves_the_job_alone() {
        let mut kernel = kernel();
        let before = kernel.document().revision();
        assert!(kernel.apply("samples.open", r#"{"id":"nope"}"#).is_err());
        assert_eq!(kernel.document().revision(), before);
    }

    #[test]
    fn the_welcome_card_offers_exactly_these_samples() {
        let card = include_str!("../../../app/src/panels/Guide/samples.ts");
        for (id, _) in SAMPLES {
            assert!(card.contains(&format!("id: '{id}'")), "{id}");
        }
        assert_eq!(card.matches("id: '").count(), SAMPLES.len());
    }
}
