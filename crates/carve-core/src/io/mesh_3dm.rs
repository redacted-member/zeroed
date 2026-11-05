//! Rhino `.3dm` files, read for their meshes, surfaces and polysurfaces.
//!
//! The layout is openNURBS's, the public toolkit Rhino files are written
//! with: a 32 byte header naming the archive version, then chunks, each a
//! type code and a length (four bytes before version 5, eight after), short
//! chunks holding their value in place of the length and chunks whose type
//! code has the CRC bit ending in a CRC-32. The object table holds one
//! record per object; a record's class chunk names the object's class by
//! UUID and carries its serialised data.
//!
//! Mesh objects (`ON_Mesh`) are decoded: counts, the face list (quads are
//! split along their shorter diagonal) and the vertex array, stored raw by
//! old writers and as a zlib compressed buffer by current ones.
//! Polysurfaces (`ON_Brep`), extrusions (`ON_Extrusion`) and lone NURBS,
//! plane, revolved and sum surfaces are meshed here (see [`brep`] and
//! [`geometry`]). Other objects (curves, points, annotations, SubD) are
//! counted and left out. Lengths are turned into millimetres from the
//! model units in the settings table.

use std::io::Read as _;

use super::mesh::{Triangle, Vertex};
use crate::error::CoreError;

mod brep;
mod geometry;

/// The fixed start of every 3DM file.
const MAGIC: &[u8; 24] = b"3D Geometry File Format ";
/// Chunk type code bits.
const TCODE_SHORT: u32 = 0x8000_0000;
const TCODE_CRC: u32 = 0x0000_8000;
/// Chunk type codes.
const ENDOFFILE: u32 = 0x0000_7fff;
const OBJECT_TABLE: u32 = 0x1000_0013;
const SETTINGS_TABLE: u32 = 0x1000_0015;
const UNITS_AND_TOLERANCES: u32 = 0x2000_8031;
const OBJECT_RECORD: u32 = 0x2000_8070;
const OPENNURBS_CLASS: u32 = 0x0002_7ffa;
const CLASS_UUID: u32 = 0x0002_fffb;
const CLASS_DATA: u32 = 0x0002_fffc;
const ANONYMOUS: u32 = 0x4000_8000;
/// `ON_Mesh`, {4ED7D4E4-E947-11D3-BFE5-0010830122F0}, as stored: the
/// first three fields little endian.
const ON_MESH: [u8; 16] = [
    0xe4, 0xd4, 0xd7, 0x4e, 0x47, 0xe9, 0xd3, 0x11, 0xbf, 0xe5, 0x00, 0x10, 0x83, 0x01, 0x22, 0xf0,
];
/// Largest counts accepted, which keeps a damaged file from asking for
/// absurd allocations.
const MAX_VERTICES: usize = 50_000_000;
const MAX_FACES: usize = 100_000_000;

/// Most triangles taken from one file.
const MAX_TRIANGLES: usize = 40_000_000;

/// What a 3DM file gave up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Read3dm {
    /// Triangles of every object read, in millimetres.
    pub triangles: Vec<Triangle>,
    /// Mesh objects read.
    pub meshes: usize,
    /// Surfaces, polysurfaces and extrusions meshed.
    pub surfaces: usize,
    /// Objects of other kinds, or in forms not read, which were left out.
    pub skipped: usize,
    /// Millimetres per file unit, from the file's model units (1 when the
    /// file names none).
    pub mm_per_unit: f64,
}

/// Millimetres per unit of openNURBS's length unit system `code`; custom
/// units give their own metres per unit.
fn mm_per_unit(code: i32, custom_metres: Option<f64>) -> f64 {
    match code {
        1 => 1e-3,
        3 => 10.0,
        4 => 1e3,
        5 => 1e6,
        6 => 25.4e-6,
        7 => 25.4e-3,
        8 => 25.4,
        9 => 304.8,
        10 => 1_609_344.0,
        11 => custom_metres
            .filter(|m| m.is_finite() && *m > 0.0)
            .map_or(1.0, |m| m * 1e3),
        12 => 1e-7,
        13 => 1e-6,
        14 => 100.0,
        15 => 1e4,
        16 => 1e5,
        17 => 1e9,
        18 => 1e12,
        19 => 914.4,
        20 => 25.4 / 72.0,
        21 => 25.4 / 6.0,
        22 => 1_852_000.0,
        23 => 1.495_978_707e14,
        24 => 9.460_730_472_580_8e18,
        25 => 3.085_677_581_491_37e19,
        // None, millimetres or unknown.
        _ => 1.0,
    }
}

/// True when the bytes start like a 3DM file.
pub fn looks_like_3dm(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// Reads the mesh objects of a 3DM file.
///
/// # Errors
/// [`CoreError::Validation`] when the bytes are not a 3DM file, a chunk
/// runs past its parent, or a mesh is damaged.
pub fn read(bytes: &[u8]) -> Result<Read3dm, CoreError> {
    if !looks_like_3dm(bytes) || bytes.len() < 32 {
        return Err(bad("this is not a Rhino 3DM file"));
    }
    let version: u32 = std::str::from_utf8(&bytes[24..32])
        .ok()
        .map(str::trim)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| bad("the 3DM header has no version"))?;
    if version < 2 {
        return Err(bad(
            "3DM files from Rhino 1 are not read; save the file again from a newer Rhino",
        ));
    }
    let archive = Archive {
        wide: version >= 50,
    };
    let mut out = Read3dm {
        mm_per_unit: 1.0,
        ..Read3dm::default()
    };
    let mut at = 32;
    while at < bytes.len() {
        let chunk = archive.chunk(bytes, at, bytes.len())?;
        if chunk.typecode == ENDOFFILE {
            break;
        }
        if chunk.typecode == SETTINGS_TABLE {
            if let Some(scale) = archive.units(bytes, chunk.body.clone()) {
                out.mm_per_unit = scale;
            }
        }
        if chunk.typecode == OBJECT_TABLE {
            archive.table(bytes, chunk.body.clone(), &mut out)?;
        }
        at = chunk.next;
    }
    if out.mm_per_unit != 1.0 {
        let s = out.mm_per_unit;
        for p in out.triangles.iter_mut().flatten() {
            *p = [p[0] * s, p[1] * s, p[2] * s];
        }
    }
    Ok(out)
}

fn bad(message: &str) -> CoreError {
    CoreError::validation(message)
}

/// How the archive frames its chunks.
struct Archive {
    /// Eight byte lengths (version 5 and later).
    wide: bool,
}

/// One chunk: its type code, the body without any trailing CRC, and where
/// the next chunk starts.
struct Chunk {
    typecode: u32,
    body: std::ops::Range<usize>,
    next: usize,
}

impl Archive {
    fn chunk(&self, bytes: &[u8], at: usize, end: usize) -> Result<Chunk, CoreError> {
        let mut reader = Reader::new(bytes, at, end);
        let typecode = reader.u32()?;
        let value = if self.wide {
            reader.i64()?
        } else {
            i64::from(reader.u32()?)
        };
        let start = reader.at;
        if typecode & TCODE_SHORT != 0 || value < 0 {
            return Ok(Chunk {
                typecode,
                body: start..start,
                next: start,
            });
        }
        let length = usize::try_from(value).map_err(|_| bad("a 3DM chunk is too long"))?;
        let stop = start
            .checked_add(length)
            .filter(|stop| *stop <= end)
            .ok_or_else(|| bad("a 3DM chunk runs past the end of the file"))?;
        let crc = if typecode & TCODE_CRC != 0 || typecode == CLASS_UUID {
            4
        } else {
            0
        };
        Ok(Chunk {
            typecode,
            body: start..stop.saturating_sub(crc).max(start),
            next: stop,
        })
    }

    /// The chunks directly inside `range`.
    fn children(
        &self,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> Result<Vec<Chunk>, CoreError> {
        let mut out = Vec::new();
        let mut at = range.start;
        // A chunk header is at least eight bytes; anything shorter left over
        // is padding.
        while at + 8 <= range.end {
            let chunk = self.chunk(bytes, at, range.end)?;
            at = chunk.next;
            out.push(chunk);
        }
        Ok(out)
    }

    /// Millimetres per model unit, from the settings table's units and
    /// tolerances record: a version, the unit system and, from version
    /// 102, metres per unit for custom units after the tolerances and
    /// display settings.
    fn units(&self, bytes: &[u8], range: std::ops::Range<usize>) -> Option<f64> {
        let record = self
            .children(bytes, range)
            .ok()?
            .into_iter()
            .find(|c| c.typecode == UNITS_AND_TOLERANCES)?;
        let mut r = Reader::new(bytes, record.body.start, record.body.end);
        let version = r.i32().ok()?;
        let code = r.i32().ok()?;
        let custom = if version >= 102 {
            r.skip(3 * 8 + 2 * 4).ok()?;
            r.f64().ok()
        } else {
            None
        };
        Some(mm_per_unit(code, custom))
    }

    fn table(
        &self,
        bytes: &[u8],
        range: std::ops::Range<usize>,
        out: &mut Read3dm,
    ) -> Result<(), CoreError> {
        for record in self.children(bytes, range)? {
            if record.typecode != OBJECT_RECORD {
                continue;
            }
            for class in self.children(bytes, record.body.clone())? {
                if class.typecode != OPENNURBS_CLASS {
                    continue;
                }
                let Ok((uuid, data)) = self.class(bytes, class.body.clone()) else {
                    out.skipped += 1;
                    continue;
                };
                if uuid == ON_MESH {
                    let mesh = self.mesh(bytes, data)?;
                    out.triangles.extend(mesh);
                    out.meshes += 1;
                } else {
                    // A polysurface or surface this reader cannot mesh fails
                    // the whole import: leaving it out would bring in a model
                    // with parts silently missing. Other kinds of object
                    // (curves, points, notes) have no surface and are left out.
                    let (meshed, what) = match uuid {
                        brep::ON_BREP => (self.brep(bytes, data), "polysurface"),
                        brep::ON_EXTRUSION => (self.extrusion(bytes, data), "extrusion"),
                        _ if geometry::is_surface(&uuid) => {
                            (self.lone_surface(bytes, &uuid, data), "surface")
                        }
                        _ => {
                            out.skipped += 1;
                            continue;
                        }
                    };
                    match meshed {
                        Ok(Some(triangles)) if !triangles.is_empty() => {
                            out.triangles.extend(triangles);
                            out.surfaces += 1;
                        }
                        _ => {
                            return Err(bad(&format!(
                                "this 3DM file has a {what} that cannot be read, so nothing was imported; mesh it in Rhino (Mesh command) and save again"
                            )));
                        }
                    }
                }
                if out.triangles.len() > MAX_TRIANGLES {
                    return Err(bad(
                        "this 3DM file makes too many triangles to import; mesh it more coarsely in Rhino",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Triangles of one serialised `ON_Mesh`.
    fn mesh(
        &self,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> Result<Vec<Triangle>, CoreError> {
        let mut r = Reader::new(bytes, range.start, range.end);
        let version = r.u8()?;
        let major = version >> 4;
        if major != 1 && major != 3 {
            return Err(bad(
                "this 3DM mesh was written in a form this reader does not know",
            ));
        }
        let vertex_count = r.count(MAX_VERTICES)?;
        let face_count = r.count(MAX_FACES)?;
        // Packed texture, surface domains, surface scale, the vertex, normal
        // and texture boxes and the closed flag.
        r.skip(4 * 16 + 2 * 8 + 16 * 4 + 4)?;
        // Optional meshing parameters and four curvature statistics, each a
        // presence byte and a chunk.
        for _ in 0..5 {
            if r.u8()? != 0 {
                let chunk = self.chunk(bytes, r.at, r.end)?;
                r.at = chunk.next;
            }
        }
        let width = r.i32()?;
        if !matches!(width, 1 | 2 | 4) {
            return Err(bad("a 3DM mesh has faces of an unknown index size"));
        }
        let face_bytes = r.take(face_count * 4 * width as usize)?;
        let vertices: Vec<Vertex> = if major == 1 {
            let count = r.i32()?;
            if usize::try_from(count).ok() != Some(vertex_count) {
                return Err(bad("a 3DM mesh's vertex list does not match its count"));
            }
            points(r.take(vertex_count * 12)?)
        } else {
            points(&self.buffer(bytes, &mut r, vertex_count * 12)?)
        };
        let index = |face: usize, corner: usize| -> usize {
            let at = (face * 4 + corner) * width as usize;
            match width {
                1 => usize::from(face_bytes[at]),
                2 => usize::from(u16::from_le_bytes([face_bytes[at], face_bytes[at + 1]])),
                _ => u32::from_le_bytes([
                    face_bytes[at],
                    face_bytes[at + 1],
                    face_bytes[at + 2],
                    face_bytes[at + 3],
                ]) as usize,
            }
        };
        let mut out = Vec::with_capacity(face_count * 2);
        for face in 0..face_count {
            let corners = [
                index(face, 0),
                index(face, 1),
                index(face, 2),
                index(face, 3),
            ];
            if corners.iter().any(|c| *c >= vertices.len()) {
                return Err(bad("a 3DM mesh face names a missing vertex"));
            }
            let v = corners.map(|c| vertices[c]);
            if corners[2] == corners[3] {
                out.push([v[0], v[1], v[2]]);
            } else if distance2(v[0], v[2]) <= distance2(v[1], v[3]) {
                out.push([v[0], v[1], v[2]]);
                out.push([v[0], v[2], v[3]]);
            } else {
                out.push([v[0], v[1], v[3]]);
                out.push([v[1], v[2], v[3]]);
            }
        }
        Ok(out)
    }

    /// A buffer written by openNURBS's compressed buffer writer: its size,
    /// a CRC, a method byte, then the bytes as they are or zlib compressed
    /// in an anonymous chunk.
    fn buffer(
        &self,
        bytes: &[u8],
        r: &mut Reader<'_>,
        expected: usize,
    ) -> Result<Vec<u8>, CoreError> {
        let size = r.u32()? as usize;
        if size == 0 {
            return Ok(Vec::new());
        }
        if size != expected {
            return Err(bad("a 3DM mesh buffer does not match its vertex count"));
        }
        let _crc = r.u32()?;
        match r.u8()? {
            0 => Ok(r.take(size)?.to_vec()),
            1 => {
                let chunk = self.chunk(bytes, r.at, r.end)?;
                if chunk.typecode != ANONYMOUS {
                    return Err(bad("a 3DM mesh buffer is not where it should be"));
                }
                r.at = chunk.next;
                let mut out = Vec::with_capacity(size);
                flate2::read::ZlibDecoder::new(&bytes[chunk.body.clone()])
                    .take(size as u64)
                    .read_to_end(&mut out)
                    .map_err(|_| bad("a 3DM mesh buffer does not decompress"))?;
                if out.len() != size {
                    return Err(bad("a 3DM mesh buffer is cut short"));
                }
                Ok(out)
            }
            _ => Err(bad("a 3DM mesh buffer uses an unknown compression")),
        }
    }
}

fn points(bytes: &[u8]) -> Vec<Vertex> {
    bytes
        .chunks_exact(12)
        .map(|p| {
            let f = |k: usize| f64::from(f32::from_le_bytes([p[k], p[k + 1], p[k + 2], p[k + 3]]));
            [f(0), f(4), f(8)]
        })
        .collect()
}

fn distance2(a: Vertex, b: Vertex) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// A bounded little endian reader.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], at: usize, end: usize) -> Self {
        Reader {
            bytes,
            at,
            end: end.min(bytes.len()),
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CoreError> {
        let stop = self
            .at
            .checked_add(n)
            .filter(|stop| *stop <= self.end)
            .ok_or_else(|| bad("a 3DM file is cut short"))?;
        let out = &self.bytes[self.at..stop];
        self.at = stop;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<(), CoreError> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, CoreError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32, CoreError> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i64(&mut self) -> Result<i64, CoreError> {
        let b = self.take(8)?;
        Ok(i64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn count(&mut self, cap: usize) -> Result<usize, CoreError> {
        usize::try_from(self.i32()?)
            .ok()
            .filter(|n| *n <= cap)
            .ok_or_else(|| bad("a 3DM mesh has an impossible size"))
    }
}

#[cfg(test)]
mod surface_tests;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write as _;

    /// Writes 3DM archives the way openNURBS lays them out, for tests.
    pub(crate) struct Writer {
        pub(crate) version: u32,
    }

    impl Writer {
        fn wide(&self) -> bool {
            self.version >= 50
        }

        pub(crate) fn chunk(&self, typecode: u32, body: &[u8]) -> Vec<u8> {
            let crc = typecode & TCODE_CRC != 0 || typecode == CLASS_UUID;
            let length = body.len() + if crc { 4 } else { 0 };
            let mut out = typecode.to_le_bytes().to_vec();
            if self.wide() {
                out.extend((length as i64).to_le_bytes());
            } else {
                out.extend((length as u32).to_le_bytes());
            }
            out.extend_from_slice(body);
            if crc {
                out.extend(crc32(body).to_le_bytes());
            }
            out
        }

        pub(crate) fn short(&self, typecode: u32, value: i64) -> Vec<u8> {
            let mut out = (typecode | TCODE_SHORT).to_le_bytes().to_vec();
            if self.wide() {
                out.extend(value.to_le_bytes());
            } else {
                out.extend((value as u32).to_le_bytes());
            }
            out
        }

        /// A serialised mesh: major 3 with a compressed vertex buffer, or
        /// major 1 with raw arrays.
        pub(crate) fn mesh(
            &self,
            vertices: &[[f32; 3]],
            faces: &[[u32; 4]],
            compressed: bool,
        ) -> Vec<u8> {
            let mut out = vec![if compressed { 0x35 } else { 0x10 }];
            out.extend((vertices.len() as i32).to_le_bytes());
            out.extend((faces.len() as i32).to_le_bytes());
            out.extend(vec![0u8; 4 * 16 + 2 * 8 + 16 * 4 + 4]);
            // Meshing parameters present, in a chunk; no curvature stats.
            out.push(1);
            out.extend(self.chunk(ANONYMOUS, &[1, 2, 3]));
            out.extend([0, 0, 0, 0]);
            let width: i32 = if vertices.len() < 256 { 1 } else { 4 };
            out.extend(width.to_le_bytes());
            for face in faces {
                for index in face {
                    if width == 1 {
                        out.push(*index as u8);
                    } else {
                        out.extend(index.to_le_bytes());
                    }
                }
            }
            let raw: Vec<u8> = vertices
                .iter()
                .flat_map(|v| v.iter().flat_map(|c| c.to_le_bytes()))
                .collect();
            if compressed {
                out.extend((raw.len() as u32).to_le_bytes());
                out.extend(crc32(&raw).to_le_bytes());
                out.push(1);
                let mut z =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                z.write_all(&raw).expect("compresses");
                out.extend(self.chunk(ANONYMOUS, &z.finish().expect("compresses")));
                // Normals, texture, curvature and colour buffers left empty.
                for _ in 0..4 {
                    out.extend(0u32.to_le_bytes());
                }
            } else {
                out.extend((vertices.len() as i32).to_le_bytes());
                out.extend(&raw);
                for _ in 0..4 {
                    out.extend(0i32.to_le_bytes());
                }
            }
            out
        }

        /// A file whose object table holds one record per `(uuid, data)`.
        pub(crate) fn file(&self, objects: &[([u8; 16], Vec<u8>)]) -> Vec<u8> {
            self.file_in(objects, None)
        }

        /// A file whose settings name `units` (a units record: version,
        /// unit system, then the rest) and whose object table holds one
        /// record per `(uuid, data)`.
        pub(crate) fn file_in(
            &self,
            objects: &[([u8; 16], Vec<u8>)],
            units: Option<Vec<u8>>,
        ) -> Vec<u8> {
            let mut out = MAGIC.to_vec();
            out.extend(format!("{:>8}", self.version).bytes());
            out.extend(self.chunk(0x0000_0001, b"made for a test"));
            if let Some(units) = units {
                let mut settings = self.short(0xa000_0038, 0);
                settings.extend(self.chunk(UNITS_AND_TOLERANCES, &units));
                settings.extend(self.short(0x7fff_ffff, 0));
                out.extend(self.chunk(SETTINGS_TABLE, &settings));
            }
            let mut table = Vec::new();
            for (uuid, data) in objects {
                let mut class = self.chunk(CLASS_UUID, uuid);
                class.extend(self.chunk(CLASS_DATA, data));
                class.extend(self.short(0x0002_7fff, 0));
                let mut record = self.short(0x0200_0071, 32);
                record.extend(self.chunk(OPENNURBS_CLASS, &class));
                record.extend(self.short(0x0200_007f, 0));
                table.extend(self.chunk(OBJECT_RECORD, &record));
            }
            table.extend(self.short(0x7fff_ffff, 0));
            out.extend(self.chunk(OBJECT_TABLE, &table));
            let size = out.len() as u64 + if self.wide() { 20 } else { 12 };
            let body = if self.wide() {
                size.to_le_bytes().to_vec()
            } else {
                (size as u32).to_le_bytes().to_vec()
            };
            out.extend(self.chunk(ENDOFFILE, &body));
            out
        }
    }

    /// The CRC-32 3DM chunks end with (the zlib one).
    pub(crate) fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    pub(crate) fn cube() -> (Vec<[f32; 3]>, Vec<[u32; 4]>) {
        let v = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [10.0, 10.0, 10.0],
            [0.0, 10.0, 10.0],
        ];
        let f = vec![
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ];
        (v, f)
    }

    #[test]
    fn meshes_come_in_from_every_archive_version() {
        let (v, f) = cube();
        for version in [4, 50, 60, 70, 80] {
            let w = Writer { version };
            for compressed in [true, false] {
                let file = w.file(&[(ON_MESH, w.mesh(&v, &f, compressed))]);
                let read = read(&file).expect("reads");
                assert_eq!(read.meshes, 1, "{version}");
                assert_eq!(read.triangles.len(), 12, "{version} {compressed}");
                let top = read
                    .triangles
                    .iter()
                    .flatten()
                    .map(|p| p[2])
                    .fold(0.0, f64::max);
                assert_eq!(top, 10.0);
            }
        }
    }

    #[test]
    fn triangles_stay_triangles_and_other_objects_are_counted() {
        let w = Writer { version: 70 };
        let tri = w.mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2, 2]],
            true,
        );
        let brep = [0x11; 16];
        let file = w.file(&[(brep, vec![1, 2, 3]), (ON_MESH, tri)]);
        let read = read(&file).expect("reads");
        assert_eq!((read.meshes, read.skipped), (1, 1));
        assert_eq!(
            read.triangles,
            vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]
        );
    }

    #[test]
    fn a_polysurface_that_cannot_be_read_fails_the_whole_import() {
        let w = Writer { version: 70 };
        let tri = w.mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2, 2]],
            true,
        );
        for (class, what) in [
            (brep::ON_BREP, "polysurface"),
            (brep::ON_EXTRUSION, "extrusion"),
        ] {
            let file = w.file(&[(ON_MESH, tri.clone()), (class, vec![1, 2, 3])]);
            let error = read(&file).expect_err("refused").to_string();
            assert!(error.contains(what), "{error}");
            assert!(error.contains("nothing was imported"), "{error}");
        }
    }

    #[test]
    fn damaged_files_are_refused() {
        assert!(read(b"not a rhino file at all, not even close").is_err());
        let w = Writer { version: 60 };
        let (v, f) = cube();
        let file = w.file(&[(ON_MESH, w.mesh(&v, &f, true))]);
        // Cut inside the object table.
        assert!(read(&file[..file.len() / 2]).is_err());
        // A face pointing past the vertices.
        let bad_face = w.file(&[(ON_MESH, w.mesh(&v, &[[0, 1, 99, 99]], false))]);
        assert!(read(&bad_face).is_err());
        let mut header = MAGIC.to_vec();
        header.extend(b"       1");
        assert!(read(&header).is_err());
    }
}
