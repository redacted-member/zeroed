//! The controller model the program readers drive: modal state, the tool
//! position and the checks made as each move runs.

use std::collections::HashSet;
use std::f64::consts::PI;

use crate::field::Field;
use crate::{Dialect, Issue, Kind, Motion, Report, Setup, Severity, ToolLength, Units};

/// Height a Z reference return (G28) is taken to leave the tool at: above
/// anything on the table.
pub(crate) const REFERENCE_Z: f64 = 1000.0;

/// Plane arcs are cut in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plane {
    Xy,
    Zx,
    Yz,
}

impl Plane {
    /// Axis indices (first, second, normal) of the plane, ordered so that
    /// counter clockwise is seen looking down the normal.
    fn axes(self) -> (usize, usize, usize) {
        match self {
            Plane::Xy => (0, 1, 2),
            Plane::Zx => (2, 0, 1),
            Plane::Yz => (1, 2, 0),
        }
    }
}

/// Axis words of one block, in program units, each flagged when it is an
/// incremental word whatever the distance mode (a Heidenhain `IX`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Axes {
    pub(crate) v: [Option<f64>; 5],
    pub(crate) inc: [bool; 5],
}

impl Axes {
    pub(crate) const X: usize = 0;
    pub(crate) const Y: usize = 1;
    pub(crate) const Z: usize = 2;
    pub(crate) const A: usize = 3;
    pub(crate) const B: usize = 4;

    pub(crate) fn any(&self) -> bool {
        self.v.iter().any(Option::is_some)
    }

    pub(crate) fn set(&mut self, axis: usize, value: f64) {
        self.v[axis] = Some(value);
    }
}

/// How an arc gives its centre, in program units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Centre {
    /// Offsets along X, Y and Z from the start (or absolute, per the arc
    /// distance mode), as I, J and K.
    Offsets([Option<f64>; 3]),
    /// Radius, negative for more than half a turn.
    Radius(f64),
    /// Absolute centre in the plane's first and second axes.
    Absolute(f64, f64),
}

pub(crate) struct Machine<'a> {
    setup: &'a Setup,
    line: usize,
    units: Option<Units>,
    absolute: Option<bool>,
    arc_absolute: bool,
    plane: Plane,
    inverse_time: bool,
    feed: Option<f64>,
    pos: [f64; 3],
    known: [bool; 3],
    /// The block being run gives Z as an absolute word, so the move ends
    /// at a known height whatever it started from.
    z_word_absolute: bool,
    spindle: bool,
    speed: f64,
    selected: Option<u32>,
    tool: Option<u32>,
    /// The tool in the spindle has had its length measured since it was
    /// fitted.
    measured: bool,
    /// The M6 that fitted the tool in the spindle probed its length.
    probed: bool,
    /// A cut with the unmeasured tool in the spindle was reported (once
    /// per tool fitted).
    unmeasured_reported: bool,
    paused: bool,
    ended: bool,
    /// Modal states the program has set itself: feed mode, plane, cutter
    /// compensation and the tool length offset.
    feed_mode_set: bool,
    plane_set: bool,
    comp_set: bool,
    length_set: bool,
    preamble_checked: bool,
    motions: Vec<Motion>,
    issues: Vec<Issue>,
    seen: HashSet<(Kind, usize)>,
    tool_changes: Vec<u32>,
    dwells: Vec<f64>,
    field: Option<Field>,
}

impl<'a> Machine<'a> {
    pub(crate) fn new(setup: &'a Setup) -> Machine<'a> {
        Machine {
            setup,
            line: 0,
            units: None,
            absolute: None,
            arc_absolute: false,
            plane: Plane::Xy,
            inverse_time: false,
            feed: None,
            pos: [0.0; 3],
            // A laser's height is its focus, set on the machine, not by
            // the program.
            known: [false, false, setup.laser],
            z_word_absolute: false,
            spindle: false,
            speed: 0.0,
            selected: None,
            tool: setup.fitted_tool,
            measured: false,
            probed: false,
            unmeasured_reported: false,
            paused: false,
            ended: false,
            feed_mode_set: false,
            plane_set: false,
            comp_set: false,
            length_set: false,
            preamble_checked: false,
            motions: Vec::new(),
            issues: Vec::new(),
            seen: HashSet::new(),
            tool_changes: setup.fitted_tool.into_iter().collect(),
            dwells: Vec::new(),
            field: setup.grid.map(Field::new),
        }
    }

    pub(crate) fn set_line(&mut self, line: usize) {
        self.line = line;
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
    }

    /// Records a finding once per kind and line.
    pub(crate) fn issue(&mut self, severity: Severity, kind: Kind, text: impl Into<String>) {
        self.issue_at(self.line, severity, kind, text);
    }

    /// Records a finding once per kind for the whole program.
    fn issue_once(&mut self, severity: Severity, kind: Kind, text: impl Into<String>) {
        if self.seen.insert((kind, 0)) {
            let line = self.line;
            self.issues.push(Issue {
                line,
                severity,
                kind,
                text: text.into(),
            });
        }
    }

    fn issue_at(&mut self, line: usize, severity: Severity, kind: Kind, text: impl Into<String>) {
        if self.seen.insert((kind, line)) {
            self.issues.push(Issue {
                line,
                severity,
                kind,
                text: text.into(),
            });
        }
    }

    pub(crate) fn set_units(&mut self, units: Units) {
        if units != self.setup.units {
            self.issue(
                Severity::Error,
                Kind::WrongUnits,
                format!(
                    "the program sets {units:?} but the post says it writes {:?}",
                    self.setup.units
                ),
            );
        }
        self.units = Some(units);
    }

    /// Millimetres per program unit, reporting a program that moves before
    /// it sets its units (the post's own units are assumed from then on).
    fn scale(&mut self) -> f64 {
        match self.units {
            Some(u) => u.mm(),
            None => {
                self.issue_once(
                    Severity::Error,
                    Kind::NoUnits,
                    "moves before the program sets its units: the machine would use whatever \
                     units it was left in",
                );
                self.setup.units.mm()
            }
        }
    }

    pub(crate) fn set_absolute(&mut self, absolute: bool) {
        self.absolute = Some(absolute);
    }

    pub(crate) fn set_arc_absolute(&mut self, absolute: bool) {
        self.arc_absolute = absolute;
    }

    pub(crate) fn set_plane(&mut self, plane: Plane) {
        self.plane = plane;
        self.plane_set = true;
    }

    pub(crate) fn set_inverse_time(&mut self, on: bool) {
        self.inverse_time = on;
        self.feed_mode_set = true;
    }

    /// Cutter radius compensation off (G40).
    pub(crate) fn compensation_off(&mut self) {
        self.comp_set = true;
    }

    /// Tool length offset cancelled (G49). After an M6 that probed, it
    /// throws the probed length away; before one, a routine that skips a
    /// change to the tool already fitted would not probe again, so a
    /// probing machine's program never cancels the length at all.
    pub(crate) fn cancel_length_offset(&mut self) {
        self.length_set = true;
        if self.setup.tool_length == ToolLength::ProbedByM6 {
            self.issue(
                Severity::Error,
                Kind::ToolLength,
                "G49 on a machine whose M6 probes the tool: it cancels the probed length (an M6 \
                 routine that skips a change to the tool already fitted does not probe again)",
            );
            self.measured = false;
        }
        if self.setup.tool_length == ToolLength::TableAfterM6 {
            self.measured = false;
        }
    }

    /// Reports, once, the modal states a program that must set its own
    /// left to the last program run.
    fn check_preamble(&mut self) {
        if !self.setup.preamble || self.preamble_checked {
            return;
        }
        self.preamble_checked = true;
        // A machine whose M6 probes keeps the length it last probed for the
        // tool in the spindle: that is the one to move with until the next
        // change, so the program leaves it alone.
        let length_set = self.length_set || self.setup.tool_length == ToolLength::ProbedByM6;
        let missing: Vec<&str> = [
            (self.feed_mode_set, "the feed mode (G93 or G94)"),
            (self.plane_set, "the plane (G17)"),
            (self.comp_set, "cutter compensation off (G40)"),
            (length_set, "the tool length offset (G43 or G49)"),
        ]
        .into_iter()
        .filter(|(set, _)| !set)
        .map(|(_, what)| what)
        .collect();
        if !missing.is_empty() {
            self.issue(
                Severity::Error,
                Kind::Modal,
                format!(
                    "moves before the program sets {}: the controller keeps them from the last \
                     program run",
                    missing.join(", ")
                ),
            );
        }
    }

    /// Sets the feed, in program units per minute (or moves per minute in
    /// inverse time).
    pub(crate) fn set_feed(&mut self, feed: f64) {
        if feed <= 0.0 || !feed.is_finite() {
            self.issue(Severity::Error, Kind::NoFeed, "a feed rate of zero");
        }
        self.feed = Some(feed);
    }

    pub(crate) fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    pub(crate) fn spindle(&mut self, on: bool) {
        self.spindle = on;
    }

    pub(crate) fn select_tool(&mut self, tool: u32) {
        self.selected = Some(tool);
        if self.paused {
            // A pause next to a tool number is a manual tool change.
            self.load(tool, true);
        }
    }

    /// An automatic tool change of the selected tool (M6 and alike). On a
    /// machine whose M6 probes, the new tool's length is measured by it.
    pub(crate) fn change_tool(&mut self) {
        match self.selected {
            Some(tool) => {
                self.load(tool, false);
                if self.setup.tool_length == ToolLength::ProbedByM6 {
                    self.measured = true;
                    self.probed = true;
                    self.length_set = true;
                }
            }
            None => self.issue(
                Severity::Error,
                Kind::NoTool,
                "a tool change with no tool selected",
            ),
        }
    }

    /// A program stop, where the operator may change the tool by hand.
    pub(crate) fn pause(&mut self) {
        self.paused = true;
        // The operator may jog the tool anywhere while the program waits.
        self.known[2] = false;
        if let Some(tool) = self.selected {
            if self.tool != Some(tool) {
                self.load(tool, true);
            }
        }
        // The operator touches the tool off while the program waits.
        if self.setup.tool_length == ToolLength::TouchOffAtPause && self.tool.is_some() {
            self.measured = true;
        }
    }

    /// A tool length offset from the tool table (G43 H). After an M6 that
    /// probed, it throws the probed length away for the table's entry.
    /// On a machine that takes the length from the table after its M6, a
    /// G43 H naming the tool in the spindle measures it.
    pub(crate) fn table_length_offset(&mut self, h: Option<u32>) {
        self.length_set = true;
        if self.setup.tool_length == ToolLength::TableAfterM6 {
            if let Some(tool) = self.tool {
                if h == Some(tool) {
                    self.measured = true;
                } else {
                    self.issue(
                        Severity::Error,
                        Kind::ToolLength,
                        format!(
                            "G43 H{} does not name tool {tool} in the spindle",
                            h.map(|h| h.to_string()).unwrap_or_default()
                        ),
                    );
                }
            }
        }
        if self.probed {
            self.issue(
                Severity::Error,
                Kind::ToolLength,
                "G43 H after an M6 that probed the tool: the tool table's length replaces the \
                 one just measured (the probing M6 applies its length itself)",
            );
        }
    }

    fn load(&mut self, tool: u32, by_hand: bool) {
        if !by_hand && self.inverse_time {
            self.issue(
                Severity::Error,
                Kind::ToolChangeInverseTime,
                format!(
                    "tool {tool} is changed in inverse time (G93): a change routine's feeds, its \
                     probing feed too, would be read as one over minutes; write G94 before M6"
                ),
            );
        }
        if self.spindle {
            let severity = if by_hand {
                Severity::Error
            } else {
                Severity::Warning
            };
            self.issue(
                severity,
                Kind::SpindleOnAtChange,
                format!("tool {tool} is changed with the spindle still on"),
            );
        }
        if self.tool == Some(tool) && self.tool_changes.last() == Some(&tool) && self.paused {
            // The same change seen twice (a pause and an M6 around it).
            return;
        }
        self.tool = Some(tool);
        self.tool_changes.push(tool);
        self.measured = false;
        self.probed = false;
        self.unmeasured_reported = false;
        // A tool change stops the spindle; an automatic one leaves Z
        // wherever the changer left it (a pause already forgot Z).
        self.spindle = false;
        if !by_hand {
            self.known[2] = false;
        }
    }

    pub(crate) fn dwell(&mut self, seconds: f64) {
        self.dwells.push(seconds);
    }

    /// A return to the Z reference: the tool goes to the top of its travel.
    pub(crate) fn home_z(&mut self) {
        self.pos[2] = REFERENCE_Z;
        self.known[2] = true;
    }

    pub(crate) fn end(&mut self) {
        if self.ended {
            return;
        }
        if !self.setup.laser && (!self.known[2] || self.pos[2] < self.setup.safe_z - 1e-3) {
            let at = if self.known[2] {
                format!("Z {:.3}", self.pos[2])
            } else {
                "an unknown height".to_string()
            };
            self.issue(
                Severity::Error,
                Kind::UnsafeEnd,
                format!(
                    "the program ends with the tool at {at}, below the safe Z {:.3}",
                    self.setup.safe_z
                ),
            );
        }
        self.ended = true;
        self.spindle = false;
    }

    /// Current position in program units, for a reader that resolves its
    /// own incremental centres.
    pub(crate) fn position_in_units(&self) -> [f64; 3] {
        let s = self.units.unwrap_or(self.setup.units).mm();
        [self.pos[0] / s, self.pos[1] / s, self.pos[2] / s]
    }

    /// Where a block's axis words put the tool, in millimetres.
    fn target(&mut self, axes: &Axes) -> [Option<f64>; 3] {
        let scale = self.scale();
        let absolute = match self.absolute {
            Some(a) => a,
            None => {
                self.issue_once(
                    Severity::Error,
                    Kind::NoDistanceMode,
                    "moves before the program sets absolute or incremental distances",
                );
                true
            }
        };
        self.z_word_absolute = axes.v[Axes::Z].is_some() && absolute && !axes.inc[Axes::Z];
        let mut out = [None; 3];
        for (axis, slot) in out.iter_mut().enumerate() {
            if let Some(value) = axes.v[axis] {
                let mm = value * scale;
                *slot = Some(if absolute && !axes.inc[axis] {
                    mm
                } else {
                    self.pos[axis] + mm
                });
            }
        }
        if let Some(rotary) = self.setup.rotary {
            let word = if rotary.axis == 'B' { Axes::B } else { Axes::A };
            let flat = if rotary.across == 'X' {
                Axes::X
            } else {
                Axes::Y
            };
            if let Some(slot) = out[flat] {
                if slot.abs() > 1e-3 {
                    self.issue(
                        Severity::Error,
                        Kind::Syntax,
                        "a wrapped program moves the axis it wraps off the rotary centre line",
                    );
                }
                out[flat] = None;
            }
            if let Some(degrees) = axes.v[word] {
                let sign = if rotary.reversed { -1.0 } else { 1.0 };
                let distance = sign
                    * if rotary.linear {
                        degrees * self.units.unwrap_or(self.setup.units).mm()
                    } else {
                        degrees / 360.0 * PI * rotary.diameter
                    };
                out[flat] = Some(if absolute && !axes.inc[word] {
                    distance
                } else {
                    self.pos[flat] + distance
                });
            }
        } else if [Axes::A, Axes::B]
            .iter()
            .any(|a| axes.v[*a].is_some_and(|v| v.abs() > 1e-9))
        {
            self.issue(
                Severity::Error,
                Kind::Unsupported,
                "a flat job turns a rotary axis",
            );
        }
        out
    }

    /// Feed in millimetres per minute for a move of `length` millimetres,
    /// given an explicit feed in program units per minute or the modal one.
    fn feed_for(&mut self, length: f64, explicit: Option<f64>) -> Option<f64> {
        let raw = explicit.or(self.feed)?;
        if self.inverse_time && explicit.is_none() {
            return Some(length * raw);
        }
        Some(raw * self.units.unwrap_or(self.setup.units).mm())
    }

    /// The surface feed of a cut that turns the rotary axis in units per
    /// minute, where the control (GRBL, Mach) spreads F over the linear axes
    /// and the degrees turned alike: a turn of a blank whose circumference
    /// is not 360 mm runs at another surface speed than F. Moves in inverse
    /// time, and moves that do not turn, keep `feed`.
    fn rotary_surface_feed(&mut self, length: f64, to: [f64; 3], feed: f64) -> f64 {
        let Some(rotary) = self.setup.rotary else {
            return feed;
        };
        if self.inverse_time {
            return feed;
        }
        let flat = if rotary.across == 'X' { 0 } else { 1 };
        let across = (to[flat] - self.pos[flat]).abs();
        if across < 1e-9 || rotary.diameter <= 0.0 {
            return feed;
        }
        let scale = self.units.unwrap_or(self.setup.units).mm();
        let degrees = across / (PI * rotary.diameter) * 360.0;
        let linear: f64 = (0..3)
            .filter(|a| *a != flat)
            .map(|a| ((to[a] - self.pos[a]) / scale).powi(2))
            .sum();
        let control = (linear + degrees * degrees).sqrt();
        let minutes = control / (feed / scale);
        let surface = if minutes > 0.0 {
            length / minutes
        } else {
            feed
        };
        if (surface / feed - 1.0).abs() > 0.02 {
            self.issue(
                Severity::Error,
                Kind::RotaryFeed,
                format!(
                    "a cut turns the rotary axis at F in units per minute: the control reads it \
                     as degrees per minute, so the surface moves at {surface:.0} mm/min, not \
                     {feed:.0} (use inverse time, G93)"
                ),
            );
        }
        surface
    }

    /// A straight move, rapid or at feed.
    pub(crate) fn linear(&mut self, rapid: bool, axes: &Axes, feed: Option<f64>) {
        if self.ended {
            return;
        }
        let target = self.target(axes);
        let mut to = self.pos;
        for axis in 0..3 {
            if let Some(v) = target[axis] {
                to[axis] = v;
            }
        }
        self.travel(rapid, false, target, to, feed);
    }

    /// An arc from the current position to `axes`, flattened to straight
    /// moves.
    pub(crate) fn arc(&mut self, cw: bool, axes: &Axes, centre: Centre, feed: Option<f64>) {
        if self.ended {
            return;
        }
        let target = self.target(axes);
        let mut end = self.pos;
        for axis in 0..3 {
            if let Some(v) = target[axis] {
                end[axis] = v;
            }
        }
        let start = self.pos;
        let (u, v, w) = self.plane.axes();
        if !self.known[u] || !self.known[v] {
            self.issue(
                Severity::Error,
                Kind::Syntax,
                "an arc from an unknown position",
            );
        }
        let scale = self.units.unwrap_or(self.setup.units).mm();
        let (cu, cv) = match centre {
            Centre::Offsets(ijk) => {
                let off = |axis: usize| ijk[axis].unwrap_or(0.0) * scale;
                if self.arc_absolute {
                    (off(u), off(v))
                } else {
                    (start[u] + off(u), start[v] + off(v))
                }
            }
            Centre::Absolute(a, b) => (a * scale, b * scale),
            Centre::Radius(r) => {
                let r_mm = r * scale;
                let (du, dv) = (end[u] - start[u], end[v] - start[v]);
                let chord = du.hypot(dv);
                if chord < 1e-9 {
                    self.issue(
                        Severity::Error,
                        Kind::ArcRadius,
                        "a radius arc whose end is its start",
                    );
                    return self.travel(false, true, target, end, feed);
                }
                let half = chord / 2.0;
                let mut h2 = r_mm * r_mm - half * half;
                if h2 < 0.0 {
                    if half - r_mm.abs() > 0.005 {
                        self.issue(
                            Severity::Error,
                            Kind::ArcRadius,
                            format!(
                                "an arc of radius {:.4} cannot span {chord:.4} mm",
                                r_mm.abs()
                            ),
                        );
                    }
                    h2 = 0.0;
                }
                let h = h2.sqrt();
                // Left of the chord for a counter clockwise arc of less than
                // half a turn; a negative radius or a clockwise arc flips it.
                let left = (!cw) == (r_mm > 0.0);
                let (mu, mv) = (start[u] + du / 2.0, start[v] + dv / 2.0);
                let (nu, nv) = (-dv / chord, du / chord);
                let sign = if left { 1.0 } else { -1.0 };
                (mu + sign * h * nu, mv + sign * h * nv)
            }
        };
        let r0 = (start[u] - cu).hypot(start[v] - cv);
        let r1 = (end[u] - cu).hypot(end[v] - cv);
        let diff = (r0 - r1).abs();
        if diff > 0.0127_f64.max(0.001 * r0) {
            self.issue(
                Severity::Error,
                Kind::ArcRadius,
                format!(
                    "the arc's end is {diff:.4} mm off its circle (radius {r0:.4} at the start, \
                     {r1:.4} at the end)"
                ),
            );
        } else if diff > 0.005 {
            self.issue(
                Severity::Warning,
                Kind::ArcRadius,
                format!("the arc's end is {diff:.4} mm off its circle"),
            );
        }
        let a0 = (start[v] - cv).atan2(start[u] - cu);
        let a1 = (end[v] - cv).atan2(end[u] - cu);
        let mut sweep = if cw { a0 - a1 } else { a1 - a0 };
        while sweep <= 1e-12 {
            sweep += 2.0 * PI;
        }
        let same = (end[u] - start[u]).hypot(end[v] - start[v]) < 1e-9;
        if same {
            sweep = 2.0 * PI;
        }
        let radius = r0.max(r1).max(1e-9);
        let tol = self.setup.arc_tolerance.clamp(1e-5, radius);
        let step = 2.0 * (1.0 - tol / radius).clamp(-1.0, 1.0).acos();
        let n = ((sweep / step.max(1e-6)).ceil() as usize).clamp(1, 20_000);
        let signed = if cw { -sweep } else { sweep };
        // The arc's length, for an inverse time feed.
        let length = (sweep * radius).hypot(end[w] - start[w]);
        let feed_mm = self.feed_for(length, feed);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let a = a0 + signed * t;
            let r = r0 + (r1 - r0) * t;
            let mut p = start;
            p[u] = cu + r * a.cos();
            p[v] = cv + r * a.sin();
            p[w] = start[w] + (end[w] - start[w]) * t;
            if k == n {
                p = end;
            }
            let target = [Some(p[0]), Some(p[1]), Some(p[2])];
            self.travel_at(false, true, target, p, feed_mm);
        }
    }

    fn travel(
        &mut self,
        rapid: bool,
        arc: bool,
        target: [Option<f64>; 3],
        to: [f64; 3],
        feed: Option<f64>,
    ) {
        let length = distance(self.pos, to);
        let feed_mm = if rapid {
            Some(0.0)
        } else {
            self.feed_for(length, feed)
                .map(|f| self.rotary_surface_feed(length, to, f))
        };
        self.travel_at(rapid, arc, target, to, feed_mm);
    }

    fn travel_at(
        &mut self,
        rapid: bool,
        arc: bool,
        target: [Option<f64>; 3],
        to: [f64; 3],
        feed_mm: Option<f64>,
    ) {
        self.check_preamble();
        let mut from = self.pos;
        let known_start = self.known.iter().all(|k| *k);
        let moves_xy =
            (0..2).any(|a| target[a].is_some_and(|v| !self.known[a] || (v - from[a]).abs() > 1e-6));
        for axis in 0..3 {
            if !self.known[axis] {
                from[axis] = to[axis];
            }
        }
        if moves_xy && !self.known[2] {
            // After a manual change the new cutter was zeroed on the
            // material, so a move across before Z lifts drags it through the
            // work (or into a clamp): the program must lift first.
            self.issue(
                Severity::Error,
                Kind::UnknownHeight,
                "moves across X and Y before Z is at a known height (the tool may still be down \
                 at the material after zeroing or a tool change)",
            );
        }
        // The tool fitted at a pause is the one the next tool number names,
        // up to the first cut.
        if !rapid {
            self.paused = false;
        }
        let cutter = self.tool.and_then(|t| self.setup.tools.get(&t).copied());
        let top = self.setup.grid.map_or(0.0, |g| g.top);
        // A move from an unknown height that gives no absolute Z ends
        // nobody knows where: it is not judged against the top.
        let z_known = self.known[2] || self.z_word_absolute;
        let below_top = z_known && from[2].min(to[2]) < top - 1e-6;
        if rapid {
            let low = from[2].min(to[2]);
            let across = (to[0] - from[0]).hypot(to[1] - from[1]);
            if across > 1e-3 && low < self.setup.safe_z - 1e-3 && known_start {
                self.issue(
                    Severity::Error,
                    Kind::RapidBelowSafeZ,
                    format!(
                        "a rapid across the job at Z {low:.3}, below the safe Z {:.3}",
                        self.setup.safe_z
                    ),
                );
            }
        } else {
            match feed_mm {
                Some(f) if f > 0.0 => {}
                _ => self.issue(
                    Severity::Error,
                    Kind::NoFeed,
                    "a cutting move with no feed rate",
                ),
            }
            if self.setup.laser {
                // In laser mode (M4) a feed at zero power is the beam held
                // off at the cutting speed, as over a picture's overscan;
                // with the laser stopped (M5) a feed burns nothing at all.
                if !self.spindle {
                    self.issue(Severity::Error, Kind::Beam, "a laser cut with the beam off");
                }
            } else if !self.spindle && below_top {
                self.issue(
                    Severity::Error,
                    Kind::SpindleOff,
                    "cuts into the material with the spindle stopped",
                );
            }
        }
        if !self.setup.laser {
            if below_top
                && self.tool.is_some()
                && !self.measured
                && !self.unmeasured_reported
                && self.setup.tool_length != ToolLength::Unchecked
            {
                self.unmeasured_reported = true;
                let how = if self.setup.tool_length == ToolLength::ProbedByM6 {
                    "no M6 probed it"
                } else if self.setup.tool_length == ToolLength::TableAfterM6 {
                    "no G43 H with its number followed the change"
                } else {
                    "no pause for a touch off followed the change"
                };
                self.issue(
                    Severity::Error,
                    Kind::ToolLength,
                    format!(
                        "cuts with tool {} whose length was not measured: {how}",
                        self.tool.unwrap_or_default()
                    ),
                );
            }
            if below_top && self.tool.is_none() {
                self.issue(Severity::Error, Kind::NoTool, "cuts with no tool loaded");
            } else if below_top && cutter.is_none() && !self.setup.tools.is_empty() {
                self.issue(
                    Severity::Error,
                    Kind::NoTool,
                    format!(
                        "cuts with tool {:?}, which the job does not have",
                        self.tool
                    ),
                );
            }
            if let Some(cutter) = cutter {
                let gap = match &self.field {
                    Some(field) if rapid && known_start => field.clearance(from, to, &cutter),
                    _ => None,
                };
                if let Some(gap) = gap.filter(|g| *g < -0.01) {
                    let text = format!("a rapid passes {:.3} mm into the material", -gap);
                    self.issue(Severity::Error, Kind::RapidIntoMaterial, text);
                }
                if let Some(field) = self.field.as_mut() {
                    if known_start || !rapid {
                        field.cut(from, to, &cutter);
                    }
                }
            }
        }
        self.motions.push(Motion {
            line: self.line,
            rapid,
            arc,
            from,
            to,
            feed: feed_mm.unwrap_or(0.0),
            tool: self.tool,
            spindle: self.spindle,
            speed: self.speed,
            known_start,
        });
        for (known, given) in self.known.iter_mut().zip(target) {
            *known |= given.is_some();
        }
        self.pos = to;
    }

    pub(crate) fn finish(mut self, dialect: Dialect) -> Report {
        if !self.ended {
            if self.spindle {
                let kind = if self.setup.laser {
                    Kind::Beam
                } else {
                    Kind::UnsafeEnd
                };
                self.issue(
                    Severity::Error,
                    kind,
                    "the program stops with the spindle or beam still on",
                );
            }
            self.issue(
                Severity::Warning,
                Kind::NoEnd,
                "the program has no end code",
            );
        }
        Report {
            dialect,
            issues: self.issues,
            motions: self.motions,
            tool_changes: self.tool_changes,
            units: self.units,
            field: self.field,
            dwells: self.dwells,
        }
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
