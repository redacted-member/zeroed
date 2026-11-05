//! Verify Fit for the Assembly View: every joint between placed pieces
//! with its clearance per side, measured on the pieces' exact solids (see
//! [`super::exact`]), and how far each piece moves in the exploded view.
//!
//! A joint is where faces of two pieces face each other within a few
//! millimetres: a tab in a slot, a panel in a dado, an edge on a face. Its
//! faces are grouped into sides by the way they face; a side with a side
//! facing the other way in the same joint is a fit (the two walls of a
//! slot round a tab) and is judged against the target clearance, a side
//! alone is a resting contact (an edge on a face) and only counts when the
//! pieces run into each other. Interference is found apart from the facing
//! faces too: the edges of each piece are sampled inside the other, so a
//! square tab corner running into the round inside corner of a slot shows.
//! Pieces with no exact solid (a raw model, a component, cuts that cannot
//! be made exact) are checked on the simulation grid instead, and say so.
//!
//! Round walls (a pin, a dowel, a drilled or round cut hole) are measured
//! as circles, not facet by facet: a round pin in a round hole is one fit
//! side whose gap runs from the radii's difference less the axes' offset
//! to that difference plus it, however the facets of the two lie. Each
//! joint is one contact region (a tab and its slot, a pin and its hole),
//! and so is each place where the pieces run into each other, as an
//! interference check lists each region.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cam::sim::{Quality, Stock};
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

use super::exact::{self, ExactSolid, Shapes};
use super::mate_faces;
use super::pieces::{self, PartsCache, PieceRef, PlacedRelief};
use super::pose::{self, Pose, Vec3};
use super::triangulate::triangulate;

/// Faces further apart than this are not a joint, mm.
const REACH: f64 = 3.0;

/// Faces must face each other to within this many degrees.
const FACING_DEGREES: f64 = 1.0;

/// Faces more out of parallel than this (a facet of a round corner relief
/// that happens to lie within [`FACING_DEGREES`] of a fit face) still
/// belong to the side they face, but do not set its largest gap, degrees.
const PARALLEL_DEGREES: f64 = 0.05;

/// Faces overlapping less than this are not a joint, square mm.
const MIN_AREA: f64 = 0.01;

/// Contacts of one joint lie within this of each other, mm.
const JOIN: f64 = 1.0;

/// Material closer than this to the other piece's surface is touching,
/// not running into it, mm.
const TOUCH: f64 = 0.001;

/// Spacing of the samples taken along each piece's edges, mm.
const EDGE_STEP: f64 = 0.1;

/// Most samples taken along one piece's edges for one other piece.
const MAX_EDGE_SAMPLES: usize = 400_000;

/// Samples along each side of the box an overlap is measured in.
const VOLUME_SAMPLES: usize = 40;

/// How far past the target's limits a gap may be and still count as in
/// them, mm: the solids are single precision, so a gap cut at exactly the
/// limit measures a fraction of a micron either side of it.
const JUDGE_SLACK: f64 = 1e-4;

/// How far apart two fits on one face may be and still be one joint, mm:
/// about a tool's relief, less than the space between two tabs.
const SHARED_JOIN: f64 = 3.0;

/// Sides within this many degrees of each other are one side.
const SIDE_DEGREES: f64 = 10.0;

/// Two walls leaning apart by up to this many degrees still hold the
/// material between them: the sides of a dovetail's tail (twice its angle).
const WEDGE_DEGREES: f64 = 62.0;

/// How a joint or one of its faces fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FitStatus {
    /// Clearance within the target and tolerance (or a resting contact).
    Ok,
    /// More clearance than the target plus the tolerance.
    Loose,
    /// Less clearance than the target less the tolerance.
    Tight,
    /// The pieces run into each other.
    Interference,
}

/// The clearance wanted in the fits, mm.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitTarget {
    /// Gap wanted on each side of a fit.
    pub target: f64,
    /// How far the gap may stray from the target either way.
    pub tolerance: f64,
}

impl FitTarget {
    /// How a fit whose smallest gap is `min` fits: the tightest place
    /// decides, so a relief or chamfer further out does not make it loose.
    fn judge(&self, min: f64) -> FitStatus {
        if min < -TOUCH {
            FitStatus::Interference
        } else if min < self.target - self.tolerance - JUDGE_SLACK {
            FitStatus::Tight
        } else if min > self.target + self.tolerance + JUDGE_SLACK {
            FitStatus::Loose
        } else {
            FitStatus::Ok
        }
    }
}

/// One way a joint's faces face, with the gaps across it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JointSide {
    /// Normal of the first piece's faces there, pointing at the second
    /// piece, assembly space.
    pub normal: Vec3,
    /// Smallest and largest gap, mm; negative where they overlap.
    pub min: f64,
    /// Largest gap, mm.
    pub max: f64,
    /// Area of the faces facing each other, square mm.
    pub area: f64,
    /// Whether a side facing the other way belongs to the joint too (a
    /// fit, judged against the target), else a resting contact.
    pub fit: bool,
    /// How the side fits.
    pub status: FitStatus,
    /// A round pin in a round hole: the gap runs all round the axis
    /// (`normal`), from `min` where the axes' offset closes it to `max`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub round: bool,
}

/// A patch of two faces facing each other, to draw: triangles in each
/// piece's own frame (x, y, z per corner).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitPatch {
    /// How it fits.
    pub status: FitStatus,
    /// Smallest gap across it, mm.
    pub min: f64,
    /// Largest gap across it, mm.
    pub max: f64,
    /// On the first piece.
    pub a: Vec<f32>,
    /// On the second piece.
    pub b: Vec<f32>,
}

/// Where the fit was measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FitSource {
    /// The pieces' exact solids.
    Exact,
    /// The toolpath simulation grid.
    Grid,
}

/// A joint between two pieces.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Joint {
    /// First piece.
    pub a: PieceRef,
    /// Second piece.
    pub b: PieceRef,
    /// The worst of its sides and any interference.
    pub status: FitStatus,
    /// What it was measured on.
    pub source: FitSource,
    /// The grid's cell size for a grid measurement, mm.
    pub cell: Option<f64>,
    /// Its sides, largest first.
    pub sides: Vec<JointSide>,
    /// Smallest gap over its fits (or its contacts when it has no fit).
    pub min: Option<f64>,
    /// Largest gap over the same faces.
    pub max: Option<f64>,
    /// How far the pieces run into each other, mm.
    pub overlap_depth: f64,
    /// How much material they share, cubic mm.
    pub overlap_volume: f64,
    /// Box round the joint in assembly space, min then max.
    pub bounds: [Vec3; 2],
    /// The faces to draw.
    pub patches: Vec<FitPatch>,
}

/// How far a piece moves at a full explode, assembly space.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplodeOffset {
    /// The piece.
    pub piece: PieceRef,
    /// Its move, mm.
    pub offset: Vec3,
}

/// A piece as the fit sees it.
pub struct FitPiece<'a> {
    /// Which piece.
    pub key: PieceRef,
    /// Local frame to assembly space.
    pub pose: Pose,
    /// Its exact solid, when it has one.
    pub solid: Option<&'a ExactSolid>,
    /// Its relief on the simulation grid.
    pub relief: &'a Stock,
    /// Its part box: width, height, thickness.
    pub size: Vec3,
}

/// A planar face: `o` plus `x u + y v` for each (x, y) of its shapes.
#[derive(Debug, Clone)]
pub(crate) struct Face {
    pub(crate) o: Vec3,
    pub(crate) u: Vec3,
    pub(crate) v: Vec3,
    /// Outward normal.
    pub(crate) n: Vec3,
    pub(crate) shapes: Shapes,
    /// The ring a wall face runs along (numbered as [`round_rings`] numbers
    /// them), `usize::MAX` for a level face.
    pub(crate) ring: usize,
    /// Which edge of its ring a wall face is (its first corner's index).
    pub(crate) edge: usize,
}

impl Face {
    pub(crate) fn at(&self, p: [f64; 2]) -> Vec3 {
        pose::add(
            self.o,
            pose::add(pose::scale(self.u, p[0]), pose::scale(self.v, p[1])),
        )
    }

    fn moved(&self, pose: &Pose) -> Face {
        Face {
            o: pose.apply(self.o),
            u: pose.apply_vector(self.u),
            v: pose.apply_vector(self.v),
            n: pose.apply_vector(self.n),
            shapes: self.shapes.clone(),
            ring: self.ring,
            edge: self.edge,
        }
    }

    fn bounds(&self) -> [Vec3; 2] {
        let mut b = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        for p in self.shapes.iter().flat_map(|s| s.first()).flatten() {
            grow(&mut b, self.at(*p));
        }
        b
    }
}

fn grow(b: &mut [Vec3; 2], p: Vec3) {
    for k in 0..3 {
        b[0][k] = b[0][k].min(p[k]);
        b[1][k] = b[1][k].max(p[k]);
    }
}

fn boxes_meet(a: &[Vec3; 2], b: &[Vec3; 2], margin: f64) -> bool {
    (0..3).all(|k| a[0][k] <= b[1][k] + margin && b[0][k] <= a[1][k] + margin)
}

/// The faces of an exact solid in its own frame: the walls of each slab,
/// its underside, its top and the floors where recesses stop.
pub(crate) fn faces_of(solid: &ExactSolid) -> Vec<Face> {
    solid_faces(solid, false)
}

/// The faces of an exact solid as fits are measured on them: as
/// [`faces_of`], but a tapered wall is its leaning faces rather than the
/// terraces under it.
pub(crate) fn fit_faces(solid: &ExactSolid) -> Vec<Face> {
    solid_faces(solid, true)
}

/// How far a terrace's wall may stand from a leaning face and still be
/// part of that tapered wall, mm.
const LEANING_REACH: f64 = exact::TERRACE_LEAN + 1e-4;

fn solid_faces(solid: &ExactSolid, leaning: bool) -> Vec<Face> {
    let mut out = Vec::new();
    let mut ring_id = 0;
    let leans: Vec<Face> = if leaning {
        solid
            .leaning
            .iter()
            .filter_map(|l| wall_face(l.bottom, l.top, l.z[0], l.z[1], usize::MAX - 1, 0))
            .collect()
    } else {
        Vec::new()
    };
    // Whether a terrace wall between two heights is part of a leaning face.
    let under = |a: [f64; 2], b: [f64; 2], z0: f64, z1: f64| {
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (z0 + z1) / 2.0];
        leans.iter().any(|f| {
            let w = pose::sub(m, f.o);
            let off = pose::dot(w, f.n);
            if off.abs() > LEANING_REACH {
                return false;
            }
            let flat = [pose::dot(w, f.u), pose::dot(w, f.v)];
            exact::contains(&f.shapes, flat)
                || exact::edge_distance(&f.shapes, flat) < LEANING_REACH
        })
    };
    // Walls of terraces that stand one on another are one face: where a
    // terrace's wall ends, keyed by its edge.
    let key = |a: [f64; 2], b: [f64; 2]| {
        let q = |v: f64| (v * 1e6).round() as i64;
        [q(a[0]), q(a[1]), q(b[0]), q(b[1])]
    };
    let mut stacked: BTreeMap<[i64; 4], (usize, f64)> = BTreeMap::new();
    for (k, section) in solid.sections.iter().enumerate() {
        let (z0, z1) = (solid.levels[k], solid.levels[k + 1]);
        let terrace = leaning && solid.terraced.get(k).copied().unwrap_or(false);
        for ring in section.iter().flat_map(|s| s.iter()) {
            let id = ring_id;
            ring_id += 1;
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                if terrace && under(a, b, z0, z1) {
                    continue;
                }
                if terrace {
                    if let Some((at, top)) = stacked.get_mut(&key(a, b)) {
                        if (*top - z0).abs() < 1e-9 {
                            let face: &mut Face = &mut out[*at];
                            let h = z1 - face.o[2];
                            if let Some(ring) = face.shapes.first_mut().and_then(|s| s.first_mut())
                            {
                                ring[2][1] = h;
                                ring[3][1] = h;
                            }
                            *top = z1;
                            continue;
                        }
                    }
                }
                if let Some(face) = wall_face([a, b], [a, b], z0, z1, id, i) {
                    if terrace {
                        stacked.insert(key(a, b), (out.len(), z1));
                    }
                    out.push(face);
                }
            }
        }
    }
    // Leaning faces take ring numbers of their own, one each, so no two
    // are taken for the walls of one ring.
    for (j, mut face) in leans.into_iter().enumerate() {
        face.ring = ring_id + j;
        out.push(face);
    }
    let flat = |z: f64, up: bool, shapes: Shapes| Face {
        o: [0.0, 0.0, z],
        u: [1.0, 0.0, 0.0],
        v: [0.0, 1.0, 0.0],
        n: [0.0, 0.0, if up { 1.0 } else { -1.0 }],
        shapes,
        ring: usize::MAX,
        edge: usize::MAX,
    };
    if let Some(first) = solid.sections.first() {
        out.push(flat(0.0, false, first.clone()));
    }
    for k in 1..solid.sections.len() {
        let (below, above) = (&solid.sections[k - 1], &solid.sections[k]);
        let floor = exact::difference(below, above);
        if !floor.is_empty() {
            out.push(flat(solid.levels[k], true, floor));
        }
        let ceiling = exact::difference(above, below);
        if !ceiling.is_empty() {
            out.push(flat(solid.levels[k], false, ceiling));
        }
    }
    if let Some(last) = solid.sections.last() {
        out.push(flat(solid.thickness, true, last.clone()));
    }
    out
}

/// The wall of a slab along one ring edge: `bottom` (its ends at `z0`)
/// up to `top` (the same edge at `z1`), upright or leaning; `None` for an
/// edge of no length.
fn wall_face(
    bottom: [[f64; 2]; 2],
    top: [[f64; 2]; 2],
    z0: f64,
    z1: f64,
    ring: usize,
    edge: usize,
) -> Option<Face> {
    let [a, b] = bottom;
    let [a1, b1] = top;
    let (mut ex, mut ey) = (b[0] - a[0], b[1] - a[1]);
    if ex.hypot(ey) < 1e-9 {
        (ex, ey) = (b1[0] - a1[0], b1[1] - a1[1]);
    }
    let len = ex.hypot(ey);
    if len < 1e-9 {
        return None;
    }
    let u = [ex / len, ey / len, 0.0];
    let o = [a[0], a[1], z0];
    if a == a1 && b == b1 {
        let h = z1 - z0;
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len < 1e-9 {
            return None;
        }
        return Some(Face {
            o,
            u,
            v: [0.0, 0.0, 1.0],
            n: [u[1], -u[0], 0.0],
            shapes: vec![vec![vec![[0.0, 0.0], [len, 0.0], [len, h], [0.0, h]]]],
            ring,
            edge,
        });
    }
    // Leaning: up the face from the bottom edge's middle to the top's.
    let up = [
        (a1[0] + b1[0] - a[0] - b[0]) / 2.0,
        (a1[1] + b1[1] - a[1] - b[1]) / 2.0,
        z1 - z0,
    ];
    let n = pose::normalized(pose::cross(u, up));
    let v = pose::cross(n, u);
    let flat = |p: [f64; 2], z: f64| {
        let w = pose::sub([p[0], p[1], z], o);
        [pose::dot(w, u), pose::dot(w, v)]
    };
    let mut ring_uv = vec![flat(a, z0), flat(b, z0), flat(b1, z1), flat(a1, z1)];
    ring_uv.dedup_by(|p, q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-9);
    if ring_uv.len() < 3 || exact::ring_area(&ring_uv).abs() < 1e-12 {
        return None;
    }
    if exact::ring_area(&ring_uv) < 0.0 {
        ring_uv.reverse();
    }
    Some(Face {
        o,
        u,
        v,
        n,
        shapes: vec![vec![ring_uv]],
        ring,
        edge,
    })
}

/// A round wall of an exact solid: a ring whose corners lie on a circle,
/// or an arc of one (the round end of a slot or a tab), over the heights of
/// the slabs it runs through.
#[derive(Debug, Clone)]
pub(crate) struct RoundRing {
    /// The rings' numbers, as [`Face::ring`] holds them, with the arc on
    /// each and how many corners the ring has.
    pub(crate) ids: Vec<(usize, mate_faces::RingArc, usize)>,
    pub(crate) centre: [f64; 2],
    pub(crate) radius: f64,
    /// Material round the centre (a pin), else away from it (a hole).
    pub(crate) pin: bool,
    /// The arc as it runs in the lowest slab.
    pub(crate) arc: mate_faces::RingArc,
    /// From the bottom of the lowest slab to the top of the highest.
    pub(crate) z: [f64; 2],
    /// One ring's corners along the arc, to draw it.
    pub(crate) corners: Vec<[f64; 2]>,
}

impl RoundRing {
    /// Whether the wall face on edge `edge` of ring `ring` is part of it.
    fn has(&self, ring: usize, edge: usize) -> bool {
        self.ids
            .iter()
            .any(|(id, arc, n)| *id == ring && arc.has_edge(edge, *n))
    }
}

/// The round walls of an exact solid, one per circle: the same circle in
/// slabs one above another is one wall.
pub(crate) fn round_rings(solid: &ExactSolid) -> Vec<RoundRing> {
    let mut out: Vec<RoundRing> = Vec::new();
    let mut ring_id = 0;
    for (k, section) in solid.sections.iter().enumerate() {
        let (z0, z1) = (solid.levels[k], solid.levels[k + 1]);
        for ring in section.iter().flat_map(|s| s.iter()) {
            let id = ring_id;
            ring_id += 1;
            for arc in mate_faces::ring_arcs(ring) {
                let (centre, radius, pin) = (arc.centre, arc.radius, arc.pin());
                let same = out.iter_mut().find(|r| {
                    r.pin == pin
                        && (r.radius - radius).abs() < 1e-6
                        && (r.centre[0] - centre[0]).hypot(r.centre[1] - centre[1]) < 1e-6
                        && (r.z[1] - z0).abs() < 1e-9
                });
                match same {
                    Some(r) => {
                        r.ids.push((id, arc, ring.len()));
                        r.z[1] = z1;
                    }
                    None => out.push(RoundRing {
                        ids: vec![(id, arc, ring.len())],
                        centre,
                        radius,
                        pin,
                        arc,
                        z: [z0, z1],
                        corners: arc.corners(ring),
                    }),
                }
            }
        }
    }
    out
}

/// The wall of a round ring between two heights, as triangles in the
/// solid's own frame.
fn ring_wall(corners: &[[f64; 2]], closed: bool, z0: f64, z1: f64) -> Vec<f32> {
    let mut out = Vec::with_capacity(corners.len() * 18);
    let n = corners.len();
    let edges = if closed { n } else { n.saturating_sub(1) };
    for i in 0..edges {
        let p = corners[i];
        let q = corners[(i + 1) % n];
        for c in [
            [p[0], p[1], z0],
            [q[0], q[1], z0],
            [q[0], q[1], z1],
            [p[0], p[1], z0],
            [q[0], q[1], z1],
            [p[0], p[1], z1],
        ] {
            out.extend(c.iter().map(|v| *v as f32));
        }
    }
    out
}

/// A round fit found between two pieces: which rings, and the contact.
struct RoundFit {
    ring_a: RoundRing,
    ring_b: RoundRing,
    contact: Contact,
}

/// Points taken round a pin's arc to measure a gap to a hole's arc.
const ARC_SAMPLES: usize = 360;

/// The gap from a pin's arc to a hole's arc facing it, where they face each
/// other: least and most, and the share of the pin's arc that faces the
/// hole's. `pin_at` takes a point of the pin's frame into the hole's.
fn arc_gaps(
    pin: &RoundRing,
    hole: &RoundRing,
    pin_z: f64,
    pin_at: &dyn Fn(Vec3) -> Vec3,
) -> Option<(f64, f64, f64)> {
    let (mut lo, mut hi, mut facing) = (f64::INFINITY, f64::NEG_INFINITY, 0);
    for k in 0..=ARC_SAMPLES {
        let t = pin.arc.start + pin.arc.sweep * k as f64 / ARC_SAMPLES as f64;
        let p = pin_at([
            pin.centre[0] + pin.radius * t.cos(),
            pin.centre[1] + pin.radius * t.sin(),
            pin_z,
        ]);
        let (dx, dy) = (p[0] - hole.centre[0], p[1] - hole.centre[1]);
        if !hole.arc.covers(dy.atan2(dx)) {
            continue;
        }
        let gap = hole.radius - dx.hypot(dy);
        lo = lo.min(gap);
        hi = hi.max(gap);
        facing += 1;
    }
    (facing >= 3).then(|| (lo, hi, facing as f64 / (ARC_SAMPLES + 1) as f64))
}

/// Every pin of one piece in a hole of the other (either way round) whose
/// axes run parallel and within reach: the gap all round from the radii
/// and the axes' offset. A round end (a tab's in a slot's, or a pin in a
/// round-ended slot) is a pin or a hole too, measured where the arcs face
/// each other.
fn round_fits(a: &FitPiece, sa: &ExactSolid, b: &FitPiece, sb: &ExactSolid) -> Vec<RoundFit> {
    let (rings_a, rings_b) = (round_rings(sa), round_rings(sb));
    let mut out = Vec::new();
    if rings_a.is_empty() || rings_b.is_empty() {
        return out;
    }
    let ua = pose::normalized(a.pose.apply_vector([0.0, 0.0, 1.0]));
    let ub = pose::normalized(b.pose.apply_vector([0.0, 0.0, 1.0]));
    let s = pose::dot(ua, ub);
    if s.abs() < FACING_DEGREES.to_radians().cos() {
        return out;
    }
    for ra in &rings_a {
        for rb in &rings_b {
            if ra.pin == rb.pin {
                continue;
            }
            let (pin_r, hole_r) = if ra.pin {
                (ra.radius, rb.radius)
            } else {
                (rb.radius, ra.radius)
            };
            let pa = a.pose.apply([ra.centre[0], ra.centre[1], ra.z[0]]);
            let pb0 = b.pose.apply([rb.centre[0], rb.centre[1], rb.z[0]]);
            let w = pose::sub(pb0, pa);
            let off = pose::sub(w, pose::scale(ua, pose::dot(w, ua)));
            let d = pose::dot(off, off).sqrt();
            if d + pin_r > hole_r + REACH || hole_r - pin_r > REACH {
                continue;
            }
            // Where the walls share height along the axis.
            let ta = pose::dot(pa, ua);
            let (tb0, tb1) = (
                pose::dot(pb0, ua),
                pose::dot(b.pose.apply([rb.centre[0], rb.centre[1], rb.z[1]]), ua),
            );
            let lo = ta.max(tb0.min(tb1));
            let hi = (ta + ra.z[1] - ra.z[0]).min(tb0.max(tb1));
            if hi - lo < 0.01 {
                continue;
            }
            let za = [ra.z[0] + (lo - ta), ra.z[0] + (hi - ta)];
            let zb = [rb.z[0] + (lo - tb0) / s, rb.z[0] + (hi - tb0) / s];
            let (zb0, zb1) = (zb[0].min(zb[1]), zb[0].max(zb[1]));
            let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
            let mut near = Vec::new();
            for c in &ra.corners {
                for z in za {
                    let p = a.pose.apply([c[0], c[1], z]);
                    grow(&mut bounds, p);
                    near.push(p);
                }
            }
            let mut far = Vec::new();
            for c in &rb.corners {
                for z in [zb0, zb1] {
                    let p = b.pose.apply([c[0], c[1], z]);
                    grow(&mut bounds, p);
                    far.push(p);
                }
            }
            let gap = hole_r - pin_r;
            // Whole circles: the gap runs from the radii's difference less
            // the axes' offset to it plus the offset. An arc (a round end)
            // is measured round where the two face each other.
            let (min, max, share) = if ra.arc.closed() && rb.arc.closed() {
                (gap - d, gap + d, 1.0)
            } else {
                let from_a = |p: Vec3| b.pose.inverse().apply(a.pose.apply(p));
                let from_b = |p: Vec3| a.pose.inverse().apply(b.pose.apply(p));
                let measured = if ra.pin {
                    arc_gaps(ra, rb, (za[0] + za[1]) / 2.0, &from_a)
                } else {
                    arc_gaps(rb, ra, (zb0 + zb1) / 2.0, &from_b)
                };
                let Some(found) = measured else {
                    continue;
                };
                found
            };
            let pin_sweep = if ra.pin { ra.arc.sweep } else { rb.arc.sweep };
            out.push(RoundFit {
                ring_a: ra.clone(),
                ring_b: rb.clone(),
                contact: Contact {
                    face: usize::MAX,
                    other: usize::MAX,
                    normal: ua,
                    overlap: Vec::new(),
                    area: pin_sweep.abs() * share * pin_r * (hi - lo),
                    min,
                    max,
                    bounds,
                    far: vec![far],
                    near,
                    round: Some((
                        ring_wall(&ra.corners, ra.arc.closed(), za[0], za[1]),
                        ring_wall(&rb.corners, rb.arc.closed(), zb0, zb1),
                    )),
                    tilted: false,
                },
            });
        }
    }
    out
}

/// Two faces facing each other across a gap.
#[derive(Debug, Clone)]
struct Contact {
    /// The first piece's face, assembly space.
    face: usize,
    /// The second piece's face.
    other: usize,
    /// The first face's normal.
    normal: Vec3,
    /// Where they overlap, in the first face's coordinates.
    overlap: Shapes,
    area: f64,
    min: f64,
    max: f64,
    /// The overlap and its mirror across the gap, assembly space.
    bounds: [Vec3; 2],
    /// Points of the overlap on the second face, one list per ring.
    far: Vec<Vec<Vec3>>,
    /// Points of the overlap on the first face.
    near: Vec<Vec3>,
    /// A round fit (see [`round_fits`]): the walls to draw on each piece,
    /// in its own frame.
    round: Option<(Vec<f32>, Vec<f32>)>,
    /// The two faces are out of parallel by more than [`PARALLEL_DEGREES`].
    tilted: bool,
}

/// Every pair of faces of `a` and `b` (assembly space) facing each other
/// within reach.
fn contacts(a: &[Face], b: &[Face]) -> Vec<Contact> {
    let facing = FACING_DEGREES.to_radians().cos();
    let boxes_b: Vec<[Vec3; 2]> = b.iter().map(Face::bounds).collect();
    let mut out = Vec::new();
    for (i, fa) in a.iter().enumerate() {
        let box_a = fa.bounds();
        for (j, (fb, box_b)) in b.iter().zip(&boxes_b).enumerate() {
            let cos = pose::dot(fa.n, fb.n);
            if cos > -facing || !boxes_meet(&box_a, box_b, REACH) {
                continue;
            }
            let gap = pose::dot(pose::sub(fb.o, fa.o), fa.n);
            if gap.abs() > REACH + 1e-9 {
                continue;
            }
            let projected: Shapes = fb
                .shapes
                .iter()
                .map(|shape| {
                    shape
                        .iter()
                        .map(|ring| {
                            ring.iter()
                                .map(|p| {
                                    let w = pose::sub(fb.at(*p), fa.o);
                                    [pose::dot(w, fa.u), pose::dot(w, fa.v)]
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect();
            // Each separate area of the overlap is a contact of its own, so
            // one long face over two tabs gives each tab its own.
            for shape in exact::intersection(&fa.shapes, &projected) {
                let overlap: Shapes = vec![shape];
                let area = exact::shapes_area(&overlap);
                if area < MIN_AREA {
                    continue;
                }
                let mut min = f64::INFINITY;
                let mut max = f64::NEG_INFINITY;
                let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
                let mut far = Vec::new();
                let mut near = Vec::new();
                for ring in overlap.iter().flat_map(|s| s.iter()) {
                    let mut points = Vec::with_capacity(ring.len());
                    for p in ring {
                        let at = fa.at(*p);
                        let t = pose::dot(pose::sub(fb.o, at), fb.n) / cos;
                        min = min.min(t);
                        max = max.max(t);
                        let there = pose::add(at, pose::scale(fa.n, t));
                        grow(&mut bounds, at);
                        grow(&mut bounds, there);
                        near.push(at);
                        points.push(there);
                    }
                    far.push(points);
                }
                out.push(Contact {
                    face: i,
                    other: j,
                    normal: fa.n,
                    overlap,
                    area,
                    min,
                    max,
                    bounds,
                    far,
                    near,
                    round: None,
                    tilted: -cos < PARALLEL_DEGREES.to_radians().cos(),
                });
            }
        }
    }
    out
}

fn box_distance(a: &[Vec3; 2], b: &[Vec3; 2]) -> f64 {
    (0..3)
        .map(|k| (a[0][k] - b[1][k]).max(b[0][k] - a[1][k]).max(0.0).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn join(parent: &mut [usize], i: usize, j: usize) {
    let (ri, rj) = (root(parent, i), root(parent, j));
    if ri != rj {
        parent[rj.max(ri)] = ri.min(rj);
    }
}

/// The contacts' joints and which contacts are fits. A contact is a fit
/// when a contact facing the other way (or leaning away from it by up to
/// [`WEDGE_DEGREES`], as the two sides of a dovetail do) lies across one
/// piece's material from it and overlaps it seen along its normal: the two
/// walls of a slot round a tab, the two sides of a tail in its socket. The
/// two are one joint. Of the walls across, the one that bounds the same
/// feature wins: both walls on one ring of each piece (a tab's two sides in
/// one slot, not two tabs' sides across the web between two slots), then
/// the nearest. Fits within [`JOIN`] of each other are one joint (the four
/// walls round a square tab), resting contacts likewise, and each group of
/// resting contacts joins the nearest fit, so a shoulder goes with its tab
/// but does not tie two tabs together.
fn clusters(
    contacts: &[Contact],
    faces: &[Face],
    faces_b: &[Face],
) -> (Vec<Vec<usize>>, Vec<bool>) {
    let n = contacts.len();
    let wedge_cos = WEDGE_DEGREES.to_radians().cos();
    let flat_box = |c: &Contact, face: &Face| {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for p in &c.near {
            let w = pose::sub(*p, face.o);
            let q = [pose::dot(w, face.u), pose::dot(w, face.v)];
            lo = [lo[0].min(q[0]), lo[1].min(q[1])];
            hi = [hi[0].max(q[0]), hi[1].max(q[1])];
        }
        (lo, hi)
    };
    let ring_of = |list: &[Face], k: usize| list.get(k).map_or(usize::MAX, |f| f.ring);
    let mut parent: Vec<usize> = (0..n).collect();
    // A round fit is a fit on its own: its gap runs all round.
    let mut fit: Vec<bool> = contacts.iter().map(|c| c.round.is_some()).collect();
    for i in 0..n {
        if contacts[i].round.is_some() {
            continue;
        }
        let face = &faces[contacts[i].face];
        let (lo, hi) = flat_box(&contacts[i], face);
        // The wall across: in front of this one (the other piece inside
        // this one's slot) or behind it (this piece inside the other's).
        let mut best: Option<(usize, f64, usize)> = None;
        for (j, other) in contacts.iter().enumerate() {
            if j == i
                || other.round.is_some()
                || pose::dot(other.normal, contacts[i].normal) > -wedge_cos
            {
                continue;
            }
            let (olo, ohi) = flat_box(other, face);
            let Some(across) = other
                .near
                .first()
                .map(|p| pose::dot(pose::sub(*p, face.o), face.n))
            else {
                continue;
            };
            // Walls leaning apart meet seen along this one's normal only
            // after the lean across the gap between them: a tail's far
            // side lies further along it by the gap times the lean.
            let facing = (-pose::dot(other.normal, contacts[i].normal)).clamp(1e-9, 1.0);
            let lean = across.abs() * (1.0 - facing * facing).sqrt() / facing;
            let meet = (0..2).all(|k| {
                let slack = if k == 0 { lean } else { 0.0 };
                olo[k] < hi[k] + slack - 1e-6 && lo[k] < ohi[k] + slack - 1e-6
            });
            if !meet || across.abs() < 1e-9 {
                continue;
            }
            let same =
                usize::from(ring_of(faces, other.face) == face.ring && face.ring != usize::MAX)
                    + usize::from(
                        ring_of(faces_b, other.other) == ring_of(faces_b, contacts[i].other)
                            && ring_of(faces_b, other.other) != usize::MAX,
                    );
            let better = best.is_none_or(|(s, d, _)| same > s || (same == s && across.abs() < d));
            if better {
                best = Some((same, across.abs(), j));
            }
        }
        if let Some((_, _, j)) = best {
            fit[i] = true;
            fit[j] = true;
            join(&mut parent, i, j);
        }
    }
    for i in 0..n {
        for j in i + 1..n {
            if fit[i] == fit[j] && box_distance(&contacts[i].bounds, &contacts[j].bounds) <= JOIN {
                join(&mut parent, i, j);
            }
            // Fits on one face are one joint: a relief's facets facing the
            // wall a fit also faces belong to that fit, not a joint of
            // their own that its wider gap would call loose.
            let shared =
                contacts[i].face == contacts[j].face || contacts[i].other == contacts[j].other;
            if fit[i]
                && fit[j]
                && shared
                && box_distance(&contacts[i].bounds, &contacts[j].bounds) <= SHARED_JOIN
            {
                join(&mut parent, i, j);
            }
        }
    }
    // Fits whose boxes meet are one joint: a slot's two pairs of walls.
    loop {
        let mut boxes: BTreeMap<usize, [Vec3; 2]> = BTreeMap::new();
        for i in (0..n).filter(|i| fit[*i]) {
            let r = root(&mut parent, i);
            let b = boxes
                .entry(r)
                .or_insert([[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]]);
            grow(b, contacts[i].bounds[0]);
            grow(b, contacts[i].bounds[1]);
        }
        let list: Vec<(usize, [Vec3; 2])> = boxes.into_iter().collect();
        let pair = list.iter().enumerate().find_map(|(k, (ra, ba))| {
            list[k + 1..]
                .iter()
                .find(|(_, bb)| box_distance(ba, bb) <= JOIN)
                .map(|(rb, _)| (*ra, *rb))
        });
        match pair {
            Some((ra, rb)) => join(&mut parent, ra, rb),
            None => break,
        }
    }
    // Each resting group joins the nearest fit group within reach.
    let mut roots: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = root(&mut parent, i);
        roots.entry(r).or_default().push(i);
    }
    let groups: Vec<Vec<usize>> = roots.into_values().collect();
    let is_fit: Vec<bool> = groups.iter().map(|g| g.iter().any(|i| fit[*i])).collect();
    let distance = |a: &[usize], b: &[usize]| {
        a.iter()
            .flat_map(|i| b.iter().map(move |j| (*i, *j)))
            .map(|(i, j)| box_distance(&contacts[i].bounds, &contacts[j].bounds))
            .fold(f64::INFINITY, f64::min)
    };
    let mut merged: Vec<Vec<usize>> = groups
        .iter()
        .zip(&is_fit)
        .filter(|(_, f)| **f)
        .map(|(g, _)| g.clone())
        .collect();
    let fits = merged.clone();
    for (group, _) in groups.iter().zip(&is_fit).filter(|(_, f)| !**f) {
        let nearest = fits
            .iter()
            .enumerate()
            .map(|(k, f)| (distance(group, f), k))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match nearest {
            Some((_, k)) => merged[k].extend(group.iter().copied()),
            None => merged.push(group.clone()),
        }
    }
    for group in &mut merged {
        group.sort_unstable();
    }
    (merged, fit)
}

/// The edges of an exact solid sampled every [`EDGE_STEP`] (or coarser,
/// to keep to `limit` samples), in its own frame.
fn edge_samples(solid: &ExactSolid, limit: usize) -> Vec<Vec3> {
    let mut edges: Vec<(Vec3, Vec3)> = Vec::new();
    for (k, section) in solid.sections.iter().enumerate() {
        let (z0, z1) = (solid.levels[k], solid.levels[k + 1]);
        for ring in section.iter().flat_map(|s| s.iter()) {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                edges.push(([a[0], a[1], z0], [a[0], a[1], z1]));
                edges.push(([a[0], a[1], z0], [b[0], b[1], z0]));
                edges.push(([a[0], a[1], z1], [b[0], b[1], z1]));
            }
        }
    }
    let length: f64 = edges
        .iter()
        .map(|(a, b)| pose::dot(pose::sub(*b, *a), pose::sub(*b, *a)).sqrt())
        .sum();
    let step = EDGE_STEP.max(length / limit.max(1) as f64);
    let mut out = Vec::new();
    for (a, b) in edges {
        let d = pose::sub(b, a);
        let len = pose::dot(d, d).sqrt();
        let n = (len / step).ceil().max(1.0) as usize;
        for i in 0..=n {
            out.push(pose::add(a, pose::scale(d, i as f64 / n as f64)));
        }
    }
    out
}

/// Where the edges of `a` run into `b`: the deepest they go and the
/// assembly space points that do.
fn penetration(a: &FitPiece, sa: &ExactSolid, b: &FitPiece, sb: &ExactSolid) -> (f64, Vec<Vec3>) {
    let to_b = b.pose.inverse().then_local(&a.pose);
    let bb = sb.bounds();
    let mut deepest = 0.0_f64;
    let mut points = Vec::new();
    for p in edge_samples(sa, MAX_EDGE_SAMPLES) {
        let q = to_b.apply(p);
        if q[0] < bb[0] || q[1] < bb[1] || q[0] > bb[2] || q[1] > bb[3] {
            continue;
        }
        if let Some(depth) = sb.depth_inside(q) {
            if depth > TOUCH {
                deepest = deepest.max(depth);
                points.push(a.pose.apply(p));
            }
        }
    }
    (deepest, points)
}

/// The volume two exact solids share inside `bounds` (assembly space),
/// sampled on a grid.
fn shared_volume(
    a: &FitPiece,
    sa: &ExactSolid,
    b: &FitPiece,
    sb: &ExactSolid,
    bounds: [Vec3; 2],
) -> f64 {
    let (to_a, to_b) = (a.pose.inverse(), b.pose.inverse());
    let size = pose::sub(bounds[1], bounds[0]);
    if size.iter().any(|s| *s <= 0.0) {
        return 0.0;
    }
    let n = VOLUME_SAMPLES as f64;
    let cell = [size[0] / n, size[1] / n, size[2] / n];
    let mut count = 0usize;
    for i in 0..VOLUME_SAMPLES {
        for j in 0..VOLUME_SAMPLES {
            for k in 0..VOLUME_SAMPLES {
                let p = [
                    bounds[0][0] + (i as f64 + 0.5) * cell[0],
                    bounds[0][1] + (j as f64 + 0.5) * cell[1],
                    bounds[0][2] + (k as f64 + 0.5) * cell[2],
                ];
                if inside(sa, to_a.apply(p)) && inside(sb, to_b.apply(p)) {
                    count += 1;
                }
            }
        }
    }
    count as f64 * cell[0] * cell[1] * cell[2]
}

fn inside(solid: &ExactSolid, p: Vec3) -> bool {
    solid.holds(p)
}

/// Triangles of a face's overlap in a piece's own frame.
fn triangles(rings: &[Vec<Vec3>], shapes: &Shapes, to_local: &Pose) -> Vec<f32> {
    let mut out = Vec::new();
    let mut at = 0;
    for shape in shapes {
        let count = shape.len();
        let world = &rings[at..at + count];
        at += count;
        let outer: Vec<[f64; 2]> = shape[0].clone();
        let holes: Vec<Vec<[f64; 2]>> = shape[1..].to_vec();
        let flat: Vec<Vec3> = world.iter().flatten().copied().collect();
        for tri in triangulate(&outer, &holes) {
            for i in tri {
                if let Some(p) = flat.get(i) {
                    let local = to_local.apply(*p);
                    out.extend(local.iter().map(|v| *v as f32));
                }
            }
        }
    }
    out
}

/// The points where one piece runs into the other, gathered into regions:
/// points within [`REACH`] of each other are one region. Each region's box.
fn hit_regions(hits: &[Vec3]) -> Vec<[Vec3; 2]> {
    if hits.is_empty() {
        return Vec::new();
    }
    // Buckets of the reach's size; a point meets the points of its own
    // bucket and the ones round it.
    let key = |p: &Vec3| p.map(|v| (v / REACH).floor() as i64);
    let mut buckets: BTreeMap<[i64; 3], usize> = BTreeMap::new();
    let mut cells: Vec<[i64; 3]> = Vec::new();
    for p in hits {
        let k = key(p);
        if let std::collections::btree_map::Entry::Vacant(slot) = buckets.entry(k) {
            slot.insert(cells.len());
            cells.push(k);
        }
    }
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    for (i, c) in cells.iter().enumerate() {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(&j) = buckets.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        join(&mut parent, i, j);
                    }
                }
            }
        }
    }
    let mut boxes: BTreeMap<usize, [Vec3; 2]> = BTreeMap::new();
    for p in hits {
        let cell = buckets[&key(p)];
        let r = root(&mut parent, cell);
        let b = boxes
            .entry(r)
            .or_insert([[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]]);
        grow(b, *p);
    }
    boxes.into_values().collect()
}

/// Measures every joint between two exact pieces.
fn exact_joints(
    a: &FitPiece,
    sa: &ExactSolid,
    b: &FitPiece,
    sb: &ExactSolid,
    target: FitTarget,
) -> Vec<Joint> {
    let faces_a: Vec<Face> = fit_faces(sa).iter().map(|f| f.moved(&a.pose)).collect();
    let faces_b: Vec<Face> = fit_faces(sb).iter().map(|f| f.moved(&b.pose)).collect();
    let rounds = round_fits(a, sa, b, sb);
    // Facets of a round pin and its hole give way to the round fit.
    let mut found: Vec<Contact> = contacts(&faces_a, &faces_b)
        .into_iter()
        .filter(|c| {
            let (fa, fb) = (&faces_a[c.face], &faces_b[c.other]);
            !rounds
                .iter()
                .any(|r| r.ring_a.has(fa.ring, fa.edge) && r.ring_b.has(fb.ring, fb.edge))
        })
        .collect();
    found.extend(rounds.into_iter().map(|r| r.contact));
    let (depth_ab, points_ab) = penetration(a, sa, b, sb);
    let (depth_ba, points_ba) = penetration(b, sb, a, sa);
    let mut hits: Vec<Vec3> = points_ab;
    hits.extend(points_ba);
    let deepest = depth_ab.max(depth_ba);
    let (groups, contact_fit) = clusters(&found, &faces_a, &faces_b);
    let (to_a, to_b) = (a.pose.inverse(), b.pose.inverse());
    let side_cos = SIDE_DEGREES.to_radians().cos();
    let (mut joints, cores): (Vec<Joint>, Vec<[Vec3; 2]>) = groups
        .iter()
        .map(|group| {
            // Sides: contacts grouped by the way the first piece's faces face.
            let mut sides: Vec<(Vec3, f64, f64, f64)> = Vec::new();
            let mut side_of = Vec::with_capacity(group.len());
            let mut round_side = Vec::new();
            // Each side's largest gap over its faces that are truly
            // parallel, when it has any.
            let mut square_max: Vec<f64> = Vec::new();
            for &c in group {
                let contact = &found[c];
                // A round fit is a side of its own.
                let at = if contact.round.is_some() {
                    None
                } else {
                    sides.iter().enumerate().position(|(k, (n, ..))| {
                        !round_side.contains(&k)
                            && pose::dot(pose::normalized(*n), contact.normal) > side_cos
                    })
                };
                let index = match at {
                    Some(k) => {
                        let s = &mut sides[k];
                        s.0 = pose::add(s.0, pose::scale(contact.normal, contact.area));
                        s.1 = s.1.min(contact.min);
                        s.2 = s.2.max(contact.max);
                        s.3 += contact.area;
                        if !contact.tilted {
                            square_max[k] = square_max[k].max(contact.max);
                        }
                        k
                    }
                    None => {
                        sides.push((
                            pose::scale(contact.normal, contact.area),
                            contact.min,
                            contact.max,
                            contact.area,
                        ));
                        square_max.push(if contact.tilted {
                            f64::NEG_INFINITY
                        } else {
                            contact.max
                        });
                        if contact.round.is_some() {
                            round_side.push(sides.len() - 1);
                        }
                        sides.len() - 1
                    }
                };
                side_of.push(index);
            }
            for (side, square) in sides.iter_mut().zip(&square_max) {
                if square.is_finite() {
                    side.2 = *square;
                }
            }
            let fits: Vec<bool> = (0..sides.len())
                .map(|k| {
                    group
                        .iter()
                        .zip(&side_of)
                        .any(|(c, s)| *s == k && contact_fit[*c])
                })
                .collect();
            let judge = |fit: bool, min: f64| {
                if fit {
                    target.judge(min)
                } else if min < -TOUCH {
                    FitStatus::Interference
                } else {
                    FitStatus::Ok
                }
            };
            let mut out_sides: Vec<JointSide> = sides
                .iter()
                .zip(&fits)
                .enumerate()
                .map(|(k, ((n, min, max, area), fit))| JointSide {
                    normal: pose::normalized(*n).map(|v| (v * 1e9).round() / 1e9),
                    min: *min,
                    max: *max,
                    area: *area,
                    fit: *fit,
                    status: judge(*fit, *min),
                    round: round_side.contains(&k),
                })
                .collect();
            let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
            let mut fit_bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
            let mut patches = Vec::new();
            for (&c, &side) in group.iter().zip(&side_of) {
                let contact = &found[c];
                grow(&mut bounds, contact.bounds[0]);
                grow(&mut bounds, contact.bounds[1]);
                if fits[side] {
                    grow(&mut fit_bounds, contact.bounds[0]);
                    grow(&mut fit_bounds, contact.bounds[1]);
                }
                if let Some((wall_a, wall_b)) = &contact.round {
                    patches.push(FitPatch {
                        status: judge(true, contact.min),
                        min: contact.min,
                        max: contact.max,
                        a: wall_a.clone(),
                        b: wall_b.clone(),
                    });
                    continue;
                }
                let face = &faces_a[contact.face];
                let near: Vec<Vec<Vec3>> = contact
                    .overlap
                    .iter()
                    .flat_map(|s| s.iter())
                    .map(|ring| ring.iter().map(|p| face.at(*p)).collect())
                    .collect();
                patches.push(FitPatch {
                    status: judge(fits[side], contact.min),
                    min: contact.min,
                    max: contact.max,
                    a: triangles(&near, &contact.overlap, &to_a),
                    b: triangles(&contact.far, &contact.overlap, &to_b),
                });
            }
            let judged: Vec<&JointSide> = if out_sides.iter().any(|s| s.fit) {
                out_sides.iter().filter(|s| s.fit).collect()
            } else {
                out_sides.iter().collect()
            };
            let min = judged.iter().map(|s| s.min).reduce(f64::min);
            let max = judged.iter().map(|s| s.max).reduce(f64::max);
            let status = out_sides
                .iter()
                .map(|s| s.status)
                .max()
                .unwrap_or(FitStatus::Ok);
            let overlap_depth = out_sides.iter().map(|s| -s.min).fold(0.0_f64, f64::max);
            out_sides.sort_by(|x, y| y.area.total_cmp(&x.area));
            let core = if fit_bounds[0][0] <= fit_bounds[1][0] {
                fit_bounds
            } else {
                bounds
            };
            let joint = Joint {
                a: a.key,
                b: b.key,
                status,
                source: FitSource::Exact,
                cell: None,
                sides: out_sides,
                min,
                max,
                overlap_depth: if overlap_depth > TOUCH {
                    overlap_depth
                } else {
                    0.0
                },
                overlap_volume: 0.0,
                bounds,
                patches,
            };
            (joint, core)
        })
        .unzip();
    for region in hit_regions(&hits) {
        // Each region where one piece runs into the other goes to the
        // joint whose fit lies nearest it, or makes a joint of its own when
        // none is within reach.
        let hit_box = region;
        let near = cores
            .iter()
            .enumerate()
            .map(|(k, core)| (box_distance(core, &hit_box), k))
            .filter(|(d, _)| *d <= REACH)
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, k)| k);
        let depth = deepest;
        let volume_box = [hit_box[0].map(|v| v - depth), hit_box[1].map(|v| v + depth)];
        let volume = shared_volume(a, sa, b, sb, volume_box);
        match near {
            Some(k) => {
                let j = &mut joints[k];
                j.status = FitStatus::Interference;
                j.overlap_depth = j.overlap_depth.max(deepest);
                j.overlap_volume += volume;
                grow(&mut j.bounds, hit_box[0]);
                grow(&mut j.bounds, hit_box[1]);
            }
            None => joints.push(Joint {
                a: a.key,
                b: b.key,
                status: FitStatus::Interference,
                source: FitSource::Exact,
                cell: None,
                sides: Vec::new(),
                min: None,
                max: None,
                overlap_depth: deepest,
                overlap_volume: volume,
                bounds: hit_box,
                patches: Vec::new(),
            }),
        }
    }
    for j in &mut joints {
        if j.overlap_volume == 0.0 && j.overlap_depth > 0.0 {
            j.overlap_volume = shared_volume(a, sa, b, sb, j.bounds);
        }
    }
    joints
}

/// A pair checked on the simulation grid: one joint when the pieces run
/// into each other or come closer than the target less the tolerance.
fn grid_joint(a: &FitPiece, b: &FitPiece, target: FitTarget) -> Option<Joint> {
    let placed = [
        PlacedRelief {
            key: a.key,
            stock: a.relief,
            pose: a.pose,
        },
        PlacedRelief {
            key: b.key,
            stock: b.relief,
            pose: b.pose,
        },
    ];
    let clearance = (target.target - target.tolerance).max(0.0);
    let issue = pieces::check(&placed, clearance).into_iter().next()?;
    Some(Joint {
        a: a.key,
        b: b.key,
        status: if issue.overlap > 0.0 {
            FitStatus::Interference
        } else {
            FitStatus::Tight
        },
        source: FitSource::Grid,
        cell: Some(a.relief.cell.max(b.relief.cell)),
        sides: Vec::new(),
        min: None,
        max: None,
        overlap_depth: 0.0,
        overlap_volume: issue.overlap,
        bounds: issue.bounds,
        patches: Vec::new(),
    })
}

/// The box round a piece's part box in assembly space.
fn piece_box(p: &FitPiece) -> [Vec3; 2] {
    let mut b = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for corner in 0..8 {
        grow(
            &mut b,
            p.pose.apply([
                if corner & 1 == 0 { 0.0 } else { p.size[0] },
                if corner & 2 == 0 { 0.0 } else { p.size[1] },
                if corner & 4 == 0 { 0.0 } else { p.size[2] },
            ]),
        );
    }
    b
}

/// Every joint between the pieces, in piece order.
pub fn joints(pieces: &[FitPiece], target: FitTarget) -> Vec<Joint> {
    let boxes: Vec<[Vec3; 2]> = pieces.iter().map(piece_box).collect();
    let mut out = Vec::new();
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            if !boxes_meet(&boxes[i], &boxes[j], REACH) {
                continue;
            }
            let (a, b) = (&pieces[i], &pieces[j]);
            match (a.solid, b.solid) {
                (Some(sa), Some(sb)) => out.extend(exact_joints(a, sa, b, sb, target)),
                _ => out.extend(grid_joint(a, b, target)),
            }
        }
    }
    out
}

/// Which way a joint pushes its second piece off its first: along its
/// resting contacts, else along all its faces, else from the first piece's
/// middle to the second's.
fn joint_direction(joint: &Joint, from: Vec3, to: Vec3) -> Vec3 {
    let sum = |fit: Option<bool>| {
        joint
            .sides
            .iter()
            .filter(|s| fit.is_none_or(|f| s.fit == f))
            .fold([0.0; 3], |acc, s| {
                pose::add(acc, pose::scale(s.normal, s.area))
            })
    };
    for v in [sum(Some(false)), sum(None), pose::sub(to, from)] {
        if pose::dot(v, v).sqrt() > 1e-6 {
            return pose::normalized(v);
        }
    }
    [0.0, 0.0, 1.0]
}

/// How far each piece moves at a full explode: out of its joints, one
/// piece of each joined group standing still (the one with the most joint
/// area), each piece after it moving on from where the piece it is joined
/// to went, by half its own length that way plus 10 mm. Groups (and
/// pieces with no joint) move out from the middle of the assembly.
pub fn explode(pieces: &[FitPiece], joints: &[Joint]) -> Vec<ExplodeOffset> {
    let boxes: Vec<[Vec3; 2]> = pieces.iter().map(piece_box).collect();
    let middles: Vec<Vec3> = boxes
        .iter()
        .map(|b| pose::scale(pose::add(b[0], b[1]), 0.5))
        .collect();
    let index = |key: &PieceRef| pieces.iter().position(|p| p.key == *key);
    let mut links: Vec<Vec<(usize, Vec3, f64)>> = vec![Vec::new(); pieces.len()];
    let mut weight = vec![0.0; pieces.len()];
    for joint in joints {
        let (Some(i), Some(j)) = (index(&joint.a), index(&joint.b)) else {
            continue;
        };
        let dir = joint_direction(joint, middles[i], middles[j]);
        let area: f64 = joint.sides.iter().map(|s| s.area).sum::<f64>().max(1.0);
        links[i].push((j, dir, area));
        links[j].push((i, pose::scale(dir, -1.0), area));
        weight[i] += area;
        weight[j] += area;
    }
    let mut all = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for b in &boxes {
        grow(&mut all, b[0]);
        grow(&mut all, b[1]);
    }
    let centre = pose::scale(pose::add(all[0], all[1]), 0.5);
    let diagonal = pose::dot(pose::sub(all[1], all[0]), pose::sub(all[1], all[0])).sqrt();
    let extent = |i: usize, dir: Vec3| {
        let b = boxes[i];
        (0..3)
            .map(|k| (b[1][k] - b[0][k]) * dir[k].abs())
            .sum::<f64>()
    };
    let mut offset: Vec<Option<Vec3>> = vec![None; pieces.len()];
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|a, b| weight[*b].total_cmp(&weight[*a]).then(a.cmp(b)));
    let groups = order.iter().filter(|i| !links[**i].is_empty()).count() > 0;
    for &start in &order {
        if offset[start].is_some() {
            continue;
        }
        // A group's still piece stays put when it is the only group;
        // otherwise every group moves out from the assembly's middle.
        let away = pose::sub(middles[start], centre);
        let dir = if pose::dot(away, away).sqrt() > 1e-6 {
            pose::normalized(away)
        } else {
            [0.0, 0.0, 1.0]
        };
        let alone = links[start].is_empty();
        let first_group = groups && !alone && offset.iter().all(Option::is_none);
        offset[start] = Some(if first_group {
            [0.0; 3]
        } else {
            pose::scale(dir, diagonal * 0.25)
        });
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(i) = queue.pop_front() {
            let here = offset[i].unwrap_or([0.0; 3]);
            let mut next = links[i].clone();
            next.sort_by(|a, b| b.2.total_cmp(&a.2));
            for (j, dir, _) in next {
                if offset[j].is_some() {
                    continue;
                }
                let step = extent(j, dir) * 0.5 + 10.0;
                offset[j] = Some(pose::add(here, pose::scale(dir, step)));
                queue.push_back(j);
            }
        }
    }
    pieces
        .iter()
        .zip(offset)
        .map(|(p, o)| ExplodeOffset {
            piece: p.key,
            offset: o.unwrap_or([0.0; 3]).map(|v| (v * 1e6).round() / 1e6),
        })
        .collect()
}

/// A piece with all the fit, the measure and the export need of it.
pub struct HeldPiece {
    /// Which piece.
    pub key: PieceRef,
    /// Its name, as the piece list shows it.
    pub name: String,
    /// Local frame to assembly space, as assembled.
    pub pose: Pose,
    /// Its part box: width, height, thickness.
    pub size: Vec3,
    /// Its relief on the simulation grid.
    pub stock: Stock,
    /// Its exact solid, when it has exact edges.
    pub solid: Option<ExactSolid>,
}

/// Every piece with its pose, relief and exact solid (none for a raw
/// model, a component, or a part a cut that cannot be made exact runs
/// over), in piece order.
///
/// # Errors
/// As [`pieces::pieces`].
pub fn solids(
    document: &Document,
    cache: &mut PartsCache,
    quality: Quality,
) -> Result<Vec<HeldPiece>, CoreError> {
    let infos = pieces::pieces(document, cache, quality.cells())?;
    let mut sheets: BTreeMap<u64, exact::SheetExact> = BTreeMap::new();
    let mut out = Vec::with_capacity(infos.len());
    for info in &infos {
        let Some(stock) = info.stock() else {
            return Err(CoreError::validation("a piece is gone"));
        };
        let sheet = info.piece.sheet;
        let sheet_name = document
            .sheet(sheet)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let solid = if info.is_model() || info.piece.component.is_some() {
            None
        } else {
            if let std::collections::btree_map::Entry::Vacant(slot) = sheets.entry(sheet) {
                slot.insert(exact::sheet_exact(document, sheet)?);
            }
            sheets
                .get(&sheet)
                .and_then(|e| exact::piece_solid(e, &info.sheet_parts, info.local))
        };
        let name = match info.piece.component {
            Some(id) => document
                .component(id)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            None if info.is_model() => format!("{sheet_name} model"),
            None => {
                pieces::vector_name(document, &info.sheet_parts, info.local).unwrap_or_else(|| {
                    if info.parts > 1 {
                        format!("{sheet_name} part {}", info.piece.part)
                    } else {
                        sheet_name.clone()
                    }
                })
            }
        };
        out.push(HeldPiece {
            key: info.piece.key(),
            name,
            pose: info.piece.pose(info.size),
            size: info.size,
            stock,
            solid,
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyArgs {
    #[serde(flatten)]
    target: FitTarget,
    #[serde(default)]
    quality: Quality,
    /// Pieces left out of the check (the hidden ones, when the view asks
    /// for the shown pieces only).
    #[serde(default)]
    leave_out: Vec<PieceRef>,
}

/// `assembly.verifyFit`: every joint between the placed pieces (all of
/// them, less any `leaveOut` names) with its clearance, and each piece's
/// move at a full explode. Reads only.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] for a negative target or tolerance.
pub fn verify(
    document: &Document,
    cache: &mut PartsCache,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let trimmed = args.trim();
    let args: VerifyArgs = serde_json::from_str(if trimmed.is_empty() { "{}" } else { trimmed })
        .map_err(|error| CoreError::invalid_args("assembly.verifyFit", error))?;
    let FitTarget { target, tolerance } = args.target;
    if !target.is_finite() || !tolerance.is_finite() || target < 0.0 || tolerance < 0.0 {
        return Err(CoreError::validation(
            "the target clearance and tolerance must be zero or more",
        ));
    }
    let held = solids(document, cache, args.quality)?;
    let placed: Vec<FitPiece> = held
        .iter()
        .filter(|h| !args.leave_out.contains(&h.key))
        .map(|h| FitPiece {
            key: h.key,
            pose: h.pose,
            solid: h.solid.as_ref(),
            relief: &h.stock,
            size: h.size,
        })
        .collect();
    let found = joints(&placed, args.target);
    let offsets = explode(&placed, &found);
    let exact_pieces: Vec<PieceRef> = placed
        .iter()
        .filter(|p| p.solid.is_some())
        .map(|p| p.key)
        .collect();
    KernelReply::ok(&json!({
        "joints": found,
        "explode": offsets,
        "exact": exact_pieces,
        "target": target,
        "tolerance": tolerance,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::super::pieces::tests::{add, document};
    use super::super::pieces_cmd::dispatch;
    use super::*;
    use crate::cam::compute::{calculate, Request, Strategy};
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{Path, Point, SubPath};

    const CELLS: f64 = 1.0e6;

    fn run(doc: &mut Document, cache: &mut PartsCache, name: &str, args: Value) -> Value {
        match dispatch(doc, cache, name, &args.to_string())
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        {
            KernelReply::Json(text) => {
                serde_json::from_str::<Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(bytes) => json!({ "bytes": bytes.len() }),
        }
    }

    /// A closed vector through `points` on `sheet`.
    fn vector(doc: &mut Document, sheet: u64, points: &[[f64; 2]]) -> u64 {
        let mut sub = SubPath::open(Point::new(points[0][0], points[0][1]));
        for p in &points[1..] {
            sub.line_to(Point::new(p[0], p[1]));
        }
        sub.line_to(Point::new(points[0][0], points[0][1]));
        sub.closed = true;
        let layer = doc.active_layer();
        doc.begin("vector");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: Path {
                    subpaths: vec![sub],
                },
            },
        });
        doc.commit();
        id
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// A 6.35 mm end mill toolpath calculated from `id` the way the form does.
    fn machine(doc: &mut Document, sheet: u64, strategy: Strategy, id: u64, params: Value) {
        let tool = Tool {
            name: "1/4 in".into(),
            tool_type: ToolGeometry::EndMill { diameter: 6.35 },
            ..Tool::default()
        };
        let request = Request {
            strategy,
            name: None,
            tool,
            params,
            ids: vec![id],
        };
        let (mut toolpath, _) = calculate(doc, &request).expect("calculates");
        toolpath.sheet = sheet;
        add(doc, toolpath);
    }

    fn profile(doc: &mut Document, sheet: u64, id: u64, side: &str) {
        machine(
            doc,
            sheet,
            Strategy::Profile,
            id,
            json!({ "side": side, "cutDepth": 10.5 }),
        );
    }

    /// Where the kernel has piece `sheet`: its pose, part box and the job
    /// box of its part.
    fn placed(doc: &Document, sheet: u64) -> (Pose, Vec3, [f64; 4]) {
        let mut cache = PartsCache::default();
        let infos = pieces::pieces(doc, &mut cache, CELLS).expect("pieces");
        let info = infos
            .iter()
            .find(|i| i.piece.sheet == sheet)
            .expect("the piece");
        let jb = info.sheet_parts.job_box(info.local).expect("box");
        (info.piece.pose(info.size), info.size, jb)
    }

    /// Places sheet `moving`'s piece turned by `rotation` so that the point
    /// `local` of its part (job X, Y and height above its underside) lands
    /// on `world`.
    fn put(doc: &mut Document, moving: u64, rotation: Vec3, local: Vec3, world: Vec3) {
        let (_, size, jb) = placed(doc, moving);
        let p = [local[0] - jb[0], local[1] - jb[1], local[2]];
        let turned = Pose::euler(rotation).apply_vector(pose::sub(p, pose::scale(size, 0.5)));
        let position = pose::sub(world, turned);
        let mut cache = PartsCache::default();
        run(
            doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": moving, "part": 1, "position": position, "rotation": rotation }),
        );
    }

    fn world_of(doc: &Document, sheet: u64, job: Vec3) -> Vec3 {
        let (pose, _, jb) = placed(doc, sheet);
        pose.apply([job[0] - jb[0], job[1] - jb[1], job[2]])
    }

    fn verify(doc: &mut Document, target: f64, tolerance: f64) -> Value {
        let mut cache = PartsCache::default();
        run(
            doc,
            &mut cache,
            "assembly.verifyFit",
            json!({ "target": target, "tolerance": tolerance }),
        )
    }

    fn side_along(joint: &Value, axis: Vec3) -> &Value {
        joint["sides"]
            .as_array()
            .expect("sides")
            .iter()
            .find(|s| {
                let n: Vec<f64> = s["normal"]
                    .as_array()
                    .expect("normal")
                    .iter()
                    .map(|v| v.as_f64().unwrap_or(0.0))
                    .collect();
                (0..3).all(|k| (n[k] - axis[k]).abs() < 1e-3)
            })
            .unwrap_or_else(|| panic!("no side along {axis:?}: {joint}"))
    }

    fn number(v: &Value) -> f64 {
        v.as_f64().unwrap_or(f64::NAN)
    }

    /// A base panel with a slot drawn 10.4 mm across and `length` along,
    /// cut inside, and an upright panel whose tab is drawn 10 mm wide, cut
    /// outside, both with a 6.35 mm end mill in 10 mm material, the tab
    /// stood in the slot's middle. Notches 7 mm square beside the tab's
    /// root keep the round tool's fillets (it cannot cut a sharp inside
    /// corner) up out of the slot.
    fn tab_and_slot(length: f64) -> (Document, u64, u64) {
        let mut doc = document();
        let base = doc.active_sheet();
        doc.begin("sheet");
        let upright = doc.add_sheet(Some("Upright".into()));
        doc.commit();
        let outline = vector(&mut doc, base, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, base, outline, "outside");
        let slot = vector(
            &mut doc,
            base,
            &rect(44.8, 25.0 - length / 2.0, 55.2, 25.0 + length / 2.0),
        );
        profile(&mut doc, base, slot, "inside");
        // The upright: a body 60 x 25 with a 10 x 10 tab under its middle.
        let tabbed = vector(
            &mut doc,
            upright,
            &[
                [20.0, 15.0],
                [38.0, 15.0],
                [38.0, 22.0],
                [45.0, 22.0],
                [45.0, 5.0],
                [55.0, 5.0],
                [55.0, 22.0],
                [62.0, 22.0],
                [62.0, 15.0],
                [80.0, 15.0],
                [80.0, 40.0],
                [20.0, 40.0],
            ],
        );
        profile(&mut doc, upright, tabbed, "outside");
        // The tab's shoulder (job y = 15, halfway through the thickness)
        // on the base's top over the slot's middle.
        let target = world_of(&doc, base, [50.0, 25.0, 10.0]);
        put(
            &mut doc,
            upright,
            [90.0, 0.0, 0.0],
            [50.0, 15.0, 5.0],
            target,
        );
        (doc, base, upright)
    }

    fn sides(joint: &Value) -> usize {
        joint["sides"].as_array().map_or(0, Vec::len)
    }

    #[test]
    fn a_10_mm_tab_in_a_slot_drawn_10_4_mm_has_0_2_mm_each_side() {
        let (mut doc, base, upright) = tab_and_slot(18.0);
        let reply = verify(&mut doc, 0.2, 0.1);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 1, "{reply}");
        let joint = &joints[0];
        assert_eq!(joint["source"], "exact");
        assert_eq!(joint["a"]["sheet"], base);
        assert_eq!(joint["b"]["sheet"], upright);
        // The slot's walls either side of the tab: 0.2 mm each, to 0.01 mm,
        // the same all along each face.
        for axis in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
            let side = side_along(joint, axis);
            assert_eq!(side["fit"], true, "{side}");
            assert_eq!(side["status"], "ok", "{side}");
            for key in ["min", "max"] {
                let gap = number(&side[key]);
                assert!((gap - 0.2).abs() < 0.01, "{axis:?} {key} {gap}");
            }
        }
        // The shoulders rest on the base's top: a contact, no gap. The
        // slot's ends lie 4 mm off the tab's faces, out of reach.
        let rest = side_along(joint, [0.0, 0.0, 1.0]);
        assert_eq!(rest["fit"], false);
        assert!(number(&rest["min"]).abs() < 0.001 && number(&rest["max"]).abs() < 0.001);
        assert_eq!(sides(joint), 3, "{joint}");
        assert_eq!(joint["status"], "ok", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0);
        assert!((number(&joint["min"]) - 0.2).abs() < 0.01);
        // A tolerance that leaves 0.2 mm out makes it loose or tight.
        let loose = verify(&mut doc, 0.1, 0.05);
        assert_eq!(loose["joints"][0]["status"], "loose");
        let tight = verify(&mut doc, 0.3, 0.05);
        assert_eq!(tight["joints"][0]["status"], "tight");
        // With the upright left out (hidden, not included) there is no joint.
        let mut cache = PartsCache::default();
        let without = run(
            &mut doc,
            &mut cache,
            "assembly.verifyFit",
            json!({ "target": 0.2, "tolerance": 0.1, "leaveOut": [joint["b"]] }),
        );
        assert_eq!(without["joints"], json!([]), "{without}");
    }

    #[test]
    fn square_tab_corners_run_into_the_round_corners_of_a_square_slot() {
        let (mut doc, _, _) = tab_and_slot(10.4);
        let reply = verify(&mut doc, 0.2, 0.1);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 1, "{reply}");
        let joint = &joints[0];
        // 0.2 mm on all four sides of the tab ...
        for axis in [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
        ] {
            let side = side_along(joint, axis);
            assert!((number(&side["min"]) - 0.2).abs() < 0.01, "{side}");
        }
        // ... but the tool leaves the slot's corners round, its own radius,
        // and the tab's square corners run into them: from the corner
        // arc's middle, (3.175 - 0.2) * sqrt 2 - 3.175 mm deep.
        let corner = (3.175 - 0.2) * 2f64.sqrt() - 3.175;
        assert_eq!(joint["status"], "interference", "{joint}");
        let depth = number(&joint["overlapDepth"]);
        assert!((depth - corner).abs() < 0.01, "{depth} vs {corner}");
        assert!(number(&joint["overlapVolume"]) > 0.0);
    }

    #[test]
    fn a_panel_in_a_dado_fits_and_explodes_out_of_it() {
        let mut doc = document();
        let base = doc.active_sheet();
        doc.begin("sheet");
        let shelf = doc.add_sheet(Some("Shelf".into()));
        doc.commit();
        let outline = vector(&mut doc, base, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, base, outline, "outside");
        // A dado 10.4 mm wide and 5 mm deep right across the base.
        let dado = vector(&mut doc, base, &rect(44.8, 0.0, 55.2, 50.0));
        machine(
            &mut doc,
            base,
            Strategy::Pocket,
            dado,
            json!({ "cutDepth": 5.0 }),
        );
        let panel = vector(&mut doc, shelf, &rect(10.0, 10.0, 90.0, 30.0));
        profile(&mut doc, shelf, panel, "outside");
        // The panel on its edge in the dado: its bottom edge's middle on
        // the dado floor's middle.
        let target = world_of(&doc, base, [50.0, 25.0, 5.0]);
        put(
            &mut doc,
            shelf,
            [90.0, 0.0, 90.0],
            [50.0, 10.0, 5.0],
            target,
        );
        let reply = verify(&mut doc, 0.2, 0.05);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 1, "{reply}");
        let joint = &joints[0];
        for axis in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
            let side = side_along(joint, axis);
            assert!((number(&side["min"]) - 0.2).abs() < 0.01, "{side}");
            assert!((number(&side["max"]) - 0.2).abs() < 0.01, "{side}");
        }
        let floor = side_along(joint, [0.0, 0.0, 1.0]);
        assert!(number(&floor["min"]).abs() < 0.001, "{floor}");
        assert_eq!(joint["status"], "ok", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0);
        // Exploded, the panel lifts straight up out of the dado and the
        // base stays put.
        let offsets = reply["explode"].as_array().expect("explode");
        let of = |sheet: u64| {
            let row = offsets
                .iter()
                .find(|o| o["piece"]["sheet"] == sheet)
                .expect("offset");
            row["offset"]
                .as_array()
                .expect("xyz")
                .iter()
                .map(number)
                .collect::<Vec<f64>>()
        };
        assert_eq!(of(base), vec![0.0, 0.0, 0.0]);
        let up = of(shelf);
        assert!(
            up[0].abs() < 1e-6 && up[1].abs() < 1e-6 && up[2] > 10.0,
            "{up:?}"
        );
    }

    #[test]
    fn a_thick_tab_runs_into_its_slot_by_the_difference() {
        let (mut doc, _, upright) = tab_and_slot(18.0);
        // Pushed 0.3 mm along X, the tab runs 0.1 mm into one wall and
        // leaves 0.5 mm at the other.
        let (pose, _, _) = placed(&doc, upright);
        let mut cache = PartsCache::default();
        let infos = pieces::pieces(&doc, &mut cache, CELLS).expect("pieces");
        let info = infos
            .iter()
            .find(|i| i.piece.sheet == upright)
            .expect("piece");
        let mut position = info.piece.position;
        position[0] += 0.3;
        let _ = pose;
        run(
            &mut doc,
            &mut cache,
            "assembly.placePiece",
            json!({ "sheet": upright, "part": 1, "position": position, "rotation": info.piece.rotation }),
        );
        let reply = verify(&mut doc, 0.2, 0.1);
        let joint = &reply["joints"][0];
        let pushed = side_along(joint, [1.0, 0.0, 0.0]);
        let left = side_along(joint, [-1.0, 0.0, 0.0]);
        let (a, b) = (number(&pushed["min"]), number(&left["min"]));
        let (wall, open) = if a < b { (a, b) } else { (b, a) };
        assert!((wall + 0.1).abs() < 0.01, "{wall}");
        assert!((open - 0.5).abs() < 0.01, "{open}");
        assert_eq!(joint["status"], "interference");
        assert!(number(&joint["overlapDepth"]) >= 0.1 - 0.01);
    }
    #[test]
    fn measure_snaps_to_exact_faces_and_gives_the_gap_between_parallel_faces() {
        let (mut doc, base, upright) = tab_and_slot(18.0);
        let (_, _, jba) = placed(&doc, base);
        let (_, _, jbb) = placed(&doc, upright);
        // Picks a little off the true faces, as a pick on the grid relief
        // is: 0.3 mm inside the slot from its -X wall, and 0.25 mm inside
        // the tab from its -X wall.
        let a = [44.8 + 0.3 - jba[0], 25.0 - jba[1], 5.0];
        let b = [45.0 + 0.25 - jbb[0], 10.0 - jbb[1], 5.0];
        let mut cache = PartsCache::default();
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.measure",
            json!({
                "a": { "sheet": base, "part": 1, "point": a },
                "b": { "sheet": upright, "part": 1, "point": b },
            }),
        );
        assert_eq!(reply["parallel"], true, "{reply}");
        assert_eq!(reply["a"]["exact"], true);
        let gap = number(&reply["faceDistance"]);
        assert!((gap - 0.2).abs() < 1e-3, "{gap}");
        assert!(number(&reply["distance"]) >= gap);
    }

    /// The volume a closed mesh holds, by the divergence theorem.
    fn mesh_volume(triangles: &[[[f64; 3]; 3]]) -> f64 {
        triangles
            .iter()
            .map(|[a, b, c]| pose::dot(*a, pose::cross(*b, *c)) / 6.0)
            .sum()
    }

    fn solid_volume(solid: &ExactSolid) -> f64 {
        solid
            .sections
            .iter()
            .zip(solid.levels.windows(2))
            .map(|(s, w)| exact::shapes_area(s) * (w[1] - w[0]))
            .sum()
    }

    #[test]
    fn export_writes_the_placed_pieces_as_closed_stl_and_3mf() {
        let (mut doc, _, _) = tab_and_slot(18.0);
        let dir =
            std::env::temp_dir().join(format!("carve-assembly-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let stl = dir.join("pieces.stl");
        let mut cache = PartsCache::default();
        let reply = run(
            &mut doc,
            &mut cache,
            "assembly.exportAssembly",
            json!({ "path": stl.to_string_lossy(), "format": "stl" }),
        );
        assert_eq!(reply["pieces"], 2);
        assert_eq!(reply["exact"], 2);
        let bytes = std::fs::read(&stl).expect("stl");
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        assert_eq!(bytes.len(), 84 + 50 * count);
        assert_eq!(count as u64, reply["triangles"].as_u64().unwrap_or(0));
        // Each piece's surface is closed and wound outwards: the mesh holds
        // the pieces' volume.
        let held = solids(&doc, &mut cache, Quality::default()).expect("solids");
        let want: f64 = held
            .iter()
            .filter_map(|h| h.solid.as_ref())
            .map(solid_volume)
            .sum();
        let mut got = 0.0;
        for h in &held {
            let local = super::super::export::exact_triangles(h.solid.as_ref().expect("exact"));
            got += mesh_volume(&local);
        }
        assert!((got - want).abs() < want * 1e-6, "{got} vs {want}");
        let three = dir.join("pieces.3mf");
        run(
            &mut doc,
            &mut cache,
            "assembly.exportAssembly",
            json!({ "path": three.to_string_lossy(), "format": "3mf" }),
        );
        let file = std::fs::File::open(&three).expect("3mf");
        let mut archive = zip::ZipArchive::new(file).expect("a zip");
        let mut model = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("3D/3dmodel.model").expect("the model"),
            &mut model,
        )
        .expect("text");
        assert!(model.contains("<object id=\"2\""), "two objects");
        assert!(model.contains("unit=\"millimeter\""));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_grid_relief_exports_as_a_closed_mesh() {
        let (doc, base, _) = tab_and_slot(18.0);
        let mut cache = PartsCache::default();
        let held = solids(&doc, &mut cache, Quality::default()).expect("solids");
        let piece = held.iter().find(|h| h.key.sheet == base).expect("base");
        let relief = super::super::export::relief_triangles(&piece.stock);
        let cells = (0..piece.stock.z.len())
            .filter(|i| piece.stock.floor[*i] < piece.stock.z[*i] - crate::cam::sim::THROUGH)
            .map(|i| f64::from(piece.stock.z[i] - piece.stock.floor[i]))
            .sum::<f64>();
        let want = cells * piece.stock.cell * piece.stock.cell;
        let got = mesh_volume(&relief);
        assert!((got - want).abs() < want * 1e-6, "{got} vs {want}");
    }

    #[test]
    fn the_cut_list_gives_exact_sizes_and_areas() {
        let (mut doc, base, _) = tab_and_slot(18.0);
        let mut cache = PartsCache::default();
        let rows = run(&mut doc, &mut cache, "assembly.pieceCutList", json!({}))["rows"].clone();
        let row = rows
            .as_array()
            .expect("rows")
            .iter()
            .find(|r| r["sheet"] == base)
            .expect("base row")
            .clone();
        assert_eq!(row["exact"], true);
        assert!((number(&row["length"]) - 90.0).abs() < 1e-3, "{row}");
        assert!((number(&row["width"]) - 40.0).abs() < 1e-3, "{row}");
        // The outline less the slot, whose corners the tool leaves round.
        let r = 3.175_f64;
        let slot = 10.4 * 18.0 - (4.0 - std::f64::consts::PI) * r * r;
        let want = 90.0 * 40.0 - slot;
        assert!(
            (number(&row["area"]) - want).abs() < 0.05,
            "{row} vs {want}"
        );
    }

    fn polygon_circle(cx: f64, cy: f64, r: f64) -> Vec<[f64; 2]> {
        (0..64)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 64.0;
                [cx + r * a.cos(), cy + r * a.sin()]
            })
            .collect()
    }

    #[test]
    fn a_round_pin_in_a_round_hole_is_one_round_fit_however_the_facets_lie() {
        let mut doc = document();
        let base = doc.active_sheet();
        doc.begin("sheet");
        let dowel = doc.add_sheet(Some("Dowel".into()));
        doc.commit();
        let outline = vector(&mut doc, base, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, base, outline, "outside");
        let hole = vector(&mut doc, base, &polygon_circle(50.0, 25.0, 4.2));
        profile(&mut doc, base, hole, "inside");
        // A pin drawn with its corners turned half a step from the hole's.
        let pin: Vec<[f64; 2]> = (0..48)
            .map(|i| {
                let a = std::f64::consts::TAU * (f64::from(i) + 0.5) / 48.0;
                [30.0 + 4.0 * a.cos(), 30.0 + 4.0 * a.sin()]
            })
            .collect();
        let pin = vector(&mut doc, dowel, &pin);
        profile(&mut doc, dowel, pin, "outside");
        // Its axis 0.05 mm off the hole's, its bottom flush with the base's.
        let target = world_of(&doc, base, [50.05, 25.0, 0.0]);
        put(&mut doc, dowel, [0.0; 3], [30.0, 30.0, 0.0], target);
        let reply = verify(&mut doc, 0.2, 0.1);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 1, "{reply}");
        let joint = &joints[0];
        let round: Vec<&Value> = joint["sides"]
            .as_array()
            .expect("sides")
            .iter()
            .filter(|s| s["round"] == true)
            .collect();
        assert_eq!(round.len(), 1, "{joint}");
        let side = round[0];
        assert_eq!(side["fit"], true);
        let (min, max) = (number(&side["min"]), number(&side["max"]));
        // 0.05 mm less on one side than the other.
        assert!((max - min - 0.1).abs() < 1e-3, "{min} {max}");
        let middle = (min + max) / 2.0;
        assert!(middle > 0.15 && middle < 0.21, "{middle}");
        assert_eq!(joint["status"], "ok", "{joint}");
        // No facet sides are left beside the round one.
        assert_eq!(sides(joint), 1, "{joint}");
        assert!(!joint["patches"][0]["a"].as_array().expect("a").is_empty());
    }

    /// A slot from (x0, y) to (x1, y) with round ends of radius `r`.
    fn stadium(x0: f64, x1: f64, y: f64, r: f64) -> Vec<[f64; 2]> {
        let mut out = Vec::new();
        for i in 0..=32 {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 32.0;
            out.push([x1 + r * a.cos(), y + r * a.sin()]);
        }
        for i in 0..=32 {
            let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 32.0;
            out.push([x0 + r * a.cos(), y + r * a.sin()]);
        }
        out
    }

    #[test]
    fn a_pin_in_the_round_end_of_a_slot_is_a_round_fit_of_that_end() {
        let mut doc = document();
        let base = doc.active_sheet();
        doc.begin("sheet");
        let dowel = doc.add_sheet(Some("Dowel".into()));
        doc.commit();
        let outline = vector(&mut doc, base, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, base, outline, "outside");
        let slot = vector(&mut doc, base, &stadium(40.0, 60.0, 25.0, 4.2));
        profile(&mut doc, base, slot, "inside");
        let pin = vector(&mut doc, dowel, &polygon_circle(30.0, 30.0, 4.0));
        profile(&mut doc, dowel, pin, "outside");
        // The pin stood in the slot's left end, 0.05 mm off its middle to
        // the left, flush with the base's underside.
        let target = world_of(&doc, base, [39.95, 25.0, 0.0]);
        put(&mut doc, dowel, [0.0; 3], [30.0, 30.0, 0.0], target);
        let reply = verify(&mut doc, 0.2, 0.1);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 1, "{reply}");
        let joint = &joints[0];
        let round: Vec<&Value> = joint["sides"]
            .as_array()
            .expect("sides")
            .iter()
            .filter(|s| s["round"] == true)
            .collect();
        assert_eq!(round.len(), 1, "{joint}");
        let (min, max) = (number(&round[0]["min"]), number(&round[0]["max"]));
        // Round the end the gap closes to 0.15 mm straight to the left and
        // opens to about 0.2 mm where the end meets the straight sides.
        assert!((min - 0.15).abs() < 0.01, "{min} {max}");
        assert!(max > 0.19 && max < 0.21, "{min} {max}");
        assert_eq!(joint["status"], "ok", "{joint}");
    }

    #[test]
    fn two_tabs_in_two_slots_are_two_joints_each_with_its_own_clash() {
        let mut doc = document();
        let base = doc.active_sheet();
        doc.begin("sheet");
        let upright = doc.add_sheet(Some("Upright".into()));
        doc.commit();
        let outline = vector(&mut doc, base, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, base, outline, "outside");
        for x in [30.0, 70.0] {
            let slot = vector(&mut doc, base, &rect(x - 5.2, 20.0, x + 5.2, 30.0));
            profile(&mut doc, base, slot, "inside");
        }
        let tabbed = vector(
            &mut doc,
            upright,
            &[
                [10.0, 15.0],
                [25.0, 15.0],
                [25.0, 5.0],
                [35.0, 5.0],
                [35.0, 15.0],
                [65.0, 15.0],
                [65.0, 5.0],
                [75.0, 5.0],
                [75.0, 15.0],
                [90.0, 15.0],
                [90.0, 40.0],
                [10.0, 40.0],
            ],
        );
        profile(&mut doc, upright, tabbed, "outside");
        let target = world_of(&doc, base, [50.0, 25.0, 10.0]);
        put(
            &mut doc,
            upright,
            [90.0, 0.0, 0.0],
            [50.0, 15.0, 5.0],
            target,
        );
        let reply = verify(&mut doc, 0.2, 0.1);
        let joints = reply["joints"].as_array().expect("joints");
        assert_eq!(joints.len(), 2, "{reply}");
        let middles: Vec<f64> = joints
            .iter()
            .map(|j| (number(&j["bounds"][0][0]) + number(&j["bounds"][1][0])) / 2.0)
            .collect();
        assert!((middles[0] - middles[1]).abs() > 20.0, "{middles:?}");
        for joint in joints {
            // Square tab corners in round slot corners, at each tab.
            assert_eq!(joint["status"], "interference", "{joint}");
            assert!(number(&joint["overlapVolume"]) > 0.0);
            let side = side_along(joint, [1.0, 0.0, 0.0]);
            assert!((number(&side["min"]) - 0.2).abs() < 0.01, "{side}");
        }
    }

    #[test]
    fn a_deep_v_groove_keeps_the_piece_exact() {
        let mut doc = document();
        let sheet = doc.active_sheet();
        let outline = vector(&mut doc, sheet, &rect(5.0, 5.0, 95.0, 45.0));
        profile(&mut doc, sheet, outline, "outside");
        // A 90 degree V groove 3 mm deep across the part, cut on the line.
        let line = vector(&mut doc, sheet, &rect(20.0, 25.0, 80.0, 25.01));
        let request = Request {
            strategy: Strategy::Profile,
            name: None,
            tool: Tool {
                name: "90 V".into(),
                tool_type: ToolGeometry::VBit {
                    diameter: 12.0,
                    included_angle: 90.0,
                },
                ..Tool::default()
            },
            params: json!({ "side": "on", "cutDepth": 3.0 }),
            ids: vec![line],
        };
        let (mut toolpath, _) = calculate(&doc, &request).expect("calculates");
        toolpath.sheet = sheet;
        add(&mut doc, toolpath);
        let mut cache = PartsCache::default();
        let held = solids(&doc, &mut cache, Quality::Standard).expect("solids");
        let solid = held[0]
            .solid
            .as_ref()
            .expect("an exact solid despite the V groove");
        // Terraces from 3 mm deep up: many levels under the top.
        assert!(solid.levels.len() > 8, "{:?}", solid.levels);
        let deep = solid.levels.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(deep.abs() < 1e-9);
        // Straight under the groove's middle, 2 mm down, is air; 2.5 mm off
        // it at that height is material (the cone is 2 mm wide there).
        let mid = [50.0 - 5.0, 25.0 - 5.0];
        assert!(solid.depth_inside([mid[0], mid[1], 8.0]).is_none());
        assert!(solid.depth_inside([mid[0], mid[1] + 2.5, 8.0]).is_some());
    }
}
