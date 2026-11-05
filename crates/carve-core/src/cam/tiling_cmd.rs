//! Toolpath Tiling commands: `cam.tiling.get` answers the document's tiling
//! and its tiles, `cam.tiling.set` changes it (one undo step) and
//! `cam.tiling.view` picks the tile the views show and where they draw it,
//! which is a view choice and not an undo step.
//!
//!, tiling needs the job's XY datum at the bottom left
//! corner: each tile's corner is then the datum the machine is zeroed on.
//! Turning tiling on, or writing tiled toolpaths, with any other datum is
//! refused.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cam::tiling::{self, Tile, Tiling};
use crate::doc::{bbox_array, Document, XyOrigin};
use crate::error::CoreError;
use crate::kernel::{edit, parse_args, KernelReply};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "cam.tiling.get" | "cam.tiling.set" | "cam.tiling.view"
    )
}

/// Refuses tiling unless the XY datum is at the job's bottom left corner.
///
/// # Errors
/// [`CoreError::Validation`] for any other datum position.
pub fn check_datum(document: &Document) -> Result<(), CoreError> {
    if document.job_setup().xy_origin == XyOrigin::BottomLeft {
        Ok(())
    } else {
        Err(CoreError::validation(
            "toolpath tiling needs the XY datum at the bottom left corner of the job; change it in Job Setup",
        ))
    }
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse and
/// [`CoreError::Validation`] for sizes that make no tiles.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "cam.tiling.get" => KernelReply::ok(&state_json(document, document.tiling())?),
        "cam.tiling.set" => {
            let args: SetArgs = parse_args(name, args)?;
            let mut next = args.tiling;
            if next.enabled {
                check_datum(document)?;
            }
            let count = tiling::tiles(document.job_bounds(), &next.spec)?.len();
            next.active = next.active.min(count.saturating_sub(1));
            edit(document, "Toolpath Tiling", |doc| {
                doc.set_tiling(next);
                state_json(doc, &next)
            })
        }
        "cam.tiling.view" => {
            let args: ViewArgs = parse_args(name, args)?;
            let mut next = *document.tiling();
            let count = tiling::tiles(document.job_bounds(), &next.spec)?.len();
            if let Some(active) = args.active {
                if active >= count {
                    return Err(CoreError::validation(format!(
                        "there is no tile {}; the job has {count}",
                        active + 1
                    )));
                }
                next.active = active;
            }
            if let Some(original) = args.original_position {
                next.original_position = original;
            }
            document.set_tiling(next);
            let revision = document.bump();
            let mut reply = state_json(document, &next)?;
            reply["revision"] = json!(revision);
            KernelReply::ok(&reply)
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// What a toolpath preview cuts of `toolpath`: with tiling on, only what
/// the active tile cuts, in its place on the job (None when the tile cuts
/// none of it); otherwise the whole toolpath. A diamond drag Quick Engrave
/// previews the depth its line width marks (`engrave::preview_view`).
pub fn for_preview<'a>(
    document: &Document,
    toolpath: &'a crate::cam::toolpath::Toolpath,
) -> Option<std::borrow::Cow<'a, crate::cam::toolpath::Toolpath>> {
    let surface = document.job_setup().for_side(toolpath.side).surface_z();
    let seen = crate::cam::engrave::preview_view(toolpath, surface);
    match (active_tile(document), seen) {
        (Some(tile), seen) => tiling::clip(seen.as_ref().unwrap_or(toolpath), &tile.in_place())
            .map(std::borrow::Cow::Owned),
        (None, Some(seen)) => Some(std::borrow::Cow::Owned(seen)),
        (None, None) => Some(std::borrow::Cow::Borrowed(toolpath)),
    }
}

/// The tile the views show when tiling is on, if the settings make tiles.
pub fn active_tile(document: &Document) -> Option<Tile> {
    let tiling = document.tiling();
    if !tiling.enabled {
        return None;
    }
    let all = tiling::tiles(document.job_bounds(), &tiling.spec).ok()?;
    let tile = all.get(tiling.active).or_else(|| all.first()).copied()?;
    Some(if tiling.original_position {
        tile.in_place()
    } else {
        tile
    })
}

fn state_json(document: &Document, tiling: &Tiling) -> Result<Value, CoreError> {
    let tiles: Vec<Value> = tiling::tiles(document.job_bounds(), &tiling.spec)
        .unwrap_or_default()
        .iter()
        .map(|t| {
            json!({
                "row": t.row,
                "column": t.column,
                "nominal": bbox_array(t.nominal),
                "area": bbox_array(t.area),
                "shift": [t.shift.0, t.shift.1],
            })
        })
        .collect();
    let mut value = serde_json::to_value(tiling)?;
    value["tiles"] = json!(tiles);
    Ok(value)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetArgs {
    tiling: Tiling,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewArgs {
    #[serde(default)]
    active: Option<usize>,
    #[serde(default)]
    original_position: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tiling::{TileSpec, TilingMethod};

    fn call(doc: &mut Document, name: &str, args: Value) -> Result<Value, CoreError> {
        match dispatch(doc, name, &args.to_string())? {
            KernelReply::Json(text) => {
                let envelope: Value = serde_json::from_str(&text)?;
                Ok(envelope["result"].clone())
            }
            KernelReply::Bytes(_) => Err(CoreError::validation("bytes")),
        }
    }

    #[test]
    fn tiling_is_set_as_one_undo_step_and_the_view_choice_is_not() {
        let mut doc = Document::new();
        let off = call(&mut doc, "cam.tiling.get", json!({})).expect("get");
        assert_eq!(off["enabled"], false);
        let tiling = Tiling {
            enabled: true,
            spec: TileSpec {
                method: TilingMethod::FeedX,
                tile_width: 200.0,
                tile_height: 0.0,
                overlap: 5.0,
                smallest_first: false,
            },
            active: 9,
            original_position: false,
        };
        let set = call(&mut doc, "cam.tiling.set", json!({ "tiling": tiling })).expect("set");
        let tiles = set["tiles"].as_array().expect("tiles");
        assert!(tiles.len() > 1, "{set}");
        assert_eq!(set["active"], tiles.len() - 1, "clamped to the last tile");
        assert_eq!(set["spec"]["method"], "feedX");
        let revision = doc.revision();
        let view = call(
            &mut doc,
            "cam.tiling.view",
            json!({ "active": 0, "originalPosition": true }),
        )
        .expect("view");
        assert_eq!(view["active"], 0);
        assert!(doc.revision() > revision);
        assert!(active_tile(&doc).is_some_and(|t| t.shift == (0.0, 0.0)));
        assert!(call(&mut doc, "cam.tiling.view", json!({ "active": 99 })).is_err());
        assert_eq!(doc.undo().as_deref(), Some("Toolpath Tiling"));
        assert!(!doc.tiling().enabled);
        assert!(active_tile(&doc).is_none());
        let bad = Tiling {
            spec: TileSpec {
                tile_width: 10.0,
                overlap: 20.0,
                ..TileSpec::default()
            },
            ..Tiling::default()
        };
        assert!(call(&mut doc, "cam.tiling.set", json!({ "tiling": bad })).is_err());
    }

    #[test]
    fn tiling_needs_the_datum_at_the_bottom_left() {
        let mut doc = Document::new();
        let mut job = *doc.job_setup();
        job.xy_origin = XyOrigin::Centre;
        doc.set_job(job).expect("job");
        let on = Tiling {
            enabled: true,
            ..Tiling::default()
        };
        let error = call(&mut doc, "cam.tiling.set", json!({ "tiling": on })).expect_err("centre");
        assert!(error.to_string().contains("bottom left"), "{error}");
        // Setting it up while off is allowed.
        let off = Tiling {
            enabled: false,
            ..Tiling::default()
        };
        call(&mut doc, "cam.tiling.set", json!({ "tiling": off })).expect("off");
        job.xy_origin = XyOrigin::BottomLeft;
        job.offset_x = 5.0;
        doc.set_job(job).expect("job");
        call(&mut doc, "cam.tiling.set", json!({ "tiling": on })).expect("bottom left");
    }

    #[test]
    fn tiling_is_saved_with_the_document() {
        let mut doc = Document::new();
        let mut tiling = Tiling {
            enabled: true,
            ..Tiling::default()
        };
        tiling.spec.overlap = 3.0;
        call(&mut doc, "cam.tiling.set", json!({ "tiling": tiling })).expect("set");
        let file = serde_json::to_value(doc.to_file()).expect("file");
        assert_eq!(file["tiling"]["spec"]["overlap"], 3.0);
        let back: crate::doc::DocumentFile = serde_json::from_value(file).expect("reads");
        let reopened = Document::from_file(back, 0).expect("opens");
        assert_eq!(reopened.tiling(), doc.tiling());
        let plain = serde_json::to_value(Document::new().to_file()).expect("file");
        assert!(plain.get("tiling").is_none(), "left out until it is set");
    }
}
