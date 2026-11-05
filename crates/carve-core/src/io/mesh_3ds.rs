//! 3D Studio (`.3ds`) mesh import: the triangle meshes of every object in
//! the file. Lengths come through as the file wrote them.
//!
//! A 3DS file is a tree of chunks, each a little endian `u16` id and a
//! `u32` length that counts its own six byte header. Only the chunks on the
//! way to the geometry are opened: main, editor, named object, triangle
//! mesh, then its vertex list (`u16` count, three `f32` each) and face list
//! (`u16` count, three `u16` corners and a `u16` of edge flags each).
//! Vertices are stored already placed in the scene, so the local axes chunk
//! is not applied. Materials, lights, cameras and keyframes are skipped.

use crate::error::CoreError;

use super::mesh::{Triangle, Vertex};

/// The whole file.
const MAIN: u16 = 0x4D4D;
/// The 3D editor part: objects and materials.
const EDITOR: u16 = 0x3D3D;
/// A named object; its name is a zero terminated string before its chunks.
const OBJECT: u16 = 0x4000;
/// A triangle mesh inside an object.
const TRIMESH: u16 = 0x4100;
/// Vertex positions of a triangle mesh.
const VERTICES: u16 = 0x4110;
/// Faces of a triangle mesh.
const FACES: u16 = 0x4120;

/// Size of a chunk header in bytes.
const HEADER: usize = 6;

/// Chunks nested deeper than this are not a mesh this reader understands.
const MAX_DEPTH: usize = 16;

fn format_error(message: &str) -> CoreError {
    CoreError::format("3DS", message)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(bytes: &[u8], at: usize) -> Option<f64> {
    let b = bytes.get(at..at + 4)?;
    Some(f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
}

/// The chunks laid end to end in `bytes`, as (id, body).
fn chunks(bytes: &[u8]) -> Result<Vec<(u16, &[u8])>, CoreError> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + HEADER <= bytes.len() {
        let id = u16_at(bytes, at).ok_or_else(|| format_error("a chunk is cut short"))?;
        let length =
            u32_at(bytes, at + 2).ok_or_else(|| format_error("a chunk is cut short"))? as usize;
        if length < HEADER || at + length > bytes.len() {
            return Err(format_error("a chunk runs past the end of its parent"));
        }
        out.push((id, &bytes[at + HEADER..at + length]));
        at += length;
    }
    Ok(out)
}

/// Mesh being gathered from one triangle mesh chunk.
#[derive(Default)]
struct Part {
    vertices: Vec<Vertex>,
    faces: Vec<[u16; 3]>,
}

fn walk(bytes: &[u8], depth: usize, out: &mut Vec<Triangle>) -> Result<(), CoreError> {
    if depth > MAX_DEPTH {
        return Err(format_error("chunks are nested too deeply"));
    }
    for (id, body) in chunks(bytes)? {
        match id {
            MAIN | EDITOR => walk(body, depth + 1, out)?,
            OBJECT => {
                let name_end = body
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or_else(|| format_error("an object name is not terminated"))?;
                walk(&body[name_end + 1..], depth + 1, out)?;
            }
            TRIMESH => {
                let mut part = Part::default();
                for (sub, data) in chunks(body)? {
                    match sub {
                        VERTICES => part.vertices = vertices(data)?,
                        FACES => part.faces = faces(data)?,
                        _ => {}
                    }
                }
                for f in &part.faces {
                    let corner = |k: usize| {
                        part.vertices
                            .get(usize::from(f[k]))
                            .copied()
                            .ok_or_else(|| format_error("a face names a missing vertex"))
                    };
                    out.push([corner(0)?, corner(1)?, corner(2)?]);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn vertices(data: &[u8]) -> Result<Vec<Vertex>, CoreError> {
    let count = usize::from(u16_at(data, 0).ok_or_else(|| format_error("no vertex count"))?);
    if data.len() < 2 + count * 12 {
        return Err(format_error("the vertex list is cut short"));
    }
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let base = 2 + k * 12;
        let read = |o: usize| f32_at(data, base + o * 4).unwrap_or(0.0);
        out.push([read(0), read(1), read(2)]);
    }
    Ok(out)
}

fn faces(data: &[u8]) -> Result<Vec<[u16; 3]>, CoreError> {
    let count = usize::from(u16_at(data, 0).ok_or_else(|| format_error("no face count"))?);
    if data.len() < 2 + count * 8 {
        return Err(format_error("the face list is cut short"));
    }
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let base = 2 + k * 8;
        let read = |o: usize| u16_at(data, base + o * 2).unwrap_or(0);
        out.push([read(0), read(1), read(2)]);
    }
    Ok(out)
}

/// Triangles of every mesh object in a 3DS file.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not a 3DS file or a
/// chunk is damaged.
pub fn read(bytes: &[u8]) -> Result<Vec<Triangle>, CoreError> {
    if u16_at(bytes, 0) != Some(MAIN) {
        return Err(format_error("this is not a 3D Studio file"));
    }
    let mut out = Vec::new();
    walk(bytes, 0, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend_from_slice(&((body.len() + HEADER) as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    /// An object to write: name, vertices and faces.
    type Object<'a> = (&'a str, Vec<Vertex>, Vec<[u16; 3]>);

    /// A 3DS file with one object per mesh, each a vertex and a face list.
    fn write(meshes: &[Object]) -> Vec<u8> {
        let mut editor = chunk(0x3D3E, &3u32.to_le_bytes());
        for (name, verts, faces) in meshes {
            let mut v = (verts.len() as u16).to_le_bytes().to_vec();
            for p in verts {
                for c in p {
                    v.extend_from_slice(&(*c as f32).to_le_bytes());
                }
            }
            let mut f = (faces.len() as u16).to_le_bytes().to_vec();
            for t in faces {
                for c in t {
                    f.extend_from_slice(&c.to_le_bytes());
                }
                f.extend_from_slice(&7u16.to_le_bytes());
            }
            // A material group inside the face list, as exporters write.
            f.extend(chunk(0x4130, b"Wood\0\x00\x00"));
            let mut mesh = chunk(VERTICES, &v);
            mesh.extend(chunk(0x4160, &[0u8; 48]));
            mesh.extend(chunk(FACES, &f));
            let mut object = name.as_bytes().to_vec();
            object.push(0);
            object.extend(chunk(TRIMESH, &mesh));
            editor.extend(chunk(OBJECT, &object));
        }
        editor.extend(chunk(0xAFFF, b"material"));
        let mut main = chunk(0x0002, &3u32.to_le_bytes());
        main.extend(chunk(EDITOR, &editor));
        main.extend(chunk(0xB000, b"keyframes"));
        chunk(MAIN, &main)
    }

    #[test]
    fn meshes_of_every_object_are_read() {
        let square = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 2.5],
            [0.0, 10.0, 2.5],
        ];
        let tri = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let bytes = write(&[
            ("Square", square.clone(), vec![[0, 1, 2], [0, 2, 3]]),
            ("Tri", tri.clone(), vec![[0, 1, 2]]),
        ]);
        let triangles = read(&bytes).expect("reads");
        assert_eq!(
            triangles,
            vec![
                [square[0], square[1], square[2]],
                [square[0], square[2], square[3]],
                [tri[0], tri[1], tri[2]],
            ]
        );
    }

    #[test]
    fn damaged_files_are_refused() {
        assert!(read(b"solid not 3ds").is_err());
        let good = write(&[("A", vec![[0.0; 3]; 3], vec![[0, 1, 2]])]);
        assert!(read(&good[..good.len() - 5]).is_err());
        let missing = write(&[("A", vec![[0.0; 3]; 3], vec![[0, 1, 9]])]);
        let error = read(&missing).expect_err("bad index");
        assert!(error.to_string().contains("missing vertex"), "{error}");
    }
}
