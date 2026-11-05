//! Measure between pieces: two points picked on pieces, each snapped to
//! the nearest face of its piece's exact solid (or the simulation grid's
//! face where there is none), and the distance between them: point to
//! point, and face to face when the faces are parallel.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cam::sim::Quality;
use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

use super::exact;
use super::fit::{self, HeldPiece};
use super::pieces::{self, PartsCache, PieceRef};
use super::pose::{self, Vec3};

/// Faces this close to parallel measure face to face, degrees.
const PARALLEL_DEGREES: f64 = 1.0;

/// A point picked on a piece, in its own frame.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickArgs {
    #[serde(flatten)]
    piece: PieceRef,
    point: Vec3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeasureArgs {
    a: PickArgs,
    b: PickArgs,
    #[serde(default)]
    quality: Quality,
}

/// A picked point snapped to its face, assembly space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapped {
    /// The point on the face.
    pub point: Vec3,
    /// The face's outward normal.
    pub normal: Vec3,
    /// Whether it came from the exact solid, else the grid.
    pub exact: bool,
}

/// The face of `held` nearest a local point, and the point on it, in
/// assembly space; `None` when the piece has no face near it.
pub fn snap(held: &HeldPiece, local: Vec3) -> Option<Snapped> {
    let (point, normal, exact) = match &held.solid {
        Some(solid) => {
            let (on, n) = nearest_face(solid, local)?;
            (on, n, true)
        }
        None => {
            let face = pieces::face_at(&held.stock, local)?;
            (face.point, face.normal, false)
        }
    };
    Some(Snapped {
        point: held.pose.apply(point),
        normal: pose::normalized(held.pose.apply_vector(normal)),
        exact,
    })
}

/// The face of an exact solid nearest a point in the solid's own frame:
/// the point on its plane and its outward normal.
pub fn nearest_face(solid: &exact::ExactSolid, local: Vec3) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f64, Vec3, Vec3)> = None;
    for face in fit::fit_faces(solid) {
        let d = pose::dot(pose::sub(local, face.o), face.n);
        let on = pose::sub(local, pose::scale(face.n, d));
        let w = pose::sub(on, face.o);
        let flat = [pose::dot(w, face.u), pose::dot(w, face.v)];
        let off = if exact::contains(&face.shapes, flat) {
            0.0
        } else {
            exact::edge_distance(&face.shapes, flat)
        };
        let cost = d.abs() + off;
        if best.is_none_or(|(c, ..)| cost < c) {
            best = Some((cost, on, face.n));
        }
    }
    best.map(|(_, on, n)| (on, n))
}

/// `assembly.measure`: the distance between two points picked on pieces,
/// and between their faces when those are parallel. Reads only.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] for a piece that does not exist or a point
/// with no face near it.
pub fn measure(
    document: &Document,
    cache: &mut PartsCache,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let args: MeasureArgs = serde_json::from_str(args)
        .map_err(|error| CoreError::invalid_args("assembly.measure", error))?;
    let held = fit::solids(document, cache, args.quality)?;
    let pick = |p: &PickArgs| -> Result<Snapped, CoreError> {
        let piece = held
            .iter()
            .find(|h| h.key == p.piece)
            .ok_or_else(|| CoreError::validation("that piece is gone"))?;
        snap(piece, p.point)
            .ok_or_else(|| CoreError::validation("there is no face of the piece there"))
    };
    let (a, b) = (pick(&args.a)?, pick(&args.b)?);
    let delta = pose::sub(b.point, a.point);
    let distance = pose::dot(delta, delta).sqrt();
    let cos = pose::dot(a.normal, b.normal).clamp(-1.0, 1.0);
    let parallel = cos.abs() >= PARALLEL_DEGREES.to_radians().cos();
    let face_distance = parallel.then(|| pose::dot(delta, a.normal).abs());
    KernelReply::ok(&json!({
        "a": a,
        "b": b,
        "distance": distance,
        "delta": delta,
        "parallel": parallel,
        "faceDistance": face_distance,
        "angle": cos.acos().to_degrees(),
    }))
}
