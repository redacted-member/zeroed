//! The engine that runs a post processor over a list of toolpaths.
//!
//! It walks the moves, picks the block each one calls for, replaces the
//! tokens in that block's output lines and writes them out. A variable that
//! has not changed since it was last written is replaced with nothing, which
//! is how a program comes out modal: the feed rate appears on the first
//! cutting move of a run and not on the ones after it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cam::tool::Tool;
use crate::cam::toolpath::{Move, MoveKind, Point3, Toolpath};
use crate::error::CoreError;
use crate::geom::Point;

use super::parser::{tokens_in, LENGTH_LINE_TOKEN, TEXT_TOKENS};
use super::rotary::Wrap;
use super::{BlockKind, DrillBankMode, OutputBounds, Post, Scaling, ValueSource, VarDef};

/// Chord tolerance used when a post cannot write arcs and they have to be
/// broken into straight moves, in millimetres.
const ARC_CHORD_TOLERANCE: f64 = 0.02;

/// Largest number of straight moves one arc is broken into.
const ARC_MAX_STEPS: usize = 2048;

/// What the engine knows about the job beyond the toolpaths themselves.
///
/// The date and time are handed in rather than read from the clock, so that
/// the same job posted twice gives the same bytes and so that the shell can
/// format them the way the user's system does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostJob {
    /// Name of the file being written, without its directory.
    pub file_name: String,
    /// Directory the file is written to.
    pub path_name: String,
    /// Date written into the header, as the shell formatted it.
    pub date: String,
    /// Time written into the header, as the shell formatted it.
    pub time: String,
    /// Notes written into the header.
    pub notes: String,
    /// Where the machine starts and ends, in millimetres.
    pub home: Point3,
    /// Height the tool travels at between cuts, in millimetres.
    pub safe_z: f64,
    /// The M6 macro measures the new tool and applies its length itself.
    /// When false the lines marked `[IF_LENGTH_BY_PROGRAM]` are written
    /// (a `G43 H` after each change and a `G49` at the end).
    #[serde(default = "applies_length")]
    pub m6_applies_length: bool,
    /// Written before the file name wherever `[TP_FILENAME]` is, such as
    /// "AIR CUT" for a trial run; empty for the real program.
    #[serde(default)]
    pub label: String,
}

fn applies_length() -> bool {
    true
}

impl Default for PostJob {
    fn default() -> Self {
        PostJob {
            file_name: String::new(),
            path_name: String::new(),
            date: String::new(),
            time: String::new(),
            notes: String::new(),
            home: Point3::new(0.0, 0.0, 25.0),
            safe_z: 5.0,
            m6_applies_length: true,
            label: String::new(),
        }
    }
}

/// Checks the post and every toolpath before anything is written: a drill
/// bank toolpath needs a post that can drive a drill bank.
fn check_toolpaths(post: &Post, toolpaths: &[Toolpath]) -> Result<(), CoreError> {
    post.validate()?;
    for toolpath in toolpaths {
        toolpath.validate()?;
        if !toolpath.firings.is_empty() && !post.supports_drill_bank() {
            return Err(CoreError::validation(format!(
                "the {} post processor cannot drive a drill bank: pick a post with \
                 SETUP_DRILL_BANK, FINAL_DRILL_BANK_ACTION and either DRILL_BANK_PATTERN \
                 or DRILL_BANK_EXTEND_DRILL with DRILL_BANK_RETRACT_DRILL to save {}",
                post.name, toolpath.name
            )));
        }
    }
    Ok(())
}

/// The bit pattern of `drills`: drill number n sets the bit worth 2 to the
/// power n - 1. Drill numbers past 53 cannot be held exactly and are left
/// out, as is 0 (no drill).
fn drill_pattern(drills: &[u32]) -> f64 {
    drills
        .iter()
        .filter(|n| (1..=53).contains(*n))
        .map(|n| 2f64.powi(*n as i32 - 1))
        .sum()
}

/// Runs `post` over `toolpaths` and answers the file text.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the post cannot write the program,
/// for instance when it has no feed move block or a toolpath is broken.
pub fn post_process(
    post: &Post,
    job: &PostJob,
    toolpaths: &[Toolpath],
) -> Result<String, CoreError> {
    post_process_wrapped(post, job, toolpaths, None)
}

/// The point `step` of `steps` of the way from `start` to `end`; the ends are
/// exactly `start` and `end`.
fn point_along(start: Point3, end: Point3, step: usize, steps: usize) -> Point3 {
    if step == 0 {
        return start;
    }
    if step >= steps {
        return end;
    }
    let t = step as f64 / steps as f64;
    Point3::new(
        start.x + (end.x - start.x) * t,
        start.y + (end.y - start.y) * t,
        start.z + (end.z - start.z) * t,
    )
}

/// Runs `post` over `toolpaths` like [`post_process`], wrapping the flat
/// moves of a rotary job around its axis when `wrap` is given: the across
/// coordinate is written as an angle and arcs are written as straight moves.
///
/// # Errors
/// As [`post_process`].
pub fn post_process_wrapped(
    post: &Post,
    job: &PostJob,
    toolpaths: &[Toolpath],
    wrap: Option<Wrap>,
) -> Result<String, CoreError> {
    check_toolpaths(post, toolpaths)?;
    let mut writer = Writer::new(post, job, toolpaths);
    writer.wrap = wrap;
    writer.run(toolpaths);
    Ok(writer.finish())
}

/// Runs `post` like [`post_process_wrapped`] and answers the program cut
/// into the files `TAPE_SPLITTING` asks for: one text when the post does
/// not split or the program is short enough. A file ends after the last
/// move that lifts straight up within the line tolerance of the most
/// lines, or after the last whole move within the most lines when none
/// does, and carries the footer. Every later file starts with the header,
/// the tool change and spindle on blocks for the tool loaded at that point
/// (the footer before stopped the spindle), and a move back to where the
/// last file stopped that gives every axis. Each file is a whole program:
/// its line numbers start again and `[TP_FILENAME]` is its own name.
///
/// # Errors
/// As [`post_process`].
pub fn post_process_pieces(
    post: &Post,
    job: &PostJob,
    toolpaths: &[Toolpath],
    wrap: Option<Wrap>,
) -> Result<Vec<String>, CoreError> {
    check_toolpaths(post, toolpaths)?;
    let mut writer = Writer::new(post, job, toolpaths);
    writer.wrap = wrap;
    writer.run(toolpaths);
    let splits = match &post.tape_splitting {
        Some(split) => writer.split_points(split),
        None => Vec::new(),
    };
    if splits.is_empty() {
        return Ok(vec![writer.finish()]);
    }
    // Written again, starting a new file after each chosen move.
    let mut writer = Writer::new(post, job, toolpaths);
    writer.wrap = wrap;
    writer.splits = splits.into();
    writer.file_name = piece_file_name(post, job, 0);
    writer.run(toolpaths);
    Ok(writer.finish_pieces())
}

/// The name of tape split piece `piece` (from 0) of the job's file, as
/// `TAPE_SPLITTING` names it, or the job's own file name for an unnumbered
/// first piece.
fn piece_file_name(post: &Post, job: &PostJob, piece: usize) -> String {
    let stem = std::path::Path::new(&job.file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    post.tape_splitting
        .as_ref()
        .and_then(|split| split.file_name(piece, stem, &post.file_extension))
        .unwrap_or_else(|| job.file_name.clone())
}

/// Which block family a move belongs to, used to spot the first move of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Rapid,
    Feed,
    Plunge,
    ArcCw,
    ArcCcw,
    Other,
}

struct Writer<'a> {
    post: &'a Post,
    job: &'a PostJob,
    out: String,
    last: BTreeMap<String, String>,
    line_number: f64,
    from: Point3,
    to: Point3,
    arc_centre: Point,
    feed: f64,
    plunge: f64,
    spindle: f64,
    tool_number: u32,
    power: f64,
    dwell: f64,
    tool: Option<Tool>,
    toolpath_name: String,
    /// True between a drill bank toolpath's setup and final blocks.
    bank_open: bool,
    /// The drills the bank has down, lowest first, for `[DB]` and `[DL]`.
    bank: Vec<u32>,
    /// Of those, the ones on the horizontal arm, for `[DBX]`.
    bank_x: Vec<u32>,
    /// Of those, the ones on the vertical arm, for `[DBY]`.
    bank_y: Vec<u32>,
    /// The drill being lowered or raised, for `[CD]`.
    current_drill: u32,
    bounds: OutputBounds,
    tools_used: Vec<(u32, String)>,
    tool_list_entry: Option<(u32, String)>,
    beam_on: bool,
    run: Option<Family>,
    wrap: Option<Wrap>,
    /// The cutting feed rate of the current toolpath, for `[FC]`.
    cut_feed: f64,
    /// The tool before the last tool change, for `[TP]`.
    previous_tool: u32,
    /// Set by a tool change until the next rapid, which is the initial one.
    initial_rapid: bool,
    /// The height a tool change lifts to, for `[ZCLEAR]`: the higher of the
    /// current toolpath's safe Z and the home Z.
    clear_z: f64,
    /// Feed rate of the last cutting move written in this toolpath.
    last_feed: Option<f64>,
    /// Where the current arc starts and how far it sweeps, in radians.
    arc_start: Point3,
    arc_sweep: f64,
    toolpath_names: Vec<String>,
    /// True while a cutting move is written, for inverse time feeds.
    cutting: bool,
    /// Byte offset where each output line starts.
    line_starts: Vec<usize>,
    /// Lines the header takes.
    header_lines: usize,
    /// Line the footer starts on.
    footer_line: usize,
    /// For each move written, the lines written by its end and whether it
    /// lifts straight up: where a tape may split.
    move_ends: Vec<(usize, bool)>,
    /// Tape splitting: the move counts after which a new file starts.
    splits: std::collections::VecDeque<usize>,
    /// Byte offsets where each file after the first starts.
    piece_starts: Vec<usize>,
    /// Name of the file being written, for `[TP_FILENAME]`: a tape split
    /// piece names itself.
    file_name: String,
}

impl<'a> Writer<'a> {
    fn new(post: &'a Post, job: &'a PostJob, toolpaths: &[Toolpath]) -> Self {
        let mut bounds: Option<OutputBounds> = None;
        for toolpath in toolpaths {
            let box3 = match toolpath.bounds {
                Some(found) => Some(found),
                None => {
                    let mut copy = toolpath.clone();
                    copy.recompute_bounds();
                    copy.bounds
                }
            };
            if let Some(found) = box3.map(OutputBounds::from_bounds) {
                bounds = Some(match bounds {
                    Some(existing) => existing.union(found),
                    None => found,
                });
            }
        }
        let mut tools_used = Vec::new();
        for toolpath in toolpaths {
            let entry = (toolpath.tool_number, toolpath.tool.name.clone());
            if !tools_used.contains(&entry) {
                tools_used.push(entry);
            }
        }
        Writer {
            post,
            job,
            out: String::new(),
            last: BTreeMap::new(),
            line_number: post.line_number_start,
            from: job.home,
            to: job.home,
            arc_centre: Point::ORIGIN,
            feed: 0.0,
            plunge: 0.0,
            spindle: 0.0,
            tool_number: 0,
            power: 0.0,
            dwell: 0.0,
            tool: None,
            toolpath_name: String::new(),
            bank_open: false,
            bank: Vec::new(),
            bank_x: Vec::new(),
            bank_y: Vec::new(),
            current_drill: 0,
            bounds: bounds.unwrap_or_else(OutputBounds::empty),
            tools_used,
            tool_list_entry: None,
            beam_on: false,
            run: None,
            wrap: None,
            cut_feed: 0.0,
            previous_tool: 0,
            initial_rapid: true,
            clear_z: job.safe_z.max(job.home.z),
            last_feed: None,
            arc_start: job.home,
            arc_sweep: 0.0,
            toolpath_names: toolpaths.iter().map(|t| t.name.clone()).collect(),
            cutting: false,
            line_starts: Vec::new(),
            header_lines: 0,
            footer_line: 0,
            move_ends: Vec::new(),
            splits: std::collections::VecDeque::new(),
            piece_starts: Vec::new(),
            file_name: job.file_name.clone(),
        }
    }

    fn run(&mut self, toolpaths: &[Toolpath]) {
        if let Some(first) = toolpaths.first() {
            self.load_tool(first);
        }
        self.emit(BlockKind::Header);
        self.header_lines = self.line_starts.len();
        let mut current_tool: Option<u32> = None;
        for toolpath in toolpaths {
            let (speed, feed, plunge) = (self.spindle, self.cut_feed, self.plunge);
            self.load_tool(toolpath);
            self.run = None;
            let changed = current_tool != Some(toolpath.tool_number);
            self.last_feed = None;
            if changed {
                self.previous_tool = current_tool.unwrap_or(0);
                self.initial_rapid = true;
                self.emit(BlockKind::Toolchange);
                self.emit(BlockKind::SpindleOn);
                // After a manual change the machine may be anywhere, so the
                // next moves print every axis rather than only what changed.
                self.last.clear();
                self.note_lift();
                current_tool = Some(toolpath.tool_number);
                self.emit(BlockKind::NewSegmentPostToolchange);
            } else {
                self.emit(BlockKind::NewSegment);
                // The same tool goes on at this toolpath's own speed and
                // feeds, with no tool change to write them.
                let first = toolpath.rates_at(0);
                let new_speed = (self.spindle - speed).abs() > 1e-9;
                let new_feeds = first.feed != feed || first.plunge != plunge;
                self.change_rates(new_speed, new_feeds, first.feed);
            }
            if !toolpath.firings.is_empty() {
                self.open_bank();
            }
            self.write_moves(toolpath);
            self.close_bank();
            if self.beam_on {
                self.emit(BlockKind::JetToolOff);
                self.beam_on = false;
            }
        }
        self.footer_line = self.line_starts.len();
        self.emit(BlockKind::Footer);
        self.emit(BlockKind::Trailer);
    }

    /// Inverse time feeds apply: the post asks for them and the job is
    /// wrapped round a rotary axis.
    fn inverse_time_mode(&self) -> bool {
        self.post.inverse_time && self.wrap.is_some()
    }

    /// Writes a cutting move block; in inverse time every cutting move uses
    /// its FIRST_ block so that each one carries its own F word.
    fn emit_cut(&mut self, kind: BlockKind) {
        self.cutting = true;
        self.emit(kind);
        self.cutting = false;
    }

    fn load_tool(&mut self, toolpath: &Toolpath) {
        self.tool = Some(toolpath.tool.clone());
        self.tool_number = toolpath.tool_number;
        self.spindle = self.post.clamp_spindle(toolpath.spindle);
        // Header and tool change blocks may print [F] before the first move.
        self.feed = toolpath.feed;
        self.cut_feed = toolpath.feed;
        self.plunge = toolpath.plunge;
        self.toolpath_name = toolpath.name.clone();
        self.clear_z = toolpath.safe_z.max(toolpath.home.z).max(self.job.home.z);
        self.power = match toolpath.tool.tool_type {
            crate::cam::tool::ToolGeometry::LaserTool { power, .. } => power,
            _ => 0.0,
        };
    }

    fn write_moves(&mut self, toolpath: &Toolpath) {
        let laser = toolpath.is_laser() && self.post.supports_jet();
        let mut last_rates = toolpath.rates_at(0);
        // The drill bank firings, met in order as the tool arrives over them.
        let mut firings = toolpath.firings.iter().peekable();
        for (index, (from, one)) in toolpath.iter_moves().enumerate() {
            self.from = from;
            self.to = one.to;
            let mut select = None;
            // A firing is met on a rapid that arrives over its holes, never
            // on one straight up or down in a hole (a peck, a retract).
            let plumb = (from.x - one.to.x).abs() < 1e-9
                && (from.y - one.to.y).abs() < 1e-9
                && (from.z - one.to.z).abs() > 1e-9;
            if one.kind == MoveKind::Rapid && !plumb {
                select = firings.next_if(|f| f.at.distance(one.to.xy()) < 1e-6);
            }
            let rates = toolpath.rates_at(index);
            let new_feeds = rates.feed != last_rates.feed || rates.plunge != last_rates.plunge;
            if new_feeds {
                // New rates start a new run, whose FIRST_ block carries them.
                self.run = None;
            }
            last_rates = rates;
            self.cut_feed = rates.feed;
            self.plunge = rates.plunge;
            // A merged toolpath runs each part at its own speed and feeds.
            let spindle = self.post.clamp_spindle(rates.spindle);
            let new_speed = (spindle - self.spindle).abs() > 1e-9;
            self.spindle = spindle;
            self.change_rates(new_speed, new_feeds, rates.feed);
            let lifts = one.kind == MoveKind::Rapid
                && (from.x - one.to.x).abs() < 1e-9
                && (from.y - one.to.y).abs() < 1e-9
                && one.to.z > from.z;
            match one.kind {
                MoveKind::Rapid => {
                    if laser && self.beam_on {
                        self.emit(BlockKind::JetToolOff);
                        self.beam_on = false;
                    }
                    let straight_up = (from.x - one.to.x).abs() < 1e-9
                        && (from.y - one.to.y).abs() < 1e-9
                        && one.to.z > from.z;
                    let first = self.first_of_run(Family::Rapid);
                    let retract =
                        straight_up && self.post.blocks.contains_key(&BlockKind::RetractMove);
                    let after_change = std::mem::take(&mut self.initial_rapid);
                    let initial =
                        after_change && self.post.blocks.contains_key(&BlockKind::InitialRapidMove);
                    let kind = match (retract, initial, first) {
                        (true, _, _) => BlockKind::RetractMove,
                        (false, true, _) => BlockKind::InitialRapidMove,
                        (false, false, true) => BlockKind::FirstRapidMove,
                        (false, false, false) => BlockKind::RapidMove,
                    };
                    if after_change && !retract {
                        self.emit_approach(kind, one.to);
                    } else {
                        self.write_rapid_pieces(kind);
                    }
                    if let Some(firing) = select {
                        // Over the next holes: the bank lowers these drills.
                        self.select_drills(&firing.drills, &firing.along_x, &firing.along_y);
                    }
                }
                MoveKind::Feed => {
                    self.feed = rates.feed;
                    self.feed_change();
                    if laser {
                        self.set_move_power(one.power);
                    }
                    if laser && !self.beam_on {
                        self.emit(BlockKind::JetToolPower);
                        self.emit(BlockKind::JetToolOn);
                        self.beam_on = true;
                    }
                    self.write_cut_pieces(
                        Family::Feed,
                        BlockKind::FirstFeedMove,
                        BlockKind::FeedMove,
                    );
                }
                MoveKind::Plunge => {
                    self.feed = rates.plunge;
                    self.feed_change();
                    self.write_cut_pieces(
                        Family::Plunge,
                        BlockKind::FirstPlungeMove,
                        BlockKind::PlungeMove,
                    );
                }
                MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                    self.feed = rates.feed;
                    self.feed_change();
                    if laser {
                        self.set_move_power(one.power);
                    }
                    if laser && !self.beam_on {
                        self.emit(BlockKind::JetToolPower);
                        self.emit(BlockKind::JetToolOn);
                        self.beam_on = true;
                    }
                    let radius = centre.distance(from.xy());
                    // A helix through arc blocks that never write Z would
                    // lose its depth, so it goes out as straight moves.
                    let moves_z = (one.to.z - from.z).abs() > 1e-9;
                    let keeps_depth =
                        !moves_z || self.post.supports_helical_arcs() || self.post.arcs_write_z();
                    // An arc within the chord tolerance of a point is a
                    // straight move: its centre words would round to zero
                    // and the control would refuse a zero radius arc.
                    if self.post.supports_arcs()
                        && self.wrap.is_none()
                        && radius > ARC_CHORD_TOLERANCE
                        && self.post.arc_radius_ok(radius)
                        && keeps_depth
                    {
                        self.arc_centre = centre;
                        self.arc_start = from;
                        self.arc_sweep = one.arc_sweep(from).unwrap_or(0.0);
                        let clockwise = matches!(one.kind, MoveKind::ArcCw { .. });
                        // An arc that moves in Z has its own blocks when the
                        // post defines them.
                        let helical = moves_z && self.post.supports_helical_arcs();
                        let family = if clockwise {
                            Family::ArcCw
                        } else {
                            Family::ArcCcw
                        };
                        let first = self.first_of_run(family);
                        let kind = match (clockwise, helical, first) {
                            (true, false, true) => BlockKind::FirstCwArcMove,
                            (true, false, false) => BlockKind::CwArcMove,
                            (false, false, true) => BlockKind::FirstCcwArcMove,
                            (false, false, false) => BlockKind::CcwArcMove,
                            (true, true, true) => BlockKind::FirstCwHelicalArcMove,
                            (true, true, false) => BlockKind::CwHelicalArcMove,
                            (false, true, true) => BlockKind::FirstCcwHelicalArcMove,
                            (false, true, false) => BlockKind::CcwHelicalArcMove,
                        };
                        self.emit(kind);
                    } else {
                        self.write_arc_as_lines(from, one);
                    }
                }
                MoveKind::Dwell { seconds } => {
                    self.dwell = seconds;
                    if self.post.blocks.contains_key(&BlockKind::DwellMove) {
                        self.run = Some(Family::Other);
                        self.emit(BlockKind::DwellMove);
                    }
                }
            }
            self.move_ends.push((self.line_starts.len(), lifts));
            if self.splits.front() == Some(&self.move_ends.len()) {
                self.splits.pop_front();
                self.start_next_file(toolpath);
            }
        }
    }

    /// Starts a drill bank toolpath: the setup block, no drill down yet.
    fn open_bank(&mut self) {
        self.bank_open = true;
        self.bank.clear();
        self.bank_x.clear();
        self.bank_y.clear();
        self.emit(BlockKind::SetupDrillBank);
    }

    /// Puts the bank's drills down as `drills` asks, the arms' shares in
    /// `along_x` and `along_y`. A pattern post writes the whole set when it
    /// differs from the set down; a per drill post raises each drill down
    /// that is not wanted, then lowers each wanted one not yet down.
    fn select_drills(&mut self, drills: &[u32], along_x: &[u32], along_y: &[u32]) {
        match self.post.drill_bank_mode() {
            Some(DrillBankMode::Pattern) => {
                let changed = self.bank != drills;
                self.set_bank(drills, along_x, along_y);
                if changed {
                    self.emit(BlockKind::DrillBankPattern);
                }
            }
            Some(DrillBankMode::PerDrill) => {
                let down = self.bank.clone();
                for drill in down.iter().filter(|d| !drills.contains(d)) {
                    self.current_drill = *drill;
                    self.emit(BlockKind::DrillBankRetractDrill);
                }
                self.set_bank(drills, along_x, along_y);
                for drill in drills.iter().filter(|d| !down.contains(d)) {
                    self.current_drill = *drill;
                    self.emit(BlockKind::DrillBankExtendDrill);
                }
            }
            None => {}
        }
    }

    fn set_bank(&mut self, drills: &[u32], along_x: &[u32], along_y: &[u32]) {
        self.bank = drills.to_vec();
        self.bank_x = along_x.to_vec();
        self.bank_y = along_y.to_vec();
    }

    /// Ends a drill bank toolpath: a per drill post raises every drill
    /// still down, then the final block. Nothing when no bank is open.
    fn close_bank(&mut self) {
        if !std::mem::take(&mut self.bank_open) {
            return;
        }
        if self.post.drill_bank_mode() == Some(DrillBankMode::PerDrill) {
            for drill in self.bank.clone() {
                self.current_drill = drill;
                self.emit(BlockKind::DrillBankRetractDrill);
            }
        }
        self.set_bank(&[], &[], &[]);
        self.emit(BlockKind::FinalDrillBankAction);
    }

    /// Tape splitting: ends the file here with the footer and starts the
    /// next one with the header, the tool change and spindle on blocks, and
    /// a move back to where the tool is, every axis written, at the
    /// clearance height first when the tool is below it.
    fn start_next_file(&mut self, toolpath: &Toolpath) {
        if self.beam_on {
            self.emit(BlockKind::JetToolOff);
            self.beam_on = false;
        }
        // A drill bank is closed at the end of each file and set up again,
        // with the same drills down, at the start of the next.
        let bank = self
            .bank_open
            .then(|| (self.bank.clone(), self.bank_x.clone(), self.bank_y.clone()));
        self.close_bank();
        self.emit(BlockKind::Footer);
        self.emit(BlockKind::Trailer);
        self.piece_starts.push(self.out.len());
        // Each file is a whole program: its lines are numbered from the
        // start again and its header names the file itself.
        self.line_number = self.post.line_number_start;
        self.file_name = piece_file_name(self.post, self.job, self.piece_starts.len());
        let here = self.to;
        self.last.clear();
        self.run = None;
        self.last_feed = None;
        self.initial_rapid = true;
        self.emit(BlockKind::Header);
        self.previous_tool = self.tool_number;
        self.emit(BlockKind::Toolchange);
        self.emit(BlockKind::SpindleOn);
        self.note_lift();
        if let Some((drills, along_x, along_y)) = bank {
            self.open_bank();
            self.select_drills(&drills, &along_x, &along_y);
        }
        let above = Point3::new(here.x, here.y, here.z.max(toolpath.safe_z));
        self.from = self.job.home;
        self.to = above;
        let initial = std::mem::take(&mut self.initial_rapid)
            && self.post.blocks.contains_key(&BlockKind::InitialRapidMove);
        self.run = Some(Family::Rapid);
        self.emit_approach(
            if initial {
                BlockKind::InitialRapidMove
            } else {
                BlockKind::FirstRapidMove
            },
            above,
        );
        if here.z < above.z - 1e-9 {
            self.from = above;
            self.to = here;
            self.feed = self.plunge;
            self.run = None;
            let kind = if self.first_of_run(Family::Plunge) {
                BlockKind::FirstPlungeMove
            } else {
                BlockKind::PlungeMove
            };
            self.emit_cut(kind);
        }
        self.from = here;
        self.to = here;
    }

    /// Tape splitting: the move counts after which a new file starts, so
    /// that no file holds more than the most lines (its footer not
    /// counted), each ending after the last move that lifts straight up
    /// within the line tolerance of that, or after the last whole move.
    /// Empty when the program fits in one file.
    fn split_points(&self, split: &super::TapeSplitting) -> Vec<usize> {
        let block_lines = |kind: BlockKind| self.post.block(kind).map_or(0, |lines| lines.len());
        // A later file starts with the header, the tool change and spindle
        // on blocks and up to two moves back to where the tool was.
        let restart = self.header_lines
            + block_lines(BlockKind::Toolchange)
            + block_lines(BlockKind::SpindleOn)
            + 2;
        let body_end = self.footer_line;
        let mut splits = Vec::new();
        let mut start = self.header_lines;
        let mut prologue = self.header_lines;
        let mut next = 0;
        loop {
            let room = split.max_lines.saturating_sub(prologue).max(1);
            let limit = start + room;
            if limit >= body_end {
                break;
            }
            let earliest = limit.saturating_sub(split.line_tolerance).max(start + 1);
            let candidates = &self.move_ends[next..];
            let fits = |(line, _): &&(usize, bool)| *line > start && *line <= limit;
            let chosen = candidates
                .iter()
                .rposition(|end| fits(&end) && end.1 && end.0 >= earliest)
                .or_else(|| candidates.iter().rposition(|end| fits(&end)))
                .or_else(|| candidates.iter().position(|(line, _)| *line > start));
            let Some(offset) = chosen else {
                break;
            };
            let index = next + offset;
            let line = self.move_ends[index].0;
            if line >= body_end {
                break;
            }
            splits.push(index + 1);
            start = line;
            prologue = restart;
            next = index + 1;
        }
        splits
    }

    /// Takes a move's own laser power. While the beam is on a new power is
    /// written straight away; otherwise the beam on block writes it.
    fn set_move_power(&mut self, power: Option<f64>) {
        let Some(power) = power else {
            return;
        };
        if (power - self.power).abs() > 1e-9 {
            self.power = power;
            if self.beam_on {
                self.emit(BlockKind::JetToolPower);
            }
        }
    }

    /// Writes an arc as a chain of straight cutting moves, for a post that
    /// has no arc blocks.
    fn write_arc_as_lines(&mut self, from: Point3, one: &Move) {
        let Some(sweep) = one.arc_sweep(from) else {
            return;
        };
        let centre = match one.kind {
            MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => centre,
            _ => return,
        };
        let radius = centre.distance(from.xy());
        let steps = arc_steps(radius, sweep);
        let start_angle = centre.to(from.xy()).angle();
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            let angle = start_angle + sweep * t;
            let point = if step == steps {
                one.to
            } else {
                Point3::new(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                    from.z + (one.to.z - from.z) * t,
                )
            };
            self.to = point;
            self.write_cut_pieces(Family::Feed, BlockKind::FirstFeedMove, BlockKind::FeedMove);
            self.from = point;
        }
    }

    /// How many pieces the current straight move takes under the post's
    /// `MAXIMUM_ROTATION`; one for a flat job or a post with no limit.
    fn turn_pieces(&self) -> usize {
        self.wrap.map_or(1, |wrap| {
            wrap.pieces(self.from, self.to, self.post.maximum_rotation)
        })
    }

    /// Writes the current cutting move, in pieces when it turns further than
    /// the post allows in one move.
    fn write_cut_pieces(&mut self, family: Family, first: BlockKind, next: BlockKind) {
        let (start, end) = (self.from, self.to);
        let pieces = self.turn_pieces();
        for piece in 1..=pieces {
            self.from = point_along(start, end, piece - 1, pieces);
            self.to = point_along(start, end, piece, pieces);
            let kind = if self.first_of_run(family) {
                first
            } else {
                next
            };
            self.emit_cut(kind);
        }
    }

    /// Writes the current rapid, in pieces when it turns further than the
    /// post allows in one move.
    fn write_rapid_pieces(&mut self, kind: BlockKind) {
        let (start, end) = (self.from, self.to);
        let pieces = self.turn_pieces();
        for piece in 1..=pieces {
            self.from = point_along(start, end, piece - 1, pieces);
            self.to = point_along(start, end, piece, pieces);
            self.emit(if piece == 1 {
                kind
            } else {
                BlockKind::RapidMove
            });
        }
    }

    /// The first rapid after a tool change, to `target`. When the post's
    /// tool change lifts to the clearance height (`[ZCLEAR]`), the tool
    /// crosses at that height and only then comes straight down to the
    /// target. A rapid that does not go below the clearance height, a post
    /// that does not lift, or one whose rapids write no Z, is written as it
    /// is.
    fn emit_approach(&mut self, kind: BlockKind, target: Point3) {
        let across = (target.x - self.from.x).abs() > 1e-9 || (target.y - self.from.y).abs() > 1e-9;
        let writes_z = self
            .post
            .block(kind)
            .is_some_and(|lines| lines.iter().any(|l| self.line_writes(l, ValueSource::Z)));
        let lifts = self.tool_change_lifts();
        if !across || !writes_z || !lifts || target.z >= self.clear_z - 1e-9 {
            self.to = target;
            self.write_rapid_pieces(kind);
            return;
        }
        self.to = Point3::new(target.x, target.y, self.clear_z);
        self.write_rapid_pieces(kind);
        self.from = self.to;
        self.to = target;
        self.run = Some(Family::Rapid);
        self.emit(BlockKind::RapidMove);
    }

    /// True when the post's tool change block lifts to `[ZCLEAR]`, or, for a
    /// post with one tool to a file and no such block, its header does.
    fn tool_change_lifts(&self) -> bool {
        let lifting = if self.post.supports_tool_change() {
            BlockKind::Toolchange
        } else {
            BlockKind::Header
        };
        self.post.block(lifting).is_some_and(|lines| {
            lines
                .iter()
                .any(|l| self.line_writes(l, ValueSource::ClearanceZ))
        })
    }

    /// After a tool change that lifted to the clearance height, Z is known
    /// again: the Z words take that height as the last one written, so the
    /// first rapid across does not repeat it.
    fn note_lift(&mut self) {
        if !self.tool_change_lifts() {
            return;
        }
        let saved = self.to;
        self.to = Point3::new(saved.x, saved.y, self.clear_z);
        let tokens: Vec<String> = self
            .post
            .variables
            .values()
            .filter(|def| def.source == ValueSource::Z && def.absolute)
            .map(|def| def.token.clone())
            .collect();
        for token in tokens {
            self.value_of(&token);
        }
        self.to = saved;
    }

    /// True when an output line writes a variable taken from `source`.
    fn line_writes(&self, line: &str, source: ValueSource) -> bool {
        tokens_in(line).iter().any(|t| {
            self.post
                .variables
                .get(&t.to_uppercase())
                .is_some_and(|def| def.source == source)
        })
    }

    /// Writes the feed rate change block before a cutting move whose rate
    /// differs from the cutting move before it in the same toolpath.
    fn feed_change(&mut self) {
        if self
            .last_feed
            .is_some_and(|last| (last - self.feed).abs() > 1e-9)
        {
            self.emit(BlockKind::FeedRateChange);
        }
        self.last_feed = Some(self.feed);
    }

    /// Writes a change of spindle speed or feeds inside the job, with no
    /// tool change: a new speed writes the whole SPINDLE_ON block, new feeds
    /// alone only the lines of it that set the move speeds (ShopBot's MS).
    /// G code posts carry their feeds on the cutting moves instead, so their
    /// SPINDLE_ON has no such line. A post that sets its move speeds there
    /// reads the part's own feed rate.
    fn change_rates(&mut self, new_speed: bool, new_feeds: bool, feed: f64) {
        if !new_speed && !new_feeds {
            return;
        }
        let Some(lines) = self.post.block(BlockKind::SpindleOn).cloned() else {
            return;
        };
        self.feed = feed;
        for line in lines {
            let sets_feeds = self.line_writes(&line, ValueSource::Feed)
                || self.line_writes(&line, ValueSource::PlungeFeed);
            if new_speed || sets_feeds {
                self.emit_line(&line);
            }
        }
    }

    fn first_of_run(&mut self, family: Family) -> bool {
        let inverse = self.inverse_time_mode() && matches!(family, Family::Feed | Family::Plunge);
        let first = inverse || self.run != Some(family);
        self.run = Some(family);
        first
    }

    fn emit(&mut self, kind: BlockKind) {
        let Some(lines) = self.post.block(kind).cloned() else {
            return;
        };
        for line in lines {
            self.emit_line(&line);
        }
    }

    fn emit_line(&mut self, template: &str) {
        if self.job.m6_applies_length
            && tokens_in(template)
                .iter()
                .any(|t| t.eq_ignore_ascii_case(LENGTH_LINE_TOKEN))
        {
            return;
        }
        if tokens_in(template).iter().any(|t| t == "TOOLS_USED") {
            if !self.post.print_toollist {
                return;
            }
            let saved = self.last.clone();
            for entry in self.tools_used.clone() {
                self.tool_list_entry = Some(entry);
                let text = self.substitute(template);
                self.push(&text);
            }
            self.tool_list_entry = None;
            self.last = saved;
            return;
        }
        let text = self.substitute(template);
        if text.is_empty() && !template.is_empty() {
            return;
        }
        self.push(&text);
    }

    fn push(&mut self, text: &str) {
        self.line_starts.push(self.out.len());
        self.out.push_str(text);
        self.out.push_str(&self.post.line_ending);
    }

    fn substitute(&mut self, template: &str) -> String {
        let mut out = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('[') {
            let after = &rest[start + 1..];
            let Some(end) = after.find(']') else {
                break;
            };
            out.push_str(&rest[..start]);
            let token = &after[..end];
            out.push_str(&self.value_of(token));
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out.trim_end().to_string()
    }

    fn value_of(&mut self, token: &str) -> String {
        // Tokens are matched whatever their case, as posts write
        // [ArcStartX] as well as [X].
        let upper = token.to_uppercase();
        if TEXT_TOKENS.contains(&upper.as_str()) {
            return comment_safe(&self.text_of(&upper));
        }
        let Some(mut def) = self.post.variables.get(&upper).cloned() else {
            return format!("[{token}]");
        };
        // A wrapped turn goes out on the letter the machine calls its axis.
        if let (Some(wrap), ValueSource::AngleA | ValueSource::AngleB) = (self.wrap, def.source) {
            let Some(word) = wrap.token_word(def.source) else {
                return String::new();
            };
            if let Some(end) = def.output.rfind(|c: char| c.is_ascii_alphabetic()) {
                def.output.replace_range(end..=end, word.letter());
            }
        }
        if def.source == ValueSource::InverseTime {
            // `[FI]`: every cutting move carries its own rate, and a rapid
            // has none.
            if !self.cutting {
                return String::new();
            }
            let text = def.format_inverse(self.inverse_rate() * def.scale);
            self.last.insert(def.token.clone(), text.clone());
            return text;
        }
        // A post with no `[FI]` of its own gets the inverse rate in its feed
        // words instead.
        let inverse = self.cutting
            && self.inverse_time_mode()
            && !self.post.variables.contains_key("FI")
            && matches!(def.source, ValueSource::Feed | ValueSource::PlungeFeed);
        if inverse {
            let text = def.format_inverse(self.inverse_rate() * def.scale);
            self.last.insert(def.token.clone(), text.clone());
            return text;
        }
        let raw = self.number_of(def.source);
        let scaled = match self.scaling_of(def.source) {
            Scaling::Length | Scaling::Rate => self.post.units.from_mm(raw),
            Scaling::None => raw,
        } * def.scale;
        let value = if def.absolute {
            scaled
        } else {
            scaled - self.previous_of(def.source) * def.scale
        };
        let text = def.format(value);
        if def.source == ValueSource::LineNumber {
            self.line_number += self.post.line_number_increment;
            if self
                .post
                .line_number_maximum
                .is_some_and(|max| self.line_number > max)
            {
                self.line_number = self.post.line_number_start;
            }
        }
        if def.always || def.source.is_drill_bank() || self.tool_list_entry.is_some() {
            self.last.insert(def.token.clone(), text.clone());
            return text;
        }
        if self.is_unchanged(&def, value, &text) {
            return String::new();
        }
        self.last.insert(def.token.clone(), text.clone());
        text
    }

    fn is_unchanged(&self, def: &VarDef, value: f64, text: &str) -> bool {
        if def.absolute {
            self.last.get(&def.token).is_some_and(|last| last == text)
        } else {
            // An incremental value repeats meaningfully, so only a move of
            // nothing at all is left out.
            value == 0.0
        }
    }

    fn previous_of(&self, source: ValueSource) -> f64 {
        let wrapped = self.wrap.and_then(|w| w.value_of(source, self.from));
        let raw = match source {
            _ if wrapped.is_some() => wrapped.unwrap_or_default(),
            ValueSource::X => self.from.x,
            ValueSource::Y => self.from.y,
            ValueSource::Z => self.from.z,
            ValueSource::ArcCentreX => self.from.x,
            ValueSource::ArcCentreY => self.from.y,
            ValueSource::ArcCentreZ => self.from.z,
            _ => 0.0,
        };
        match self.scaling_of(source) {
            Scaling::Length | Scaling::Rate => self.post.units.from_mm(raw),
            Scaling::None => raw,
        }
    }

    /// How a value scales with the post's units: a rotary word that holds a
    /// length does, as any length does; one in degrees does not.
    fn scaling_of(&self, source: ValueSource) -> Scaling {
        let turn = matches!(source, ValueSource::AngleA | ValueSource::AngleB);
        if turn && self.wrap.is_some_and(|w| w.is_linear()) {
            Scaling::Length
        } else {
            source.scaling()
        }
    }

    fn number_of(&self, source: ValueSource) -> f64 {
        if let Some(value) = self.wrap.and_then(|w| w.value_of(source, self.to)) {
            return value;
        }
        let tool = self.tool.as_ref();
        match source {
            ValueSource::AngleA | ValueSource::AngleB => 0.0,
            ValueSource::X => self.to.x,
            ValueSource::Y => self.to.y,
            ValueSource::Z => self.to.z,
            ValueSource::Feed => self.feed,
            ValueSource::PlungeFeed => self.plunge,
            ValueSource::RapidRate => self.post.rapid_rate,
            ValueSource::Spindle => self.spindle,
            ValueSource::ToolNumber => match &self.tool_list_entry {
                Some((number, _)) => f64::from(*number),
                None => f64::from(self.tool_number),
            },
            ValueSource::LineNumber => self.line_number,
            ValueSource::ArcCentreX => self.arc_centre.x,
            ValueSource::ArcCentreY => self.arc_centre.y,
            ValueSource::ArcCentreZ => self.from.z,
            ValueSource::ArcRadius => self.arc_centre.distance(self.from.xy()),
            ValueSource::Dwell => self.dwell,
            ValueSource::Power => self.power,
            ValueSource::SafeZ => self.job.safe_z,
            ValueSource::ClearanceZ => self.clear_z,
            ValueSource::HomeX => self.job.home.x,
            ValueSource::HomeY => self.job.home.y,
            ValueSource::HomeZ => self.job.home.z,
            ValueSource::MinX => self.bounds.min.x,
            ValueSource::MinY => self.bounds.min.y,
            ValueSource::MinZ => self.bounds.min.z,
            ValueSource::MaxX => self.bounds.max.x,
            ValueSource::MaxY => self.bounds.max.y,
            ValueSource::MaxZ => self.bounds.max.z,
            ValueSource::LengthX => self.bounds.max.x - self.bounds.min.x,
            ValueSource::LengthY => self.bounds.max.y - self.bounds.min.y,
            ValueSource::LengthZ => self.bounds.max.z - self.bounds.min.z,
            ValueSource::ToolDiameter => tool.map_or(0.0, Tool::diameter),
            ValueSource::ToolStepover => tool.map_or(0.0, |t| t.stepover),
            ValueSource::ToolPassDepth => tool.map_or(0.0, |t| t.pass_depth),
            ValueSource::CutFeed => self.tool_feed(),
            ValueSource::PreviousToolNumber => f64::from(self.previous_tool),
            ValueSource::ArcCentreAbsX => self.arc_centre.x,
            ValueSource::ArcCentreAbsY => self.arc_centre.y,
            ValueSource::ArcStartX => self.arc_start.x,
            ValueSource::ArcStartY => self.arc_start.y,
            ValueSource::ArcMidX => self.arc_mid().x,
            ValueSource::ArcMidY => self.arc_mid().y,
            ValueSource::ArcMidXInc => self.arc_mid().x - self.arc_start.x,
            ValueSource::ArcMidYInc => self.arc_mid().y - self.arc_start.y,
            ValueSource::ArcAngle => self.arc_sweep.to_degrees(),
            ValueSource::WrapDiameter => self.wrap.map_or(0.0, |w| w.diameter),
            ValueSource::InverseTime => self.inverse_rate(),
            ValueSource::DrillPattern => drill_pattern(&self.bank),
            ValueSource::DrillPatternX => drill_pattern(&self.bank_x),
            ValueSource::DrillPatternY => drill_pattern(&self.bank_y),
            ValueSource::CurrentDrill => f64::from(self.current_drill),
        }
    }

    /// One over the minutes the current move takes at its feed rate,
    /// whatever the units.
    fn inverse_rate(&self) -> f64 {
        let length = ((self.to.x - self.from.x).powi(2)
            + (self.to.y - self.from.y).powi(2)
            + (self.to.z - self.from.z).powi(2))
        .sqrt()
        .max(1e-3);
        self.feed / length
    }

    /// The cutting feed of the current toolpath.
    fn tool_feed(&self) -> f64 {
        self.cut_feed
    }

    /// The point half way round the current arc.
    fn arc_mid(&self) -> Point {
        let start = self.arc_centre.to(self.arc_start.xy());
        let radius = start.length();
        let angle = start.angle() + self.arc_sweep / 2.0;
        Point::new(
            self.arc_centre.x + radius * angle.cos(),
            self.arc_centre.y + radius * angle.sin(),
        )
    }

    fn text_of(&self, token: &str) -> String {
        match token {
            "TOOL_NAME" => match &self.tool_list_entry {
                Some((_, name)) => name.clone(),
                None => self
                    .tool
                    .as_ref()
                    .map(|t| t.name.clone())
                    .unwrap_or_default(),
            },
            "TOOLS_USED" => match &self.tool_list_entry {
                Some((number, name)) => format!("T{number} = {name}"),
                None => String::new(),
            },
            "TOOLNAME" => self.text_of("TOOL_NAME"),
            "TOOL_NOTES" => self
                .tool
                .as_ref()
                .map(|t| t.notes.clone())
                .unwrap_or_default(),
            "TP_DIR" => self.job.path_name.clone(),
            "TP_EXT" => self.post.file_extension.clone(),
            "FILE_NOTES" => self.job.notes.clone(),
            "PRODUCT" => "CarveStudio".to_string(),
            "TOOLPATHS_OUTPUT" => self.toolpath_names.join(", "),
            "TOOLPATH_NAME" => self.toolpath_name.clone(),
            "DL" => self
                .post
                .drill_list
                .clone()
                .unwrap_or_default()
                .format(&self.bank),
            "TOOLPATH_NOTES" => self.job.notes.clone(),
            "TP_FILENAME" if self.job.label.is_empty() => self.file_name.clone(),
            "TP_FILENAME" => format!("{}: {}", self.job.label, self.file_name),
            "PATHNAME" => self.job.path_name.clone(),
            "DATE" => self.job.date.clone(),
            "TIME" => self.job.time.clone(),
            "POST_NAME" => self.post.name.clone(),
            "FILE_EXTENSION" => self.post.file_extension.clone(),
            // G93 while the feed words are inverse time rates, so a flat job
            // through a rotary post keeps units per minute.
            "FEED_MODE" => if self.inverse_time_mode() {
                "G93"
            } else {
                "G94"
            }
            .to_string(),
            _ => String::new(),
        }
    }

    /// The output as the files a tape split run started.
    fn finish_pieces(self) -> Vec<String> {
        let mut bounds = vec![0];
        bounds.extend(self.piece_starts.iter().copied());
        bounds.push(self.out.len());
        bounds
            .windows(2)
            .map(|w| {
                let mut text = self.out[w[0]..w[1]].to_string();
                for (from, to) in &self.post.substitutions {
                    text = text.replace(*from, to);
                }
                text
            })
            .collect()
    }

    fn finish(self) -> String {
        if self.post.substitutions.is_empty() {
            return self.out;
        }
        let mut text = self.out;
        for (from, to) in &self.post.substitutions {
            text = text.replace(*from, to);
        }
        text
    }
}

/// How many straight moves one arc is broken into to stay inside the chord
/// tolerance.
fn arc_steps(radius: f64, sweep: f64) -> usize {
    if !radius.is_finite() || radius <= ARC_CHORD_TOLERANCE {
        return 1;
    }
    let ratio = (1.0 - ARC_CHORD_TOLERANCE / radius).clamp(-1.0, 1.0);
    let per_step = 2.0 * ratio.acos();
    if per_step <= 1e-9 {
        return 1;
    }
    let steps = (sweep.abs() / per_step).ceil() as usize;
    steps.clamp(1, ARC_MAX_STEPS)
}

/// Text the user typed (a tool or toolpath name, a note, a file name), made
/// safe to print inside a program: a round bracket would end a `( )` comment
/// early and leave the rest of the line for the controller to read as code,
/// and a line break would start a line of its own. Brackets become braces
/// and line breaks a space.
fn comment_safe(text: &str) -> String {
    text.replace('(', "{")
        .replace(')', "}")
        .replace("\r\n", " ")
        .replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::cam::toolpath::StrategyKind;
    use crate::post::parser::parse_post;

    const POST: &str = r#"
POST_NAME = "Test post (mm) (*.tap)"
FILE_EXTENSION = "tap"
UNITS = MM
RAPID_RATE = 3000
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE = [S|A|S|1.0]
VAR TOOL_NUMBER = [T|A|T|1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
begin HEADER
+ "G90G21"
+ "[TOOLPATH_NOTES]"
end
begin TOOLCHANGE
+ "[T]M6"
+ "[S]M3"
end
begin RAPID_MOVE
+ "G0[X][Y][Z]"
end
begin FEED_MOVE
+ "G1[X][Y][Z]"
end
begin FIRST_FEED_MOVE
+ "G1[X][Y][Z][F]"
end
begin PLUNGE_MOVE
+ "G1[Z][F]"
end
begin CW_ARC_MOVE
+ "G2[X][Y][I][J]"
end
begin CCW_ARC_MOVE
+ "G3[X][Y][I][J]"
end
begin FOOTER
+ "M5"
+ "M30"
end
"#;

    fn post() -> Post {
        parse_post("test", POST).expect("test post parses")
    }

    fn tool(name: &str, number: u32) -> Tool {
        Tool {
            name: name.to_string(),
            number,
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        }
    }

    fn toolpath(name: &str, tool: Tool) -> Toolpath {
        let mut tp = Toolpath::new(1, name, tool, StrategyKind::Profile);
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.feed = 1200.0;
        tp.plunge = 400.0;
        tp.spindle = 18000.0;
        tp
    }

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(|l| l.to_string()).collect()
    }

    #[test]
    fn a_simple_profile_posts_as_expected() {
        let mut tp = toolpath("profile 1", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, 5.0)),
            Move::plunge(Point3::new(10.0, 10.0, -2.0)),
            Move::feed(Point3::new(40.0, 10.0, -2.0)),
            Move::feed(Point3::new(40.0, 30.0, -2.0)),
            Move::rapid(Point3::new(40.0, 30.0, 5.0)),
        ];
        let text = post_process(&post(), &PostJob::default(), &[tp]).expect("posts");
        assert_eq!(
            lines(&text),
            vec![
                "G90G21",
                "T1M6",
                "S18000M3",
                "G0X10.000Y10.000Z5.000",
                "G1Z-2.000F400.0",
                "G1X40.000F1200.0",
                "G1Y30.000",
                "G0Z5.000",
                "M5",
                "M30",
            ]
        );
    }

    #[test]
    fn unchanged_values_are_left_out_and_the_first_move_carries_the_feed() {
        let mut tp = toolpath("profile 1", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::feed(Point3::new(10.0, 0.0, 0.0)),
            Move::feed(Point3::new(20.0, 0.0, 0.0)),
        ];
        let text = post_process(&post(), &PostJob::default(), &[tp]).expect("posts");
        let lines = lines(&text);
        assert!(
            lines.contains(&"G1X10.000Y0.000Z0.000F1200.0".to_string()),
            "{lines:?}"
        );
        assert!(lines.contains(&"G1X20.000".to_string()), "{lines:?}");
    }

    #[test]
    fn a_second_toolpath_with_the_same_tool_skips_the_tool_change() {
        let mut first = toolpath("one", tool("End Mill 6 mm", 1));
        first.moves = vec![Move::feed(Point3::new(10.0, 0.0, -1.0))];
        let mut second = toolpath("two", tool("End Mill 6 mm", 1));
        second.moves = vec![Move::feed(Point3::new(20.0, 0.0, -1.0))];
        let text = post_process(&post(), &PostJob::default(), &[first, second]).expect("posts");
        assert_eq!(text.matches("M6").count(), 1);
    }

    #[test]
    fn a_new_tool_forces_a_tool_change() {
        let mut first = toolpath("one", tool("End Mill 6 mm", 1));
        first.moves = vec![Move::feed(Point3::new(10.0, 0.0, -1.0))];
        let mut second = toolpath("two", tool("Ball Nose 3 mm", 2));
        second.moves = vec![Move::feed(Point3::new(20.0, 0.0, -1.0))];
        let text = post_process(&post(), &PostJob::default(), &[first, second]).expect("posts");
        assert_eq!(text.matches("M6").count(), 2);
        assert!(text.contains("T2M6"), "{text}");
        // The second toolpath's first move repeats Y and Z even though
        // they did not change.
        let after = text.split("T2M6").nth(1).unwrap_or_default();
        assert!(after.contains("X20.000Y0.000Z-1.000"), "{after}");
    }

    /// A drill bank toolpath with one firing at each `(x, drills)`, each
    /// a plunge and a retract; the drills are all on the horizontal arm
    /// and only drill 1 on the vertical one.
    fn bank_toolpath(shots: &[(f64, &[u32])]) -> Toolpath {
        let mut tp = toolpath("bank", tool("Drill 5 mm", 1));
        for (x, drills) in shots {
            tp.moves.push(Move::rapid(Point3::new(*x, 0.0, 5.0)));
            tp.moves.push(Move::plunge(Point3::new(*x, 0.0, -10.0)));
            tp.moves.push(Move::rapid(Point3::new(*x, 0.0, 5.0)));
            tp.firings.push(crate::cam::toolpath::BankFiring {
                at: Point::new(*x, 0.0),
                drills: drills.to_vec(),
                along_x: drills.to_vec(),
                along_y: drills.iter().copied().filter(|d| *d == 1).collect(),
            });
        }
        tp
    }

    #[test]
    fn a_pattern_drill_bank_post_sets_up_once_and_selects_each_new_set() {
        let source = format!(
            "{POST}\nVAR DRILL = [DB|A|P|1.0]\nVAR DRILL_X = [DBX|A|X|1.0]\n\
             VAR DRILL_Y = [DBY|A|Y|1.0]\nvar DRILL_LIST = [DL|D|,| ]\n\
             begin SETUP_DRILL_BANK\n+ \"(BANK ON)\"\nend\n\
             begin DRILL_BANK_PATTERN\n+ \"M100[DB][DBX][DBY] ([DL])\"\nend\n\
             begin FINAL_DRILL_BANK_ACTION\n+ \"(BANK OFF)\"\nend\n"
        );
        let bank = parse_post("bank", &source).expect("post parses");
        assert_eq!(bank.drill_bank_mode(), Some(DrillBankMode::Pattern));
        assert!(bank.summary().drill_bank);
        let tp = bank_toolpath(&[(10.0, &[1, 2, 3]), (106.0, &[1, 2, 3]), (74.0, &[4])]);
        let text =
            post_process(&bank, &PostJob::default(), std::slice::from_ref(&tp)).expect("posts");
        assert_eq!(
            lines(&text),
            vec![
                "G90G21",
                "T1M6",
                "S18000M3",
                "(BANK ON)",
                "G0X10.000Y0.000Z5.000",
                "M100P7X7Y1 (D1 D2,D3)",
                "G1Z-10.000F400.0",
                "G0Z5.000",
                // The same drills again: no new pattern.
                "G0X106.000",
                "G1Z-10.000F400.0",
                "G0Z5.000",
                "G0X74.000",
                "M100P8X8Y0 (D4)",
                "G1Z-10.000F400.0",
                "G0Z5.000",
                "(BANK OFF)",
                "M5",
                "M30",
            ]
        );
        // A post without the drill bank blocks refuses the toolpath.
        assert!(post_process(&post(), &PostJob::default(), &[tp]).is_err());
    }

    #[test]
    fn a_per_drill_bank_post_extends_and_retracts_each_drill() {
        let source = format!(
            "{POST}\nVAR CURRENT_DRILL = [CD|A|D|1.0]\n\
             begin SETUP_DRILL_BANK\n+ \"(BANK ON)\"\nend\n\
             begin DRILL_BANK_EXTEND_DRILL\n+ \"M61[CD]\"\nend\n\
             begin DRILL_BANK_RETRACT_DRILL\n+ \"M62[CD]\"\nend\n\
             begin FINAL_DRILL_BANK_ACTION\n+ \"(BANK OFF)\"\nend\n"
        );
        let bank = parse_post("bank", &source).expect("post parses");
        assert_eq!(bank.drill_bank_mode(), Some(DrillBankMode::PerDrill));
        let tp = bank_toolpath(&[(10.0, &[1, 2]), (42.0, &[2, 3])]);
        let text = post_process(&bank, &PostJob::default(), &[tp]).expect("posts");
        assert_eq!(
            lines(&text),
            vec![
                "G90G21",
                "T1M6",
                "S18000M3",
                "(BANK ON)",
                "G0X10.000Y0.000Z5.000",
                "M61D1",
                "M61D2",
                "G1Z-10.000F400.0",
                "G0Z5.000",
                "G0X42.000",
                // Drill 2 stays down, 1 goes up and 3 comes down.
                "M62D1",
                "M61D3",
                "G1Z-10.000F400.0",
                "G0Z5.000",
                // Every drill still down is raised before the final block.
                "M62D2",
                "M62D3",
                "(BANK OFF)",
                "M5",
                "M30",
            ]
        );
    }

    #[test]
    fn a_drill_bank_needs_setup_final_and_one_way_of_selecting() {
        let with = |blocks: &[&str]| {
            let mut source = format!("{POST}\nVAR CURRENT_DRILL = [CD|A|D|1.0]\n");
            for name in blocks {
                source.push_str(&format!("begin {name}\n+ \"({name} [CD])\"\nend\n"));
            }
            parse_post("bank", &source).expect("post parses")
        };
        let pattern = [
            "SETUP_DRILL_BANK",
            "DRILL_BANK_PATTERN",
            "FINAL_DRILL_BANK_ACTION",
        ];
        assert!(with(&pattern).supports_drill_bank());
        assert!(!with(&pattern[..2]).supports_drill_bank(), "no final block");
        assert!(!with(&pattern[1..]).supports_drill_bank(), "no setup block");
        let both = [
            "SETUP_DRILL_BANK",
            "DRILL_BANK_PATTERN",
            "DRILL_BANK_EXTEND_DRILL",
            "DRILL_BANK_RETRACT_DRILL",
            "FINAL_DRILL_BANK_ACTION",
        ];
        assert!(!with(&both).supports_drill_bank(), "pattern and per drill");
        assert!(with(
            &both[..1]
                .iter()
                .chain(&both[2..])
                .copied()
                .collect::<Vec<_>>()
        )
        .supports_drill_bank());
        assert!(
            !with(&[
                "SETUP_DRILL_BANK",
                "DRILL_BANK_EXTEND_DRILL",
                "FINAL_DRILL_BANK_ACTION"
            ])
            .supports_drill_bank(),
            "extend without retract"
        );
        let format = super::super::DrillListFormat {
            prefix: "D".to_string(),
            separator: ",".to_string(),
            first_separator: " ".to_string(),
        };
        assert_eq!(format.format(&[1, 3, 5]), "D1 D3,D5");
        assert_eq!(
            super::super::DrillListFormat::default().format(&[2, 4]),
            "2,4"
        );
        assert_eq!(drill_pattern(&[1, 3, 5]), 21.0);
    }

    #[test]
    fn a_tape_split_closes_the_drill_bank_and_sets_it_up_again() {
        let source = format!(
            "{POST}\nVAR DRILL = [DB|A|P|1.0]\nTAPE_SPLITTING = 8 0 \"%s_%d.tap\" 1 NO\n\
             begin SETUP_DRILL_BANK\n+ \"(BANK ON)\"\nend\n\
             begin DRILL_BANK_PATTERN\n+ \"M100[DB]\"\nend\n\
             begin FINAL_DRILL_BANK_ACTION\n+ \"(BANK OFF)\"\nend\n"
        );
        let bank = parse_post("bank", &source).expect("post parses");
        let tp = bank_toolpath(&[(10.0, &[1, 2]), (42.0, &[1, 2]), (74.0, &[1, 2])]);
        let job = PostJob {
            file_name: "job.tap".to_string(),
            ..PostJob::default()
        };
        let files = post_process_pieces(&bank, &job, &[tp], None).expect("posts");
        assert!(files.len() > 1, "{files:?}");
        for file in &files {
            let lines = lines(file);
            let on = lines.iter().position(|l| l == "(BANK ON)").expect("setup");
            let off = lines.iter().position(|l| l == "(BANK OFF)").expect("final");
            let pattern = lines.iter().position(|l| l == "M100P3").expect("pattern");
            assert!(on < pattern && pattern < off, "{file}");
            assert_eq!(lines.iter().filter(|l| *l == "(BANK ON)").count(), 1);
        }
    }

    #[test]
    fn a_merged_toolpath_runs_each_part_at_its_own_rates() {
        let source = format!("{POST}\nbegin SPINDLE_ON\n+ \"(spindle [S])\"\nend\n");
        let speeds = parse_post("speeds", &source).expect("post parses");
        let mut tp = toolpath("merged", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::feed(Point3::new(10.0, 0.0, -1.0)),
            Move::feed(Point3::new(20.0, 0.0, -1.0)),
        ];
        tp.rates = vec![
            crate::cam::toolpath::RateChange {
                from: 0,
                feed: 1200.0,
                plunge: 400.0,
                spindle: 18000.0,
            },
            crate::cam::toolpath::RateChange {
                from: 1,
                feed: 600.0,
                plunge: 200.0,
                spindle: 12000.0,
            },
        ];
        let text = post_process(&speeds, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("F1200.0"), "{text}");
        assert!(text.contains("F600.0"), "{text}");
        assert!(text.contains("(spindle S12000)"), "{text}");
    }

    #[test]
    fn new_feeds_alone_write_only_the_spindle_on_lines_that_set_feeds() {
        let source =
            format!("{POST}\nbegin SPINDLE_ON\n+ \"(feeds [F])\"\n+ \"(spindle [S])\"\nend\n");
        let speeds = parse_post("speeds", &source).expect("post parses");
        let rate = |from, feed| crate::cam::toolpath::RateChange {
            from,
            feed,
            plunge: 400.0,
            spindle: 18000.0,
        };
        let mut merged = toolpath("merged", tool("End Mill 6 mm", 1));
        merged.feed = 1200.0;
        merged.spindle = 18000.0;
        merged.moves = vec![
            Move::feed(Point3::new(10.0, 0.0, -1.0)),
            Move::feed(Point3::new(20.0, 0.0, -1.0)),
        ];
        merged.rates = vec![rate(0, 1200.0), rate(1, 600.0)];
        let text = post_process(&speeds, &PostJob::default(), &[merged]).expect("posts");
        assert!(text.contains("(feeds F600.0)"), "{text}");
        assert_eq!(text.matches("(spindle").count(), 1, "{text}");
        // The same between two toolpaths with one tool: no tool change
        // writes the second one's feeds or speed.
        let mut one = toolpath("one", tool("End Mill 6 mm", 1));
        one.feed = 1200.0;
        one.spindle = 18000.0;
        one.moves = vec![Move::feed(Point3::new(10.0, 0.0, -1.0))];
        let mut two = one.clone();
        two.feed = 600.0;
        let text = post_process(&speeds, &PostJob::default(), &[one.clone(), two]).expect("posts");
        assert!(text.contains("(feeds F600.0)"), "{text}");
        assert_eq!(text.matches("(spindle").count(), 1, "{text}");
        let mut three = one.clone();
        three.spindle = 12000.0;
        let text = post_process(&speeds, &PostJob::default(), &[one, three]).expect("posts");
        assert!(text.contains("(spindle S12000)"), "{text}");
    }

    #[test]
    fn arcs_are_written_with_incremental_centres() {
        let mut tp = toolpath("arc", tool("End Mill 6 mm", 1));
        tp.home = Point3::new(10.0, 0.0, -1.0);
        tp.moves = vec![
            Move::arc_ccw(Point::ORIGIN, Point3::new(0.0, 10.0, -1.0)),
            Move::arc_cw(Point::ORIGIN, Point3::new(10.0, 0.0, -1.0)),
        ];
        let text = post_process(&post(), &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("G3X0.000Y10.000I-10.000J0.000"), "{text}");
        assert!(text.contains("G2X10.000Y0.000I0.000J-10.000"), "{text}");
    }

    #[test]
    fn a_tool_change_block_prints_the_feed_of_the_toolpath_it_starts() {
        let source = POST.replace("+ \"[S]M3\"\n", "+ \"[S]M3\"\n+ \"(speed [F])\"\n");
        let speeds = parse_post("speeds", &source).expect("post parses");
        let mut first = toolpath("one", tool("End Mill 6 mm", 1));
        first.feed = 2400.0;
        first.moves = vec![Move::feed(Point3::new(10.0, 0.0, -1.0))];
        let mut second = toolpath("two", tool("Drill 6 mm", 2));
        second.feed = 300.0;
        second.moves = vec![Move::feed(Point3::new(20.0, 0.0, -1.0))];
        let text = post_process(&speeds, &PostJob::default(), &[first, second]).expect("posts");
        let lines = lines(&text);
        let first_speed = lines.iter().position(|l| l.starts_with("(speed"));
        assert_eq!(
            first_speed.map(|i| lines[i].as_str()),
            Some("(speed F2400.0)"),
            "{lines:?}"
        );
        assert!(lines.contains(&"(speed F300.0)".to_string()), "{lines:?}");
    }

    #[test]
    fn a_post_without_arc_blocks_writes_short_straight_moves() {
        let source = POST
            .replace("begin CW_ARC_MOVE\n+ \"G2[X][Y][I][J]\"\nend\n", "")
            .replace("begin CCW_ARC_MOVE\n+ \"G3[X][Y][I][J]\"\nend\n", "");
        let flat = parse_post("flat", &source).expect("post parses");
        assert!(!flat.supports_arcs());
        let mut tp = toolpath("arc", tool("End Mill 6 mm", 1));
        tp.home = Point3::new(10.0, 0.0, -1.0);
        tp.moves = vec![Move::arc_ccw(Point::ORIGIN, Point3::new(-10.0, 0.0, -1.0))];
        let text = post_process(&flat, &PostJob::default(), &[tp]).expect("posts");
        assert!(!text.contains("G3"), "{text}");
        let moves = text.matches("G1").count();
        assert!(
            moves > 10,
            "a half circle should take many moves, got {moves}"
        );
        assert!(text.contains("X-10.000"), "{text}");
    }

    #[test]
    fn an_inch_post_converts_lengths_and_feeds() {
        let inch =
            parse_post("inch", &POST.replace("UNITS = MM", "UNITS = INCHES")).expect("post parses");
        let mut tp = toolpath("profile", tool("End Mill 1/4 inch", 1));
        tp.feed = 254.0;
        tp.moves = vec![Move::feed(Point3::new(25.4, 0.0, 0.0))];
        let text = post_process(&inch, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("X1.000"), "{text}");
        assert!(text.contains("F10.0"), "{text}");
    }

    #[test]
    fn the_spindle_speed_is_clamped_to_the_machine_range() {
        let mut clamped = post();
        clamped.spindle_speed_range = Some((8000.0, 12000.0));
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.moves = vec![Move::feed(Point3::new(10.0, 0.0, 0.0))];
        let text = post_process(&clamped, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("S12000M3"), "{text}");
    }

    #[test]
    fn a_line_that_comes_out_empty_is_left_out() {
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.moves = vec![Move::feed(Point3::new(10.0, 0.0, 0.0))];
        let text = post_process(&post(), &PostJob::default(), &[tp.clone()]).expect("posts");
        assert!(!text.starts_with("G90G21\n\n"), "{text}");
        let job = PostJob {
            notes: "cut carefully".to_string(),
            ..PostJob::default()
        };
        let with_notes = post_process(&post(), &job, &[tp]).expect("posts");
        assert!(with_notes.contains("cut carefully"), "{with_notes}");
    }

    #[test]
    fn the_tool_list_repeats_its_line_once_per_tool() {
        let mut listed = post();
        listed.print_toollist = true;
        listed.blocks.insert(
            BlockKind::Header,
            vec!["G90G21".to_string(), "([TOOLS_USED])".to_string()],
        );
        let mut first = toolpath("one", tool("End Mill 6 mm", 1));
        first.moves = vec![Move::feed(Point3::new(10.0, 0.0, -1.0))];
        let mut second = toolpath("two", tool("Ball Nose 3 mm", 2));
        second.moves = vec![Move::feed(Point3::new(20.0, 0.0, -1.0))];
        let text = post_process(
            &listed,
            &PostJob::default(),
            &[first.clone(), second.clone()],
        )
        .expect("posts");
        assert!(text.contains("(T1 = End Mill 6 mm)"), "{text}");
        assert!(text.contains("(T2 = Ball Nose 3 mm)"), "{text}");
        // The list does not swallow the tool change that follows it.
        assert!(text.contains("T1M6"), "{text}");
        listed.print_toollist = false;
        let quiet = post_process(&listed, &PostJob::default(), &[first, second]).expect("posts");
        assert!(!quiet.contains("(T1"), "{quiet}");
    }

    #[test]
    fn line_numbers_count_up_by_the_declared_step() {
        let mut numbered = post();
        numbered.line_number_start = 10.0;
        numbered.line_number_increment = 10.0;
        numbered.variables.insert(
            "N".to_string(),
            VarDef {
                name: "LINE_NUMBER".to_string(),
                token: "N".to_string(),
                source: ValueSource::LineNumber,
                absolute: true,
                always: true,
                zero_pad: false,
                force_sign: false,
                output: "N".to_string(),
                int_digits: 1,
                decimals: 0,
                scale: 1.0,
            },
        );
        numbered
            .blocks
            .insert(BlockKind::Header, vec!["[N]G90".to_string()]);
        numbered
            .blocks
            .insert(BlockKind::FeedMove, vec!["[N]G1[X]".to_string()]);
        numbered
            .blocks
            .insert(BlockKind::FirstFeedMove, vec!["[N]G1[X][F]".to_string()]);
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::feed(Point3::new(10.0, 0.0, 0.0)),
            Move::feed(Point3::new(20.0, 0.0, 0.0)),
        ];
        let text = post_process(&numbered, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.starts_with("N10G90"), "{text}");
        assert!(text.contains("N20G1X10.000F1200.0"), "{text}");
        assert!(text.contains("N30G1X20.000"), "{text}");
    }

    #[test]
    fn a_retract_block_is_used_for_a_lift_straight_up() {
        let mut lifted = post();
        lifted
            .blocks
            .insert(BlockKind::RetractMove, vec!["G0Z[Z](retract)".to_string()]);
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.home = Point3::new(0.0, 0.0, -2.0);
        tp.moves = vec![Move::rapid(Point3::new(0.0, 0.0, 5.0))];
        let text = post_process(&lifted, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("(retract)"), "{text}");
    }

    #[test]
    fn substitutions_are_applied_to_the_finished_text() {
        let mut swapped = post();
        swapped.substitutions = vec![('G', "g".to_string())];
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.moves = vec![Move::feed(Point3::new(10.0, 0.0, 0.0))];
        let text = post_process(&swapped, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("g1"), "{text}");
        assert!(!text.contains("G1"), "{text}");
    }

    #[test]
    fn the_line_ending_is_the_one_the_post_asks_for() {
        let mut crlf = post();
        crlf.line_ending = "\r\n".to_string();
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.moves = vec![Move::feed(Point3::new(10.0, 0.0, 0.0))];
        let text = post_process(&crlf, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("\r\n"), "{text:?}");
    }

    #[test]
    fn a_dwell_is_written_when_the_post_asks_for_one() {
        let mut dwelling = post();
        dwelling.variables.insert(
            "DWELL".to_string(),
            VarDef {
                name: "DWELL_TIME".to_string(),
                token: "DWELL".to_string(),
                source: ValueSource::Dwell,
                absolute: true,
                always: true,
                zero_pad: false,
                force_sign: false,
                output: "P".to_string(),
                int_digits: 1,
                decimals: 2,
                scale: 1.0,
            },
        );
        dwelling
            .blocks
            .insert(BlockKind::DwellMove, vec!["G4[DWELL]".to_string()]);
        let mut tp = toolpath("drill", tool("Drill 6 mm", 1));
        tp.moves = vec![
            Move::plunge(Point3::new(0.0, 0.0, -5.0)),
            Move::dwell(1.5, Point3::new(0.0, 0.0, -5.0)),
        ];
        let text = post_process(&dwelling, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("G4P1.50"), "{text}");
    }

    #[test]
    fn a_laser_toolpath_turns_the_beam_on_and_off() {
        let mut laser = post();
        laser.variables.insert(
            "POWER".to_string(),
            VarDef {
                name: "LASER_POWER".to_string(),
                token: "POWER".to_string(),
                source: ValueSource::Power,
                absolute: true,
                always: true,
                zero_pad: false,
                force_sign: false,
                output: "S".to_string(),
                int_digits: 1,
                decimals: 0,
                scale: 1.0,
            },
        );
        laser
            .blocks
            .insert(BlockKind::JetToolPower, vec!["[POWER]".to_string()]);
        laser
            .blocks
            .insert(BlockKind::JetToolOn, vec!["M3".to_string()]);
        laser
            .blocks
            .insert(BlockKind::JetToolOff, vec!["M5".to_string()]);
        let mut tp = toolpath("laser", tool("Laser", 1));
        tp.tool.tool_type = ToolGeometry::LaserTool {
            power: 75.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
        };
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 0.0)),
            Move::feed(Point3::new(10.0, 0.0, 0.0)),
            Move::rapid(Point3::new(20.0, 0.0, 0.0)),
            Move::feed(Point3::new(30.0, 0.0, 0.0)),
        ];
        let text = post_process(&laser, &PostJob::default(), &[tp]).expect("posts");
        assert_eq!(text.matches("S75").count(), 2, "{text}");
        assert_eq!(text.lines().filter(|l| *l == "M3").count(), 2, "{text}");
        // Once between the two cuts, once at the end of the toolpath and
        // once in the footer.
        assert_eq!(text.lines().filter(|l| *l == "M5").count(), 3, "{text}");
    }

    #[test]
    fn a_move_with_its_own_power_changes_the_laser_power() {
        let post = crate::post::parser::parse_post(
            "laser_grbl",
            include_str!("../../assets/posts/laser_grbl.pp"),
        )
        .expect("the laser post");
        let mut tp = toolpath("picture", tool("Laser", 1));
        tp.tool.tool_type = ToolGeometry::LaserTool {
            power: 75.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
        };
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 0.0)),
            Move::feed(Point3::new(1.0, 0.0, 0.0)).with_power(20.0),
            Move::feed(Point3::new(2.0, 0.0, 0.0)).with_power(20.0),
            Move::feed(Point3::new(3.0, 0.0, 0.0)).with_power(55.5),
            Move::feed(Point3::new(4.0, 0.0, 0.0)),
            Move::rapid(Point3::new(0.0, 1.0, 0.0)),
            Move::feed(Point3::new(4.0, 1.0, 0.0)).with_power(0.0),
        ];
        let text = post_process(&post, &PostJob::default(), &[tp]).expect("posts");
        let lines: Vec<&str> = text.lines().collect();
        let at = |needle: &str| {
            lines
                .iter()
                .position(|l| *l == needle)
                .unwrap_or_else(|| panic!("{needle} in {text}"))
        };
        // Power is a percentage scaled to GRBL's 0 to 1000.
        assert!(at("S200") < at("M4"), "{text}");
        assert!(at("S555") > at("M4"), "{text}");
        assert_eq!(text.matches("S200").count(), 1, "{text}");
        assert!(at("S0") > at("S555"), "{text}");
    }

    #[test]
    fn the_job_tokens_report_the_extents_of_the_output() {
        let mut bounded = post();
        for token in ["XMIN", "YMAX", "ZLENGTH"] {
            bounded.variables.insert(
                token.to_string(),
                VarDef {
                    name: token.to_string(),
                    token: token.to_string(),
                    source: super::super::parser::value_source(token).expect("known token"),
                    absolute: true,
                    always: true,
                    zero_pad: false,
                    force_sign: false,
                    output: format!("{token}="),
                    int_digits: 1,
                    decimals: 1,
                    scale: 1.0,
                },
            );
        }
        bounded.blocks.insert(
            BlockKind::Header,
            vec!["([XMIN] [YMAX] [ZLENGTH])".to_string()],
        );
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.moves = vec![
            Move::rapid(Point3::new(-5.0, 0.0, 5.0)),
            Move::feed(Point3::new(-5.0, 40.0, -3.0)),
        ];
        tp.recompute_bounds();
        let text = post_process(&bounded, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("XMIN=-5.0"), "{text}");
        assert!(text.contains("YMAX=40.0"), "{text}");
        assert!(text.contains("ZLENGTH=23.0"), "{text}");
    }

    #[test]
    fn a_broken_toolpath_is_refused() {
        let mut tp = toolpath("profile", tool("End Mill 6 mm", 1));
        tp.feed = 0.0;
        assert!(post_process(&post(), &PostJob::default(), &[tp]).is_err());
        let empty = Post::default();
        assert!(post_process(&empty, &PostJob::default(), &[]).is_err());
    }

    #[test]
    fn an_empty_toolpath_list_still_writes_the_frame() {
        let text = post_process(&post(), &PostJob::default(), &[]).expect("posts");
        assert_eq!(lines(&text), vec!["G90G21", "M5", "M30"]);
    }

    #[test]
    fn arc_steps_follow_the_chord_tolerance() {
        assert_eq!(arc_steps(0.0, 1.0), 1);
        assert!(arc_steps(50.0, std::f64::consts::PI) > arc_steps(5.0, std::f64::consts::PI));
        assert!(arc_steps(1e9, std::f64::consts::TAU) <= ARC_MAX_STEPS);
    }

    /// The test post with extra settings and blocks spliced in before the
    /// footer.
    fn post_with(settings: &str, blocks: &str) -> Post {
        let text = POST.replacen(
            "RAPID_RATE = 3000",
            &format!("RAPID_RATE = 3000\n{settings}"),
            1,
        );
        let text = text.replacen("begin FOOTER", &format!("{blocks}\nbegin FOOTER"), 1);
        parse_post("extended", &text).expect("extended post parses")
    }

    fn quarter_arc(z_end: f64) -> Toolpath {
        let mut tp = toolpath("arc", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::rapid(Point3::new(10.0, 0.0, 5.0)),
            Move::plunge(Point3::new(10.0, 0.0, -1.0)),
            Move::arc_ccw(Point::ORIGIN, Point3::new(0.0, 10.0, z_end)),
        ];
        tp
    }

    #[test]
    fn arcs_that_move_in_z_use_the_helical_blocks_when_defined() {
        let helical = post_with(
            "",
            "begin CW_HELICAL_ARC_MOVE\n+ \"G2[X][Y][Z][I][J] (helix)\"\nend\n\
             begin CCW_HELICAL_ARC_MOVE\n+ \"G3[X][Y][Z][I][J] (helix)\"\nend",
        );
        assert!(helical.supports_helical_arcs());
        let text =
            post_process(&helical, &PostJob::default(), &[quarter_arc(-2.0)]).expect("posts");
        assert!(
            text.contains("G3X0.000Y10.000Z-2.000I-10.000J0.000 (helix)"),
            "{text}"
        );
        let flat =
            post_process(&helical, &PostJob::default(), &[quarter_arc(-1.0)]).expect("posts");
        assert!(flat.contains("G3X0.000Y10.000I-10.000J0.000"), "{flat}");
        assert!(!flat.contains("helix"));
        // Without them, and with arc blocks that do not write Z, a helix is
        // broken into straight moves that go down as they go round.
        let plain =
            post_process(&post(), &PostJob::default(), &[quarter_arc(-2.0)]).expect("posts");
        assert!(!plain.contains("G3"), "{plain}");
        assert!(plain.contains("G1X0.000Y10.000Z-2.000"), "{plain}");
        let flat = post_process(&post(), &PostJob::default(), &[quarter_arc(-1.0)]).expect("posts");
        assert!(flat.contains("G3X0.000Y10.000I-10.000J0.000"), "{flat}");
    }

    #[test]
    fn a_helix_keeps_its_depth_through_arc_blocks_that_write_z() {
        let source = POST
            .replace("\"G2[X][Y][I][J]\"", "\"G2[X][Y][Z][I][J]\"")
            .replace("\"G3[X][Y][I][J]\"", "\"G3[X][Y][Z][I][J]\"");
        let with_z = parse_post("with z", &source).expect("post parses");
        assert!(with_z.arcs_write_z());
        assert!(!with_z.supports_helical_arcs());
        let text = post_process(&with_z, &PostJob::default(), &[quarter_arc(-2.0)]).expect("posts");
        assert!(
            text.contains("G3X0.000Y10.000Z-2.000I-10.000J0.000"),
            "{text}"
        );
    }

    #[test]
    fn an_arc_whose_centre_words_would_round_to_zero_is_a_straight_move() {
        // A 0.0004 mm arc: at three decimals I and J are both 0.000, which
        // LinuxCNC and UCCNC refuse as a zero radius arc.
        let mut tp = toolpath("tiny", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::feed(Point3::new(20.0, 10.0, -1.0)),
            Move::arc_cw(
                Point::new(20.0, 10.0004),
                Point3::new(20.0004, 10.0004, -1.0),
            ),
            Move::feed(Point3::new(20.0004, 20.0, -1.0)),
        ];
        let text = post_process(&post(), &PostJob::default(), &[tp]).expect("posts");
        assert!(!text.lines().any(|l| l.starts_with("G2")), "{text}");
        assert!(!text.contains("I0.000J0.000"), "{text}");
        assert!(text.contains("G1Y20.000"), "{text}");
        // An arc just wide enough keeps its centre.
        let mut tp = toolpath("small", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::feed(Point3::new(20.0, 10.0, -1.0)),
            Move::arc_cw(Point::new(20.0, 10.05), Point3::new(20.05, 10.05, -1.0)),
        ];
        let text = post_process(&post(), &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("G2X20.050Y10.050I0.000J0.050"), "{text}");
    }

    #[test]
    fn arcs_outside_the_radius_limits_become_straight_moves() {
        let small = post_with("MIN_ARC_RADIUS = 20", "");
        let text = post_process(&small, &PostJob::default(), &[quarter_arc(-1.0)]).expect("posts");
        assert!(!text.contains("G3"), "{text}");
        assert!(text.lines().filter(|l| l.starts_with("G1")).count() > 5);
        let large = post_with("MAX_ARC_RADIUS = 5", "");
        let text = post_process(&large, &PostJob::default(), &[quarter_arc(-1.0)]).expect("posts");
        assert!(!text.contains("G3"));
        let fits = post_with("MIN_ARC_RADIUS = 1\nMAX_ARC_RADIUS = 100", "");
        let text = post_process(&fits, &PostJob::default(), &[quarter_arc(-1.0)]).expect("posts");
        assert!(text.contains("G3"));
    }

    #[test]
    fn spindle_on_initial_rapid_and_feed_rate_change_blocks() {
        let extended = post_with(
            "",
            "begin SPINDLE_ON\n+ \"(spindle on)\"\nend\n\
             begin INITIAL_RAPID_MOVE\n+ \"G0[X][Y][Z] (initial)\"\nend\n\
             begin FEED_RATE_CHANGE\n+ \"(feed [F])\"\nend",
        );
        let mut tp = toolpath("profile 1", tool("End Mill 6 mm", 1));
        tp.moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, 5.0)),
            Move::plunge(Point3::new(10.0, 10.0, -2.0)),
            Move::feed(Point3::new(40.0, 10.0, -2.0)),
            Move::rapid(Point3::new(40.0, 10.0, 5.0)),
            Move::rapid(Point3::new(50.0, 10.0, 5.0)),
        ];
        let text = post_process(&extended, &PostJob::default(), &[tp]).expect("posts");
        let lines = lines(&text);
        let at = |needle: &str| lines.iter().position(|l| l.contains(needle));
        assert!(at("S18000M3") < at("(spindle on)"));
        assert_eq!(lines.iter().filter(|l| l.contains("(initial)")).count(), 1);
        assert!(at("(initial)") < at("G1Z-2.000"));
        // Plunge rate to cut rate is a change; the first cut move is not.
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("(feed")).count(),
            1,
            "{text}"
        );
        assert!(at("(feed F1200.0)") < at("G1X40.000"));
    }

    #[test]
    fn reference_tokens_are_matched_in_any_case() {
        let extended = post_with(
            "VAR ARC_START_X = [ArcStartX|AF|SX|1.3]\n\
             VAR ARC_MID_X = [ArcMidX|AF|MX|1.3]\n\
             VAR ARC_CENTRE_I_ABS = [IA|AF|IA|1.3]\n\
             VAR ARC_ANGLE = [Angle|AF|ANG|1.1]\n\
             VAR CUT_RATE = [FC|AF|FC|1.0]\n\
             VAR PREVIOUS_TOOL = [TP|AF|TP|1.0]\n\
             VAR TOOL_DIAMETER = [TDIA|AF|D|1.1]",
            "begin FIRST_CCW_ARC_MOVE\n+ \"G3[X][Y][I][J] [ArcStartX][ArcMidX][IA][Angle]\"\nend\n\
             begin NEW_SEGMENT_POST_TOOLCHANGE\n+ \"([FC] [TP] [TDIA] [TOOL_NOTES] [PRODUCT] [TOOLNAME])\"\nend",
        );
        let mut tp = quarter_arc(-1.0);
        tp.tool.notes = "sharp".to_string();
        let text = post_process(&extended, &PostJob::default(), &[tp]).expect("posts");
        assert!(text.contains("SX10.000MX7.071IA0.000ANG90.0"), "{text}");
        assert!(
            text.contains("(FC1200 TP0 D6.0 sharp CarveStudio End Mill 6 mm)"),
            "{text}"
        );
    }

    #[test]
    fn line_numbers_start_again_after_the_maximum() {
        let numbered = post_with(
            "LINE_NUMBER = 10\nLINE_NUMBER_INCREMENT = 10\nLINE_NUMBER_MAXIMUM = 30\n\
             VAR LINE_NUMBER = [N|A|N|1.0]",
            "",
        );
        let text = numbered.blocks.get(&BlockKind::Footer).cloned();
        assert!(text.is_some());
        let mut post = numbered;
        post.blocks.insert(
            BlockKind::Footer,
            vec![
                "[N]M5".to_string(),
                "[N]M30".to_string(),
                "[N]M2".to_string(),
                "[N]M0".to_string(),
            ],
        );
        let mut tp = toolpath("p", tool("End Mill 6 mm", 1));
        tp.moves = vec![Move::rapid(Point3::new(1.0, 1.0, 5.0))];
        let out = post_process(&post, &PostJob::default(), &[tp]).expect("posts");
        let numbers: Vec<&str> = out.lines().filter(|l| l.starts_with('N')).collect();
        assert_eq!(numbers, vec!["N10M5", "N20M30", "N30M2", "N10M0"]);
    }

    #[test]
    fn settings_for_other_programs_are_read_and_listed() {
        let post = post_with(
            "PRINT_DIRECT = YES\nRAPID_PLUNGE_TO_STARTZ = YES\nTAPE_SPLITTING = 1000\n\
             LASER_SUPPORT = YES\nROTARY_WRAP_Y = A",
            "",
        );
        assert_eq!(
            post.ignored_settings,
            vec!["PRINT_DIRECT", "RAPID_PLUNGE_TO_STARTZ"]
        );
        assert!(post.laser_support && post.supports_jet());
        assert_eq!(
            post.rotary_wrap_y,
            Some(super::super::rotary::RotaryWord::A)
        );
        assert_eq!(post.rotary_wrap_x, None);
        let split = post.tape_splitting.expect("tape splitting");
        assert_eq!(
            (split.max_lines, split.line_tolerance, split.start_index),
            (1000, 0, 1)
        );
        assert!(split.index_on_first);
    }

    /// Ten slots, each plunged, cut and lifted straight up.
    fn slots() -> Toolpath {
        let mut tp = toolpath("slots", tool("End Mill 6 mm", 1));
        let mut moves = Vec::new();
        for k in 0..10 {
            let y = f64::from(k) * 10.0;
            moves.push(Move::rapid(Point3::new(0.0, y, 5.0)));
            moves.push(Move::plunge(Point3::new(0.0, y, -1.0)));
            moves.push(Move::feed(Point3::new(20.0, y, -1.0)));
            moves.push(Move::feed(Point3::new(40.0, y, -1.0)));
            moves.push(Move::rapid(Point3::new(40.0, y, 5.0)));
        }
        tp.moves = moves;
        tp
    }

    #[test]
    fn every_tape_piece_restarts_the_spindle_with_the_tool_it_cuts_with() {
        let post = post_with("TAPE_SPLITTING = 14 4 \"%s_%d.tap\" 1 NO", "");
        let first = slots();
        let mut second = slots();
        second.name = "second".to_string();
        second.tool = tool("End Mill 3 mm", 2);
        second.tool_number = 2;
        second.spindle = 9000.0;
        let pieces =
            post_process_pieces(&post, &PostJob::default(), &[first, second], None).expect("posts");
        assert!(pieces.len() > 3, "{pieces:?}");
        let mut tool_now = "T1M6".to_string();
        for (index, piece) in pieces.iter().enumerate() {
            let piece_lines = lines(piece);
            let first_cut = piece_lines
                .iter()
                .position(|l| l.starts_with("G1"))
                .expect("every piece cuts");
            let before = &piece_lines[..first_cut];
            // The footer stops the spindle, so each piece starts it again,
            // with the tool loaded at that point, before it cuts.
            if let Some(change) = before.iter().rev().find(|l| l.ends_with("M6")) {
                tool_now.clone_from(change);
            }
            assert!(
                before.iter().any(|l| l.ends_with("M3")),
                "piece {index} cuts with the spindle stopped:\n{piece}"
            );
            if index > 0 {
                assert!(before.iter().any(|l| l.ends_with("M6")), "{piece}");
                // The first move of a piece gives every axis.
                let first_move = before
                    .iter()
                    .find(|l| l.starts_with("G0"))
                    .expect("a rapid");
                assert!(
                    first_move.contains('X')
                        && first_move.contains('Y')
                        && first_move.contains('Z'),
                    "{piece}"
                );
            }
            assert!(piece_lines.len() <= 16, "{piece}");
        }
        assert_eq!(tool_now, "T2M6", "the last pieces load the second tool");
    }

    #[test]
    fn a_tape_split_inside_a_cut_goes_back_down_before_cutting_on() {
        let post = post_with("TAPE_SPLITTING = 10", "");
        let mut tp = toolpath("long", tool("End Mill 6 mm", 1));
        tp.safe_z = 5.0;
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -1.0)),
        ];
        for k in 1..=30 {
            let y = if k % 2 == 0 { 0.0 } else { 1.0 };
            tp.moves
                .push(Move::feed(Point3::new(f64::from(k), y, -1.0)));
        }
        let pieces = post_process_pieces(&post, &PostJob::default(), &[tp], None).expect("posts");
        assert!(pieces.len() > 2, "{pieces:?}");
        for piece in &pieces[1..] {
            let piece_lines = lines(piece);
            let start = piece_lines
                .iter()
                .position(|l| l == "S18000M3")
                .expect("the spindle starts");
            let back = &piece_lines[start + 1..start + 3];
            assert!(
                back[0].starts_with("G0X") && back[0].ends_with("Z5.000"),
                "{piece}"
            );
            assert_eq!(back[1], "G1Z-1.000F400.0", "{piece}");
            assert!(piece_lines[start + 3].contains("F1200.0"), "{piece}");
        }
    }

    #[test]
    fn every_tape_piece_numbers_from_the_start_and_names_its_own_file() {
        let text = POST
            .replacen(
                "RAPID_RATE = 3000",
                "RAPID_RATE = 3000\nTAPE_SPLITTING = 14 4 \"%s_%d.tap\" 1 NO\n\
                 LINE_NUMBER_START = 10\nLINE_NUMBER_INCREMENT = 10\n\
                 VAR LINE_NUMBER = [N|A|N|1.0]",
                1,
            )
            .replace(
                "+ \"G90G21\"",
                "+ \"[N](FILE [TP_FILENAME])\"\n+ \"[N]G90G21\"",
            );
        let post = parse_post("numbered", &text).expect("numbered post parses");
        let job = PostJob {
            file_name: "job.tap".to_string(),
            ..PostJob::default()
        };
        let pieces = post_process_pieces(&post, &job, &[slots()], None).expect("posts");
        assert!(pieces.len() > 2, "{pieces:?}");
        for (index, piece) in pieces.iter().enumerate() {
            let piece_lines = lines(piece);
            let name = if index == 0 {
                "job.tap".to_string()
            } else {
                format!("job_{index}.tap")
            };
            assert_eq!(piece_lines[0], format!("N10(FILE {name})"), "{piece}");
            assert_eq!(piece_lines[1], "N20G90G21", "{piece}");
        }
        // An unsplit program keeps the name it was given.
        let one = post_process(&post, &job, &[slots()]).expect("posts");
        assert!(one.starts_with("N10(FILE job.tap)"), "{one}");
    }

    #[test]
    fn tape_splitting_cuts_a_long_program_at_retracts() {
        let post = post_with("TAPE_SPLITTING = 12 4 \"%s_%d.tap\" 1 NO", "");
        let whole = post_process(&post, &PostJob::default(), &[slots()]).expect("posts");
        let pieces =
            post_process_pieces(&post, &PostJob::default(), &[slots()], None).expect("posts");
        assert!(pieces.len() > 2, "{pieces:?}");
        let body_of = |text: &str| -> Vec<String> {
            lines(text)
                .into_iter()
                .filter(|l| l != "G90G21" && l != "M5" && l != "M30")
                .collect()
        };
        let mut joined = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let piece_lines = lines(piece);
            assert_eq!(piece_lines.first().map(String::as_str), Some("G90G21"));
            assert_eq!(piece_lines.last().map(String::as_str), Some("M30"));
            // At most 12 lines before the two footer lines.
            assert!(piece_lines.len() <= 14, "{piece}");
            // A later piece first loads the tool, starts the spindle and
            // goes back to where the last one stopped.
            let skip = if index == 0 { 0 } else { 3 };
            joined.extend(body_of(piece).into_iter().skip(skip));
        }
        assert_eq!(joined, body_of(&whole), "the pieces hold the whole program");
        // Every piece but the last ends on a lift straight up.
        for piece in &pieces[..pieces.len() - 1] {
            let body = body_of(piece);
            assert!(body.last().is_some_and(|l| l.contains("Z5.000")), "{piece}");
        }
        let split = post.tape_splitting.expect("split");
        assert_eq!(split.file_name(0, "job", "tap"), None);
        assert_eq!(
            split.file_name(1, "job", "tap").as_deref(),
            Some("job_1.tap")
        );
        // A short program is not split.
        let short = post_with("TAPE_SPLITTING = 1000", "");
        let one =
            post_process_pieces(&short, &PostJob::default(), &[slots()], None).expect("posts");
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn inverse_time_writes_one_over_the_minutes_of_each_wrapped_cut() {
        let post = post_with("ROTARY_WRAP_Y = A\nINVERSE_TIME_MODE = YES", "");
        assert!(post.inverse_time);
        let mut tp = toolpath("wrap", tool("End Mill 6 mm", 1));
        tp.feed = 600.0;
        tp.plunge = 300.0;
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -1.0)),
            Move::feed(Point3::new(60.0, 0.0, -1.0)),
            Move::feed(Point3::new(90.0, 0.0, -1.0)),
        ];
        let wrap = Wrap {
            word: super::super::rotary::RotaryWord::A,
            blank_along: crate::doc::RotaryAxis::X,
            wiring: Default::default(),
            diameter: 50.0,
        };
        let text = post_process_wrapped(&post, &PostJob::default(), &[tp.clone()], Some(wrap))
            .expect("posts");
        // 6 mm plunged at 300 mm/min takes 1/50 min; 60 mm at 600 takes 1/10
        // and 30 mm 1/20: each cut carries its own F.
        assert!(text.contains("F50.0"), "{text}");
        assert!(text.contains("X60.000F10.0"), "{text}");
        assert!(text.contains("X90.000F20.0"), "{text}");
        // Flat jobs keep ordinary feed rates.
        let flat = post_process(&post, &PostJob::default(), &[tp]).expect("posts");
        assert!(flat.contains("F600.0") && !flat.contains("F10.0"), "{flat}");
    }

    fn wrapped_long_cut() -> (Toolpath, Wrap) {
        let mut tp = toolpath("wrap", tool("End Mill 6 mm", 1));
        tp.feed = 100.0;
        tp.plunge = 50.0;
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -1.0)),
            Move::feed(Point3::new(2000.0, 0.0, -1.0)),
            Move::feed(Point3::new(2003.0, 0.0, -1.0)),
        ];
        let wrap = Wrap {
            word: super::super::rotary::RotaryWord::A,
            blank_along: crate::doc::RotaryAxis::X,
            wiring: Default::default(),
            diameter: 50.0,
        };
        (tp, wrap)
    }

    #[test]
    fn a_long_inverse_time_cut_never_writes_a_zero_rate() {
        // One decimal on F, 2 m at 100 mm/min: 1/20 of a minute.
        let post = post_with("ROTARY_WRAP_Y = A\nINVERSE_TIME_MODE = YES", "");
        let (tp, wrap) = wrapped_long_cut();
        let text =
            post_process_wrapped(&post, &PostJob::default(), &[tp], Some(wrap)).expect("posts");
        assert!(
            !text.contains("F0.0\n") && !text.contains("F0.0\r"),
            "{text}"
        );
        assert!(text.contains("X2000.000F0.050"), "{text}");
        // The short cut keeps the declared single decimal.
        assert!(text.contains("X2003.000F33.3"), "{text}");
    }

    #[test]
    fn a_declared_fi_carries_the_inverse_rate_and_f_stays_a_feed() {
        let text = POST
            .replacen(
                "RAPID_RATE = 3000",
                "RAPID_RATE = 3000\nROTARY_WRAP_Y = A\nINVERSE_TIME_MODE = YES\n\
                 VAR INVERSE_TIME = [FI|A|F|1.3]",
                1,
            )
            .replace("G1[X][Y][Z][F]", "G93G1[X][Y][Z][FI]")
            .replace("\"G1[X][Y][Z]\"", "\"G1[X][Y][Z][FI]\"")
            .replace("G1[Z][F]", "G1[Z][FI](feed [F])");
        let post = parse_post("fi", &text).expect("post with FI parses");
        let (tp, wrap) = wrapped_long_cut();
        let out =
            post_process_wrapped(&post, &PostJob::default(), &[tp], Some(wrap)).expect("posts");
        // 6 mm plunged at 50 mm/min, then 2 m and 3 mm at 100 mm/min.
        assert!(out.contains("Z-1.000F8.333(feed F50.0)"), "{out}");
        assert!(out.contains("X2000.000F0.050"), "{out}");
        assert!(out.contains("X2003.000F33.333"), "{out}");
        // A rapid writes no inverse rate.
        assert!(
            !out.lines().any(|l| l.starts_with("G0") && l.contains('F')),
            "{out}"
        );
    }
    #[test]
    fn brackets_and_line_breaks_in_names_and_notes_stay_inside_their_comment() {
        let mut tp = toolpath("Profile (outside)", tool("End Mill (1/4 inch)", 1));
        tp.tool.notes = "spare (drawer)".to_string();
        tp.moves = vec![Move::feed(Point3::new(10.0, 0.0, 0.0))];
        let source = POST.replace(
            "+ \"[TOOLPATH_NOTES]\"",
            "+ \"([TOOLPATH_NOTES])\"\n+ \"([TOOL_NAME] [TOOLPATH_NAME] [TOOL_NOTES])\"",
        );
        let post = parse_post("commented", &source).expect("post parses");
        let job = PostJob {
            notes: "Ann's sign (front)\nclamp the ends".to_string(),
            ..PostJob::default()
        };
        let text = post_process(&post, &job, &[tp]).expect("posts");
        let lines = lines(&text);
        assert_eq!(lines[1], "(Ann's sign {front} clamp the ends)", "{text}");
        assert_eq!(
            lines[2], "(End Mill {1/4 inch} Profile {outside} spare {drawer})",
            "{text}"
        );
    }
}
