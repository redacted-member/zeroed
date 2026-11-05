//! Post processing: the text post processor format, the engine that runs it
//! and the posts shipped with the app.
//!
//! A post processor is a plain text file that says how one controller wants
//! its program written: which lines go at the top, what a feed move looks
//! like, how numbers are formatted and what the file is called. The format is
//! described in `parser`, which also holds the grammar this build supports.

pub mod changelog;
pub mod interp;
pub mod library;
pub mod parser;
pub mod rotary;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cam::toolpath::{Bounds3, Point3};
use crate::error::CoreError;

pub use interp::{post_process, PostJob};
pub use library::{PostLibrary, SHIPPED_POSTS};
pub use parser::parse_post;

/// Unit a post writes its numbers in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PostUnits {
    /// Millimetres.
    Mm,
    /// Inches.
    Inch,
}

impl PostUnits {
    /// Millimetres in one of these units.
    pub fn millimetres(self) -> f64 {
        match self {
            PostUnits::Mm => 1.0,
            PostUnits::Inch => 25.4,
        }
    }

    /// Converts a length in millimetres to this unit.
    pub fn from_mm(self, mm: f64) -> f64 {
        mm / self.millimetres()
    }
}

/// One of the sections a post file can define. Each is a list of output lines
/// the engine writes when that moment comes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    /// Top of the file.
    Header,
    /// Written when the program changes tool.
    Toolchange,
    /// Start of a toolpath that kept the tool it already had.
    NewSegment,
    /// Start of a toolpath that follows a tool change.
    NewSegmentPostToolchange,
    /// First rapid of a run of rapids.
    FirstRapidMove,
    /// Rapid travel move.
    RapidMove,
    /// Rapid that only lifts the tool straight up.
    RetractMove,
    /// First plunge of a run of plunges.
    FirstPlungeMove,
    /// Move down into the material.
    PlungeMove,
    /// First cutting move of a run.
    FirstFeedMove,
    /// Cutting move.
    FeedMove,
    /// First clockwise arc of a run.
    FirstCwArcMove,
    /// Clockwise arc.
    CwArcMove,
    /// First counter clockwise arc of a run.
    FirstCcwArcMove,
    /// Counter clockwise arc.
    CcwArcMove,
    /// Pause in place.
    DwellMove,
    /// Laser or other jet turned on.
    JetToolOn,
    /// Laser or other jet turned off.
    JetToolOff,
    /// Laser or other jet power changed.
    JetToolPower,
    /// End of the program.
    Footer,
    /// Lines written after the footer, such as a direct output sign off.
    Trailer,
    /// The first rapid after the header or a tool change.
    InitialRapidMove,
    /// Written when the spindle is first turned on for a tool, after the
    /// tool change.
    SpindleOn,
    /// Written before a cutting move whose feed rate differs from the
    /// last one written.
    FeedRateChange,
    /// First clockwise arc of a run that also moves in Z.
    FirstCwHelicalArcMove,
    /// Clockwise arc that also moves in Z.
    CwHelicalArcMove,
    /// First counter clockwise arc of a run that also moves in Z.
    FirstCcwHelicalArcMove,
    /// Counter clockwise arc that also moves in Z.
    CcwHelicalArcMove,
    /// Written once as a drill bank toolpath starts, before its first move.
    SetupDrillBank,
    /// Written over a firing whose set of lowered drills differs from the
    /// last one, with the whole set in `[DB]`, `[DBX]`, `[DBY]` and `[DL]`.
    DrillBankPattern,
    /// Written for each drill to lower at a firing, the drill in `[CD]`.
    DrillBankExtendDrill,
    /// Written for each lowered drill to raise at a firing, and for every
    /// drill still down as the toolpath ends, the drill in `[CD]`.
    DrillBankRetractDrill,
    /// Written once as a drill bank toolpath ends, after its last move.
    FinalDrillBankAction,
}

impl BlockKind {
    /// The name this block is written with in a post file.
    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Header => "HEADER",
            BlockKind::Toolchange => "TOOLCHANGE",
            BlockKind::NewSegment => "NEW_SEGMENT",
            BlockKind::NewSegmentPostToolchange => "NEW_SEGMENT_POST_TOOLCHANGE",
            BlockKind::FirstRapidMove => "FIRST_RAPID_MOVE",
            BlockKind::RapidMove => "RAPID_MOVE",
            BlockKind::RetractMove => "RETRACT_MOVE",
            BlockKind::FirstPlungeMove => "FIRST_PLUNGE_MOVE",
            BlockKind::PlungeMove => "PLUNGE_MOVE",
            BlockKind::FirstFeedMove => "FIRST_FEED_MOVE",
            BlockKind::FeedMove => "FEED_MOVE",
            BlockKind::FirstCwArcMove => "FIRST_CW_ARC_MOVE",
            BlockKind::CwArcMove => "CW_ARC_MOVE",
            BlockKind::FirstCcwArcMove => "FIRST_CCW_ARC_MOVE",
            BlockKind::CcwArcMove => "CCW_ARC_MOVE",
            BlockKind::DwellMove => "DWELL_MOVE",
            BlockKind::JetToolOn => "JET_TOOL_ON",
            BlockKind::JetToolOff => "JET_TOOL_OFF",
            BlockKind::JetToolPower => "JET_TOOL_POWER",
            BlockKind::Footer => "FOOTER",
            BlockKind::Trailer => "TRAILER",
            BlockKind::InitialRapidMove => "INITIAL_RAPID_MOVE",
            BlockKind::SpindleOn => "SPINDLE_ON",
            BlockKind::FeedRateChange => "FEED_RATE_CHANGE",
            BlockKind::FirstCwHelicalArcMove => "FIRST_CW_HELICAL_ARC_MOVE",
            BlockKind::CwHelicalArcMove => "CW_HELICAL_ARC_MOVE",
            BlockKind::FirstCcwHelicalArcMove => "FIRST_CCW_HELICAL_ARC_MOVE",
            BlockKind::CcwHelicalArcMove => "CCW_HELICAL_ARC_MOVE",
            BlockKind::SetupDrillBank => "SETUP_DRILL_BANK",
            BlockKind::DrillBankPattern => "DRILL_BANK_PATTERN",
            BlockKind::DrillBankExtendDrill => "DRILL_BANK_EXTEND_DRILL",
            BlockKind::DrillBankRetractDrill => "DRILL_BANK_RETRACT_DRILL",
            BlockKind::FinalDrillBankAction => "FINAL_DRILL_BANK_ACTION",
        }
    }

    /// The block a post file names, if it names one this build knows.
    pub fn from_name(name: &str) -> Option<BlockKind> {
        const ALL: [BlockKind; 33] = [
            BlockKind::Header,
            BlockKind::Toolchange,
            BlockKind::NewSegment,
            BlockKind::NewSegmentPostToolchange,
            BlockKind::FirstRapidMove,
            BlockKind::RapidMove,
            BlockKind::RetractMove,
            BlockKind::FirstPlungeMove,
            BlockKind::PlungeMove,
            BlockKind::FirstFeedMove,
            BlockKind::FeedMove,
            BlockKind::FirstCwArcMove,
            BlockKind::CwArcMove,
            BlockKind::FirstCcwArcMove,
            BlockKind::CcwArcMove,
            BlockKind::DwellMove,
            BlockKind::JetToolOn,
            BlockKind::JetToolOff,
            BlockKind::JetToolPower,
            BlockKind::Footer,
            BlockKind::Trailer,
            BlockKind::InitialRapidMove,
            BlockKind::SpindleOn,
            BlockKind::FeedRateChange,
            BlockKind::FirstCwHelicalArcMove,
            BlockKind::CwHelicalArcMove,
            BlockKind::FirstCcwHelicalArcMove,
            BlockKind::CcwHelicalArcMove,
            BlockKind::SetupDrillBank,
            BlockKind::DrillBankPattern,
            BlockKind::DrillBankExtendDrill,
            BlockKind::DrillBankRetractDrill,
            BlockKind::FinalDrillBankAction,
        ];
        ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The block to write instead when this one is not defined.
    pub fn fallback(self) -> Option<BlockKind> {
        match self {
            BlockKind::FirstRapidMove | BlockKind::RetractMove => Some(BlockKind::RapidMove),
            BlockKind::FirstPlungeMove => Some(BlockKind::PlungeMove),
            BlockKind::PlungeMove => Some(BlockKind::FirstFeedMove),
            BlockKind::FirstFeedMove => Some(BlockKind::FeedMove),
            BlockKind::FirstCwArcMove => Some(BlockKind::CwArcMove),
            BlockKind::FirstCcwArcMove => Some(BlockKind::CcwArcMove),
            BlockKind::NewSegmentPostToolchange => Some(BlockKind::NewSegment),
            BlockKind::InitialRapidMove => Some(BlockKind::FirstRapidMove),
            BlockKind::FirstCwHelicalArcMove => Some(BlockKind::CwHelicalArcMove),
            BlockKind::FirstCcwHelicalArcMove => Some(BlockKind::CcwHelicalArcMove),
            _ => None,
        }
    }
}

/// Where a variable takes its number from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueSource {
    /// X of the move end point.
    X,
    /// Y of the move end point.
    Y,
    /// Z of the move end point.
    Z,
    /// Angle about X in degrees, the wrapped flat Y of a rotary job.
    AngleA,
    /// Angle about Y in degrees, the wrapped flat X of a rotary job.
    AngleB,
    /// Feed rate of the current move.
    Feed,
    /// Plunge rate of the current toolpath.
    PlungeFeed,
    /// Rapid rate declared by the post.
    RapidRate,
    /// Spindle speed.
    Spindle,
    /// Tool number.
    ToolNumber,
    /// Line number, which counts up as lines are written.
    LineNumber,
    /// Arc centre offset along X.
    ArcCentreX,
    /// Arc centre offset along Y.
    ArcCentreY,
    /// Arc centre offset along Z, which is always zero for a flat arc.
    ArcCentreZ,
    /// Arc radius.
    ArcRadius,
    /// Dwell time in seconds.
    Dwell,
    /// Laser power.
    Power,
    /// Safe Z height.
    SafeZ,
    /// The height a tool change lifts to before any move across: the
    /// higher of the toolpath's safe Z and the home Z (`[ZCLEAR]`).
    ClearanceZ,
    /// Home position X.
    HomeX,
    /// Home position Y.
    HomeY,
    /// Home position Z.
    HomeZ,
    /// Smallest X of the output.
    MinX,
    /// Smallest Y of the output.
    MinY,
    /// Smallest Z of the output.
    MinZ,
    /// Largest X of the output.
    MaxX,
    /// Largest Y of the output.
    MaxY,
    /// Largest Z of the output.
    MaxZ,
    /// Width of the output along X.
    LengthX,
    /// Depth of the output along Y.
    LengthY,
    /// Height of the output along Z.
    LengthZ,
    /// Diameter of the current tool.
    ToolDiameter,
    /// Stepover of the current tool.
    ToolStepover,
    /// Pass depth of the current tool.
    ToolPassDepth,
    /// Cutting feed rate of the current toolpath, whatever the move.
    CutFeed,
    /// Tool number before the last tool change.
    PreviousToolNumber,
    /// Arc centre X in job coordinates.
    ArcCentreAbsX,
    /// Arc centre Y in job coordinates.
    ArcCentreAbsY,
    /// X where the arc starts.
    ArcStartX,
    /// Y where the arc starts.
    ArcStartY,
    /// X half way round the arc.
    ArcMidX,
    /// Y half way round the arc.
    ArcMidY,
    /// X half way round the arc, from where it starts.
    ArcMidXInc,
    /// Y half way round the arc, from where it starts.
    ArcMidYInc,
    /// Angle the arc sweeps, degrees, positive counter clockwise.
    ArcAngle,
    /// Diameter a rotary job is wrapped round.
    WrapDiameter,
    /// Inverse time rate of the current cutting move: one over the minutes
    /// the move takes (`[FI]`, for G93 feeds).
    InverseTime,
    /// The drills a drill bank has down, as a bit pattern: drill number n
    /// sets the bit worth 2 to the power n - 1 (`[DB]`).
    DrillPattern,
    /// As `DrillPattern`, only the drills on the bank's horizontal arm
    /// (`[DBX]`).
    DrillPatternX,
    /// As `DrillPattern`, only the drills on the bank's vertical arm
    /// (`[DBY]`).
    DrillPatternY,
    /// The one drill being lowered or raised (`[CD]`).
    CurrentDrill,
}

impl ValueSource {
    /// How the value is converted when the post writes inches.
    pub fn scaling(self) -> Scaling {
        match self {
            ValueSource::X
            | ValueSource::Y
            | ValueSource::Z
            | ValueSource::ArcCentreX
            | ValueSource::ArcCentreY
            | ValueSource::ArcCentreZ
            | ValueSource::ArcRadius
            | ValueSource::SafeZ
            | ValueSource::ClearanceZ
            | ValueSource::HomeX
            | ValueSource::HomeY
            | ValueSource::HomeZ
            | ValueSource::MinX
            | ValueSource::MinY
            | ValueSource::MinZ
            | ValueSource::MaxX
            | ValueSource::MaxY
            | ValueSource::MaxZ
            | ValueSource::LengthX
            | ValueSource::LengthY
            | ValueSource::LengthZ
            | ValueSource::ToolDiameter
            | ValueSource::ToolStepover
            | ValueSource::ToolPassDepth
            | ValueSource::ArcCentreAbsX
            | ValueSource::ArcCentreAbsY
            | ValueSource::ArcStartX
            | ValueSource::ArcStartY
            | ValueSource::ArcMidX
            | ValueSource::ArcMidY
            | ValueSource::ArcMidXInc
            | ValueSource::ArcMidYInc
            | ValueSource::WrapDiameter => Scaling::Length,
            ValueSource::Feed | ValueSource::PlungeFeed | ValueSource::CutFeed => Scaling::Rate,
            ValueSource::RapidRate
            | ValueSource::Spindle
            | ValueSource::ToolNumber
            | ValueSource::LineNumber
            | ValueSource::Dwell
            | ValueSource::Power
            | ValueSource::AngleA
            | ValueSource::AngleB
            | ValueSource::PreviousToolNumber
            | ValueSource::ArcAngle
            | ValueSource::InverseTime
            | ValueSource::DrillPattern
            | ValueSource::DrillPatternX
            | ValueSource::DrillPatternY
            | ValueSource::CurrentDrill => Scaling::None,
        }
    }

    /// True for the drill bank values, which are written every time they
    /// appear: a controller reads each pattern or drill as a command, not
    /// as a modal word.
    pub fn is_drill_bank(self) -> bool {
        matches!(
            self,
            ValueSource::DrillPattern
                | ValueSource::DrillPatternX
                | ValueSource::DrillPatternY
                | ValueSource::CurrentDrill
        )
    }

    /// True when the value tracks the tool position, which is what an
    /// incremental variable takes the difference of.
    pub fn is_position(self) -> bool {
        matches!(
            self,
            ValueSource::X
                | ValueSource::Y
                | ValueSource::Z
                | ValueSource::AngleA
                | ValueSource::AngleB
        )
    }
}

/// Whether a value is converted when the post writes inches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scaling {
    /// A length in millimetres.
    Length,
    /// A rate in millimetres per minute.
    Rate,
    /// A plain number that is the same in any unit.
    None,
}

/// A variable declared by the post with `VAR NAME = [TOKEN|FLAGS|OUTPUT|F.D]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VarDef {
    /// Name the post gave the declaration, kept for error messages.
    pub name: String,
    /// Token that stands for this variable in output lines.
    pub token: String,
    /// Where the number comes from.
    pub source: ValueSource,
    /// False when the variable writes the change since the last output.
    pub absolute: bool,
    /// True when the variable is written even if it has not changed.
    pub always: bool,
    /// True when the whole number part is padded with zeros.
    pub zero_pad: bool,
    /// True when positive values are written with a plus sign.
    pub force_sign: bool,
    /// Text written in front of the number, usually the word address.
    pub output: String,
    /// Smallest number of digits in front of the decimal point.
    pub int_digits: usize,
    /// Digits after the decimal point. Zero writes a whole number.
    pub decimals: usize,
    /// Factor the value is multiplied by before it is written, for a control
    /// that wants a rate per second where the kernel holds one per minute.
    pub scale: f64,
}

impl VarDef {
    /// Formats a value the way this variable asks for, without the output
    /// prefix.
    pub fn format_number(&self, value: f64) -> String {
        let value = if value == 0.0 { 0.0 } else { value };
        let text = format!("{:.*}", self.decimals, value);
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.to_string()),
            None => (false, text),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole.to_string(), Some(fraction.to_string())),
            None => (digits, None),
        };
        let whole = if self.zero_pad && whole.len() < self.int_digits {
            format!("{}{}", "0".repeat(self.int_digits - whole.len()), whole)
        } else {
            whole
        };
        let mut out = String::new();
        let all_zero = whole.chars().all(|c| c == '0')
            && fraction
                .as_ref()
                .is_none_or(|f| f.chars().all(|c| c == '0'));
        if negative && !all_zero {
            out.push('-');
        } else if self.force_sign && !all_zero {
            out.push('+');
        }
        out.push_str(&whole);
        if let Some(fraction) = fraction {
            out.push('.');
            out.push_str(&fraction);
        }
        out
    }

    /// Formats a value with the output prefix in front of it.
    pub fn format(&self, value: f64) -> String {
        format!("{}{}", self.output, self.format_number(value))
    }

    /// Formats an inverse time rate with the output prefix. The declared
    /// decimals are widened when they would show fewer than two significant
    /// digits, so a long move never writes `F0.0` (a G93 control refuses a
    /// zero rate) and a short one is not timed wildly off by rounding.
    pub fn format_inverse(&self, value: f64) -> String {
        let mut def = self.clone();
        if value.is_finite() && value > 0.0 {
            let magnitude = value.log10().floor();
            let wanted = (1.0 - magnitude).clamp(0.0, 12.0) as usize;
            def.decimals = def.decimals.max(wanted);
        }
        def.format(value)
    }
}

/// How `[DL]` lists the drills a drill bank has down, declared with
/// `VAR DRILL_LIST = [DL|prefix|separator|first separator]`: each drill
/// number follows the prefix, the first two are parted by the first
/// separator and the rest by the separator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrillListFormat {
    /// Written in front of each drill number.
    pub prefix: String,
    /// Written between the later drills.
    pub separator: String,
    /// Written between the first drill and the second.
    pub first_separator: String,
}

impl Default for DrillListFormat {
    fn default() -> Self {
        DrillListFormat {
            prefix: String::new(),
            separator: ",".to_string(),
            first_separator: ",".to_string(),
        }
    }
}

impl DrillListFormat {
    /// The list for `drills`, in the order given.
    pub fn format(&self, drills: &[u32]) -> String {
        let mut out = String::new();
        for (index, drill) in drills.iter().enumerate() {
            match index {
                0 => {}
                1 => out.push_str(&self.first_separator),
                _ => out.push_str(&self.separator),
            }
            out.push_str(&self.prefix);
            out.push_str(&drill.to_string());
        }
        out
    }
}

/// How a post drives a drill bank, from the drill bank blocks it defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillBankMode {
    /// One `DRILL_BANK_PATTERN` block selects the whole set of drills.
    Pattern,
    /// `DRILL_BANK_EXTEND_DRILL` and `DRILL_BANK_RETRACT_DRILL` lower and
    /// raise one drill at a time.
    PerDrill,
}

/// A parsed post processor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    /// Identity, taken from the file name without its extension.
    pub id: String,
    /// Name shown in the save dialog.
    pub name: String,
    /// Extension of the file the post writes, without the dot.
    pub file_extension: String,
    /// Unit the post writes numbers in.
    pub units: PostUnits,
    /// Rapid travel rate in the post's own units per minute, used by the
    /// machining time estimate.
    pub rapid_rate: f64,
    /// Lowest and highest spindle speed the machine accepts.
    pub spindle_speed_range: Option<(f64, f64)>,
    /// Whether the header writes a list of the tools the file uses.
    pub print_toollist: bool,
    /// Characters that end a line.
    pub line_ending: String,
    /// Name of the control software a direct send would talk to. Recorded
    /// only, this build always writes a file.
    pub direct_output: Option<String>,
    /// Characters replaced on the way out, in the order they are applied.
    pub substitutions: Vec<(char, String)>,
    /// First line number.
    pub line_number_start: f64,
    /// Step between line numbers.
    pub line_number_increment: f64,
    /// Variables by the token that stands for them.
    pub variables: BTreeMap<String, VarDef>,
    /// Output lines by block.
    pub blocks: BTreeMap<BlockKind, Vec<String>>,
    /// Where the post was read from, empty for a shipped post.
    pub source_path: String,
    /// The word flat Y wraps onto, for a blank along X (`ROTARY_WRAP_Y`).
    #[serde(default)]
    pub rotary_wrap_y: Option<rotary::RotaryWord>,
    /// The word flat X wraps onto, for a blank along Y (`ROTARY_WRAP_X`).
    #[serde(default)]
    pub rotary_wrap_x: Option<rotary::RotaryWord>,
    /// The longest turn of a rotary axis in one move, degrees; longer turns
    /// are broken into pieces (`MAXIMUM_ROTATION`).
    #[serde(default)]
    pub maximum_rotation: Option<f64>,
    /// Arcs smaller than this radius are written as straight moves, in the
    /// post's units (`MIN_ARC_RADIUS`).
    #[serde(default)]
    pub min_arc_radius: Option<f64>,
    /// Arcs larger than this radius are written as straight moves, in the
    /// post's units (`MAX_ARC_RADIUS`).
    #[serde(default)]
    pub max_arc_radius: Option<f64>,
    /// Line numbers start again from the first after this one
    /// (`LINE_NUMBER_MAXIMUM`).
    #[serde(default)]
    pub line_number_maximum: Option<f64>,
    /// Whether the post declares laser support (`LASER_SUPPORT`).
    #[serde(default)]
    pub laser_support: bool,
    /// Settings the post declares that this build reads but does not act
    /// on, by name, so the post list can say so.
    #[serde(default)]
    pub ignored_settings: Vec<String>,
    /// Feed rates of a wrapped rotary job are written in inverse time: the
    /// F word is one over the minutes the move takes (`INVERSE_TIME_MODE`).
    #[serde(default)]
    pub inverse_time: bool,
    /// Long programs are split into several files (`TAPE_SPLITTING`).
    #[serde(default)]
    pub tape_splitting: Option<TapeSplitting>,
    /// How `[DL]` lists a drill bank's drills, from its `VAR` declaration.
    #[serde(default)]
    pub drill_list: Option<DrillListFormat>,
}

/// How `TAPE_SPLITTING` cuts a long program into files:
/// `TAPE_SPLITTING = MAX_NUM_LINES LINE_TOL "FILENAME_FORMAT" START_INDEX INDEX_ON_FIRST_FILE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapeSplitting {
    /// Most lines in one file, the footer not counted.
    pub max_lines: usize,
    /// A file may end this many lines early at a retract move.
    pub line_tolerance: usize,
    /// File name of each piece: `%s` is the file name without its
    /// extension, `%d` the piece's number.
    pub name_format: String,
    /// Number of the first numbered file.
    pub start_index: usize,
    /// Whether the first file is numbered too; otherwise it keeps the plain
    /// name and numbering starts with the second.
    pub index_on_first: bool,
}

impl TapeSplitting {
    /// The file name of piece `piece` (from 0) of a program saved as
    /// `stem` with `extension`, or None for the plain name.
    ///
    /// A name format without `%d` would give every piece the same name, each
    /// file overwriting the last, so the number is then added as `_N` before
    /// the extension. The number is counted wide enough that a start index
    /// near the top of `usize` cannot overflow.
    pub fn file_name(&self, piece: usize, stem: &str, extension: &str) -> Option<String> {
        let index = if self.index_on_first {
            self.start_index as u128 + piece as u128
        } else if piece == 0 {
            return None;
        } else {
            self.start_index as u128 + piece as u128 - 1
        };
        let name = self
            .name_format
            .replace("%s", stem)
            .replace("%d", &index.to_string())
            .replace("[TP_EXT]", extension);
        if self.name_format.contains("%d") {
            return Some(name);
        }
        // The extension is what follows the last dot of the last path part.
        let base_start = name.rfind(['/', '\\']).map_or(0, |i| i + 1);
        let dot = name[base_start..]
            .rfind('.')
            .filter(|&i| i > 0)
            .map(|i| base_start + i);
        Some(match dot {
            Some(dot) => format!("{}_{index}{}", &name[..dot], &name[dot..]),
            None => format!("{name}_{index}"),
        })
    }
}

impl Default for Post {
    fn default() -> Self {
        Post {
            id: String::new(),
            name: String::new(),
            file_extension: "txt".to_string(),
            units: PostUnits::Mm,
            rapid_rate: 0.0,
            spindle_speed_range: None,
            print_toollist: false,
            line_ending: "\n".to_string(),
            direct_output: None,
            substitutions: Vec::new(),
            line_number_start: 1.0,
            line_number_increment: 1.0,
            variables: BTreeMap::new(),
            blocks: BTreeMap::new(),
            source_path: String::new(),
            rotary_wrap_y: None,
            rotary_wrap_x: None,
            maximum_rotation: None,
            min_arc_radius: None,
            max_arc_radius: None,
            line_number_maximum: None,
            laser_support: false,
            ignored_settings: Vec::new(),
            inverse_time: false,
            tape_splitting: None,
            drill_list: None,
        }
    }
}

impl Post {
    /// The output lines of a block, following the fallback chain when the
    /// post does not define it.
    pub fn block(&self, kind: BlockKind) -> Option<&Vec<String>> {
        let mut current = Some(kind);
        while let Some(kind) = current {
            if let Some(lines) = self.blocks.get(&kind) {
                return Some(lines);
            }
            current = kind.fallback();
        }
        None
    }

    /// True when the post can write arcs, which decides whether an arc move
    /// is written as an arc or broken into short straight moves.
    pub fn supports_arcs(&self) -> bool {
        self.blocks.contains_key(&BlockKind::CwArcMove)
            && self.blocks.contains_key(&BlockKind::CcwArcMove)
    }

    /// True when the post can change tools part way through a file.
    pub fn supports_tool_change(&self) -> bool {
        self.blocks.contains_key(&BlockKind::Toolchange)
    }

    /// How the post drives a drill bank, or None when it cannot. It needs
    /// `SETUP_DRILL_BANK` and `FINAL_DRILL_BANK_ACTION`, then either
    /// `DRILL_BANK_PATTERN` alone or both `DRILL_BANK_EXTEND_DRILL` and
    /// `DRILL_BANK_RETRACT_DRILL`; any other mix drives none.
    pub fn drill_bank_mode(&self) -> Option<DrillBankMode> {
        let has = |kind: BlockKind| self.blocks.contains_key(&kind);
        if !has(BlockKind::SetupDrillBank) || !has(BlockKind::FinalDrillBankAction) {
            return None;
        }
        match (
            has(BlockKind::DrillBankPattern),
            has(BlockKind::DrillBankExtendDrill),
            has(BlockKind::DrillBankRetractDrill),
        ) {
            (true, false, false) => Some(DrillBankMode::Pattern),
            (false, true, true) => Some(DrillBankMode::PerDrill),
            _ => None,
        }
    }

    /// True when the post can drive a drill bank (see
    /// [`Post::drill_bank_mode`]).
    pub fn supports_drill_bank(&self) -> bool {
        self.drill_bank_mode().is_some()
    }

    /// The word a blank along `blank_along` is wrapped onto, when the post
    /// can wrap one that way.
    pub fn wrap_word(&self, blank_along: crate::doc::RotaryAxis) -> Option<rotary::RotaryWord> {
        match blank_along {
            crate::doc::RotaryAxis::X => self.rotary_wrap_y,
            crate::doc::RotaryAxis::Y => self.rotary_wrap_x,
        }
    }

    /// True when the post can drive a laser or other jet tool.
    pub fn supports_jet(&self) -> bool {
        self.laser_support || self.blocks.contains_key(&BlockKind::JetToolOn)
    }

    /// True when an arc of `radius` millimetres is written as an arc, not
    /// broken into straight moves, by the post's arc radius limits.
    pub fn arc_radius_ok(&self, radius: f64) -> bool {
        let radius = self.units.from_mm(radius);
        self.min_arc_radius.is_none_or(|min| radius >= min)
            && self.max_arc_radius.is_none_or(|max| radius <= max)
    }

    /// True when both plain arc blocks write the Z position, so an arc
    /// that moves in Z keeps its depth without the helical blocks.
    pub fn arcs_write_z(&self) -> bool {
        [BlockKind::CwArcMove, BlockKind::CcwArcMove]
            .iter()
            .all(|kind| {
                self.blocks.get(kind).is_some_and(|lines| {
                    lines.iter().any(|line| {
                        line.split('[').skip(1).any(|rest| {
                            let token = rest.split(']').next().unwrap_or_default();
                            self.variables
                                .get(&token.to_uppercase())
                                .is_some_and(|def| def.source == ValueSource::Z)
                        })
                    })
                })
            })
    }

    /// True when the post can write arcs that move in Z as well.
    pub fn supports_helical_arcs(&self) -> bool {
        self.blocks.contains_key(&BlockKind::CwHelicalArcMove)
            && self.blocks.contains_key(&BlockKind::CcwHelicalArcMove)
    }

    /// Clamps a spindle speed to the range the post declares.
    pub fn clamp_spindle(&self, speed: f64) -> f64 {
        match self.spindle_speed_range {
            Some((low, high)) if high >= low => speed.clamp(low, high),
            _ => speed,
        }
    }

    /// A one line summary for the post list in the save dialog.
    pub fn summary(&self) -> PostSummary {
        PostSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            file_extension: self.file_extension.clone(),
            units: self.units,
            arcs: self.supports_arcs(),
            jet: self.supports_jet(),
            rotary: self.rotary_wrap_y.is_some() || self.rotary_wrap_x.is_some(),
            rotary_wrap_y: self.rotary_wrap_y.map(|w| w.letter().to_string()),
            rotary_wrap_x: self.rotary_wrap_x.map(|w| w.letter().to_string()),
            tool_change: self.supports_tool_change(),
            drill_bank: self.supports_drill_bank(),
            source_path: self.source_path.clone(),
            direct_output: self.direct_output.clone(),
            length_choice: self.has_length_lines(),
        }
    }

    /// Whether any block has a line written only when the M6 macro leaves
    /// the tool length to the program, so that the choice means something.
    pub fn has_length_lines(&self) -> bool {
        self.blocks.values().flatten().any(|line| {
            parser::tokens_in(line)
                .iter()
                .any(|t| t.eq_ignore_ascii_case(parser::LENGTH_LINE_TOKEN))
        })
    }

    /// Checks the post has enough in it to write a program.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when a required part is missing.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::validation("a post needs a POST_NAME"));
        }
        if self.file_extension.trim().is_empty() {
            return Err(CoreError::validation("a post needs a FILE_EXTENSION"));
        }
        if !self.blocks.contains_key(&BlockKind::FeedMove) {
            return Err(CoreError::validation(
                "a post needs a FEED_MOVE block to cut anything",
            ));
        }
        if !self.blocks.contains_key(&BlockKind::RapidMove) {
            return Err(CoreError::validation(
                "a post needs a RAPID_MOVE block to travel",
            ));
        }
        Ok(())
    }
}

/// What the post list shows about one post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSummary {
    /// Identity used by the save command.
    pub id: String,
    /// Name shown in the list.
    pub name: String,
    /// Extension of the file it writes.
    pub file_extension: String,
    /// Unit it writes numbers in.
    pub units: PostUnits,
    /// Whether it writes arcs.
    pub arcs: bool,
    /// Whether it can drive a laser.
    pub jet: bool,
    /// Whether it can wrap a rotary job.
    #[serde(default)]
    pub rotary: bool,
    /// The letter a blank along X is written on, when it can wrap one.
    #[serde(default)]
    pub rotary_wrap_y: Option<String>,
    /// The letter a blank along Y is written on, when it can wrap one.
    #[serde(default)]
    pub rotary_wrap_x: Option<String>,
    /// Whether it can change tools part way through a file.
    #[serde(default)]
    pub tool_change: bool,
    /// Whether it can drive a drill bank.
    #[serde(default)]
    pub drill_bank: bool,
    /// Where it was read from, empty for a shipped post.
    pub source_path: String,
    /// The driver program Output direct to machine sends files to, from
    /// `DIRECT_OUTPUT`; none for a post that only saves files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_output: Option<String>,
    /// Whether "M6 macro applies tool length" changes what it writes: the
    /// post has lines for a program that applies the length itself.
    #[serde(default)]
    pub length_choice: bool,
}

/// The extents a post reports through the minimum and maximum tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputBounds {
    /// Lowest corner in millimetres.
    pub min: Point3,
    /// Highest corner in millimetres.
    pub max: Point3,
}

impl OutputBounds {
    /// Bounds around nothing at all, which is what an empty file reports.
    pub fn empty() -> OutputBounds {
        OutputBounds {
            min: Point3::ORIGIN,
            max: Point3::ORIGIN,
        }
    }

    /// Bounds taken from a toolpath box.
    pub fn from_bounds(bounds: Bounds3) -> OutputBounds {
        OutputBounds {
            min: bounds.min,
            max: bounds.max,
        }
    }

    /// The two boxes together.
    pub fn union(self, other: OutputBounds) -> OutputBounds {
        OutputBounds {
            min: Point3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Point3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(int_digits: usize, decimals: usize) -> VarDef {
        VarDef {
            name: "X_POSITION".to_string(),
            token: "X".to_string(),
            source: ValueSource::X,
            absolute: true,
            always: false,
            zero_pad: false,
            force_sign: false,
            output: "X".to_string(),
            int_digits,
            decimals,
            scale: 1.0,
        }
    }

    #[test]
    fn inverse_rates_keep_two_significant_digits() {
        let mut feed = var(1, 1);
        feed.output = "F".to_string();
        assert_eq!(feed.format_inverse(50.0), "F50.0");
        assert_eq!(feed.format_inverse(5.26), "F5.3");
        assert_eq!(feed.format_inverse(0.14), "F0.14");
        // 2 m at 100 mm/min: one over twenty minutes.
        assert_eq!(feed.format_inverse(0.05), "F0.050");
        assert_eq!(feed.format_inverse(0.0004), "F0.00040");
        // A post that already asks for more keeps its own decimals.
        let fine = var(1, 4);
        assert_eq!(fine.format_inverse(0.05), "X0.0500");
        let whole = var(1, 0);
        assert_eq!(whole.format_inverse(120.0), "X120");
        assert_eq!(whole.format_inverse(3.0), "X3.0");
    }

    #[test]
    fn numbers_take_the_declared_decimals() {
        let def = var(1, 3);
        assert_eq!(def.format(12.3456), "X12.346");
        assert_eq!(def.format(-2.0), "X-2.000");
        assert_eq!(def.format_number(0.0), "0.000");
        let whole = var(1, 0);
        assert_eq!(whole.format(1200.4), "X1200");
    }

    #[test]
    fn minus_zero_is_written_as_zero() {
        let def = var(1, 3);
        assert_eq!(def.format_number(-0.0001), "0.000");
        assert_eq!(def.format_number(-0.0), "0.000");
    }

    #[test]
    fn zero_padding_fills_the_whole_number_part() {
        let mut def = var(4, 2);
        def.zero_pad = true;
        assert_eq!(def.format_number(7.5), "0007.50");
        assert_eq!(def.format_number(-7.5), "-0007.50");
        def.zero_pad = false;
        assert_eq!(def.format_number(7.5), "7.50");
    }

    #[test]
    fn a_forced_sign_marks_positive_values() {
        let mut def = var(1, 1);
        def.force_sign = true;
        assert_eq!(def.format_number(2.0), "+2.0");
        assert_eq!(def.format_number(-2.0), "-2.0");
        assert_eq!(def.format_number(0.0), "0.0");
    }

    #[test]
    fn block_names_round_trip() {
        for name in [
            "HEADER",
            "FOOTER",
            "TRAILER",
            "RAPID_MOVE",
            "FIRST_FEED_MOVE",
            "CCW_ARC_MOVE",
            "NEW_SEGMENT_POST_TOOLCHANGE",
            "JET_TOOL_POWER",
        ] {
            let kind = BlockKind::from_name(name).expect("known block");
            assert_eq!(kind.name(), name);
        }
        assert!(BlockKind::from_name("MADE_UP").is_none());
    }

    #[test]
    fn blocks_follow_their_fallback_chain() {
        let mut post = Post::default();
        post.blocks
            .insert(BlockKind::FeedMove, vec!["G1[X][Y]".to_string()]);
        // A plunge with no block of its own is written as a feed move.
        assert_eq!(
            post.block(BlockKind::FirstPlungeMove),
            Some(&vec!["G1[X][Y]".to_string()])
        );
        assert_eq!(post.block(BlockKind::Header), None);
        post.blocks
            .insert(BlockKind::PlungeMove, vec!["G1[Z]".to_string()]);
        assert_eq!(
            post.block(BlockKind::FirstPlungeMove),
            Some(&vec!["G1[Z]".to_string()])
        );
    }

    #[test]
    fn arc_and_jet_support_follow_the_blocks() {
        let mut post = Post::default();
        assert!(!post.supports_arcs());
        post.blocks.insert(BlockKind::CwArcMove, vec![]);
        assert!(!post.supports_arcs());
        post.blocks.insert(BlockKind::CcwArcMove, vec![]);
        assert!(post.supports_arcs());
        assert!(!post.supports_jet());
        post.blocks.insert(BlockKind::JetToolOn, vec![]);
        assert!(post.supports_jet());
    }

    #[test]
    fn spindle_speeds_are_clamped_to_the_declared_range() {
        let mut post = Post::default();
        assert_eq!(post.clamp_spindle(30000.0), 30000.0);
        post.spindle_speed_range = Some((6000.0, 24000.0));
        assert_eq!(post.clamp_spindle(30000.0), 24000.0);
        assert_eq!(post.clamp_spindle(1000.0), 6000.0);
        assert_eq!(post.clamp_spindle(12000.0), 12000.0);
    }

    #[test]
    fn validation_wants_the_parts_that_make_a_program() {
        let mut post = Post::default();
        assert!(post.validate().is_err());
        post.name = "Test".to_string();
        assert!(post.validate().is_err());
        post.blocks.insert(BlockKind::FeedMove, vec![]);
        assert!(post.validate().is_err());
        post.blocks.insert(BlockKind::RapidMove, vec![]);
        assert!(post.validate().is_ok());
        post.file_extension = " ".to_string();
        assert!(post.validate().is_err());
    }

    #[test]
    fn inches_convert_lengths_and_rates() {
        assert_eq!(PostUnits::Mm.from_mm(25.4), 25.4);
        assert_eq!(PostUnits::Inch.from_mm(25.4), 1.0);
        assert_eq!(ValueSource::X.scaling(), Scaling::Length);
        assert_eq!(ValueSource::Feed.scaling(), Scaling::Rate);
        assert_eq!(ValueSource::Spindle.scaling(), Scaling::None);
        assert!(ValueSource::Z.is_position());
        assert!(!ValueSource::ArcCentreX.is_position());
    }
}

#[cfg(test)]
mod tape_splitting_tests {
    use super::TapeSplitting;

    fn split(name_format: &str, start_index: usize, index_on_first: bool) -> TapeSplitting {
        TapeSplitting {
            max_lines: 10,
            line_tolerance: 0,
            name_format: name_format.to_string(),
            start_index,
            index_on_first,
        }
    }

    #[test]
    fn a_format_without_a_number_still_names_every_piece_apart() {
        let plain = split("%s.[TP_EXT]", 1, true);
        let names: Vec<_> = (0..3)
            .map(|piece| plain.file_name(piece, "job", "tap"))
            .collect();
        assert_eq!(
            names,
            vec![
                Some("job_1.tap".to_string()),
                Some("job_2.tap".to_string()),
                Some("job_3.tap".to_string())
            ]
        );
        // The first piece keeps the plain name when it is not numbered.
        let later = split("%s.[TP_EXT]", 1, false);
        assert_eq!(later.file_name(0, "job", "tap"), None);
        assert_eq!(
            later.file_name(1, "job", "tap").as_deref(),
            Some("job_1.tap")
        );
        // No extension, or a dot only in a folder name: the number goes last.
        assert_eq!(
            split("part", 5, true).file_name(1, "job", "nc").as_deref(),
            Some("part_6")
        );
        assert_eq!(
            split("out.d/%s", 0, true)
                .file_name(0, "job", "nc")
                .as_deref(),
            Some("out.d/job_0")
        );
    }

    #[test]
    fn a_numbered_format_is_used_as_written() {
        let numbered = split("%s_%d.[TP_EXT]", 1, true);
        assert_eq!(
            numbered.file_name(0, "job", "tap").as_deref(),
            Some("job_1.tap")
        );
        assert_eq!(
            numbered.file_name(4, "job", "tap").as_deref(),
            Some("job_5.tap")
        );
    }

    #[test]
    fn a_huge_start_index_does_not_overflow() {
        let huge = split("%s_%d.[TP_EXT]", usize::MAX, true);
        let past = (usize::MAX as u128 + 2).to_string();
        assert_eq!(
            huge.file_name(2, "job", "tap"),
            Some(format!("job_{past}.tap"))
        );
        let later = split("%s_%d.[TP_EXT]", usize::MAX, false);
        assert_eq!(
            later.file_name(1, "job", "tap"),
            Some(format!("job_{}.tap", usize::MAX))
        );
    }
}
