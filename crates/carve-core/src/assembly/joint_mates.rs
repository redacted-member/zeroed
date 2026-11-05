//! Joint mates: the kept mate each Joint Creator joint has between the two
//! Assembly View pieces its parts are cut into.
//!
//! A joint mate holds the second piece where the joint puts it against the
//! first, fixed in every direction. What it holds is never saved: each
//! time the mates are taken up it is worked out again from the joint (as
//! last made) and the job boxes of the two parts as the toolpaths cut them
//! now, so a toolpath edit or a changed fit, tool or thickness never leaves
//! the parts apart.

use crate::joints::JointSet;

use super::mates::{self, PieceMate, PieceMateKind, SolvePiece};
use super::pieces::PieceInfo;
use super::pose::Pose;
use super::Assembly;

/// A pose from a column major 4 by 4 matrix.
pub fn matrix_pose(m: &[f64; 16]) -> Pose {
    Pose {
        r: [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]],
        t: [m[12], m[13], m[14]],
    }
}

/// Piece `b`'s frame in piece `a`'s as joint `joint` of `set` puts them,
/// `None` when the joint is gone or not made yet. With `part`, `b` is that
/// extra part of the joint (a key), seated where the joint puts it.
pub fn lock(
    set: &JointSet,
    joint: u64,
    part: Option<u64>,
    a: &PieceInfo,
    b: &PieceInfo,
) -> Option<Pose> {
    let joint = set.joints.iter().find(|j| j.id == joint)?;
    let ja = a.sheet_parts.job_box(a.local)?;
    if let Some(part) = part {
        // A seat is kept from the corner of the part's box as drawn; the
        // piece's frame starts at its box on the simulation grid, within a
        // cell or so of it while the part is cut where it was drawn.
        let made = joint.vectors.iter().find(|v| v.id == part)?;
        let seat = matrix_pose(&made.seat?);
        let jb = b.sheet_parts.job_box(b.local)?;
        let cell = b.sheet_parts.stock.cell;
        let shift = made
            .corner
            .map(|c| [jb[0] - c[0], jb[1] - c[1]])
            .filter(|d| d[0].abs() <= 2.0 * cell && d[1].abs() <= 2.0 * cell)
            .unwrap_or([0.0, 0.0]);
        return Some(
            Pose::translation([-ja[0], -ja[1], 0.0])
                .then_local(&seat)
                .then_local(&Pose::translation([shift[0], shift[1], 0.0])),
        );
    }
    let b_to_a = joint.b_to_a?;
    let jb = b.sheet_parts.job_box(b.local)?;
    // A piece's frame starts at its part's job box corner, on its underside.
    Some(
        Pose::translation([-ja[0], -ja[1], 0.0])
            .then_local(&matrix_pose(&b_to_a))
            .then_local(&Pose::translation([jb[0], jb[1], 0.0])),
    )
}

/// Fills in what each joint mate of `mates` holds, its ends named by the
/// pieces of `infos` they join now.
pub fn fill_locks(set: &JointSet, mates: &mut [PieceMate], infos: &[PieceInfo]) {
    for mate in mates.iter_mut() {
        if mate.kind != PieceMateKind::Joint {
            continue;
        }
        let find = |key| infos.iter().find(|i| i.piece.key() == key);
        mate.lock = match (mate.joint, find(mate.a), find(mate.b)) {
            (Some(joint), Some(a), Some(b)) => lock(set, joint, mate.part, a, b),
            _ => None,
        };
    }
}

/// Moves the pieces of `infos` that a joint mate no longer holds where the
/// joint puts them (after a toolpath edit or a joint change); nothing is
/// saved, the next change that solves the mates keeps them.
pub fn settle(assembly: &Assembly, infos: &mut [PieceInfo]) {
    if !assembly
        .piece_mates
        .iter()
        .any(|m| m.kind == PieceMateKind::Joint && !m.suppressed)
    {
        return;
    }
    let joint_mates: Vec<PieceMate> = super::pieces_cmd::mates_now(assembly, infos)
        .into_iter()
        .filter(|m| m.kind == PieceMateKind::Joint && !m.suppressed && m.lock.is_some())
        .collect();
    if joint_mates.is_empty() {
        return;
    }
    let mut solving: Vec<SolvePiece> = infos
        .iter()
        .map(|info| SolvePiece {
            key: info.piece.key(),
            position: info.piece.position,
            rotation: info.piece.rotation,
            size: info.size,
            grounded: info.piece.grounded,
        })
        .collect();
    let all_met = mates::report(&solving, &joint_mates)
        .iter()
        .all(|r| r.state == mates::MateState::Solved);
    if all_met {
        return;
    }
    mates::solve(&mut solving, &joint_mates, None);
    for (info, solved) in infos.iter_mut().zip(&solving) {
        info.piece.position = solved.position;
        info.piece.rotation = solved.rotation;
    }
}
