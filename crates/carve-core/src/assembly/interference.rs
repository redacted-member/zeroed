//! Where placed parts run into each other. Each pair whose boxes overlap is
//! sampled on a grid inside one part, over the overlap of the boxes only,
//! and every sample is tested against the other solid (holes and pockets
//! included). Faces that merely touch do not count: samples keep an inset
//! from every surface.

use serde::Serialize;

use super::pose::{Pose, Vec3};
use super::solid::Profile;

/// Surfaces closer than this do not count as running into each other.
pub const INSET: f64 = 0.05;

/// Most samples tried for one pair; the grid coarsens beyond it.
const MAX_SAMPLES: f64 = 400_000.0;

/// A pair of instances that overlap.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clash {
    /// First instance.
    pub a: u64,
    /// Second instance.
    pub b: u64,
    /// Approximate overlap volume in cubic millimetres.
    pub volume: f64,
    /// Box of the overlap in assembly space, min then max corner.
    pub bounds: [Vec3; 2],
}

/// A placed solid.
pub struct Placed<'a> {
    /// Instance id.
    pub id: u64,
    /// Its shape.
    pub profile: &'a Profile,
    /// Its pose in assembly space.
    pub pose: Pose,
}

fn local_box(profile: &Profile) -> [Vec3; 2] {
    let [lo, hi] = profile.bounds();
    [[lo[0], lo[1], 0.0], [hi[0], hi[1], profile.thickness]]
}

fn corners(b: &[Vec3; 2]) -> [Vec3; 8] {
    let mut out = [[0.0; 3]; 8];
    for (k, c) in out.iter_mut().enumerate() {
        *c = [
            if k & 1 == 0 { b[0][0] } else { b[1][0] },
            if k & 2 == 0 { b[0][1] } else { b[1][1] },
            if k & 4 == 0 { b[0][2] } else { b[1][2] },
        ];
    }
    out
}

/// The box around `b` after `pose`.
fn moved_box(b: &[Vec3; 2], pose: &Pose) -> [Vec3; 2] {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for c in corners(b) {
        let p = pose.apply(c);
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    [lo, hi]
}

fn intersect(a: &[Vec3; 2], b: &[Vec3; 2]) -> Option<[Vec3; 2]> {
    let lo = [
        a[0][0].max(b[0][0]),
        a[0][1].max(b[0][1]),
        a[0][2].max(b[0][2]),
    ];
    let hi = [
        a[1][0].min(b[1][0]),
        a[1][1].min(b[1][1]),
        a[1][2].min(b[1][2]),
    ];
    (0..3)
        .all(|i| hi[i] - lo[i] > 2.0 * INSET)
        .then_some([lo, hi])
}

/// The overlap of two placed solids, or `None` when they do not overlap.
/// `step` is the sample spacing in millimetres.
pub fn clash(a: &Placed, b: &Placed, step: f64) -> Option<Clash> {
    // Work in b's frame: sample b over the part of its box a's box covers.
    let a_in_b = b.pose.inverse().then_local(&a.pose);
    let region = intersect(
        &local_box(b.profile),
        &moved_box(&local_box(a.profile), &a_in_b),
    )?;
    let b_to_a = a_in_b.inverse();
    let span = [
        region[1][0] - region[0][0],
        region[1][1] - region[0][1],
        region[1][2] - region[0][2],
    ];
    let mut h = step.max(0.05);
    while (span[0] / h).ceil() * (span[1] / h).ceil() * (span[2] / h).ceil() > MAX_SAMPLES {
        h *= 1.5;
    }
    let counts = [
        ((span[0] / h).ceil() as usize).max(1),
        ((span[1] / h).ceil() as usize).max(1),
        ((span[2] / h).ceil() as usize).max(1),
    ];
    let cell = [
        span[0] / counts[0] as f64,
        span[1] / counts[1] as f64,
        span[2] / counts[2] as f64,
    ];
    let mut hits = 0usize;
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for i in 0..counts[0] {
        for j in 0..counts[1] {
            for k in 0..counts[2] {
                let p = [
                    region[0][0] + (i as f64 + 0.5) * cell[0],
                    region[0][1] + (j as f64 + 0.5) * cell[1],
                    region[0][2] + (k as f64 + 0.5) * cell[2],
                ];
                if !b.profile.contains(p, INSET) || !a.profile.contains(b_to_a.apply(p), INSET) {
                    continue;
                }
                hits += 1;
                let w = b.pose.apply(p);
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(w[axis]);
                    hi[axis] = hi[axis].max(w[axis]);
                }
            }
        }
    }
    (hits > 0).then(|| Clash {
        a: a.id,
        b: b.id,
        volume: hits as f64 * cell[0] * cell[1] * cell[2],
        bounds: [lo, hi],
    })
}

/// Every overlapping pair among `solids`, in id order.
pub fn clashes(solids: &[Placed], step: f64) -> Vec<Clash> {
    let mut out = Vec::new();
    for (i, a) in solids.iter().enumerate() {
        for b in &solids[i + 1..] {
            if let Some(c) = clash(a, b, step) {
                out.push(c);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slab(w: f64, h: f64, t: f64) -> Profile {
        Profile::new(
            vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            vec![],
            vec![],
            t,
        )
    }

    #[test]
    fn touching_parts_do_not_clash() {
        let p = slab(100.0, 100.0, 18.0);
        let a = Placed {
            id: 1,
            profile: &p,
            pose: Pose::IDENTITY,
        };
        let b = Placed {
            id: 2,
            profile: &p,
            pose: Pose::translation([0.0, 0.0, 18.0]),
        };
        assert!(clash(&a, &b, 2.0).is_none());
    }

    #[test]
    fn overlapping_parts_report_their_volume() {
        let p = slab(100.0, 100.0, 18.0);
        let a = Placed {
            id: 1,
            profile: &p,
            pose: Pose::IDENTITY,
        };
        let b = Placed {
            id: 2,
            profile: &p,
            pose: Pose::translation([50.0, 0.0, 0.0]),
        };
        let c = clash(&a, &b, 2.0).expect("clash");
        let expected = 50.0 * 100.0 * 18.0;
        assert!(
            (c.volume - expected).abs() / expected < 0.05,
            "{}",
            c.volume
        );
    }

    #[test]
    fn a_shelf_in_a_dado_fits() {
        // An 18 mm panel with a 6 mm deep dado 12 mm wide across it, and a
        // 12 mm shelf standing in the dado.
        let dado = vec![[-5.0, 40.0], [105.0, 40.0], [105.0, 52.0], [-5.0, 52.0]];
        let side = Profile::new(
            vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
            vec![],
            vec![(dado, 6.0)],
            18.0,
        );
        // The shelf as a block 100 long, 12 thick (Y) and 60 tall (Z),
        // standing on the dado floor at z = 12.
        let shelf = slab(100.0, 12.0, 60.0);
        let stand = Pose::translation([0.0, 40.0, 12.0]);
        let a = Placed {
            id: 1,
            profile: &side,
            pose: Pose::IDENTITY,
        };
        let b = Placed {
            id: 2,
            profile: &shelf,
            pose: stand,
        };
        let fits = clash(&a, &b, 1.0);
        let sunk = Placed {
            id: 2,
            profile: &shelf,
            pose: Pose::translation([0.0, 0.0, -3.0]).then_local(&stand),
        };
        let deep = clash(&a, &sunk, 1.0);
        assert!(fits.is_none(), "{fits:?}");
        assert!(deep.is_some());
    }
}
