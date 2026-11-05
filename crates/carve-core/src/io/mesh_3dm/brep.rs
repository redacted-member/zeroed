//! Polysurfaces (`ON_Brep`), extrusions (`ON_Extrusion`) and lone surfaces
//! turned into triangles.
//!
//! A brep holds arrays of 2D trim curves, 3D edge curves and surfaces,
//! then vertices, edges, trims, loops and faces referring to them by
//! index, a bounding box and, when Rhino saved them, a render mesh per
//! face. Each face is its surface cut by its loops: the loops are chains
//! of trims, each a stretch of a 2D curve in the surface's parameter
//! plane. A face is meshed on a grid over its loops' extent in that plane,
//! fine enough for a tolerance taken from the object's size; grid cells
//! wholly inside the loops become two triangles, cells a loop crosses are
//! clipped to the loops and the pieces triangulated. A face whose surface
//! or trims are of a kind not read falls back to Rhino's render mesh.

use std::ops::Range;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use super::geometry::{self, add, cross, scale, sub, unit, Curve, Surface, P3};
use super::{bad, Archive, Reader, ON_MESH};
use crate::assembly::triangulate::triangulate;
use crate::error::CoreError;
use crate::io::mesh::Triangle;

/// `ON_Brep`, {60B5DBC5-E660-11D3-BFE4-0010830122F0}.
pub(super) const ON_BREP: [u8; 16] = geometry::wire([
    0x60, 0xb5, 0xdb, 0xc5, 0xe6, 0x60, 0x11, 0xd3, 0xbf, 0xe4, 0x00, 0x10, 0x83, 0x01, 0x22, 0xf0,
]);
/// `ON_Extrusion`, {36F53175-72B8-4D47-BF1F-B4E6FC24F4B9}.
pub(super) const ON_EXTRUSION: [u8; 16] = geometry::wire([
    0x36, 0xf5, 0x31, 0x75, 0x72, 0xb8, 0x4d, 0x47, 0xbf, 0x1f, 0xb4, 0xe6, 0xfc, 0x24, 0xf4, 0xb9,
]);

/// Chord tolerance as a share of an object's diagonal.
const RELATIVE_TOLERANCE: f64 = 2e-4;
/// Pieces per NURBS span of a trim curve, which lives in the parameter
/// plane where lengths mean nothing.
const TRIM_TOLERANCE_SHARE: f64 = 1e-3;
/// Most loop points of one face.
const MAX_LOOP_POINTS: usize = 200_000;
/// Loop kinds that bound a face: unknown, outer and inner.
const BOUNDING_LOOPS: [i32; 3] = [0, 1, 2];

type P2 = [f64; 2];

/// An object of an array: its class and where its data lies.
type Item = Option<([u8; 16], Range<usize>)>;

/// What one trim uses: its 2D curve and the stretch of it.
struct Trim {
    curve: i32,
    domain: [f64; 2],
}

struct Loop {
    trims: Vec<i32>,
    kind: i32,
}

struct Face {
    loops: Vec<i32>,
    surface: i32,
    reversed: bool,
}

/// A chord tolerance for an object whose corners are `min` and `max`.
fn tolerance(min: P3, max: P3) -> f64 {
    let diagonal = geometry::length(sub(max, min));
    if diagonal.is_finite() && diagonal > 0.0 {
        diagonal * RELATIVE_TOLERANCE
    } else {
        0.01
    }
}

impl Archive {
    /// The anonymous chunk at `r`, moving past it.
    fn next_chunk(&self, bytes: &[u8], r: &mut Reader<'_>) -> Result<Range<usize>, CoreError> {
        let chunk = self.chunk(bytes, r.at, r.end)?;
        r.at = chunk.next;
        Ok(chunk.body)
    }

    /// An array of objects (`ON_CurveArray`, `ON_SurfaceArray`): a version,
    /// a count, then per item a presence flag and the object.
    fn objects(&self, bytes: &[u8], r: &mut Reader<'_>) -> Result<Vec<Item>, CoreError> {
        let body = self.next_chunk(bytes, r)?;
        let mut a = Reader::new(bytes, body.start, body.end);
        a.u8()?;
        let n = a.count(super::MAX_FACES)?;
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            if a.i32()? == 0 {
                out.push(None);
                continue;
            }
            let (uuid, data, next) = self.class_at(bytes, a.at, a.end)?;
            a.at = next;
            out.push(Some((uuid, data)));
        }
        Ok(out)
    }

    /// The triangles of a serialised `ON_Brep`; `None` when it is written
    /// in a form this reader does not know.
    pub(super) fn brep(
        &self,
        bytes: &[u8],
        data: Range<usize>,
    ) -> Result<Option<Vec<Triangle>>, CoreError> {
        let mut r = Reader::new(bytes, data.start, data.end);
        let version = r.u8()?;
        if version >> 4 != 3 {
            return Ok(None);
        }
        let curves: Vec<Option<Curve>> = self
            .objects(bytes, &mut r)?
            .into_iter()
            .map(|item| {
                item.and_then(|(uuid, data)| self.curve(bytes, &uuid, data, 0).ok().flatten())
            })
            .collect();
        // 3D edge curves.
        self.next_chunk(bytes, &mut r)?;
        let surfaces: Vec<Option<Surface>> = self
            .objects(bytes, &mut r)?
            .into_iter()
            .map(|item| {
                item.and_then(|(uuid, data)| self.surface(bytes, &uuid, data).ok().flatten())
            })
            .collect();
        // Vertices and edges.
        self.next_chunk(bytes, &mut r)?;
        self.next_chunk(bytes, &mut r)?;
        let trims = self.trims(bytes, &mut r)?;
        let loops = self.loops(bytes, &mut r)?;
        let faces = self.faces(bytes, &mut r)?;
        let min = r.point()?;
        let max = r.point()?;
        let tol = tolerance(min, max);
        let render = if version & 0x0f >= 1 {
            self.render_meshes(bytes, &mut r, faces.len())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut out = Vec::new();
        for (index, face) in faces.iter().enumerate() {
            let own = face_loops(face, &loops, &trims, &curves).and_then(|rings| {
                let surface = usize::try_from(face.surface)
                    .ok()
                    .and_then(|s| surfaces.get(s))
                    .and_then(Option::as_ref)?;
                Some(face_mesh(surface, &rings, tol, face.reversed))
            });
            match own.or_else(|| render.get(index).cloned().flatten()) {
                Some(triangles) => out.extend(triangles),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    fn trims(&self, bytes: &[u8], r: &mut Reader<'_>) -> Result<Vec<Trim>, CoreError> {
        let body = self.next_chunk(bytes, r)?;
        let mut a = Reader::new(bytes, body.start, body.end);
        a.u8()?;
        let n = a.count(super::MAX_FACES)?;
        // Trim records have one size; newer files may append more after
        // the fields read here.
        let size = if n > 0 && (a.end - a.at) % n == 0 {
            (a.end - a.at) / n
        } else {
            132
        };
        if size < 68 {
            return Err(bad("a 3DM polysurface's trims are cut short"));
        }
        let mut out = Vec::with_capacity(n.min(65_536));
        for _ in 0..n {
            let start = a.at;
            a.i32()?;
            let curve = a.i32()?;
            let domain = a.interval()?;
            out.push(Trim { curve, domain });
            a.at = start;
            a.skip(size)?;
        }
        Ok(out)
    }

    fn loops(&self, bytes: &[u8], r: &mut Reader<'_>) -> Result<Vec<Loop>, CoreError> {
        let body = self.next_chunk(bytes, r)?;
        let mut a = Reader::new(bytes, body.start, body.end);
        a.u8()?;
        let n = a.count(super::MAX_FACES)?;
        let mut out = Vec::with_capacity(n.min(65_536));
        for _ in 0..n {
            a.i32()?;
            let trims = indexes(&mut a)?;
            let kind = a.i32()?;
            a.i32()?;
            out.push(Loop { trims, kind });
        }
        Ok(out)
    }

    fn faces(&self, bytes: &[u8], r: &mut Reader<'_>) -> Result<Vec<Face>, CoreError> {
        let body = self.next_chunk(bytes, r)?;
        let mut a = Reader::new(bytes, body.start, body.end);
        a.u8()?;
        let n = a.count(super::MAX_FACES)?;
        let mut out = Vec::with_capacity(n.min(65_536));
        for _ in 0..n {
            a.i32()?;
            let loops = indexes(&mut a)?;
            let surface = a.i32()?;
            let reversed = a.i32()? != 0;
            // Material channel.
            a.i32()?;
            out.push(Face {
                loops,
                surface,
                reversed,
            });
        }
        Ok(out)
    }

    /// Rhino's render mesh of each face, where saved.
    fn render_meshes(
        &self,
        bytes: &[u8],
        r: &mut Reader<'_>,
        faces: usize,
    ) -> Result<Vec<Option<Vec<Triangle>>>, CoreError> {
        let body = self.next_chunk(bytes, r)?;
        let mut a = Reader::new(bytes, body.start, body.end);
        let mut out = Vec::with_capacity(faces);
        for _ in 0..faces {
            if a.u8()? == 0 {
                out.push(None);
                continue;
            }
            let (uuid, data, next) = self.class_at(bytes, a.at, a.end)?;
            a.at = next;
            out.push(if uuid == ON_MESH {
                self.mesh(bytes, data).ok()
            } else {
                None
            });
        }
        Ok(out)
    }

    /// The triangles of a lone surface, over its whole domain.
    pub(super) fn lone_surface(
        &self,
        bytes: &[u8],
        uuid: &[u8; 16],
        data: Range<usize>,
    ) -> Result<Option<Vec<Triangle>>, CoreError> {
        let Some(surface) = self.surface(bytes, uuid, data)? else {
            return Ok(None);
        };
        let [u0, u1] = surface.domain(0);
        let [v0, v1] = surface.domain(1);
        // The size from a coarse sampling.
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for i in 0..=8 {
            for j in 0..=8 {
                let p = surface.at(
                    u0 + (u1 - u0) * f64::from(i) / 8.0,
                    v0 + (v1 - v0) * f64::from(j) / 8.0,
                );
                for c in 0..3 {
                    min[c] = min[c].min(p[c]);
                    max[c] = max[c].max(p[c]);
                }
            }
        }
        let ring = vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        Ok(Some(face_mesh(
            &surface,
            &[ring],
            tolerance(min, max),
            false,
        )))
    }

    /// The triangles of a serialised `ON_Extrusion`: its profile swept
    /// along a straight path, with end caps when the profile is closed.
    /// Mitred ends are taken square.
    pub(super) fn extrusion(
        &self,
        bytes: &[u8],
        data: Range<usize>,
    ) -> Result<Option<Vec<Triangle>>, CoreError> {
        let outer = self.chunk(bytes, data.start, data.end)?;
        let mut r = Reader::new(bytes, outer.body.start, outer.body.end);
        let major = r.i32()?;
        let minor = r.i32()?;
        if major != 1 || minor < 0 {
            return Ok(None);
        }
        let Some(profile) = self.curve_at(bytes, &mut r, 0)? else {
            return Ok(None);
        };
        let from = r.point()?;
        let to = r.point()?;
        let trim = r.interval()?;
        let up = r.point()?;
        // Mitre flags and planes.
        r.skip(2 + 48)?;
        r.interval()?;
        r.u8()?;
        let profiles = if minor >= 1 { r.count(4096)? } else { 1 };
        let caps = if minor >= 2 {
            Some([r.u8()? != 0, r.u8()? != 0])
        } else {
            None
        };
        let path = sub(to, from);
        let (Some(tangent), Some(up)) = (unit(path), unit(up)) else {
            return Err(bad("a 3DM extrusion has no direction"));
        };
        let Some(x) = unit(cross(up, tangent)) else {
            return Err(bad("a 3DM extrusion's up direction runs along it"));
        };
        let pieces = if profiles > 1 {
            profile.pieces()
        } else {
            vec![profile]
        };
        if pieces.len() != profiles {
            return Err(bad("a 3DM extrusion's profiles do not match their count"));
        }
        let start = add(from, scale(path, trim[0]));
        let end = add(from, scale(path, trim[1]));
        let length = geometry::length(sub(end, start));
        let rings: Vec<Vec<P2>> = pieces
            .iter()
            .map(|piece| {
                let [a, b] = piece.domain();
                let size = piece_size(piece);
                piece
                    .params(a, b, tolerance([0.0; 3], [size, size, length]))
                    .into_iter()
                    .map(|t| {
                        let p = piece.at(t);
                        [p[0], p[1]]
                    })
                    .collect()
            })
            .collect();
        let place = |p: P2, origin: P3| add(origin, add(scale(x, p[0]), scale(up, p[1])));
        let mut out = Vec::new();
        for ring in &rings {
            for w in ring.windows(2) {
                let (a0, b0) = (place(w[0], start), place(w[1], start));
                let (a1, b1) = (place(w[0], end), place(w[1], end));
                out.push([a0, b0, b1]);
                out.push([a0, b1, a1]);
            }
        }
        let closed: Vec<Vec<P2>> = rings
            .iter()
            .filter_map(|ring| {
                let (first, last) = (ring.first()?, ring.last()?);
                let gap = (first[0] - last[0]).hypot(first[1] - last[1]);
                (ring.len() >= 4 && gap < 1e-9 * (1.0 + first[0].abs() + first[1].abs()))
                    .then(|| ring[..ring.len() - 1].to_vec())
            })
            .collect();
        let caps = caps.unwrap_or([closed.len() == rings.len(); 2]);
        if closed.len() == rings.len() && (caps[0] || caps[1]) {
            let (outer, holes) = closed.split_at(1);
            let flat: Vec<P2> = closed.iter().flatten().copied().collect();
            for tri in triangulate(&outer[0], holes) {
                let [a, b, c] = tri.map(|i| flat[i]);
                if caps[0] {
                    out.push([place(a, start), place(c, start), place(b, start)]);
                }
                if caps[1] {
                    out.push([place(a, end), place(b, end), place(c, end)]);
                }
            }
        }
        Ok(Some(out))
    }
}

/// A rough size of a profile piece, for its tolerance.
fn piece_size(piece: &Curve) -> f64 {
    let [a, b] = piece.domain();
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for k in 0..=16 {
        let p = piece.at(a + (b - a) * f64::from(k) / 16.0);
        for c in 0..2 {
            min[c] = min[c].min(p[c]);
            max[c] = max[c].max(p[c]);
        }
    }
    (max[0] - min[0]).max(max[1] - min[1])
}

fn indexes(r: &mut Reader<'_>) -> Result<Vec<i32>, CoreError> {
    let n = r.count(super::MAX_FACES)?;
    (0..n).map(|_| r.i32()).collect()
}

/// The face's bounding loops as rings in its surface's parameter plane;
/// `None` when a trim's curve is missing or of a kind not read.
fn face_loops(
    face: &Face,
    loops: &[Loop],
    trims: &[Trim],
    curves: &[Option<Curve>],
) -> Option<Vec<Vec<P2>>> {
    let mut rings = Vec::new();
    let mut total = 0;
    for li in &face.loops {
        let lp = loops.get(usize::try_from(*li).ok()?)?;
        if !BOUNDING_LOOPS.contains(&lp.kind) {
            continue;
        }
        let mut ring: Vec<P2> = Vec::new();
        for ti in &lp.trims {
            let trim = trims.get(usize::try_from(*ti).ok()?)?;
            let curve = curves.get(usize::try_from(trim.curve).ok()?)?.as_ref()?;
            let [a, b] = trim.domain;
            if b <= a {
                continue;
            }
            let [c0, c1] = curve.domain();
            let (a, b) = (a.max(c0), b.min(c1));
            if b <= a {
                continue;
            }
            let size = trim_size(curve, a, b);
            for t in curve.params(a, b, size * TRIM_TOLERANCE_SHARE) {
                let p = curve.at(t);
                let q = [p[0], p[1]];
                if ring.last() != Some(&q) {
                    ring.push(q);
                }
            }
        }
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        total += ring.len();
        if total > MAX_LOOP_POINTS {
            return None;
        }
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    (!rings.is_empty()).then_some(rings)
}

fn trim_size(curve: &Curve, a: f64, b: f64) -> f64 {
    let p = curve.at(a);
    let q = curve.at(b);
    let m = curve.at((a + b) / 2.0);
    geometry::length(sub(p, m))
        .max(geometry::length(sub(q, m)))
        .max(1e-9)
}

/// Largest trim coordinate a face is meshed with.
const MAX_PARAMETER: f64 = 1e12;

/// Grid lines inside `(lo, hi)` with both ends.
fn clip_params(params: Vec<f64>, lo: f64, hi: f64) -> Vec<f64> {
    let mut out = vec![lo];
    out.extend(params.into_iter().filter(|p| *p > lo && *p < hi));
    out.push(hi);
    out
}

/// The cell holding `x` among grid lines `params`.
fn cell(params: &[f64], x: f64) -> usize {
    params
        .partition_point(|p| *p <= x)
        .saturating_sub(1)
        .min(params.len() - 2)
}

/// Triangles of `surface` inside `rings` (even-odd), in the surface's
/// parameter plane, facing along the surface normal unless `reversed`.
fn face_mesh(surface: &Surface, rings: &[Vec<P2>], tol: f64, reversed: bool) -> Vec<Triangle> {
    // A damaged file can hold trims at infinity or NaN, which the polygon
    // clipper cannot place; such a face is left out.
    if rings
        .iter()
        .flatten()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > MAX_PARAMETER)
    {
        return Vec::new();
    }
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in rings.iter().flatten() {
        for c in 0..2 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    if !(hi[0] > lo[0] && hi[1] > lo[1]) {
        return Vec::new();
    }
    let us = clip_params(surface.grid(0, lo[0], hi[0], tol), lo[0], hi[0]);
    let vs = clip_params(surface.grid(1, lo[1], hi[1], tol), lo[1], hi[1]);
    let (nu, nv) = (us.len() - 1, vs.len() - 1);
    // Cells a loop passes through.
    let mut boundary = vec![false; nu * nv];
    let segments: Vec<(P2, P2)> = rings
        .iter()
        .flat_map(|ring| (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()])))
        .collect();
    for (a, b) in &segments {
        let (i0, i1) = (cell(&us, a[0].min(b[0])), cell(&us, a[0].max(b[0])));
        for i in i0..=i1 {
            // The part of the segment over column i.
            let (x0, x1) = (us[i].max(a[0].min(b[0])), us[i + 1].min(a[0].max(b[0])));
            let y = |x: f64| {
                if (b[0] - a[0]).abs() > 0.0 {
                    a[1] + (x - a[0]) / (b[0] - a[0]) * (b[1] - a[1])
                } else {
                    a[1]
                }
            };
            let (ya, yb) = if (b[0] - a[0]).abs() > 0.0 {
                (y(x0), y(x1))
            } else {
                (a[1], b[1])
            };
            let (j0, j1) = (cell(&vs, ya.min(yb)), cell(&vs, ya.max(yb)));
            for j in j0..=j1 {
                boundary[i * nv + j] = true;
            }
        }
    }
    // Grid points evaluated once.
    let mut corners: Vec<Option<P3>> = vec![None; (nu + 1) * (nv + 1)];
    let mut corner = |i: usize, j: usize| -> P3 {
        *corners[i * (nv + 1) + j].get_or_insert_with(|| surface.at(us[i], vs[j]))
    };
    let mut out = Vec::new();
    let mut push = |t: [P3; 3]| {
        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
            out.push(if reversed { [t[0], t[2], t[1]] } else { t });
        }
    };
    let clip: Vec<Vec<P2>> = rings.to_vec();
    for j in 0..nv {
        // Crossings of the row's middle line, for the inside test.
        let yc = (vs[j] + vs[j + 1]) / 2.0;
        let mut crossings: Vec<f64> = segments
            .iter()
            .filter(|(a, b)| (a[1] > yc) != (b[1] > yc))
            .map(|(a, b)| a[0] + (yc - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
            .collect();
        crossings.sort_by(f64::total_cmp);
        for i in 0..nu {
            if boundary[i * nv + j] {
                let cell_ring = vec![
                    [us[i], vs[j]],
                    [us[i + 1], vs[j]],
                    [us[i + 1], vs[j + 1]],
                    [us[i], vs[j + 1]],
                ];
                let shapes =
                    vec![cell_ring].overlay(&clip, OverlayRule::Intersect, FillRule::EvenOdd);
                for shape in shapes {
                    let Some((outer, holes)) = shape.split_first() else {
                        continue;
                    };
                    let flat: Vec<P2> = shape.iter().flatten().copied().collect();
                    for tri in triangulate(outer, holes) {
                        push(tri.map(|k| surface.at(flat[k][0], flat[k][1])));
                    }
                }
            } else {
                let xc = (us[i] + us[i + 1]) / 2.0;
                if crossings.partition_point(|x| *x < xc) % 2 == 1 {
                    let (a, b, c, d) = (
                        corner(i, j),
                        corner(i + 1, j),
                        corner(i + 1, j + 1),
                        corner(i, j + 1),
                    );
                    push([a, b, c]);
                    push([a, c, d]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> Surface {
        Surface::Plane {
            origin: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            domain: [[0.0, 100.0], [0.0, 100.0]],
            extents: [[0.0, 100.0], [0.0, 100.0]],
        }
    }

    fn area(triangles: &[Triangle]) -> f64 {
        triangles
            .iter()
            .map(|t| geometry::length(cross(sub(t[1], t[0]), sub(t[2], t[0]))) / 2.0)
            .sum()
    }

    #[test]
    fn a_plate_with_a_hole_keeps_its_area() {
        let outer = vec![[10.0, 10.0], [90.0, 10.0], [90.0, 60.0], [10.0, 60.0]];
        let hole = vec![[40.0, 20.0], [60.0, 20.0], [60.0, 40.0], [40.0, 40.0]];
        let triangles = face_mesh(&plane(), &[outer, hole], 0.01, false);
        assert!((area(&triangles) - (80.0 * 50.0 - 400.0)).abs() < 1e-6);
        // Facing up, or down when the face is reversed.
        let up = triangles
            .iter()
            .all(|t| cross(sub(t[1], t[0]), sub(t[2], t[0]))[2] > 0.0);
        assert!(up);
        let down = face_mesh(
            &plane(),
            &[vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            0.01,
            true,
        );
        assert!(cross(sub(down[0][1], down[0][0]), sub(down[0][2], down[0][0]))[2] < 0.0);
    }

    #[test]
    fn a_curved_face_is_meshed_on_a_grid_and_trimmed() {
        // A half cylinder of radius 10 along y, trimmed to a disc in its
        // parameter plane.
        let surface = Surface::Rev {
            origin: [0.0; 3],
            axis: [0.0, 1.0, 0.0],
            angle: [0.0, std::f64::consts::PI],
            domain: [0.0, 1.0],
            transposed: false,
            profile: Box::new(Curve::Line {
                from: [10.0, 0.0, 0.0],
                to: [10.0, 40.0, 0.0],
                domain: [0.0, 1.0],
            }),
        };
        let disc: Vec<P2> = (0..64)
            .map(|k| {
                let a = f64::from(k) / 64.0 * std::f64::consts::TAU;
                [0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin()]
            })
            .collect();
        let triangles = face_mesh(&surface, &[disc], 0.01, false);
        assert!(triangles.len() > 100);
        for p in triangles.iter().flatten() {
            let r = p[0].hypot(p[2]);
            assert!((r - 10.0).abs() < 0.02, "{p:?}");
        }
    }
}
