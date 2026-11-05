//! Mates between the Assembly View's pieces: constraints kept with the
//! document, solved again whenever a piece moves or a mate changes.
//!
//! A mate ties a face (or a round axis) of one piece to one of another:
//! face against face, flush (in line, looking the same way), a set
//! distance apart, a set angle apart, or two round axes in line
//! (concentric). Each leaves the rest of the moving piece's freedom alone,
//! so a panel laid against a face still slides along it and turns on it.
//!
//! The solver is sequential: grounded pieces stay put, then each piece
//! mated to a piece already placed is moved the least that satisfies its
//! mates to the placed pieces, outward from the grounded ones, then from
//! the piece the user just moved, then from the first piece of any group
//! still left. A mate whose two pieces are both placed before it is met
//! and is not satisfied is a conflict; it is reported, never looped on.

use serde::{Deserialize, Serialize};

use super::pieces::{Anchor, PieceRef};
use super::pose::{self, Pose, Vec3};

/// A mate is met when its faces are within this many degrees of the
/// angle it wants.
pub const ANGLE_TOLERANCE: f64 = 0.01;

/// A mate is met when its faces are within this of the distance it
/// wants, mm.
pub const DISTANCE_TOLERANCE: f64 = 0.001;

/// Most rounds a piece with several mates is moved in before its mates
/// are taken as they stand.
const PASSES: usize = 64;

/// What a mate holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PieceMateKind {
    /// The faces lie on each other, looking at each other, `value` apart.
    #[default]
    Against,
    /// The faces lie in line, looking the same way, `value` apart.
    Flush,
    /// The faces are parallel and `value` apart, looking the way they
    /// looked when the mate was made (`aligned`).
    Distance,
    /// The faces' normals are `value` degrees apart.
    Angle,
    /// The round axes lie on one line.
    Concentric,
    /// The pieces sit together as a Joint Creator joint puts them: fixed
    /// to each other in every direction.
    Joint,
}

/// A face (or a round axis) of a piece in its own frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MateFace {
    /// A point on the face, or on the axis, mm.
    pub point: Vec3,
    /// The face's outward unit normal, or the axis direction.
    pub normal: Vec3,
    /// The radius of a round face, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A mate between two pieces. `a` is the piece whose face stayed put when
/// the mate was made; the solver may still move either.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceMate {
    /// Id, from the assembly's id counter.
    pub id: u64,
    /// What it holds.
    #[serde(default)]
    pub kind: PieceMateKind,
    /// The first piece.
    pub a: PieceRef,
    /// Its face.
    pub a_face: MateFace,
    /// The second piece.
    pub b: PieceRef,
    /// Its face.
    pub b_face: MateFace,
    /// The gap (mm, along the first face's normal) or the angle (degrees).
    #[serde(default)]
    pub value: f64,
    /// For a distance: the faces look the same way.
    #[serde(default, skip_serializing_if = "is_false")]
    pub aligned: bool,
    /// Left out of the solve and the checks until switched back on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub suppressed: bool,
    /// Where the first piece's part lay on its sheet, so the mate stays on
    /// that part when a toolpath edit numbers the parts again; `None` for a
    /// component piece, a raw model piece or an older file (by number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_anchor: Option<Anchor>,
    /// The same for the second piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_anchor: Option<Anchor>,
    /// The Joint Creator joint a [`PieceMateKind::Joint`] mate keeps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<u64>,
    /// For a joint mate that seats an extra part of the joint (a key in its
    /// recess): the part's vector; the second piece is that part's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u64>,
    /// For a joint mate: the second piece's frame in the first's, as the
    /// joint puts them, worked out from the joint each time the mates are
    /// taken up (never saved). `None` leaves the mate unmet.
    #[serde(skip)]
    pub lock: Option<Pose>,
}

impl PieceMate {
    /// Whether the mate names `piece`.
    pub fn involves(&self, piece: PieceRef) -> bool {
        self.a == piece || self.b == piece
    }

    /// Which way the second face's normal should look against the first's:
    /// 1 the same way, -1 the other way; `None` for the kinds that hold no
    /// face to a plane.
    fn sign(&self) -> Option<f64> {
        match self.kind {
            PieceMateKind::Against => Some(-1.0),
            PieceMateKind::Flush => Some(1.0),
            PieceMateKind::Distance => Some(if self.aligned { 1.0 } else { -1.0 }),
            PieceMateKind::Angle | PieceMateKind::Concentric | PieceMateKind::Joint => None,
        }
    }
}

/// How a mate stands after a solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MateState {
    /// Met.
    Solved,
    /// Not met: other mates or grounded pieces hold its pieces elsewhere.
    Conflict,
    /// Switched off.
    Suppressed,
    /// One of its pieces is gone.
    Missing,
}

/// A mate's state and how far off it is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MateReport {
    /// The mate.
    pub id: u64,
    /// How it stands.
    pub state: MateState,
    /// How far the faces are from where the mate wants them, mm.
    pub distance_error: f64,
    /// How far they are turned from the angle it wants, degrees.
    pub angle_error: f64,
}

/// A piece as the solver moves it: its place as stored with the pieces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolvePiece {
    /// Which piece.
    pub key: PieceRef,
    /// Where the middle of its part box sits, mm.
    pub position: Vec3,
    /// Its turns about X, then Y, then Z, degrees.
    pub rotation: Vec3,
    /// Its part box: width, height, thickness, mm.
    pub size: Vec3,
    /// Grounded pieces never move.
    pub grounded: bool,
}

impl SolvePiece {
    /// Its local frame to assembly space, as [`super::pieces::Piece::pose`].
    pub fn pose(&self) -> Pose {
        Pose::translation(self.position)
            .then_local(&Pose::euler(self.rotation))
            .then_local(&Pose::translation(pose::scale(self.size, -0.5)))
    }

    /// Puts the piece where its local frame is `pose`.
    fn set_pose(&mut self, pose: &Pose) {
        self.position = pose
            .apply(pose::scale(self.size, 0.5))
            .map(|v| (v * 1e9).round() / 1e9);
        self.rotation = pose.euler_angles();
    }

    /// Moves the piece by `motion`, a rigid move of assembly space.
    fn moved_by(&mut self, motion: &Pose) {
        let placed = motion
            .then_local(&Pose::translation(self.position))
            .then_local(&Pose::euler(self.rotation));
        self.position = placed.t.map(|v| (v * 1e9).round() / 1e9);
        self.rotation = placed.euler_angles();
    }
}

/// A face in assembly space.
#[derive(Debug, Clone, Copy)]
struct WorldFace {
    point: Vec3,
    normal: Vec3,
}

fn world(face: &MateFace, pose: &Pose) -> WorldFace {
    WorldFace {
        point: pose.apply(face.point),
        normal: pose::normalized(pose.apply_vector(face.normal)),
    }
}

fn angle_between(u: Vec3, v: Vec3) -> f64 {
    let c = pose::dot(u, v);
    let s = pose::dot(pose::cross(u, v), pose::cross(u, v)).sqrt();
    s.atan2(c).to_degrees()
}

/// Some unit direction square to `v`.
fn square_to(v: Vec3) -> Vec3 {
    let guide = if v[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    pose::normalized(pose::cross(v, guide))
}

/// The least turn about `pivot` that brings direction `from` onto `to`.
fn turn_onto(from: Vec3, to: Vec3, pivot: Vec3) -> Pose {
    let axis = pose::cross(from, to);
    let length = pose::dot(axis, axis).sqrt();
    let angle = angle_between(from, to);
    if angle < 1e-12 {
        return Pose::IDENTITY;
    }
    let axis = if length < 1e-12 {
        square_to(from)
    } else {
        pose::scale(axis, 1.0 / length)
    };
    Pose::rotation_about(pivot, axis, angle)
}

/// How far the mate is from met, mm and degrees, with the pieces at
/// `pose_a` and `pose_b`.
pub fn mate_error(mate: &PieceMate, pose_a: &Pose, pose_b: &Pose) -> (f64, f64) {
    if mate.kind == PieceMateKind::Joint {
        return match mate.lock {
            Some(lock) => lock_error(&lock, mate.b_face.point, pose_a, pose_b),
            None => (f64::INFINITY, f64::INFINITY),
        };
    }
    let fa = world(&mate.a_face, pose_a);
    let fb = world(&mate.b_face, pose_b);
    match mate.kind {
        PieceMateKind::Angle => (
            0.0,
            (angle_between(fa.normal, fb.normal) - mate.value).abs(),
        ),
        PieceMateKind::Concentric => {
            let d = fa.normal;
            let w = pose::sub(fb.point, fa.point);
            let off = pose::sub(w, pose::scale(d, pose::dot(w, d)));
            let angle = angle_between(d, fb.normal);
            (pose::dot(off, off).sqrt(), angle.min(180.0 - angle))
        }
        _ => {
            let s = mate.sign().unwrap_or(-1.0);
            let angle = angle_between(pose::scale(fa.normal, s), fb.normal);
            let gap = pose::dot(pose::sub(fb.point, fa.point), fa.normal);
            ((gap - mate.value).abs(), angle)
        }
    }
}

/// How far the second piece at `pose_b` is from where `lock` puts it
/// against the first at `pose_a`: the distance `point` (in the second
/// piece's frame) is off, mm, and the turn between them, degrees.
fn lock_error(lock: &Pose, point: Vec3, pose_a: &Pose, pose_b: &Pose) -> (f64, f64) {
    let want = pose_a.then_local(lock);
    let off = pose::sub(want.apply(point), pose_b.apply(point));
    let mut trace = 0.0;
    for i in 0..3 {
        for k in 0..3 {
            trace += want.r[k][i] * pose_b.r[k][i];
        }
    }
    let angle = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees();
    (pose::dot(off, off).sqrt(), angle)
}

fn met(error: (f64, f64)) -> bool {
    error.0 <= DISTANCE_TOLERANCE && error.1 <= ANGLE_TOLERANCE
}

/// Moves `moving` the least that meets `mate` with the other piece held at
/// `fixed`; `fixed_is_a` says which side of the mate the held piece is.
fn project(mate: &PieceMate, fixed_is_a: bool, fixed: &Pose, moving: &mut SolvePiece) {
    let (fixed_face, moving_face) = if fixed_is_a {
        (&mate.a_face, &mate.b_face)
    } else {
        (&mate.b_face, &mate.a_face)
    };
    if mate.kind == PieceMateKind::Joint {
        if let Some(lock) = mate.lock {
            let want = if fixed_is_a {
                fixed.then_local(&lock)
            } else {
                fixed.then_local(&lock.inverse())
            };
            moving.set_pose(&want);
        }
        return;
    }
    let f = world(fixed_face, fixed);
    let m = world(moving_face, &moving.pose());
    match mate.kind {
        PieceMateKind::Angle => {
            let now = angle_between(f.normal, m.normal);
            let axis = pose::cross(f.normal, m.normal);
            let length = pose::dot(axis, axis).sqrt();
            let axis = if length < 1e-12 {
                square_to(m.normal)
            } else {
                pose::scale(axis, 1.0 / length)
            };
            moving.moved_by(&Pose::rotation_about(m.point, axis, mate.value - now));
        }
        PieceMateKind::Concentric => {
            let want = if pose::dot(f.normal, m.normal) >= 0.0 {
                f.normal
            } else {
                pose::scale(f.normal, -1.0)
            };
            moving.moved_by(&turn_onto(m.normal, want, m.point));
            let m = world(moving_face, &moving.pose());
            let w = pose::sub(f.point, m.point);
            let off = pose::sub(w, pose::scale(f.normal, pose::dot(w, f.normal)));
            moving.moved_by(&Pose::translation(off));
        }
        _ => {
            let s = mate.sign().unwrap_or(-1.0);
            moving.moved_by(&turn_onto(m.normal, pose::scale(f.normal, s), m.point));
            let m = world(moving_face, &moving.pose());
            // The gap runs along the first piece's normal from its face.
            let want = if fixed_is_a {
                mate.value
            } else {
                -s * mate.value
            };
            let now = pose::dot(pose::sub(m.point, f.point), f.normal);
            moving.moved_by(&Pose::translation(pose::scale(f.normal, want - now)));
        }
    }
}

/// Solves `mates` over `pieces`, moving the pieces that are not grounded,
/// and says how each mate stands. `driver` is the piece the user just
/// moved: it keeps its new place as far as the grounded pieces allow.
pub fn solve(
    pieces: &mut [SolvePiece],
    mates: &[PieceMate],
    driver: Option<PieceRef>,
) -> Vec<MateReport> {
    let keys: Vec<PieceRef> = pieces.iter().map(|p| p.key).collect();
    let index = |key: PieceRef| keys.iter().position(|k| *k == key);
    // Live mates with the indices of their pieces.
    let live: Vec<(&PieceMate, usize, usize)> = mates
        .iter()
        .filter(|m| !m.suppressed && m.a != m.b)
        .filter_map(|m| Some((m, index(m.a)?, index(m.b)?)))
        .collect();
    let n = pieces.len();
    let mut placed = vec![false; n];
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let fix = |i: usize, placed: &mut Vec<bool>, queue: &mut std::collections::VecDeque<usize>| {
        placed[i] = true;
        for (_, a, b) in &live {
            let other = if *a == i {
                *b
            } else if *b == i {
                *a
            } else {
                continue;
            };
            if !placed[other] {
                queue.push_back(other);
            }
        }
    };
    let grounded: Vec<usize> = (0..n).filter(|i| pieces[*i].grounded).collect();
    for i in grounded {
        fix(i, &mut placed, &mut queue);
    }
    let mut driver_used = false;
    loop {
        while let Some(i) = queue.pop_front() {
            if placed[i] {
                continue;
            }
            let held: Vec<(&PieceMate, bool, usize)> = live
                .iter()
                .filter_map(|(m, a, b)| {
                    if *b == i && placed[*a] {
                        Some((*m, true, *a))
                    } else if *a == i && placed[*b] {
                        Some((*m, false, *b))
                    } else {
                        None
                    }
                })
                .collect();
            for _ in 0..PASSES {
                for (mate, fixed_is_a, other) in &held {
                    let fixed = pieces[*other].pose();
                    project(mate, *fixed_is_a, &fixed, &mut pieces[i]);
                }
                let all_met = held.iter().all(|(mate, fixed_is_a, other)| {
                    let (pa, pb) = if *fixed_is_a {
                        (pieces[*other].pose(), pieces[i].pose())
                    } else {
                        (pieces[i].pose(), pieces[*other].pose())
                    };
                    met(mate_error(mate, &pa, &pb))
                });
                if all_met || held.len() == 1 {
                    break;
                }
            }
            fix(i, &mut placed, &mut queue);
        }
        if !driver_used {
            driver_used = true;
            if let Some(i) = driver.and_then(index) {
                if !placed[i] {
                    fix(i, &mut placed, &mut queue);
                    continue;
                }
            }
        }
        // A group with nothing grounded or moved: its first mate's first
        // piece stays.
        match live.iter().find(|(_, a, b)| !placed[*a] && !placed[*b]) {
            Some((_, a, _)) => fix(*a, &mut placed, &mut queue),
            None => break,
        }
    }
    report(pieces, mates)
}

/// How each mate stands with the pieces where they are.
pub fn report(pieces: &[SolvePiece], mates: &[PieceMate]) -> Vec<MateReport> {
    let pose_of = |key: PieceRef| pieces.iter().find(|p| p.key == key).map(SolvePiece::pose);
    mates
        .iter()
        .map(|m| {
            let (state, error) = if m.suppressed {
                (MateState::Suppressed, (0.0, 0.0))
            } else {
                let unlocked = m.kind == PieceMateKind::Joint && m.lock.is_none();
                match (pose_of(m.a), pose_of(m.b)) {
                    (Some(pa), Some(pb)) if m.a != m.b && !unlocked => {
                        let error = mate_error(m, &pa, &pb);
                        // A little slack over the solve's own tolerance:
                        // stored places are rounded.
                        let ok = error.0 <= DISTANCE_TOLERANCE * 10.0
                            && error.1 <= ANGLE_TOLERANCE * 10.0;
                        (
                            if ok {
                                MateState::Solved
                            } else {
                                MateState::Conflict
                            },
                            error,
                        )
                    }
                    _ => (MateState::Missing, (0.0, 0.0)),
                }
            };
            MateReport {
                id: m.id,
                state,
                distance_error: (error.0 * 1e6).round() / 1e6,
                angle_error: (error.1 * 1e6).round() / 1e6,
            }
        })
        .collect()
}

/// A mate's value as it stands with the pieces where they are: the gap
/// along the first face's normal, or the angle between the normals; and
/// whether the faces look the same way.
pub fn current_value(
    kind: PieceMateKind,
    a: &MateFace,
    pose_a: &Pose,
    b: &MateFace,
    pose_b: &Pose,
) -> (f64, bool) {
    let fa = world(a, pose_a);
    let fb = world(b, pose_b);
    let aligned = pose::dot(fa.normal, fb.normal) > 0.0;
    let value = match kind {
        PieceMateKind::Angle => angle_between(fa.normal, fb.normal),
        PieceMateKind::Concentric | PieceMateKind::Joint => 0.0,
        _ => pose::dot(pose::sub(fb.point, fa.point), fa.normal),
    };
    ((value * 1e6).round() / 1e6, aligned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(sheet: u64) -> PieceRef {
        PieceRef {
            sheet,
            part: 1,
            component: None,
        }
    }

    /// A 100 x 50 x 10 panel lying flat with its middle at `at`.
    fn panel(sheet: u64, at: Vec3, grounded: bool) -> SolvePiece {
        SolvePiece {
            key: key(sheet),
            position: at,
            rotation: [0.0; 3],
            size: [100.0, 50.0, 10.0],
            grounded,
        }
    }

    fn face(point: Vec3, normal: Vec3) -> MateFace {
        MateFace {
            point,
            normal,
            radius: None,
        }
    }

    fn mate(
        id: u64,
        kind: PieceMateKind,
        a: u64,
        fa: MateFace,
        b: u64,
        fb: MateFace,
        value: f64,
    ) -> PieceMate {
        PieceMate {
            id,
            kind,
            a: key(a),
            a_face: fa,
            b: key(b),
            b_face: fb,
            value,
            aligned: false,
            suppressed: false,
            a_anchor: None,
            b_anchor: None,
            joint: None,
            part: None,
            lock: None,
        }
    }

    const TOP: Vec3 = [0.0, 0.0, 1.0];
    const UNDER: Vec3 = [0.0, 0.0, -1.0];

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    #[test]
    fn against_lays_the_underside_on_the_top_and_keeps_the_slide() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            panel(2, [30.0, 12.0, 40.0], false),
        ];
        let mates = vec![mate(
            1,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 0.0], UNDER),
            0.0,
        )];
        let report = solve(&mut pieces, &mates, None);
        assert_eq!(report[0].state, MateState::Solved);
        // Lifted down onto the top (z 10 to 20), left where it was along the face.
        assert!(
            close(pieces[1].position, [30.0, 12.0, 15.0]),
            "{:?}",
            pieces[1].position
        );
        assert!(close(pieces[0].position, [0.0, 0.0, 5.0]));
    }

    #[test]
    fn a_gap_turns_a_piece_over_and_flush_lines_up_the_ends() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            panel(2, [0.0, 0.0, 50.0], false),
        ];
        // The second piece's top against the first's top: it turns over.
        let mut mates = vec![mate(
            1,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 10.0], TOP),
            2.0,
        )];
        // Its right end in line with the first piece's right end.
        mates.push(mate(
            2,
            PieceMateKind::Flush,
            1,
            face([100.0, 25.0, 5.0], [1.0, 0.0, 0.0]),
            2,
            face([100.0, 25.0, 5.0], [1.0, 0.0, 0.0]),
            0.0,
        ));
        let report = solve(&mut pieces, &mates, None);
        assert!(
            report.iter().all(|r| r.state == MateState::Solved),
            "{report:?}"
        );
        let pose = pieces[1].pose();
        // Its top faces down, 2 mm over the first piece's top.
        let top = pose.apply([50.0, 25.0, 10.0]);
        assert!((top[2] - 12.0).abs() < 1e-6, "{top:?}");
        assert!(pose.apply_vector(TOP)[2] < -0.999);
        let end = pose.apply([100.0, 25.0, 5.0]);
        assert!((end[0] - 50.0).abs() < 1e-6, "{end:?}");
    }

    #[test]
    fn distance_and_angle_hold_their_values() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            panel(2, [0.0, 80.0, 5.0], false),
        ];
        let mut d = mate(
            1,
            PieceMateKind::Distance,
            1,
            face([50.0, 50.0, 5.0], [0.0, 1.0, 0.0]),
            2,
            face([50.0, 0.0, 5.0], [0.0, -1.0, 0.0]),
            12.5,
        );
        d.aligned = false;
        let report = solve(&mut pieces, &[d.clone()], None);
        assert_eq!(report[0].state, MateState::Solved);
        let near = pieces[1].pose().apply([50.0, 0.0, 5.0]);
        assert!((near[1] - (25.0 + 12.5)).abs() < 1e-6, "{near:?}");

        let angle = mate(
            2,
            PieceMateKind::Angle,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 10.0], TOP),
            90.0,
        );
        let report = solve(&mut pieces, &[d, angle], None);
        assert!(
            report.iter().all(|r| r.state == MateState::Solved),
            "{report:?}"
        );
        let up = pieces[1].pose().apply_vector(TOP);
        assert!(up[2].abs() < 1e-6, "{up:?}");
    }

    #[test]
    fn concentric_brings_a_peg_onto_the_axis_of_a_hole() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            SolvePiece {
                key: key(2),
                position: [7.0, -3.0, 30.0],
                rotation: [0.0, 20.0, 0.0],
                size: [8.0, 8.0, 30.0],
                grounded: false,
            },
        ];
        let hole = MateFace {
            point: [20.0, 20.0, 5.0],
            normal: TOP,
            radius: Some(4.1),
        };
        let peg = MateFace {
            point: [4.0, 4.0, 0.0],
            normal: TOP,
            radius: Some(4.0),
        };
        let m = mate(1, PieceMateKind::Concentric, 1, hole, 2, peg, 0.0);
        let report = solve(&mut pieces, &[m], None);
        assert_eq!(report[0].state, MateState::Solved, "{report:?}");
        let pose = pieces[1].pose();
        let foot = pose.apply([4.0, 4.0, 0.0]);
        let top = pose.apply([4.0, 4.0, 30.0]);
        // On the hole's axis (world X -30, Y -5 from the panel's corner at -50, -25).
        for p in [foot, top] {
            assert!(
                (p[0] + 30.0).abs() < 1e-6 && (p[1] + 5.0).abs() < 1e-6,
                "{p:?}"
            );
        }
    }

    #[test]
    fn a_mate_between_grounded_pieces_that_is_not_met_is_a_conflict() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            panel(2, [0.0, 0.0, 50.0], true),
        ];
        let m = mate(
            1,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 0.0], UNDER),
            0.0,
        );
        let report = solve(&mut pieces, std::slice::from_ref(&m), None);
        assert_eq!(report[0].state, MateState::Conflict);
        assert!((report[0].distance_error - 35.0).abs() < 1e-6);
        assert!(close(pieces[1].position, [0.0, 0.0, 50.0]));
        let mut off = m;
        off.suppressed = true;
        assert_eq!(
            solve(&mut pieces, &[off], None)[0].state,
            MateState::Suppressed
        );
    }

    #[test]
    fn the_moved_piece_leads_an_ungrounded_group_and_the_rest_follow() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], false),
            panel(2, [0.0, 0.0, 15.0], false),
        ];
        let m = mate(
            1,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 0.0], UNDER),
            0.0,
        );
        // The second piece was lifted: the first follows it up.
        pieces[1].position = [0.0, 0.0, 40.0];
        solve(&mut pieces, std::slice::from_ref(&m), Some(key(2)));
        assert!(close(pieces[1].position, [0.0, 0.0, 40.0]));
        assert!(
            close(pieces[0].position, [0.0, 0.0, 30.0]),
            "{:?}",
            pieces[0].position
        );
        // With the first grounded, the lifted piece drops back onto it.
        pieces[0].position = [0.0, 0.0, 5.0];
        pieces[0].grounded = true;
        solve(&mut pieces, &[m], Some(key(2)));
        assert!(
            close(pieces[1].position, [0.0, 0.0, 15.0]),
            "{:?}",
            pieces[1].position
        );
    }

    #[test]
    fn mates_that_fight_are_reported_and_the_solve_ends() {
        let mut pieces = vec![
            panel(1, [0.0, 0.0, 5.0], true),
            panel(2, [0.0, 0.0, 50.0], false),
        ];
        let a = mate(
            1,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 0.0], UNDER),
            0.0,
        );
        let b = mate(
            2,
            PieceMateKind::Against,
            1,
            face([50.0, 25.0, 10.0], TOP),
            2,
            face([50.0, 25.0, 0.0], UNDER),
            5.0,
        );
        let report = solve(&mut pieces, &[a, b], None);
        assert!(
            report.iter().any(|r| r.state == MateState::Conflict),
            "{report:?}"
        );
    }
    #[test]
    fn a_distance_mate_turns_the_second_face_to_look_at_or_along_the_first() {
        // Two panels whose mated faces look at each other. Anti-aligned, the
        // second keeps its bearing; aligned, it turns round so its face looks
        // the same way as the first. Either way its face ends 12.5 from the
        // first panel's face at y 25.
        for aligned in [false, true] {
            let mut pieces = vec![
                panel(1, [0.0, 0.0, 5.0], true),
                panel(2, [0.0, 80.0, 5.0], false),
            ];
            let mut d = mate(
                1,
                PieceMateKind::Distance,
                1,
                face([50.0, 50.0, 5.0], [0.0, 1.0, 0.0]),
                2,
                face([50.0, 0.0, 5.0], [0.0, -1.0, 0.0]),
                12.5,
            );
            d.aligned = aligned;
            let report = solve(&mut pieces, &[d], None);
            assert_eq!(report[0].state, MateState::Solved, "{aligned}");
            let normal = pieces[1].pose().apply_vector([0.0, -1.0, 0.0]);
            let want = if aligned { 1.0 } else { -1.0 };
            assert!(
                (normal[1] - want).abs() < 1e-6,
                "aligned {aligned}: the face looks {normal:?}"
            );
            let near = pieces[1].pose().apply([50.0, 0.0, 5.0]);
            assert!((near[1] - 37.5).abs() < 1e-6, "aligned {aligned}: {near:?}");
        }
    }
}
