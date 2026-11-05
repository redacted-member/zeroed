//! Toolpath preview commands: `cam.moves` draws the moves, `sim.*` cuts the
//! visible toolpaths into the stock.

use serde::Deserialize;
use serde_json::json;

use crate::cam::playback::{Budget, Playback};
use crate::cam::sim::{Quality, Stock};
use crate::cam::toolpath::{MoveKind, Point3};
use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::Point;
use crate::kernel::{parse_args, IdsArgs, KernelReply};

/// Largest material texture picture read, in bytes.
const MAX_TEXTURE_BYTES: u64 = 32 * 1024 * 1024;

/// `sim.texture {path}`: the picture file of a material texture, checked to
/// be a picture this build reads, as a blob with its type for the view to
/// decode.
fn texture_blob(args: &str) -> Result<KernelReply, CoreError> {
    #[derive(Deserialize)]
    struct Args {
        path: std::path::PathBuf,
    }
    let args: Args =
        serde_json::from_str(args).map_err(|e| CoreError::invalid_args("sim.texture", e))?;
    let size = std::fs::metadata(&args.path)
        .map_err(|e| CoreError::io(args.path.display(), e))?
        .len();
    if size > MAX_TEXTURE_BYTES {
        return Err(CoreError::validation(
            "the texture picture is larger than 32 MB",
        ));
    }
    let bytes = std::fs::read(&args.path).map_err(|e| CoreError::io(args.path.display(), e))?;
    let format = image::guess_format(&bytes)
        .map_err(|_| CoreError::validation("the texture file is not a picture"))?;
    let mime = format.to_mime_type();
    let header = serde_json::to_vec(&json!({
        "mime": mime,
        "sections": [{ "name": "bytes", "dtype": "u8", "offset": 0, "count": bytes.len() }],
    }))?;
    let mut out = Vec::with_capacity(4 + header.len() + bytes.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&bytes);
    Ok(KernelReply::Bytes(out))
}

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "sim.run"
            | "sim.reset"
            | "sim.heightfield"
            | "cam.moves"
            | "sim.texture"
            | "sim.undo"
            | "sim.removeWaste"
            | "sim.check"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and whatever
/// the document reports for the rest.
pub fn dispatch(
    document: &Document,
    preview: &mut Option<Stock>,
    undo: &mut Option<Option<Stock>>,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "sim.run" => {
            let args: SimRunArgs = parse_args(name, args)?;
            let mut stock = continued(document, preview, undo, args.quality, args.keep);
            let chosen = args.ids.filter(|ids| !ids.is_empty());
            // The same sweep and checks as the animation, all at once.
            let mut run = Playback::new(document, |t| match &chosen {
                Some(ids) => ids.contains(&t.id),
                None => t.visible,
            });
            let collisions = run.step(document, &mut stock, Budget::All)?.collisions;
            let seconds = run.seconds();
            let reply = json!({
                "width": stock.width,
                "height": stock.height,
                "cellSize": stock.cell,
                "collisions": collisions,
                "seconds": seconds,
            });
            *preview = Some(stock);
            KernelReply::ok(&reply)
        }
        "sim.texture" => texture_blob(args),
        "sim.reset" => {
            *preview = None;
            *undo = None;
            KernelReply::ok(&json!({}))
        }
        "sim.undo" => match undo.take() {
            Some(before) => {
                *preview = before;
                KernelReply::ok(&json!({ "restored": true, "empty": preview.is_none() }))
            }
            None => KernelReply::ok(&json!({ "restored": false, "empty": preview.is_none() })),
        },
        "sim.removeWaste" => {
            let args: WasteArgs = parse_args(name, args)?;
            let stock = preview.as_mut().ok_or_else(|| {
                CoreError::validation("run a toolpath preview before removing its waste")
            })?;
            let before = stock.clone();
            let removed = stock.remove_piece(args.x, args.y);
            if removed > 0 {
                *undo = Some(Some(before));
            }
            KernelReply::ok(&json!({ "removed": removed }))
        }
        "sim.check" => {
            let args: CheckArgs = parse_args(name, args)?;
            if !args.tolerance.is_finite() || args.tolerance < 0.0 {
                return Err(CoreError::validation(
                    "the check tolerance must be zero or more",
                ));
            }
            let stock = preview.as_ref().ok_or_else(|| {
                CoreError::validation("run a toolpath preview before checking it")
            })?;
            let report = crate::cam::deviation::check(document, stock, args.tolerance);
            let header = json!({
                "width": stock.width,
                "height": stock.height,
                "tolerance": args.tolerance,
                "issues": report.issues,
                "found": report.found,
            });
            Ok(KernelReply::Bytes(crate::blob::frame(
                header,
                &[crate::blob::Section::u8("check", report.map)],
            )))
        }
        "sim.heightfield" => {
            let stock = preview.as_ref().ok_or_else(|| {
                CoreError::validation("run a toolpath preview before asking for its result")
            })?;
            Ok(KernelReply::Bytes(stock.to_blob(json!({
                "revision": document.revision(),
            }))))
        }
        "cam.moves" => {
            let args: IdsArgs = parse_args(name, args)?;
            Ok(KernelReply::Bytes(toolpath_moves(
                document,
                args.ids.as_deref(),
            )))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The binary preview of toolpath moves: every position the tool passes
/// through (arcs broken into short steps), with the kind of move that
/// arrives there. Visible toolpaths when no ids are given.
fn toolpath_moves(document: &Document, ids: Option<&[u64]>) -> Vec<u8> {
    let mut xyz: Vec<f32> = Vec::new();
    let mut kinds: Vec<u8> = Vec::new();
    let mut widths: Vec<f32> = Vec::new();
    let mut entries = Vec::new();
    let tile = crate::cam::tiling_cmd::active_tile(document);
    for toolpath in document.toolpaths() {
        let wanted = match ids {
            Some(list) if !list.is_empty() => list.contains(&toolpath.id),
            _ => toolpath.visible,
        };
        // The other side's toolpaths are in that side's job space.
        let other_side =
            toolpath.side != document.active_side() && ids.is_none_or(|list| list.is_empty());
        // Only the active sheet's toolpaths are drawn on it.
        let other_sheet = toolpath.sheet != 0
            && toolpath.sheet != document.active_sheet()
            && ids.is_none_or(|list| list.is_empty());
        if !wanted || other_side || other_sheet {
            continue;
        }
        // With tiling on, the views show what the active tile cuts.
        let tiled;
        let toolpath = match tile {
            Some(tile) => match crate::cam::tiling::clip(toolpath, &tile) {
                Some(clipped) => {
                    tiled = clipped;
                    &tiled
                }
                None => continue,
            },
            None => toolpath,
        };
        let offset = kinds.len();
        // Width the tool cuts at each point, at the material surface of
        // the toolpath's side: what a solid 2D preview fills.
        let surface = document.job_setup().for_side(toolpath.side).surface_z();
        let tool = &toolpath.tool.tool_type;
        let mut push = |p: Point3, kind: u8| {
            xyz.extend([p.x as f32, p.y as f32, p.z as f32]);
            kinds.push(kind);
            let depth = surface - p.z;
            let width = if depth > 0.0 {
                2.0 * tool.radius_at_height(depth)
            } else {
                0.0
            };
            widths.push(width as f32);
        };
        push(toolpath.home, MOVE_RAPID);
        let classes = crate::cam::move_class::classify(toolpath);
        for ((from, one), class) in toolpath.iter_moves().zip(classes) {
            use crate::cam::move_class::MoveClass;
            match one.kind {
                MoveKind::Rapid | MoveKind::Feed | MoveKind::Plunge | MoveKind::Dwell { .. } => {
                    push(
                        one.to,
                        match class {
                            MoveClass::Rapid => MOVE_RAPID,
                            MoveClass::Plunge => MOVE_PLUNGE,
                            MoveClass::Dwell => MOVE_DWELL,
                            MoveClass::Retract => MOVE_RETRACT,
                            MoveClass::Join => MOVE_JOIN,
                            MoveClass::Feed | MoveClass::Arc => MOVE_FEED,
                        },
                    )
                }
                MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                    let ccw = matches!(one.kind, MoveKind::ArcCcw { .. });
                    for p in arc_steps(from, one.to, centre, ccw) {
                        push(p, MOVE_ARC);
                    }
                }
            }
        }
        entries.push(json!({
            "id": toolpath.id,
            "offset": offset,
            "count": kinds.len() - offset,
            "visible": toolpath.visible,
        }));
    }
    let header = json!({
        "revision": document.revision(),
        "toolpaths": entries,
        "sections": [
            { "name": "xyz", "dtype": "f32", "offset": 0, "count": xyz.len() },
            { "name": "width", "dtype": "f32", "offset": xyz.len() * 4, "count": widths.len() },
            { "name": "kind", "dtype": "u8", "offset": xyz.len() * 4 + widths.len() * 4, "count": kinds.len() },
        ],
    });
    let header = header.to_string().into_bytes();
    let mut out =
        Vec::with_capacity(4 + header.len() + (xyz.len() + widths.len()) * 4 + kinds.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    for value in xyz.iter().chain(&widths) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&kinds);
    out
}

/// Kinds of move in the `cam.moves` preview.
pub(crate) const MOVE_RAPID: u8 = 0;
pub(crate) const MOVE_FEED: u8 = 1;
pub(crate) const MOVE_PLUNGE: u8 = 2;
pub(crate) const MOVE_ARC: u8 = 3;
pub(crate) const MOVE_DWELL: u8 = 4;
pub(crate) const MOVE_RETRACT: u8 = 5;
pub(crate) const MOVE_JOIN: u8 = 6;

/// Points along an arc move for drawing, about every five degrees, with Z
/// changing evenly for a helix. The last point is the move's end.
fn arc_steps(from: Point3, to: Point3, centre: Point, ccw: bool) -> Vec<Point3> {
    let a0 = (from.y - centre.y).atan2(from.x - centre.x);
    let a1 = (to.y - centre.y).atan2(to.x - centre.x);
    let tau = std::f64::consts::TAU;
    let mut sweep = a1 - a0;
    if ccw {
        while sweep <= 1e-12 {
            sweep += tau;
        }
    } else {
        while sweep >= -1e-12 {
            sweep -= tau;
        }
    }
    let radius = ((from.x - centre.x).powi(2) + (from.y - centre.y).powi(2)).sqrt();
    let steps = ((sweep.abs() / 5f64.to_radians()).ceil() as usize).clamp(1, 720);
    (1..=steps)
        .map(|i| {
            if i == steps {
                return to;
            }
            let t = i as f64 / steps as f64;
            let a = a0 + sweep * t;
            Point3::new(
                centre.x + radius * a.cos(),
                centre.y + radius * a.sin(),
                from.z + (to.z - from.z) * t,
            )
        })
        .collect()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SimRunArgs {
    ids: Option<Vec<u64>>,
    quality: Quality,
    /// Cut into the preview already shown (Preview Selected Toolpath)
    /// rather than into fresh material.
    #[serde(default)]
    keep: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CheckArgs {
    /// How far off the design, millimetres, the stock may be.
    tolerance: f64,
}

impl Default for CheckArgs {
    fn default() -> Self {
        CheckArgs { tolerance: 0.1 }
    }
}

#[derive(Debug, Deserialize)]
struct WasteArgs {
    x: f64,
    y: f64,
}

/// The stock a new preview cuts into: the one shown when `keep` asks for it
/// and it covers the same cells, otherwise fresh material. What was shown
/// goes to `undo`, so Undo Last can bring it back.
pub fn continued(
    document: &Document,
    preview: &mut Option<Stock>,
    undo: &mut Option<Option<Stock>>,
    quality: Quality,
    keep: bool,
) -> Stock {
    let fresh = Stock::new(document.job(), quality);
    let shown = preview.take();
    let stock = match &shown {
        Some(stock) if keep && stock.same_grid(&fresh) => stock.clone(),
        _ => fresh,
    };
    *undo = Some(shown);
    stock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_answers_its_issues_and_a_code_a_cell() {
        let doc = Document::new();
        let (mut preview, mut undo) = (None, None);
        assert!(dispatch(&doc, &mut preview, &mut undo, "sim.check", "{}").is_err());
        dispatch(&doc, &mut preview, &mut undo, "sim.run", "{}").expect("runs");
        assert!(dispatch(
            &doc,
            &mut preview,
            &mut undo,
            "sim.check",
            r#"{"tolerance":-1}"#
        )
        .is_err());
        let reply = dispatch(&doc, &mut preview, &mut undo, "sim.check", "{}").expect("checks");
        let KernelReply::Bytes(bytes) = reply else {
            panic!("a blob");
        };
        let size = u32::from_le_bytes(bytes[..4].try_into().expect("length")) as usize;
        let header: serde_json::Value =
            serde_json::from_slice(&bytes[4..4 + size]).expect("header");
        assert_eq!(header["issues"], json!([]));
        assert_eq!(header["tolerance"], json!(0.1));
        let stock = preview.as_ref().expect("stock");
        let cells = stock.width * stock.height;
        assert_eq!(header["sections"][0]["count"], json!(cells));
    }

    #[test]
    fn previews_add_up_and_undo_last_goes_back_one() {
        let doc = Document::new();
        let mut preview = None;
        let mut undo = None;
        let quality = Quality::Standard;
        // Undo with nothing to undo leaves nothing.
        dispatch(&doc, &mut preview, &mut undo, "sim.undo", "{}").expect("undo");
        assert!(preview.is_none());
        let first = continued(&doc, &mut preview, &mut undo, quality, true);
        preview = Some(first);
        assert_eq!(undo, Some(None));
        // A kept preview cuts into what is shown.
        if let Some(stock) = preview.as_mut() {
            stock.z[0] = -1.0;
        }
        let kept = continued(&doc, &mut preview, &mut undo, quality, true);
        assert_eq!(kept.z[0], -1.0);
        preview = Some(kept);
        let fresh = continued(&doc, &mut preview, &mut undo, quality, false);
        assert_ne!(fresh.z[0], -1.0);
        preview = Some(fresh);
        dispatch(&doc, &mut preview, &mut undo, "sim.undo", "{}").expect("undo");
        assert_eq!(preview.as_ref().map(|s| s.z[0]), Some(-1.0));
        // Only one step back.
        dispatch(&doc, &mut preview, &mut undo, "sim.undo", "{}").expect("undo");
        assert_eq!(preview.as_ref().map(|s| s.z[0]), Some(-1.0));
        // Nothing cut free: no waste to remove, nothing to undo.
        let reply = dispatch(
            &doc,
            &mut preview,
            &mut undo,
            "sim.removeWaste",
            r#"{"x":10,"y":10}"#,
        )
        .expect("waste");
        assert!(matches!(reply, KernelReply::Json(ref text) if text.contains(r#""removed":0"#)));
        assert!(undo.is_none());
        dispatch(&doc, &mut preview, &mut undo, "sim.reset", "{}").expect("reset");
        assert!(preview.is_none() && undo.is_none());
    }

    #[test]
    fn arc_moves_are_drawn_as_short_steps_ending_on_the_arc_end() {
        let steps = arc_steps(
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(-10.0, 0.0, -2.0),
            Point::ORIGIN,
            true,
        );
        assert!(steps.len() >= 30);
        assert_eq!(steps.last().copied(), Some(Point3::new(-10.0, 0.0, -2.0)));
        assert!(steps
            .iter()
            .all(|p| ((p.x * p.x + p.y * p.y).sqrt() - 10.0).abs() < 1e-9));
        assert!(
            steps[steps.len() / 2].y > 0.0,
            "counter clockwise goes over the top"
        );
    }
}
