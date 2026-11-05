//! The clipart library that ships with the app.
//!
//! Most items are made by code in this module: reliefs from formulas and
//! our own modeling functions, vectors from geometry and from outlines
//! drawn point by point for this library. The rest are converted from
//! outside art that is free to ship (CC0, the Public Domain Mark or CC BY
//! with its author credited), see [`imported`].
//! `crates/carve-core/assets/clipart/MANIFEST.json` records each item with
//! its source and license, and a test keeps it in step with the catalogue.
//!
//! The catalogue is listed without touching the disk. An item's file is
//! written into the versioned library folder the first time it is drawn or
//! inserted (see [`materialize`]), so the first scan is instant and the
//! folder only holds what was used.

pub mod art;
mod borders;
mod floral;
mod frames;
mod geometric;
pub mod imported;
mod letters;
mod profiles;
mod rosettes;
mod scrolls;
mod shapes;
mod signs;
mod silhouettes;
mod stars;
mod textures;

use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::OnceLock;

use crate::error::CoreError;
use crate::geom::{flatten_path, BBox, Path};
use crate::io::svg::{export, ExportLayer};
use crate::model::field::Field;

use super::clip_file::{self, Clip};
use super::scan::{ClipFormat, ClipItem, ClipKind};

/// Version of the library. The folder the items are written to is named
/// after it, so a changed library is written afresh.
pub const LIBRARY_VERSION: u32 = 4;

/// Start of the library folder's name; the version follows.
pub const DIR_PREFIX: &str = "clipart-library-";

/// Author recorded for every item.
pub const AUTHOR: &str = "CarveStudio contributors";

/// License recorded for items made wholly by our code: the project's own.
pub const LICENSE: &str = "MIT";

/// License recorded for letters, whose outlines are glyphs of the bundled
/// Fira Sans faces (see `assets/fonts/README.md`).
pub const FONT_LICENSE: &str = "OFL-1.1 (Fira Sans glyphs)";

/// Name of the library folder for this version.
pub fn dir_name() -> String {
    format!("{DIR_PREFIX}{LIBRARY_VERSION}")
}

/// What an item holds once built.
#[derive(Debug, Clone)]
pub enum Art {
    /// A relief component.
    Relief(Field),
    /// Vectors, millimetres.
    Vectors(Vec<Path>),
}

type Builder = Box<dyn Fn() -> Result<Art, CoreError> + Send + Sync>;

/// One item of the library.
pub struct Entry {
    /// Category, `/` between levels.
    pub category: String,
    /// Name shown under the thumbnail, unique in its category.
    pub name: String,
    /// Words a search matches besides the name and category.
    pub tags: Vec<String>,
    /// What it inserts as.
    pub kind: ClipKind,
    /// The generator file under `clipart/library/` that makes it.
    pub source: &'static str,
    /// Vectors meant to stay open lines (profiles, engraving line art),
    /// exempt from the closed shape checks.
    pub line_art: bool,
    /// Where an item converted from outside art came from; `None` for
    /// items made by our code.
    pub origin: Option<&'static imported::Origin>,
    build: Builder,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("category", &self.category)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Entry {
    /// A relief item.
    pub fn relief(
        category: &str,
        name: &str,
        tags: &[&str],
        source: &'static str,
        build: impl Fn() -> Result<Field, CoreError> + Send + Sync + 'static,
    ) -> Entry {
        Entry {
            category: category.to_string(),
            name: name.to_string(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            kind: ClipKind::Relief,
            source,
            line_art: false,
            origin: None,
            // Every relief gets a light final smoothing, so no one cell
            // spike or crease is left for the finishing cutter.
            build: Box::new(move || build().map(|f| Art::Relief(art::soften(f, 2)))),
        }
    }

    /// A relief tile that repeats without a seam, for texturing.
    pub fn tile(
        category: &str,
        name: &str,
        tags: &[&str],
        source: &'static str,
        build: impl Fn() -> Result<Field, CoreError> + Send + Sync + 'static,
    ) -> Entry {
        Entry {
            category: category.to_string(),
            name: name.to_string(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            kind: ClipKind::Relief,
            source,
            line_art: false,
            origin: None,
            build: Box::new(move || build().map(|f| Art::Relief(art::soften_tile(f, 2)))),
        }
    }

    /// A vector item.
    pub fn vectors(
        category: &str,
        name: &str,
        tags: &[&str],
        source: &'static str,
        build: impl Fn() -> Result<Vec<Path>, CoreError> + Send + Sync + 'static,
    ) -> Entry {
        Entry {
            category: category.to_string(),
            name: name.to_string(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            kind: ClipKind::Vector,
            source,
            line_art: false,
            origin: None,
            build: Box::new(move || build().map(Art::Vectors)),
        }
    }

    /// The license the item is shipped under.
    pub fn license(&self) -> &'static str {
        if let Some(origin) = self.origin {
            origin.license
        } else if self.source == "letters.rs" {
            FONT_LICENSE
        } else {
            LICENSE
        }
    }

    /// The file format the item is written in.
    pub fn format(&self) -> ClipFormat {
        match self.kind {
            ClipKind::Relief => ClipFormat::Clip,
            ClipKind::Vector => ClipFormat::Vectors,
        }
    }

    /// The item's file under the library folder.
    pub fn relative_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for part in self.category.split('/') {
            path.push(part);
        }
        let extension = match self.kind {
            ClipKind::Relief => clip_file::EXTENSION,
            ClipKind::Vector => "svg",
        };
        path.push(format!("{}.{extension}", self.name));
        path
    }

    /// Makes the item.
    ///
    /// # Errors
    /// The generator's errors.
    pub fn build(&self) -> Result<Art, CoreError> {
        (self.build)()
    }
}

/// Every item, in the order the library lists them.
pub fn catalogue() -> &'static [Entry] {
    static CATALOGUE: OnceLock<Vec<Entry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        let mut out = Vec::new();
        shapes::entries(&mut out);
        rosettes::entries(&mut out);
        stars::entries(&mut out);
        borders::entries(&mut out);
        frames::entries(&mut out);
        profiles::entries(&mut out);
        floral::entries(&mut out);
        scrolls::entries(&mut out);
        geometric::entries(&mut out);
        textures::entries(&mut out);
        signs::entries(&mut out);
        letters::entries(&mut out);
        silhouettes::entries(&mut out);
        imported::entries(&mut out);
        out
    })
}

fn by_path() -> &'static BTreeMap<PathBuf, usize> {
    static INDEX: OnceLock<BTreeMap<PathBuf, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        catalogue()
            .iter()
            .enumerate()
            .map(|(k, e)| (e.relative_path(), k))
            .collect()
    })
}

/// The library's items as the scan lists them, their files under `dir`.
pub fn items(dir: &FsPath) -> Vec<ClipItem> {
    catalogue()
        .iter()
        .map(|entry| ClipItem {
            name: entry.name.clone(),
            path: dir
                .join(entry.relative_path())
                .to_string_lossy()
                .into_owned(),
            category: entry.category.clone(),
            kind: entry.kind,
            format: entry.format(),
            tags: entry.tags.clone(),
        })
        .collect()
}

/// The catalogue entry a path in a library folder stands for, if any: the
/// path must run through a folder named [`dir_name`].
pub fn entry_for(path: &FsPath) -> Option<&'static Entry> {
    let name = dir_name();
    let mut rest = PathBuf::new();
    let mut found = false;
    for part in path.components() {
        if found {
            rest.push(part);
        } else if part.as_os_str() == name.as_str() {
            found = true;
        }
    }
    if !found {
        return None;
    }
    by_path().get(&rest).map(|&k| &catalogue()[k])
}

/// Writes the library item at `path` when it is one and its file is not
/// there yet. Anything else is left alone.
///
/// # Errors
/// The generator's errors and [`CoreError::Io`] when the file cannot be
/// written.
pub fn materialize(path: &FsPath) -> Result<(), CoreError> {
    if path.is_file() {
        return Ok(());
    }
    let Some(entry) = entry_for(path) else {
        return Ok(());
    };
    write(entry, path)
}

/// Builds `entry` and writes it to `path`, aside first then renamed, so a
/// reader never meets a half written file.
///
/// # Errors
/// As [`materialize`].
pub fn write(entry: &Entry, path: &FsPath) -> Result<(), CoreError> {
    let dir = path
        .parent()
        .ok_or_else(|| CoreError::validation("the item has no folder"))?;
    std::fs::create_dir_all(dir).map_err(|error| CoreError::io(dir.display(), error))?;
    let partial = dir.join(format!(".{}.{}.partial", entry.name, std::process::id()));
    match entry.build()? {
        Art::Relief(field) => clip_file::write(
            &Clip {
                name: entry.name.clone(),
                field,
            },
            &partial,
        )?,
        Art::Vectors(paths) => {
            let text = svg_text(&entry.name, &paths)?;
            std::fs::write(&partial, text)
                .map_err(|error| CoreError::io(partial.display(), error))?;
        }
    }
    std::fs::rename(&partial, path).map_err(|error| CoreError::io(path.display(), error))
}

/// Vectors as an SVG document in millimetres with a small margin, the
/// drawing the right way up.
///
/// # Errors
/// [`CoreError::Validation`] for vectors with no extent.
pub fn svg_text(name: &str, paths: &[Path]) -> Result<String, CoreError> {
    let bbox = paths
        .iter()
        .flat_map(|p| flatten_path(p, art::tol()))
        .filter_map(|l| BBox::of_points(l.points))
        .reduce(BBox::union)
        .ok_or_else(|| CoreError::validation("the item has no vectors"))?;
    let margin = 1.0;
    let bounds = BBox::of_points([
        art::p(bbox.min.x - margin, bbox.min.y - margin),
        art::p(bbox.max.x + margin, bbox.max.y + margin),
    ])
    .ok_or_else(|| CoreError::validation("the item has no vectors"))?;
    let layer = ExportLayer {
        name: name.to_string(),
        colour: "#000000".to_string(),
        paths: paths.to_vec(),
    };
    Ok(export(&[layer], bounds))
}

/// Registers vector items that share a category, tags and source.
pub(crate) struct Adder<'a> {
    pub out: &'a mut Vec<Entry>,
    pub category: &'static str,
    pub source: &'static str,
}

impl Adder<'_> {
    /// Adds a vector item.
    pub fn vectors(
        &mut self,
        name: &str,
        tags: &[&str],
        build: impl Fn() -> Result<Vec<Path>, CoreError> + Send + Sync + 'static,
    ) {
        self.out.push(Entry::vectors(
            self.category,
            name,
            tags,
            self.source,
            build,
        ));
    }

    /// Adds a vector item made of open lines on purpose.
    pub fn line_art(
        &mut self,
        name: &str,
        tags: &[&str],
        build: impl Fn() -> Result<Vec<Path>, CoreError> + Send + Sync + 'static,
    ) {
        let mut entry = Entry::vectors(self.category, name, tags, self.source, build);
        entry.line_art = true;
        self.out.push(entry);
    }

    /// Adds a relief tile that repeats without a seam.
    pub fn tile(
        &mut self,
        name: &str,
        tags: &[&str],
        build: impl Fn() -> Result<Field, CoreError> + Send + Sync + 'static,
    ) {
        self.out
            .push(Entry::tile(self.category, name, tags, self.source, build));
    }

    /// Adds a relief item.
    pub fn relief(
        &mut self,
        name: &str,
        tags: &[&str],
        build: impl Fn() -> Result<Field, CoreError> + Send + Sync + 'static,
    ) {
        self.out
            .push(Entry::relief(self.category, name, tags, self.source, build));
    }
}

#[cfg(test)]
mod tests;
