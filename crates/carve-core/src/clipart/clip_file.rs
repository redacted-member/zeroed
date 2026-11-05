//! The `.carveclip` file: one relief component on its own, for the clipart
//! library.
//!
//! A `.carveclip` file is a zip archive holding
//! - `clip.json`: `{format, kind: "relief", name, width, height, cell}`,
//!   the grid size in cells and the cell size in millimetres,
//! - `heights.f32`: the heights, little endian f32 per cell, rows from the
//!   lowest Y up, NaN where there is no material.
//!
//! The heights are millimetres above the zero plane with the component's
//! height scale already applied, so a clip inserts at the size it was saved.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::CoreError;
use crate::geom::Point;
use crate::model::field::Field;

/// Extension of a clip file, without the dot.
pub const EXTENSION: &str = "carveclip";
/// Format version written into `clip.json`.
pub const CLIP_FORMAT: u32 = 1;
/// Entry holding the clip's description.
const CLIP_ENTRY: &str = "clip.json";
/// Entry holding the heights.
const HEIGHTS_ENTRY: &str = "heights.f32";
/// Most cells a clip may hold, so a damaged file cannot ask for more memory
/// than a machine has.
pub const MAX_CLIP_CELLS: usize = 16_000_000;

/// One relief clip: a name and its heights.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Name the component gets when the clip is inserted.
    pub name: String,
    /// The heights, cropped to the cells that hold material. The origin is
    /// not saved; a read clip has its first cell centre at the origin.
    pub field: Field,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipHeader {
    format: u32,
    kind: String,
    name: String,
    width: usize,
    height: usize,
    cell: f64,
}

/// Writes `clip` to `path`.
///
/// # Errors
/// [`CoreError::Validation`] for a clip with no cells or a bad cell size,
/// [`CoreError::Io`] when the file cannot be written.
pub fn write(clip: &Clip, path: &FsPath) -> Result<(), CoreError> {
    let field = &clip.field;
    if field.width == 0 || field.height == 0 || field.z.len() != field.width * field.height {
        return Err(CoreError::validation("the clip has no heights to save"));
    }
    if !(field.cell.is_finite() && field.cell > 0.0) {
        return Err(CoreError::validation(
            "the clip's cell size must be above zero",
        ));
    }
    let header = ClipHeader {
        format: CLIP_FORMAT,
        kind: "relief".to_string(),
        name: clip.name.clone(),
        width: field.width,
        height: field.height,
        cell: field.cell,
    };
    let io_error = |error: &dyn std::fmt::Display| CoreError::io(path.display(), error);
    let file = File::create(path).map_err(|e| io_error(&e))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(CLIP_ENTRY, options)
        .map_err(|e| io_error(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&header)?)
        .map_err(|e| io_error(&e))?;
    zip.start_file(HEIGHTS_ENTRY, options)
        .map_err(|e| io_error(&e))?;
    zip.write_all(&field.z_bytes()).map_err(|e| io_error(&e))?;
    zip.finish().map_err(|e| io_error(&e))?;
    Ok(())
}

/// Reads a clip written by [`write`].
///
/// # Errors
/// [`CoreError::Io`] when the file cannot be read, [`CoreError::Validation`]
/// when it is not a clip file or is damaged.
pub fn read(path: &FsPath) -> Result<Clip, CoreError> {
    let not_clip = || CoreError::validation(format!("{} is not a clip file", path.display()));
    let file = File::open(path).map_err(|error| CoreError::io(path.display(), error))?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|_| not_clip())?;
    let mut text = String::new();
    zip.by_name(CLIP_ENTRY)
        .map_err(|_| not_clip())?
        .read_to_string(&mut text)
        .map_err(|error| CoreError::io(path.display(), error))?;
    let header: ClipHeader = serde_json::from_str(&text).map_err(|_| not_clip())?;
    if header.format > CLIP_FORMAT {
        return Err(CoreError::unsupported(
            "this clip was saved by a newer version; update to insert it",
        ));
    }
    if header.kind != "relief" {
        return Err(CoreError::unsupported(format!(
            "clips of kind {} are not supported",
            header.kind
        )));
    }
    let cells = header.width.saturating_mul(header.height);
    if cells == 0 || cells > MAX_CLIP_CELLS || !(header.cell.is_finite() && header.cell > 0.0) {
        return Err(CoreError::validation("the clip's size is damaged"));
    }
    let mut bytes = Vec::with_capacity(cells * 4);
    zip.by_name(HEIGHTS_ENTRY)
        .map_err(|_| CoreError::validation("the clip has no heights in it"))?
        .read_to_end(&mut bytes)
        .map_err(|error| CoreError::io(path.display(), error))?;
    let mut field = Field::empty(header.width, header.height, header.cell, Point::ORIGIN);
    field.set_z_bytes(&bytes)?;
    Ok(Clip {
        name: header.name,
        field,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn a_clip_survives_a_round_trip() {
        let temp = TempDir::new("clip-round-trip");
        let path = temp.path().join("bump.carveclip");
        let mut field = Field::empty(3, 2, 0.5, Point::ORIGIN);
        field.z = vec![0.0, 1.5, f32::NAN, 2.0, 3.25, 0.5];
        let clip = Clip {
            name: "Bump".to_string(),
            field,
        };
        write(&clip, &path).expect("writes");
        let back = read(&path).expect("reads");
        assert_eq!(back, clip);
    }

    #[test]
    fn a_file_that_is_not_a_clip_is_refused() {
        let temp = TempDir::new("clip-not");
        let path = temp.path().join("bad.carveclip");
        std::fs::write(&path, b"hello").expect("writes");
        let error = read(&path).expect_err("refused");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn an_empty_clip_is_not_written() {
        let temp = TempDir::new("clip-empty");
        let clip = Clip {
            name: "Nothing".to_string(),
            field: Field::empty(0, 0, 1.0, Point::ORIGIN),
        };
        assert!(write(&clip, &temp.path().join("x.carveclip")).is_err());
    }
}
