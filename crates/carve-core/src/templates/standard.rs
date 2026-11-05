//! The standard sheet templates: common sheet goods at their usual sizes
//! and thicknesses, made here so the template list is never empty. They are
//! written into their own folder the first time templates are listed, the
//! way the clipart samples are.

use std::path::{Path as FsPath, PathBuf};

use crate::doc::{Document, JobSetup, Units};
use crate::error::CoreError;
use crate::io::native;

/// Version of the standard templates. Their folder is named after it, so
/// changed templates are written afresh instead of reusing old files.
pub const STANDARD_VERSION: u32 = 1;

/// Category the standard templates are listed under.
pub const STANDARD_CATEGORY: &str = "Standard";

/// Millimetres per inch.
const MM_PER_INCH: f64 = 25.4;

/// One standard sheet, sized in its own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardSheet {
    /// Template name, also its file name.
    pub name: &'static str,
    /// What the sheet is, for the template's notes.
    pub material: &'static str,
    /// Units the sizes below are in, and the job's display units.
    pub units: Units,
    /// Width along X.
    pub width: f64,
    /// Height along Y.
    pub height: f64,
    /// Material thickness.
    pub thickness: f64,
}

/// The standard sheets, in the order the list shows them.
pub const STANDARD_SHEETS: [StandardSheet; 5] = [
    StandardSheet {
        name: "Plywood 2440 x 1220 x 18 mm",
        material: "Plywood",
        units: Units::Mm,
        width: 2440.0,
        height: 1220.0,
        thickness: 18.0,
    },
    StandardSheet {
        name: "MDF 1220 x 610 x 12 mm",
        material: "MDF",
        units: Units::Mm,
        width: 1220.0,
        height: 610.0,
        thickness: 12.0,
    },
    StandardSheet {
        name: "Hardwood 600 x 400 x 19 mm",
        material: "Hardwood",
        units: Units::Mm,
        width: 600.0,
        height: 400.0,
        thickness: 19.0,
    },
    StandardSheet {
        name: "Plywood 48 x 96 x 0.75 in",
        material: "Plywood",
        units: Units::Inch,
        width: 48.0,
        height: 96.0,
        thickness: 0.75,
    },
    StandardSheet {
        name: "Sign Blank 24 x 18 x 0.5 in",
        material: "Sign blank",
        units: Units::Inch,
        width: 24.0,
        height: 18.0,
        thickness: 0.5,
    },
];

impl StandardSheet {
    /// The job setup of this sheet: its size in millimetres, Z zero on top
    /// and the datum at the lower left corner.
    pub fn job(&self) -> JobSetup {
        let factor = match self.units {
            Units::Mm => 1.0,
            Units::Inch => MM_PER_INCH,
        };
        JobSetup {
            units: self.units,
            width: self.width * factor,
            height: self.height * factor,
            thickness: self.thickness * factor,
            ..JobSetup::default()
        }
    }

    /// A new document set up for this sheet.
    ///
    /// # Errors
    /// [`CoreError::Validation`] if the sheet's job is not machinable,
    /// which the tests rule out for the shipped sheets.
    pub fn document(&self) -> Result<Document, CoreError> {
        let mut doc = Document::new();
        doc.set_job(self.job())?;
        doc.set_notes(format!("{} sheet, {}.", self.material, self.name));
        Ok(doc)
    }
}

/// Writes every standard template missing from `dir` and answers the paths
/// of all of them, in [`STANDARD_SHEETS`] order.
///
/// # Errors
/// [`CoreError::Io`] when the folder or a file cannot be written.
pub fn ensure(dir: &FsPath) -> Result<Vec<PathBuf>, CoreError> {
    std::fs::create_dir_all(dir).map_err(|error| CoreError::io(dir.display(), error))?;
    let mut paths = Vec::with_capacity(STANDARD_SHEETS.len());
    for sheet in &STANDARD_SHEETS {
        let path = dir.join(format!("{}.{}", sheet.name, native::EXTENSION));
        if !path.is_file() {
            // Written aside and renamed, so a listing running at the same
            // time never reads a half written file.
            native::save_copy(&sheet.document()?, &path)?;
        }
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn every_sheet_is_a_valid_job() {
        for sheet in &STANDARD_SHEETS {
            sheet.job().validate().expect("machinable");
            assert!(sheet.document().is_ok(), "{}", sheet.name);
        }
    }

    #[test]
    fn inch_sheets_are_stored_in_millimetres() {
        let job = STANDARD_SHEETS[3].job();
        assert_eq!(job.units, Units::Inch);
        assert!((job.width - 1219.2).abs() < 1e-9);
        assert!((job.height - 2438.4).abs() < 1e-9);
        assert!((job.thickness - 19.05).abs() < 1e-9);
    }

    #[test]
    fn ensure_writes_each_sheet_once() {
        let temp = TempDir::new("templates-standard");
        let dir = temp.path().join("standard");
        let paths = ensure(&dir).expect("written");
        assert_eq!(paths.len(), STANDARD_SHEETS.len());
        for (path, sheet) in paths.iter().zip(&STANDARD_SHEETS) {
            let doc = native::open(path, 0).expect("opens");
            assert_eq!(doc.job_setup(), &sheet.job());
        }
        // A second call keeps the files it finds.
        std::fs::write(&paths[0], b"kept").expect("overwrite");
        ensure(&dir).expect("again");
        assert_eq!(std::fs::read(&paths[0]).expect("read"), b"kept");
    }
}
