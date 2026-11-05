//! Finding and loading fonts.
//!
//! Faces come from two places: the fonts installed on the machine, and the
//! faces bundled with the crate: four outline faces as a fallback and the
//! single line (engraving) fonts. The bundled faces are always available, so
//! text layout never fails because nothing is installed.

use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, OnceLock};

use rustybuzz::ttf_parser;

use super::stroke::StrokeFont;
use crate::error::CoreError;

/// Regular face of the bundled fallback family.
const BUNDLED_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/FiraSans-Regular.ttf");
/// Bold face of the bundled fallback family.
const BUNDLED_BOLD: &[u8] = include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf");
/// Italic face of the bundled fallback family.
const BUNDLED_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/FiraSans-Italic.ttf");
/// Bold italic face of the bundled fallback family.
const BUNDLED_BOLD_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/FiraSans-BoldItalic.ttf");

/// The bundled single line fonts, in their text format.
const BUNDLED_SINGLE_LINE: [&str; 2] = [
    include_str!("../../../assets/fonts/SingleLineSans.txt"),
    include_str!("../../../assets/fonts/SingleLineScript.txt"),
];

/// The bundled single line fonts, read once.
fn single_line_fonts() -> &'static [Arc<StrokeFont>] {
    static FONTS: OnceLock<Vec<Arc<StrokeFont>>> = OnceLock::new();
    FONTS.get_or_init(|| {
        BUNDLED_SINGLE_LINE
            .iter()
            .filter_map(|source| StrokeFont::parse(source).ok().map(Arc::new))
            .collect()
    })
}

/// Where the bytes of a face come from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FaceSource {
    /// Compiled into the crate.
    Bundled(&'static [u8]),
    /// A file on the machine.
    File(PathBuf),
    /// A bundled single line font, by its place in the bundled list.
    SingleLine(usize),
}

/// One face the library knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FaceRecord {
    family: String,
    bold: bool,
    italic: bool,
    index: u32,
    source: FaceSource,
}

/// A face with its bytes in hand, ready to shape and outline with.
pub struct LoadedFont {
    family: String,
    bold: bool,
    italic: bool,
    index: u32,
    data: Arc<Vec<u8>>,
    stroke: Option<Arc<StrokeFont>>,
}

impl LoadedFont {
    /// Family name as the font itself reports it.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// True when this is a bold face.
    pub fn is_bold(&self) -> bool {
        self.bold
    }

    /// True when this is an italic or oblique face.
    pub fn is_italic(&self) -> bool {
        self.italic
    }

    /// The single line font, when this is one: its letters are open strokes
    /// rather than outlines, and it has no shaping face.
    pub fn stroke(&self) -> Option<&StrokeFont> {
        self.stroke.as_deref()
    }

    /// The shaping face, which also derefs to the outline parser.
    ///
    /// # Errors
    /// Returns [`CoreError::Font`] for a single line font or unreadable data.
    pub fn face(&self) -> Result<rustybuzz::Face<'_>, CoreError> {
        if self.stroke.is_some() {
            return Err(CoreError::font(format!(
                "{} is a single line font with no outlines",
                self.family
            )));
        }
        rustybuzz::Face::from_slice(&self.data, self.index)
            .ok_or_else(|| CoreError::font(format!("could not read the font {}", self.family)))
    }
}

/// The set of faces text layout can choose from.
#[derive(Debug, Clone, Default)]
pub struct FontLibrary {
    faces: Vec<FaceRecord>,
}

impl FontLibrary {
    /// Family name of the faces bundled with the crate.
    pub const FALLBACK_FAMILY: &'static str = "Fira Sans";

    /// Largest font file the scanner will open, in bytes.
    const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
    /// Most files the scanner will look at, so a strange directory tree
    /// cannot make startup take forever.
    const MAX_FILES: usize = 4000;

    /// A library holding only the bundled faces: the four fallback faces and
    /// the single line fonts.
    ///
    /// Use this where the result has to be the same on every machine, which
    /// is what the tests need.
    pub fn bundled() -> FontLibrary {
        let mut faces = Vec::with_capacity(4);
        for (data, bold, italic) in [
            (BUNDLED_REGULAR, false, false),
            (BUNDLED_BOLD, true, false),
            (BUNDLED_ITALIC, false, true),
            (BUNDLED_BOLD_ITALIC, true, true),
        ] {
            let family = family_of(data, 0).unwrap_or_else(|| Self::FALLBACK_FAMILY.to_string());
            faces.push(FaceRecord {
                family,
                bold,
                italic,
                index: 0,
                source: FaceSource::Bundled(data),
            });
        }
        for (index, font) in single_line_fonts().iter().enumerate() {
            faces.push(FaceRecord {
                family: font.family().to_string(),
                bold: false,
                italic: false,
                index: 0,
                source: FaceSource::SingleLine(index),
            });
        }
        let mut library = FontLibrary { faces };
        library.sort();
        library
    }

    /// The bundled faces plus every face installed on the machine.
    ///
    /// Reading the installed fonts takes a moment, so build this once and
    /// keep it. Which families turn up depends on the machine, so anything
    /// that has to be reproducible should name a family that is bundled.
    pub fn system() -> FontLibrary {
        let mut library = FontLibrary::bundled();
        let mut seen = 0usize;
        for dir in system_font_dirs() {
            collect_dir(&dir, 0, &mut seen, &mut library.faces);
        }
        library.sort();
        library
    }

    /// Adds every font file in one directory tree.
    pub fn add_directory(&mut self, dir: &FsPath) {
        let mut seen = 0usize;
        collect_dir(dir, 0, &mut seen, &mut self.faces);
        self.sort();
    }

    fn sort(&mut self) {
        self.faces.sort_by(|a, b| {
            a.family
                .to_lowercase()
                .cmp(&b.family.to_lowercase())
                .then(a.bold.cmp(&b.bold))
                .then(a.italic.cmp(&b.italic))
                .then(a.index.cmp(&b.index))
        });
        self.faces.dedup_by(|a, b| {
            a.family == b.family
                && a.bold == b.bold
                && a.italic == b.italic
                && a.source == b.source
                && a.index == b.index
        });
    }

    /// Every family name the library holds, sorted and without repeats.
    /// Nameless faces and the system's private faces (macOS names them with
    /// a leading dot) are left out, since nobody picks those from a list.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .faces
            .iter()
            .map(|f| f.family.trim().to_string())
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// The families that are single line (engraving) fonts, sorted.
    pub fn single_line_families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .faces
            .iter()
            .filter(|f| matches!(f.source, FaceSource::SingleLine(_)))
            .map(|f| f.family.clone())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// Number of faces in the library.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// True when the library holds no faces at all.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Loads the closest face to what was asked for.
    ///
    /// The family is matched without regard to case. When the family is there
    /// but the weight or slant is not, another face of the same family is
    /// used. When the family is not there at all, the bundled fallback family
    /// is used.
    pub fn load(&self, family: &str, bold: bool, italic: bool) -> Result<LoadedFont, CoreError> {
        let wanted = family.trim().to_lowercase();
        let record = self
            .pick(&wanted, bold, italic)
            .or_else(|| self.pick(&Self::FALLBACK_FAMILY.to_lowercase(), bold, italic))
            .or_else(|| self.faces.first())
            .ok_or_else(|| CoreError::font("no fonts are available"))?;
        let (data, stroke) =
            match &record.source {
                FaceSource::Bundled(bytes) => (Arc::new(bytes.to_vec()), None),
                FaceSource::File(path) => (
                    Arc::new(std::fs::read(path).map_err(|e| {
                        CoreError::font(format!("could not read the font file: {e}"))
                    })?),
                    None,
                ),
                FaceSource::SingleLine(index) => {
                    let font = single_line_fonts()
                        .get(*index)
                        .cloned()
                        .ok_or_else(|| CoreError::font("the single line font is missing"))?;
                    (Arc::new(Vec::new()), Some(font))
                }
            };
        Ok(LoadedFont {
            family: record.family.clone(),
            bold: record.bold,
            italic: record.italic,
            index: record.index,
            data,
            stroke,
        })
    }

    fn pick(&self, family_lower: &str, bold: bool, italic: bool) -> Option<&FaceRecord> {
        let same_family: Vec<&FaceRecord> = self
            .faces
            .iter()
            .filter(|f| f.family.to_lowercase() == family_lower)
            .collect();
        if same_family.is_empty() {
            return None;
        }
        same_family
            .iter()
            .find(|f| f.bold == bold && f.italic == italic)
            .or_else(|| same_family.iter().find(|f| f.italic == italic))
            .or_else(|| same_family.iter().find(|f| f.bold == bold))
            .or_else(|| same_family.first())
            .copied()
    }
}

/// Family name of a face, preferring the typographic family when there is one.
fn family_of(data: &[u8], index: u32) -> Option<String> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let mut fallback = None;
    for name in face.names() {
        if !name.is_unicode() {
            continue;
        }
        match name.name_id {
            ttf_parser::name_id::TYPOGRAPHIC_FAMILY => {
                if let Some(value) = name.to_string() {
                    return Some(value);
                }
            }
            ttf_parser::name_id::FAMILY if fallback.is_none() => {
                fallback = name.to_string();
            }
            _ => {}
        }
    }
    fallback
}

/// The directories fonts are installed in on this platform.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Library").join("Fonts"));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(root) = std::env::var_os("SystemRoot") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(&home).join(".fonts"));
            dirs.push(
                PathBuf::from(&home)
                    .join(".local")
                    .join("share")
                    .join("fonts"),
            );
        }
    }
    dirs
}

fn collect_dir(dir: &FsPath, depth: u32, seen: &mut usize, out: &mut Vec<FaceRecord>) {
    const MAX_DEPTH: u32 = 4;
    if depth > MAX_DEPTH || *seen >= FontLibrary::MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if *seen >= FontLibrary::MAX_FILES {
            return;
        }
        if path.is_dir() {
            collect_dir(&path, depth + 1, seen, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_lowercase();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.len() > FontLibrary::MAX_FILE_BYTES {
            continue;
        }
        *seen += 1;
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        for index in 0..count {
            let Ok(face) = ttf_parser::Face::parse(&data, index) else {
                continue;
            };
            let Some(family) = family_of(&data, index) else {
                continue;
            };
            out.push(FaceRecord {
                family,
                bold: face.is_bold(),
                italic: face.is_italic() || face.is_oblique(),
                index,
                source: FaceSource::File(path.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_library_has_four_faces_and_the_single_line_fonts() {
        let library = FontLibrary::bundled();
        assert_eq!(library.len(), 6);
        assert!(!library.is_empty());
        assert_eq!(
            library.families(),
            vec![
                FontLibrary::FALLBACK_FAMILY,
                "Single Line Sans",
                "Single Line Script"
            ]
        );
        assert_eq!(
            library.single_line_families(),
            vec!["Single Line Sans", "Single Line Script"]
        );
    }

    #[test]
    fn every_bundled_single_line_font_reads() {
        assert_eq!(single_line_fonts().len(), BUNDLED_SINGLE_LINE.len());
        let library = FontLibrary::bundled();
        for family in library.single_line_families() {
            let font = library.load(&family, true, true).expect("font");
            let stroke = font.stroke().expect("single line");
            assert_eq!(stroke.family(), family);
            assert!(font.face().is_err());
            // Every printable ASCII character has its own glyph.
            for ch in ' '..='~' {
                assert!(stroke.glyph(ch).is_some(), "{family} lacks {ch}");
            }
        }
    }

    #[test]
    fn hidden_and_nameless_families_are_not_listed() {
        let mut library = FontLibrary::bundled();
        let bundled = library.faces[0].source.clone();
        for family in ["", ".Hidden UI", "  Visible  "] {
            library.faces.push(FaceRecord {
                family: family.to_string(),
                bold: false,
                italic: false,
                index: 0,
                source: bundled.clone(),
            });
        }
        let families = library.families();
        assert!(families.contains(&"Visible".to_string()), "{families:?}");
        assert!(families
            .iter()
            .all(|f| !f.is_empty() && !f.starts_with('.')));
    }

    #[test]
    fn the_bundled_faces_load_and_parse() {
        let library = FontLibrary::bundled();
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let font = library
                .load(FontLibrary::FALLBACK_FAMILY, bold, italic)
                .expect("font");
            assert_eq!(font.is_bold(), bold);
            assert_eq!(font.is_italic(), italic);
            let face = font.face().expect("face");
            assert!(face.units_per_em() > 0);
        }
    }

    #[test]
    fn an_unknown_family_falls_back_to_the_bundled_one() {
        let library = FontLibrary::bundled();
        let font = library
            .load("A Font Nobody Has Installed", false, false)
            .expect("font");
        assert_eq!(font.family(), FontLibrary::FALLBACK_FAMILY);
    }

    #[test]
    fn family_matching_ignores_case_and_padding() {
        let library = FontLibrary::bundled();
        let font = library.load("  fira sans  ", false, false).expect("font");
        assert_eq!(font.family(), FontLibrary::FALLBACK_FAMILY);
    }

    #[test]
    fn an_empty_library_reports_no_fonts() {
        let library = FontLibrary::default();
        assert!(library.is_empty());
        assert!(library.load("anything", false, false).is_err());
        assert!(library.families().is_empty());
    }

    #[test]
    fn system_font_directories_are_listed() {
        assert!(!system_font_dirs().is_empty());
    }

    #[test]
    fn adding_a_missing_directory_is_harmless() {
        let mut library = FontLibrary::bundled();
        library.add_directory(FsPath::new("/this/path/does/not/exist"));
        assert_eq!(library.len(), 6);
    }
}
