//! Animated toolpath preview: the stock is cut a little at a time, so the
//! view can show the material being removed with the tool drawn where it is.
//!
//! `sim.start` makes fresh stock and the list of toolpaths to cut;
//! `sim.step` sweeps the tool on by a distance (or one move, or to the end
//! of the toolpath or of the preview) and answers where the tool is;
//! `sim.patch` answers the cells changed since the last patch, so the view
//! need not fetch the whole stock every frame; `sim.advance` does both in
//! one call, the step's answer in the patch's header. The finished stock is
//! the same one `sim.heightfield` answers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;

use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

use super::collide::{Clamps, Guard, IssueKind};
use super::sim::{
    flipped, point_on, Burn, Collision, Cutter, Footprint, MovePart, Quality, Stock, Sweep,
};
use super::toolpath::{MoveKind, Point3, Toolpath};

/// Events of one kind this many moves apart or closer are one event.
const MERGE_MOVES: usize = 6;

/// Commands this module answers.
pub fn handles(name: &str) -> bool {
    matches!(name, "sim.start" | "sim.step" | "sim.patch" | "sim.advance")
}

/// Where an animated preview has got to.
#[derive(Debug, Clone)]
pub struct Playback {
    /// Toolpath ids in cutting order with the owner tag of each.
    queue: Vec<(u64, u16)>,
    /// Position in `queue`.
    current: usize,
    /// Move being cut in the current toolpath, and how far along it.
    move_index: usize,
    fraction: f64,
    /// The current toolpath as the stock sees it (flipped for the other side),
    /// its tool laid out for sweeping, whether it cuts from underneath and
    /// its collision checks and, for a laser, the marks it leaves, prepared
    /// when the toolpath starts.
    prepared: Option<(Toolpath, Cutter, bool, Guard, Option<Burn>)>,
    /// The job's clamps on the stock's cells, found when the first
    /// toolpath starts, and the other side's for its toolpaths.
    clamps: Option<Option<Arc<Clamps>>>,
    other_clamps: Option<Option<Arc<Clamps>>>,
    /// The events found so far, each kind once a move.
    seen: HashSet<(u64, usize, IssueKind)>,
    /// The last move of each toolpath each kind of event was found on: an
    /// event within [`MERGE_MOVES`] moves of it goes on from it and is not
    /// reported again.
    last_event: HashMap<(u64, IssueKind), usize>,
    /// The sweep under way, which notes the tiles changed since the last
    /// patch (none until the first toolpath starts).
    sweep: Sweep,
    /// Estimated time of the toolpaths finished so far, seconds.
    seconds: f64,
    /// Total and done length of every move, millimetres, for progress.
    total: f64,
    done: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    #[serde(default)]
    quality: Quality,
    /// Toolpaths to preview; the visible ones when absent or empty.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Every calculated toolpath, visible or not.
    #[serde(default)]
    all: bool,
    /// Cut into the preview already shown (Preview Selected Toolpath).
    #[serde(default)]
    keep: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepArgs {
    /// How far to move the tool along the toolpaths, millimetres.
    #[serde(default)]
    distance: Option<f64>,
    /// Cut exactly one more move (the rest of the current one).
    #[serde(default)]
    one_move: bool,
    /// Cut to the end of the current toolpath.
    #[serde(default)]
    to_toolpath_end: bool,
    /// Cut up to the next retract.
    #[serde(default)]
    to_retract: bool,
    /// Cut everything left.
    #[serde(default)]
    to_end: bool,
    /// Cut to the point of a move nearest a job point.
    #[serde(default)]
    to_move: Option<ToMoveArgs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToMoveArgs {
    toolpath_id: u64,
    move_index: usize,
    x: f64,
    y: f64,
    z: f64,
}

/// How far to go in one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Budget {
    /// Along the moves by this many millimetres.
    Distance(f64),
    /// To the end of the move being cut.
    OneMove,
    /// To the end of the toolpath being cut.
    Toolpath,
    /// Up to and including the next move that lifts the tool out (Run to
    /// Retract), or the end of the toolpath.
    Retract,
    /// To the point of a move nearest a job point: into toolpath `id`,
    /// move `index`, stopping where the move passes closest to `at`.
    ToMove {
        /// Toolpath id.
        id: u64,
        /// Move index in the toolpath.
        index: usize,
        /// The point to stop nearest.
        at: Point3,
    },
    /// To the end of the preview.
    All,
}

/// What a step answers.
#[derive(Debug, Clone, PartialEq)]
pub struct StepResult {
    /// True when every toolpath has been cut.
    pub done: bool,
    /// Where the tool tip is and which toolpath it is cutting.
    pub tool: Option<(Point3, u64)>,
    /// Rapids into the material and the other events the collision checks
    /// found during the step.
    pub collisions: Vec<Collision>,
}

impl Playback {
    /// A preview of `wanted` toolpaths of `doc`, in list order.
    pub fn new(doc: &Document, wanted: impl Fn(&Toolpath) -> bool) -> Playback {
        let mut queue = Vec::new();
        let mut total = 0.0;
        for (position, toolpath) in doc.toolpaths().into_iter().enumerate() {
            if wanted(toolpath) {
                let owner = u16::try_from(position + 1).unwrap_or(u16::MAX);
                queue.push((toolpath.id, owner));
                let Some(cut) = crate::cam::tiling_cmd::for_preview(doc, toolpath) else {
                    continue;
                };
                total += cut
                    .iter_moves()
                    .map(|(from, m)| m.length(from))
                    .sum::<f64>();
            }
        }
        Playback {
            queue,
            current: 0,
            move_index: 0,
            fraction: 0.0,
            prepared: None,
            clamps: None,
            other_clamps: None,
            seen: HashSet::new(),
            last_event: HashMap::new(),
            sweep: Sweep::default(),
            seconds: 0.0,
            total,
            done: 0.0,
        }
    }

    /// Fraction of the whole preview cut, 0 to 1.
    pub fn progress(&self) -> f64 {
        if self.current >= self.queue.len() {
            1.0
        } else if self.total > 0.0 {
            (self.done / self.total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Estimated machining time of the toolpaths finished so far.
    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    /// Sweeps the tool on through `stock` by `budget`.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when a toolpath's tool cannot be
    /// rasterised.
    pub fn step(
        &mut self,
        doc: &Document,
        stock: &mut Stock,
        budget: Budget,
    ) -> Result<StepResult, CoreError> {
        let mut left = match budget {
            Budget::Distance(d) => d.max(0.0),
            _ => f64::INFINITY,
        };
        let mut collisions = Vec::new();
        let mut tool = None;
        while self.current < self.queue.len() {
            let (id, owner) = self.queue[self.current];
            if self.prepared.is_none() {
                let Ok(whole) = doc.toolpath(id) else {
                    self.next_toolpath(0.0);
                    continue;
                };
                // With tiling on, the preview cuts the active tile alone.
                let Some(original) = crate::cam::tiling_cmd::for_preview(doc, whole) else {
                    self.next_toolpath(0.0);
                    continue;
                };
                let setup = doc.job_setup();
                let (toolpath, below) = if original.side == doc.active_side() {
                    (original.into_owned(), false)
                } else if setup.is_double_sided() {
                    (flipped(&original, setup), true)
                } else {
                    // The other side of a job that is no longer double sided.
                    self.next_toolpath(0.0);
                    continue;
                };
                let cutter = Cutter::new(&Footprint::of(&toolpath.tool, stock.cell)?);
                let clamps = if below {
                    self.other_clamps
                        .get_or_insert_with(|| Clamps::of_other_side(doc, stock).map(Arc::new))
                        .clone()
                } else {
                    self.clamps
                        .get_or_insert_with(|| Clamps::of(doc, stock).map(Arc::new))
                        .clone()
                };
                let guard = Guard::new(&toolpath.tool, stock.cell, clamps, toolpath.safe_z);
                // A laser chars the face it is over; the other side's laser
                // work is underneath, out of sight.
                let burn = if below { None } else { Burn::of(&toolpath) };
                self.prepared = Some((toolpath, cutter, below, guard, burn));
                if self.sweep == Sweep::default() {
                    self.sweep = Sweep::tracking(stock);
                }
            }
            let Some((toolpath, cutter, below, guard, burn)) = self.prepared.as_ref() else {
                break;
            };
            let Some(one) = toolpath.moves.get(self.move_index) else {
                stock.finish_sweep(cutter, &mut self.sweep, owner, *below);
                let seconds = toolpath.estimated_seconds;
                self.next_toolpath(seconds);
                if matches!(budget, Budget::Toolpath | Budget::OneMove | Budget::Retract) {
                    break;
                }
                continue;
            };
            let from = if self.move_index == 0 {
                toolpath.home
            } else {
                toolpath.moves[self.move_index - 1].to
            };
            let length = one.length(from);
            let remaining = length * (1.0 - self.fraction);
            let target = match budget {
                Budget::ToMove {
                    id: goal,
                    index,
                    at,
                } if goal == id && index == self.move_index => {
                    Some(nearest_on(from, one, at).max(self.fraction))
                }
                _ => None,
            };
            let t1 = if let Some(t) = target {
                t
            } else if remaining <= left || length <= 0.0 {
                1.0
            } else {
                self.fraction + left / length
            };
            let part = MovePart {
                toolpath_id: id,
                index: self.move_index,
                from,
                one,
                t0: self.fraction,
                t1,
            };
            let mut found = Vec::new();
            if !guard.is_idle() {
                // The checks read the stock as cut up to here.
                stock.flush_sweep(cutter, &mut self.sweep, owner, *below);
            }
            guard.check(stock, &part, *below, &mut self.seen, &mut found);
            if let Some(burn) = burn {
                stock.burn_move(&mut self.sweep, &part, burn);
            } else if let Some(hit) = stock.cut_move(cutter, &mut self.sweep, &part, owner, *below)
            {
                if self.seen.insert((id, part.index, hit.kind)) {
                    found.push(hit);
                }
            }
            for hit in found {
                // Moves close together meeting the same thing (the lines of
                // one pass) are one event, reported where it starts.
                let before = self.last_event.insert((id, hit.kind), part.index);
                if before.is_none_or(|last| part.index > last + MERGE_MOVES) {
                    collisions.push(hit);
                }
            }
            let moved = length * (t1 - self.fraction);
            left -= moved;
            self.done += moved;
            tool = Some((point_on(from, one, t1).unwrap_or(one.to), id));
            if target.is_some() {
                // Stopped at the point asked for.
                if t1 >= 1.0 {
                    self.move_index += 1;
                    self.fraction = 0.0;
                } else {
                    self.fraction = t1;
                }
                break;
            }
            if t1 >= 1.0 {
                self.move_index += 1;
                self.fraction = 0.0;
                // A retract lifts the tool straight up, or rapids up.
                let lifts = one.to.z > from.z + 1e-9
                    && (one.kind == MoveKind::Rapid
                        || (one.to.x - from.x).hypot(one.to.y - from.y) < 1e-9);
                if budget == Budget::OneMove || (budget == Budget::Retract && lifts) {
                    break;
                }
            } else {
                self.fraction = t1;
                break;
            }
            if left <= 0.0 {
                break;
            }
        }
        // Every cut up to the tool shows before the step answers.
        if let Some((_, cutter, below, _, _)) = self.prepared.as_ref() {
            let owner = self.queue.get(self.current).map_or(0, |q| q.1);
            stock.finish_sweep(cutter, &mut self.sweep, owner, *below);
        }
        Ok(StepResult {
            done: self.current >= self.queue.len(),
            tool,
            collisions,
        })
    }

    fn next_toolpath(&mut self, seconds: f64) {
        self.seconds += seconds;
        self.current += 1;
        self.move_index = 0;
        self.fraction = 0.0;
        self.prepared = None;
        self.sweep.restart();
    }

    /// The cells changed since the last call as a blob like
    /// [`Stock::to_blob`] for those rectangles only (`rects` in the
    /// header); they are then clean.
    pub fn take_patch(&mut self, stock: &Stock) -> Vec<u8> {
        patch_blob(stock, &self.sweep.take_changed(), None)
    }
}

/// The fraction of `one`'s length (from `from`) where it passes nearest
/// `at`.
fn nearest_on(from: Point3, one: &super::toolpath::Move, at: Point3) -> f64 {
    const SAMPLES: usize = 200;
    (0..=SAMPLES)
        .map(|k| k as f64 / SAMPLES as f64)
        .filter_map(|t| point_on(from, one, t).map(|p| (t, p.distance(at))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(1.0, |(t, _)| t)
}

/// A step's answer as `sim.step` gives it.
fn step_json(result: &StepResult, run: &Playback) -> serde_json::Value {
    json!({
        "done": result.done,
        "tool": result.tool.map(|(p, id)| json!({
            "x": p.x, "y": p.y, "z": p.z, "toolpathId": id,
        })),
        "collisions": result.collisions,
        "progress": run.progress(),
        "seconds": run.seconds(),
    })
}

/// The budget a step's arguments ask for.
fn budget_of(args: &StepArgs) -> Result<Budget, CoreError> {
    Ok(if let Some(to) = &args.to_move {
        Budget::ToMove {
            id: to.toolpath_id,
            index: to.move_index,
            at: Point3::new(to.x, to.y, to.z),
        }
    } else if args.to_end {
        Budget::All
    } else if args.to_toolpath_end {
        Budget::Toolpath
    } else if args.to_retract {
        Budget::Retract
    } else if args.one_move {
        Budget::OneMove
    } else {
        let d = args.distance.unwrap_or(0.0);
        if !d.is_finite() || d < 0.0 {
            return Err(CoreError::validation(
                "the step distance must be zero or more",
            ));
        }
        Budget::Distance(d)
    })
}

/// The cells of `rects` (`[x0, y0, width, height]` each) of the stock as
/// a `BLOB` reply body with `rects` and, when given, `step` (a step's
/// answer) in the header: `z`, `owner` (u16), `floor` and `burn` (u8, once
/// a laser has charred the stock) sections, the
/// rectangles one after another row by row, cells cut through NaN in `z`
/// and `floor`. The `floor` section is left out when every one of those
/// cells has its underside uncut at the bottom. Heights go as `u16` steps
/// under [`Stock::height_code`] (named by `heightBase` and `heightStep` in
/// the header, as in `sim.heightfield`), as `f32` without one. No
/// rectangles when nothing changed.
fn patch_blob(stock: &Stock, rects: &[[usize; 4]], step: Option<serde_json::Value>) -> Vec<u8> {
    use crate::blob;
    let cells = || {
        rects.iter().flat_map(|&[x0, y0, w, h]| {
            (y0..y0 + h).flat_map(move |j| (x0..x0 + w).map(move |i| j * stock.width + i))
        })
    };
    let through = |i: usize| stock.floor[i] >= stock.z[i] - super::sim::THROUGH;
    let open = |values: &[f32]| -> Vec<f32> {
        cells()
            .map(|i| if through(i) { f32::NAN } else { values[i] })
            .collect()
    };
    let z = open(&stock.z);
    let bottom = stock.bottom as f32;
    let flat = cells().all(|i| stock.floor[i] == bottom && !through(i));
    let owner: Vec<u16> = cells().map(|i| stock.owner[i]).collect();
    let code = stock.height_code();
    let mut header = json!({ "rects": rects });
    if let Some(step) = step {
        header["step"] = step;
    }
    blob::describe_heights(&mut header, code.as_ref());
    let mut sections = vec![
        blob::heights("z", &z, code.as_ref()),
        blob::Section::u16("owner", &owner),
    ];
    if !flat {
        sections.push(blob::heights("floor", &open(&stock.floor), code.as_ref()));
    }
    // Laser marks, once a laser has charred the stock.
    if stock.burn.len() == stock.z.len() {
        sections.push(blob::Section::u8(
            "burn",
            cells().map(|i| stock.burn[i]).collect(),
        ));
    }
    blob::frame(header, &sections)
}

/// Samples up the tool outline, enough for a round ball nose tip.
const OUTLINE_STEPS: u32 = 32;

/// The outline of a tool as (radius, height above the tip) pairs from the
/// tip up to its flute length, or twice its diameter when that is not
/// known, for drawing it.
fn tool_outline(toolpath: &Toolpath) -> Vec<[f64; 2]> {
    let shape = &toolpath.tool.tool_type;
    let top = if toolpath.tool.flute_length > 0.0 {
        toolpath.tool.flute_length
    } else {
        (shape.diameter() * 2.0).max(shape.shoulder_height() + 1.0)
    };
    (0..=OUTLINE_STEPS)
        .map(|k| {
            let h = top * f64::from(k) / f64::from(OUTLINE_STEPS);
            [shape.radius_at_height(h), h]
        })
        .collect()
}

/// Runs one of this module's commands.
///
/// # Errors
/// Returns [`CoreError::UnknownCommand`] for another command,
/// [`CoreError::InvalidArgs`] for bad arguments and
/// [`CoreError::Validation`] when no preview is running.
pub fn dispatch(
    doc: &Document,
    stock: &mut Option<Stock>,
    undo: &mut Option<Option<Stock>>,
    playback: &mut Option<Playback>,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let text = match args.trim() {
        "" | "null" => "{}",
        other => other,
    };
    match name {
        "sim.start" => {
            let args: StartArgs =
                serde_json::from_str(text).map_err(|e| CoreError::invalid_args(name, e))?;
            let chosen = args.ids.filter(|ids| !ids.is_empty());
            let run = Playback::new(doc, |t| match &chosen {
                Some(ids) => ids.contains(&t.id),
                None => args.all || t.visible,
            });
            let fresh =
                crate::cam::preview_cmd::continued(doc, stock, undo, args.quality, args.keep);
            let toolpaths: Vec<_> = run
                .queue
                .iter()
                .filter_map(|(id, _)| doc.toolpath(*id).ok())
                .map(|t| {
                    // The shank as set for the checks (radius, bottom and
                    // top above the tip) and the holder when it is checked.
                    let tool = &t.tool;
                    let shank = (tool.flute_length > 0.0).then(|| {
                        let top = tool
                            .holder()
                            .map_or(tool.flute_length + tool.diameter() * 1.5, |h| h.1);
                        [
                            tool.shank_radius(),
                            tool.flute_length,
                            top.max(tool.flute_length),
                        ]
                    });
                    json!({
                        "id": t.id,
                        "name": t.name,
                        "tool": tool_outline(t),
                        "shank": shank,
                        "holder": tool.holder().map(|(r, bottom, top)| [r, bottom, top]),
                    })
                })
                .collect();
            let reply = json!({
                "width": fresh.width,
                "height": fresh.height,
                "cellSize": fresh.cell,
                "toolpaths": toolpaths,
                "revision": doc.revision(),
            });
            *stock = Some(fresh);
            *playback = Some(run);
            KernelReply::ok(&reply)
        }
        "sim.step" => {
            let args: StepArgs =
                serde_json::from_str(text).map_err(|e| CoreError::invalid_args(name, e))?;
            let (Some(run), Some(stock)) = (playback.as_mut(), stock.as_mut()) else {
                return Err(CoreError::validation("start a toolpath preview first"));
            };
            let result = run.step(doc, stock, budget_of(&args)?)?;
            KernelReply::ok(&step_json(&result, run))
        }
        "sim.advance" => {
            let args: StepArgs =
                serde_json::from_str(text).map_err(|e| CoreError::invalid_args(name, e))?;
            let (Some(run), Some(stock)) = (playback.as_mut(), stock.as_mut()) else {
                return Err(CoreError::validation("start a toolpath preview first"));
            };
            let result = run.step(doc, stock, budget_of(&args)?)?;
            let step = step_json(&result, run);
            let rects = run.sweep.take_changed();
            Ok(KernelReply::Bytes(patch_blob(stock, &rects, Some(step))))
        }
        "sim.patch" => {
            let (Some(run), Some(stock)) = (playback.as_mut(), stock.as_ref()) else {
                return Err(CoreError::validation("start a toolpath preview first"));
            };
            Ok(KernelReply::Bytes(run.take_patch(stock)))
        }
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::cam::toolpath::Move;
    use crate::doc::JobSetup;

    fn job() -> JobSetup {
        JobSetup {
            width: 100.0,
            height: 80.0,
            thickness: 10.0,
            ..JobSetup::default()
        }
    }

    /// A slot 3 deep along Y = 40 from X = 10 to 90.
    fn slot(id: u64) -> Toolpath {
        let tool = Tool {
            name: "end mill".into(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        let mut t = Toolpath::new(id, "slot", tool, crate::cam::toolpath::StrategyKind::Pocket);
        let top = job().surface_z();
        t.home = Point3::new(0.0, 0.0, top + 20.0);
        t.moves = vec![
            Move::rapid(Point3::new(10.0, 40.0, top + 5.0)),
            Move::plunge(Point3::new(10.0, 40.0, top - 3.0)),
            Move::feed(Point3::new(90.0, 40.0, top - 3.0)),
            Move::rapid(Point3::new(90.0, 40.0, top + 5.0)),
        ];
        t
    }

    fn doc_with(toolpaths: Vec<Toolpath>) -> Document {
        let mut doc = Document::new();
        doc.set_job(job()).expect("job");
        for t in toolpaths {
            doc.add_toolpath(t);
        }
        doc
    }

    #[test]
    fn stepping_cuts_as_far_as_the_tool_has_gone() {
        let doc = doc_with(vec![slot(0)]);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let top = doc.job().surface_z() as f32;
        // Home to above the start, the plunge, then 40 of the 80 mm cut.
        let lead = Point3::new(0.0, 0.0, f64::from(top) + 20.0).distance(Point3::new(
            10.0,
            40.0,
            f64::from(top) + 5.0,
        )) + 8.0;
        let r = run
            .step(&doc, &mut stock, Budget::Distance(lead + 40.0))
            .expect("steps");
        assert!(!r.done);
        let (at, _) = r.tool.expect("tool");
        assert!(
            (at.x - 50.0).abs() < 1e-6 && (at.y - 40.0).abs() < 1e-6,
            "{at:?}"
        );
        assert!(stock.height_at(30.0, 40.0).expect("inside") < top - 2.9);
        assert_eq!(stock.height_at(70.0, 40.0), Some(top));
        let r = run.step(&doc, &mut stock, Budget::All).expect("steps");
        assert!(r.done);
        assert!(stock.height_at(70.0, 40.0).expect("inside") < top - 2.9);
        assert!((run.progress() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_animated_stock_ends_like_the_whole_preview() {
        let doc = doc_with(vec![slot(0)]);
        let mut whole = Stock::new(doc.job(), Quality::Standard);
        whole.cut(doc.toolpaths()[0], 1).expect("cuts");
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let mut steps = 0;
        while !run
            .step(&doc, &mut stock, Budget::Distance(7.3))
            .expect("steps")
            .done
        {
            steps += 1;
            assert!(steps < 1000);
        }
        let differ = whole
            .z
            .iter()
            .zip(&stock.z)
            .filter(|(a, b)| (*a - *b).abs() > 1e-4)
            .count();
        assert_eq!(differ, 0);
    }

    #[test]
    fn sloped_moves_animated_in_small_steps_leave_what_whole_tool_stamps_do() {
        let mut vgroove = slot(0);
        vgroove.tool.tool_type = ToolGeometry::VBit {
            diameter: 8.0,
            included_angle: 60.0,
        };
        let top = job().surface_z();
        let mut moves = vec![
            Move::rapid(Point3::new(10.0, 20.0, top + 5.0)),
            Move::plunge(Point3::new(10.0, 20.0, top - 0.5)),
        ];
        for k in 1..=30 {
            let x = 10.0 + f64::from(k) * 2.6;
            let y = 20.0 + f64::from(k % 4) * 3.1;
            let z = top - if k % 2 == 0 { 0.5 } else { 3.0 };
            moves.push(Move::feed(Point3::new(x, y, z)));
        }
        moves.push(Move::rapid(Point3::new(88.0, 20.0, top + 5.0)));
        vgroove.moves = moves;
        let doc = doc_with(vec![vgroove]);
        let mut whole = Stock::new(doc.job(), Quality::High);
        whole.cut(doc.toolpaths()[0], 1).expect("cuts");
        let mut stock = Stock::new(doc.job(), Quality::High);
        let mut run = Playback::new(&doc, |_| true);
        let mut steps = 0;
        while !run
            .step(&doc, &mut stock, Budget::Distance(1.7))
            .expect("steps")
            .done
        {
            steps += 1;
            assert!(steps < 10_000);
        }
        assert!(stock.z != Stock::new(doc.job(), Quality::High).z);
        // Every sample of every step stamping the whole tool.
        let tp = doc.toolpaths()[0];
        let mut every = Stock::new(doc.job(), Quality::High);
        let footprint = Footprint::of(&tp.tool, every.cell).expect("footprint");
        let (mut index, mut fraction) = (0, 0.0);
        while index < tp.moves.len() {
            let mut left = 1.7;
            while index < tp.moves.len() {
                let one = &tp.moves[index];
                let from = if index == 0 {
                    tp.home
                } else {
                    tp.moves[index - 1].to
                };
                let length = one.length(from);
                let t1 = if length * (1.0 - fraction) <= left || length <= 0.0 {
                    1.0
                } else {
                    fraction + left / length
                };
                if one.kind != MoveKind::Rapid {
                    let steps = (length * (t1 - fraction) / (every.cell * 0.5))
                        .ceil()
                        .max(1.0);
                    for k in 0..=steps as usize {
                        let t = fraction + (t1 - fraction) * k as f64 / steps;
                        let p = point_on(from, one, t).expect("a point");
                        let cx = ((p.x - every.origin_x) / every.cell).round() as i64;
                        let cy = ((p.y - every.origin_y) / every.cell).round() as i64;
                        for &(dx, dy, rise) in footprint.cells() {
                            let (x, y) = (cx + i64::from(dx), cy + i64::from(dy));
                            if x < 0 || y < 0 || x >= every.width as i64 || y >= every.height as i64
                            {
                                continue;
                            }
                            let i = y as usize * every.width + x as usize;
                            let bottom = (p.z as f32 + rise).max(every.bottom as f32);
                            if bottom < every.z[i] {
                                every.z[i] = bottom;
                                every.owner[i] = 1;
                            }
                        }
                    }
                }
                left -= length * (t1 - fraction);
                if t1 >= 1.0 {
                    index += 1;
                    fraction = 0.0;
                } else {
                    fraction = t1;
                    break;
                }
                if left <= 0.0 {
                    break;
                }
            }
        }
        assert_eq!(stock.z, every.z);
        assert_eq!(stock.owner, every.owner);
        // And close to the stock of one sweep, which samples the moves at
        // other points.
        let off = whole
            .z
            .iter()
            .zip(&stock.z)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(off < 0.2, "{off}");
    }

    #[test]
    fn a_checked_tool_cuts_the_same_stock_as_an_unchecked_one() {
        let run = |checked: bool| {
            let mut vgroove = slot(0);
            vgroove.tool.tool_type = ToolGeometry::VBit {
                diameter: 8.0,
                included_angle: 60.0,
            };
            if checked {
                vgroove.tool.flute_length = 3.5;
                vgroove.tool.shank_diameter = 8.0;
                vgroove.tool.length_below_holder = 30.0;
                vgroove.tool.holder_diameter = 30.0;
                vgroove.tool.holder_length = 40.0;
            }
            let top = job().surface_z();
            let mut moves = vec![Move::rapid(Point3::new(20.0, 40.0, top + 5.0))];
            for k in 0..8 {
                let a = f64::from(k) * 0.8;
                let depth = 1.0 + f64::from(k % 3) * 1.5;
                moves.push(Move::feed(Point3::new(50.0, 40.0, top - 4.5)));
                moves.push(Move::feed(Point3::new(
                    50.0 + 30.0 * a.cos(),
                    40.0 + 30.0 * a.sin(),
                    top - depth,
                )));
            }
            vgroove.moves = moves;
            let doc = doc_with(vec![vgroove]);
            let mut stock = Stock::new(doc.job(), Quality::Standard);
            let mut run = Playback::new(&doc, |_| true);
            let mut found = Vec::new();
            loop {
                let r = run
                    .step(&doc, &mut stock, Budget::Distance(23.0))
                    .expect("steps");
                found.extend(r.collisions);
                if r.done {
                    break;
                }
            }
            (stock, found)
        };
        let (plain, none) = run(false);
        let (checked, found) = run(true);
        assert!(none.is_empty(), "{none:?}");
        // The shank sinks into the material deeper than the flutes.
        assert!(
            found.iter().any(|c| c.kind == IssueKind::Flute),
            "{found:?}"
        );
        assert!(plain == checked);
    }

    #[test]
    fn a_patch_covers_the_cells_cut_since_the_last_one() {
        let doc = doc_with(vec![slot(0)]);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        run.step(&doc, &mut stock, Budget::OneMove).expect("steps");
        run.step(&doc, &mut stock, Budget::OneMove).expect("steps");
        let blob = run.take_patch(&stock);
        let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        let header: serde_json::Value = serde_json::from_slice(&blob[4..4 + len]).expect("header");
        let rects: Vec<[usize; 4]> =
            serde_json::from_value(header["rects"].clone()).expect("rectangles");
        let count: usize = rects.iter().map(|r| r[2] * r[3]).sum();
        assert!(count > 0);
        // u16 heights and owners, each section padded to four bytes; the
        // uncut underside of a single sided job is left out.
        let padded = (count * 2).next_multiple_of(4);
        assert_eq!(blob.len(), 4 + len + padded * 2);
        // Under the same code as the whole heightfield, within half a step.
        let whole = stock.to_blob(json!({}));
        let (full, _) = crate::blob::read_heights(&whole, "z");
        assert_eq!(full["heightBase"], header["heightBase"]);
        assert_eq!(full["heightStep"], header["heightStep"]);
        let step = header["heightStep"].as_f64().expect("a step");
        let (_, z) = crate::blob::read_heights(&blob, "z");
        let sent: Vec<usize> = rects
            .iter()
            .flat_map(|&[x0, y0, w, h]| {
                (y0..y0 + h).flat_map(move |j| (x0..x0 + w).map(move |i| (j, i)))
            })
            .map(|(j, i)| j * stock.width + i)
            .collect();
        assert_eq!(sent.len(), z.len());
        for (value, &i) in z.iter().zip(&sent) {
            assert!(f64::from((value - stock.z[i]).abs()) <= step * 0.5 + 1e-5);
        }
        // Every cell cut is in the patch, and nothing far from the slot.
        let fresh = Stock::new(doc.job(), Quality::Standard);
        for i in 0..stock.z.len() {
            if stock.z[i] != fresh.z[i] {
                assert!(sent.contains(&i), "cell {i} cut but not sent");
            }
        }
        let row = |i: usize| stock.origin_y + (i / stock.width) as f64 * stock.cell;
        assert!(sent
            .iter()
            .all(|&i| (row(i) - 40.0).abs() < 3.0 + 33.0 * stock.cell));
        // Nothing new: an empty patch.
        let empty = run.take_patch(&stock);
        let len = u32::from_le_bytes([empty[0], empty[1], empty[2], empty[3]]) as usize;
        assert_eq!(empty.len(), 4 + len);
    }

    #[test]
    fn a_laser_animates_its_marks_and_ends_like_the_whole_preview() {
        let mut beam = slot(0);
        beam.tool.tool_type = ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
        };
        let top = job().surface_z();
        beam.feed = 1200.0;
        beam.moves = vec![
            Move::rapid(Point3::new(10.0, 40.0, top)),
            Move::feed(Point3::new(90.0, 40.0, top)),
        ];
        let doc = doc_with(vec![beam]);
        let mut whole = Stock::new(doc.job(), Quality::Standard);
        whole.cut(doc.toolpaths()[0], 1).expect("burns");
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let mut marked = 0;
        loop {
            let done = run
                .step(&doc, &mut stock, Budget::Distance(20.0))
                .expect("steps")
                .done;
            let blob = run.take_patch(&stock);
            let len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
            let header: serde_json::Value =
                serde_json::from_slice(&blob[4..4 + len]).expect("header");
            let names: Vec<&str> = header["sections"]
                .as_array()
                .expect("sections")
                .iter()
                .filter_map(|s| s["name"].as_str())
                .collect();
            if header["rects"].as_array().is_some_and(|r| !r.is_empty()) {
                assert!(names.contains(&"burn"), "{names:?}");
                marked += 1;
            }
            if done {
                break;
            }
        }
        assert!(marked >= 3, "{marked} patches with marks");
        assert_eq!(stock.burn, whole.burn);
        assert_eq!(stock.z, whole.z, "a laser removes no material");
    }

    #[test]
    fn one_move_and_toolpath_steps_stop_where_they_should() {
        let doc = doc_with(vec![slot(0), slot(0)]);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let r = run.step(&doc, &mut stock, Budget::OneMove).expect("steps");
        let (at, first) = r.tool.expect("tool");
        assert!((at.x - 10.0).abs() < 1e-9);
        run.step(&doc, &mut stock, Budget::Toolpath).expect("steps");
        let r = run.step(&doc, &mut stock, Budget::OneMove).expect("steps");
        let (_, second) = r.tool.expect("tool");
        assert_ne!(first, second, "the second toolpath has started");
        assert!(!r.done);
        assert!(run.seconds() >= 0.0);
    }

    #[test]
    fn run_to_retract_stops_after_the_tool_lifts_out() {
        let doc = doc_with(vec![slot(0), slot(0)]);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let r = run.step(&doc, &mut stock, Budget::Retract).expect("steps");
        let (at, first) = r.tool.expect("tool");
        let moves = &doc.toolpath(first).expect("toolpath").moves;
        let lifted = moves
            .windows(2)
            .position(|w| w[1].to.z > w[0].to.z + 1e-9)
            .expect("a lift");
        assert!(
            (at.z - moves[lifted + 1].to.z).abs() < 1e-9,
            "stopped at {at:?}"
        );
        assert!(!r.done);
    }

    #[test]
    fn a_step_can_stop_at_the_point_of_a_move() {
        let doc = doc_with(vec![slot(0)]);
        let id = doc.toolpaths()[0].id;
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let top = job().surface_z();
        let at = Point3::new(50.0, 41.0, top - 3.0);
        let r = run
            .step(&doc, &mut stock, Budget::ToMove { id, index: 2, at })
            .expect("steps");
        let (tip, on) = r.tool.expect("tool");
        assert_eq!(on, id);
        assert!(
            (tip.x - 50.0).abs() < 0.5 && (tip.y - 40.0).abs() < 1e-9,
            "{tip:?}"
        );
        assert!(stock.height_at(30.0, 40.0).expect("inside") < top as f32 - 2.9);
        assert_eq!(stock.height_at(70.0, 40.0), Some(top as f32));
        assert!(!r.done);
    }

    #[test]
    fn the_checks_run_as_the_animation_cuts_and_report_each_move_once() {
        let mut plunging = slot(0);
        plunging.tool.center_cutting = false;
        let doc = doc_with(vec![plunging]);
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let mut found = Vec::new();
        loop {
            let r = run
                .step(&doc, &mut stock, Budget::Distance(0.7))
                .expect("steps");
            found.extend(r.collisions);
            if r.done {
                break;
            }
        }
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, IssueKind::Plunge);
        assert_eq!(found[0].move_index, 1);
        assert!((found[0].x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn the_other_sides_toolpaths_get_every_check() {
        use crate::doc::{JobKind, Side};
        let mut doc = Document::new();
        doc.set_job(JobSetup {
            kind: JobKind::DoubleSided,
            ..job()
        })
        .expect("job");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let mut plunging = slot(0);
        plunging.tool.center_cutting = false;
        doc.add_toolpath(plunging);
        doc.set_active_side(Side::Top).expect("top");
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        let mut run = Playback::new(&doc, |_| true);
        let r = run.step(&doc, &mut stock, Budget::All).expect("steps");
        assert!(r.done);
        assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
        assert_eq!(r.collisions[0].kind, IssueKind::Plunge);
        assert_eq!(r.collisions[0].move_index, 1);
        // Turned over left to right: x 10 of the bottom is x 90 on top.
        assert!((r.collisions[0].x - 90.0).abs() < 1e-9);
        // It cut the underside.
        let bottom = stock.bottom as f32;
        assert!(stock.floor_at(50.0, 40.0).expect("inside") > bottom + 2.9);
    }

    #[test]
    fn commands_run_through_dispatch() {
        let doc = doc_with(vec![slot(0)]);
        let (mut stock, mut undo, mut run) = (None, None, None);
        assert!(dispatch(&doc, &mut stock, &mut undo, &mut run, "sim.step", "{}").is_err());
        let started =
            dispatch(&doc, &mut stock, &mut undo, &mut run, "sim.start", "{}").expect("starts");
        let KernelReply::Json(text) = started else {
            panic!("json reply");
        };
        assert!(text.contains("\"toolpaths\""));
        assert!(text.contains("\"holder\":null"), "{text}");
        let stepped = dispatch(
            &doc,
            &mut stock,
            &mut undo,
            &mut run,
            "sim.step",
            r#"{"toEnd":true}"#,
        )
        .expect("steps");
        let KernelReply::Json(text) = stepped else {
            panic!("json reply");
        };
        assert!(text.contains("\"done\":true"), "{text}");
        assert!(matches!(
            dispatch(&doc, &mut stock, &mut undo, &mut run, "sim.patch", "").expect("patch"),
            KernelReply::Bytes(_)
        ));
        // Advance steps and answers the patch in one call, the step in its header.
        dispatch(&doc, &mut stock, &mut undo, &mut run, "sim.start", "{}").expect("starts");
        let advanced = dispatch(
            &doc,
            &mut stock,
            &mut undo,
            &mut run,
            "sim.advance",
            r#"{"distance":60}"#,
        )
        .expect("advances");
        let KernelReply::Bytes(bytes) = advanced else {
            panic!("bytes reply");
        };
        let size = u32::from_le_bytes(bytes[..4].try_into().expect("length")) as usize;
        let header: serde_json::Value =
            serde_json::from_slice(&bytes[4..4 + size]).expect("header");
        assert_eq!(header["step"]["done"], false, "{header}");
        assert!(header["step"]["tool"]["x"].is_number(), "{header}");
        assert!(
            !header["rects"].as_array().expect("rects").is_empty(),
            "{header}"
        );
        assert!(dispatch(
            &doc,
            &mut stock,
            &mut undo,
            &mut run,
            "sim.advance",
            r#"{"distance":-1}"#
        )
        .is_err());
        // A kept start goes on from the stock shown, and Undo Last can go
        // back to it.
        let cut = stock.clone();
        dispatch(
            &doc,
            &mut stock,
            &mut undo,
            &mut run,
            "sim.start",
            r#"{"keep":true}"#,
        )
        .expect("starts");
        assert_eq!(stock, cut);
        assert_eq!(undo, Some(cut));
    }
}
