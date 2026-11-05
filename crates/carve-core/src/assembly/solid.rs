//! The solid of a flat part: its outline extruded by the thickness, less its
//! holes, with pockets sunk from the top. Coordinates are part local: the
//! outline's lower left corner at the origin, Z from 0 (bottom) to the
//! thickness (top).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use serde::{Deserialize, Serialize};

use super::pose::{cross, normalized, Pose, Vec3};
use super::triangulate::{signed_area2, triangulate, P2};

/// Polygons with holes: each shape is an outer ring then its holes.
pub(super) type Shapes = Vec<Vec<Vec<P2>>>;

/// Boolean of two ring sets (non-zero fill), outer rings counter-clockwise
/// and holes clockwise.
pub(super) fn overlay(subject: &[Vec<P2>], clip: &[Vec<P2>], rule: OverlayRule) -> Shapes {
    if subject.is_empty() {
        return Vec::new();
    }
    let subject: Vec<Vec<P2>> = subject.to_vec();
    let clip: Vec<Vec<P2>> = clip.to_vec();
    let mut shapes = subject.overlay(&clip, rule, FillRule::NonZero);
    // Outer rings counter-clockwise, holes clockwise.
    for shape in &mut shapes {
        for (i, ring) in shape.iter_mut().enumerate() {
            let ccw = signed_area2(ring) > 0.0;
            if ccw != (i == 0) {
                ring.reverse();
            }
        }
    }
    shapes
}

/// Every ring of `shapes`, outer rings and holes alike.
pub(super) fn rings(shapes: &Shapes) -> Vec<Vec<P2>> {
    shapes.iter().flatten().cloned().collect()
}

/// A face of a part that mates can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "index")]
pub enum Face {
    /// The top face (the sheet's top surface).
    Top,
    /// The bottom face.
    Bottom,
    /// The side face along span `i` of the outline (counter-clockwise from
    /// the outline's first point).
    Edge(u32),
    /// The floor of pocket `i`.
    PocketFloor(u32),
}

/// Face code of a triangle that no mate can use (hole and pocket walls).
pub const NO_FACE: u32 = u32::MAX;

impl Face {
    /// The code the mesh gives each triangle: 0 top, 1 bottom, 2 + i for
    /// edge i, 0x8000_0000 + i for pocket floor i.
    pub fn code(self) -> u32 {
        match self {
            Face::Top => 0,
            Face::Bottom => 1,
            Face::Edge(i) => 2 + i,
            Face::PocketFloor(i) => 0x8000_0000 + i,
        }
    }

    /// The face a mesh code stands for.
    pub fn from_code(code: u32) -> Option<Face> {
        match code {
            NO_FACE => None,
            0 => Some(Face::Top),
            1 => Some(Face::Bottom),
            c if c >= 0x8000_0000 => Some(Face::PocketFloor(c - 0x8000_0000)),
            c => Some(Face::Edge(c - 2)),
        }
    }
}

/// A pocket sunk into the top face.
#[derive(Debug, Clone, PartialEq)]
pub struct PocketRing {
    /// The pocket boundary, counter-clockwise, part local.
    pub ring: Vec<P2>,
    /// Depth below the top in millimetres, less than the thickness.
    pub depth: f64,
}

/// A part's shape in part local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// The outline as cut, counter-clockwise, without a repeated last point.
    pub outer: Vec<P2>,
    /// The outline the face frames come from, counter-clockwise: the outline
    /// before any joint rewrote it, so mates stay put when tabs or fingers
    /// are added. The same as `outer` for a part without such joints.
    pub frame: Vec<P2>,
    /// Through holes, counter-clockwise.
    pub holes: Vec<Vec<P2>>,
    /// Pockets from the top.
    pub pockets: Vec<PocketRing>,
    /// Thickness in millimetres.
    pub thickness: f64,
    /// Where the local origin sits in sheet coordinates.
    pub origin: P2,
}

/// A triangle mesh with a face code per triangle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Vertex positions, xyz.
    pub positions: Vec<f32>,
    /// Vertex normals, xyz.
    pub normals: Vec<f32>,
    /// Triangle vertex indices.
    pub indices: Vec<u32>,
    /// One [`Face::code`] (or [`NO_FACE`]) per triangle.
    pub faces: Vec<u32>,
}

impl Mesh {
    /// The enclosed volume in cubic millimetres (divergence theorem), exact
    /// for a closed, outward facing mesh.
    pub fn volume(&self) -> f64 {
        let v = |i: u32| {
            let k = i as usize * 3;
            [
                f64::from(self.positions[k]),
                f64::from(self.positions[k + 1]),
                f64::from(self.positions[k + 2]),
            ]
        };
        self.indices
            .chunks_exact(3)
            .map(|t| {
                let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    fn vertex(&mut self, p: Vec3, n: Vec3) -> u32 {
        let index = (self.positions.len() / 3) as u32;
        self.positions.extend(p.iter().map(|v| *v as f32));
        self.normals.extend(n.iter().map(|v| *v as f32));
        index
    }

    /// Flat polygons with holes at height `z`, facing up or down, each
    /// triangle tagged by `face_of` with its shape.
    fn caps(&mut self, shapes: &Shapes, z: f64, up: bool, face: u32) {
        let normal = [0.0, 0.0, if up { 1.0 } else { -1.0 }];
        for shape in shapes {
            let Some((outer, holes)) = shape.split_first() else {
                continue;
            };
            let base = (self.positions.len() / 3) as u32;
            for p in outer.iter().chain(holes.iter().flatten()) {
                self.vertex([p[0], p[1], z], normal);
            }
            for t in triangulate(outer, holes) {
                let (a, b, c) = (t[0] as u32, t[1] as u32, t[2] as u32);
                if up {
                    self.indices.extend([base + a, base + b, base + c]);
                } else {
                    self.indices.extend([base + a, base + c, base + b]);
                }
                self.faces.push(face);
            }
        }
    }

    /// Walls between two heights along every ring of `shapes`, facing out of
    /// the material (outer rings are counter-clockwise, holes clockwise).
    fn walls(&mut self, shapes: &Shapes, z0: f64, z1: f64, face_of: &impl Fn(P2, P2) -> u32) {
        for ring in shapes.iter().flatten() {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                let normal = normalized([b[1] - a[1], -(b[0] - a[0]), 0.0]);
                let v0 = self.vertex([a[0], a[1], z0], normal);
                let v1 = self.vertex([b[0], b[1], z0], normal);
                let v2 = self.vertex([b[0], b[1], z1], normal);
                let v3 = self.vertex([a[0], a[1], z1], normal);
                self.indices.extend([v0, v1, v2, v0, v2, v3]);
                let face = face_of(a, b);
                self.faces.extend([face, face]);
            }
        }
    }
}

impl Profile {
    /// Builds a profile from sheet coordinate rings: `outer`, `holes` and
    /// `pockets`. Rings are reoriented counter-clockwise and moved so the
    /// outline's lower left corner is the origin.
    pub fn new(
        outer: Vec<P2>,
        holes: Vec<Vec<P2>>,
        pockets: Vec<(Vec<P2>, f64)>,
        thickness: f64,
    ) -> Profile {
        Profile::with_frame(None, outer, holes, pockets, thickness)
    }

    /// Builds a profile whose face frames come from `frame` (sheet
    /// coordinates) rather than the cut outline; the local origin is then the
    /// frame's lower left corner.
    pub fn with_frame(
        frame: Option<Vec<P2>>,
        outer: Vec<P2>,
        holes: Vec<Vec<P2>>,
        pockets: Vec<(Vec<P2>, f64)>,
        thickness: f64,
    ) -> Profile {
        let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
        for p in frame.as_ref().unwrap_or(&outer) {
            min_x = min_x.min(p[0]);
            min_y = min_y.min(p[1]);
        }
        let shift = |ring: Vec<P2>| -> Vec<P2> {
            let mut ring: Vec<P2> = ring
                .into_iter()
                .map(|p| [p[0] - min_x, p[1] - min_y])
                .collect();
            if signed_area2(&ring) < 0.0 {
                ring.reverse();
            }
            ring
        };
        let outer = shift(outer);
        Profile {
            frame: frame.map_or_else(|| outer.clone(), shift),
            outer,
            holes: holes.into_iter().map(shift).collect(),
            pockets: pockets
                .into_iter()
                .map(|(ring, depth)| PocketRing {
                    ring: shift(ring),
                    depth,
                })
                .collect(),
            thickness,
            origin: [min_x, min_y],
        }
    }

    /// Width and height of the frame outline's bounding box.
    pub fn size(&self) -> P2 {
        let mut max = [0.0f64, 0.0f64];
        for p in &self.frame {
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
        max
    }

    /// Lower left and upper right corners of the cut outline, which reaches
    /// past the frame where tabs or fingers were added.
    pub fn bounds(&self) -> [P2; 2] {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in &self.outer {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        [lo, hi]
    }

    /// Number of frame outline spans, which is the number of edge faces.
    pub fn edge_count(&self) -> usize {
        self.frame.len()
    }

    /// The closed triangle mesh of the part, built slab by slab between
    /// the Z levels of the bottom, the pocket floors and the top, so pockets
    /// may run out through the outline (a dado across a panel).
    pub fn mesh(&self) -> Mesh {
        let t = self.thickness;
        let base = overlay(
            std::slice::from_ref(&self.outer),
            &self.holes,
            OverlayRule::Difference,
        );
        let mut levels: Vec<f64> = vec![0.0, t];
        for pocket in &self.pockets {
            let floor = t - pocket.depth;
            if floor > 1e-9 && floor < t - 1e-9 {
                levels.push(floor);
            }
        }
        levels.sort_by(f64::total_cmp);
        levels.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let region = |z: f64| -> Shapes {
            let cut: Vec<Vec<P2>> = self
                .pockets
                .iter()
                .filter(|p| t - p.depth < z)
                .map(|p| p.ring.clone())
                .collect();
            if cut.is_empty() {
                base.clone()
            } else {
                overlay(&rings(&base), &cut, OverlayRule::Difference)
            }
        };
        let slabs: Vec<Shapes> = levels
            .windows(2)
            .map(|w| region((w[0] + w[1]) / 2.0))
            .collect();
        let edge_of = |a: P2, b: P2| self.edge_of(a, b);
        let mut mesh = Mesh::default();
        for (i, shapes) in slabs.iter().enumerate() {
            mesh.walls(shapes, levels[i], levels[i + 1], &edge_of);
        }
        let empty: Shapes = Vec::new();
        for (j, &z) in levels.iter().enumerate() {
            let below = if j == 0 { &empty } else { &slabs[j - 1] };
            let above = slabs.get(j).unwrap_or(&empty);
            let up = overlay(&rings(below), &rings(above), OverlayRule::Difference);
            let down = overlay(&rings(above), &rings(below), OverlayRule::Difference);
            if (z - t).abs() < 1e-9 {
                mesh.caps(&up, z, true, Face::Top.code());
            } else {
                let mut rest = up;
                for (k, pocket) in self.pockets.iter().enumerate() {
                    if (t - pocket.depth - z).abs() < 1e-9 && !rest.is_empty() {
                        let floor = overlay(
                            &rings(&rest),
                            std::slice::from_ref(&pocket.ring),
                            OverlayRule::Intersect,
                        );
                        mesh.caps(&floor, z, true, Face::PocketFloor(k as u32).code());
                        rest = overlay(
                            &rings(&rest),
                            std::slice::from_ref(&pocket.ring),
                            OverlayRule::Difference,
                        );
                    }
                }
                mesh.caps(&rest, z, true, NO_FACE);
            }
            let bottom = if j == 0 { Face::Bottom.code() } else { NO_FACE };
            mesh.caps(&down, z, false, bottom);
        }
        mesh
    }

    /// The edge face a wall segment from `a` to `b` lies on, if any.
    fn edge_of(&self, a: P2, b: P2) -> u32 {
        let n = self.frame.len();
        let near = |p: P2, s0: P2, s1: P2| {
            let d = [s1[0] - s0[0], s1[1] - s0[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            if len2 <= 0.0 {
                return false;
            }
            let t = ((p[0] - s0[0]) * d[0] + (p[1] - s0[1]) * d[1]) / len2;
            let q = [s0[0] + d[0] * t, s0[1] + d[1] * t];
            (-1e-6..=1.0 + 1e-6).contains(&t)
                && (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) < 1e-8
        };
        (0..n)
            .find(|&i| {
                let s0 = self.frame[i];
                let s1 = self.frame[(i + 1) % n];
                near(a, s0, s1) && near(b, s0, s1)
            })
            .map_or(NO_FACE, |i| Face::Edge(i as u32).code())
    }

    /// Whether a part local point lies inside the solid, at least `inset`
    /// millimetres from every surface.
    pub fn contains(&self, p: Vec3, inset: f64) -> bool {
        let q = [p[0], p[1]];
        if p[2] < inset || p[2] > self.thickness - inset {
            return false;
        }
        if !inside_ring(&self.outer, q) || ring_distance(&self.outer, q) < inset {
            return false;
        }
        for hole in &self.holes {
            if inside_ring(hole, q) || ring_distance(hole, q) < inset {
                return false;
            }
        }
        for pocket in &self.pockets {
            if p[2] > self.thickness - pocket.depth - inset
                && (inside_ring(&pocket.ring, q) || ring_distance(&pocket.ring, q) < inset)
            {
                return false;
            }
        }
        true
    }

    /// The frame of `face`: origin, then the in-plane axes u and v and the
    /// outward normal, as a pose mapping face coordinates (u, v, n) to part
    /// local coordinates. `None` for a face the part does not have.
    pub fn face_frame(&self, face: Face) -> Option<Pose> {
        let t = self.thickness;
        match face {
            Face::Top => Some(Pose::frame(
                [0.0, 0.0, t],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            )),
            Face::Bottom => {
                let h = self.size()[1];
                Some(Pose::frame(
                    [0.0, h, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, -1.0, 0.0],
                    [0.0, 0.0, -1.0],
                ))
            }
            Face::Edge(i) => {
                let n = self.frame.len();
                let i = i as usize;
                if i >= n {
                    return None;
                }
                let a = self.frame[i];
                let b = self.frame[(i + 1) % n];
                let u = normalized([b[0] - a[0], b[1] - a[1], 0.0]);
                let normal = [u[1], -u[0], 0.0];
                let v = cross(normal, u);
                Some(Pose::frame([a[0], a[1], 0.0], u, v, normal))
            }
            Face::PocketFloor(k) => {
                let pocket = self.pockets.get(k as usize)?;
                let (mut x, mut y) = (f64::MAX, f64::MAX);
                for p in &pocket.ring {
                    x = x.min(p[0]);
                    y = y.min(p[1]);
                }
                Some(Pose::frame(
                    [x, y, t - pocket.depth],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                ))
            }
        }
    }

    /// Length along u and v of a face, in millimetres.
    pub fn face_size(&self, face: Face) -> Option<P2> {
        match face {
            Face::Top | Face::Bottom => Some(self.size()),
            Face::Edge(i) => {
                let n = self.frame.len();
                let i = i as usize;
                (i < n).then(|| {
                    let a = self.frame[i];
                    let b = self.frame[(i + 1) % n];
                    [
                        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt(),
                        self.thickness,
                    ]
                })
            }
            Face::PocketFloor(k) => {
                let pocket = self.pockets.get(k as usize)?;
                let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                for p in &pocket.ring {
                    lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                    hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                }
                Some([hi[0] - lo[0], hi[1] - lo[1]])
            }
        }
    }
}

/// Even-odd point in ring test.
pub fn inside_ring(ring: &[P2], p: P2) -> bool {
    let n = ring.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance from `p` to the nearest point of the ring's edges.
pub fn ring_distance(ring: &[P2], p: P2) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let d = [b[0] - a[0], b[1] - a[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            let t = if len2 > 0.0 {
                (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = [a[0] + d[0] * t, a[1] + d[1] * t];
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
        })
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Profile {
        let outer = vec![[100.0, 50.0], [400.0, 50.0], [400.0, 250.0], [100.0, 250.0]];
        let hole = vec![
            [150.0, 100.0],
            [170.0, 100.0],
            [170.0, 120.0],
            [150.0, 120.0],
        ];
        let dado = vec![[90.0, 140.0], [410.0, 140.0], [410.0, 158.0], [90.0, 158.0]];
        Profile::new(outer, vec![hole], vec![(dado, 6.0)], 18.0)
    }

    #[test]
    fn the_profile_moves_to_the_origin() {
        let p = panel();
        assert_eq!(p.origin, [100.0, 50.0]);
        assert_eq!(p.size(), [300.0, 200.0]);
        assert_eq!(p.edge_count(), 4);
    }

    #[test]
    fn the_mesh_is_closed_and_tags_its_faces() {
        let mesh = panel().mesh();
        assert_eq!(mesh.indices.len() / 3, mesh.faces.len());
        assert!(mesh.faces.contains(&Face::Top.code()));
        assert!(mesh.faces.contains(&Face::Edge(3).code()));
        assert!(mesh.faces.contains(&Face::PocketFloor(0).code()));
        assert!(mesh.faces.contains(&NO_FACE));
    }

    #[test]
    fn the_mesh_encloses_the_right_volume() {
        // 300 x 200 x 18, less a 20 x 20 hole and a dado 18 wide and 6 deep
        // right across the part. A closed, outward facing mesh gives the
        // exact volume by the divergence theorem.
        let expected = 300.0 * 200.0 * 18.0 - 20.0 * 20.0 * 18.0 - 300.0 * 18.0 * 6.0;
        let got = panel().mesh().volume();
        assert!((got - expected).abs() < 1.0, "{got} vs {expected}");
    }

    #[test]
    fn containment_honours_holes_and_pockets() {
        let p = panel();
        assert!(p.contains([10.0, 10.0, 9.0], 0.01));
        assert!(!p.contains([60.0, 60.0, 9.0], 0.01), "inside the hole");
        assert!(!p.contains([10.0, 100.0, 15.0], 0.01), "inside the dado");
        assert!(p.contains([10.0, 100.0, 9.0], 0.01), "under the dado floor");
        assert!(!p.contains([10.0, 10.0, 19.0], 0.01), "above the top");
    }

    #[test]
    fn edge_frames_face_outwards() {
        let p = panel();
        let frame = p.face_frame(Face::Edge(0)).expect("edge 0");
        // Edge 0 runs along the bottom of the outline, so it faces -Y.
        let n = frame.apply_vector([0.0, 0.0, 1.0]);
        assert!((n[1] + 1.0).abs() < 1e-9);
        // v points up the part's thickness.
        let v = frame.apply_vector([0.0, 1.0, 0.0]);
        assert!((v[2] - 1.0).abs() < 1e-9);
    }
}
