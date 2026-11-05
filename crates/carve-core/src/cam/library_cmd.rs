//! Tool, post and posting commands. The tool database and post library
//! live in [`CamState`] and run in [`crate::cam::commands`]; this module adds
//! what those commands need from the document and the preferences: the
//! active machine for tool lookups, the vector a form tool is drawn from and
//! the toolpaths, rotary setup and tiling a post writes.

use serde_json::{json, Value};

use crate::cam::commands::{self as cam_commands, CamState};
use crate::cam::toolpath::Toolpath;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;
use crate::prefs::Prefs;

/// Refuses to post a toolpath of the bottom side once the job is no longer
/// double sided: its moves are in the turned over material's job space, so
/// on a single sided job it would cut mirrored.
fn refuse_side_the_job_lacks(document: &Document, args: &str) -> Result<(), CoreError> {
    if document.job_setup().is_double_sided() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    let all_visible = value.get("outputAllVisible").and_then(Value::as_bool) == Some(true);
    let chosen: Vec<u64> = value
        .get("toolpathIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    let bottom = document.toolpaths().into_iter().find(|t| {
        t.side == crate::doc::Side::Bottom
            && if all_visible {
                t.visible
            } else {
                chosen.contains(&t.id)
            }
    });
    match bottom {
        Some(toolpath) => Err(CoreError::validation(format!(
            "{} cuts the bottom side of a double sided job; make the job double sided again in Job Setup to save it",
            toolpath.name
        ))),
        None => Ok(()),
    }
}

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "tools.formProfile" || cam_commands::handles(name)
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the tool database, post library or document reports for the rest.
pub fn dispatch(
    cam: &mut CamState,
    document: &Document,
    prefs: &Prefs,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    if name == "tools.formProfile" {
        return form_profile(document, args);
    }
    if matches!(name, "tools.list" | "tools.get" | "tools.previewName") {
        // Tools resolve their cutting data and names for the active
        // machine unless the caller names another.
        let text = if args.trim().is_empty() { "{}" } else { args };
        let mut value: Value =
            serde_json::from_str(text).map_err(|e| CoreError::invalid_args(name, e))?;
        if value.is_object() && value.get("machine").is_none_or(Value::is_null) {
            value["machine"] = json!(prefs.values().machines.active);
        }
        return cam_commands::dispatch(cam, name, &value.to_string());
    }
    if name == "cam.save" {
        refuse_side_the_job_lacks(document, args)?;
        // Posts write the document's toolpaths.
        let toolpaths: Vec<Toolpath> = document.toolpaths().into_iter().cloned().collect();
        cam.clear_toolpaths();
        for toolpath in toolpaths {
            cam.set_toolpath(toolpath);
        }
        // Tiling divides the job, which only the document knows.
        if let Ok(mut value) = serde_json::from_str::<Value>(args) {
            // An air or scrap cut is measured from the material surface of
            // the side being cut.
            let trial = value.get("trialCut").is_some_and(|t| !t.is_null());
            if trial && value.get("materialTop").is_none_or(Value::is_null) {
                let first = value
                    .get("toolpathIds")
                    .and_then(Value::as_array)
                    .and_then(|ids| ids.first())
                    .and_then(Value::as_u64);
                let side = document
                    .toolpaths()
                    .into_iter()
                    .find(|t| Some(t.id) == first)
                    .map_or(crate::doc::Side::Top, |t| t.side);
                value["materialTop"] = json!(document.job_setup().for_side(side).surface_z());
            }
            // A rotary job is wrapped around its blank as it is posted, by
            // a post that drives the rotary axis; through any other post it
            // comes out flat, as Save Toolpaths warns (indexed work).
            let setup = document.job_setup();
            let wraps = |id: &str| {
                cam.posts()
                    .get(id)
                    .is_ok_and(|post| post.wrap_word(setup.rotary.axis).is_some())
            };
            let rotary =
                setup.is_rotary() && value.get("post").and_then(Value::as_str).is_some_and(wraps);
            if rotary && value.is_object() {
                value["rotary"] = serde_json::to_value(document.job_setup().rotary)?;
            }
            // Output Tiled Toolpaths divides them as the Toolpath Tiling
            // form set up.
            if value.get("outputTiled").and_then(Value::as_bool) == Some(true) {
                let tiling = document.tiling();
                if !tiling.enabled {
                    return Err(CoreError::validation(
                        "toolpath tiling is off: set it up in Toolpath Tiling first",
                    ));
                }
                value["tiling"] = serde_json::to_value(tiling.spec)?;
            }
            if value.get("tiling").is_some_and(|t| !t.is_null()) {
                crate::cam::tiling_cmd::check_datum(document)?;
                let job = document.job_bounds();
                value["jobBounds"] = json!([job.min.x, job.min.y, job.max.x, job.max.y]);
                return cam_commands::dispatch(cam, name, &value.to_string());
            }
            if (rotary || trial) && value.is_object() {
                return cam_commands::dispatch(cam, name, &value.to_string());
            }
        }
    }
    cam_commands::dispatch(cam, name, args)
}

/// `tools.formProfile`: the half section of a form cutter from the
/// vector `id`, or the one selected vector.
fn form_profile(document: &Document, args: &str) -> Result<KernelReply, CoreError> {
    let text = if args.trim().is_empty() { "{}" } else { args };
    let value: Value =
        serde_json::from_str(text).map_err(|e| CoreError::invalid_args("tools.formProfile", e))?;
    let id = match value.get("id").and_then(Value::as_u64) {
        Some(id) => id,
        None => match document.selection() {
            [one] => *one,
            _ => {
                return Err(CoreError::validation(
                    "select the one vector that draws the right hand side of the cutter",
                ))
            }
        },
    };
    document.entity(id)?;
    let path = document
        .paths_of(id)
        .into_iter()
        .map(|(_, path)| path)
        .next()
        .ok_or_else(|| CoreError::validation("that object has no vector to use"))?;
    let profile = crate::cam::tool::form_profile_from(&path)?;
    KernelReply::ok(&json!({ "profile": profile }))
}
