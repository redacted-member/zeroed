//! Binary STL export of a relief heightfield as a closed solid: the relief
//! surface on top, four walls and a flat bottom, so other programs can
//! print or machine it.

use std::path::Path as FsPath;

use crate::error::CoreError;
use crate::model::field::Field;

use super::mesh::{Triangle, Vertex};

/// Size of a binary STL header in bytes.
const HEADER: usize = 80;

/// Writes `field` to `path` as a binary STL solid, closed down to `base_z`
/// millimetres. See [`stl_bytes`] for the shape.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the field is smaller than two by
/// two cells or its heights do not match its size, and [`CoreError::Io`]
/// when the file cannot be written.
pub fn write_stl(field: &Field, base_z: f64, path: &FsPath) -> Result<(), CoreError> {
    if field.width < 2 || field.height < 2 {
        return Err(CoreError::validation(
            "a relief needs at least two by two cells to export",
        ));
    }
    if field.z.len() != field.width * field.height {
        return Err(CoreError::validation(
            "a relief's heights do not match its size",
        ));
    }
    std::fs::write(path, stl_bytes(field, base_z))
        .map_err(|error| CoreError::io(path.display(), error))
}

/// A binary STL of `field` as a closed solid, lengths in millimetres.
///
/// The cell centres are the vertices. The top is two triangles per grid
/// square at the cells' heights (empty cells at `base_z`), four walls run
/// from the outer cells down to `base_z`, and the bottom is a fan at
/// `base_z`. Every triangle winds counter clockwise seen from outside and
/// carries its outward unit normal. A field smaller than two by two cells,
/// or whose heights do not match its size, gives a file with no triangles.
pub fn stl_bytes(field: &Field, base_z: f64) -> Vec<u8> {
    let triangles =
        if field.width >= 2 && field.height >= 2 && field.z.len() == field.width * field.height {
            solid(field, base_z)
        } else {
            Vec::new()
        };
    stl_of(&triangles)
}

/// A binary STL of `triangles`, each with its unit normal by the right
/// hand rule.
pub fn stl_of(triangles: &[Triangle]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + 4 + triangles.len() * 50);
    let mut header = [b' '; HEADER];
    let title = b"CarveStudio relief";
    header[..title.len()].copy_from_slice(title);
    out.extend_from_slice(&header);
    let count = u32::try_from(triangles.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&count.to_le_bytes());
    for t in triangles.iter().take(count as usize) {
        for v in normal(t) {
            out.extend_from_slice(&(v as f32).to_le_bytes());
        }
        for corner in t {
            for v in corner {
                out.extend_from_slice(&(*v as f32).to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// The triangles of the closed solid [`stl_bytes`] writes, for the other
/// mesh exports. Empty for a field smaller than two by two cells or whose
/// heights do not match its size.
pub fn solid_triangles(field: &Field, base_z: f64) -> Vec<Triangle> {
    if field.width >= 2 && field.height >= 2 && field.z.len() == field.width * field.height {
        solid(field, base_z)
    } else {
        Vec::new()
    }
}

/// The triangles of the closed solid.
fn solid(field: &Field, base_z: f64) -> Vec<Triangle> {
    let (w, h) = (field.width, field.height);
    let top = |i: usize, j: usize| -> Vertex {
        let c = field.centre(i, j);
        let z = field.z[j * w + i];
        [c.x, c.y, if z.is_nan() { base_z } else { f64::from(z) }]
    };
    let bottom = |i: usize, j: usize| -> Vertex {
        let c = field.centre(i, j);
        [c.x, c.y, base_z]
    };
    // The border, counter clockwise seen from above.
    let mut ring: Vec<(usize, usize)> = Vec::with_capacity(2 * (w + h));
    ring.extend((0..w - 1).map(|i| (i, 0)));
    ring.extend((0..h - 1).map(|j| (w - 1, j)));
    ring.extend((1..w).rev().map(|i| (i, h - 1)));
    ring.extend((1..h).rev().map(|j| (0, j)));

    let mut out = Vec::with_capacity(2 * (w - 1) * (h - 1) + 3 * ring.len());
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let (a, b, c, d) = (top(i, j), top(i + 1, j), top(i + 1, j + 1), top(i, j + 1));
            out.push([a, b, c]);
            out.push([a, c, d]);
        }
    }
    let middle = {
        let lo = field.centre(0, 0);
        let hi = field.centre(w - 1, h - 1);
        [(lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, base_z]
    };
    for k in 0..ring.len() {
        let (ai, aj) = ring[k];
        let (bi, bj) = ring[(k + 1) % ring.len()];
        let (at, bt) = (top(ai, aj), top(bi, bj));
        let (ab, bb) = (bottom(ai, aj), bottom(bi, bj));
        out.push([ab, bb, bt]);
        out.push([ab, bt, at]);
        out.push([middle, bb, ab]);
    }
    out
}

/// Unit normal of a triangle by the right hand rule, zero when degenerate.
fn normal(t: &Triangle) -> [f64; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len <= f64::MIN_POSITIVE {
        [0.0, 0.0, 0.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::geom::Point;
    use crate::io::mesh::stl;

    fn field(width: usize, height: usize) -> Field {
        let mut f = Field::empty(width, height, 2.0, Point::new(1.0, 1.0));
        for j in 0..height {
            for i in 0..width {
                f.z[j * width + i] = 1.0 + (i + j) as f32 * 0.5;
            }
        }
        f
    }

    fn expected(w: usize, h: usize) -> usize {
        let ring = 2 * (w - 1) + 2 * (h - 1);
        2 * (w - 1) * (h - 1) + 3 * ring
    }

    #[test]
    fn triangle_count_follows_the_grid() {
        for (w, h) in [(2, 2), (3, 3), (4, 3), (5, 7)] {
            let bytes = stl_bytes(&field(w, h), 0.0);
            let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]);
            assert_eq!(count as usize, expected(w, h));
            assert_eq!(bytes.len(), 84 + expected(w, h) * 50);
        }
        assert_eq!(expected(4, 3), 42);
    }

    #[test]
    fn it_reads_back_with_the_top_height() {
        let mut f = field(4, 3);
        f.z[5] = f32::NAN;
        let tris = stl(&stl_bytes(&f, -1.0)).expect("read back");
        assert_eq!(tris.len(), expected(4, 3));
        let top = tris.iter().flatten().map(|v| v[2]).fold(f64::MIN, f64::max);
        let low = tris.iter().flatten().map(|v| v[2]).fold(f64::MAX, f64::min);
        assert!((top - f64::from(f.max().expect("max"))).abs() < 1e-6);
        assert!((low + 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_solid_is_watertight_and_wound_outward() {
        let f = field(3, 3);
        let tris = stl(&stl_bytes(&f, 0.0)).expect("read back");
        let key = |v: &Vertex| (v[0].to_bits(), v[1].to_bits(), v[2].to_bits());
        let mut directed: BTreeMap<_, usize> = BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (key(&t[k]), key(&t[(k + 1) % 3]));
                *directed.entry((a, b)).or_default() += 1;
            }
        }
        for (&(a, b), &n) in &directed {
            assert_eq!(n, 1, "an edge is used twice the same way");
            assert_eq!(directed.get(&(b, a)), Some(&1), "an edge has no partner");
        }
        // Signed volume of a closed outward mesh is positive.
        let volume: f64 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (t[0], t[1], t[2]);
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0;
        assert!(volume > 0.0);
    }

    #[test]
    fn normals_point_out() {
        let bytes = stl_bytes(&field(3, 3), 0.0);
        let at =
            |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        // The first triangle is on top, the last on the bottom.
        assert!(at(84 + 8) > 0.0);
        let last = 84 + (expected(3, 3) - 1) * 50;
        assert!((at(last + 8) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_tiny_field_is_refused() {
        let f = Field::empty(1, 4, 1.0, Point::new(0.0, 0.0));
        let dir = std::env::temp_dir().join("carve_stl_export_tiny.stl");
        assert!(write_stl(&f, 0.0, &dir).is_err());
    }
}
