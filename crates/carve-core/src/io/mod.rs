//! File input and output: the native document format and the importers and
//! exporters for interchange formats.
//!
//! Every importer answers with the same shape, [`ImportedPath`], so the
//! document only has one way of taking vectors in whatever the file was.
//! Anything an importer could not bring across is reported as an
//! [`ImportWarning`] rather than failing the import, which is what lets a
//! drawing with a stray text label still come in.

pub mod bitmap;
pub mod colours;
pub mod commands;
pub mod dwg;
pub mod dxf;
pub mod dxf_mesh;
pub mod eps;
pub mod heightmap;
pub mod mesh;
pub mod mesh_3dm;
pub mod mesh_3ds;
pub mod mesh_export;
pub mod mesh_reduce;
pub mod native;
pub mod nurbs;
pub mod pdf;
mod pdf_encoding;
mod pdf_text;
pub mod stl_export;
pub mod svg;
mod type1;
pub mod vectors_cmd;
pub mod view_image;

use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::text::FontLibrary;
use crate::geom::{BBox, Path};

/// How a vector file is read.
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions<'a> {
    /// Multiplies every length once it is in millimetres.
    pub scale: f64,
    /// Units a DXF file without a units header is taken to be in.
    pub units_hint: Option<&'a str>,
    /// Page of a PDF to read, counting from 1; the first when `None`.
    pub page: Option<u32>,
    /// Installed fonts, which stand in for fonts a PDF uses but does not
    /// include. Without them that text is skipped.
    pub fonts: Option<&'a FontLibrary>,
}

impl Default for ImportOptions<'_> {
    fn default() -> Self {
        ImportOptions {
            scale: 1.0,
            units_hint: None,
            page: None,
            fonts: None,
        }
    }
}

/// One vector on its way into the document.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPath {
    /// Layer name the file put it on. Layers are created as needed.
    pub layer: String,
    /// Name the file gave it, when it had one.
    pub name: Option<String>,
    /// The geometry, already in job millimetres.
    pub path: Path,
}

/// A picture a vector file places, on its way in as a bitmap.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedImage {
    /// Layer name the file put it on.
    pub layer: String,
    /// Name the file gave it, when it had one.
    pub name: Option<String>,
    /// The encoded image file.
    pub bytes: Vec<u8>,
    /// Extension of the file it came from, a hint to its format.
    pub ext: Option<String>,
    /// Corners in job millimetres: lower left, lower right, upper right,
    /// upper left of the picture.
    pub quad: [crate::geom::Point; 4],
}

/// Everything read from a vector file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VectorImport {
    /// The vectors.
    pub paths: Vec<ImportedPath>,
    /// Placed pictures.
    pub images: Vec<ImportedImage>,
    /// What could not be brought across.
    pub warnings: Vec<ImportWarning>,
}

/// Something an importer met that it could not bring across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportWarning {
    /// Text was left out, because laying it out needs the original font.
    TextSkipped,
    /// A placed image was left out.
    ImageSkipped,
    /// An SVG `use` reference was left out.
    UseSkipped,
    /// A block reference pointed at a block that is not in the file.
    BlockMissing,
    /// Block references nested deeper than the importer follows.
    BlockTooDeep,
    /// Text used a font the file does not carry; an installed font drew it.
    FontSubstituted,
}

impl ImportWarning {
    /// The warning as the kernel reports it, with its code.
    pub fn to_warning(self) -> crate::warning::Warning {
        let code = match self {
            ImportWarning::TextSkipped => "import.textSkipped",
            ImportWarning::ImageSkipped => "import.imageSkipped",
            ImportWarning::UseSkipped => "import.useSkipped",
            ImportWarning::BlockMissing => "import.blockMissing",
            ImportWarning::BlockTooDeep => "import.blockTooDeep",
            ImportWarning::FontSubstituted => "import.fontSubstituted",
        };
        crate::warning::Warning::new(code, self.message())
    }

    /// A sentence for the import report.
    pub fn message(self) -> &'static str {
        match self {
            ImportWarning::TextSkipped => {
                "text was skipped: import it as vectors from the source program"
            }
            ImportWarning::ImageSkipped => "a placed image was skipped",
            ImportWarning::UseSkipped => "a reused shape was skipped",
            ImportWarning::BlockMissing => "a block reference pointed at a missing block",
            ImportWarning::BlockTooDeep => "block references were nested too deeply to follow",
            ImportWarning::FontSubstituted => {
                "some text uses a font the file does not include; an installed font was used"
            }
        }
    }
}

/// Formats the kernel can read vectors from and write them to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VectorFormat {
    /// Scalable Vector Graphics.
    Svg,
    /// AutoCAD drawing interchange.
    Dxf,
    /// Encapsulated PostScript.
    Eps,
    /// Portable Document Format.
    Pdf,
    /// Adobe Illustrator.
    Ai,
}

impl VectorFormat {
    /// The format a file name suggests, by its extension.
    pub fn of_path(path: &FsPath) -> Option<VectorFormat> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "svg" => Some(VectorFormat::Svg),
            "dxf" => Some(VectorFormat::Dxf),
            "eps" => Some(VectorFormat::Eps),
            "pdf" => Some(VectorFormat::Pdf),
            "ai" => Some(VectorFormat::Ai),
            _ => None,
        }
    }

    /// The name shown in a message.
    pub fn label(self) -> &'static str {
        match self {
            VectorFormat::Svg => "SVG",
            VectorFormat::Dxf => "DXF",
            VectorFormat::Eps => "EPS",
            VectorFormat::Pdf => "PDF",
            VectorFormat::Ai => "Illustrator",
        }
    }
}

/// Reads vectors out of a file, picking the importer by extension.
///
/// # Errors
/// Returns [`CoreError::Unsupported`] for a format this build cannot read,
/// [`CoreError::Io`] when the file cannot be read and [`CoreError::Format`]
/// when it is not the format its name claims.
pub fn import_vectors(
    path: &FsPath,
    options: &ImportOptions<'_>,
) -> Result<VectorImport, CoreError> {
    let only_paths = |(paths, warnings)| VectorImport {
        paths,
        images: Vec::new(),
        warnings,
    };
    if is_dwg(path) {
        return dwg::import(path, options.scale, options.units_hint).map(only_paths);
    }
    match VectorFormat::of_path(path) {
        Some(VectorFormat::Svg) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| CoreError::io(path.display(), e))?;
            svg::import_all(&text, options.scale, path.parent())
        }
        Some(VectorFormat::Dxf) => {
            dxf::import(path, options.scale, options.units_hint).map(only_paths)
        }
        Some(format) => {
            let bytes = std::fs::read(path).map_err(|e| CoreError::io(path.display(), e))?;
            import_page(format, &bytes, options).map(only_paths)
        }
        None => Err(CoreError::unsupported(format!(
            "{} is not a vector file this build can read",
            path.display()
        ))),
    }
}

/// Reads vectors from the bytes of an EPS, PDF or Illustrator file.
///
/// Illustrator files since version 9 are PDF inside and go to the PDF
/// reader; older ones are PostScript and go to the EPS reader. PDF files are
/// read from `options.page`, counting from 1, or from their first page.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not the format named,
/// and [`CoreError::Unsupported`] for SVG and DXF, which are read from
/// their files by [`import_vectors`].
pub fn import_page(
    format: VectorFormat,
    bytes: &[u8],
    options: &ImportOptions<'_>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let (scale, page, fonts) = (options.scale, options.page, options.fonts);
    match format {
        VectorFormat::Eps => eps::import(bytes, scale),
        VectorFormat::Pdf => pdf::import_with_fonts(bytes, scale, page, fonts),
        VectorFormat::Ai if pdf::looks_like_pdf(bytes) => {
            pdf::import_with_fonts(bytes, scale, page, fonts)
        }
        VectorFormat::Ai => eps::import(bytes, scale).map_err(|_| {
            CoreError::format(
                "Illustrator",
                "this is neither a PDF based nor a PostScript based Illustrator file",
            )
        }),
        VectorFormat::Svg | VectorFormat::Dxf => Err(CoreError::unsupported(format!(
            "{} is read from its file, not from bytes",
            format.label()
        ))),
    }
}

/// True for a path with the `.dwg` extension, which is read but never
/// written.
pub fn is_dwg(path: &FsPath) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dwg"))
}

/// How many pages a vector file has to choose from: the page count of a PDF
/// or a PDF based Illustrator file, and 1 for every other file.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be read and
/// [`CoreError::Format`] when a PDF cannot be parsed.
pub fn page_count(path: &FsPath) -> Result<u32, CoreError> {
    match VectorFormat::of_path(path) {
        Some(VectorFormat::Pdf | VectorFormat::Ai) => {
            let bytes = std::fs::read(path).map_err(|e| CoreError::io(path.display(), e))?;
            if pdf::looks_like_pdf(&bytes) {
                pdf::page_count(&bytes)
            } else {
                Ok(1)
            }
        }
        _ => Ok(1),
    }
}

/// Writes vectors as a PDF, EPS or Illustrator file, the page covering
/// `bounds` with its lower left corner at the page origin.
///
/// Returns `None` for SVG and DXF, which have their own writers.
pub fn export_page(
    format: VectorFormat,
    layers: &[svg::ExportLayer],
    bounds: BBox,
) -> Option<Vec<u8>> {
    match format {
        VectorFormat::Pdf => Some(pdf::export(layers, bounds)),
        VectorFormat::Eps => Some(eps::export(layers, bounds, eps::Flavour::Eps).into_bytes()),
        VectorFormat::Ai => {
            Some(eps::export(layers, bounds, eps::Flavour::Illustrator).into_bytes())
        }
        VectorFormat::Svg | VectorFormat::Dxf => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_recognised_by_extension() {
        assert_eq!(
            VectorFormat::of_path(FsPath::new("a/b/part.SVG")),
            Some(VectorFormat::Svg)
        );
        assert_eq!(
            VectorFormat::of_path(FsPath::new("part.dxf")),
            Some(VectorFormat::Dxf)
        );
        assert_eq!(VectorFormat::of_path(FsPath::new("part.stl")), None);
    }

    #[test]
    fn an_unknown_extension_is_refused() {
        let error = import_vectors(FsPath::new("part.stl"), &ImportOptions::default())
            .expect_err("not a vector file");
        assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
    }

    #[test]
    fn a_missing_page_file_is_a_file_error() {
        let error = import_vectors(FsPath::new("no/such/part.pdf"), &ImportOptions::default())
            .expect_err("missing file");
        assert!(matches!(error, CoreError::Io { .. }), "{error}");
    }

    #[test]
    fn illustrator_files_go_to_the_pdf_or_the_postscript_reader() {
        let layers = eps::tests::sample_layers();
        let bounds = BBox::new(
            crate::geom::Point::new(0.0, 0.0),
            crate::geom::Point::new(100.0, 80.0),
        );
        for format in [VectorFormat::Pdf, VectorFormat::Eps, VectorFormat::Ai] {
            let bytes = export_page(format, &layers, bounds).expect("a page format");
            let (paths, _) =
                import_page(VectorFormat::Ai, &bytes, &ImportOptions::default()).expect("imports");
            assert_eq!(paths.len(), 2, "{format:?}");
            let (again, _) =
                import_page(format, &bytes, &ImportOptions::default()).expect("imports");
            assert_eq!(paths, again);
        }
        let error = import_page(VectorFormat::Ai, b"<svg/>", &ImportOptions::default())
            .expect_err("neither");
        assert!(error.to_string().contains("Illustrator"), "{error}");
        assert!(export_page(VectorFormat::Svg, &layers, bounds).is_none());
        assert!(import_page(VectorFormat::Dxf, b"", &ImportOptions::default()).is_err());
    }

    #[test]
    fn warnings_carry_a_sentence() {
        assert!(ImportWarning::TextSkipped.message().contains("text"));
        assert_eq!(
            serde_json::to_string(&ImportWarning::TextSkipped).expect("serialises"),
            "\"textSkipped\""
        );
    }
}
