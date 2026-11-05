//! Where part B sits against part A once the joint is together. Each kind
//! gives a map from B's edge frame (along the edge, out of the part, up
//! through the thickness) to A's; with the two edge frames that makes the
//! rigid transform from B's part space (job X and Y, height above its
//! underside) to A's. The 3D preview and the Assembly View placement use it.

use crate::assembly::pose::{Pose, Vec3};

use super::geometry::Frame;

/// B's edge frame coordinates to A's: `a = m * b + o`, each a triple of
/// (along the edge, out of the part, up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Map {
    pub m: [[f64; 3]; 3],
    pub o: Vec3,
}

impl Map {
    /// A map whose B axes land on the A axes `to` (each an axis index and a
    /// sign), then shifted by `o`.
    pub fn axes(to: [(usize, f64); 3], o: Vec3) -> Map {
        let mut m = [[0.0; 3]; 3];
        for (b_axis, (a_axis, sign)) in to.iter().enumerate() {
            m[*a_axis][b_axis] = *sign;
        }
        Map { m, o }
    }

    fn pose(&self) -> Pose {
        Pose {
            r: self.m,
            t: self.o,
        }
    }
}

/// The edge frame's coordinates to part space.
fn edge_pose(f: &Frame) -> Pose {
    Pose::frame(
        [f.a[0], f.a[1], 0.0],
        [f.u[0], f.u[1], 0.0],
        [f.n[0], f.n[1], 0.0],
        [0.0, 0.0, 1.0],
    )
}

/// B's part space to A's part space.
pub fn b_to_a(fa: &Frame, fb: &Frame, map: &Map) -> Pose {
    edge_pose(fa)
        .then_local(&map.pose())
        .then_local(&edge_pose(fb).inverse())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_map_stands_b_up_on_a_s_edge() {
        let a_ring = vec![vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]]];
        let b_ring = vec![vec![
            [0.0, 100.0],
            [100.0, 100.0],
            [100.0, 140.0],
            [0.0, 140.0],
        ]];
        let fa = Frame::of_edge([0.0, 0.0], [100.0, 0.0], &a_ring);
        let fb = Frame::of_edge([0.0, 100.0], [100.0, 100.0], &b_ring);
        // along -> -along (+100), out -> -up, up -> -out.
        let map = Map::axes([(0, -1.0), (2, -1.0), (1, -1.0)], [100.0, 0.0, 0.0]);
        let pose = b_to_a(&fa, &fb, &map);
        let det = pose.r[0][0] * (pose.r[1][1] * pose.r[2][2] - pose.r[1][2] * pose.r[2][1])
            - pose.r[0][1] * (pose.r[1][0] * pose.r[2][2] - pose.r[1][2] * pose.r[2][0])
            + pose.r[0][2] * (pose.r[1][0] * pose.r[2][1] - pose.r[1][1] * pose.r[2][0]);
        assert!((det - 1.0).abs() < 1e-9, "{det}");
        // B's edge start lands on the end of A's edge.
        let p = pose.apply([0.0, 100.0, 0.0]);
        assert!(
            (p[0] - 100.0).abs() < 1e-9 && p[1].abs() < 1e-9 && p[2].abs() < 1e-9,
            "{p:?}"
        );
        // A point of B 10 mm in from its edge stands 10 mm up.
        let q = pose.apply([50.0, 110.0, 0.0]);
        assert!((q[2] - 10.0).abs() < 1e-9, "{q:?}");
    }
}
