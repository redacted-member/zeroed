//! From panels and the joints picked for their connections to flat parts:
//! which face of each panel is cut from the top, each panel's frame (its
//! flat part space to furniture space), how far each joint makes a panel
//! grow past its box so the assembled piece keeps its outside size, and
//! each joint laid out as the Joint Creator takes it, in flat coordinates.
//!
//! Flat coordinates are a panel's part space before it is placed on a
//! sheet: X and Y on the sheet, Z up from the underside; the panel's box
//! corner is at the origin. Each joint's edges and face line are chosen so
//! that the Joint Creator's own idea of how the parts go together (its B to
//! A transform) is where the boxes put them.

use crate::assembly::pose::{Pose, Vec3};
use crate::joints::geometry::{ccw, Frame, P2};
use crate::joints::kinds::{coplanar_map, corner_map, tee_map_at};
use crate::joints::pose::{b_to_a, Map};
use crate::joints::{JointKind, JointParams};

use super::connect::{shared, ConnType, Connection, FJoint, JointChoice};
use super::eval::{Issue, Panel, TOL};

/// How a panel lies: its axes and which face is its top on the sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelFrame {
    /// The flat axes (furniture axis indices) and the normal axis.
    pub u: usize,
    pub v: usize,
    pub n: usize,
    /// +1 when the top on the sheet faces along +n, -1 along -n.
    pub up: f64,
    /// Whether flat Y runs against the v axis (so the frame turns, not
    /// mirrors).
    pub mirror: bool,
    /// Where the flat origin lies, furniture space.
    pub origin: Vec3,
}

fn axis(k: usize, s: f64) -> Vec3 {
    let mut a = [0.0; 3];
    a[k] = s;
    a
}

impl PanelFrame {
    /// The frame of `panel` with its top facing `up` along its normal.
    pub fn of(panel: &Panel, up: f64) -> PanelFrame {
        let (u, v) = panel.flat_axes();
        let n = panel.normal;
        // (e_u x e_v) . e_n for flat axes in X, Y, Z order.
        let handed = if (u, v) == (0, 2) { -1.0 } else { 1.0 };
        let mirror = up * handed < 0.0;
        let mut origin = panel.min;
        if mirror {
            origin[v] = panel.max[v];
        }
        if up < 0.0 {
            origin[n] = panel.max[n];
        }
        PanelFrame {
            u,
            v,
            n,
            up,
            mirror,
            origin,
        }
    }

    /// Flat part space to furniture space.
    pub fn pose(&self) -> Pose {
        Pose::frame(
            self.origin,
            axis(self.u, 1.0),
            axis(self.v, if self.mirror { -1.0 } else { 1.0 }),
            axis(self.n, self.up),
        )
    }

    /// The flat X and Y of a furniture point.
    pub fn flat(&self, p: Vec3) -> P2 {
        let q = self.pose().inverse().apply(p);
        [q[0], q[1]]
    }

    /// The flat point of panel coordinates `(u, v)` (from the box corner,
    /// along the first and second flat axes).
    pub fn flat_uv(&self, panel: &Panel, u: f64, v: f64) -> P2 {
        let mut p = panel.min;
        p[self.u] += u;
        p[self.v] += v;
        self.flat(p)
    }
}

/// How far each side of a panel grows past its box: per furniture axis,
/// the low side then the high side, millimetres.
pub type Growth = [[f64; 2]; 3];

/// A panel's box grown, furniture space.
pub fn grown_box(panel: &Panel, g: &Growth) -> ([f64; 3], [f64; 3]) {
    let mut min = panel.min;
    let mut max = panel.max;
    for k in 0..3 {
        min[k] -= g[k][0];
        max[k] += g[k][1];
    }
    (min, max)
}

/// The flat rectangle of a panel's grown box, counter-clockwise.
pub fn flat_rect(panel: &Panel, frame: &PanelFrame, g: &Growth) -> Vec<P2> {
    let (min, max) = grown_box(panel, g);
    let (u, v) = (frame.u, frame.v);
    let corner = |a: f64, b: f64| {
        let mut p = min;
        p[u] = a;
        p[v] = b;
        frame.flat(p)
    };
    ccw(vec![
        corner(min[u], min[v]),
        corner(max[u], min[v]),
        corner(max[u], max[v]),
        corner(min[u], max[v]),
    ])
}

/// Where a joint's part B is, in B's flat coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlatTarget {
    Edge(P2, P2),
    Face {
        from: P2,
        to: P2,
        offset: f64,
        along: f64,
    },
}

/// A joint laid out for the Joint Creator, in flat coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    /// The connection's index.
    pub conn: usize,
    pub kind: JointKind,
    /// Panel indices.
    pub a: usize,
    pub b: usize,
    /// A's edge, flat coordinates of A.
    pub a_edge: (P2, P2),
    pub b_target: FlatTarget,
    pub params: JointParams,
    pub flip: bool,
    /// Whether the joint's own B to A transform is where the boxes put B
    /// (else the joint is cut right but assembled the other way round, as
    /// for a through joint whose B is cut from its other face).
    pub matched: bool,
}

/// How far apart two poses are: rotation entries (scaled) and translation.
pub fn pose_error(a: &Pose, b: &Pose) -> f64 {
    let mut rot: f64 = 0.0;
    for i in 0..3 {
        for j in 0..3 {
            rot = rot.max((a.r[i][j] - b.r[i][j]).abs());
        }
    }
    let t = (0..3)
        .map(|k| (a.t[k] - b.t[k]).powi(2))
        .sum::<f64>()
        .sqrt();
    rot * 1000.0 + t
}

/// The default depth of a dado in B, as the Joint Creator works it out.
pub fn dado_depth(p: &JointParams, tb: f64) -> f64 {
    p.depth.unwrap_or(tb / 3.0)
}

/// The default head of a twist lock, as the Joint Creator works it out.
fn twist_head(p: &JointParams, ta: f64) -> f64 {
    p.proud.unwrap_or(ta.max(10.0))
}

/// How far a stopped dado stops short of B's front edge when not asked.
pub const STOP: f64 = 20.0;

/// How far A's edge grows past its box into or through B for a joint on
/// B's face or a corner: the Joint Creator draws A to where its edge ends
/// up, not to B's face.
pub fn a_growth(kind: ConnType, joint: FJoint, p: &JointParams, ta: f64, tb: f64) -> f64 {
    match (kind, joint) {
        (ConnType::Tee | ConnType::Corner, FJoint::Dado | FJoint::StoppedDado | FJoint::Groove) => {
            dado_depth(p, tb)
        }
        (ConnType::Tee | ConnType::Corner, FJoint::TwistLock) => tb + twist_head(p, ta),
        (ConnType::Corner, FJoint::Rabbet | FJoint::Finger | FJoint::CornerDovetail) => tb,
        _ => 0.0,
    }
}

/// Which of a panel's sides (per flat index u then v, low then high) no
/// other panel touches or crosses: corners between two such may be
/// rounded, and front ones get edge banding.
pub fn free_sides(panels: &[Panel], i: usize, frame: &PanelFrame) -> [[bool; 2]; 2] {
    let p = &panels[i];
    let mut free = [[true; 2]; 2];
    for (fi, k) in [frame.u, frame.v].into_iter().enumerate() {
        for (side, plane) in [p.min[k], p.max[k]].into_iter().enumerate() {
            free[fi][side] = !panels.iter().enumerate().any(|(j, q)| {
                j != i
                    && q.min[k] <= plane + TOL
                    && q.max[k] >= plane - TOL
                    && (0..3)
                        .filter(|a| *a != k)
                        .all(|a| p.max[a].min(q.max[a]) - p.min[a].max(q.min[a]) > TOL)
            });
        }
    }
    free
}

/// Which side of panel `p` (along `k`) faces panel `q`: +1 its high side.
fn side_towards(p: &Panel, q: &Panel, k: usize) -> f64 {
    if (p.max[k] - q.min[k]).abs() <= TOL {
        1.0
    } else if (p.min[k] - q.max[k]).abs() <= TOL {
        -1.0
    } else if q.centre()[k] >= p.centre()[k] {
        1.0
    } else {
        -1.0
    }
}

/// Adds the growth each connection's joint gives its panels.
pub fn growth(panels: &[Panel], conns: &[Connection], joints: &[JointChoice]) -> Vec<Growth> {
    let mut g: Vec<Growth> = vec![[[0.0; 2]; 3]; panels.len()];
    let mut grow = |i: usize, k: usize, dir: f64, by: f64| {
        let side = usize::from(dir > 0.0);
        g[i][k][side] = g[i][k][side].max(by);
    };
    for (c, choice) in conns.iter().zip(joints) {
        let (pa, pb) = (&panels[c.a], &panels[c.b]);
        if c.line_touch {
            let na = pa.normal;
            // B reaches over A's end to A's outer face.
            let dir = if (pb.min[na] - pa.max[na]).abs() <= TOL {
                -1.0
            } else {
                1.0
            };
            grow(c.b, na, dir, pa.thickness);
        }
        match c.kind {
            ConnType::Tee | ConnType::Corner => {
                let by = a_growth(
                    c.kind,
                    choice.joint,
                    &choice.params,
                    pa.thickness,
                    pb.thickness,
                );
                if by > 0.0 {
                    let k = c.axis;
                    grow(c.a, k, side_towards(pa, pb, k), by);
                }
            }
            ConnType::FaceOnEdges if choice.joint == FJoint::Rabbet => {
                // B runs on to A's outer face; A sits in its rabbet.
                let k = c.axis;
                grow(c.b, k, side_towards(pb, pa, k), pa.thickness);
            }
            _ => {}
        }
    }
    g
}

/// Everything a joint layout needs to know of the panels.
pub struct Ctx<'a> {
    pub panels: &'a [Panel],
    pub frames: &'a [PanelFrame],
    pub growth: &'a [Growth],
}

impl Ctx<'_> {
    fn rect(&self, i: usize) -> Vec<Vec<P2>> {
        vec![flat_rect(&self.panels[i], &self.frames[i], &self.growth[i])]
    }

    fn expected(&self, a: usize, b: usize) -> Pose {
        self.frames[a]
            .pose()
            .inverse()
            .then_local(&self.frames[b].pose())
    }

    fn flat(&self, i: usize, p: Vec3) -> P2 {
        self.frames[i].flat(p)
    }

    /// The stretch `lo` to `hi` along axis `l` of panel `i`'s side at
    /// `side` along axis `k` (a box side), cut to where the panel's own
    /// outline runs along that side; ends at the box's ends keep their
    /// growth.
    fn span(&self, i: usize, k: usize, side: f64, l: usize, lo: f64, hi: f64) -> (f64, f64) {
        let p = &self.panels[i];
        let Some(points) = p.outline.as_ref() else {
            return (lo, hi);
        };
        let frame = &self.frames[i];
        let along: Vec<f64> = points
            .iter()
            .map(|q| {
                let mut w = p.min;
                w[frame.u] += q[0];
                w[frame.v] += q[1];
                w
            })
            .filter(|w| (w[k] - side).abs() <= TOL)
            .map(|w| w[l])
            .collect();
        let (Some(a), Some(b)) = (
            along.iter().copied().reduce(f64::min),
            along.iter().copied().reduce(f64::max),
        ) else {
            return (lo, hi);
        };
        if b - a <= TOL {
            return (lo, hi);
        }
        let from = if a <= p.min[l] + TOL { lo } else { a.max(lo) };
        let to = if b >= p.max[l] - TOL { hi } else { b.min(hi) };
        (from, to)
    }
}

fn point(values: [(usize, f64); 3]) -> Vec3 {
    let mut p = [0.0; 3];
    for (k, v) in values {
        p[k] = v;
    }
    p
}

/// Lays out connection `conn` with `choice` as a Joint Creator joint, or
/// `None` for a butt joint.
///
/// # Errors
/// An issue in plain words when the joint cannot be laid out here.
pub fn spec(
    ctx: &Ctx,
    conn_index: usize,
    c: &Connection,
    choice: &JointChoice,
) -> Result<Option<Spec>, Issue> {
    let Some(kind) = choice.joint.kind() else {
        return Ok(None);
    };
    let (pa, pb) = (&ctx.panels[c.a], &ctx.panels[c.b]);
    let (ta, tb) = (pa.thickness, pb.thickness);
    let expected = ctx.expected(c.a, c.b);
    let (ga, gb) = (&ctx.growth[c.a], &ctx.growth[c.b]);
    let (amin, amax) = grown_box(pa, ga);
    let (bmin, bmax) = grown_box(pb, gb);
    let mut params = choice.params.clone();
    let mut out = Spec {
        conn: conn_index,
        kind,
        a: c.a,
        b: c.b,
        a_edge: ([0.0; 2], [0.0; 2]),
        b_target: FlatTarget::Edge([0.0; 2], [0.0; 2]),
        params: JointParams::default(),
        flip: false,
        matched: false,
    };
    let fail = |m: String| Issue::on_connection(m, &c.key);
    match c.kind {
        ConnType::Tee | ConnType::Corner => {
            let (k, na, l) = (c.axis, pa.normal, c.line);
            let dir = side_towards(pa, pb, k);
            let a_k = if dir > 0.0 { amax[k] } else { amin[k] };
            // Fingers and tails pair the shared stretch of both edges; the
            // other joints run along A's whole (grown) side, so it is cut
            // back or seated end to end.
            let a_box = if dir > 0.0 { pa.max[k] } else { pa.min[k] };
            // Fasteners (dowels, cam locks, screws) go where the two meet.
            let (lo, hi) = if matches!(
                choice.joint,
                FJoint::Finger
                    | FJoint::CornerDovetail
                    | FJoint::Dowel
                    | FJoint::CamLock
                    | FJoint::Screws
            ) {
                shared(pa, pb, l)
            } else {
                ctx.span(c.a, k, a_box, l, amin[l], amax[l])
            };
            let a_from = ctx.flat(c.a, point([(k, a_k), (l, lo), (na, pa.min[na])]));
            let a_to = ctx.flat(c.a, point([(k, a_k), (l, hi), (na, pa.min[na])]));
            let fa = Frame::of_edge(a_from, a_to, &ctx.rect(c.a));
            out.a_edge = (a_from, a_to);
            let b_rings = ctx.rect(c.b);
            // B's side at `at` (a grown side of its box), cut to its
            // outline.
            let b_side = |at: f64, l0: f64, l1: f64| {
                let side = if (at - bmin[na]).abs() <= TOL {
                    pb.min[na]
                } else {
                    pb.max[na]
                };
                let (l0, l1) = ctx.span(c.b, na, side, l, l0, l1);
                (
                    ctx.flat(c.b, point([(na, at), (l, l0), (k, pb.min[k])])),
                    ctx.flat(c.b, point([(na, at), (l, l1), (k, pb.min[k])])),
                )
            };
            if kind.on_face() {
                let depth = a_growth(c.kind, choice.joint, &params, ta, tb);
                if choice.joint == FJoint::StoppedDado && params.stop.is_none() {
                    params.stop = Some(STOP);
                }
                let mut best: Option<(f64, Spec)> = None;
                for at in [bmin[na], bmax[na]] {
                    let (from, to) = b_side(at, bmin[l], bmax[l]);
                    let fb = Frame::of_edge(from, to, &b_rings);
                    let local = |p: Vec3| fb.local(ctx.flat(c.b, p));
                    let mid = (lo + hi) / 2.0;
                    let along = local(point([(na, pa.min[na]), (l, mid), (k, pb.min[k])]))[0];
                    let t0 = local(point([(na, pa.min[na]), (l, mid), (k, pb.min[k])]))[1];
                    let t1 = local(point([(na, pa.max[na]), (l, mid), (k, pb.min[k])]))[1];
                    let offset = -t0.max(t1);
                    for flip in [false, true] {
                        let map = tee_map_at(fa.len, ta, tb, depth, offset, along, flip);
                        let mut err = pose_error(&b_to_a(&fa, &fb, &map), &expected);
                        // A stopped dado stops at the end of B's edge its
                        // frame runs to: the front (low Y), else the top.
                        if choice.joint == FJoint::StoppedDado {
                            let run = sub3(
                                ctx.frames[c.b].pose().apply([
                                    fb.a[0] + fb.u[0],
                                    fb.a[1] + fb.u[1],
                                    0.0,
                                ]),
                                ctx.frames[c.b].pose().apply([fb.a[0], fb.a[1], 0.0]),
                            );
                            let towards_front = if l == 1 { -run[1] } else { run[2] };
                            if towards_front < 0.5 {
                                err += 0.5;
                            }
                        }
                        let candidate = Spec {
                            b_target: FlatTarget::Face {
                                from,
                                to,
                                offset,
                                along,
                            },
                            flip,
                            matched: err < 1e-3,
                            ..out.clone()
                        };
                        if best.as_ref().is_none_or(|(e, _)| err < *e - 1e-9) {
                            best = Some((err, candidate));
                        }
                    }
                }
                let Some((_, best)) = best else {
                    return Ok(None);
                };
                out = best;
            } else {
                // B's end edge, flush with A's outer face.
                let at = if (pa.min[na] - pb.min[na]).abs() <= TOL {
                    bmin[na]
                } else {
                    bmax[na]
                };
                let la = fa.len;
                let (from, to, map) = match choice.joint {
                    FJoint::Rabbet => {
                        let (from, to) = b_side(at, bmin[l], bmax[l]);
                        let fb = Frame::of_edge(from, to, &b_rings);
                        let s0 =
                            fb.local(ctx.flat(c.b, point([(na, at), (l, lo), (k, pb.min[k])])))[0];
                        let s1 =
                            fb.local(ctx.flat(c.b, point([(na, at), (l, hi), (k, pb.min[k])])))[0];
                        let start = s0.min(s1);
                        params.at_b = Some(start);
                        let map =
                            Map::axes([(0, -1.0), (2, -1.0), (1, -1.0)], [la + start, 0.0, 0.0]);
                        (from, to, map)
                    }
                    FJoint::CornerDovetail => {
                        let (from, to) = b_side(at, lo, hi);
                        (
                            from,
                            to,
                            Map::axes([(0, 1.0), (2, -1.0), (1, 1.0)], [0.0, -tb, 0.0]),
                        )
                    }
                    _ => {
                        let (from, to) = b_side(at, lo, hi);
                        (from, to, corner_map(la))
                    }
                };
                let fb = Frame::of_edge(from, to, &b_rings);
                let err = pose_error(&b_to_a(&fa, &fb, &map), &expected);
                out.b_target = FlatTarget::Edge(from, to);
                out.matched = err < 1e-3;
            }
        }
        ConnType::Edge => {
            let (k, l) = (c.axis, c.line);
            let n = pa.normal;
            let (lo, hi) = shared(pa, pb, l);
            let dir = side_towards(pa, pb, k);
            let a_k = if dir > 0.0 { amax[k] } else { amin[k] };
            let b_k = if dir > 0.0 { bmin[k] } else { bmax[k] };
            let a_from = ctx.flat(c.a, point([(k, a_k), (l, lo), (n, pa.min[n])]));
            let a_to = ctx.flat(c.a, point([(k, a_k), (l, hi), (n, pa.min[n])]));
            let b_from = ctx.flat(c.b, point([(k, b_k), (l, lo), (n, pb.min[n])]));
            let b_to = ctx.flat(c.b, point([(k, b_k), (l, hi), (n, pb.min[n])]));
            let fa = Frame::of_edge(a_from, a_to, &ctx.rect(c.a));
            let fb = Frame::of_edge(b_from, b_to, &ctx.rect(c.b));
            let err = pose_error(&b_to_a(&fa, &fb, &coplanar_map(fa.len)), &expected);
            out.a_edge = (a_from, a_to);
            out.b_target = FlatTarget::Edge(b_from, b_to);
            out.matched = err < 1e-3;
        }
        ConnType::Cross => {
            let cm = c.axis;
            let (na, nb) = (pa.normal, pb.normal);
            // B's slot comes from its end nearer A, A's from the other.
            let from_low = pa.min[cm] - pb.min[cm] <= pb.max[cm] - pa.max[cm];
            let (a_end, b_end) = if from_low {
                (amax[cm], bmin[cm])
            } else {
                (amin[cm], bmax[cm])
            };
            let da = pa.size(cm) / 2.0;
            let db = (a_end - b_end).abs() - da;
            if db <= TOL {
                return Err(fail(format!(
                    "{} and {} cross too near an end for a cross lap",
                    pa.name, pb.name
                )));
            }
            let a_side = if from_low { pa.max[cm] } else { pa.min[cm] };
            let b_side = if from_low { pb.min[cm] } else { pb.max[cm] };
            let (a0, a1) = ctx.span(c.a, cm, a_side, nb, amin[nb], amax[nb]);
            let (b0, b1) = ctx.span(c.b, cm, b_side, na, bmin[na], bmax[na]);
            let a_from = ctx.flat(c.a, point([(cm, a_end), (nb, a0), (na, pa.min[na])]));
            let a_to = ctx.flat(c.a, point([(cm, a_end), (nb, a1), (na, pa.min[na])]));
            let b_from = ctx.flat(c.b, point([(cm, b_end), (na, b0), (nb, pb.min[nb])]));
            let b_to = ctx.flat(c.b, point([(cm, b_end), (na, b1), (nb, pb.min[nb])]));
            let fa = Frame::of_edge(a_from, a_to, &ctx.rect(c.a));
            let fb = Frame::of_edge(b_from, b_to, &ctx.rect(c.b));
            let mid_b = (pb.min[nb] + pb.max[nb]) / 2.0;
            let mid_a = (pa.min[na] + pa.max[na]) / 2.0;
            let at_a =
                fa.local(ctx.flat(c.a, point([(cm, a_end), (nb, mid_b), (na, pa.min[na])])))[0];
            let at_b =
                fb.local(ctx.flat(c.b, point([(cm, b_end), (na, mid_a), (nb, pb.min[nb])])))[0];
            params.at_a = Some(at_a);
            params.at_b = Some(at_b);
            params.depth = Some(da);
            params.depth_b = Some(db);
            let map = Map::axes(
                [(2, 1.0), (1, -1.0), (0, 1.0)],
                [at_a - tb / 2.0, -(da + db), ta / 2.0 - at_b],
            );
            let err = pose_error(&b_to_a(&fa, &fb, &map), &expected);
            out.a_edge = (a_from, a_to);
            out.b_target = FlatTarget::Edge(b_from, b_to);
            out.matched = err < 1e-3;
        }
        ConnType::FaceOnEdges => {
            // A (thin) sits in a rabbet along B's edge, its border edge on
            // B's outer face.
            let (k, nb, l) = (c.axis, pb.normal, c.line);
            let dir_b = side_towards(pb, pa, k);
            let b_k = if dir_b > 0.0 { bmax[k] } else { bmin[k] };
            let outer = if (pb.min[nb] - pa.min[nb]).abs() <= TOL {
                amin[nb]
            } else {
                amax[nb]
            };
            let (lo, hi) = shared(pa, pb, l);
            let a_from = ctx.flat(c.a, point([(nb, outer), (l, lo), (k, pa.min[k])]));
            let a_to = ctx.flat(c.a, point([(nb, outer), (l, hi), (k, pa.min[k])]));
            let b_from = ctx.flat(c.b, point([(k, b_k), (l, bmin[l]), (nb, pb.min[nb])]));
            let b_to = ctx.flat(c.b, point([(k, b_k), (l, bmax[l]), (nb, pb.min[nb])]));
            let fa = Frame::of_edge(a_from, a_to, &ctx.rect(c.a));
            let fb = Frame::of_edge(b_from, b_to, &ctx.rect(c.b));
            let s0 = fb.local(ctx.flat(c.b, point([(k, b_k), (l, lo), (nb, pb.min[nb])])))[0];
            let s1 = fb.local(ctx.flat(c.b, point([(k, b_k), (l, hi), (nb, pb.min[nb])])))[0];
            let start = s0.min(s1);
            params.at_b = Some(start);
            let map = Map::axes(
                [(0, -1.0), (2, -1.0), (1, -1.0)],
                [fa.len + start, 0.0, 0.0],
            );
            let err = pose_error(&b_to_a(&fa, &fb, &map), &expected);
            out.a_edge = (a_from, a_to);
            out.b_target = FlatTarget::Edge(b_from, b_to);
            out.matched = err < 1e-3;
        }
    }
    out.params = params;
    Ok(Some(out))
}

fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Which way each panel's top faces: towards the panels whose joints sink
/// pockets into it (a dado, a rabbet, dowel holes), away from those whose
/// sockets are cut from its outside, else towards the middle of the piece.
pub fn tops(
    panels: &[Panel],
    conns: &[Connection],
    joints: &[JointChoice],
    models: &[Option<f64>],
    double_sided: bool,
) -> Vec<f64> {
    let middle = middle_of(panels);
    // Features sunk into a face (pockets, hinge cups, pins) count as much
    // as a joint's pockets.
    let mut votes: Vec<f64> = panels
        .iter()
        .map(|p| {
            p.features
                .iter()
                .filter_map(|f| super::features::feature_face(p, f, middle))
                .sum()
        })
        .collect();
    for (c, choice) in conns.iter().zip(joints) {
        let (pa, pb) = (&panels[c.a], &panels[c.b]);
        let nb = pb.normal;
        let towards = (pa.centre()[nb] - pb.centre()[nb]).signum();
        if pockets_into_b(c, choice) {
            votes[c.b] += towards;
        } else if choice.joint.from_outside() {
            votes[c.b] -= towards;
        }
    }
    panels
        .iter()
        .zip(votes)
        .zip(models)
        .map(|((p, vote), model)| {
            // Models choose a panel's top in a one sided job (the joints
            // that need the other face are said); in a double sided job
            // the joints choose and the models are carved from below.
            if let Some(face) = model.filter(|_| !double_sided || vote == 0.0) {
                face.signum()
            } else if vote.abs() > 0.0 {
                vote.signum()
            } else {
                let d = middle[p.normal] - p.centre()[p.normal];
                if d < -TOL {
                    -1.0
                } else {
                    1.0
                }
            }
        })
        .collect()
}

/// The middle of the boxes of all panels.
pub fn middle_of(panels: &[Panel]) -> Vec3 {
    let (min, max) = bounds(panels);
    [0, 1, 2].map(|k| (min[k] + max[k]) / 2.0)
}

/// The box round all panels' boxes.
pub fn bounds(panels: &[Panel]) -> (Vec3, Vec3) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in panels {
        for k in 0..3 {
            min[k] = min[k].min(p.min[k]);
            max[k] = max[k].max(p.max[k]);
        }
    }
    if panels.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }
    (min, max)
}

/// Whether `c`'s joint needs B's top towards A (and B's top is not), so
/// the joint cannot be cut from one side; the issue says so.
pub fn wrong_face(
    panels: &[Panel],
    c: &Connection,
    choice: &JointChoice,
    up: &[f64],
) -> Option<Issue> {
    let (pa, pb) = (&panels[c.a], &panels[c.b]);
    let nb = pb.normal;
    let towards = (pa.centre()[nb] - pb.centre()[nb]).signum();
    let needs = if pockets_into_b(c, choice) {
        towards
    } else if choice.joint.from_outside() {
        -towards
    } else {
        return None;
    };
    if needs == up[c.b] {
        return None;
    }
    let mut issue = Issue::on_connection(
        format!(
            "{} needs pockets on both faces: its {} for {} would have to be cut from below. Use a through joint (Tab and slot) here, or cut that panel from both sides",
            pb.name,
            choice.joint.title().to_lowercase(),
            pa.name
        ),
        &c.key,
    );
    issue.suggest = Some("tabSlot".into());
    Some(issue)
}

/// Whether `choice` on connection `c` sinks pockets into B's face from the
/// top, so B's top must face A. Dowels edge to edge or into the edges a
/// panel lies on are drilled by hand at marks.
pub fn pockets_into_b(c: &Connection, choice: &JointChoice) -> bool {
    if choice.joint == FJoint::Dowel && matches!(c.kind, ConnType::Edge | ConnType::FaceOnEdges) {
        return false;
    }
    choice.joint.pockets_b()
        || (choice.joint == FJoint::MortiseTenon && choice.params.blind == Some(true))
}
