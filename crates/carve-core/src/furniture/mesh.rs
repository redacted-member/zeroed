//! Panel meshes for the preview: each panel's outline after its joints,
//! less the holes cut through it, extruded through its thickness and
//! placed in furniture space.

use serde::Serialize;

use crate::assembly::pose::Pose;
use crate::assembly::triangulate::triangulate;
use crate::joints::geometry::{signed_area2, P2};

/// A triangle mesh: positions and normals as x, y, z triples, three
/// indices per triangle, counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub normals: Vec<f32>,
}

impl Mesh {
    fn vertex(&mut self, p: [f64; 3], n: [f64; 3]) -> u32 {
        let index = (self.positions.len() / 3) as u32;
        self.positions.extend(p.iter().map(|v| *v as f32));
        self.normals.extend(n.iter().map(|v| *v as f32));
        index
    }

    /// Adds `other`'s triangles to this mesh.
    pub fn append(&mut self, other: Mesh) {
        let base = (self.positions.len() / 3) as u32;
        self.positions.extend(other.positions);
        self.normals.extend(other.normals);
        self.indices
            .extend(other.indices.into_iter().map(|i| i + base));
    }

    /// The box round the mesh, furniture millimetres.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in self.positions.chunks(3) {
            for k in 0..3 {
                min[k] = min[k].min(f64::from(p[k]));
                max[k] = max[k].max(f64::from(p[k]));
            }
        }
        Some((min, max))
    }
}

/// `outer` (any winding) less `holes`, from Z 0 to `thickness`, through
/// `pose`.
pub fn prism(outer: &[P2], holes: &[Vec<P2>], thickness: f64, pose: &Pose) -> Mesh {
    let mut mesh = Mesh::default();
    if outer.len() < 3 {
        return mesh;
    }
    let mut outer = outer.to_vec();
    if signed_area2(&outer) < 0.0 {
        outer.reverse();
    }
    let holes: Vec<Vec<P2>> = holes
        .iter()
        .filter(|h| h.len() >= 3)
        .map(|h| {
            let mut h = h.clone();
            if signed_area2(&h) > 0.0 {
                h.reverse();
            }
            h
        })
        .collect();
    let triangles = triangulate(&outer, &holes);
    let all: Vec<P2> = outer
        .iter()
        .copied()
        .chain(holes.iter().flatten().copied())
        .collect();
    let up = pose.apply_vector([0.0, 0.0, 1.0]);
    let down = [-up[0], -up[1], -up[2]];
    for (z, normal, flip) in [(thickness, up, false), (0.0, down, true)] {
        let base = (mesh.positions.len() / 3) as u32;
        for p in &all {
            mesh.vertex(pose.apply([p[0], p[1], z]), normal);
        }
        for t in &triangles {
            let [a, b, c] = t.map(|i| base + i as u32);
            if flip {
                mesh.indices.extend([a, c, b]);
            } else {
                mesh.indices.extend([a, b, c]);
            }
        }
    }
    for ring in std::iter::once(&outer).chain(holes.iter()) {
        for k in 0..ring.len() {
            let (p, q) = (ring[k], ring[(k + 1) % ring.len()]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let len = dx.hypot(dy);
            if len < 1e-9 {
                continue;
            }
            // Material on the left of every ring as wound, so out is right.
            let n = pose.apply_vector([dy / len, -dx / len, 0.0]);
            let v = [
                mesh.vertex(pose.apply([p[0], p[1], 0.0]), n),
                mesh.vertex(pose.apply([q[0], q[1], 0.0]), n),
                mesh.vertex(pose.apply([q[0], q[1], thickness]), n),
                mesh.vertex(pose.apply([p[0], p[1], thickness]), n),
            ];
            mesh.indices.extend([v[0], v[1], v[2], v[0], v[2], v[3]]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_with_a_hole_extrudes_through_its_pose() {
        let outer = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let hole = vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]];
        let pose = Pose::translation([5.0, 6.0, 7.0]);
        let m = prism(&outer, &[hole], 18.0, &pose);
        let (min, max) = m.bounds().expect("bounds");
        assert_eq!(min, [5.0, 6.0, 7.0]);
        assert_eq!(max, [105.0, 56.0, 25.0]);
        // Top and bottom: 8 triangles each (a square less a square);
        // sides: 8 edges, 2 triangles each.
        assert_eq!(m.indices.len() / 3, 8 + 8 + 16);
        assert_eq!(m.positions.len(), m.normals.len());
        // Every side normal points away from the material.
        let first_side = 2 * 8;
        let n = &m.normals[first_side * 3..first_side * 3 + 3];
        // Outer edge (0,0) to (100,0): material above, out is -Y.
        assert_eq!(n, [0.0, -1.0, 0.0]);
    }
}
