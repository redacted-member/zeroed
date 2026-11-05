//! DWG import: AutoCAD's native drawing files, R13 to 2018 and later.
//!
//! The reader is this crate's own, written from the Open Design Alliance's
//! public "Open Design Specification for .dwg files": `file.rs` splits a
//! file into its sections for each release (the flat R13 to R2000 layout,
//! the paged and compressed R2004 layout that R2010 to R2018 share, and
//! the Reed-Solomon coded R2007 layout), `header.rs` reads `$INSUNITS`, the
//! class list and the object map, and `objects.rs` reads the entities, the
//! layers and the block definitions. `walk.rs` then turns model space into
//! vectors the way the DXF importer does: LINE, ARC, CIRCLE, ELLIPSE (whole
//! or partial), LWPOLYLINE and 2D POLYLINE with bulges, 3D polylines
//! flattened, SPLINE (exactly, through [`super::nurbs`]) and INSERT,
//! exploding blocks (and MINSERT arrays) as it goes. Entities keep their
//! layer; block contents on layer 0 take the layer of the reference that
//! places them, as CAD programs draw them. Arcs, circles and polylines drawn
//! with their extrusion pointing down (mirrored in plan) are turned back
//! over. Lengths follow `$INSUNITS`. Text is reported and skipped, like
//! images. Damaged files end as errors: every read is bounds checked and
//! every size a file states is checked against what the file could hold.

use std::path::Path as FsPath;

mod bits;
mod file;
mod header;
mod helper;
mod objects;
mod r2004;
mod r2007;
mod walk;

use file::{Bad, Sections, Version};
pub use helper::{run_helper, set_helper, Helper, HELPER_FLAG};
pub use objects::Drawing;

use super::{ImportWarning, ImportedPath};
use crate::error::CoreError;
use crate::geom::{transform_path, Mat3};

/// Millimetres per drawing unit for an `$INSUNITS` code; unitless and
/// unknown codes are taken as millimetres.
pub fn insunits_to_mm(code: i16) -> f64 {
    match code {
        1 => 25.4,
        2 => 304.8,
        3 => 1_609_344.0,
        4 => 1.0,
        5 => 10.0,
        6 => 1000.0,
        7 => 1_000_000.0,
        8 => 2.54e-5,
        9 => 0.0254,
        10 => 914.4,
        13 => 0.001,
        14 => 100.0,
        _ => 1.0,
    }
}

/// True when the bytes start like a DWG file (`AC10..` version tag).
pub fn looks_like_dwg(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && &bytes[..4] == b"AC10" && bytes[4..6].iter().all(u8::is_ascii_digit)
}

/// Reads vectors out of a DWG file.
///
/// `scale` multiplies every length after `$INSUNITS` handling; a
/// `units_hint` of `mm` or `inch` overrides what the file says.
///
/// # Errors
/// [`CoreError::Io`] when the file cannot be read and [`CoreError::Format`]
/// when it is not a DWG file this reader understands.
///
/// With a helper set by [`set_helper`], the file is decoded in that child
/// process, so a decoder fault that would abort the process (an allocation
/// a damaged file asks for) or a decode that runs too long ends as an error
/// here instead.
pub fn import(
    path: &FsPath,
    scale: f64,
    units_hint: Option<&str>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    import_with(path, scale, units_hint, helper::current().as_ref())
}

/// [`import`] through `helper`, or in this process when it is `None`.
///
/// # Errors
/// As [`import`].
pub fn import_with(
    path: &FsPath,
    scale: f64,
    units_hint: Option<&str>,
    helper: Option<&Helper>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path.display(), e))?;
    if !looks_like_dwg(&bytes) {
        return Err(CoreError::format("DWG", "this is not a DWG file"));
    }
    match helper {
        Some(helper) => helper.decode(path, scale, units_hint),
        None => {
            let drawing = read(&bytes)?;
            Ok(vectors(&drawing, scale, units_hint))
        }
    }
}

/// Decodes the bytes of a DWG file.
///
/// # Errors
/// [`CoreError::Format`] when they are not a DWG file this reader
/// understands.
pub fn read(bytes: &[u8]) -> Result<Drawing, CoreError> {
    if !looks_like_dwg(bytes) {
        return Err(CoreError::format("DWG", "this is not a DWG file"));
    }
    let bad = |b: Bad| CoreError::format("DWG", b.0);
    let sections = file::sections(bytes).map_err(bad)?;
    objects::drawing(&sections).map_err(bad)
}

/// The vectors of a decoded drawing's model space, in millimetres.
pub fn vectors(
    drawing: &Drawing,
    scale: f64,
    units_hint: Option<&str>,
) -> (Vec<ImportedPath>, Vec<ImportWarning>) {
    let per_unit = match units_hint {
        Some("mm") => 1.0,
        Some("inch" | "in") => 25.4,
        _ => insunits_to_mm(drawing.insunits()),
    };
    let scale = if scale.is_finite() && scale != 0.0 {
        scale
    } else {
        1.0
    };
    let factor = per_unit * scale;
    let (mut out, mut warnings) = walk::paths(drawing);
    if factor != 1.0 {
        let m = Mat3::scale(factor, factor);
        for imported in &mut out {
            imported.path = transform_path(&imported.path, &m);
        }
    }
    warnings.sort_by_key(|w| format!("{w:?}"));
    warnings.dedup();
    (out, warnings)
}

#[cfg(test)]
mod tests;
