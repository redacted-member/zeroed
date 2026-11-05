//! The geometry of each kind of joint: what it adds to and cuts from the
//! two parts, the vectors it makes and how the parts go together.
//!
//! Sizes are worked out in the edge frames (`s` along the edge, `t` out of
//! the part). The drawn outlines are the finished outer sizes: notches and
//! slots are cut into them, tabs stand out of them, and every cavity is
//! widened by the fit's clearance on each side while depths stay true, so
//! faces come out flush.

use std::collections::BTreeMap;

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::Path;

use super::geometry::{
    add, arcs_path, cavity_reliefs, chord, cross2, disc, dot, inside_rings, length, rewrite, scale,
    sub, unit, with_circles, Circle, Frame, Outline, ReliefShape, P2,
};
use super::pose::Map;
use super::{
    edge_frame, part_rings, HingePattern, Joint, JointKind, JointParams, JointSettings, Role,
    Start, Target,
};
use crate::assembly::pose::Pose;

/// A joint's cut runs this far past an edge so it leaves no sliver.
pub const OVERRUN: f64 = 0.5;

/// Which part a made vector belongs with (its layer and sheet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    A,
    B,
}

/// A vector a joint makes.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub on: On,
    pub path: Path,
    pub role: Role,
    /// Last word of its name ("slot 2").
    pub name: String,
}

/// What a joint does.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Part A's new outline, or `None` when it is unchanged.
    pub a: Option<Outline>,
    /// Part B's.
    pub b: Option<Outline>,
    /// Vectors made.
    pub vectors: Vec<Made>,
    /// The sizes used.
    pub resolved: JointParams,
    /// Plain notes.
    pub notes: Vec<String>,
    /// B's edge frame coordinates to A's, assembled.
    pub map: Map,
    /// The edge frames.
    pub fa: Frame,
    pub fb: Frame,
    /// Thicknesses.
    pub ta: f64,
    pub tb: f64,
    /// Where each extra part (by its index in `vectors`) goes once the
    /// joint is together: its part space as drawn to A's.
    pub seats: Vec<(usize, Pose)>,
}

/// Everything a kind needs to lay out its joint.
pub struct Ctx {
    pub fa: Frame,
    pub fb: Frame,
    /// Current outline rings.
    pub ra: Vec<Vec<P2>>,
    pub rb: Vec<Vec<P2>>,
    pub ta: f64,
    pub tb: f64,
    /// Clearance per side.
    pub c: f64,
    /// Tool radius.
    pub r: f64,
    pub relief: ReliefShape,
    /// For a face target: from B's edge to A's near face, and from the
    /// edge's start to A's middle.
    pub face: Option<(f64, f64)>,
    /// A turned half round (see [`tee_map`]).
    pub flip: bool,
}

impl Ctx {
    fn tool(&self) -> f64 {
        2.0 * self.r
    }

    /// How far a pocket runs out past an open edge, so the round tool
    /// clears right to the edge.
    fn run_out(&self) -> f64 {
        self.r + OVERRUN
    }

    /// Where A stands on B's face: offset in from B's edge and A's middle
    /// along it.
    fn face(&self) -> (f64, f64) {
        self.face.unwrap_or((0.0, self.fb.len / 2.0))
    }

    /// Where along B's edge a point of A's edge line at `s` lands when A
    /// stands on B: A's edge runs the other way along B's, or the same way
    /// when A is turned (see [`tee_map`]).
    fn b_along(&self, s: f64) -> f64 {
        let (_, along) = self.face();
        if self.flip {
            along - self.fa.len / 2.0 + s
        } else {
            along + self.fa.len / 2.0 - s
        }
    }

    /// Where along B's edge the stretch `s0` to `s1` of A's edge lands,
    /// the smaller end first.
    fn b_span(&self, s0: f64, s1: f64) -> (f64, f64) {
        let (p, q) = (self.b_along(s0), self.b_along(s1));
        (p.min(q), p.max(q))
    }
}

/// The edges of a corner joint must be the same length.
fn same_length(ctx: &Ctx) -> Result<(), CoreError> {
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    if (la - lb).abs() > 0.5 {
        return Err(CoreError::validation(format!(
            "these edges are {} and {} mm long; pick edges of the same length",
            fmt(la),
            fmt(lb)
        )));
    }
    Ok(())
}

/// A depth from the top of a part `thick` thick: the one asked for or
/// `default`, checked.
fn depth_in(asked: Option<f64>, default: f64, thick: f64, what: &str) -> Result<f64, CoreError> {
    let depth = asked.unwrap_or(default);
    if !(depth > 0.0 && depth < thick) {
        return Err(CoreError::validation(format!(
            "the {what} depth must be more than 0 and less than the part's thickness ({} mm)",
            fmt(thick)
        )));
    }
    Ok(depth)
}

/// The map of a corner joint: B stands on A's edge, B's outer face flush
/// with A's edge line, B's inside face towards A's top.
pub(crate) fn corner_map(la: f64) -> Map {
    Map::axes([(0, -1.0), (2, -1.0), (1, -1.0)], [la, 0.0, 0.0])
}

/// The map of a T joint: A stands on B's top face along the line `offset`
/// in from B's edge, its middle `along` it, its edge `depth` below B's top.
/// A's underside is its near face and its top faces away from B's edge,
/// into B: two legs set in from the two ends of a top face each other, so
/// what stands on a leg's top face (a shelf in a dado, a stretcher) is
/// between them. A's edge runs the other way along B's. Turned (`flip`),
/// A is half round about its own middle: its top faces B's edge and its
/// edge runs along B's the same way.
fn tee_map(ctx: &Ctx, depth: f64) -> Map {
    let (offset, along) = ctx.face();
    tee_map_at(ctx.fa.len, ctx.ta, ctx.tb, depth, offset, along, ctx.flip)
}

/// [`tee_map`] from its sizes: A's edge `la` long, the thicknesses.
pub(crate) fn tee_map_at(
    la: f64,
    ta: f64,
    tb: f64,
    depth: f64,
    offset: f64,
    along: f64,
    flip: bool,
) -> Map {
    if flip {
        Map::axes(
            [(0, 1.0), (2, 1.0), (1, -1.0)],
            [la / 2.0 - along, tb - depth, offset + ta],
        )
    } else {
        Map::axes(
            [(0, -1.0), (2, -1.0), (1, -1.0)],
            [la / 2.0 + along, tb - depth, -offset],
        )
    }
}

fn fmt(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    format!("{rounded}")
}

/// Lays out `joint` on the parts as they are now.
///
/// # Errors
/// [`CoreError::Validation`] in plain words saying what to change.
pub fn plan(
    document: &Document,
    originals: &BTreeMap<u64, Path>,
    settings: &JointSettings,
    joint: &Joint,
) -> Result<Plan, CoreError> {
    let b_edge = joint.b.edge();
    if joint.a.vector == b_edge.vector && !joint.kind.one_part() {
        return Err(CoreError::validation(
            "both picks are on the same part; pick an edge of another part",
        ));
    }
    let face = match joint.b {
        Target::Face { offset, along, .. } => Some((offset, along)),
        Target::Edge(_) => None,
    };
    // Dowels also join two edges, drilled by hand at marks.
    let edge_dowels = joint.kind == JointKind::Dowel && face.is_none();
    if joint.kind.on_face() != face.is_some() && !edge_dowels {
        return Err(CoreError::validation(if joint.kind.on_face() {
            "this joint needs a place on the second part's face; click inside the second part"
        } else {
            "this joint needs an edge of the second part; click on its edge"
        }));
    }
    let sheet_of = |v: u64| document.entity(v).map(|e| e.sheet).unwrap_or(0);
    let ctx = Ctx {
        fa: edge_frame(document, originals, &joint.a)?,
        fb: edge_frame(document, originals, &b_edge)?,
        ra: part_rings(document, joint.a.vector)?,
        rb: part_rings(document, b_edge.vector)?,
        ta: settings.thickness_of(document, sheet_of(joint.a.vector)),
        tb: settings.thickness_of(document, sheet_of(b_edge.vector)),
        c: settings.clearance(),
        r: settings.tool_diameter / 2.0,
        relief: settings.relief.shape(),
        face,
        flip: joint.flip,
    };
    let p = &joint.params;
    let plan = match joint.kind {
        JointKind::Finger => finger(&ctx, p)?,
        JointKind::Dado => dado(&ctx, p, false)?,
        JointKind::Rabbet => rabbet(&ctx, p)?,
        JointKind::TabSlot => tabs(&ctx, p, false)?,
        JointKind::MortiseTenon => tabs(&ctx, p, true)?,
        JointKind::CrossLap => cross_lap(&ctx, p)?,
        JointKind::WedgedTenon => wedged(&ctx, p)?,
        JointKind::Dovetail => dovetail(&ctx, p)?,
        JointKind::Dowel => dowels(&ctx, p)?,
        JointKind::CaptiveNut => captive_nut(&ctx, p)?,
        JointKind::Butterfly => butterfly(&ctx, p)?,
        JointKind::Puzzle => puzzle(&ctx, p)?,
        JointKind::LivingHinge => hinge(&ctx, p)?,
        JointKind::HalfLap => half_lap(&ctx, p)?,
        JointKind::VJoint => v_joint(&ctx, p)?,
        JointKind::TwistLock => twist_lock(&ctx, p)?,
        JointKind::CornerDovetail => corner_dovetail(&ctx, p)?,
        JointKind::KerfTenon => kerf_tenon(&ctx, p)?,
        JointKind::CamLock => cam_lock(&ctx, p)?,
        JointKind::Groove => dado(&ctx, p, true)?,
        JointKind::Screws => screws(&ctx, p)?,
    };
    Ok(plan)
}

fn base_plan(ctx: &Ctx, resolved: JointParams, map: Map) -> Plan {
    Plan {
        a: None,
        b: None,
        vectors: Vec::new(),
        resolved,
        notes: Vec::new(),
        map,
        fa: ctx.fa,
        fb: ctx.fb,
        ta: ctx.ta,
        tb: ctx.tb,
        seats: Vec::new(),
    }
}

/// The nearest odd whole number to `x`, at least `min`.
fn odd(x: f64, min: u32) -> u32 {
    let n = x.round().max(1.0) as u32;
    let n = if n % 2 == 0 { n + 1 } else { n };
    n.max(min)
}

/// The ends of `n` segments over `len`, the first `1 + bias` times as wide.
fn segments(len: f64, n: u32, bias: f64) -> Vec<f64> {
    let w = len / (f64::from(n) + bias);
    let mut ends = vec![0.0];
    for i in 0..n {
        let width = if i == 0 { w * (1.0 + bias) } else { w };
        let last = ends[ends.len() - 1];
        ends.push(if i + 1 == n { len } else { last + width });
    }
    ends
}

/// Box joint: two edges meeting at a corner, alternating fingers.
fn finger(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    finger_with(ctx, p, None)
}

/// How the fingers of a V joint go: how deep A's and B's notches are and
/// where B sits.
struct Angled {
    notch_a: f64,
    notch_b: f64,
    map: Map,
}

/// Fingers on two edges meeting at a corner; `angled` for panels at an
/// angle other than square (no blind fingers then).
fn finger_with(ctx: &Ctx, p: &JointParams, angled: Option<&Angled>) -> Result<Plan, CoreError> {
    same_length(ctx)?;
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let thick = ctx.ta.max(ctx.tb);
    let bias = p.bias.unwrap_or(0.0);
    if !(0.0..=0.5).contains(&bias) {
        return Err(CoreError::validation(
            "the bias must be between 0 and 0.5 (half a finger wider)",
        ));
    }
    let count = match (p.count, p.width) {
        (Some(n), _) => n,
        (None, Some(w)) if w > 0.0 => odd(la / w, 3),
        _ => odd(la / (2.0 * thick), 3),
    };
    if count < 2 {
        return Err(CoreError::validation(
            "a finger joint needs at least 2 fingers",
        ));
    }
    let start = p.start.unwrap_or(Start::Finger);
    let blind = angled.is_none() && p.blind.unwrap_or(false);
    let (notch_a, notch_b) = angled.map_or((ctx.tb, ctx.ta), |v| (v.notch_a, v.notch_b));
    let depth = p.depth.unwrap_or(ctx.tb * 2.0 / 3.0);
    if blind && !(depth > 0.0 && depth < ctx.tb) {
        return Err(CoreError::validation(format!(
            "a blind finger's depth must be more than 0 and less than the second part's thickness ({} mm)",
            fmt(ctx.tb)
        )));
    }
    let ends = segments(la, count, bias);
    let c = ctx.c;
    let narrowest = ends
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(f64::INFINITY, f64::min);
    if narrowest + c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the fingers ({} mm) are narrower than the tool ({} mm); use fewer fingers or a smaller tool",
            fmt(narrowest),
            fmt(ctx.tool())
        )));
    }
    let a_has = |i: u32| (i % 2 == 0) == (start == Start::Finger);
    // Both parts have cavities here, so each takes half the clearance: a
    // cut spanning segment i is widened by half of it on each side (the
    // faces end up the clearance apart) and runs out past the edge's ends.
    let half = c / 2.0;
    let span = |i: usize, len: f64, ends: &[f64]| -> (f64, f64) {
        let s0 = if i == 0 { -OVERRUN } else { ends[i] - half };
        let s1 = if i + 1 == ends.len() - 1 {
            len + OVERRUN
        } else {
            ends[i + 1] + half
        };
        (s0, s1)
    };
    // B's segments run the other way along its edge.
    let b_ends: Vec<f64> = ends.iter().rev().map(|s| lb * (1.0 - s / la)).collect();
    let mut a_cut = Vec::new();
    let mut b_cut = Vec::new();
    let mut pockets = Vec::new();
    for i in 0..count as usize {
        let (s0, s1) = span(i, la, &ends);
        // The same segment on B, counted from B's start.
        let j = count as usize - 1 - i;
        let (t0, t1) = span(j, lb, &b_ends);
        if a_has(i as u32) {
            if blind {
                // A's finger stops inside B; B gets a pocket for it.
                a_cut.push(ctx.fa.rect(s0, s1, -(ctx.tb - depth), OVERRUN));
                pockets.push(ctx.fb.rect(t0, t1, -(ctx.ta + c), ctx.run_out()));
            } else {
                b_cut.push(ctx.fb.rect(t0, t1, -notch_b, OVERRUN));
            }
        } else {
            a_cut.push(ctx.fa.rect(s0, s1, -notch_a, OVERRUN));
        }
    }
    let resolved = JointParams {
        count: Some(count),
        width: Some(la / (f64::from(count) + bias)),
        start: Some(start),
        bias: Some(bias),
        blind: Some(blind),
        depth: blind.then_some(depth),
        ..JointParams::default()
    };
    let map = angled.map_or_else(|| corner_map(la), |v| v.map);
    let mut plan = base_plan(ctx, resolved, map);
    plan.a = Some(rewrite(
        &ctx.ra,
        &[],
        &a_cut,
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?);
    if !b_cut.is_empty() {
        plan.b = Some(rewrite(
            &ctx.rb,
            &[],
            &b_cut,
            ctx.r,
            ctx.relief,
            ctx.fb.n,
            "the second part",
        )?);
    }
    for (k, ring) in pockets.into_iter().enumerate() {
        let ring = inside_relieved(ring, ctx, &ctx.rb, ctx.fb.n);
        plan.vectors.push(Made {
            on: On::B,
            path: ring,
            role: Role::Pocket { depth },
            name: format!("pocket {}", k + 1),
        });
    }
    plan.notes.push(format!(
        "{count} fingers about {} mm wide",
        fmt(la / f64::from(count))
    ));
    Ok(plan)
}

/// Housing: a pocket in B's face along the line where A stands, A's
/// thickness plus the clearance on each side wide. Through, it runs out
/// past both ends of B; stopped, it ends `stop` short of the far end and
/// A gets a notch to pass the stop.
/// How much wider than the tool a groove for stock thinner than the tool
/// is cut, a side, millimetres.
const GROOVE_PLAY: f64 = 0.05;

fn dado(ctx: &Ctx, p: &JointParams, groove: bool) -> Result<Plan, CoreError> {
    let (offset, along) = ctx.face();
    let la = ctx.fa.len;
    let depth = depth_in(p.depth, ctx.tb / 3.0, ctx.tb, "dado")?;
    let near = -offset;
    let far = -(offset + ctx.ta);
    // A groove for thin stock (a back, a drawer bottom) is a little wider
    // than the tool when the stock is thinner than it: the panel floats in
    // it.
    let c = if groove {
        ctx.c.max((ctx.tool() - ctx.ta) / 2.0 + GROOVE_PLAY)
    } else {
        ctx.c
    };
    if ctx.ta + 2.0 * c < ctx.tool() - 1e-9 {
        return Err(CoreError::validation(format!(
            "the dado ({} mm) is narrower than the tool ({} mm); use a smaller tool",
            fmt(ctx.ta + 2.0 * c),
            fmt(ctx.tool())
        )));
    }
    let (c0, c1) = chord(&ctx.rb, &ctx.fb, (near + far) / 2.0, along).ok_or_else(|| {
        CoreError::validation("the place picked is not on the second part; click inside it")
    })?;
    let stop = p.stop.unwrap_or(0.0);
    if stop < 0.0 || stop >= c1 - c0 - ctx.tool() {
        return Err(CoreError::validation(format!(
            "the stop must be between 0 and {} mm",
            fmt((c1 - c0 - ctx.tool()).max(0.0))
        )));
    }
    let run = ctx.run_out();
    let end = if stop > 0.0 { c1 - stop } else { c1 + run };
    let pocket = ctx.fb.rect(c0 - run, end, far - c, near + c);
    let mut a_cut = Vec::new();
    let a_end = along + la / 2.0;
    if stop > 0.0 && a_end > end {
        // A's front corner (the end of its edge that lands furthest along
        // B: its start, or its end when A is turned) steps up over the stop.
        let at = end - c - along;
        a_cut.push(if ctx.flip {
            ctx.fa.rect(at + la / 2.0, la + OVERRUN, -depth, OVERRUN)
        } else {
            ctx.fa.rect(-OVERRUN, la / 2.0 - at, -depth, OVERRUN)
        });
    }
    let resolved = JointParams {
        depth: Some(depth),
        stop: Some(stop),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, depth));
    if !a_cut.is_empty() {
        plan.a = Some(rewrite(
            &ctx.ra,
            &[],
            &a_cut,
            ctx.r,
            ctx.relief,
            ctx.fa.n,
            "the first part",
        )?);
    }
    plan.vectors.push(Made {
        on: On::B,
        // The reliefs dig in past the stop, so the walls the part bears on
        // stay straight.
        path: inside_relieved(pocket, ctx, &ctx.rb, ctx.fb.u),
        role: Role::Pocket { depth },
        name: "dado".into(),
    });
    plan.notes.push(if stop > 0.0 {
        format!("{} mm deep, stopped {} mm short", fmt(depth), fmt(stop))
    } else {
        format!("{} mm deep, through", fmt(depth))
    });
    Ok(plan)
}

/// Rabbet: a step along B's edge, A's thickness wide, that A's edge sits
/// in; A is cut back so its outer face stays flush with B's edge.
fn rabbet(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    // Given `at_b`, A's edge may be shorter than B's and lies along it
    // from there; the rabbet still runs the whole of B's edge.
    let at = match p.at_b {
        Some(at) => {
            if at < -0.5 || at + la > lb + 0.5 {
                return Err(CoreError::validation(format!(
                    "the first part's edge ({} mm) does not lie along the second part's ({} mm)",
                    fmt(la),
                    fmt(lb)
                )));
            }
            at
        }
        None => {
            same_length(ctx)?;
            0.0
        }
    };
    let depth = depth_in(p.depth, ctx.tb / 2.0, ctx.tb, "rabbet")?;
    let run = ctx.run_out();
    let pocket = ctx.fb.rect(-run, lb + run, -(ctx.ta + ctx.c), run);
    let cut = ctx
        .fa
        .rect(-OVERRUN, la + OVERRUN, -(ctx.tb - depth), OVERRUN);
    let resolved = JointParams {
        depth: Some(depth),
        at_b: p.at_b,
        ..JointParams::default()
    };
    let map = Map::axes([(0, -1.0), (2, -1.0), (1, -1.0)], [la + at, 0.0, 0.0]);
    let mut plan = base_plan(ctx, resolved, map);
    plan.a = Some(rewrite(
        &ctx.ra,
        &[],
        &[cut],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?);
    plan.vectors.push(Made {
        on: On::B,
        path: arcs_path(&[pocket], &[]),
        role: Role::Pocket { depth },
        name: "rabbet".into(),
    });
    plan.notes.push(format!("{} mm deep", fmt(depth)));
    Ok(plan)
}

/// Tab and slot (`one` false): tabs on A's edge through slots in B. Mortise
/// and tenon (`one` true): one wide tenon with shoulders at both ends,
/// through or blind (a pocket instead of a slot). Tabs stand out past B by
/// `proud`, their tips rounded with `ends`.
fn tabs(ctx: &Ctx, p: &JointParams, one: bool) -> Result<Plan, CoreError> {
    let (offset, _) = ctx.face();
    let la = ctx.fa.len;
    let c = ctx.c;
    let (count, width) = if one {
        let shoulder = (la / 6.0).max(ctx.tool());
        (1, p.width.unwrap_or(la - 2.0 * shoulder))
    } else {
        let count = p
            .count
            .unwrap_or_else(|| ((la / 100.0).round() as u32).max(2));
        let width = p
            .width
            .unwrap_or_else(|| (la / f64::from(2 * count + 1)).clamp(2.0 * ctx.tool(), 60.0));
        (count, width)
    };
    if count == 0 {
        return Err(CoreError::validation("the joint needs at least one tab"));
    }
    let bias = if one { 0.0 } else { p.bias.unwrap_or(0.0) };
    if !(0.0..=0.5).contains(&bias) {
        return Err(CoreError::validation(
            "the bias must be between 0 and 0.5 (half a tab wider)",
        ));
    }
    let blind = one && p.blind.unwrap_or(false);
    let depth = if blind {
        depth_in(p.depth, ctx.tb * 2.0 / 3.0, ctx.tb, "mortise")?
    } else {
        ctx.tb
    };
    let proud = if blind { 0.0 } else { p.proud.unwrap_or(0.0) };
    if proud < 0.0 {
        return Err(CoreError::validation(
            "the tabs cannot stand out by less than 0",
        ));
    }
    if ctx.ta + 2.0 * c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the slots ({} mm) are narrower than the tool ({} mm); use a smaller tool",
            fmt(ctx.ta + 2.0 * c),
            fmt(ctx.tool())
        )));
    }
    // Tab middles, evenly spread; the first one wider for the bias.
    let widths: Vec<f64> = (0..count)
        .map(|k| if k == 0 { width * (1.0 + bias) } else { width })
        .collect();
    if widths.iter().sum::<f64>() > la - 2.0 * ctx.tool() {
        return Err(CoreError::validation(format!(
            "{count} tabs {} mm wide do not fit on a {} mm edge; use fewer or narrower tabs",
            fmt(width),
            fmt(la)
        )));
    }
    if width + 2.0 * c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tabs ({} mm) are narrower than the tool ({} mm); use wider tabs or a smaller tool",
            fmt(width),
            fmt(ctx.tool())
        )));
    }
    let spans: Vec<(f64, f64)> = widths
        .iter()
        .enumerate()
        .map(|(k, w)| {
            let mid = la * (2.0 * k as f64 + 1.0) / (2.0 * f64::from(count));
            (mid - w / 2.0, mid + w / 2.0)
        })
        .collect();
    // A tenon a hair short of a blind mortise's floor, so it cannot bottom.
    let length = if blind { depth - c } else { ctx.tb + proud };
    let rounded = p.ends == Some(super::Ends::Rounded) && !blind;
    let mut adds = Vec::new();
    let mut tips: Vec<Circle> = Vec::new();
    for (s0, s1) in &spans {
        let rr = (s1 - s0)
            .min(length)
            .min(if proud > 0.0 { proud * 2.0 } else { length })
            / 2.0;
        if rounded && rr > 0.5 {
            adds.push(ctx.fa.rect(*s0, *s1, -OVERRUN, length - rr));
            adds.push(ctx.fa.rect(s0 + rr, s1 - rr, -OVERRUN, length));
            for s in [s0 + rr, s1 - rr] {
                let c = Circle {
                    c: ctx.fa.at(s, length - rr),
                    r: rr,
                };
                adds.push(disc(c.c, c.r));
                tips.push(c);
            }
        } else {
            adds.push(ctx.fa.rect(*s0, *s1, -OVERRUN, length));
        }
    }
    let mut outline = rewrite(
        &ctx.ra,
        &adds,
        &[],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?;
    outline.circles.extend(tips);
    let resolved = JointParams {
        count: Some(count),
        width: Some(width),
        bias: (!one).then_some(bias),
        blind: one.then_some(blind),
        depth: blind.then_some(depth),
        proud: (!blind).then_some(proud),
        ends: Some(if rounded {
            super::Ends::Rounded
        } else {
            super::Ends::Square
        }),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    plan.a = Some(outline);
    // The slots: each tab plus the clearance all round, where A stands.
    let near = -offset;
    let far = -(offset + ctx.ta);
    for (k, (s0, s1)) in spans.iter().enumerate() {
        let (b0, b1) = ctx.b_span(*s0, *s1);
        let ring = ctx.fb.rect(b0 - c, b1 + c, far - c, near + c);
        if !ring
            .iter()
            .all(|q| super::geometry::inside_rings(&ctx.rb, *q))
        {
            return Err(CoreError::validation(
                "a slot would run off the second part; move the place or use fewer tabs",
            ));
        }
        plan.vectors.push(Made {
            on: On::B,
            path: inside_relieved(ring, ctx, &ctx.rb, ctx.fb.u),
            role: if blind {
                Role::Pocket { depth }
            } else {
                Role::Hole
            },
            name: if one {
                "mortise".into()
            } else {
                format!("slot {}", k + 1)
            },
        });
    }
    plan.notes.push(if one {
        format!("a {} mm tenon", fmt(width))
    } else {
        format!("{count} tabs {} mm wide", fmt(width))
    });
    Ok(plan)
}

/// Wedged (tusk) tenon for knock-down furniture: one tenon right through
/// B and out past it, a hole through the tenon just outside B's face, and
/// the wedge as a part of its own drawn beside A. The wedge is cut from
/// A's stock and goes down through the hole narrow end first: its straight
/// side bears on B's face (the hole starts just inside it) and its sloped
/// side on the hole's far end, where it is seated as it starts to bind at
/// A's top, about half way along its length.
fn wedged(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (offset, _) = ctx.face();
    let la = ctx.fa.len;
    let c = ctx.c;
    let shoulder = (la / 6.0).max(ctx.tool());
    let width = p.width.unwrap_or(la - 2.0 * shoulder);
    // The wedge: its narrow end, its slope (1 in 8) and its length.
    let w0 = p.length.unwrap_or(12.0).max(ctx.tool() + 2.0);
    let height = ctx.ta + 40.0;
    let w1 = w0 + height / 8.0;
    // The hole starts 1 mm inside B's far face, so the wedge bears on the
    // face, and is long enough for the wedge to seat half way.
    let inset = 1.0;
    let hole_len = inset + (w0 + w1) / 2.0;
    let wall = (0.6 * w0).max(ctx.tool());
    let proud = p.proud.unwrap_or(hole_len - inset + wall);
    if proud < hole_len - inset + ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tenon must stand out by at least {} mm to hold the wedge",
            fmt(hole_len - inset + ctx.tool())
        )));
    }
    let hole_w = ctx.ta + 2.0 * c;
    if width < hole_w + 2.0 * ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tenon ({} mm) is too narrow for a wedge hole; it needs {} mm",
            fmt(width),
            fmt(hole_w + 2.0 * ctx.tool())
        )));
    }
    if width > la - 2.0 * ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tenon is wider than the edge allows ({} mm)",
            fmt(la - 2.0 * ctx.tool())
        )));
    }
    let (s0, s1) = (la / 2.0 - width / 2.0, la / 2.0 + width / 2.0);
    let length = ctx.tb + proud;
    let tenon = ctx.fa.rect(s0, s1, -OVERRUN, length);
    let outline = rewrite(
        &ctx.ra,
        std::slice::from_ref(&tenon),
        &[],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?;
    // The wedge hole through the tenon, across its middle.
    let hole = ctx.fa.rect(
        la / 2.0 - hole_w / 2.0,
        la / 2.0 + hole_w / 2.0,
        ctx.tb - inset,
        ctx.tb - inset + hole_len,
    );
    let hole_path = {
        let circles = super::geometry::cavity_reliefs(&hole, ctx.r, ctx.relief, ctx.fa.u);
        let ring = with_circles(hole, &circles);
        arcs_path(&[ring], &circles)
    };
    // The mortise through B.
    let near = -offset;
    let far = -(offset + ctx.ta);
    let (b0, b1) = ctx.b_span(s0, s1);
    let mortise = ctx.fb.rect(b0 - c, b1 + c, far - c, near + c);
    if !mortise.iter().all(|q| inside_rings(&ctx.rb, *q)) {
        return Err(CoreError::validation(
            "the mortise would run off the second part; move the place",
        ));
    }
    // The wedge, drawn beside A (right of everything A covers).
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    for q in ctx.ra.iter().flatten().chain(tenon.iter()) {
        xmax = xmax.max(q[0]);
        ymin = ymin.min(q[1]);
    }
    let x = xmax + 10.0;
    let wedge = vec![
        [x, ymin],
        [x + w1, ymin],
        [x + w0, ymin + height],
        [x, ymin + height],
    ];
    let resolved = JointParams {
        width: Some(width),
        proud: Some(proud),
        length: Some(w0),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    plan.a = Some(outline);
    plan.vectors.push(Made {
        on: On::B,
        path: inside_relieved(mortise, ctx, &ctx.rb, ctx.fb.u),
        role: Role::Hole,
        name: "mortise".into(),
    });
    plan.vectors.push(Made {
        on: On::A,
        path: hole_path,
        role: Role::Hole,
        name: "wedge hole".into(),
    });
    // Seated: the wedge's width runs out along the tenon from B's face,
    // its length down through A (wide end up), its thickness across the
    // hole. It binds where its width at A's top fills the hole beyond B.
    let bind = height * (w1 - (hole_len - inset)) / (w1 - w0);
    let face = ctx.fa.at(la / 2.0 - ctx.ta / 2.0, ctx.tb);
    let (n, u) = (ctx.fa.n, ctx.fa.u);
    let placed = Pose::frame(
        [face[0], face[1], ctx.ta + bind],
        [n[0], n[1], 0.0],
        [0.0, 0.0, -1.0],
        [u[0], u[1], 0.0],
    );
    plan.seats.push((
        plan.vectors.len(),
        placed.then_local(&Pose::translation([-x, -ymin, 0.0])),
    ));
    plan.vectors.push(Made {
        on: On::A,
        path: arcs_path(&[wedge], &[]),
        role: Role::Part { thickness: ctx.ta },
        name: "wedge".into(),
    });
    plan.notes.push(format!(
        "the tenon stands out {} mm; the wedge is drawn beside the first part",
        fmt(proud)
    ));
    Ok(plan)
}

/// Kerf wedged (split) tenon: one tenon right through B, flush with its far
/// face, with a kerf cut from the tenon's end along its middle and a wedge
/// (drawn beside A, cut from A's stock) that spreads the two halves against
/// the mortise ends when driven in. The kerf is as wide as the tool cuts it
/// plus the clearance a side, round at its bottom; the wedge tapers from
/// half the kerf at its tip to the kerf plus the clearance and 0.5 mm a
/// side at its head, as long as the kerf is deep, so driven flush it closes
/// the clearance and presses the halves 0.5 mm into the mortise ends. It is
/// seated where it starts to bind, standing out by what is left to drive.
fn kerf_tenon(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let c = ctx.c;
    let tenon_params = JointParams {
        blind: Some(false),
        proud: Some(0.0),
        ends: Some(super::Ends::Square),
        ..p.clone()
    };
    let mut plan = tabs(ctx, &tenon_params, true)?;
    let width = plan.resolved.width.unwrap_or(0.0);
    let kerf = ctx.tool() + 2.0 * c;
    if width < kerf + 2.0 * ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tenon ({} mm) is too narrow to split; it needs {} mm",
            fmt(width),
            fmt(kerf + 2.0 * ctx.tool())
        )));
    }
    let depth = depth_in(p.depth, 0.75 * ctx.tb, ctx.tb, "kerf")?;
    if depth < kerf {
        return Err(CoreError::validation(format!(
            "the kerf must be at least {} mm deep, as wide as the tool cuts it",
            fmt(kerf)
        )));
    }
    let mid = ctx.fa.len / 2.0;
    let tip = ctx.tb;
    let bottom = tip - depth + kerf / 2.0;
    let slot = ctx
        .fa
        .rect(mid - kerf / 2.0, mid + kerf / 2.0, bottom, tip + OVERRUN);
    let round = Circle {
        c: ctx.fa.at(mid, bottom),
        r: kerf / 2.0,
    };
    let Some(tenoned) = plan.a.take() else {
        return Err(CoreError::validation("the tenon could not be made"));
    };
    let mut outline = rewrite(
        &tenoned.rings,
        &[],
        &[slot, disc(round.c, round.r)],
        ctx.r,
        ReliefShape::None,
        ctx.fa.n,
        "the first part",
    )?;
    outline.circles.extend(tenoned.circles);
    outline.circles.push(round);
    plan.a = Some(outline);
    // The wedge as it sits driven flush, then drawn beside A.
    let spread = c + 0.5;
    let (w_tip, w_head) = (kerf / 2.0, kerf + 2.0 * spread);
    let wedge = ctx.fa.ring(&[
        [mid - w_tip / 2.0, tip - depth],
        [mid + w_tip / 2.0, tip - depth],
        [mid + w_head / 2.0, tip],
        [mid - w_head / 2.0, tip],
    ]);
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    for q in ctx.ra.iter().flatten() {
        xmax = xmax.max(q[0]);
        ymin = ymin.min(q[1]);
    }
    let lo = wedge
        .iter()
        .fold([f64::INFINITY; 2], |m, q| [m[0].min(q[0]), m[1].min(q[1])]);
    let (dx, dy) = (xmax + 10.0 - lo[0], ymin - lo[1]);
    let drawn: Vec<P2> = wedge.iter().map(|q| [q[0] + dx, q[1] + dy]).collect();
    // Where it starts to bind: as wide as the kerf at the tenon's end.
    let drive = depth * (w_head - kerf) / (w_head - w_tip);
    let out = drive + 0.02;
    plan.seats.push((
        plan.vectors.len(),
        Pose::translation([ctx.fa.n[0] * out - dx, ctx.fa.n[1] * out - dy, 0.0]),
    ));
    plan.vectors.push(Made {
        on: On::A,
        path: arcs_path(&[drawn], &[]),
        role: Role::Part { thickness: ctx.ta },
        name: "wedge".into(),
    });
    plan.resolved.depth = Some(depth);
    plan.notes.clear();
    plan.notes.push(format!(
        "a {} mm tenon split by a kerf {} mm wide and {} mm deep; the wedge is drawn beside the first part: glue it and drive it {} mm in to spread the tenon",
        fmt(width),
        fmt(kerf),
        fmt(depth),
        fmt(drive)
    ));
    Ok(plan)
}

/// `count` places along a `len` long edge, the outer ones `inset` in from
/// the ends, the rest evenly between.
fn spread(len: f64, count: u32, inset: f64) -> Vec<f64> {
    if count == 1 {
        return vec![len / 2.0];
    }
    let step = (len - 2.0 * inset) / f64::from(count - 1);
    (0..count).map(|k| inset + step * f64::from(k)).collect()
}

/// A circle as a closed path of two arcs.
fn circle_path(c: P2, r: f64) -> Path {
    let circle = Circle { c, r };
    arcs_path(&[disc(c, r)], &[circle])
}

/// Where along B's edge (and how far in) a point of A's edge line at `s`
/// lands when A stands on B, on the middle of A's thickness.
fn on_b(ctx: &Ctx, s: f64) -> P2 {
    let (offset, _) = ctx.face();
    ctx.fb.at(ctx.b_along(s), -(offset + ctx.ta / 2.0))
}

/// Dowels (or pegs): holes in B along the line where A stands, blind for
/// dowels or right through for pegs, and marks on A's edge where to drill
/// the matching holes by hand (a flat part's edge cannot be drilled on the
/// machine).
fn dowels(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    if ctx.face.is_none() {
        return edge_dowels(ctx, p);
    }
    let la = ctx.fa.len;
    let c = ctx.c;
    // The smallest usual dowel at least half the stock and not smaller
    // than the tool can cut.
    let diameter = p.diameter.unwrap_or_else(|| {
        let least = (ctx.ta / 2.0).max(ctx.tool() - 2.0 * c);
        [4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0]
            .into_iter()
            .find(|d| *d >= least - 1e-9)
            .unwrap_or(16.0)
    });
    if diameter <= 0.0 || diameter > ctx.ta {
        return Err(CoreError::validation(format!(
            "the dowels must be thinner than the part they go into ({} mm)",
            fmt(ctx.ta)
        )));
    }
    let hole = diameter + 2.0 * c;
    if hole < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the holes ({} mm) are smaller than the tool ({} mm); use a smaller tool or thicker dowels",
            fmt(hole),
            fmt(ctx.tool())
        )));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / 100.0).round() as u32 + 1).max(2));
    let inset = p
        .inset
        .unwrap_or_else(|| short_inset(la, 2.0 * diameter, hole / 2.0));
    if count == 0 || inset < hole / 2.0 || (count > 1 && inset > la / 2.0) {
        return Err(CoreError::validation(
            "the dowels do not fit on the edge; use fewer or move them in from the ends less",
        ));
    }
    let through = p.blind == Some(false);
    let depth = if through {
        ctx.tb
    } else {
        depth_in(p.depth, ctx.tb * 2.0 / 3.0, ctx.tb, "dowel hole")?
    };
    // The edge holes go as deep as the face holes (a blind one), or twice
    // the stock for pegs.
    let edge_depth = if through { 2.0 * ctx.tb } else { depth };
    let resolved = JointParams {
        count: Some(count),
        diameter: Some(diameter),
        inset: Some(inset),
        depth: Some(depth),
        blind: Some(!through),
        length: Some(depth + edge_depth),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    for (k, s) in spread(la, count, inset).into_iter().enumerate() {
        let centre = on_b(ctx, s);
        if !inside_rings(&ctx.rb, centre) {
            return Err(CoreError::validation(
                "a dowel hole would be off the second part; move the place",
            ));
        }
        plan.vectors.push(Made {
            on: On::B,
            path: circle_path(centre, hole / 2.0),
            role: if through {
                Role::Hole
            } else {
                Role::Pocket { depth }
            },
            name: format!("hole {}", k + 1),
        });
        // On A's face, the hole to drill into its edge by hand: its two
        // sides from the edge in and its bottom, named with its size.
        let r = diameter / 2.0;
        plan.vectors.push(Made {
            on: On::A,
            path: super::geometry::line_path(&[
                ctx.fa.at(s - r, 0.0),
                ctx.fa.at(s - r, -edge_depth),
                ctx.fa.at(s + r, -edge_depth),
                ctx.fa.at(s + r, 0.0),
            ]),
            role: Role::Mark,
            name: format!(
                "drill mark {}: {} mm, {} mm deep into the edge",
                k + 1,
                fmt(diameter),
                fmt(edge_depth)
            ),
        });
    }
    plan.notes.push(format!(
        "the machine cuts the holes in the second part's face; drill the first part's edge by hand at the {count} drill marks, {} mm and {} mm deep; dowels {} mm long",
        fmt(diameter),
        fmt(edge_depth),
        fmt(depth + edge_depth)
    ));
    Ok(plan)
}

/// How far in from the ends of an edge `la` long the outer dowels go by
/// default: an eighth of the edge, at least `least`; on an edge too short
/// for that, a quarter of it, never nearer the end than `edge`.
fn short_inset(la: f64, least: f64, edge: f64) -> f64 {
    let inset = (la / 8.0).max(least);
    if inset > la / 2.0 {
        (la / 4.0).max(edge)
    } else {
        inset
    }
}

/// Dowels between two edges: edge to edge in one plane, or (given
/// `at_b`, where A's edge starts along B's) a thin part A lying with its
/// face on B's edge, A's edge flush with B's outer face. A flat sheet
/// cannot have holes cut into its edges, so every hole is a drill mark on
/// the part's face: across the edge for an edge hole, a circle for a hole
/// into A's face. Dowels are the usual size at most half the thinner stock
/// drilled into its edge.
fn edge_dowels(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let lying = p.at_b;
    // The stock whose edge takes the dowel: both edge to edge, B lying.
    let thin = if lying.is_some() {
        ctx.tb
    } else {
        ctx.ta.min(ctx.tb)
    };
    let diameter = p.diameter.unwrap_or_else(|| {
        [4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0]
            .into_iter()
            .rev()
            .find(|d| *d <= thin / 2.0 + 1e-9)
            .unwrap_or(4.0)
    });
    if diameter <= 0.0 || diameter >= thin {
        return Err(CoreError::validation(format!(
            "the dowels must be thinner than the edge they go into ({} mm)",
            fmt(thin)
        )));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / 100.0).round() as u32 + 1).max(2));
    let inset = p
        .inset
        .unwrap_or_else(|| short_inset(la, 2.0 * diameter, diameter));
    if count == 0 || inset < diameter || (count > 1 && inset > la / 2.0) {
        return Err(CoreError::validation(
            "the dowels do not fit on the edge; use fewer or move them in from the ends less",
        ));
    }
    // Each edge hole is three dowel widths deep; a hole into a lying
    // part's face two thirds of its stock.
    let edge_depth = p.depth.unwrap_or(3.0 * diameter);
    if edge_depth <= 0.0 {
        return Err(CoreError::validation(
            "the dowel holes must be deeper than 0",
        ));
    }
    let face_depth = ctx.ta * 2.0 / 3.0;
    let a_depth = if lying.is_some() {
        face_depth
    } else {
        edge_depth
    };
    let map = match lying {
        Some(at) => Map::axes([(0, -1.0), (2, -1.0), (1, -1.0)], [la + at, 0.0, 0.0]),
        None => coplanar_map(la),
    };
    let resolved = JointParams {
        count: Some(count),
        diameter: Some(diameter),
        inset: Some(inset),
        depth: Some(edge_depth),
        length: Some(a_depth + edge_depth),
        at_b: lying,
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, map);
    let r = diameter / 2.0;
    let across = |f: &Frame, s: f64, deep: f64| {
        super::geometry::line_path(&[
            f.at(s - r, 0.0),
            f.at(s - r, -deep),
            f.at(s + r, -deep),
            f.at(s + r, 0.0),
        ])
    };
    let mark = |k: usize, deep: f64, into: &str| {
        format!(
            "drill mark {}: {} mm, {} mm deep into the {into}",
            k + 1,
            fmt(diameter),
            fmt(deep)
        )
    };
    for (k, s) in spread(la, count, inset).into_iter().enumerate() {
        let sb = la + lying.unwrap_or(0.0) - s;
        if sb < r - 1e-6 || sb > lb - r + 1e-6 {
            return Err(CoreError::validation(
                "a dowel would be off the second part's edge; move them in from the ends",
            ));
        }
        plan.vectors.push(Made {
            on: On::B,
            path: across(&ctx.fb, sb, edge_depth),
            role: Role::Mark,
            name: mark(k, edge_depth, "edge"),
        });
        let (path, name) = if lying.is_some() {
            (
                circle_path(ctx.fa.at(s, -ctx.tb / 2.0), r),
                mark(k, face_depth, "face"),
            )
        } else {
            (across(&ctx.fa, s, edge_depth), mark(k, edge_depth, "edge"))
        };
        plan.vectors.push(Made {
            on: On::A,
            path,
            role: Role::Mark,
            name,
        });
    }
    plan.notes.push(format!(
        "drill the dowel holes by hand at the {} drill marks on each part, {} mm across; dowels {} mm long",
        count,
        fmt(diameter),
        fmt(a_depth + edge_depth)
    ));
    Ok(plan)
}

/// The cam's depth for stock `thick` thick: 12.5 mm in 16 mm, 13.5 mm in
/// 18 mm, 14.5 mm in 19 mm stock (the fittings' published depths, between
/// them in step), never closer than 3 mm to the far face.
pub fn cam_depth(thick: f64) -> f64 {
    let depth = if thick <= 16.0 {
        12.5
    } else if thick <= 18.0 {
        12.5 + (thick - 16.0) / 2.0
    } else {
        13.5 + (thick - 18.0)
    };
    depth.min(thick - 3.0)
}

/// Cam lock (knock-down fitting): for each fitting, a cam bore (15 mm
/// across) sunk into A's face `length` (34 mm) in from A's edge, and in
/// B's face where A's middle stands a hole for the bolt: a 5 mm pilot when
/// the tool cuts one, else an 8 mm hole for an expanding dowel. The bolt's
/// 8 mm bore runs from A's edge to the cam, level, which a flat part
/// cannot have cut on the machine: a mark on A's face shows where to drill
/// it by hand.
fn cam_lock(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let la = ctx.fa.len;
    let c = ctx.c;
    let cam = p.diameter.unwrap_or(15.0);
    let depth = depth_in(p.depth, cam_depth(ctx.ta), ctx.ta, "cam bore")?;
    let reach = p.length.unwrap_or(34.0);
    if reach <= cam / 2.0 {
        return Err(CoreError::validation(
            "the cam must sit more than half its width in from the edge",
        ));
    }
    if cam + 2.0 * c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the cam bore ({} mm) is smaller than the tool ({} mm); use a smaller tool",
            fmt(cam),
            fmt(ctx.tool())
        )));
    }
    let bolt = if ctx.tool() <= 5.0 { 5.0 } else { 8.0 };
    let bolt_depth = 11.5_f64.min(ctx.tb - 3.0);
    if bolt_depth <= 0.0 {
        return Err(CoreError::validation(
            "the second part is too thin for the bolt's hole",
        ));
    }
    let count = p.count.unwrap_or(if la > 600.0 { 3 } else { 2 });
    let inset = p.inset.unwrap_or(37.0_f64.min(la / 4.0));
    if count == 0 || inset < cam / 2.0 || (count > 1 && inset > la / 2.0) {
        return Err(CoreError::validation(
            "the cams do not fit on the edge; use fewer or move them in from the ends less",
        ));
    }
    let resolved = JointParams {
        count: Some(count),
        diameter: Some(cam),
        depth: Some(depth),
        length: Some(reach),
        inset: Some(inset),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    for (k, s) in spread(la, count, inset).into_iter().enumerate() {
        let centre = ctx.fa.at(s, -reach);
        if !inside_rings(&ctx.ra, centre) {
            return Err(CoreError::validation(
                "a cam bore would be off the first part; use a shorter reach",
            ));
        }
        plan.vectors.push(Made {
            on: On::A,
            path: circle_path(centre, cam / 2.0 + c),
            role: Role::Pocket { depth },
            name: format!("cam {}", k + 1),
        });
        let hole = on_b(ctx, s);
        if !inside_rings(&ctx.rb, hole) {
            return Err(CoreError::validation(
                "a bolt hole would be off the second part; move the place",
            ));
        }
        plan.vectors.push(Made {
            on: On::B,
            path: circle_path(hole, bolt / 2.0),
            role: Role::Pocket { depth: bolt_depth },
            name: format!("bolt hole {}", k + 1),
        });
        plan.vectors.push(Made {
            on: On::A,
            path: super::geometry::line_path(&[
                ctx.fa.at(s - 4.0, 0.0),
                ctx.fa.at(s - 4.0, -reach),
                ctx.fa.at(s + 4.0, -reach),
                ctx.fa.at(s + 4.0, 0.0),
            ]),
            role: Role::Mark,
            name: format!(
                "drill mark {}: 8 mm into the edge to the cam, {} mm deep at mid thickness",
                k + 1,
                fmt(reach)
            ),
        });
    }
    plan.notes.push(format!(
        "the machine cuts {count} cam bores ({} mm, {} mm deep) and the bolt holes ({} mm); drill the 8 mm bores into the first part's edge by hand at the drill marks, {} mm deep at mid thickness",
        fmt(cam),
        fmt(depth),
        fmt(bolt),
        fmt(reach)
    ));
    Ok(plan)
}

/// Screws: a butt joint held by `count` screws through B into A's edge;
/// marks on B's face for each countersunk clearance hole and on A's edge
/// for each pilot, drilled by hand.
fn screws(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let la = ctx.fa.len;
    let diameter = p.diameter.unwrap_or(4.0);
    let count = p
        .count
        .unwrap_or_else(|| ((la / 150.0).round() as u32 + 1).max(2));
    let inset = p.inset.unwrap_or(50.0_f64.min(la / 4.0));
    if count == 0 || diameter <= 0.0 || (count > 1 && inset > la / 2.0) {
        return Err(CoreError::validation(
            "the screws do not fit on the edge; use fewer or move them in from the ends less",
        ));
    }
    let resolved = JointParams {
        count: Some(count),
        diameter: Some(diameter),
        inset: Some(inset),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    for (k, s) in spread(la, count, inset).into_iter().enumerate() {
        let centre = on_b(ctx, s);
        plan.vectors.push(Made {
            on: On::B,
            path: circle_path(centre, diameter / 2.0 + 0.25),
            role: Role::Mark,
            name: format!(
                "screw {}: {} mm clearance, countersunk",
                k + 1,
                fmt(diameter + 0.5)
            ),
        });
        let r = diameter / 4.0;
        plan.vectors.push(Made {
            on: On::A,
            path: super::geometry::line_path(&[
                ctx.fa.at(s - r, 0.0),
                ctx.fa.at(s - r, -20.0),
                ctx.fa.at(s + r, -20.0),
                ctx.fa.at(s + r, 0.0),
            ]),
            role: Role::Mark,
            name: format!("pilot {}: {} mm into the edge", k + 1, fmt(diameter * 0.6)),
        });
    }
    plan.notes.push(format!(
        "{count} screws {} mm: drill and countersink the second part at the marks, pilot the first part's edge",
        fmt(diameter)
    ));
    Ok(plan)
}

/// Captive nut (Pettis joint): bolts through B into A's edge, each held by
/// a square nut in a cross slot through A, so the joint comes apart and
/// goes together again.
fn captive_nut(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let la = ctx.fa.len;
    let c = ctx.c;
    // The slots for the bolt and the nut must be at least as wide as the
    // tool: the smallest usual bolt (M6 or larger) whose slots fit.
    let fits =
        |bolt: f64, nut_t: f64| bolt + 2.0 * c >= ctx.tool() && nut_t + 2.0 * c >= ctx.tool();
    let smallest = [3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0]
        .into_iter()
        .find(|b| fits(*b, *b * 5.0 / 6.0));
    let bolt = match p.diameter {
        Some(b) => b,
        None => [6.0, 8.0, 10.0, 12.0, 16.0, 20.0]
            .into_iter()
            .find(|b| fits(*b, *b * 5.0 / 6.0))
            .unwrap_or(20.0),
    };
    let length = p.length.unwrap_or((ctx.tb + 20.0).max(30.0));
    let nut_w = p.nut_width.unwrap_or(bolt * 5.0 / 3.0);
    let nut_t = p.nut_thickness.unwrap_or(bolt * 5.0 / 6.0);
    if bolt > 0.0 && !fits(bolt, nut_t) {
        let hint = smallest.map_or_else(
            || "use a smaller tool".to_string(),
            |b| format!("use an M{} bolt or a smaller tool", fmt(b)),
        );
        return Err(CoreError::validation(format!(
            "the slots for an M{} bolt and its nut ({} and {} mm) are narrower than the tool ({} mm); {hint}",
            fmt(bolt),
            fmt(bolt + 2.0 * c),
            fmt(nut_t + 2.0 * c),
            fmt(ctx.tool())
        )));
    }
    if bolt <= 0.0 || nut_w <= bolt || nut_t <= 0.0 {
        return Err(CoreError::validation(
            "the nut must be wider than the bolt and thicker than 0",
        ));
    }
    let into = length - ctx.tb;
    if into < nut_t + 4.0 {
        return Err(CoreError::validation(format!(
            "the bolt is too short; it needs to be at least {} mm long",
            fmt(ctx.tb + nut_t + 4.0)
        )));
    }
    let bolt_slot = bolt + 2.0 * c;
    let nut_slot = nut_t + 2.0 * c;
    let count = p.count.unwrap_or(2);
    let inset = p.inset.unwrap_or(la / 4.0);
    if count == 0 || inset < nut_w / 2.0 + ctx.tool() || (count > 1 && inset > la / 2.0) {
        return Err(CoreError::validation(
            "the bolts do not fit on the edge; use fewer or move them further in",
        ));
    }
    // The nut sits near the bolt's end, 2 mm of thread past it.
    let nut_at = into - nut_t / 2.0 - 2.0;
    let mut cuts = Vec::new();
    let mut plan_vectors = Vec::new();
    for (k, s) in spread(la, count, inset).into_iter().enumerate() {
        cuts.push(ctx.fa.rect(
            s - bolt_slot / 2.0,
            s + bolt_slot / 2.0,
            -(into + 1.0),
            OVERRUN,
        ));
        cuts.push(ctx.fa.rect(
            s - nut_w / 2.0 - c,
            s + nut_w / 2.0 + c,
            -(nut_at + nut_slot / 2.0),
            -(nut_at - nut_slot / 2.0),
        ));
        let centre = on_b(ctx, s);
        if !inside_rings(&ctx.rb, centre) {
            return Err(CoreError::validation(
                "a bolt hole would be off the second part; move the place",
            ));
        }
        plan_vectors.push(Made {
            on: On::B,
            path: circle_path(centre, bolt_slot / 2.0),
            role: Role::Hole,
            name: format!("bolt hole {}", k + 1),
        });
    }
    let resolved = JointParams {
        count: Some(count),
        diameter: Some(bolt),
        length: Some(length),
        nut_width: Some(nut_w),
        nut_thickness: Some(nut_t),
        inset: Some(inset),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, 0.0));
    plan.a = Some(rewrite(
        &ctx.ra,
        &[],
        &cuts,
        ctx.r,
        ctx.relief,
        ctx.fa.u,
        "the first part",
    )?);
    plan.vectors = plan_vectors;
    plan.notes.push(format!(
        "{count} M{} bolts {} mm long with square nuts",
        fmt(bolt),
        fmt(length)
    ));
    Ok(plan)
}

/// The map of two parts edge to edge in one plane: B turned half round
/// about the seam, its edge on A's.
pub(crate) fn coplanar_map(la: f64) -> Map {
    Map::axes([(0, -1.0), (1, -1.0), (2, 1.0)], [la, 0.0, 0.0])
}

/// The line through `p` along `d` moved `by` to its left.
fn shifted(p: P2, d: P2, by: f64) -> (P2, P2) {
    let n = [-d[1], d[0]];
    let len = (n[0] * n[0] + n[1] * n[1]).sqrt().max(1e-12);
    ([p[0] + n[0] / len * by, p[1] + n[1] / len * by], d)
}

/// Where the lines `(p, d)` and `(q, e)` cross.
fn crossing(line: (P2, P2), other: (P2, P2)) -> P2 {
    let ((p, d), (q, e)) = (line, other);
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() < 1e-12 {
        return p;
    }
    let w = [q[0] - p[0], q[1] - p[1]];
    let k = (w[0] * e[1] - w[1] * e[0]) / den;
    [p[0] + d[0] * k, p[1] + d[1] * k]
}

/// Dovetail cut from the top with an end mill, locking two panels edge to
/// edge in one plane: tails on A's edge, wider at the tip than the root,
/// in sockets cut into B with the clearance on every side.
fn dovetail(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    same_length(ctx)?;
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let c = ctx.c;
    let depth = p.depth.unwrap_or((2.5 * ctx.tool()).max(15.0));
    let angle = p.angle.unwrap_or(14.0);
    if !(5.0..=30.0).contains(&angle) {
        return Err(CoreError::validation(
            "the tail angle must be between 5 and 30 degrees",
        ));
    }
    if depth <= 0.0 {
        return Err(CoreError::validation("the tails must be longer than 0"));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / 60.0).round() as u32).max(1));
    if count == 0 {
        return Err(CoreError::validation("a dovetail needs at least one tail"));
    }
    let pitch = la / f64::from(count);
    let flare = depth * angle.to_radians().tan();
    // Tails fill half of each pitch on average.
    let half_root = pitch / 4.0 - flare / 2.0;
    let half_tip = pitch / 4.0 + flare / 2.0;
    if 2.0 * half_root + 2.0 * c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tails are narrower at the root than the tool ({} mm); use fewer or shorter tails",
            fmt(ctx.tool())
        )));
    }
    if pitch - 2.0 * half_tip - 2.0 * c < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the gaps between the tail tips are narrower than the tool ({} mm); use fewer or shorter tails",
            fmt(ctx.tool())
        )));
    }
    let run = ctx.run_out();
    let mut tails = Vec::new();
    let mut sockets = Vec::new();
    for k in 0..count {
        let mid = pitch * (f64::from(k) + 0.5);
        let (r0, r1) = (mid - half_root, mid + half_root);
        let (t0, t1) = (mid - half_tip, mid + half_tip);
        tails.push(ctx.fa.ring(&[
            [r0, -OVERRUN],
            [r1, -OVERRUN],
            [r1, 0.0],
            [t1, depth],
            [t0, depth],
            [r0, 0.0],
        ]));
        // The socket: the tail's sides and tip moved out by the clearance,
        // open past B's edge (into A's side of the seam).
        let right = shifted([r1, 0.0], [t1 - r1, depth], -c);
        let left = shifted([r0, 0.0], [t0 - r0, depth], c);
        let tip = ([0.0, depth + c], [1.0, 0.0]);
        let open = ([0.0, -run], [1.0, 0.0]);
        let local = [
            crossing(left, open),
            crossing(right, open),
            crossing(right, tip),
            crossing(left, tip),
        ];
        // In B's frame: along runs the other way, out is the other side.
        let socket: Vec<P2> = local.iter().map(|q| [lb - q[0], -q[1]]).collect();
        sockets.push(ctx.fb.ring(&socket));
    }
    let resolved = JointParams {
        count: Some(count),
        depth: Some(depth),
        angle: Some(angle),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, coplanar_map(la));
    plan.a = Some(rewrite(
        &ctx.ra,
        &tails,
        &[],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?);
    plan.b = Some(rewrite(
        &ctx.rb,
        &[],
        &sockets,
        ctx.r,
        ctx.relief,
        ctx.fb.n,
        "the second part",
    )?);
    plan.notes.push(format!(
        "{count} tails {} mm long at {} degrees",
        fmt(depth),
        fmt(angle)
    ));
    Ok(plan)
}

/// Butterfly (bow tie) keys across the seam of two panels edge to edge,
/// set in as an inlay: a pocket half in each panel, its waist rounded so
/// the key's inside corners (which the tool leaves round) fit, and each
/// key drawn beside A: the pocket shape moved in by the clearance, its
/// outside corners rounded as the tool rounds the pocket's.
fn butterfly(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    use i_overlay::core::overlay_rule::OverlayRule;
    same_length(ctx)?;
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let c = ctx.c;
    let length = p.length.unwrap_or(60.0);
    let width = p.width.unwrap_or(length / 2.0);
    let waist = width * 0.45;
    let depth = depth_in(p.depth, (ctx.ta / 2.0).max(ctx.ta.min(5.0)), ctx.ta, "key")?;
    if waist - 2.0 * ctx.r < 0.5 {
        return Err(CoreError::validation(format!(
            "the keys are too narrow at the waist for the tool ({} mm); make them wider",
            fmt(ctx.tool())
        )));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / 250.0).round() as u32).max(1));
    if count == 0 || f64::from(count) * width > la - ctx.tool() {
        return Err(CoreError::validation(
            "the keys do not fit along the seam; use fewer or narrower keys",
        ));
    }
    let r = ctx.r + super::geometry::RELIEF_EXTRA;
    let run = ctx.run_out();
    let mut plan = base_plan(
        ctx,
        JointParams {
            count: Some(count),
            length: Some(length),
            width: Some(width),
            depth: Some(depth),
            ..JointParams::default()
        },
        coplanar_map(la),
    );
    // The keys go beside A, right of everything A covers.
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    for q in ctx.ra.iter().flatten() {
        xmax = xmax.max(q[0]);
        ymin = ymin.min(q[1]);
    }
    for k in 0..count {
        let mid = la * (2.0 * f64::from(k) + 1.0) / (2.0 * f64::from(count));
        let (h, w) = (length / 2.0, width / 2.0);
        let local = [
            [mid - w, -h],
            [mid + w, -h],
            [mid + waist / 2.0, 0.0],
            [mid + w, h],
            [mid - w, h],
            [mid - waist / 2.0, 0.0],
        ];
        let bowtie = ctx.fa.ring(&local);
        // The same bow tie on B, turned about the seam.
        let mapped: Vec<P2> = local.iter().map(|q| [lb - q[0], -q[1]]).collect();
        let bowtie_b = ctx.fb.ring(&mapped);
        // Each panel's half, running a little past the seam.
        for (on, shape, keep) in [
            (
                On::A,
                &bowtie,
                ctx.fa.rect(-la, 2.0 * la, -2.0 * length, run),
            ),
            (
                On::B,
                &bowtie_b,
                ctx.fb.rect(-lb, 2.0 * lb, -2.0 * length, run),
            ),
        ] {
            let (pocket, waist_arcs) = super::geometry::fillet_polygon(shape, 0.0, r);
            let halves = super::geometry::overlay(
                std::slice::from_ref(&pocket),
                std::slice::from_ref(&keep),
                OverlayRule::Intersect,
            );
            let half = super::geometry::one_shape(halves, "the key pocket")?;
            plan.vectors.push(Made {
                on,
                path: arcs_path(&half, &waist_arcs),
                role: Role::Pocket { depth },
                name: format!("key pocket {}", k + 1),
            });
        }
        // The key: moved in by the clearance, outside corners rounded as the
        // tool leaves the pocket's, inside ones to match the pocket's waist.
        let inner = super::geometry::offset_polygon(&bowtie, c);
        let (key, key_arcs) = super::geometry::fillet_polygon(&inner, ctx.r, r + c);
        let lo = key
            .iter()
            .fold([f64::INFINITY; 2], |m, q| [m[0].min(q[0]), m[1].min(q[1])]);
        let dx = xmax + 10.0 + f64::from(k) * (width + 10.0) - lo[0];
        let dy = ymin - lo[1];
        let moved: Vec<P2> = key.iter().map(|q| [q[0] + dx, q[1] + dy]).collect();
        let arcs: Vec<Circle> = key_arcs
            .iter()
            .map(|a| Circle {
                c: [a.c[0] + dx, a.c[1] + dy],
                r: a.r,
            })
            .collect();
        // Seated, the key stands on the pocket floors, back over the bow tie.
        plan.seats.push((
            plan.vectors.len(),
            Pose::translation([-dx, -dy, ctx.ta - depth]),
        ));
        plan.vectors.push(Made {
            on: On::A,
            path: arcs_path(&[moved], &arcs),
            role: Role::Part {
                thickness: depth + 1.0,
            },
            name: format!("key {}", k + 1),
        });
    }
    plan.notes.push(format!(
        "{count} keys {} mm deep; cut them from stock {} mm thick and sand flush",
        fmt(depth),
        fmt(depth + 1.0)
    ));
    Ok(plan)
}

/// Puzzle (jigsaw) joint locking two panels edge to edge: round knobs on
/// A's edge on narrower necks, in sockets in B the clearance larger all
/// round. Holds in tension without glue.
fn puzzle(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    same_length(ctx)?;
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let c = ctx.c;
    // Left to size themselves, the knobs are never so small that the tool
    // cannot cut round their necks.
    let smallest = (ctx.tool() + 0.5) / 1.1;
    let radius = p
        .width
        .map_or_else(|| (la / 12.0).clamp(6.0, 15.0).max(smallest), |w| w / 2.0);
    let neck = radius * 1.1;
    let reach = radius * 1.6;
    if neck + 1e-9 < ctx.tool() + 0.5 {
        return Err(CoreError::validation(format!(
            "the knobs are too small for the tool ({} mm); make them wider",
            fmt(ctx.tool())
        )));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / 80.0).round() as u32).max(1));
    if count == 0 || f64::from(count) * (2.0 * radius + 2.0 * ctx.tool()) > la {
        return Err(CoreError::validation(
            "the knobs do not fit along the edge; use fewer or smaller knobs",
        ));
    }
    let run = ctx.run_out();
    let mut knobs = Vec::new();
    let mut knob_arcs = Vec::new();
    let mut sockets = Vec::new();
    let mut socket_arcs = Vec::new();
    for k in 0..count {
        let mid = la * (2.0 * f64::from(k) + 1.0) / (2.0 * f64::from(count));
        let centre = ctx.fa.at(mid, reach);
        knobs.push(
            ctx.fa
                .rect(mid - neck / 2.0, mid + neck / 2.0, -OVERRUN, reach),
        );
        knobs.push(disc(centre, radius));
        knob_arcs.push(Circle {
            c: centre,
            r: radius,
        });
        // In B's frame: along runs the other way, out is the other side.
        let bm = lb - mid;
        let b_centre = ctx.fb.at(bm, -reach);
        sockets.push(
            ctx.fb
                .rect(bm - neck / 2.0 - c, bm + neck / 2.0 + c, -reach, run),
        );
        sockets.push(disc(b_centre, radius + c));
        socket_arcs.push(Circle {
            c: b_centre,
            r: radius + c,
        });
    }
    let mut a = rewrite(
        &ctx.ra,
        &knobs,
        &[],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?;
    a.circles.extend(knob_arcs);
    let mut b = rewrite(
        &ctx.rb,
        &[],
        &sockets,
        ctx.r,
        ctx.relief,
        ctx.fb.n,
        "the second part",
    )?;
    b.circles.extend(socket_arcs);
    let resolved = JointParams {
        count: Some(count),
        width: Some(2.0 * radius),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, coplanar_map(la));
    plan.a = Some(a);
    plan.b = Some(b);
    plan.notes
        .push(format!("{count} knobs {} mm across", fmt(2.0 * radius)));
    Ok(plan)
}

/// Living hinge: a band across the part, centred where its edge was
/// clicked, of slots through the part so it bends. The band is as long as
/// the bend: the bend angle times the radius the middle of the stock bends
/// round (the inside radius asked for plus half the thickness). The slots
/// are as narrow as the tool cuts; the webs between them are what bends.
/// Straight and sinuous slots run across the part staggered (two slots
/// with a bridge in the middle, then one long one); a spiral hinge is a
/// grid of spiral slots, one per cell, turning alternate ways.
fn hinge(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let la = ctx.fa.len;
    let t = ctx.ta;
    let kerf = ctx.tool() + 0.1;
    let web = p.depth.unwrap_or((t / 4.0).clamp(1.5, 4.0));
    let at = p.at_a.unwrap_or(la / 2.0);
    let pattern = p.pattern.unwrap_or(HingePattern::Straight);
    let radius = p.radius.unwrap_or(3.0 * t);
    let bend = p.angle.unwrap_or(90.0);
    if !(radius > 0.0 && radius.is_finite()) {
        return Err(CoreError::validation("the bend radius must be more than 0"));
    }
    if !(bend > 0.0 && bend <= 360.0) {
        return Err(CoreError::validation(
            "the bend angle must be more than 0 and at most 360 degrees",
        ));
    }
    if web <= 0.0 {
        return Err(CoreError::validation(
            "the web between the slots must be more than 0",
        ));
    }
    let band = p.width.unwrap_or(bend.to_radians() * (radius + t / 2.0));
    let (b0, b1) = (at - band / 2.0, at + band / 2.0);
    if b0 < 0.0 || b1 > la {
        return Err(CoreError::validation(
            "the hinge band runs off the end of the edge; click nearer the middle, or bend less or round a smaller radius",
        ));
    }
    let height = height_at(&ctx.fa, &ctx.ra, at).ok_or_else(|| {
        CoreError::validation("the picked place is not on the part's edge; pick again")
    })?;
    let margin = web.max(3.0);
    let span = height - 2.0 * margin;
    let resolved = JointParams {
        width: Some(band),
        depth: Some(web),
        at_a: Some(at),
        radius: Some(radius),
        angle: Some(bend),
        pattern: Some(pattern),
        ..JointParams::default()
    };
    let map = Map::axes([(0, 1.0), (1, 1.0), (2, 1.0)], [0.0, 0.0, 0.0]);
    let mut plan = base_plan(ctx, resolved, map);
    let slots = match pattern {
        HingePattern::Spiral => spiral_slots(ctx, b0, b1, margin, span, kerf, web)?,
        _ => {
            let gap = 2.0 * web + kerf;
            if span < 2.0 * gap + 4.0 * kerf {
                return Err(CoreError::validation(
                    "the part is too narrow here for a living hinge",
                ));
            }
            wave_slots(
                ctx,
                (b0, b1),
                (margin, height - margin),
                kerf,
                web,
                pattern == HingePattern::Sinuous,
            )?
        }
    };
    let count = match pattern {
        HingePattern::Spiral => slots.len() as u32,
        _ => slots.iter().map(|(col, _)| *col).max().map_or(0, |c| c + 1),
    };
    plan.resolved.count = Some(count);
    for (k, (_, ring)) in slots.into_iter().enumerate() {
        plan.vectors.push(Made {
            on: On::A,
            path: arcs_path(&[ring], &[]),
            role: Role::Hole,
            name: format!("hinge slot {}", k + 1),
        });
    }
    let what = match pattern {
        HingePattern::Spiral => format!("{count} spiral cells"),
        _ => format!("{count} rows of slots"),
    };
    plan.notes.push(format!(
        "{what} over {} mm; bends {} degrees round {} mm",
        fmt(band),
        fmt(bend),
        fmt(radius)
    ));
    Ok(plan)
}

/// A slot `kerf` wide along the centre line `line` (job points), its sides
/// the line moved half the kerf out each way, square at the ends.
fn slot_ring(line: &[P2], kerf: f64) -> Vec<P2> {
    let n = line.len();
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let prev = line[i.saturating_sub(1)];
        let next = line[(i + 1).min(n - 1)];
        let d = super::geometry::unit(super::geometry::sub(next, prev));
        let normal = [-d[1], d[0]];
        let h = kerf / 2.0;
        left.push([line[i][0] + normal[0] * h, line[i][1] + normal[1] * h]);
        right.push([line[i][0] - normal[0] * h, line[i][1] - normal[1] * h]);
    }
    right.reverse();
    left.extend(right);
    super::geometry::ccw(left)
}

/// The staggered slots of a straight or sinuous hinge over `band` (along
/// the edge) and `across` (in from the edge), each with its column. A
/// sinuous slot's centre line waves along its length, all in step, the
/// columns spread so the web square to the slots stays `web`.
fn wave_slots(
    ctx: &Ctx,
    band: (f64, f64),
    across: (f64, f64),
    kerf: f64,
    web: f64,
    wavy: bool,
) -> Result<Vec<(u32, Vec<P2>)>, CoreError> {
    let (b0, b1) = band;
    let (m0, m1) = across;
    let gap = 2.0 * web + kerf;
    // Steepest slope of a wave: the web square to it is the column pitch
    // times the cosine of that.
    let slope: f64 = if wavy { 0.6 } else { 0.0 };
    let pitch = (kerf + web) * (1.0 + slope * slope).sqrt();
    let amp = if wavy { pitch / 2.0 } else { 0.0 };
    let wave = if wavy {
        std::f64::consts::TAU * amp / slope
    } else {
        1.0
    };
    let room = b1 - b0 - kerf - 2.0 * amp;
    if room < 0.0 {
        return Err(CoreError::validation(
            "the hinge band is too short for its slots; bend more or round a larger radius",
        ));
    }
    let columns = (room / pitch).floor() as u32 + 1;
    let seg = (m1 - m0 - gap) / 2.0;
    let mut out = Vec::new();
    for col in 0..columns {
        let s = b0 + amp + kerf / 2.0 + f64::from(col) * pitch;
        let runs: Vec<(f64, f64)> = if col % 2 == 0 {
            vec![(m0, m0 + seg), (m0 + seg + gap, m1)]
        } else {
            vec![(m0 + gap / 2.0, m1 - gap / 2.0)]
        };
        for (d0, d1) in runs {
            let ring = if wavy {
                let steps = (((d1 - d0) / (wave / 32.0).min(1.0)).ceil() as usize).max(2);
                let line: Vec<P2> = (0..=steps)
                    .map(|k| {
                        let d = d0 + (d1 - d0) * k as f64 / steps as f64;
                        let off = amp * (std::f64::consts::TAU * d / wave).sin();
                        ctx.fa.at(s + off, -d)
                    })
                    .collect();
                slot_ring(&line, kerf)
            } else {
                ctx.fa.rect(s - kerf / 2.0, s + kerf / 2.0, -d1, -d0)
            };
            out.push((col, ring));
        }
    }
    Ok(out)
}

/// The spiral slots of a spiral hinge: square cells filling the band
/// (along the edge, `b0` to `b1`) and `span` across it from `margin` in,
/// each with an Archimedean spiral slot of a turn and a quarter, the arms
/// `web` apart, turning alternate ways cell by cell.
fn spiral_slots(
    ctx: &Ctx,
    b0: f64,
    b1: f64,
    margin: f64,
    span: f64,
    kerf: f64,
    web: f64,
) -> Result<Vec<(u32, Vec<P2>)>, CoreError> {
    let arm = kerf + web;
    let turns = 1.25;
    let inner = 0.75 * kerf;
    let outer = inner + turns * arm;
    let least = 2.0 * (outer + kerf / 2.0 + web / 2.0);
    let (nx, ny) = (((b1 - b0) / least).floor(), (span / least).floor());
    if nx < 1.0 || ny < 1.0 {
        return Err(CoreError::validation(format!(
            "spiral cells need {} mm square for this tool; bend more, round a larger radius or use a smaller tool",
            fmt(least)
        )));
    }
    let (cx, cy) = ((b1 - b0) / nx, span / ny);
    let mut out = Vec::new();
    for i in 0..nx as u32 {
        for j in 0..ny as u32 {
            let centre = [
                b0 + cx * (f64::from(i) + 0.5),
                margin + cy * (f64::from(j) + 0.5),
            ];
            let hand = if (i + j) % 2 == 0 { 1.0 } else { -1.0 };
            let sweep = std::f64::consts::TAU * turns;
            let steps = (sweep / 0.08).ceil() as usize;
            let line: Vec<P2> = (0..=steps)
                .map(|k| {
                    let phi = sweep * k as f64 / steps as f64;
                    let r = inner + arm * phi / std::f64::consts::TAU;
                    let (sin, cos) = (hand * phi).sin_cos();
                    ctx.fa.at(centre[0] + r * cos, -(centre[1] + r * sin))
                })
                .collect();
            out.push((i, slot_ring(&line, kerf)));
        }
    }
    Ok(out)
}

/// How far into a part the material runs from its edge at `s`, straight
/// in.
fn height_at(frame: &Frame, rings: &[Vec<P2>], s: f64) -> Option<f64> {
    let d = [-frame.n[0], -frame.n[1]];
    let start = frame.at(s, -0.01);
    // Where an arch or a notch leaves the edge line with no material, on
    // across the gap to the part and through it.
    let gap = if inside_rings(rings, start) {
        0.0
    } else {
        let enter = super::geometry::ray_exit(rings, start, d)? + 0.02;
        if !inside_rings(rings, [start[0] + d[0] * enter, start[1] + d[1] * enter]) {
            return None;
        }
        enter
    };
    let from = [start[0] + d[0] * gap, start[1] + d[1] * gap];
    super::geometry::ray_exit(rings, from, d).map(|reach| gap + reach + 0.01)
}

/// Cross lap (egg crate): two parts standing across each other, each with
/// a slot half way into it from the picked edge at the picked place, as
/// wide as the other part is thick plus the clearance on each side. A snap
/// detent narrows each slot's mouth by `detent` a side, so the parts click
/// together.
fn cross_lap(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let c = ctx.c;
    let at_a = p.at_a.unwrap_or(la / 2.0);
    let at_b = p.at_b.unwrap_or(lb / 2.0);
    let half_a = (ctx.tb + 2.0 * c) / 2.0;
    let half_b = (ctx.ta + 2.0 * c) / 2.0;
    if at_a - half_a < 0.0 || at_a + half_a > la || at_b - half_b < 0.0 || at_b + half_b > lb {
        return Err(CoreError::validation(
            "the slot would run off the end of the edge; click further in",
        ));
    }
    if 2.0 * half_a < ctx.tool() || 2.0 * half_b < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the slots are narrower than the tool ({} mm); use a smaller tool",
            fmt(ctx.tool())
        )));
    }
    let across = |f: &Frame, r: &[Vec<P2>], s: f64| {
        height_at(f, r, s).ok_or_else(|| {
            CoreError::validation("the picked place is not on the part's edge; pick again")
        })
    };
    let ha = across(&ctx.fa, &ctx.ra, at_a)?;
    let hb = across(&ctx.fb, &ctx.rb, at_b)?;
    // Slot depths: half of each part where it crosses, or as asked (the
    // two always add up to how far apart the picked edges are).
    let da = p.depth.unwrap_or(ha / 2.0);
    let db = p.depth_b.unwrap_or(hb / 2.0);
    if !(da > 0.0 && da < ha && db > 0.0 && db < hb) {
        return Err(CoreError::validation(
            "each slot must be deeper than 0 and shallower than its part",
        ));
    }
    let detent = p.detent.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&detent) {
        return Err(CoreError::validation(
            "the snap bump must be between 0 and 1 mm a side",
        ));
    }
    let slot = |f: &Frame, r: &[Vec<P2>], at: f64, half: f64, depth: f64, what: &str| {
        let cut = f.rect(at - half, at + half, -depth, OVERRUN);
        // A bump each side of the mouth: a circle standing out of the wall
        // by the detent, a little way in.
        let mut add = Vec::new();
        if detent > 0.0 {
            let rho: f64 = 1.5;
            let t = -(rho + 1.0).min(depth / 3.0);
            for (side, s) in [(-1.0, at - half), (1.0, at + half)] {
                add.push(disc(f.at(s + side * (rho - detent), t), rho));
            }
        }
        let cut_out = rewrite(r, &[], &[cut], ctx.r, ctx.relief, f.n, what)?;
        if add.is_empty() {
            return Ok::<Outline, CoreError>(cut_out);
        }
        let bumped = rewrite(&cut_out.rings, &add, &[], 0.0, ReliefShape::None, f.n, what)?;
        Ok(Outline {
            rings: bumped.rings,
            circles: cut_out.circles,
        })
    };
    let resolved = JointParams {
        at_a: Some(at_a),
        at_b: Some(at_b),
        depth: Some(da),
        depth_b: p.depth_b.map(|_| db),
        detent: Some(detent),
        ..JointParams::default()
    };
    let map = Map::axes(
        [(2, 1.0), (1, -1.0), (0, 1.0)],
        [at_a - ctx.tb / 2.0, -(da + db), ctx.ta / 2.0 - at_b],
    );
    let mut plan = base_plan(ctx, resolved, map);
    plan.a = Some(slot(&ctx.fa, &ctx.ra, at_a, half_a, da, "the first part")?);
    plan.b = Some(slot(&ctx.fb, &ctx.rb, at_b, half_b, db, "the second part")?);
    plan.notes
        .push(format!("slots {} and {} mm deep", fmt(da), fmt(db)));
    Ok(plan)
}

/// Half lap at a corner: the ends of two parts laid over each other, each
/// cleared from its top over the other's width, so they meet flush with
/// one turned over.
fn half_lap(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (la, lb) = (ctx.fa.len, ctx.fb.len);
    let c = ctx.c;
    let da = depth_in(p.depth, ctx.ta / 2.0, ctx.ta, "lap")?;
    let db = ctx.tb - (ctx.ta - da);
    if !(db > 0.0 && db < ctx.tb) {
        return Err(CoreError::validation(
            "the parts are too different in thickness for a half lap at this depth",
        ));
    }
    let run = ctx.run_out();
    let pa = ctx.fa.rect(-run, la + run, -(lb + c), run);
    let pb = ctx.fb.rect(-run, lb + run, -(la + c), run);
    for (h, f, r, s) in [
        (lb, &ctx.fa, &ctx.ra, la / 2.0),
        (la, &ctx.fb, &ctx.rb, lb / 2.0),
    ] {
        if height_at(f, r, s).is_none_or(|reach| reach < h + c + ctx.tool()) {
            return Err(CoreError::validation(
                "a half lap joins the ends of two parts: pick an end edge of each, on parts longer than the other is wide",
            ));
        }
    }
    let resolved = JointParams {
        depth: Some(da),
        ..JointParams::default()
    };
    let map = Map::axes(
        [(1, -1.0), (0, -1.0), (2, -1.0)],
        [0.0, 0.0, ctx.ta - da - db + ctx.tb],
    );
    let mut plan = base_plan(ctx, resolved, map);
    plan.vectors.push(Made {
        on: On::A,
        path: arcs_path(&[pa], &[]),
        role: Role::Pocket { depth: da },
        name: "lap".into(),
    });
    plan.vectors.push(Made {
        on: On::B,
        path: arcs_path(&[pb], &[]),
        role: Role::Pocket { depth: db },
        name: "lap".into(),
    });
    plan.notes.push(format!(
        "cleared {} and {} mm deep; turn the second part over",
        fmt(da),
        fmt(db)
    ));
    Ok(plan)
}

/// `poly` (convex) cut down to the side of the line `p . n >= k`.
fn clip_half(poly: &[P2], n: P2, k: f64) -> Vec<P2> {
    let side = |p: P2| p[0] * n[0] + p[1] * n[1] - k;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let f = sp / (sp - sq);
            out.push([p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f]);
        }
    }
    out
}

/// V joint: fingers for two panels meeting at `angle` degrees (between
/// their inside faces; 90 is a box corner), both cut square from the top.
/// Seen end on (A's out of the part `t` and up `z`), A lies flat with its
/// outer (under) face meeting B's outer face at A's edge line; each part's
/// notches run in until they clear all of the other part's finger, so the
/// fingers meet in a V on the outside.
fn v_joint(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let angle = p.angle.unwrap_or(120.0);
    if !(30.0..=170.0).contains(&angle) {
        return Err(CoreError::validation(
            "the angle between the panels must be between 30 and 170 degrees",
        ));
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    // Along B away from the corner, and into B from its outer face.
    let d = [-cos, sin];
    let m = [-sin, -cos];
    let far = 20.0 * (ctx.ta + ctx.tb) + 100.0;
    // B's finger (from its end line through the corner), cut to A's slab.
    let b_finger = [
        [0.0, 0.0],
        [d[0] * far, d[1] * far],
        [d[0] * far + m[0] * ctx.tb, d[1] * far + m[1] * ctx.tb],
        [m[0] * ctx.tb, m[1] * ctx.tb],
    ];
    let in_slab = clip_half(&clip_half(&b_finger, [0.0, 1.0], 0.0), [0.0, -1.0], -ctx.ta);
    let notch_a = in_slab.iter().map(|q| -q[0]).fold(0.0, f64::max);
    // A's finger, cut to B's strip beyond B's end line.
    let a_finger = [[-far, 0.0], [0.0, 0.0], [0.0, ctx.ta], [-far, ctx.ta]];
    let in_strip = clip_half(
        &clip_half(&clip_half(&a_finger, m, 0.0), [-m[0], -m[1]], -ctx.tb),
        d,
        0.0,
    );
    let notch_b = in_strip
        .iter()
        .map(|q| q[0] * d[0] + q[1] * d[1])
        .fold(0.0, f64::max);
    if notch_a < 0.01 || notch_b < 0.01 {
        return Err(CoreError::validation(
            "at this angle the panels hardly overlap; use a larger angle",
        ));
    }
    let la = ctx.fa.len;
    // B's edge frame (along, out, up) to A's: out of B runs back along d,
    // up through B runs along m.
    let map = Map {
        m: [[-1.0, 0.0, 0.0], [0.0, -d[0], m[0]], [0.0, -d[1], m[1]]],
        o: [la, 0.0, 0.0],
    };
    let mut plan = finger_with(
        ctx,
        p,
        Some(&Angled {
            notch_a,
            notch_b,
            map,
        }),
    )?;
    plan.resolved.angle = Some(angle);
    plan.resolved.blind = None;
    plan.notes.push(format!(
        "panels at {} degrees; notches {} and {} mm deep",
        fmt(angle),
        fmt(notch_a),
        fmt(notch_b)
    ));
    Ok(plan)
}

/// Twist lock: the end of A goes through a hole in B where A stands, held
/// across the line, and a quarter turn locks it. A's end keeps a head as
/// wide as the part past B, then a neck (`width` across, B's thickness
/// plus the clearance each side long) that turns in the round middle of
/// the hole; the hole is A's section long across the line with that round
/// middle. Once turned, A stands along the line and its full width holds
/// on both faces of B.
fn twist_lock(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    let (offset, along) = ctx.face();
    let la = ctx.fa.len;
    let c = ctx.c;
    let neck = p.width.unwrap_or(ctx.ta);
    let head = p.proud.unwrap_or(ctx.ta.max(10.0));
    if !(neck > 0.0 && neck < la) {
        return Err(CoreError::validation(format!(
            "the neck must be more than 0 and narrower than the part ({} mm)",
            fmt(la)
        )));
    }
    if head < c + ctx.tool() {
        return Err(CoreError::validation(format!(
            "the head must stand out at least {} mm",
            fmt(c + ctx.tool())
        )));
    }
    let round = (neck * neck + ctx.ta * ctx.ta).sqrt() + 2.0 * c;
    if la < round + 2.0 {
        return Err(CoreError::validation(format!(
            "the first part is {} mm wide; a twist lock needs it at least {} mm wide (or a narrower neck)",
            fmt(la),
            fmt(round + 2.0)
        )));
    }
    let slot = ctx.tb + 2.0 * c;
    if slot < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the second part is too thin for the neck to be cut with this tool ({} mm)",
            fmt(ctx.tool())
        )));
    }
    let (n0, n1) = ((la - neck) / 2.0, (la + neck) / 2.0);
    // The neck stands the clearance off both faces of B.
    let (t_head, t_shoulder) = (-(head - c), -(head + ctx.tb + c));
    let cuts = [
        ctx.fa.rect(-OVERRUN, n0, t_shoulder, t_head),
        ctx.fa.rect(n1, la + OVERRUN, t_shoulder, t_head),
    ];
    let centre_t = -(offset + ctx.ta / 2.0);
    let long = la / 2.0 + c;
    let short = ctx.ta / 2.0 + c;
    let rect = ctx.fb.rect(
        along - short,
        along + short,
        centre_t - long,
        centre_t + long,
    );
    if !rect.iter().all(|q| inside_rings(&ctx.rb, *q)) {
        return Err(CoreError::validation(
            "the hole would run off the second part; move the place",
        ));
    }
    let (rect, mut circles) = super::geometry::relieved_cavity(rect, ctx.r, ctx.relief, ctx.fb.n);
    let centre = ctx.fb.at(along, centre_t);
    let middle = Circle {
        c: centre,
        r: round / 2.0,
    };
    let joined = super::geometry::union(&[rect, disc(centre, middle.r)]);
    let ring = super::geometry::one_shape(joined, "the hole")?
        .into_iter()
        .next()
        .unwrap_or_default();
    circles.push(middle);
    let resolved = JointParams {
        width: Some(neck),
        proud: Some(head),
        ..JointParams::default()
    };
    let mut plan = base_plan(ctx, resolved, tee_map(ctx, ctx.tb + head));
    plan.a = Some(rewrite(
        &ctx.ra,
        &[],
        &cuts,
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?);
    plan.vectors.push(Made {
        on: On::B,
        path: arcs_path(&[ring], &circles),
        role: Role::Hole,
        name: "twist hole".into(),
    });
    plan.notes.push(format!(
        "put the first part through across the line, then turn it a quarter turn; neck {} mm, head {} mm",
        fmt(neck),
        fmt(head)
    ));
    Ok(plan)
}

/// Corner dovetail, cut from the top with an end mill as a locking box
/// corner: tails on A's edge (as long as B is thick, wider at the tip than
/// at the root) and sockets in B. A socket's side walls slope through B's
/// thickness, wider at B's top, which is the outside of the corner: B's
/// outline is notched as narrow as the socket's foot, and each side wall
/// is a tapered wall (see [`crate::cam::taper`]) leaning at the tail angle,
/// its vector running down the wall, round the relief at the back corner
/// and along the back wall, moved as a whole as the wall rises so the back
/// wall stays upright. The walls stand the clearance off the tails square
/// to their faces, and the corner comes apart only one way.
fn corner_dovetail(ctx: &Ctx, p: &JointParams) -> Result<Plan, CoreError> {
    same_length(ctx)?;
    let la = ctx.fa.len;
    let (ta, tb, c) = (ctx.ta, ctx.tb, ctx.c);
    let angle = p.angle.unwrap_or(14.0);
    if !(5.0..=30.0).contains(&angle) {
        return Err(CoreError::validation(
            "the tail angle must be between 5 and 30 degrees",
        ));
    }
    let count = p
        .count
        .unwrap_or_else(|| ((la / (4.0 * ta.max(tb))).round() as u32).max(1));
    if count == 0 {
        return Err(CoreError::validation("a dovetail needs at least one tail"));
    }
    let pitch = la / f64::from(count);
    let flare = tb * angle.to_radians().tan();
    let half_root = pitch / 4.0 - flare / 2.0;
    let half_tip = pitch / 4.0 + flare / 2.0;
    // The clearance across a sloped wall, measured along the edge.
    let side = c / angle.to_radians().cos();
    if 2.0 * half_root + 2.0 * side < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the tails are narrower at the root than the tool ({} mm); use fewer tails or a smaller angle",
            fmt(ctx.tool())
        )));
    }
    if pitch - 2.0 * half_tip - 2.0 * side < ctx.tool() {
        return Err(CoreError::validation(format!(
            "the gaps between the tail tips are narrower than the tool ({} mm); use fewer tails",
            fmt(ctx.tool())
        )));
    }
    // A: everything past the root line cut away, the tails put back.
    let band = ctx.fa.rect(-OVERRUN, la + OVERRUN, -tb, OVERRUN);
    let mut tails = Vec::new();
    for k in 0..count {
        let mid = pitch * (f64::from(k) + 0.5);
        tails.push(ctx.fa.ring(&[
            [mid - half_root, -tb - OVERRUN],
            [mid + half_root, -tb - OVERRUN],
            [mid + half_root, -tb],
            [mid + half_tip, 0.0],
            [mid - half_tip, 0.0],
            [mid - half_root, -tb],
        ]));
    }
    let cut = rewrite(
        &ctx.ra,
        &[],
        &[band],
        0.0,
        ReliefShape::None,
        ctx.fa.n,
        "the first part",
    )?;
    let a = rewrite(
        &cut.rings,
        &tails,
        &[],
        ctx.r,
        ctx.relief,
        ctx.fa.n,
        "the first part",
    )?;
    // B: height z above its underside meets the tail at A's t = z - tb; the
    // socket is as wide as the tail at its root plus the clearance at B's
    // underside, the foot of its sloped walls.
    let foot = half_root + side;
    let run = ctx.run_out();
    let far = -(ta + c);
    let mut through = Vec::new();
    let mut walls = Vec::new();
    for k in 0..count {
        let mid = pitch * (f64::from(k) + 0.5);
        let socket = ctx.fb.rect(mid - foot, mid + foot, far, run);
        let reliefs: Vec<Circle> = cavity_reliefs(&socket, ctx.r, ctx.relief, ctx.fb.n)
            .into_iter()
            .filter(|circle| inside_rings(&ctx.rb, circle.c))
            .collect();
        through.push(ctx.fb.rect(mid - foot, mid + foot, far, OVERRUN));
        // Along the back wall past the relief, not quite to the other wall.
        let back = (2.0 * foot - ctx.r).clamp(0.0, 2.0 * ctx.r);
        for (x, into) in [(mid - foot, 1.0), (mid + foot, -1.0)] {
            let top = ctx.fb.at(x, run);
            let corner = ctx.fb.at(x, far);
            let end = ctx.fb.at(x + into * back, far);
            let relief = reliefs
                .iter()
                .min_by(|a, b| length(sub(a.c, corner)).total_cmp(&length(sub(b.c, corner))))
                .filter(|circle| length(sub(circle.c, corner)) < 2.0 * circle.r);
            let mut points = vec![top];
            let mut bulge = None;
            match relief {
                Some(circle) => {
                    // Where the circle meets each wall line again, and the
                    // arc between round the far side of the corner.
                    let meet = |dir: P2| {
                        let d = unit(dir);
                        add(corner, scale(d, -2.0 * dot(d, sub(corner, circle.c))))
                    };
                    let p1 = meet(sub(top, corner));
                    let p2 = meet(sub(end, corner));
                    let sweep = turn(circle.c, p1, corner) + turn(circle.c, corner, p2);
                    points.push(p1);
                    bulge = Some((points.len(), p2, (sweep / 4.0).tan()));
                    if length(sub(end, p2)) > 1e-6 && dot(sub(end, p2), sub(end, corner)) > 0.0 {
                        points.push(end);
                    }
                }
                None => {
                    points.push(corner);
                    points.push(end);
                }
            }
            let path = open_path(&points, bulge);
            // The right wall runs the other way, so the socket is on the
            // same side of both.
            let path = if into < 0.0 {
                crate::geom::edit::reverse(&path)
            } else {
                path
            };
            walls.push(path);
        }
    }
    let b = rewrite(
        &ctx.rb,
        &[],
        &through,
        ctx.r,
        ctx.relief,
        ctx.fb.n,
        "the second part",
    )?;
    let resolved = JointParams {
        count: Some(count),
        angle: Some(angle),
        ..JointParams::default()
    };
    // B's along runs with A's; B's top is the outside of the corner.
    let map = Map::axes([(0, 1.0), (2, -1.0), (1, 1.0)], [0.0, -tb, 0.0]);
    let mut plan = base_plan(ctx, resolved, map);
    plan.a = Some(a);
    plan.b = Some(b);
    for (k, path) in walls.into_iter().enumerate() {
        plan.vectors.push(Made {
            on: On::B,
            path,
            role: Role::Taper { depth: tb, angle },
            name: format!("socket wall {}", k + 1),
        });
    }
    plan.notes.push(format!(
        "{count} tails at {} degrees; the socket walls lean at the tail angle, cut in fine steps with the second part's outside face up",
        fmt(angle)
    ));
    Ok(plan)
}

/// Signed angle from `p` to `q` about `c`, counter-clockwise positive.
fn turn(c: P2, p: P2, q: P2) -> f64 {
    let (a, b) = (sub(p, c), sub(q, c));
    cross2(a, b).atan2(dot(a, b))
}

/// An open path through `points`, with an arc of `bulge` from the point at
/// index `at - 1` to `to` inserted before index `at` when given.
fn open_path(points: &[P2], arc: Option<(usize, P2, f64)>) -> Path {
    use crate::geom::{Point, SubPath};
    let mut sub_path = SubPath::open(Point::new(points[0][0], points[0][1]));
    for (i, p) in points.iter().enumerate().skip(1) {
        if let Some((at, to, bulge)) = arc {
            if i == at {
                sub_path.arc_to(Point::new(to[0], to[1]), bulge);
            }
        }
        sub_path.line_to(Point::new(p[0], p[1]));
    }
    if let Some((at, to, bulge)) = arc {
        if at == points.len() {
            sub_path.arc_to(Point::new(to[0], to[1]), bulge);
        }
    }
    Path {
        subpaths: vec![sub_path],
    }
}

/// A cavity relieved only at the corners that lie inside the part's
/// material (corners out past the edge need none), as a path with its
/// reliefs as arcs.
fn inside_relieved(ring: Vec<P2>, ctx: &Ctx, part: &[Vec<P2>], along: P2) -> Path {
    let circles: Vec<_> = cavity_reliefs(&ring, ctx.r, ctx.relief, along)
        .into_iter()
        .filter(|c| inside_rings(part, c.c))
        .collect();
    let ring = with_circles(ring, &circles);
    arcs_path(&[ring], &circles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_fill_the_edge_with_the_first_one_wider() {
        let ends = segments(100.0, 5, 0.0);
        assert_eq!(ends.len(), 6);
        assert!((ends[1] - 20.0).abs() < 1e-9);
        let biased = segments(100.0, 5, 0.5);
        assert!((biased[1] - 100.0 / 5.5 * 1.5).abs() < 1e-9);
        assert!((biased[5] - 100.0).abs() < 1e-12);
        assert_eq!(odd(4.0, 3), 5);
        assert_eq!(odd(1.0, 3), 3);
    }
}
