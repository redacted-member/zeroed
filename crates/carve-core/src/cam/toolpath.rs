//! The calculated toolpath: an ordered list of machine moves plus everything
//! the post processor and the simulation need to know about them.
//!
//! Coordinates are millimetres in job space, with Z up and zero wherever the
//! job puts Z zero.
//! Feed rates are millimetres per minute.

use serde::{Deserialize, Serialize};

use crate::doc::Side;
use crate::error::CoreError;
use crate::geom::Point;

use super::tool::Tool;

/// A position in job space, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3 {
    /// Distance from the job origin along X.
    pub x: f64,
    /// Distance from the job origin along Y.
    pub y: f64,
    /// Job Z: zero is the job's Z zero, the material surface when Z zero is
    /// on top or the machine bed when it is on the bottom.
    pub z: f64,
}

impl Point3 {
    /// The job origin at Z zero.
    pub const ORIGIN: Point3 = Point3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Builds a position from millimetre coordinates.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z }
    }

    /// The XY part of the position.
    pub fn xy(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Straight line distance to another position, in millimetres.
    pub fn distance(self, other: Point3) -> f64 {
        let (dx, dy, dz) = (other.x - self.x, other.y - self.y, other.z - self.z);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// True when all three coordinates are finite numbers.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Default for Point3 {
    fn default() -> Self {
        Point3::ORIGIN
    }
}

/// What a move does between the previous position and its end point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MoveKind {
    /// Travel move at the machine rapid rate, clear of the material.
    Rapid,
    /// Cutting move at the toolpath feed rate.
    Feed,
    /// Move down into the material at the plunge rate.
    Plunge,
    /// Clockwise circular arc about `centre`, seen looking down at the job.
    ArcCw {
        /// Arc centre in XY, in millimetres.
        centre: Point,
    },
    /// Counter clockwise circular arc about `centre`.
    ArcCcw {
        /// Arc centre in XY, in millimetres.
        centre: Point,
    },
    /// Pause in place.
    Dwell {
        /// How long the machine waits, in seconds.
        seconds: f64,
    },
}

impl MoveKind {
    /// True when the move cuts material, that is everything but a rapid and a
    /// dwell.
    pub fn is_cutting(&self) -> bool {
        matches!(
            self,
            MoveKind::Feed | MoveKind::Plunge | MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. }
        )
    }

    /// True when the move is one of the two arcs.
    pub fn is_arc(&self) -> bool {
        matches!(self, MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. })
    }
}

/// One machine move. The start of a move is the end of the one before it, and
/// the first move starts from the toolpath home position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Move {
    /// What the move does.
    pub kind: MoveKind,
    /// Where the move ends, in millimetres.
    pub to: Point3,
    /// Laser power for this move as a percentage of the machine maximum,
    /// overriding the tool's power. `None` keeps the power already set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<f64>,
}

impl Move {
    /// A rapid travel move.
    pub fn rapid(to: Point3) -> Move {
        Move {
            kind: MoveKind::Rapid,
            to,
            power: None,
        }
    }

    /// A cutting move at the feed rate.
    pub fn feed(to: Point3) -> Move {
        Move {
            kind: MoveKind::Feed,
            to,
            power: None,
        }
    }

    /// A move down into the material at the plunge rate.
    pub fn plunge(to: Point3) -> Move {
        Move {
            kind: MoveKind::Plunge,
            to,
            power: None,
        }
    }

    /// A clockwise arc about `centre`.
    pub fn arc_cw(centre: Point, to: Point3) -> Move {
        Move {
            kind: MoveKind::ArcCw { centre },
            to,
            power: None,
        }
    }

    /// A counter clockwise arc about `centre`.
    pub fn arc_ccw(centre: Point, to: Point3) -> Move {
        Move {
            kind: MoveKind::ArcCcw { centre },
            to,
            power: None,
        }
    }

    /// A pause in place at `at`.
    pub fn dwell(seconds: f64, at: Point3) -> Move {
        Move {
            kind: MoveKind::Dwell { seconds },
            to: at,
            power: None,
        }
    }

    /// The same move at a laser power, a percentage of the machine maximum.
    #[must_use]
    pub fn with_power(mut self, power: f64) -> Move {
        self.power = Some(power);
        self
    }

    /// Length of the move from `from`, in millimetres. An arc is measured
    /// along its sweep, including any change of height. A dwell is zero.
    pub fn length(&self, from: Point3) -> f64 {
        match self.kind {
            MoveKind::Dwell { .. } => 0.0,
            MoveKind::Rapid | MoveKind::Feed | MoveKind::Plunge => from.distance(self.to),
            MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                let sweep = self.arc_sweep(from).unwrap_or(0.0);
                let radius = centre.distance(from.xy());
                let flat = (sweep.abs() * radius).abs();
                let rise = self.to.z - from.z;
                (flat * flat + rise * rise).sqrt()
            }
        }
    }

    /// Signed sweep of an arc move in radians, positive counter clockwise.
    /// `None` for a move that is not an arc.
    pub fn arc_sweep(&self, from: Point3) -> Option<f64> {
        let (centre, clockwise) = match self.kind {
            MoveKind::ArcCw { centre } => (centre, true),
            MoveKind::ArcCcw { centre } => (centre, false),
            _ => return None,
        };
        let start = centre.to(from.xy()).angle();
        let end = centre.to(self.to.xy()).angle();
        let mut sweep = end - start;
        let full = std::f64::consts::TAU;
        // An arc that ends where it starts is a full circle, not a zero sweep.
        if clockwise {
            while sweep > -1e-12 {
                sweep -= full;
            }
        } else {
            while sweep < 1e-12 {
                sweep += full;
            }
        }
        Some(sweep)
    }
}

/// Which strategy calculated a toolpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyKind {
    /// Cut along a vector at an offset.
    Profile,
    /// Clear an enclosed area.
    Pocket,
    /// Drill at vector centres.
    Drill,
    /// Single pass engraving.
    QuickEngrave,
    /// Matched plug and socket.
    Inlay,
    /// Variable depth V carving.
    VGroove,
    /// Flutes with ramped ends.
    Fluting,
    /// Repeated decorative pattern.
    Texture,
    /// Raised sharp edged prism.
    PrismCarve,
    /// Profile swept along a drive rail.
    Moulding,
    /// Bulk clearance of the 3D model.
    Rough3d,
    /// Final surface pass over the 3D model.
    Finish3d,
    /// A flat toolpath dropped onto the model surface.
    Project3d,
    /// Laser cut, fill or picture.
    Laser,
    /// Wrapped rotary machining.
    Rotary,
    /// A V shaped bevel along edges.
    Chamfer,
    /// A helical thread round circle centres.
    ThreadMilling,
    /// A straight walled inlay insert or cavity.
    StraightInlay,
    /// A photograph carved as grooves of varying depth.
    #[serde(rename = "photoCarve")]
    PhotoCarve,
    /// Keyhole hanging slots run out from vector centres and back.
    Keyhole,
    /// Moves read from a G-code file (Import Toolpaths): positioned and
    /// deepened, never recalculated.
    Imported,
    /// Anything else, including toolpaths merged from others.
    Custom,
}

/// Axis aligned box around a toolpath, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds3 {
    /// Lowest corner.
    pub min: Point3,
    /// Highest corner.
    pub max: Point3,
}

impl Bounds3 {
    /// A box holding a single position.
    pub fn from_point(p: Point3) -> Bounds3 {
        Bounds3 { min: p, max: p }
    }

    /// Grows the box to hold `p`.
    pub fn with_point(self, p: Point3) -> Bounds3 {
        Bounds3 {
            min: Point3::new(
                self.min.x.min(p.x),
                self.min.y.min(p.y),
                self.min.z.min(p.z),
            ),
            max: Point3::new(
                self.max.x.max(p.x),
                self.max.y.max(p.y),
                self.max.z.max(p.z),
            ),
        }
    }
}

/// A calculated toolpath, ready to simulate and to post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolpath {
    /// Document identity.
    pub id: u64,
    /// Name shown in the toolpath list and written into the output file.
    pub name: String,
    /// Copy of the tool the path was calculated with, so that editing the
    /// database later does not silently change a calculated path.
    pub tool: Tool,
    /// The moves in order.
    pub moves: Vec<Move>,
    /// Height the tool travels at between cuts, in millimetres.
    pub safe_z: f64,
    /// Where the machine starts and ends, in millimetres.
    pub home: Point3,
    /// Extra height above the material the tool lifts to for short hops, in
    /// millimetres.
    pub rapid_clearance_gap: f64,
    /// Tool number sent on a tool change.
    pub tool_number: u32,
    /// Spindle speed in revolutions per minute.
    pub spindle: f64,
    /// Cutting feed rate in millimetres per minute.
    pub feed: f64,
    /// Plunge feed rate in millimetres per minute.
    pub plunge: f64,
    /// Box around every move, `None` while the toolpath is empty.
    pub bounds: Option<Bounds3>,
    /// Machining time estimate in seconds.
    pub estimated_seconds: f64,
    /// Whether the toolpath is drawn and included in "all visible" output.
    pub visible: bool,
    /// Which strategy calculated it.
    pub kind: StrategyKind,
    /// The strategy form's parameters, kept so that the path can be edited
    /// and recalculated.
    pub params_json: serde_json::Value,
    /// The vectors the toolpath was calculated from and their revisions at
    /// the time, so that a later edit marks the toolpath out of date.
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    /// The side of a double sided job the toolpath cuts, whose job space
    /// its moves are in. Top on a single sided job.
    #[serde(default)]
    pub side: Side,
    /// The sheet the toolpath belongs to; zero for a toolpath from before
    /// sheets owned toolpaths, which shows on every sheet.
    #[serde(default)]
    pub sheet: u64,
    /// The toolpath group it sits in in the list; zero for none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub group: u64,
    /// How far the toolpath was set to cut below the bottom of its material
    /// when it was last calculated from its settings, in millimetres (zero
    /// when it stopped inside the material). The depth check after a job
    /// change allows this much; `None` until the toolpath has moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_cut: Option<f64>,
    /// The gadget that made the toolpath and the dialog values it ran with,
    /// so that editing the toolpath reopens that gadget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gadget: Option<GadgetSource>,
    /// Feeds and spindle speed that change part way through, for a toolpath
    /// put together from others (Merge Toolpaths): each entry holds from its
    /// move on. Empty when `feed`, `plunge` and `spindle` hold throughout.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rates: Vec<RateChange>,
    /// A drill bank toolpath's firings, in cutting order: where the head's
    /// reference drill stands and which drills go down there. Empty for
    /// every other toolpath.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub firings: Vec<BankFiring>,
    /// A tapered profile's rigid walls, as the exact solids read them.
    /// Empty for every other toolpath.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slopes: Vec<super::taper::SlopeCut>,
}

/// One firing of a drill bank: the reference drill's position and the drill
/// numbers lowered together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankFiring {
    /// Job position of the bank's reference drill, in millimetres.
    pub at: crate::geom::Point,
    /// Drill numbers that go down, lowest first.
    pub drills: Vec<u32>,
    /// Of `drills`, those on the bank's horizontal arm (the central drill's
    /// row), lowest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub along_x: Vec<u32>,
    /// Of `drills`, those on the bank's vertical arm (the central drill's
    /// column), lowest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub along_y: Vec<u32>,
}

/// The feed rate, plunge rate and spindle speed from one move of a toolpath
/// on, until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateChange {
    /// Index of the first move the rates apply to.
    pub from: usize,
    /// Feed rate, millimetres per minute.
    pub feed: f64,
    /// Plunge rate, millimetres per minute.
    pub plunge: f64,
    /// Spindle speed, revolutions per minute.
    pub spindle: f64,
}

/// Feed rate, plunge rate and spindle speed a move runs at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    /// Feed rate, millimetres per minute.
    pub feed: f64,
    /// Plunge rate, millimetres per minute.
    pub plunge: f64,
    /// Spindle speed, revolutions per minute.
    pub spindle: f64,
}

/// The gadget run that made a toolpath.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GadgetSource {
    /// The gadget's name (its folder).
    pub name: String,
    /// The dialog values it ran with, lengths in millimetres.
    pub values: serde_json::Value,
    /// Vectors the run drew (preview outlines), with their revisions, so a
    /// later edit can take back the ones nobody has changed since.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawn: Vec<SourceRef>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// One vector a toolpath was calculated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Entity id.
    pub id: u64,
    /// The entity's revision when the toolpath was calculated.
    pub revision: u64,
}

impl Toolpath {
    /// An empty toolpath cut with `tool`, taking its feeds and speeds from it.
    pub fn new(id: u64, name: impl Into<String>, tool: Tool, kind: StrategyKind) -> Toolpath {
        let feed = tool.feed_mm_per_min();
        let plunge = tool.plunge_mm_per_min();
        let spindle = tool.spindle_speed;
        let tool_number = tool.number;
        Toolpath {
            id,
            name: name.into(),
            tool,
            moves: Vec::new(),
            safe_z: 5.0,
            home: Point3::new(0.0, 0.0, 25.0),
            rapid_clearance_gap: 2.0,
            tool_number,
            spindle,
            feed,
            plunge,
            bounds: None,
            estimated_seconds: 0.0,
            visible: true,
            kind,
            params_json: serde_json::Value::Null,
            sources: Vec::new(),
            side: Side::Top,
            sheet: 0,
            group: 0,
            through_cut: None,
            gadget: None,
            rates: Vec::new(),
            firings: Vec::new(),
            slopes: Vec::new(),
        }
    }

    /// The rates move `index` runs at: the last change at or before it, or
    /// the toolpath's own feeds and speed.
    pub fn rates_at(&self, index: usize) -> Rates {
        let own = Rates {
            feed: self.feed,
            plunge: self.plunge,
            spindle: self.spindle,
        };
        let count = self.rates.partition_point(|r| r.from <= index);
        match count.checked_sub(1).and_then(|i| self.rates.get(i)) {
            Some(change) => Rates {
                feed: change.feed,
                plunge: change.plunge,
                spindle: change.spindle,
            },
            None => own,
        }
    }

    /// True when the toolpath has no moves.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Where the tool sits before the first move.
    pub fn start(&self) -> Point3 {
        self.home
    }

    /// Where the tool sits after the last move.
    pub fn end(&self) -> Point3 {
        self.moves.last().map_or(self.home, |m| m.to)
    }

    /// Walks the moves with the position each one starts from.
    pub fn iter_moves(&self) -> impl Iterator<Item = (Point3, &Move)> {
        let mut from = self.home;
        self.moves.iter().map(move |m| {
            let start = from;
            from = m.to;
            (start, m)
        })
    }

    /// Total length of every cutting move, in millimetres.
    pub fn cutting_length(&self) -> f64 {
        self.iter_moves()
            .filter(|(_, m)| m.kind.is_cutting())
            .map(|(from, m)| m.length(from))
            .sum()
    }

    /// Recomputes [`Toolpath::bounds`] from the moves, including the home
    /// position the path starts and ends at.
    pub fn recompute_bounds(&mut self) {
        let mut bounds = if self.moves.is_empty() {
            None
        } else {
            Some(Bounds3::from_point(self.home))
        };
        for m in &self.moves {
            bounds = Some(match bounds {
                Some(b) => b.with_point(m.to),
                None => Bounds3::from_point(m.to),
            });
        }
        self.bounds = bounds;
    }

    /// True when the tool is a laser, which the post processor answers with
    /// the jet blocks rather than a spindle.
    pub fn is_laser(&self) -> bool {
        matches!(
            self.tool.tool_type,
            super::tool::ToolGeometry::LaserTool { .. }
        )
    }

    /// Checks the toolpath is machinable.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first problem found.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::validation("a toolpath needs a name"));
        }
        if !self.home.is_finite() || !self.safe_z.is_finite() {
            return Err(CoreError::validation(
                "home position and safe Z must be numbers",
            ));
        }
        if self.feed <= 0.0 || self.plunge <= 0.0 {
            return Err(CoreError::validation(
                "feed and plunge rates must be greater than zero",
            ));
        }
        for (index, m) in self.moves.iter().enumerate() {
            if !m.to.is_finite() {
                return Err(CoreError::validation(format!(
                    "move {index} ends at a position that is not a number"
                )));
            }
            if let MoveKind::Dwell { seconds } = m.kind {
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err(CoreError::validation(format!(
                        "move {index} dwells for a time that is not a positive number"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;

    fn tool() -> Tool {
        Tool {
            name: "6 mm end mill".to_string(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        }
    }

    fn path() -> Toolpath {
        let mut tp = Toolpath::new(1, "profile 1", tool(), StrategyKind::Profile);
        tp.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 5.0)),
            Move::plunge(Point3::new(0.0, 0.0, -3.0)),
            Move::feed(Point3::new(10.0, 0.0, -3.0)),
            Move::feed(Point3::new(10.0, 20.0, -3.0)),
            Move::rapid(Point3::new(10.0, 20.0, 5.0)),
        ];
        tp
    }

    #[test]
    fn a_new_toolpath_takes_feeds_from_its_tool() {
        let tp = Toolpath::new(3, "pocket 1", tool(), StrategyKind::Pocket);
        assert_eq!(tp.feed, 2400.0);
        assert_eq!(tp.plunge, 600.0);
        assert_eq!(tp.spindle, 18000.0);
        assert!(tp.is_empty());
        assert!(tp.bounds.is_none());
        assert!(tp.visible);
    }

    #[test]
    fn moves_are_walked_from_the_home_position() {
        let tp = path();
        let starts: Vec<f64> = tp.iter_moves().map(|(from, _)| from.z).collect();
        assert_eq!(starts, vec![25.0, 5.0, -3.0, -3.0, -3.0]);
        assert_eq!(tp.start().z, 25.0);
        assert_eq!(tp.end().z, 5.0);
    }

    #[test]
    fn cutting_length_skips_rapids() {
        let tp = path();
        // 8 mm of plunge plus 10 mm and 20 mm of feed.
        assert!((tp.cutting_length() - 38.0).abs() < 1e-9);
    }

    #[test]
    fn bounds_cover_the_moves_and_home() {
        let mut tp = path();
        tp.recompute_bounds();
        let bounds = tp.bounds.expect("bounds");
        assert_eq!(bounds.min.x, 0.0);
        assert_eq!(bounds.max.x, 10.0);
        assert_eq!(bounds.max.y, 20.0);
        assert_eq!(bounds.min.z, -3.0);
        assert_eq!(bounds.max.z, 25.0);
    }

    #[test]
    fn an_empty_toolpath_has_no_bounds() {
        let mut tp = Toolpath::new(1, "empty", tool(), StrategyKind::Profile);
        tp.recompute_bounds();
        assert!(tp.bounds.is_none());
    }

    #[test]
    fn arc_length_follows_the_sweep() {
        let from = Point3::new(10.0, 0.0, -1.0);
        let quarter = Move::arc_ccw(Point::new(0.0, 0.0), Point3::new(0.0, 10.0, -1.0));
        let expected = std::f64::consts::FRAC_PI_2 * 10.0;
        assert!((quarter.length(from) - expected).abs() < 1e-9);
        let clockwise = Move::arc_cw(Point::new(0.0, 0.0), Point3::new(0.0, 10.0, -1.0));
        let three_quarters = 3.0 * expected;
        assert!((clockwise.length(from) - three_quarters).abs() < 1e-9);
    }

    #[test]
    fn a_helical_arc_counts_its_rise() {
        let from = Point3::new(10.0, 0.0, 0.0);
        let helix = Move::arc_ccw(Point::new(0.0, 0.0), Point3::new(0.0, 10.0, -3.0));
        let flat = std::f64::consts::FRAC_PI_2 * 10.0;
        let expected = (flat * flat + 9.0f64).sqrt();
        assert!((helix.length(from) - expected).abs() < 1e-9);
    }

    #[test]
    fn a_full_circle_sweeps_all_the_way_round() {
        let from = Point3::new(10.0, 0.0, -1.0);
        let circle = Move::arc_ccw(Point::new(0.0, 0.0), Point3::new(10.0, 0.0, -1.0));
        let sweep = circle.arc_sweep(from).expect("an arc sweeps");
        assert!((sweep - std::f64::consts::TAU).abs() < 1e-9, "{sweep}");
        assert!(circle.arc_sweep(Point3::ORIGIN).is_some());
        assert!(Move::feed(Point3::ORIGIN).arc_sweep(from).is_none());
    }

    #[test]
    fn dwell_has_no_length() {
        let from = Point3::new(1.0, 2.0, -1.0);
        assert_eq!(Move::dwell(2.0, from).length(from), 0.0);
        assert!(!MoveKind::Dwell { seconds: 1.0 }.is_cutting());
        assert!(!MoveKind::Rapid.is_cutting());
        assert!(MoveKind::Plunge.is_cutting());
        assert!(MoveKind::ArcCw {
            centre: Point::ORIGIN
        }
        .is_arc());
    }

    #[test]
    fn validation_catches_broken_moves() {
        let mut tp = path();
        assert!(tp.validate().is_ok());
        tp.moves.push(Move::dwell(-1.0, Point3::ORIGIN));
        assert!(tp.validate().is_err());
        tp.moves.pop();
        tp.moves.push(Move::feed(Point3::new(f64::NAN, 0.0, 0.0)));
        assert!(tp.validate().is_err());
        tp.moves.pop();
        tp.feed = 0.0;
        assert!(tp.validate().is_err());
        tp.feed = 1000.0;
        tp.name = " ".to_string();
        assert!(tp.validate().is_err());
    }

    #[test]
    fn toolpath_json_uses_camel_case_and_round_trips() {
        let mut tp = path();
        tp.recompute_bounds();
        tp.params_json = serde_json::json!({ "cutDepth": 3.0 });
        let json = serde_json::to_string(&tp).expect("toolpath serialises");
        assert!(json.contains("\"safeZ\":5.0"), "{json}");
        assert!(json.contains("\"rapidClearanceGap\""), "{json}");
        assert!(json.contains("\"kind\":\"profile\""), "{json}");
        assert!(json.contains("{\"kind\":\"plunge\"}"), "{json}");
        let back: Toolpath = serde_json::from_str(&json).expect("toolpath round trips");
        assert_eq!(back, tp);
    }

    #[test]
    fn a_laser_toolpath_knows_it_is_one() {
        let mut tp = path();
        assert!(!tp.is_laser());
        tp.tool.tool_type = ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
        };
        assert!(tp.is_laser());
    }
}
