//! What every 2D strategy shares: turning depths below the surface into job
//! Z, splitting a cut into passes, and writing the moves that get the tool
//! to a cut, along it (with ramps and tabs) and home again.
//!
//! Toolpath Z is job Z: zero is wherever the job puts Z zero, the material
//! surface when Z zero is on top or the bed when it is on the bottom.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{bulge_from_sweep, Point, Vec2};

use super::chain::{Chain, CutSeg};
use super::toolpath::{Move, Point3};

/// Heights a toolpath moves between, all in job Z millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heights {
    /// The material's top surface.
    pub surface: f64,
    /// Clearance height rapids travel at.
    pub safe: f64,
    /// Height the tool rapids down to before it starts to plunge.
    pub plunge_from: f64,
    /// Where the job starts and ends.
    pub home: Point3,
}

impl Heights {
    /// The heights a job's material setup asks for.
    pub fn from_job(job: &JobSetup) -> Heights {
        let surface = job.surface_z();
        let m = &job.machine;
        // A square blank turns its corners past the surface the design wraps
        // onto: the heights are measured from the top of that sweep, so the
        // tool clears a corner coming round and feeds down through one.
        let rise = job.rotary_corner_rise();
        Heights {
            surface,
            safe: surface + rise + m.safe_z,
            plunge_from: surface + rise + m.plunge_gap,
            home: Point3::new(m.home_x, m.home_y, surface + rise + m.home_z),
        }
    }

    /// Job Z of a depth below the surface.
    pub fn z_at(&self, depth: f64) -> f64 {
        self.surface - depth
    }
}

/// Depths below the surface the passes finish at, shallowest first. Passes
/// are spread evenly, as many as it takes for none to be deeper than
/// `pass_depth`, and the last one lands exactly on `cut_depth`.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the cut depth is not deeper than
/// the start depth or the pass depth is not positive.
pub fn pass_depths(
    start_depth: f64,
    cut_depth: f64,
    pass_depth: f64,
) -> Result<Vec<f64>, CoreError> {
    if !start_depth.is_finite() || start_depth < 0.0 {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !cut_depth.is_finite() || cut_depth <= start_depth {
        return Err(CoreError::validation(
            "the cut depth must be deeper than the start depth",
        ));
    }
    if !pass_depth.is_finite() || pass_depth <= 0.0 {
        return Err(CoreError::validation(
            "the pass depth must be more than zero",
        ));
    }
    let span = cut_depth - start_depth;
    let count = ((span / pass_depth) - 1e-9).ceil().max(1.0) as usize;
    Ok((1..=count)
        .map(|i| {
            if i == count {
                cut_depth
            } else {
                start_depth + span * i as f64 / count as f64
            }
        })
        .collect())
}

/// How far the pass depth may grow, as a fraction of it, when that saves a
/// pass (unless the exact pass depth is kept).
pub const PASS_FLEX: f64 = 0.15;

/// How a toolpath's depth passes are chosen, as the Edit Passes dialog
/// sets them. Depths are millimetres below the material surface.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PassPlan<'a> {
    /// Evenly spaced passes, this many, instead of by the pass depth.
    pub count: Option<usize>,
    /// Thickness of a last pass set apart from the others.
    pub last_thickness: Option<f64>,
    /// Keep every pass exactly the pass depth (the last takes what is
    /// left) instead of letting it grow by up to [`PASS_FLEX`] to save a pass.
    pub exact: bool,
    /// A custom list of pass depths, which wins over everything else. The
    /// last pass always finishes on the cut depth.
    pub list: Option<&'a [f64]>,
}

/// Depths by the pass depth: with `exact`, a pass depth apart with the
/// last pass taking what is left; otherwise spread evenly, with the pass
/// depth grown by up to [`PASS_FLEX`] when that saves a pass.
///
/// # Errors
/// As [`pass_depths`].
pub fn stepped_depths(
    start_depth: f64,
    cut_depth: f64,
    pass_depth: f64,
    exact: bool,
) -> Result<Vec<f64>, CoreError> {
    let even = pass_depths(start_depth, cut_depth, pass_depth)?;
    let span = cut_depth - start_depth;
    if exact {
        let mut depths: Vec<f64> = (1..even.len())
            .map(|i| start_depth + pass_depth * i as f64)
            .filter(|d| *d < cut_depth - 1e-9)
            .collect();
        depths.push(cut_depth);
        return Ok(depths);
    }
    let fewer = ((span / (pass_depth * (1.0 + PASS_FLEX))) - 1e-9)
        .ceil()
        .max(1.0) as usize;
    if fewer < even.len() {
        return Ok((1..=fewer)
            .map(|i| {
                if i == fewer {
                    cut_depth
                } else {
                    start_depth + span * i as f64 / fewer as f64
                }
            })
            .collect());
    }
    Ok(even)
}

/// Pass depths as the pass editor sets them: a custom list when given;
/// otherwise by the tool's pass depth (see [`stepped_depths`]), or `count`
/// evenly spaced passes when given, and with a last pass exactly
/// `last_thickness` millimetres thick when given (the other passes are
/// spread over the rest of the depth).
///
/// # Errors
/// As [`pass_depths`], and when the number of passes is zero, the last
/// pass is not thinner than the whole cut, or a listed depth is not a
/// number.
pub fn planned_depths(
    start_depth: f64,
    cut_depth: f64,
    pass_depth: f64,
    plan: &PassPlan,
) -> Result<Vec<f64>, CoreError> {
    let mut depths = stepped_depths(start_depth, cut_depth, pass_depth, plan.exact)?;
    if let Some(list) = plan.list {
        if list.iter().any(|d| !d.is_finite()) {
            return Err(CoreError::validation("a pass depth is not a number"));
        }
        let mut custom: Vec<f64> = list
            .iter()
            .copied()
            .filter(|d| *d > start_depth + 1e-9 && *d < cut_depth - 1e-9)
            .collect();
        custom.sort_by(f64::total_cmp);
        custom.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        custom.push(cut_depth);
        return Ok(custom);
    }
    let count = plan.count;
    if count == Some(0) {
        return Err(CoreError::validation(
            "the number of passes must be at least one",
        ));
    }
    let last = match plan.last_thickness {
        Some(t) if t.is_finite() && t > 1e-9 => {
            if t >= cut_depth - start_depth - 1e-9 {
                return Err(CoreError::validation(
                    "the last pass must be thinner than the whole cut",
                ));
            }
            Some(t)
        }
        Some(t) if !t.is_finite() || t < 0.0 => {
            return Err(CoreError::validation(
                "the last pass thickness must be zero or more",
            ));
        }
        _ => None,
    };
    let above = cut_depth - last.unwrap_or(0.0);
    if let Some(n) = count {
        let n = if last.is_some() {
            n.saturating_sub(1).max(1)
        } else {
            n
        };
        let span = above - start_depth;
        depths = (1..=n)
            .map(|i| {
                if i == n {
                    above
                } else {
                    start_depth + span * i as f64 / n as f64
                }
            })
            .collect();
    } else if last.is_some() {
        depths = stepped_depths(start_depth, above, pass_depth, plan.exact)?;
    }
    if last.is_some() {
        depths.push(cut_depth);
    }
    Ok(depths)
}

/// Tabs (bridges) left in a profile so that parts stay in the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TabSpec {
    /// Whether tabs are added at all.
    pub enabled: bool,
    /// Tabs on each closed vector.
    pub count: usize,
    /// Length of each tab along the vector, in millimetres.
    pub length: f64,
    /// How much material each tab leaves above the cut depth.
    pub thickness: f64,
    /// Triangular (3D) tabs: the cut climbs to the tab's top at its middle
    /// and back down, instead of lifting square over the whole tab.
    pub triangular: bool,
    /// Tabs placed this many millimetres apart along each vector instead
    /// of a fixed number per vector.
    pub spacing: Option<f64>,
    /// Where Add Tabs puts the first tab and whether it keeps the tabs away
    /// from corners (the Edit Tabs dialog).
    pub placement: TabPlacement,
}

/// How the Edit Tabs dialog places tabs automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabPlacement {
    /// Evenly spread, the first half a spacing from the start.
    #[default]
    Even,
    /// The first tab at the machining start point.
    FirstAtStart,
    /// Evenly spread, each moved off corners and curved regions.
    AvoidCorners,
}

impl Default for TabSpec {
    fn default() -> Self {
        TabSpec {
            enabled: false,
            count: 4,
            length: 10.0,
            thickness: 3.0,
            triangular: false,
            spacing: None,
            placement: TabPlacement::Even,
        }
    }
}

impl TabSpec {
    /// Stretches of a closed chain of `length` millimetres the tabs cover,
    /// spread evenly with the first centred a half spacing in.
    pub fn intervals(&self, length: f64) -> Vec<(f64, f64)> {
        let count = match self.spacing {
            Some(apart) if apart.is_finite() && apart > 1e-9 => {
                ((length / apart).round() as usize).max(1)
            }
            _ => self.count,
        };
        if !self.enabled || count == 0 || self.length <= 0.0 || length <= 0.0 {
            return Vec::new();
        }
        let spacing = length / count as f64;
        let half = (self.length / 2.0).min(spacing / 2.0 - 1e-6).max(0.0);
        (0..count)
            .map(|i| {
                let centre = spacing * (i as f64 + 0.5);
                (centre - half, centre + half)
            })
            .collect()
    }
}

/// The shape of a ramp down into a cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RampKind {
    /// One slope along the cut, which is cut again flat at the end.
    #[default]
    Smooth,
    /// Forwards and back along the start of the cut, down half the step
    /// each way.
    ZigZag,
    /// Down over a whole lap of a closed cut, then a flat lap.
    Spiral,
}

/// How a cut gets down to depth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RampSpec {
    /// Ramp down along the cut instead of plunging straight down.
    pub enabled: bool,
    /// How far along the cut each ramp runs, in millimetres.
    pub distance: f64,
    /// Smooth, zig zag or spiral.
    pub kind: RampKind,
    /// Ramp at this angle below the horizontal, in degrees, instead of over
    /// a fixed distance: a smooth ramp's length then follows from each step
    /// down, and a zig zag runs back and forth over `distance` as often as
    /// the angle needs.
    pub angle: Option<f64>,
    /// With a lead in, ramp down along the lead instead of the cut.
    pub on_lead_in: bool,
}

impl Default for RampSpec {
    fn default() -> Self {
        RampSpec {
            enabled: false,
            distance: 20.0,
            kind: RampKind::Smooth,
            angle: None,
            on_lead_in: false,
        }
    }
}

impl RampSpec {
    /// The ramp angle in degrees when one is set and usable.
    fn usable_angle(&self) -> Option<f64> {
        self.angle
            .filter(|angle| angle.is_finite() && *angle > 1e-6 && *angle < 90.0)
    }

    /// How far along the cut a ramp down `drop` millimetres runs.
    pub fn distance_for(&self, drop: f64) -> f64 {
        match self.usable_angle() {
            Some(angle) => drop.abs() / angle.to_radians().tan(),
            None => self.distance,
        }
    }

    /// How many legs, an even number, a zig zag of `leg` millimetres takes
    /// to go down `drop` millimetres no steeper than the angle; two without
    /// an angle.
    pub fn zig_zag_legs(&self, drop: f64, leg: f64) -> usize {
        let Some(angle) = self.usable_angle() else {
            return 2;
        };
        let per_leg = leg * angle.to_radians().tan();
        if per_leg <= 1e-9 {
            return 2;
        }
        let legs = (drop.abs() / per_leg - 1e-9).ceil().max(1.0) as usize;
        (legs + legs % 2).clamp(2, 10_000)
    }
}

/// The shape of a lead into or out of a cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeadKind {
    /// Plunge straight onto the cut.
    #[default]
    None,
    /// A straight line at an angle to the cut.
    Straight,
    /// A circular arc that meets the cut tangentially (Circular Lead).
    Arc,
}

/// How a closed profile cut is entered and left: from the waste side along
/// a line or an arc, with an overcut past the start before leaving.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LeadSpec {
    /// None, straight or arc.
    pub kind: LeadKind,
    /// Length of a straight lead, in millimetres.
    pub length: f64,
    /// Angle of a straight lead to the cut's direction, in degrees: 0 runs
    /// along the tangent, 90 comes in square to the cut.
    pub angle_deg: f64,
    /// Radius of an arc lead, in millimetres.
    pub radius: f64,
    /// How far round an arc lead turns, in degrees, 0.1 to 90.
    pub arc_angle_deg: f64,
    /// How far past the start the loop is cut again before leaving, in
    /// millimetres.
    pub overcut: f64,
    /// Leave the cut along a lead too; without it the tool lifts straight
    /// off the end of the cut.
    pub lead_out: bool,
}

impl Default for LeadSpec {
    fn default() -> Self {
        LeadSpec {
            kind: LeadKind::None,
            length: 5.0,
            angle_deg: 45.0,
            radius: 5.0,
            arc_angle_deg: 90.0,
            overcut: 0.0,
            lead_out: true,
        }
    }
}

impl LeadSpec {
    /// Checks the values that are in use.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a lead length, radius or
    /// overcut that is negative or not a number.
    pub fn validate(&self) -> Result<(), CoreError> {
        let size_ok = match self.kind {
            LeadKind::None => true,
            LeadKind::Straight => {
                self.length.is_finite() && self.length >= 0.0 && self.angle_deg.is_finite()
            }
            LeadKind::Arc => {
                self.radius.is_finite()
                    && self.radius >= 0.0
                    && self.arc_angle_deg.is_finite()
                    && (0.1 - 1e-9..=90.0 + 1e-9).contains(&self.arc_angle_deg)
            }
        };
        if !size_ok {
            return Err(CoreError::validation(
                "the lead length and radius must be zero or more, and a circular lead's angle 0.1 to 90 degrees",
            ));
        }
        if self.is_active() && !(self.overcut.is_finite() && self.overcut >= 0.0) {
            return Err(CoreError::validation("the overcut must be zero or more"));
        }
        Ok(())
    }

    /// True when a lead is actually cut.
    pub fn is_active(&self) -> bool {
        match self.kind {
            LeadKind::None => false,
            LeadKind::Straight => self.length > 1e-9,
            LeadKind::Arc => self.radius > 1e-9,
        }
    }

    /// The lead into `at`, where the cut sets off along the unit vector
    /// `tangent`. `waste` is the unit normal pointing to the waste side.
    /// The lead starts on the waste side and its last span ends on `at`.
    pub fn lead_in(&self, at: Point, tangent: Vec2, waste: Vec2) -> Vec<CutSeg> {
        self.lead_out(at, tangent.scaled(-1.0), waste)
            .iter()
            .rev()
            .map(CutSeg::reversed)
            .collect()
    }

    /// The lead out of `at`, where the cut arrives along the unit vector
    /// `tangent`. `waste` is the unit normal pointing to the waste side.
    /// The lead mirrors [`LeadSpec::lead_in`] and starts on `at`.
    pub fn lead_out(&self, at: Point, tangent: Vec2, waste: Vec2) -> Vec<CutSeg> {
        match self.kind {
            LeadKind::None => Vec::new(),
            LeadKind::Straight if self.length > 1e-9 => {
                let angle = self.angle_deg.to_radians();
                let dir = tangent.scaled(angle.cos()).plus(waste.scaled(angle.sin()));
                vec![CutSeg {
                    from: at,
                    to: at.offset(dir.scaled(self.length)),
                    bulge: 0.0,
                }]
            }
            LeadKind::Arc if self.radius > 1e-9 => {
                // A turn of the lead's angle about a centre on the waste
                // side, leaving `at` along the tangent and curving away from
                // the cut.
                let r = self.radius;
                let sweep = self.arc_angle_deg.clamp(0.1, 90.0).to_radians();
                let centre = at.offset(waste.scaled(r));
                let to = centre.offset(
                    waste
                        .scaled(-sweep.cos() * r)
                        .plus(tangent.scaled(sweep.sin() * r)),
                );
                let turn = if tangent.cross(waste) > 0.0 {
                    1.0
                } else {
                    -1.0
                };
                vec![CutSeg {
                    from: at,
                    to,
                    bulge: bulge_from_sweep(turn * sweep),
                }]
            }
            _ => Vec::new(),
        }
    }
}

/// The unit direction of travel at the start of a span, or `None` for a
/// span with no length.
pub fn start_tangent(seg: &CutSeg) -> Option<Vec2> {
    match seg.arc() {
        Some(arc) => arc_tangent(arc.centre, seg.from, arc.sweep),
        None => seg.from.to(seg.to).normalised(),
    }
}

/// The unit direction of travel at the end of a span, or `None` for a span
/// with no length.
pub fn end_tangent(seg: &CutSeg) -> Option<Vec2> {
    match seg.arc() {
        Some(arc) => arc_tangent(arc.centre, seg.to, arc.sweep),
        None => seg.from.to(seg.to).normalised(),
    }
}

fn arc_tangent(centre: Point, at: Point, sweep: f64) -> Option<Vec2> {
    let radial = centre.to(at).normalised()?;
    Some(if sweep > 0.0 {
        radial.perp()
    } else {
        radial.perp().scaled(-1.0)
    })
}

/// Builds the move list of one toolpath, keeping track of where the tool is.
#[derive(Debug, Clone)]
pub struct Emitter {
    heights: Heights,
    at: Point3,
    moves: Vec<Move>,
}

impl Emitter {
    /// Starts at the home position.
    pub fn new(heights: Heights) -> Emitter {
        Emitter {
            heights,
            at: heights.home,
            moves: Vec::new(),
        }
    }

    /// Where the tool is now.
    pub fn at(&self) -> Point3 {
        self.at
    }

    /// The heights in use.
    pub fn heights(&self) -> Heights {
        self.heights
    }

    fn push(&mut self, m: Move) {
        if m.to != self.at || matches!(m.kind, super::toolpath::MoveKind::Dwell { .. }) {
            self.at = m.to;
            self.moves.push(m);
        }
    }

    /// Lifts to the clearance height if the tool is below it.
    pub fn retract(&mut self) {
        if self.at.z < self.heights.safe - 1e-9 {
            let to = Point3::new(self.at.x, self.at.y, self.heights.safe);
            self.push(Move::rapid(to));
        }
    }

    /// Rapids to above `p` at the clearance height, lifting first. From
    /// higher up (home) it moves across first, then down to clearance.
    pub fn travel_to(&mut self, p: Point) {
        self.retract();
        let z = self.at.z.max(self.heights.safe);
        self.push(Move::rapid(Point3::new(p.x, p.y, z)));
        self.push(Move::rapid(Point3::new(p.x, p.y, self.heights.safe)));
    }

    /// Goes straight down to `z` where the tool is: rapid to the plunge
    /// height when above it, then plunge. Moving up is a feed.
    pub fn plunge_to(&mut self, z: f64) {
        if z > self.at.z + 1e-9 {
            let to = Point3::new(self.at.x, self.at.y, z);
            self.push(Move::feed(to));
            return;
        }
        let from = self.heights.plunge_from.max(z);
        if self.at.z > from + 1e-9 {
            self.push(Move::rapid(Point3::new(self.at.x, self.at.y, from)));
        }
        self.push(Move::plunge(Point3::new(self.at.x, self.at.y, z)));
    }

    /// Rapids straight down to `z`, for going back into a hole that is
    /// already cut. Does nothing when the tool is not above `z`.
    pub fn rapid_down_to(&mut self, z: f64) {
        if self.at.z > z + 1e-9 {
            let to = Point3::new(self.at.x, self.at.y, z);
            self.push(Move::rapid(to));
        }
    }

    /// Writes moves made elsewhere, carrying on from where they end.
    pub fn extend(&mut self, moves: impl IntoIterator<Item = Move>) {
        for m in moves {
            self.push(m);
        }
    }

    /// How many moves have been written so far.
    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    /// From move `from` on, each level cut that ends on one of the corner
    /// points of `corners` runs on to that corner's target, `rise`
    /// millimetres higher, and straight back: the tip of a V-bit running up
    /// into an internal corner (Sharp Internal Corner).
    pub fn add_corner_runs(&mut self, from: usize, corners: &[(Point, Point)], rise: f64) {
        if corners.is_empty() || from >= self.moves.len() {
            return;
        }
        let tail = self.moves.split_off(from);
        let mut before = self.moves.last().map(|m| m.to);
        for m in tail {
            let level = before.is_some_and(|b| (b.z - m.to.z).abs() < 1e-9);
            let corner = (level && m.kind.is_cutting())
                .then(|| corners.iter().find(|(at, _)| at.distance(m.to.xy()) < 1e-6))
                .flatten();
            before = Some(m.to);
            self.moves.push(m);
            if let Some((_, target)) = corner {
                let back = m.to;
                self.moves
                    .push(Move::feed(Point3::new(target.x, target.y, back.z + rise)));
                self.moves.push(Move::feed(back));
            }
        }
    }

    /// Waits in place.
    pub fn dwell(&mut self, seconds: f64) {
        if seconds > 0.0 {
            let at = self.at;
            self.push(Move::dwell(seconds, at));
        }
    }

    /// Cuts one span, ending at height `z_end` (a helix or slope when that
    /// differs from the current height).
    pub fn cut(&mut self, seg: &CutSeg, z_end: f64) {
        let to = Point3::new(seg.to.x, seg.to.y, z_end);
        match seg.arc() {
            Some(arc) if arc.sweep > 0.0 => self.push(Move::arc_ccw(arc.centre, to)),
            Some(arc) => self.push(Move::arc_cw(arc.centre, to)),
            None => self.push(Move::feed(to)),
        }
    }

    /// Cuts spans at a steady height.
    pub fn cut_flat(&mut self, segs: &[CutSeg], z: f64) {
        for seg in segs {
            self.cut(seg, z);
        }
    }

    /// Cuts spans while going from the current height to `z` evenly over
    /// their length.
    pub fn cut_sloped(&mut self, segs: &[CutSeg], z: f64) {
        let total: f64 = segs.iter().map(CutSeg::length).sum();
        let from = self.at.z;
        let mut done = 0.0;
        for seg in segs {
            done += seg.length();
            let t = if total > 0.0 { done / total } else { 1.0 };
            self.cut(seg, from + (z - from) * t);
        }
    }

    /// Cuts a whole chain at height `z`, going up over tabs to `tab_top`
    /// where the chain passes under them.
    pub fn cut_chain(&mut self, chain: &Chain, z: f64, tabs: &[(f64, f64)], tab_top: f64) {
        self.cut_chain_tabbed(chain, z, tabs, tab_top, false);
    }

    /// As [`Emitter::cut_chain`]; with `triangular` the cut climbs to
    /// `tab_top` at each tab's middle and straight back down to `z` at its
    /// end, leaving a ridge shaped tab.
    pub fn cut_chain_tabbed(
        &mut self,
        chain: &Chain,
        z: f64,
        tabs: &[(f64, f64)],
        tab_top: f64,
        triangular: bool,
    ) {
        if triangular {
            if tabs.is_empty() || z >= tab_top - 1e-9 {
                self.cut_flat(&chain.segs, z);
                return;
            }
            let cuts: Vec<f64> = tabs
                .iter()
                .flat_map(|&(a, b)| [a, (a + b) / 2.0, b])
                .collect();
            for (index, piece) in chain.split_at(&cuts).iter().enumerate() {
                if piece.is_empty() {
                    continue;
                }
                match index % 3 {
                    1 => self.cut_sloped(piece, tab_top),
                    2 => self.cut_sloped(piece, z),
                    _ => self.cut_flat(piece, z),
                }
            }
            return;
        }
        if tabs.is_empty() || z >= tab_top - 1e-9 {
            self.cut_flat(&chain.segs, z);
            return;
        }
        let cuts: Vec<f64> = tabs.iter().flat_map(|&(a, b)| [a, b]).collect();
        for (index, piece) in chain.split_at(&cuts).iter().enumerate() {
            if piece.is_empty() {
                continue;
            }
            // Odd pieces lie between a tab's start and end.
            if index % 2 == 1 {
                self.plunge_to(tab_top);
                self.cut_flat(piece, tab_top);
            } else {
                self.plunge_to(z);
                self.cut_flat(piece, z);
            }
        }
        self.plunge_to(z);
    }

    /// Cuts a whole chain at height `z`, getting down from the current
    /// height the way `ramp` asks: a smooth slope, a zig zag over the start
    /// of the chain, or a spiral lap for closed chains (open chains always
    /// zig zag, since they cannot be recut from the end).
    pub fn cut_chain_ramp(&mut self, chain: &Chain, z: f64, ramp: &RampSpec) {
        let drop = self.at.z - z;
        if drop <= 1e-9 || chain.is_empty() {
            self.plunge_to(z);
            self.cut_flat(&chain.segs, z);
            return;
        }
        let distance = ramp.distance_for(drop);
        let kind = if chain.closed {
            ramp.kind
        } else {
            RampKind::ZigZag
        };
        match kind {
            RampKind::Smooth => self.cut_chain_ramped(chain, z, distance),
            RampKind::Spiral => {
                self.cut_sloped(&chain.segs, z);
                self.cut_flat(&chain.segs, z);
            }
            RampKind::ZigZag => {
                // By angle the zig zag keeps to its distance and goes back
                // and forth as often as the angle needs; by distance it goes
                // out half the distance and back once.
                let reach = match ramp.usable_angle() {
                    Some(_) => ramp.distance.min(chain.length()),
                    None => distance.min(chain.length()) / 2.0,
                };
                if !reach.is_finite() || reach <= 1e-9 {
                    self.plunge_to(z);
                    self.cut_flat(&chain.segs, z);
                    return;
                }
                let legs = ramp.zig_zag_legs(drop, reach);
                let pieces = chain.split_at(&[reach]);
                let slope = &pieces[0];
                let back: Vec<CutSeg> = slope.iter().rev().map(CutSeg::reversed).collect();
                let top = self.at.z;
                for leg in 1..=legs {
                    let to = top - drop * leg as f64 / legs as f64;
                    let segs = if leg % 2 == 1 { slope } else { &back };
                    self.cut_sloped(segs, if leg == legs { z } else { to });
                }
                self.cut_flat(&chain.segs, z);
            }
        }
    }

    /// Goes back along a closed chain to its start at the current height
    /// when the tool stands part way along it, as it does after a smooth
    /// ramp recut its slope, so the next pass starts where its tabs and
    /// ramp expect. The way back runs in the cut just made.
    pub fn back_to_start(&mut self, chain: &Chain) {
        let (Some(begin), true) = (chain.start(), chain.closed) else {
            return;
        };
        let here = self.at.xy();
        if here.distance(begin) <= 1e-6 {
            return;
        }
        let (at, along) = along_nearest(chain, here);
        let z = self.at.z;
        if at <= 1e-9 || at >= along - 1e-9 {
            self.cut(
                &CutSeg {
                    from: here,
                    to: begin,
                    bulge: 0.0,
                },
                z,
            );
            return;
        }
        let head = chain.split_at(&[at]).into_iter().next().unwrap_or_default();
        let back: Vec<CutSeg> = head.iter().rev().map(CutSeg::reversed).collect();
        self.cut_flat(&back, z);
    }

    /// Cuts a closed chain at height `z` starting from the current height
    /// over the first `ramp` millimetres, then the rest, then the ramped
    /// stretch again flat so that it is cut to depth too.
    pub fn cut_chain_ramped(&mut self, chain: &Chain, z: f64, ramp: f64) {
        let length = chain.length();
        let ramp = ramp.min(length);
        if ramp <= 1e-9 {
            self.plunge_to(z);
            self.cut_flat(&chain.segs, z);
            return;
        }
        let pieces = chain.split_at(&[ramp]);
        let (slope, rest) = (&pieces[0], &pieces[1]);
        self.cut_sloped(slope, z);
        self.cut_flat(rest, z);
        if chain.closed {
            self.cut_flat(slope, z);
        }
    }

    /// Lifts clear and goes home, then answers the moves.
    pub fn finish(mut self) -> Vec<Move> {
        self.retract();
        let home = self.heights.home;
        let high = self.at.z.max(home.z);
        self.push(Move::rapid(Point3::new(self.at.x, self.at.y, high)));
        self.push(Move::rapid(Point3::new(home.x, home.y, high)));
        self.push(Move::rapid(home));
        self.moves
    }
}

/// The separate areas of a region: each outer loop with the holes directly
/// inside it (those whose smallest surrounding outer loop it is).
pub fn region_parts(
    region: &crate::geom::Region,
    tol: crate::geom::Tolerance,
) -> Vec<crate::geom::Region> {
    use crate::geom::{point_in_path, subpath_signed_area, FillRule};
    let outers: Vec<&crate::geom::SubPath> = region.outers().collect();
    let areas: Vec<f64> = outers
        .iter()
        .map(|s| subpath_signed_area(s).abs())
        .collect();
    let alone: Vec<crate::geom::Path> = outers.iter().map(|s| (*s).clone().into_path()).collect();
    let mut parts: Vec<crate::geom::Region> = outers
        .iter()
        .map(|s| crate::geom::Region {
            loops: vec![(*s).clone()],
        })
        .collect();
    for hole in region.holes() {
        let owner = alone
            .iter()
            .enumerate()
            .filter(|(_, p)| point_in_path(p, hole.start, FillRule::EvenOdd, tol))
            .min_by(|a, b| areas[a.0].total_cmp(&areas[b.0]))
            .map(|(i, _)| i);
        match owner {
            Some(i) => parts[i].loops.push(hole.clone()),
            None => parts.push(crate::geom::Region {
                loops: vec![hole.clone()],
            }),
        }
    }
    parts
}

/// A closed chain restarted at the point along it nearest `p`, splitting a
/// span there when that point is not a node. Open chains are unchanged.
pub fn restart_nearest(chain: &Chain, p: Point) -> Chain {
    if !chain.closed || chain.segs.is_empty() {
        return chain.clone();
    }
    let (at, along) = along_nearest(chain, p);
    if at <= 1e-9 || at >= along - 1e-9 {
        return chain.clone();
    }
    let mut pieces = chain.split_at(&[at]).into_iter();
    let head = pieces.next().unwrap_or_default();
    let mut segs = pieces.next().unwrap_or_default();
    segs.extend(head);
    Chain { segs, closed: true }
}

/// How far along a chain the point nearest `p` lies, with the chain's
/// whole length.
fn along_nearest(chain: &Chain, p: Point) -> (f64, f64) {
    let mut best = (f64::MAX, 0.0);
    let mut along = 0.0;
    for seg in &chain.segs {
        let length = seg.length();
        let d = match seg.arc() {
            None => {
                let v = seg.from.to(seg.to);
                let len2 = v.x * v.x + v.y * v.y;
                if len2 <= 1e-18 {
                    0.0
                } else {
                    let w = seg.from.to(p);
                    ((w.x * v.x + w.y * v.y) / len2).clamp(0.0, 1.0) * length
                }
            }
            Some(_) => {
                let steps = 32;
                (0..=steps)
                    .map(|i| length * i as f64 / steps as f64)
                    .min_by(|a, b| {
                        seg.point_at(*a)
                            .distance_squared(p)
                            .total_cmp(&seg.point_at(*b).distance_squared(p))
                    })
                    .unwrap_or(0.0)
            }
        };
        let gap = seg.point_at(d).distance_squared(p);
        if gap < best.0 - 1e-12 {
            best = (gap, along + d);
        }
        along += length;
    }
    (best.1, along)
}

/// Replaces the rounded corners an offset makes (arcs of radius `radius`
/// turning less than a half turn) with the two straight spans meeting at a
/// point, so that the tool runs out into sharp corners.
pub fn sharpen_corners(chain: &Chain, radius: f64) -> Chain {
    if radius <= 1e-9 {
        return chain.clone();
    }
    let mut segs = Vec::with_capacity(chain.segs.len() + 4);
    for seg in &chain.segs {
        let sharp = seg.arc().and_then(|arc| {
            if (arc.radius - radius).abs() > 1e-3 * radius.max(1.0)
                || arc.sweep.abs() >= std::f64::consts::PI * 0.95
            {
                return None;
            }
            let t0 = start_tangent(seg)?;
            let t1 = end_tangent(seg)?;
            // Tangent lines meet a radius times tan(sweep / 2) along each.
            let reach = arc.radius * (arc.sweep.abs() / 2.0).tan();
            let corner = seg.from.offset(t0.scaled(reach));
            let check = seg.to.offset(t1.scaled(-reach));
            (corner.distance(check) < 1e-6 * radius.max(1.0) + 1e-6).then_some(corner)
        });
        match sharp {
            Some(corner) => {
                segs.push(CutSeg {
                    from: seg.from,
                    to: corner,
                    bulge: 0.0,
                });
                segs.push(CutSeg {
                    from: corner,
                    to: seg.to,
                    bulge: 0.0,
                });
            }
            None => segs.push(*seg),
        }
    }
    Chain {
        segs,
        closed: chain.closed,
    }
}

/// Orders items nearest first, starting from `from`: each next item is the
/// one whose start is closest to where the previous one ended.
pub fn order_nearest<T, S, E>(items: Vec<T>, from: Point, start: S, end: E) -> Vec<T>
where
    S: Fn(&T) -> Point,
    E: Fn(&T) -> Point,
{
    let mut left: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(left.len());
    let mut at = from;
    loop {
        let best = left
            .iter()
            .enumerate()
            .filter_map(|(i, item)| item.as_ref().map(|t| (i, start(t).distance_squared(at))))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let Some((index, _)) = best else {
            break;
        };
        if let Some(item) = left[index].take() {
            at = end(&item);
            out.push(item);
        }
    }
    out
}

/// Orders items nearest first like [`order_nearest`], but an item may be
/// entered from either end: `reverse` turns one round when its far end is
/// the nearer, as open cuts can be cut either way.
pub fn order_nearest_either_way<T, S, E, R>(
    items: Vec<T>,
    from: Point,
    start: S,
    end: E,
    reverse: R,
) -> Vec<T>
where
    S: Fn(&T) -> Point,
    E: Fn(&T) -> Point,
    R: Fn(T) -> T,
{
    let mut left: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(left.len());
    let mut at = from;
    loop {
        let best = left
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                item.as_ref().map(|t| {
                    let forward = start(t).distance_squared(at);
                    let backward = end(t).distance_squared(at);
                    (i, forward.min(backward), backward < forward)
                })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let Some((index, _, flip)) = best else {
            break;
        };
        if let Some(item) = left[index].take() {
            let item = if flip { reverse(item) } else { item };
            at = end(&item);
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::{rect, Tolerance};

    fn heights() -> Heights {
        Heights::from_job(&JobSetup::default())
    }

    fn square() -> Chain {
        let path = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        Chain::from_subpath(&path.subpaths[0], Tolerance::new(1e-3).expect("tol"))
    }

    #[test]
    fn the_pass_depth_grows_by_up_to_fifteen_percent_to_save_a_pass() {
        // 11 mm at 5 mm needs 3 passes; at 5.5 mm (10% deeper) 2 do.
        assert_eq!(
            stepped_depths(0.0, 11.0, 5.0, false).expect("passes"),
            vec![5.5, 11.0]
        );
        // 12 mm would need 6 mm passes (20% deeper): three even passes.
        assert_eq!(
            stepped_depths(0.0, 12.0, 5.0, false).expect("passes"),
            vec![4.0, 8.0, 12.0]
        );
        // Keeping the exact depth cuts 5 mm passes and a thin last one.
        assert_eq!(
            stepped_depths(0.0, 11.0, 5.0, true).expect("passes"),
            vec![5.0, 10.0, 11.0]
        );
        assert_eq!(
            stepped_depths(1.0, 11.0, 5.0, true).expect("passes"),
            vec![6.0, 11.0]
        );
    }

    #[test]
    fn a_custom_pass_list_wins_and_ends_on_the_cut_depth() {
        let list = [7.0, 2.0, 2.0, 15.0, -1.0];
        let plan = PassPlan {
            count: Some(9),
            list: Some(&list),
            ..PassPlan::default()
        };
        assert_eq!(
            planned_depths(0.0, 10.0, 3.0, &plan).expect("passes"),
            vec![2.0, 7.0, 10.0]
        );
        let empty = PassPlan {
            list: Some(&[]),
            ..PassPlan::default()
        };
        assert_eq!(
            planned_depths(0.0, 10.0, 3.0, &empty).expect("passes"),
            vec![10.0]
        );
        let bad = [f64::NAN];
        let plan = PassPlan {
            list: Some(&bad),
            ..PassPlan::default()
        };
        assert!(planned_depths(0.0, 10.0, 3.0, &plan).is_err());
    }

    #[test]
    fn passes_are_spread_evenly_and_end_on_the_cut_depth() {
        assert_eq!(
            pass_depths(0.0, 10.0, 6.0).expect("passes"),
            vec![5.0, 10.0]
        );
        assert_eq!(pass_depths(0.0, 6.0, 6.0).expect("passes"), vec![6.0]);
        assert_eq!(pass_depths(2.0, 5.0, 10.0).expect("passes"), vec![5.0]);
        assert!(pass_depths(5.0, 5.0, 1.0).is_err());
        assert!(pass_depths(0.0, 5.0, 0.0).is_err());
    }

    #[test]
    fn heights_follow_the_z_zero_face() {
        let mut job = JobSetup::default();
        assert_eq!(Heights::from_job(&job).z_at(3.0), -3.0);
        job.z_zero = crate::doc::ZZero::Bottom;
        let h = Heights::from_job(&job);
        assert_eq!(h.surface, job.thickness);
        assert_eq!(h.z_at(3.0), job.thickness - 3.0);
        assert_eq!(h.safe, job.thickness + job.machine.safe_z);
    }

    #[test]
    fn a_plunge_rapids_to_the_plunge_height_first() {
        let mut out = Emitter::new(heights());
        out.travel_to(Point::new(5.0, 5.0));
        out.plunge_to(-3.0);
        let moves = out.finish();
        let plunge = moves
            .iter()
            .position(|m| m.kind == MoveKind::Plunge)
            .expect("a plunge");
        assert_eq!(
            moves[plunge - 1].to.z,
            JobSetup::default().machine.plunge_gap
        );
        assert_eq!(moves[plunge].to.z, -3.0);
        let last = moves.last().expect("moves");
        assert_eq!(last.to, heights().home);
    }

    #[test]
    fn tabs_lift_the_cut_over_their_stretches_only() {
        let tabs = TabSpec {
            enabled: true,
            count: 2,
            length: 4.0,
            thickness: 2.0,
            ..TabSpec::default()
        };
        let intervals = tabs.intervals(40.0);
        assert_eq!(intervals, vec![(8.0, 12.0), (28.0, 32.0)]);
        let mut out = Emitter::new(heights());
        out.travel_to(Point::ORIGIN);
        out.plunge_to(-10.0);
        out.cut_chain(&square(), -10.0, &intervals, -8.0);
        let moves = out.finish();
        let at_tab = moves.iter().filter(|m| m.to.z == -8.0).count();
        assert!(at_tab >= 4, "{moves:?}");
        assert!(moves.iter().all(|m| m.to.z >= -10.0));
    }

    #[test]
    fn a_ramp_reaches_depth_along_the_cut_and_recuts_the_slope() {
        let mut out = Emitter::new(heights());
        out.travel_to(Point::ORIGIN);
        out.plunge_to(0.0);
        out.cut_chain_ramped(&square(), -4.0, 15.0);
        let moves = out.finish();
        let cutting: Vec<&Move> = moves.iter().filter(|m| m.kind.is_cutting()).collect();
        assert!(
            cutting.iter().any(|m| m.to.z < 0.0 && m.to.z > -4.0),
            "a sloped move"
        );
        let at_depth: f64 = cutting.iter().filter(|m| m.to.z == -4.0).count() as f64;
        assert!(at_depth >= 4.0);
        assert!(!cutting
            .iter()
            .any(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0));
    }

    #[test]
    fn open_items_are_entered_from_their_nearer_end() {
        let lines = vec![
            (Point::new(10.0, 0.0), Point::new(2.0, 0.0)),
            (Point::new(20.0, 0.0), Point::new(30.0, 0.0)),
        ];
        let ordered =
            order_nearest_either_way(lines, Point::ORIGIN, |l| l.0, |l| l.1, |l| (l.1, l.0));
        assert_eq!(ordered[0], (Point::new(2.0, 0.0), Point::new(10.0, 0.0)));
        assert_eq!(ordered[1], (Point::new(20.0, 0.0), Point::new(30.0, 0.0)));
    }

    #[test]
    fn nearest_ordering_hops_to_the_closest_start() {
        let points = vec![
            Point::new(10.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(5.0, 0.0),
        ];
        let ordered = order_nearest(points, Point::ORIGIN, |p| *p, |p| *p);
        assert_eq!(
            ordered,
            vec![
                Point::new(1.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(10.0, 0.0)
            ]
        );
    }
}
