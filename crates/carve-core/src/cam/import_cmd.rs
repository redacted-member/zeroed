//! Import Toolpaths commands: `cam.importToolpaths` reads a G-code file
//! into toolpaths (one undo step), `cam.imported.get` answers an imported
//! toolpath's placement and feeds, and `cam.imported.edit` moves, deepens,
//! renames or refeeds it (one undo step). Imported toolpaths are never
//! recalculated; see [`super::gcode_import`].

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use crate::cam::gcode_import::{self, reposition};
use crate::cam::tool::Tool;
use crate::cam::toolpath::StrategyKind;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::{edit, parse_args, IdArgs, KernelReply};

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "cam.importToolpaths" | "cam.imported.get" | "cam.imported.edit"
    )
}

#[derive(Debug, Deserialize)]
struct ImportArgs {
    path: PathBuf,
}

/// Changes to an imported toolpath; absent fields stay.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    /// Where the toolpath sits, as a move from where the file put it, mm.
    #[serde(default)]
    offset_x: Option<f64>,
    #[serde(default)]
    offset_y: Option<f64>,
    /// How far the cuts sit below where the file put them, mm.
    #[serde(default)]
    start_depth: Option<f64>,
    /// Feed, plunge (mm/min) and spindle (rpm).
    #[serde(default)]
    feed: Option<f64>,
    #[serde(default)]
    plunge: Option<f64>,
    #[serde(default)]
    spindle: Option<f64>,
    /// The tool the preview cuts with.
    #[serde(default)]
    tool: Option<Tool>,
}

fn number(value: &serde_json::Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse,
/// [`CoreError::Io`] for a file that cannot be read and
/// [`CoreError::Validation`] for a file with no moves, a toolpath that was
/// not imported or values that are not usable.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "cam.importToolpaths" => {
            let args: ImportArgs = parse_args(name, args)?;
            let bytes =
                std::fs::read(&args.path).map_err(|e| CoreError::io(args.path.display(), e))?;
            let text = String::from_utf8_lossy(&bytes);
            let file = args.path.to_string_lossy();
            let read = gcode_import::read(&text, &file)?;
            let warnings = read.warnings;
            edit(document, "Import Toolpaths", |doc| {
                let ids: Vec<u64> = read
                    .toolpaths
                    .into_iter()
                    .map(|t| doc.add_toolpath(t))
                    .collect();
                Ok(json!({ "ids": ids, "warnings": warnings }))
            })
        }
        "cam.imported.get" => {
            let args: IdArgs = parse_args(name, args)?;
            let toolpath = document.toolpath(args.id)?;
            let imported = toolpath
                .params_json
                .get("imported")
                .filter(|_| toolpath.kind == StrategyKind::Imported)
                .ok_or_else(|| CoreError::validation("that toolpath was not imported"))?;
            KernelReply::ok(&json!({
                "id": toolpath.id,
                "name": toolpath.name,
                "file": imported["file"],
                "offsetX": number(&imported["offsetX"]),
                "offsetY": number(&imported["offsetY"]),
                "startDepth": number(&imported["startDepth"]),
                "feed": toolpath.feed,
                "plunge": toolpath.plunge,
                "spindle": toolpath.spindle,
                "tool": toolpath.tool,
            }))
        }
        "cam.imported.edit" => {
            let args: EditArgs = parse_args(name, args)?;
            let toolpath = document.toolpath(args.id)?;
            let imported = toolpath
                .params_json
                .get("imported")
                .filter(|_| toolpath.kind == StrategyKind::Imported)
                .ok_or_else(|| CoreError::validation("that toolpath was not imported"))?;
            let dx = args
                .offset_x
                .map_or(0.0, |x| x - number(&imported["offsetX"]));
            let dy = args
                .offset_y
                .map_or(0.0, |y| y - number(&imported["offsetY"]));
            let dz = args
                .start_depth
                .map_or(0.0, |d| d - number(&imported["startDepth"]));
            if ![dx, dy, dz].iter().all(|v| v.is_finite()) {
                return Err(CoreError::validation("the position must be numbers"));
            }
            for rate in [args.feed, args.plunge, args.spindle].into_iter().flatten() {
                if !(rate.is_finite() && rate > 0.0) {
                    return Err(CoreError::validation(
                        "feeds and the spindle speed must be above zero",
                    ));
                }
            }
            if let Some(tool) = &args.tool {
                tool.validate()?;
            }
            let label = "Edit Toolpath";
            edit(document, label, |doc| {
                doc.update_toolpath(args.id, |t| {
                    if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
                        t.name = name.trim().to_string();
                    }
                    if let Some(feed) = args.feed {
                        t.feed = feed;
                    }
                    if let Some(plunge) = args.plunge {
                        t.plunge = plunge;
                    }
                    if let Some(spindle) = args.spindle {
                        t.spindle = spindle;
                    }
                    if let Some(tool) = args.tool {
                        t.tool_number = tool.number;
                        t.tool = tool;
                    }
                    reposition(t, dx, dy, dz);
                })?;
                Ok(json!({ "id": args.id }))
            })
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// True for a toolpath read from a file, which recalculating leaves alone.
pub fn is_imported(document: &Document, id: u64) -> bool {
    document
        .toolpath(id)
        .is_ok_and(|t| t.kind == StrategyKind::Imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(doc: &mut Document, name: &str, args: serde_json::Value) -> serde_json::Value {
        match dispatch(doc, name, &args.to_string()).expect(name) {
            KernelReply::Json(text) => {
                serde_json::from_str::<serde_json::Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(_) => serde_json::Value::Null,
        }
    }

    #[test]
    fn a_file_imports_as_toolpaths_that_move_and_deepen() {
        let temp = crate::test_support::TempDir::new("import-toolpaths");
        let path = temp.path().join("part.nc");
        std::fs::write(
            &path,
            "(T1 = End Mill 3 mm)\nG21 G90\nT1 M6\nS12000 M3\nG0 X5 Y5 Z5\nG1 Z-2 F300\nG1 X25 F1200\nG0 Z5\n",
        )
        .expect("writes");
        let mut doc = Document::new();
        let reply = run(&mut doc, "cam.importToolpaths", json!({ "path": path }));
        let id = reply["ids"][0].as_u64().expect("an id");
        assert!(is_imported(&doc, id));
        let got = run(&mut doc, "cam.imported.get", json!({ "id": id }));
        assert_eq!(got["name"], "part");
        assert_eq!(got["feed"], 1200.0);
        assert_eq!(got["spindle"], 12000.0);
        run(
            &mut doc,
            "cam.imported.edit",
            json!({ "id": id, "offsetX": 100.0, "offsetY": 50.0, "startDepth": 1.0, "feed": 900.0 }),
        );
        let t = doc.toolpath(id).expect("toolpath");
        assert_eq!(t.moves[2].to.x, 125.0);
        assert_eq!(t.moves[1].to.z, -3.0);
        assert_eq!(t.feed, 900.0);
        // Setting the same place again moves nothing further.
        run(
            &mut doc,
            "cam.imported.edit",
            json!({ "id": id, "offsetX": 100.0, "offsetY": 50.0, "startDepth": 1.0 }),
        );
        assert_eq!(doc.toolpath(id).expect("toolpath").moves[2].to.x, 125.0);
        doc.undo();
        doc.undo();
        assert_eq!(doc.toolpath(id).expect("toolpath").moves[2].to.x, 25.0);
        assert!(dispatch(
            &mut doc,
            "cam.imported.edit",
            &json!({ "id": id, "feed": 0.0 }).to_string()
        )
        .is_err());
        assert!(dispatch(
            &mut doc,
            "cam.importToolpaths",
            &json!({ "path": temp.path().join("missing.nc") }).to_string()
        )
        .is_err());
    }
}
