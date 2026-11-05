//! Text in PDF content streams, brought in as glyph outlines.
//!
//! A font resource names a font program and how character codes pick its
//! glyphs. Embedded TrueType and OpenType programs (`FontFile2`, `FontFile3`
//! of subtype `OpenType`), bare CFF programs (`Type1C`, `CIDFontType0C`) and
//! Type 1 programs (`FontFile`, through [`super::type1`]) are outlined from
//! the file itself, so the letters come in exactly as they were drawn. A font
//! that is not embedded is replaced by the closest installed family (the
//! standard fonts Helvetica, Times and Courier by their usual stand ins), and
//! the file's own character widths still place each glyph.
//!
//! Simple fonts map one byte to a glyph through their encoding (a base
//! encoding plus `Differences`); composite (Type 0) fonts are read with the
//! `Identity-H` and `Identity-V` encodings, two bytes per glyph, which is
//! what nearly every program writes. Type 3 fonts, other CMaps and missing
//! composite fonts are skipped with a warning.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use lopdf::{Dictionary, Document, Object};
use rustybuzz::ttf_parser::{self, GlyphId, OutlineBuilder};

use super::pdf_encoding::{MAC_ROMAN, NAME_TO_UNICODE, STANDARD, WIN_ANSI};
use super::type1::Type1Font;
use crate::geom::text::{FontLibrary, LoadedFont};
use crate::geom::{Mat3, Path, Point, SubPath, Tolerance};

/// Largest font program read, decompressed.
const MAX_FONT: usize = 64 << 20;

/// Why a font resource cannot be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FontProblem {
    /// A Type 3 font, a CMap other than Identity, or a composite font that
    /// is not embedded.
    Unsupported,
}

/// Where the glyph outlines come from.
enum Program {
    /// A TrueType or OpenType file.
    Sfnt(Vec<u8>),
    /// A bare CFF font program.
    Cff(Vec<u8>),
    /// A Type 1 font program.
    Type1(Box<Type1Font>),
    /// An installed font standing in for one that is not embedded.
    Installed(LoadedFont),
}

/// How character codes find glyph ids.
enum CodeMap {
    /// One byte per code, looked up in a table.
    Simple(Box<[Option<u16>; 256]>),
    /// Two bytes per code (a CID). `None` maps each CID to the glyph with
    /// the same id.
    Composite(Option<Vec<u16>>),
}

/// Advance widths in text space units (thousandths of the font size).
enum Widths {
    Simple(Box<[f64; 256]>),
    Composite {
        widths: BTreeMap<u32, f64>,
        default: f64,
    },
}

/// A font resource ready to draw.
pub(super) struct PdfFont {
    program: Program,
    /// Glyph program units to text space (an em of 1).
    glyph_matrix: Mat3,
    map: CodeMap,
    widths: Widths,
    outlines: RefCell<HashMap<u16, Option<Rc<Path>>>>,
    /// True when the font had to be replaced by an installed one.
    pub(super) substituted: bool,
}

/// One glyph of a shown string.
pub(super) struct Glyph {
    /// The glyph to draw, when the code has one.
    pub(super) id: Option<u16>,
    /// Advance in text space units.
    pub(super) width: f64,
    /// True for the single byte code 32, which word spacing applies to.
    pub(super) space: bool,
}

impl PdfFont {
    /// Splits a string into glyphs.
    pub(super) fn glyphs(&self, bytes: &[u8]) -> Vec<Glyph> {
        match &self.map {
            CodeMap::Simple(table) => bytes
                .iter()
                .map(|&code| Glyph {
                    id: table[usize::from(code)],
                    width: self.width(u32::from(code)),
                    space: code == 32,
                })
                .collect(),
            CodeMap::Composite(cid_to_gid) => bytes
                .chunks_exact(2)
                .map(|pair| {
                    let cid = u16::from_be_bytes([pair[0], pair[1]]);
                    let id = match cid_to_gid {
                        None => Some(cid),
                        Some(table) => table.get(usize::from(cid)).copied().filter(|g| *g != 0),
                    };
                    Glyph {
                        id,
                        width: self.width(u32::from(cid)),
                        space: false,
                    }
                })
                .collect(),
        }
    }

    fn width(&self, code: u32) -> f64 {
        match &self.widths {
            Widths::Simple(table) => table[code as usize & 0xFF],
            Widths::Composite { widths, default } => widths.get(&code).copied().unwrap_or(*default),
        }
    }

    /// The outline of glyph `id` in text space, `None` for a glyph that
    /// draws nothing.
    pub(super) fn outline(&self, id: u16) -> Option<Rc<Path>> {
        if let Some(known) = self.outlines.borrow().get(&id) {
            return known.clone();
        }
        let mut sink = Sink::new(self.glyph_matrix);
        let drawn = match &self.program {
            Program::Sfnt(data) => ttf_parser::Face::parse(data, 0)
                .ok()
                .and_then(|face| face.outline_glyph(GlyphId(id), &mut sink))
                .map(|_| ()),
            Program::Cff(data) => ttf_parser::cff::Table::parse(data)
                .and_then(|table| table.outline(GlyphId(id), &mut sink).ok())
                .map(|_| ()),
            Program::Type1(font) => font.outline(id, &mut sink),
            Program::Installed(font) => font
                .face()
                .ok()
                .and_then(|face| face.outline_glyph(GlyphId(id), &mut sink))
                .map(|_| ()),
        };
        let path = drawn.and_then(|_| {
            let path = sink.finish();
            (!path.is_empty()).then(|| Rc::new(path))
        });
        self.outlines.borrow_mut().insert(id, path.clone());
        path
    }
}

/// Collects a glyph outline as a path, mapped by a matrix.
pub(super) struct Sink {
    matrix: Mat3,
    path: Path,
    current: Option<SubPath>,
}

impl Sink {
    pub(super) fn new(matrix: Mat3) -> Self {
        Sink {
            matrix,
            path: Path::new(),
            current: None,
        }
    }

    fn point(&self, x: f32, y: f32) -> Point {
        self.matrix.apply(Point::new(f64::from(x), f64::from(y)))
    }

    fn flush(&mut self) {
        if let Some(mut sub) = self.current.take() {
            if !sub.segments.is_empty() {
                sub.close_with_line(Tolerance::EQUALITY);
                self.path.push(sub);
            }
        }
    }

    pub(super) fn finish(mut self) -> Path {
        self.flush();
        self.path
    }
}

impl OutlineBuilder for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.current = Some(SubPath::open(self.point(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        if let Some(sub) = self.current.as_mut() {
            sub.line_to(to);
        }
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control = self.point(x1, y1);
        let to = self.point(x, y);
        if let Some(sub) = self.current.as_mut() {
            let from = sub.end();
            sub.cubic_to(
                from.lerp(control, 2.0 / 3.0),
                to.lerp(control, 2.0 / 3.0),
                to,
            );
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, to) = (self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        if let Some(sub) = self.current.as_mut() {
            sub.cubic_to(c1, c2, to);
        }
    }

    fn close(&mut self) {
        self.flush();
    }
}

// ------------------------------------------------------------------ loading

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    doc.dereference(object).ok().map(|(_, o)| o)
}

fn get<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    resolve(doc, dict.get(key).ok()?)
}

fn name<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    get(doc, dict, key)?.as_name().ok()
}

fn number(doc: &Document, object: &Object) -> Option<f64> {
    let value = match resolve(doc, object)? {
        Object::Integer(n) => *n as f64,
        Object::Real(n) => f64::from(*n),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn stream_bytes(doc: &Document, object: &Object) -> Option<Vec<u8>> {
    let stream = resolve(doc, object)?.as_stream().ok()?;
    stream.get_plain_content_with_limit(MAX_FONT).ok()
}

/// Reads a font resource.
///
/// # Errors
/// [`FontProblem::Unsupported`] for fonts this reader does not draw.
pub(super) fn load(
    doc: &Document,
    font: &Dictionary,
    installed: Option<&FontLibrary>,
) -> Result<PdfFont, FontProblem> {
    match name(doc, font, b"Subtype") {
        Some(b"Type0") => load_composite(doc, font),
        Some(b"Type1" | b"MMType1" | b"TrueType") | None => load_simple(doc, font, installed),
        Some(_) => Err(FontProblem::Unsupported),
    }
}

/// The embedded program named by a font descriptor.
fn embedded(doc: &Document, descriptor: &Dictionary) -> Option<Program> {
    if let Some(data) = descriptor
        .get(b"FontFile2")
        .ok()
        .and_then(|o| stream_bytes(doc, o))
    {
        return ttf_parser::Face::parse(&data, 0)
            .is_ok()
            .then_some(Program::Sfnt(data));
    }
    if let Ok(object) = descriptor.get(b"FontFile3") {
        let subtype = resolve(doc, object)
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.dict.get(b"Subtype").ok())
            .and_then(|s| s.as_name().ok())
            .map(<[u8]>::to_vec);
        let data = stream_bytes(doc, object)?;
        return match subtype.as_deref() {
            Some(b"OpenType") => ttf_parser::Face::parse(&data, 0)
                .is_ok()
                .then_some(Program::Sfnt(data)),
            _ => ttf_parser::cff::Table::parse(&data)
                .is_some()
                .then_some(Program::Cff(data)),
        };
    }
    let object = descriptor.get(b"FontFile").ok()?;
    let data = stream_bytes(doc, object)?;
    Type1Font::parse(&data).map(|font| Program::Type1(Box::new(font)))
}

/// The font name without a subset tag (`ABCDEF+Helvetica` -> `Helvetica`).
fn base_name(doc: &Document, font: &Dictionary) -> String {
    let raw = name(doc, font, b"BaseFont").unwrap_or(b"");
    let text = String::from_utf8_lossy(raw).into_owned();
    match text.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => {
            rest.to_string()
        }
        _ => text,
    }
}

/// The installed family, bold and italic that best stand in for a font
/// that is not embedded.
fn stand_in(base: &str, flags: i64) -> (Vec<&'static str>, String, bool, bool) {
    let lower = base.to_ascii_lowercase();
    let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
    let italic = lower.contains("italic") || lower.contains("oblique") || flags & 64 != 0;
    let family = base
        .split(['-', ','])
        .next()
        .unwrap_or(base)
        .trim()
        .to_string();
    let candidates: Vec<&'static str> =
        if lower.starts_with("helvetica") || lower.starts_with("arial") {
            vec!["Helvetica", "Arial", "Liberation Sans"]
        } else if lower.starts_with("times") {
            vec!["Times New Roman", "Times", "Liberation Serif"]
        } else if lower.starts_with("courier") {
            vec!["Courier New", "Courier", "Liberation Mono"]
        } else if flags & 2 != 0 {
            // Serif flag.
            vec!["Times New Roman", "Times"]
        } else {
            Vec::new()
        };
    (candidates, family, bold, italic)
}

fn installed_font(library: Option<&FontLibrary>, base: &str, flags: i64) -> Option<LoadedFont> {
    let library = library?;
    let (candidates, family, bold, italic) = stand_in(base, flags);
    let families: Vec<String> = library
        .families()
        .into_iter()
        .map(|f| f.to_lowercase())
        .collect();
    let chosen = std::iter::once(family.as_str())
        .chain(candidates.iter().copied())
        .find(|f| families.contains(&f.to_lowercase()))
        .unwrap_or(FontLibrary::FALLBACK_FAMILY);
    library.load(chosen, bold, italic).ok()
}

/// The Unicode character a glyph name stands for.
fn unicode_of(name: &str) -> Option<char> {
    if let Ok(at) = NAME_TO_UNICODE.binary_search_by(|(n, _)| (*n).cmp(name)) {
        return Some(NAME_TO_UNICODE[at].1);
    }
    let hex = name
        .strip_prefix("uni")
        .filter(|h| h.len() == 4)
        .or_else(|| {
            name.strip_prefix('u')
                .filter(|h| (4..=6).contains(&h.len()))
        })?;
    char::from_u32(u32::from_str_radix(hex, 16).ok()?)
}

/// The glyph names of a simple font's codes: its base encoding with the
/// `Differences` laid over it. `None` where the encoding says nothing,
/// which leaves the code to the font program's own encoding.
fn encoding_names(
    doc: &Document,
    font: &Dictionary,
    default: Option<&'static [&'static str; 256]>,
) -> Vec<Option<String>> {
    let table = |name: &[u8]| -> Option<&'static [&'static str; 256]> {
        match name {
            b"WinAnsiEncoding" => Some(&WIN_ANSI),
            b"MacRomanEncoding" => Some(&MAC_ROMAN),
            b"StandardEncoding" => Some(&STANDARD),
            _ => None,
        }
    };
    let encoding = get(doc, font, b"Encoding");
    let base = match encoding {
        Some(Object::Name(n)) => table(n).or(default),
        Some(Object::Dictionary(d)) => d
            .get(b"BaseEncoding")
            .ok()
            .and_then(|o| resolve(doc, o))
            .and_then(|o| o.as_name().ok())
            .and_then(table)
            .or(default),
        _ => default,
    };
    let mut names: Vec<Option<String>> = (0..256)
        .map(|code| {
            base.map(|t| t[code])
                .filter(|n| !n.is_empty())
                .map(str::to_string)
        })
        .collect();
    if let Some(Object::Dictionary(d)) = encoding {
        if let Some(differences) = d
            .get(b"Differences")
            .ok()
            .and_then(|o| resolve(doc, o))
            .and_then(|o| o.as_array().ok())
        {
            let mut code = 0usize;
            for item in differences {
                match resolve(doc, item) {
                    Some(Object::Integer(n)) => code = usize::try_from(*n).unwrap_or(256),
                    Some(Object::Name(n)) => {
                        if code < 256 {
                            names[code] = Some(String::from_utf8_lossy(n).into_owned());
                        }
                        code += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    names
}

/// A glyph id from the symbol (3,0) or Macintosh (1,0) cmap of a font, as
/// PDF looks up codes of symbolic TrueType fonts.
fn symbol_glyph(face: &ttf_parser::Face<'_>, code: u8) -> Option<u16> {
    let cmap = face.tables().cmap?;
    let code = u32::from(code);
    for subtable in cmap.subtables {
        let hit = match (subtable.platform_id, subtable.encoding_id) {
            (ttf_parser::PlatformId::Windows, 0) => subtable
                .glyph_index(0xF000 | code)
                .or_else(|| subtable.glyph_index(code)),
            (ttf_parser::PlatformId::Macintosh, 0) => subtable.glyph_index(code),
            _ => None,
        };
        if let Some(GlyphId(id)) = hit.filter(|g| g.0 != 0) {
            return Some(id);
        }
    }
    None
}

fn sfnt_glyph(
    face: &ttf_parser::Face<'_>,
    code: u8,
    name: Option<&str>,
    symbolic: bool,
) -> Option<u16> {
    let by_name = name
        .and_then(|n| {
            unicode_of(n)
                .and_then(|c| face.glyph_index(c))
                .or_else(|| face.glyph_index_by_name(n))
        })
        .map(|g| g.0)
        .filter(|g| *g != 0);
    if symbolic {
        symbol_glyph(face, code).or(by_name)
    } else {
        by_name.or_else(|| symbol_glyph(face, code))
    }
}

fn load_simple(
    doc: &Document,
    font: &Dictionary,
    installed: Option<&FontLibrary>,
) -> Result<PdfFont, FontProblem> {
    let descriptor = get(doc, font, b"FontDescriptor").and_then(|o| o.as_dict().ok());
    let flags = descriptor
        .and_then(|d| get(doc, d, b"Flags"))
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(32);
    let symbolic = flags & 4 != 0;
    let base = base_name(doc, font);
    let mut substituted = false;
    let program = match descriptor.and_then(|d| embedded(doc, d)) {
        Some(program) => program,
        None => {
            let lower = base.to_ascii_lowercase();
            if lower.starts_with("symbol") || lower.starts_with("zapfdingbats") {
                return Err(FontProblem::Unsupported);
            }
            substituted = true;
            Program::Installed(
                installed_font(installed, &base, flags).ok_or(FontProblem::Unsupported)?,
            )
        }
    };
    let default = match &program {
        Program::Cff(_) | Program::Type1(_) => None,
        Program::Sfnt(_) if symbolic => None,
        _ => Some(&STANDARD),
    };
    let names = encoding_names(doc, font, default);

    let mut table = Box::new([None; 256]);
    let glyph_matrix;
    match &program {
        Program::Sfnt(data) => {
            let face = ttf_parser::Face::parse(data, 0).map_err(|_| FontProblem::Unsupported)?;
            glyph_matrix = em_matrix(face.units_per_em());
            for (code, slot) in table.iter_mut().enumerate() {
                *slot = sfnt_glyph(&face, code as u8, names[code].as_deref(), symbolic);
            }
        }
        Program::Installed(loaded) => {
            let face = loaded.face().map_err(|_| FontProblem::Unsupported)?;
            glyph_matrix = em_matrix(u16::try_from(face.units_per_em()).unwrap_or(1000));
            for (code, slot) in table.iter_mut().enumerate() {
                *slot = sfnt_glyph(&face, code as u8, names[code].as_deref(), false);
            }
        }
        Program::Cff(data) => {
            let cff = ttf_parser::cff::Table::parse(data).ok_or(FontProblem::Unsupported)?;
            let m = cff.matrix();
            glyph_matrix = Mat3::new(
                f64::from(m.sx),
                f64::from(m.ky),
                f64::from(m.kx),
                f64::from(m.sy),
                0.0,
                0.0,
            );
            for (code, slot) in table.iter_mut().enumerate() {
                let hit = match names[code].as_deref() {
                    Some(n) => cff.glyph_index_by_name(n),
                    None => cff.glyph_index(code as u8),
                };
                *slot = hit.map(|g| g.0).filter(|g| *g != 0);
            }
        }
        Program::Type1(type1) => {
            glyph_matrix = type1.matrix();
            for (code, slot) in table.iter_mut().enumerate() {
                *slot = match names[code].as_deref() {
                    Some(n) => type1.glyph_by_name(n),
                    None => type1.glyph_by_code(code as u8),
                };
            }
        }
    }

    // Widths from the file, else from the program.
    let mut widths = Box::new([0.0; 256]);
    for (code, slot) in widths.iter_mut().enumerate() {
        if let Some(id) = table[code] {
            *slot = program_advance(&program, id, &glyph_matrix);
        }
    }
    let first = get(doc, font, b"FirstChar")
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    if let Some(list) = get(doc, font, b"Widths").and_then(|o| o.as_array().ok()) {
        for (index, item) in list.iter().enumerate() {
            let code = first + index as i64;
            if let (Ok(code), Some(w)) = (usize::try_from(code), number(doc, item)) {
                if code < 256 {
                    widths[code] = w / 1000.0;
                }
            }
        }
    }
    Ok(PdfFont {
        program,
        glyph_matrix,
        map: CodeMap::Simple(table),
        widths: Widths::Simple(widths),
        outlines: RefCell::new(HashMap::new()),
        substituted,
    })
}

fn em_matrix(units_per_em: u16) -> Mat3 {
    let s = 1.0 / f64::from(units_per_em.max(1));
    Mat3::scale(s, s)
}

/// A glyph's advance in text space units according to its program.
fn program_advance(program: &Program, id: u16, matrix: &Mat3) -> f64 {
    let units = match program {
        Program::Sfnt(data) => ttf_parser::Face::parse(data, 0)
            .ok()
            .and_then(|f| f.glyph_hor_advance(GlyphId(id))),
        Program::Installed(font) => font
            .face()
            .ok()
            .and_then(|f| f.glyph_hor_advance(GlyphId(id))),
        Program::Cff(data) => {
            ttf_parser::cff::Table::parse(data).and_then(|t| t.glyph_width(GlyphId(id)))
        }
        Program::Type1(font) => font.advance(id),
    };
    units.map_or(0.0, |u| {
        matrix
            .apply_vector(crate::geom::Vec2::new(f64::from(u), 0.0))
            .x
    })
}

fn load_composite(doc: &Document, font: &Dictionary) -> Result<PdfFont, FontProblem> {
    match name(doc, font, b"Encoding") {
        Some(b"Identity-H" | b"Identity-V") => {}
        _ => return Err(FontProblem::Unsupported),
    }
    let descendant = get(doc, font, b"DescendantFonts")
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.first())
        .and_then(|o| resolve(doc, o))
        .and_then(|o| o.as_dict().ok())
        .ok_or(FontProblem::Unsupported)?;
    let descriptor = get(doc, descendant, b"FontDescriptor")
        .and_then(|o| o.as_dict().ok())
        .ok_or(FontProblem::Unsupported)?;
    let program = embedded(doc, descriptor).ok_or(FontProblem::Unsupported)?;
    let (glyph_matrix, cid_to_gid) = match &program {
        Program::Sfnt(data) => {
            let face = ttf_parser::Face::parse(data, 0).map_err(|_| FontProblem::Unsupported)?;
            let map = match get(doc, descendant, b"CIDToGIDMap") {
                Some(Object::Stream(stream)) => {
                    let bytes = stream
                        .get_plain_content_with_limit(MAX_FONT)
                        .map_err(|_| FontProblem::Unsupported)?;
                    Some(
                        bytes
                            .chunks_exact(2)
                            .map(|p| u16::from_be_bytes([p[0], p[1]]))
                            .collect(),
                    )
                }
                _ => None,
            };
            (em_matrix(face.units_per_em()), map)
        }
        Program::Cff(data) => {
            let cff = ttf_parser::cff::Table::parse(data).ok_or(FontProblem::Unsupported)?;
            let m = cff.matrix();
            let matrix = Mat3::new(
                f64::from(m.sx),
                f64::from(m.ky),
                f64::from(m.kx),
                f64::from(m.sy),
                0.0,
                0.0,
            );
            let mut map: Vec<u16> = Vec::new();
            for gid in 0..cff.number_of_glyphs() {
                if let Some(cid) = cff.glyph_cid(GlyphId(gid)) {
                    let cid = usize::from(cid);
                    if map.len() <= cid {
                        map.resize(cid + 1, 0);
                    }
                    map[cid] = gid;
                }
            }
            (matrix, (!map.is_empty()).then_some(map))
        }
        Program::Type1(_) | Program::Installed(_) => return Err(FontProblem::Unsupported),
    };
    let default = get(doc, descendant, b"DW")
        .and_then(|o| number(doc, o))
        .unwrap_or(1000.0)
        / 1000.0;
    let mut widths = BTreeMap::new();
    if let Some(list) = get(doc, descendant, b"W").and_then(|o| o.as_array().ok()) {
        let mut at = 0;
        while at < list.len() {
            let Some(first) = number(doc, &list[at]) else {
                break;
            };
            let first = first as u32;
            match list.get(at + 1).and_then(|o| resolve(doc, o)) {
                Some(Object::Array(each)) => {
                    for (offset, w) in each.iter().enumerate() {
                        if let Some(w) = number(doc, w) {
                            widths.insert(first + offset as u32, w / 1000.0);
                        }
                    }
                    at += 2;
                }
                Some(last) => {
                    let (Some(last), Some(w)) = (
                        number(doc, last),
                        list.get(at + 2).and_then(|o| number(doc, o)),
                    ) else {
                        break;
                    };
                    let last = (last as u32).min(first.saturating_add(0xFFFF));
                    for cid in first..=last {
                        widths.insert(cid, w / 1000.0);
                    }
                    at += 3;
                }
                None => break,
            }
        }
    }
    Ok(PdfFont {
        program,
        glyph_matrix,
        map: CodeMap::Composite(cid_to_gid),
        widths: Widths::Composite { widths, default },
        outlines: RefCell::new(HashMap::new()),
        substituted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names_give_characters() {
        assert_eq!(unicode_of("A"), Some('A'));
        assert_eq!(unicode_of("eacute"), Some('\u{e9}'));
        assert_eq!(unicode_of("quotedblleft"), Some('\u{201c}'));
        assert_eq!(unicode_of("uni20AC"), Some('\u{20ac}'));
        assert_eq!(unicode_of("u1F600"), Some('\u{1f600}'));
        assert_eq!(unicode_of("g123"), None);
        assert!(NAME_TO_UNICODE.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn encodings_name_the_usual_characters() {
        assert_eq!(WIN_ANSI[0x41], "A");
        assert_eq!(WIN_ANSI[0x80], "Euro");
        assert_eq!(WIN_ANSI[0xE9], "eacute");
        assert_eq!(MAC_ROMAN[0x8E], "eacute");
        assert_eq!(STANDARD[0x27], "quoteright");
        assert_eq!(STANDARD[0x80], "");
    }

    #[test]
    fn missing_fonts_are_replaced_by_their_usual_stand_ins() {
        let (candidates, family, bold, italic) = stand_in("Helvetica-BoldOblique", 0);
        assert_eq!(candidates[0], "Helvetica");
        assert_eq!(family, "Helvetica");
        assert!(bold && italic);
        let (candidates, family, bold, _) = stand_in("Calibri,Bold", 32);
        assert!(candidates.is_empty());
        assert_eq!(family, "Calibri");
        assert!(bold);
    }
}
