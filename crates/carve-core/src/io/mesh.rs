//! Triangle meshes for 3D model import: binary and ASCII STL, and OBJ
//! (vertex positions and faces; polygons are fanned into triangles), plus
//! 3DS, DXF 3D faces and Rhino 3DM meshes, surfaces and polysurfaces
//! through their own readers. Lengths come through as the file wrote them,
//! except 3DM files, which name their units and come in as millimetres.

use std::path::Path as FsPath;

use crate::error::CoreError;

/// A point of a mesh.
pub type Vertex = [f64; 3];

/// A triangle of a mesh.
pub type Triangle = [Vertex; 3];

/// Reads the triangles of a `.stl`, `.obj`, `.3ds`, `.dxf` or `.3dm` file.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be read and
/// [`CoreError::Validation`] when it is not a mesh this reader understands
/// or holds no triangles.
pub fn read(path: &FsPath) -> Result<Vec<Triangle>, CoreError> {
    let bytes = std::fs::read(path).map_err(|error| CoreError::io(path.display(), error))?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let triangles = match extension.as_str() {
        "stl" => stl(&bytes)?,
        "obj" => obj(&String::from_utf8_lossy(&bytes))?,
        "3ds" => super::mesh_3ds::read(&bytes)?,
        "dxf" => super::dxf_mesh::read(&bytes)?,
        "3dm" => {
            let read = super::mesh_3dm::read(&bytes)?;
            if read.meshes + read.surfaces == 0 && read.skipped > 0 {
                return Err(CoreError::validation(
                    "this 3DM file holds no meshes, surfaces or polysurfaces that can be read; mesh the model in Rhino and save again",
                ));
            }
            read.triangles
        }
        _ => {
            return Err(CoreError::validation(
                "3D models can be imported from STL, OBJ, 3DS, DXF or 3DM files",
            ))
        }
    };
    if triangles.is_empty() {
        return Err(CoreError::validation("the file holds no triangles"));
    }
    Ok(triangles)
}

/// Triangles of an STL file, binary or ASCII.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a file that is neither.
pub fn stl(bytes: &[u8]) -> Result<Vec<Triangle>, CoreError> {
    // A binary STL says how many triangles follow its 80 byte header; its
    // size then matches exactly. ASCII files often start with "solid" too,
    // so the size is the reliable test.
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if bytes.len() == 84 + count * 50 {
            let mut out = Vec::with_capacity(count);
            for k in 0..count {
                let base = 84 + k * 50 + 12;
                let f = |o: usize| {
                    let at = base + o * 4;
                    f64::from(f32::from_le_bytes([
                        bytes[at],
                        bytes[at + 1],
                        bytes[at + 2],
                        bytes[at + 3],
                    ]))
                };
                out.push([[f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)]]);
            }
            return Ok(out);
        }
    }
    let text = String::from_utf8_lossy(bytes);
    if !text.trim_start().starts_with("solid") {
        return Err(CoreError::validation("this is not an STL file"));
    }
    let mut out = Vec::new();
    let mut corners: Vec<Vertex> = Vec::with_capacity(3);
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("vertex") {
            continue;
        }
        let numbers: Vec<f64> = words.filter_map(|w| w.parse().ok()).collect();
        if numbers.len() != 3 {
            return Err(CoreError::validation("an STL vertex needs three numbers"));
        }
        corners.push([numbers[0], numbers[1], numbers[2]]);
        if corners.len() == 3 {
            out.push([corners[0], corners[1], corners[2]]);
            corners.clear();
        }
    }
    Ok(out)
}

/// Triangles of an OBJ file: `v` positions and `f` faces, fanned.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a face that names a missing vertex.
pub fn obj(text: &str) -> Result<Vec<Triangle>, CoreError> {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => {
                let numbers: Vec<f64> = words.take(3).filter_map(|w| w.parse().ok()).collect();
                if numbers.len() == 3 {
                    vertices.push([numbers[0], numbers[1], numbers[2]]);
                }
            }
            Some("f") => {
                let mut face = Vec::new();
                for word in words {
                    // "7", "7/2" or "7/2/5"; negative counts back from the end.
                    let index: i64 = word
                        .split('/')
                        .next()
                        .and_then(|i| i.parse().ok())
                        .ok_or_else(|| {
                            CoreError::validation("an OBJ face index is not a number")
                        })?;
                    let resolved = if index < 0 {
                        vertices.len() as i64 + index
                    } else {
                        index - 1
                    };
                    let vertex = usize::try_from(resolved)
                        .ok()
                        .and_then(|i| vertices.get(i).copied())
                        .ok_or_else(|| {
                            CoreError::validation("an OBJ face names a missing vertex")
                        })?;
                    face.push(vertex);
                }
                for k in 1..face.len().saturating_sub(1) {
                    out.push([face[0], face[k], face[k + 1]]);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary(triangles: &[Triangle]) -> Vec<u8> {
        let mut out = vec![0u8; 80];
        out.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for t in triangles {
            out.extend_from_slice(&[0u8; 12]);
            for v in t {
                for c in v {
                    out.extend_from_slice(&(*c as f32).to_le_bytes());
                }
            }
            out.extend_from_slice(&[0u8; 2]);
        }
        out
    }

    #[test]
    fn binary_and_ascii_stl_read_the_same_triangles() {
        let tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 1.0], [0.0, 5.0, 2.0]];
        assert_eq!(stl(&binary(&[tri])).expect("binary"), vec![tri]);
        let ascii = "solid t\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 10 0 1\n   vertex 0 5 2\n  endloop\n endfacet\nendsolid t\n";
        assert_eq!(stl(ascii.as_bytes()).expect("ascii"), vec![tri]);
        assert!(stl(b"not a mesh at all").is_err());
    }

    #[test]
    fn obj_faces_are_fanned_into_triangles() {
        let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1 2/2 3/3 4/4\nf -4 -3 -2\n";
        let triangles = obj(text).expect("obj");
        assert_eq!(triangles.len(), 3);
        assert_eq!(triangles[1][2], [0.0, 1.0, 0.0]);
        assert!(obj("v 0 0 0\nf 1 2 3\n").is_err());
    }
}
