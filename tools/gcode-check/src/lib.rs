//! An independent checker for posted CNC programs.
//!
//! The checker reads a program the way a controller would: modal state
//! (units, absolute or incremental, plane, motion mode, feed, spindle, tool,
//! length offset), arcs by centre or radius flattened to a tolerance, and
//! the rapids, feeds and tool changes in the order the machine runs them.
//! It cuts the moves into a heightfield of its own and reports anything a
//! machine would do that the design did not ask for.
//!
//! It shares no code with the CAM, the post processor or the preview it
//! checks, and has no dependencies, so a mistake in any of them shows up as
//! a difference here instead of being repeated.
//!
//! Three program languages are read: RS274 G code (Fanuc, Grbl, Mach,
//! LinuxCNC, UCCNC, Centroid and alike), ShopBot part files and Heidenhain
//! conversational programs.

pub mod cutter;
pub mod field;
mod heidenhain;
mod machine;
mod rs274;
mod shopbot;

use std::collections::BTreeMap;

pub use cutter::Cutter;
pub use field::{compare, Comparison, Field, Grid};

/// Length unit of a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    /// Millimetres.
    Mm,
    /// Inches.
    Inch,
}

impl Units {
    /// Millimetres in one of these units.
    pub fn mm(self) -> f64 {
        match self {
            Units::Mm => 1.0,
            Units::Inch => 25.4,
        }
    }
}

/// Program language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// RS274 G code.
    GCode,
    /// ShopBot part file.
    ShopBot,
    /// Heidenhain conversational.
    Heidenhain,
}

impl Dialect {
    /// Picks the language from the text of a program.
    pub fn detect(text: &str) -> Dialect {
        let first = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        if first.contains("BEGIN PGM") {
            return Dialect::Heidenhain;
        }
        let shopbot = text.lines().map(str::trim).any(|l| {
            let upper = l.to_ascii_uppercase();
            ["J3,", "JZ,", "M3,", "MZ,", "MS,", "JS,", "J2,", "M2,"]
                .iter()
                .any(|p| upper.starts_with(p))
        });
        if shopbot {
            Dialect::ShopBot
        } else {
            Dialect::GCode
        }
    }
}

/// A rotary axis the program may drive, with the blank it turns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotary {
    /// The word the program turns it with, `'A'` or `'B'`.
    pub axis: char,
    /// The flat axis that word stands for, `'Y'` (a blank along X) or `'X'`
    /// (a blank along Y).
    pub across: char,
    /// Diameter the flat design was unwrapped from, millimetres.
    pub diameter: f64,
    /// The word is the distance round the blank in program units, not degrees.
    pub linear: bool,
    /// The word runs the other way from the flat axis.
    pub reversed: bool,
}

/// How the machine learns the length of a tool after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolLength {
    /// Not checked: the lengths are taken to be set up on the machine.
    #[default]
    Unchecked,
    /// The operator touches the new tool off while the program is stopped
    /// at a pause (M0 or M1) after the change.
    TouchOffAtPause,
    /// The M6 macro or remap probes the new tool on a tool setter and
    /// applies the length itself: no pause is needed, and a G43 H after it
    /// would replace the probed length with the tool table's entry.
    ProbedByM6,
    /// The M6 macro probes the new tool into the tool table but leaves the
    /// length to the program: a G43 H with the new tool's number must
    /// follow each change before it cuts.
    TableAfterM6,
}

/// What the checker is told about the machine and the job. Lengths are
/// millimetres in job coordinates.
#[derive(Debug, Clone)]
pub struct Setup {
    /// Program language, detected from the text when `None`.
    pub dialect: Option<Dialect>,
    /// Units the post says it writes.
    pub units: Units,
    /// Height every rapid across the job should stay at or above.
    pub safe_z: f64,
    /// Cutters by tool number.
    pub tools: BTreeMap<u32, Cutter>,
    /// The material to cut, `None` to check the program without cutting.
    pub grid: Option<Grid>,
    /// Largest distance a flattened arc may stray from the true arc.
    pub arc_tolerance: f64,
    /// A laser program: the beam replaces the spindle and nothing is cut
    /// into the heightfield.
    pub laser: bool,
    /// A wrapped rotary program, whose angles are unwrapped back onto the
    /// flat layout before cutting.
    pub rotary: Option<Rotary>,
    /// The tool fitted before the program starts, for a controller with one
    /// tool to a file and no tool word (T selects a nozzle on Snapmaker).
    pub fitted_tool: Option<u32>,
    /// How each new tool's length is measured.
    pub tool_length: ToolLength,
    /// The program must set, before its first move, every modal state a
    /// controller keeps from the program before it (LinuxCNC, UCCNC): feed
    /// mode (G93 or G94), plane, cutter compensation off (G40) and the tool
    /// length offset (G43 or G49; on a machine whose M6 probes, the length
    /// it last probed stands and the program must not cancel it).
    pub preamble: bool,
}

impl Setup {
    /// A setup with no material, tools or rotary axis.
    pub fn new(units: Units, safe_z: f64) -> Setup {
        Setup {
            dialect: None,
            units,
            safe_z,
            tools: BTreeMap::new(),
            grid: None,
            arc_tolerance: 0.002,
            laser: false,
            rotary: None,
            fitted_tool: None,
            tool_length: ToolLength::Unchecked,
            preamble: false,
        }
    }
}

/// How bad a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing, the machine still cuts what was designed.
    Warning,
    /// The machine would cut wrongly, stop, or risk the tool or the work.
    Error,
}

/// What a finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A line the controller would refuse.
    Syntax,
    /// A code the checker does not read, so the program is not checked past
    /// its meaning.
    Unsupported,
    /// A move before the program sets its units.
    NoUnits,
    /// The program's units differ from the ones the post declares.
    WrongUnits,
    /// A move before the program sets absolute or incremental distances.
    NoDistanceMode,
    /// Axis words with no motion mode in force.
    NoMotionMode,
    /// A cutting move with no feed rate set.
    NoFeed,
    /// A cutting move into material with the spindle stopped.
    SpindleOff,
    /// A cutting move with no tool loaded, or a tool the job does not have.
    NoTool,
    /// A tool change while the spindle is still turning.
    SpindleOnAtChange,
    /// A move across X or Y before Z is at a known height, at the start or
    /// after a tool change.
    UnknownHeight,
    /// A rapid whose cutter would pass through material.
    RapidIntoMaterial,
    /// A rapid across the job below the safe Z.
    RapidBelowSafeZ,
    /// An arc whose end is not on its circle.
    ArcRadius,
    /// The program ends with the tool below the safe Z or the spindle on.
    UnsafeEnd,
    /// The program has no end code.
    NoEnd,
    /// A laser feed with the laser stopped (M5), or a laser left on. A feed
    /// at zero power in laser mode is a move with the beam held off.
    Beam,
    /// A tool change in inverse time (G93): any feed the change routine
    /// moves at, its probing feed too, would be read as one over minutes.
    ToolChangeInverseTime,
    /// A cut that turns the rotary axis in units per minute (G94): the
    /// control reads F as degrees per minute along the turn, so the surface
    /// does not move at the feed the program asks for.
    RotaryFeed,
    /// A cut with a tool whose length was not measured since it was fitted,
    /// or a length offset that replaces or cancels the one an M6 probed.
    ToolLength,
    /// A move before the program sets a modal state the controller keeps
    /// from the last program run (feed mode, plane, cutter compensation,
    /// tool length offset).
    Modal,
}

/// One finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Program line, from 1.
    pub line: usize,
    /// How bad it is.
    pub severity: Severity,
    /// What it is about.
    pub kind: Kind,
    /// What was found.
    pub text: String,
}

/// One straight move the machine makes; arcs arrive as many of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// Program line the move comes from.
    pub line: usize,
    /// True for a rapid.
    pub rapid: bool,
    /// True when the move is part of a flattened arc.
    pub arc: bool,
    /// Start, millimetres; flat layout coordinates for a rotary program.
    pub from: [f64; 3],
    /// End.
    pub to: [f64; 3],
    /// Feed rate in millimetres per minute; zero for a rapid.
    pub feed: f64,
    /// Tool in the spindle.
    pub tool: Option<u32>,
    /// Spindle (or beam) running.
    pub spindle: bool,
    /// Spindle speed (or laser power) in force.
    pub speed: f64,
    /// False when the start height was not known (the start of the
    /// program or after a tool change).
    pub known_start: bool,
}

/// What the checker found.
#[derive(Debug, Clone)]
pub struct Report {
    /// Language the program was read as.
    pub dialect: Dialect,
    /// Every finding, in program order.
    pub issues: Vec<Issue>,
    /// Every move.
    pub motions: Vec<Motion>,
    /// Tools loaded, in the order they were loaded.
    pub tool_changes: Vec<u32>,
    /// Units the program set, if it set any.
    pub units: Option<Units>,
    /// The material after the program, when a grid was given.
    pub field: Option<Field>,
    /// Dwells, seconds, in order.
    pub dwells: Vec<f64>,
}

impl Report {
    /// Findings of at least `severity`.
    pub fn at_least(&self, severity: Severity) -> Vec<&Issue> {
        self.issues
            .iter()
            .filter(|i| i.severity >= severity)
            .collect()
    }

    /// Findings of one kind.
    pub fn of_kind(&self, kind: Kind) -> Vec<&Issue> {
        self.issues.iter().filter(|i| i.kind == kind).collect()
    }

    /// Lowest tip height of any feed move, per tool.
    pub fn lowest_by_tool(&self) -> BTreeMap<Option<u32>, f64> {
        let mut out = BTreeMap::new();
        for m in self.motions.iter().filter(|m| !m.rapid) {
            let z = m.from[2].min(m.to[2]);
            let low = out.entry(m.tool).or_insert(z);
            if z < *low {
                *low = z;
            }
        }
        out
    }

    /// Lowest tip height of any move.
    pub fn lowest(&self) -> Option<f64> {
        self.motions
            .iter()
            .map(|m| m.to[2].min(if m.known_start { m.from[2] } else { m.to[2] }))
            .reduce(f64::min)
    }

    /// A short listing of the findings, one per line.
    pub fn summary(&self) -> String {
        self.issues
            .iter()
            .map(|i| format!("line {} {:?} {:?}: {}", i.line, i.severity, i.kind, i.text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Reads and runs a program, cutting it into the setup's material.
pub fn check(text: &str, setup: &Setup) -> Report {
    let dialect = setup.dialect.unwrap_or_else(|| Dialect::detect(text));
    let mut machine = machine::Machine::new(setup);
    match dialect {
        Dialect::GCode => rs274::run(text, &mut machine),
        Dialect::ShopBot => shopbot::run(text, &mut machine),
        Dialect::Heidenhain => heidenhain::run(text, &mut machine),
    }
    machine.finish(dialect)
}
