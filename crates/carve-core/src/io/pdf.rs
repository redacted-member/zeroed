//! PDF vector import and export.
//!
//! Import reads one page (the first unless told otherwise). The file
//! structure, page tree and stream filters are read with `lopdf`; the page
//! content stream is then run with the same lexer and path builder as the
//! EPS importer: the path operators `m l c v y h re`, the painting
//! operators that end a path, `q`, `Q` and `cm` for the transform, and `Do`
//! for Form XObjects, followed a few levels deep. Optional content groups
//! (`/OC /name BDC ... EMC`) name the layer the vectors land on, which is
//! how Illustrator and most drawing programs write layers into a PDF.
//! Text comes in as glyph outlines, one vector per character, drawn from
//! the fonts embedded in the file (see [`super::pdf_text`]). Images are
//! skipped and reported.
//!
//! PDF user space is in points with Y going up, like the job, so a point
//! becomes 25.4 / 72 millimetres and the lower left corner of the page's
//! MediaBox becomes the job origin. Page rotation is not applied.
//!
//! Export writes a single page PDF sized to the bounds in millimetres, paths
//! only, one optional content group per layer so the layers survive the
//! trip to other programs and back.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};

use super::eps::{
    number, page_to_mm, path_operators, push_path, rgb, warn, Lexer, PathBuilder, Token,
    DEFAULT_LAYER, PT_PER_MM,
};
use super::pdf_text::{self, PdfFont};
use super::svg::ExportLayer;
use super::{ImportWarning, ImportedPath};
use crate::error::CoreError;
use crate::geom::text::FontLibrary;
use crate::geom::{transform_path, BBox, Mat3, Point, Vec2};

/// Largest a single stream may decompress to, which stops a small file
/// that inflates without end.
const MAX_STREAM: usize = 256 << 20;
/// Form XObjects nested deeper than this are not followed.
const MAX_FORM_DEPTH: usize = 12;
/// Page tree levels followed when looking for inherited page attributes.
const MAX_TREE_DEPTH: usize = 32;
/// US Letter, used when a page has no MediaBox.
const DEFAULT_MEDIA_BOX: [f64; 4] = [0.0, 0.0, 612.0, 792.0];

/// True when the bytes are a PDF file, allowing for junk before the header
/// as PDF readers do.
pub fn looks_like_pdf(bytes: &[u8]) -> bool {
    bytes.windows(5).take(1024).any(|window| window == b"%PDF-")
}

/// Reads vectors out of one page of a PDF file.
///
/// `page` counts from 1 and defaults to the first page. `scale` multiplies
/// every length after points become millimetres.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not a readable PDF and
/// [`CoreError::Validation`] when the page is not in the file.
pub fn import(
    bytes: &[u8],
    scale: f64,
    page: Option<u32>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    import_with_fonts(bytes, scale, page, None)
}

/// [`import`], with the installed fonts that stand in for fonts the file
/// uses but does not embed. Without them, text in such fonts is skipped.
///
/// # Errors
/// As [`import`].
pub fn import_with_fonts(
    bytes: &[u8],
    scale: f64,
    page: Option<u32>,
    fonts: Option<&FontLibrary>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    if !looks_like_pdf(bytes) {
        return Err(CoreError::format(
            "PDF",
            "this is not a PDF file: it has no %PDF header",
        ));
    }
    // The PDF reader is a third party parser working on untrusted bytes;
    // a bug there must end up as an error for this file, not a crash.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        import_page(bytes, scale, page, fonts)
    }))
    .unwrap_or_else(|_| {
        Err(CoreError::format(
            "PDF",
            "the file is damaged and could not be read",
        ))
    })
}

/// How many pages a PDF file has.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not a readable PDF.
pub fn page_count(bytes: &[u8]) -> Result<u32, CoreError> {
    if !looks_like_pdf(bytes) {
        return Err(CoreError::format(
            "PDF",
            "this is not a PDF file: it has no %PDF header",
        ));
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let options = LoadOptions {
            max_decompressed_size: Some(MAX_STREAM),
            ..LoadOptions::default()
        };
        let doc = Document::load_mem_with_options(bytes, options).map_err(pdf_error)?;
        u32::try_from(doc.get_pages().len())
            .map_err(|_| CoreError::format("PDF", "the file has too many pages"))
    }))
    .unwrap_or_else(|_| {
        Err(CoreError::format(
            "PDF",
            "the file is damaged and could not be read",
        ))
    })
}

fn pdf_error(error: lopdf::Error) -> CoreError {
    CoreError::format("PDF", error)
}

fn import_page(
    bytes: &[u8],
    scale: f64,
    page: Option<u32>,
    fonts: Option<&FontLibrary>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let options = LoadOptions {
        max_decompressed_size: Some(MAX_STREAM),
        ..LoadOptions::default()
    };
    let doc = Document::load_mem_with_options(bytes, options).map_err(pdf_error)?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(CoreError::format("PDF", "the file has no pages"));
    }
    let number = page.unwrap_or(1);
    let page_id = *pages.get(&number).ok_or_else(|| {
        CoreError::validation(format!(
            "page {number} is not in this PDF, which has {} page{}",
            pages.len(),
            if pages.len() == 1 { "" } else { "s" }
        ))
    })?;
    let page_dict = doc.get_dictionary(page_id).map_err(pdf_error)?;
    let media = inherited(&doc, page_dict, b"MediaBox")
        .and_then(|object| rectangle(&doc, object))
        .unwrap_or(DEFAULT_MEDIA_BOX);
    let resources = inherited(&doc, page_dict, b"Resources").and_then(|o| dict(&doc, o));
    let content = doc
        .get_page_content_with_limit(page_id, MAX_STREAM)
        .map_err(pdf_error)?;

    let mut run = Run::new(fonts);
    run.content(&doc, &content, resources, 0);

    let origin = Point::new(media[0].min(media[2]), media[1].min(media[3]));
    let to_mm = page_to_mm(origin, scale);
    let paths = run
        .out
        .into_iter()
        .map(|imported| ImportedPath {
            path: transform_path(&imported.path, &to_mm),
            ..imported
        })
        .collect();
    Ok((paths, run.warnings))
}

/// A page attribute, looked up through the page tree as PDF inherits them.
fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").and_then(Object::as_reference).ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    doc.dereference(object).ok().map(|(_, o)| o)
}

fn dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(doc, object)?.as_dict().ok()
}

fn number_of(doc: &Document, object: &Object) -> Option<f64> {
    let value = match resolve(doc, object)? {
        Object::Integer(n) => *n as f64,
        Object::Real(n) => f64::from(*n),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn rectangle(doc: &Document, object: &Object) -> Option<[f64; 4]> {
    let items = resolve(doc, object)?.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = number_of(doc, item)?;
    }
    Some(out)
}

fn matrix(doc: &Document, object: &Object) -> Option<Mat3> {
    let items = resolve(doc, object)?.as_array().ok()?;
    if items.len() != 6 {
        return None;
    }
    let mut m = [0.0; 6];
    for (slot, item) in m.iter_mut().zip(items) {
        *slot = number_of(doc, item)?;
    }
    Some(Mat3::new(m[0], m[1], m[2], m[3], m[4], m[5]))
}

/// A PDF text string: UTF-16 with a byte order mark, UTF-8 with one, or
/// else one byte per character.
fn text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|b| char::from(*b)).collect()
}

#[derive(Debug, Clone)]
enum Operand {
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<Operand>),
    Other,
}

/// The text state parameters, which `q` and `Q` save and restore with the
/// transform.
#[derive(Clone)]
struct TextState {
    font: Option<Rc<PdfFont>>,
    size: f64,
    char_spacing: f64,
    word_spacing: f64,
    /// Horizontal scaling, 1 for 100 percent.
    h_scale: f64,
    leading: f64,
    rise: f64,
    /// Rendering mode; 3 and 7 draw nothing.
    render: i64,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            font: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            render: 0,
        }
    }
}

/// The state of a content stream run.
struct Run<'f> {
    ctm: Mat3,
    saved: Vec<(Mat3, TextState)>,
    path: PathBuilder,
    /// Marked content nesting; `Some` where a section names a layer.
    layers: Vec<Option<String>>,
    out: Vec<ImportedPath>,
    warnings: Vec<ImportWarning>,
    text: TextState,
    /// Text matrix and text line matrix.
    tm: Mat3,
    tlm: Mat3,
    /// Installed fonts, for fonts the file does not embed.
    fonts: Option<&'f FontLibrary>,
    /// Fonts read so far by object, `None` for ones that cannot be drawn.
    font_cache: HashMap<ObjectId, Option<Rc<PdfFont>>>,
}

impl<'f> Run<'f> {
    fn new(fonts: Option<&'f FontLibrary>) -> Self {
        Run {
            ctm: Mat3::IDENTITY,
            saved: Vec::new(),
            path: PathBuilder::default(),
            layers: Vec::new(),
            out: Vec::new(),
            warnings: Vec::new(),
            text: TextState::default(),
            tm: Mat3::IDENTITY,
            tlm: Mat3::IDENTITY,
            fonts,
            font_cache: HashMap::new(),
        }
    }

    /// Selects font resource `name` at `size`, as `Tf` does.
    fn set_font(&mut self, doc: &Document, resources: Option<&Dictionary>, name: &str, size: f64) {
        self.text.size = size;
        let Some(object) = resources
            .and_then(|r| r.get(b"Font").ok())
            .and_then(|f| dict(doc, f))
            .and_then(|f| f.get(name.as_bytes()).ok())
        else {
            self.text.font = None;
            return;
        };
        let id = object.as_reference().ok();
        if let Some(cached) = id.and_then(|id| self.font_cache.get(&id)) {
            self.text.font = cached.clone();
            return;
        }
        let loaded = dict(doc, object)
            .and_then(|font| pdf_text::load(doc, font, self.fonts).ok())
            .map(Rc::new);
        if let Some(id) = id {
            self.font_cache.insert(id, loaded.clone());
        }
        self.text.font = loaded;
    }

    /// Moves to the start of the next line, `Td` style.
    fn next_line(&mut self, tx: f64, ty: f64) {
        self.tlm = Mat3::translate(Vec2::new(tx, ty)).then(&self.tlm);
        self.tm = self.tlm;
    }

    /// Draws a string with the current font and moves along it.
    fn show(&mut self, bytes: &[u8]) {
        let Some(font) = self.text.font.clone() else {
            warn(&mut self.warnings, ImportWarning::TextSkipped);
            return;
        };
        if font.substituted {
            warn(&mut self.warnings, ImportWarning::FontSubstituted);
        }
        let state = &self.text;
        let size_matrix = Mat3::new(
            state.size * state.h_scale,
            0.0,
            0.0,
            state.size,
            0.0,
            state.rise,
        );
        let visible = !matches!(state.render, 3 | 7);
        let layer = self.layer().to_string();
        for glyph in font.glyphs(bytes) {
            if visible {
                if let Some(outline) = glyph.id.and_then(|id| font.outline(id)) {
                    let to_user = size_matrix.then(&self.tm).then(&self.ctm);
                    push_path(&mut self.out, &layer, transform_path(&outline, &to_user));
                }
            }
            let state = &self.text;
            let spacing = state.char_spacing + if glyph.space { state.word_spacing } else { 0.0 };
            let advance = (glyph.width * state.size + spacing) * state.h_scale;
            self.tm = Mat3::translate(Vec2::new(advance, 0.0)).then(&self.tm);
        }
    }

    /// `TJ`: strings and position adjustments in thousandths of an em.
    fn show_array(&mut self, items: &[Operand]) {
        for item in items {
            match item {
                Operand::Str(bytes) => self.show(bytes),
                Operand::Num(n) => {
                    let shift = -n / 1000.0 * self.text.size * self.text.h_scale;
                    self.tm = Mat3::translate(Vec2::new(shift, 0.0)).then(&self.tm);
                }
                _ => {}
            }
        }
    }

    fn layer(&self) -> &str {
        self.layers
            .iter()
            .rev()
            .find_map(|l| l.as_deref())
            .unwrap_or(DEFAULT_LAYER)
    }

    fn user(&self, x: f64, y: f64) -> Point {
        self.ctm.apply(Point::new(x, y))
    }

    fn paint(&mut self, close_first: bool, fill: bool) {
        if close_first {
            self.path.close();
        }
        if let Some(path) = self.path.take(fill) {
            let layer = self.layer().to_string();
            push_path(&mut self.out, &layer, path);
        }
    }

    /// Runs one content stream with the resources it can refer to.
    fn content(
        &mut self,
        doc: &Document,
        bytes: &[u8],
        resources: Option<&Dictionary>,
        depth: usize,
    ) {
        let mut lexer = Lexer::new(bytes);
        let mut operands: Vec<Operand> = Vec::new();
        let mut marks: Vec<usize> = Vec::new();
        while let Some(token) = lexer.next_token() {
            match token {
                Token::Number(n) => operands.push(Operand::Num(n)),
                Token::Literal(name) => operands.push(Operand::Name(name)),
                Token::Str(bytes) => operands.push(Operand::Str(bytes)),
                Token::ArrayOpen | Token::DictOpen => marks.push(operands.len()),
                Token::ArrayClose => {
                    let items = marks
                        .pop()
                        .map(|mark| operands.split_off(mark.min(operands.len())))
                        .unwrap_or_default();
                    operands.push(Operand::Array(items));
                }
                Token::DictClose => {
                    if let Some(mark) = marks.pop() {
                        operands.truncate(mark);
                    }
                    operands.push(Operand::Other);
                }
                Token::ProcOpen | Token::ProcClose | Token::Comment(_) => {}
                Token::Name(op) => {
                    if op == "BI" {
                        // Inline image: its dictionary runs to ID, then raw
                        // data to EI.
                        while let Some(inner) = lexer.next_token() {
                            if inner == Token::Name("ID".into()) {
                                lexer.skip_inline_image();
                                break;
                            }
                        }
                        warn(&mut self.warnings, ImportWarning::ImageSkipped);
                    } else {
                        self.operator(&op, &operands, doc, resources, depth);
                    }
                    operands.clear();
                    marks.clear();
                }
            }
        }
    }

    fn operator(
        &mut self,
        op: &str,
        operands: &[Operand],
        doc: &Document,
        resources: Option<&Dictionary>,
        depth: usize,
    ) {
        match op {
            "m" => {
                if let Some([x, y]) = nums(operands) {
                    let p = self.user(x, y);
                    self.path.move_to(p);
                }
            }
            "l" => {
                if let Some([x, y]) = nums(operands) {
                    let p = self.user(x, y);
                    self.path.line_to(p);
                }
            }
            "c" => {
                if let Some([x1, y1, x2, y2, x3, y3]) = nums(operands) {
                    let (c1, c2, p) = (self.user(x1, y1), self.user(x2, y2), self.user(x3, y3));
                    self.path.curve_to(c1, c2, p);
                }
            }
            "v" => {
                if let Some([x2, y2, x3, y3]) = nums(operands) {
                    if let Some(c1) = self.path.current_point() {
                        let (c2, p) = (self.user(x2, y2), self.user(x3, y3));
                        self.path.curve_to(c1, c2, p);
                    }
                }
            }
            "y" => {
                if let Some([x1, y1, x3, y3]) = nums(operands) {
                    let (c1, p) = (self.user(x1, y1), self.user(x3, y3));
                    self.path.curve_to(c1, p, p);
                }
            }
            "h" => self.path.close(),
            "re" => {
                if let Some([x, y, w, h]) = nums(operands) {
                    let corners = [
                        self.user(x, y),
                        self.user(x + w, y),
                        self.user(x + w, y + h),
                        self.user(x, y + h),
                    ];
                    self.path.rect(corners);
                }
            }
            "S" => self.paint(false, false),
            "s" => self.paint(true, false),
            "f" | "F" | "f*" | "B" | "B*" => self.paint(false, true),
            "b" | "b*" => self.paint(true, true),
            "n" => self.path.clear(),
            "q" => self.saved.push((self.ctm, self.text.clone())),
            "Q" => {
                if let Some((ctm, text)) = self.saved.pop() {
                    self.ctm = ctm;
                    self.text = text;
                }
            }
            "cm" => {
                if let Some([a, b, c, d, e, f]) = nums(operands) {
                    self.ctm = Mat3::new(a, b, c, d, e, f).then(&self.ctm);
                }
            }
            "BT" => {
                self.tm = Mat3::IDENTITY;
                self.tlm = Mat3::IDENTITY;
            }
            "Tf" => {
                if let [.., Operand::Name(font), Operand::Num(size)] = operands {
                    self.set_font(doc, resources, font, *size);
                }
            }
            "Tc" => {
                if let Some([v]) = nums(operands) {
                    self.text.char_spacing = v;
                }
            }
            "Tw" => {
                if let Some([v]) = nums(operands) {
                    self.text.word_spacing = v;
                }
            }
            "Tz" => {
                if let Some([v]) = nums(operands) {
                    self.text.h_scale = v / 100.0;
                }
            }
            "TL" => {
                if let Some([v]) = nums(operands) {
                    self.text.leading = v;
                }
            }
            "Ts" => {
                if let Some([v]) = nums(operands) {
                    self.text.rise = v;
                }
            }
            "Tr" => {
                if let Some([v]) = nums(operands) {
                    self.text.render = v as i64;
                }
            }
            "Td" => {
                if let Some([tx, ty]) = nums(operands) {
                    self.next_line(tx, ty);
                }
            }
            "TD" => {
                if let Some([tx, ty]) = nums(operands) {
                    self.text.leading = -ty;
                    self.next_line(tx, ty);
                }
            }
            "Tm" => {
                if let Some([a, b, c, d, e, f]) = nums(operands) {
                    self.tlm = Mat3::new(a, b, c, d, e, f);
                    self.tm = self.tlm;
                }
            }
            "T*" => self.next_line(0.0, -self.text.leading),
            "Tj" => {
                if let Some(Operand::Str(bytes)) = operands.last() {
                    self.show(bytes);
                }
            }
            "'" => {
                self.next_line(0.0, -self.text.leading);
                if let Some(Operand::Str(bytes)) = operands.last() {
                    self.show(bytes);
                }
            }
            "\"" => {
                if let [.., Operand::Num(aw), Operand::Num(ac), Operand::Str(bytes)] = operands {
                    self.text.word_spacing = *aw;
                    self.text.char_spacing = *ac;
                    self.next_line(0.0, -self.text.leading);
                    self.show(bytes);
                }
            }
            "TJ" => {
                if let Some(Operand::Array(items)) = operands.last() {
                    self.show_array(items);
                }
            }
            "BMC" => self.layers.push(None),
            "BDC" => {
                let name = layer_name(doc, operands, resources);
                self.layers.push(name);
            }
            "EMC" => {
                self.layers.pop();
            }
            "Do" => {
                if let Some(Operand::Name(name)) = operands.last() {
                    self.xobject(doc, name, resources, depth);
                }
            }
            _ => {}
        }
    }

    /// Draws a Form XObject, or reports an image.
    fn xobject(
        &mut self,
        doc: &Document,
        name: &str,
        resources: Option<&Dictionary>,
        depth: usize,
    ) {
        let Some(stream) = resources
            .and_then(|r| r.get(b"XObject").ok())
            .and_then(|x| dict(doc, x))
            .and_then(|x| x.get(name.as_bytes()).ok())
            .and_then(|o| resolve(doc, o))
            .and_then(|o| o.as_stream().ok())
        else {
            return;
        };
        let subtype = stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or(b"");
        match subtype {
            b"Image" => warn(&mut self.warnings, ImportWarning::ImageSkipped),
            b"Form" if depth < MAX_FORM_DEPTH => {
                let Ok(content) = stream.get_plain_content_with_limit(MAX_STREAM) else {
                    return;
                };
                let form_matrix = stream
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|m| matrix(doc, m))
                    .unwrap_or(Mat3::IDENTITY);
                let form_resources = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|r| dict(doc, r))
                    .or(resources);
                let (ctm, saved, layers) = (self.ctm, self.saved.len(), self.layers.len());
                let text = self.text.clone();
                self.ctm = form_matrix.then(&self.ctm);
                self.content(doc, &content, form_resources, depth + 1);
                self.ctm = ctm;
                self.text = text;
                self.saved.truncate(saved);
                self.layers.truncate(layers);
            }
            _ => {}
        }
    }
}

/// The last `N` operands as numbers, when they are numbers.
fn nums<const N: usize>(operands: &[Operand]) -> Option<[f64; N]> {
    let tail = operands.get(operands.len().checked_sub(N)?..)?;
    let mut out = [0.0; N];
    for (slot, operand) in out.iter_mut().zip(tail) {
        match operand {
            Operand::Num(n) => *slot = *n,
            _ => return None,
        }
    }
    Some(out)
}

/// The layer an optional content section names: `/OC /name BDC` where the
/// page's Properties map `name` to an optional content group.
fn layer_name(
    doc: &Document,
    operands: &[Operand],
    resources: Option<&Dictionary>,
) -> Option<String> {
    let [Operand::Name(tag), Operand::Name(property)] = operands else {
        return None;
    };
    if tag != "OC" {
        return None;
    }
    let group = resources
        .and_then(|r| r.get(b"Properties").ok())
        .and_then(|p| dict(doc, p))
        .and_then(|p| p.get(property.as_bytes()).ok())
        .and_then(|g| dict(doc, g))?;
    let name = resolve(doc, group.get(b"Name").ok()?)?.as_str().ok()?;
    let name = text_string(name).trim().to_string();
    (!name.is_empty()).then_some(name)
}

// ------------------------------------------------------------------ export

/// Text for a PDF string: `(...)` for printable ASCII, UTF-16 hex otherwise.
fn pdf_string(text: &str) -> String {
    if text.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        let mut out = String::from("(");
        for c in text.chars() {
            if matches!(c, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for unit in text.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push('>');
        out
    }
}

/// Assembles numbered objects into a PDF file with a cross reference table.
/// Object `n` is `objects[n - 1]`.
fn write_file(objects: &[Vec<u8>], trailer: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(table, "{offset:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} {trailer} >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(table.as_bytes());
    out
}

/// A stream object with the given content.
fn stream_object(dictionary: &str, content: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dictionary} /Length {} >>\nstream\n", content.len()).into_bytes();
    out.extend_from_slice(content);
    out.extend_from_slice(b"\nendstream");
    out
}

/// Writes vectors as a one page PDF, the page sized to `bounds`.
///
/// Lengths are written in points, with the lower left corner of `bounds`
/// at the page origin. Each layer is an optional content group named after
/// it and stroked in its colour. Arcs are written as cubic Beziers.
pub fn export(layers: &[ExportLayer], bounds: BBox) -> Vec<u8> {
    let width = bounds.width().max(1.0) * PT_PER_MM;
    let height = bounds.height().max(1.0) * PT_PER_MM;
    let used: Vec<&ExportLayer> = layers.iter().filter(|l| !l.paths.is_empty()).collect();
    let to_pt = Mat3::translate(Vec2::new(-bounds.min.x, -bounds.min.y))
        .then(&Mat3::scale(PT_PER_MM, PT_PER_MM));

    let mut content = String::from("0.28346 w 1 J 1 j\n");
    for (index, layer) in used.iter().enumerate() {
        let (r, g, b) = rgb(&layer.colour);
        let _ = writeln!(content, "/OC /L{} BDC", index + 1);
        let _ = writeln!(content, "{} {} {} RG", number(r), number(g), number(b));
        for path in &layer.paths {
            let body = path_operators(path, &to_pt, "m", "l", "c", "h");
            if !body.is_empty() {
                content.push_str(&body);
                content.push_str("S\n");
            }
        }
        content.push_str("EMC\n");
    }

    // 1 catalog, 2 page tree, 3 page, 4 content, 5 info, 6.. layer groups.
    let first_group = 6;
    let groups: Vec<String> = (0..used.len())
        .map(|i| format!("{} 0 R", first_group + i))
        .collect();
    let group_list = groups.join(" ");
    let properties: String = groups
        .iter()
        .enumerate()
        .map(|(i, r)| format!("/L{} {r} ", i + 1))
        .collect();
    let mut objects: Vec<Vec<u8>> = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [{group_list}] \
             /D << /Order [{group_list}] /ON [{group_list}] >> >> >>"
        )
        .into_bytes(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
             /Resources << /Properties << {properties}>> >> /Contents 4 0 R >>",
            number(width),
            number(height)
        )
        .into_bytes(),
        stream_object("", content.as_bytes()),
        b"<< /Producer (CarveStudio) /Creator (CarveStudio) >>".to_vec(),
    ];
    for layer in &used {
        objects.push(format!("<< /Type /OCG /Name {} >>", pdf_string(&layer.name)).into_bytes());
    }
    write_file(&objects, "/Root 1 0 R /Info 5 0 R")
}

#[cfg(test)]
mod tests {
    use super::super::eps::tests::{assert_same_geometry, sample_layers};
    use super::*;
    use crate::io::svg::MM_PER_PT;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-4
    }

    fn mm(pt: f64) -> f64 {
        pt * MM_PER_PT
    }

    /// A hand written file: the page tree carries the MediaBox, the page
    /// has a Form XObject and an optional content group.
    fn fixture() -> Vec<u8> {
        let content = b"q 1 0 0 1 72 0 cm 0 0 m 72 0 l S Q\n\
            0 0 36 72 re f\n\
            /OC /P1 BDC 0 0 m 10 10 20 10 30 0 c S EMC\n\
            BT /F1 12 Tf 10 10 Td (Hi) Tj ET\n\
            BI /W 1 /H 1 /BPC 8 /CS /G ID \xff EI\n\
            q 2 0 0 2 0 0 cm /Fm1 Do Q\n\
            /Im1 Do\n";
        let form = b"0 0 m 5 0 l 5 5 l h S";
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << \
              /XObject << /Fm1 5 0 R /Im1 7 0 R >> /Properties << /P1 6 0 R >> >> >>"
                .to_vec(),
            stream_object("", content),
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [1 0 0 1 10 10]",
                form,
            ),
            b"<< /Type /OCG /Name <FEFF00440065006D006F> >>".to_vec(),
            stream_object(
                "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 \
                 /ColorSpace /DeviceGray",
                b"\xff",
            ),
        ];
        write_file(&objects, "/Root 1 0 R")
    }

    #[test]
    fn a_hand_written_page_is_read() {
        let (paths, warnings) = import(&fixture(), 1.0, None).expect("imports");
        assert_eq!(paths.len(), 4, "{paths:#?}");
        // cm moved the first line 72 points (one inch) right.
        let line = &paths[0].path.subpaths[0];
        assert!(close(line.start, Point::new(25.4, 0.0)));
        assert!(close(line.segments[0].end(), Point::new(50.8, 0.0)));
        // re then f is a closed rectangle.
        let rect = &paths[1].path.subpaths[0];
        assert!(rect.closed);
        assert_eq!(rect.segments.len(), 4);
        assert!(close(rect.segments[1].end(), Point::new(12.7, 25.4)));
        // The curve sits on the named layer.
        assert_eq!(paths[2].layer, "Demo");
        assert!(matches!(
            paths[2].path.subpaths[0].segments[0],
            crate::geom::Segment::Cubic { .. }
        ));
        assert_eq!(paths[3].layer, DEFAULT_LAYER);
        // The form: its matrix, then the doubling from the page.
        let form = &paths[3].path.subpaths[0];
        assert!(form.closed);
        assert!(close(form.start, Point::new(mm(20.0), mm(20.0))));
        assert!(close(
            form.segments[0].end(),
            Point::new(mm(30.0), mm(20.0))
        ));
        assert_eq!(
            warnings,
            vec![ImportWarning::TextSkipped, ImportWarning::ImageSkipped]
        );
    }

    const FIRA: &[u8] = include_bytes!("../../assets/fonts/FiraSans-Regular.ttf");

    /// A one page file whose resources hold `font` as /F1 (object 4) and
    /// whatever `extra` objects follow it (numbered from 5).
    fn text_file(content: &[u8], font: &str, extra: Vec<Vec<u8>>) -> Vec<u8> {
        let mut objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 99 0 R \
              /Resources << /Font << /F1 4 0 R >> >> >>"
                .to_vec(),
            font.as_bytes().to_vec(),
        ];
        objects.extend(extra);
        let content_id = objects.len() + 1;
        objects.push(stream_object("", content));
        let page = String::from_utf8(objects[2].clone())
            .expect("text")
            .replace("99 0 R", &format!("{content_id} 0 R"));
        objects[2] = page.into_bytes();
        write_file(&objects, "/Root 1 0 R")
    }

    fn bounds(path: &crate::geom::Path) -> BBox {
        crate::geom::path_bbox(path).expect("a glyph has a box")
    }

    fn fira_gid(c: char) -> u16 {
        rustybuzz::ttf_parser::Face::parse(FIRA, 0)
            .expect("face")
            .glyph_index(c)
            .expect("glyph")
            .0
    }

    #[test]
    fn text_in_an_embedded_truetype_font_comes_in_as_glyph_outlines() {
        let font = "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+FiraSans /FirstChar 72 \
            /LastChar 105 /Widths [600 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 250] \
            /Encoding /WinAnsiEncoding /FontDescriptor 5 0 R >>";
        let descriptor = b"<< /Type /FontDescriptor /FontName /ABCDEF+FiraSans /Flags 32 \
            /FontFile2 6 0 R >>"
            .to_vec();
        let content = b"BT /F1 100 Tf 1 0 0 1 72 72 Tm (Hi) Tj ET\n\
            BT /F1 100 Tf 72 200 Td [(H) -400 (i)] TJ ET\n\
            BT 3 Tr /F1 100 Tf 72 300 Td (H) Tj ET";
        let bytes = text_file(content, font, vec![descriptor, stream_object("", FIRA)]);
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(warnings.is_empty(), "{warnings:?}");
        // Two glyphs from Tj, two from TJ; the invisible H draws nothing.
        assert_eq!(paths.len(), 4);
        let h = bounds(&paths[0].path);
        let i = bounds(&paths[1].path);
        // H sits on the baseline at one inch, and is about 0.69 em tall.
        assert!((h.min.y - 25.4).abs() < 0.5, "{h:?}");
        assert!(h.height() > mm(60.0) && h.height() < mm(75.0), "{h:?}");
        // The i starts after the H's width from /Widths, 600/1000 em.
        assert!(
            i.min.x > mm(72.0 + 60.0) && i.min.x < mm(72.0 + 60.0 + 15.0),
            "{i:?}"
        );
        // TJ's -400 pushes the second i 40 points further right.
        let shifted = bounds(&paths[3].path);
        assert!(
            (shifted.min.x - i.min.x - mm(40.0)).abs() < 1e-6,
            "{shifted:?} {i:?}"
        );
        assert!((shifted.min.y - i.min.y - mm(128.0)).abs() < 1e-6);
    }

    #[test]
    fn identity_encoded_composite_fonts_are_read_two_bytes_a_glyph() {
        let font = "<< /Type /Font /Subtype /Type0 /BaseFont /FiraSans /Encoding /Identity-H \
            /DescendantFonts [5 0 R] >>";
        let cid_font = b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /FiraSans \
            /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
            /FontDescriptor 6 0 R /DW 500 /CIDToGIDMap /Identity >>"
            .to_vec();
        let descriptor = b"<< /Type /FontDescriptor /FontName /FiraSans /Flags 32 \
            /FontFile2 7 0 R >>"
            .to_vec();
        let (h, i) = (fira_gid('H'), fira_gid('i'));
        let content = format!("BT /F1 100 Tf 0 0 Td <{h:04X}{i:04X}> Tj ET");
        let bytes = text_file(
            content.as_bytes(),
            font,
            vec![cid_font, descriptor, stream_object("", FIRA)],
        );
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(paths.len(), 2);
        // /DW 500 moves the i half an em along.
        assert!(bounds(&paths[1].path).min.x > mm(50.0));
    }

    #[test]
    fn type1_fonts_are_outlined_from_their_charstrings() {
        let program = super::super::type1::tests::sample(false);
        let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Sample /FirstChar 65 /LastChar 66 \
            /Widths [600 500] /FontDescriptor 5 0 R >>";
        let descriptor = b"<< /Type /FontDescriptor /FontName /Sample /Flags 4 \
            /FontFile 6 0 R >>"
            .to_vec();
        let bytes = text_file(
            b"BT /F1 10 Tf (AB) Tj ET",
            font,
            vec![
                descriptor,
                stream_object("/Length1 0 /Length2 0 /Length3 0", &program),
            ],
        );
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(paths.len(), 2);
        let a = bounds(&paths[0].path);
        assert!(close(a.min, Point::new(mm(0.5), 0.0)), "{a:?}");
        assert!(close(a.max, Point::new(mm(5.5), mm(7.0))), "{a:?}");
        // B follows at A's width, 6 points.
        assert!((bounds(&paths[1].path).min.x - mm(6.0)).abs() < 1e-6);
    }

    /// A bare CFF program with a 400 by 700 unit `A` and a 400 unit square
    /// `B`, made for this test.
    const SAMPLE_CFF: &str = "01000401000101010753616d706c650001010112f81b028b8bf888f95005bd0f8be5\
        12c111000101010753616d706c650000010022010003010106121ef8888b160ef8ecef16f824f950fc24060ef888\
        8b16f824f824fc24060e";

    #[test]
    fn bare_cff_fonts_are_outlined() {
        let cff: Vec<u8> = SAMPLE_CFF
            .as_bytes()
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("ascii"), 16).expect("hex")
            })
            .collect();
        let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Sample /FontDescriptor 5 0 R >>";
        let descriptor =
            b"<< /Type /FontDescriptor /FontName /Sample /Flags 4 /FontFile3 6 0 R >>".to_vec();
        let bytes = text_file(
            b"BT /F1 10 Tf (AB) Tj ET",
            font,
            vec![descriptor, stream_object("/Subtype /Type1C", &cff)],
        );
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(paths.len(), 2);
        let a = bounds(&paths[0].path);
        assert!(close(a.min, Point::new(mm(1.0), 0.0)), "{a:?}");
        assert!(close(a.max, Point::new(mm(5.0), mm(7.0))), "{a:?}");
        // No /Widths: the B follows at the A's own advance, 600 units.
        assert!((bounds(&paths[1].path).min.x - mm(6.0)).abs() < 1e-6);
    }

    #[test]
    fn fonts_that_are_not_embedded_are_drawn_with_an_installed_stand_in() {
        let font =
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
        let bytes = text_file(b"BT /F1 50 Tf 10 10 Td (Hi!) Tj ET", font, Vec::new());
        let library = FontLibrary::bundled();
        let (paths, warnings) =
            import_with_fonts(&bytes, 1.0, None, Some(&library)).expect("imports");
        assert_eq!(paths.len(), 3);
        assert_eq!(warnings, vec![ImportWarning::FontSubstituted]);
        // Without installed fonts the text is left out, and said so.
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(paths.is_empty());
        assert_eq!(warnings, vec![ImportWarning::TextSkipped]);
    }

    #[test]
    fn export_then_import_gives_the_same_geometry_and_layers() {
        let layers = sample_layers();
        let bounds = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 80.0));
        let bytes = export(&layers, bounds);
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("/MediaBox [0 0 283.46457 226.77165]"),
            "{text}"
        );
        let (paths, warnings) = import(&bytes, 1.0, None).expect("imports");
        assert!(warnings.is_empty());
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].layer, "Cut (outside)");
        assert_eq!(paths[1].layer, "Engrave");
        assert_same_geometry(&layers[0].paths[0], &paths[0].path);
        assert_same_geometry(&layers[1].paths[0], &paths[1].path);
    }

    #[test]
    fn export_moves_the_bounds_corner_to_the_page_origin() {
        let mut layers = sample_layers();
        layers.truncate(1);
        layers[0].name = "Gravure \u{e9}t\u{e9}".into();
        let bounds = BBox::new(Point::new(-50.0, -40.0), Point::new(50.0, 40.0));
        let (paths, _) = import(&export(&layers, bounds), 1.0, None).expect("imports");
        assert_eq!(paths[0].layer, "Gravure \u{e9}t\u{e9}");
        let start = paths[0].path.subpaths[0].start;
        assert!(close(start, Point::new(60.0, 50.0)), "{start:?}");
    }

    #[test]
    fn a_page_can_be_chosen() {
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 5 0 R >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [10 10 100 100] /Contents 6 0 R >>".to_vec(),
            stream_object("", b"0 0 m 1 0 l S"),
            stream_object("", b"10 10 m 82 10 l S 10 10 m 10 20 l S"),
        ];
        let bytes = write_file(&objects, "/Root 1 0 R");
        let (first, _) = import(&bytes, 1.0, None).expect("first page");
        assert_eq!(first.len(), 1);
        let (second, _) = import(&bytes, 1.0, Some(2)).expect("second page");
        assert_eq!(second.len(), 2);
        assert!(close(
            second[0].path.subpaths[0].start,
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            second[0].path.subpaths[0].segments[0].end(),
            Point::new(25.4, 0.0)
        ));
        assert_eq!(page_count(&bytes).expect("counts"), 2);
        assert_eq!(page_count(&fixture()).expect("counts"), 1);
        assert!(page_count(b"not a pdf").is_err());
        let error = import(&bytes, 1.0, Some(3)).expect_err("no third page");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
        assert!(error.to_string().contains("2 pages"), "{error}");
    }

    #[test]
    fn form_xobjects_that_draw_themselves_stop() {
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
              /Resources << /XObject << /X 5 0 R >> >> >>"
                .to_vec(),
            stream_object("", b"/X Do"),
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 1 1] \
                 /Resources << /XObject << /X 5 0 R >> >>",
                b"0 0 m 1 0 l S /X Do",
            ),
        ];
        let (paths, _) = import(&write_file(&objects, "/Root 1 0 R"), 1.0, None).expect("imports");
        // Every level draws the same line, so the repeats collapse.
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn malformed_input_is_an_error_and_never_panics() {
        assert!(matches!(
            import(b"not a pdf", 1.0, None),
            Err(CoreError::Format { .. })
        ));
        assert!(import(b"", 1.0, None).is_err());
        assert!(import(b"%PDF-1.4\ngarbage", 1.0, None).is_err());
        let bytes = export(
            &sample_layers(),
            BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 80.0)),
        );
        for cut in (0..bytes.len()).step_by(37) {
            let _ = import(&bytes[..cut], 1.0, None);
        }
        let mut seed = 0x9e37_79b9_u32;
        for _ in 0..50 {
            let mut damaged = bytes.clone();
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let at = seed as usize % damaged.len();
                damaged[at] = (seed >> 8) as u8;
            }
            let _ = import(&damaged, 1.0, None);
        }
    }

    #[test]
    fn strings_are_written_for_pdf() {
        assert_eq!(pdf_string("a(b)"), "(a\\(b\\))");
        assert_eq!(pdf_string("\u{e9}"), "<FEFF00E9>");
        assert_eq!(text_string(&[0xFE, 0xFF, 0x00, 0x41]), "A");
        assert_eq!(text_string(b"\xe9"), "\u{e9}");
        assert!(looks_like_pdf(b"junk%PDF-1.7"));
        assert!(!looks_like_pdf(b"%!PS-Adobe"));
    }

    mod pdf_review_tests {
        use super::*;

        fn type1_file() -> Vec<u8> {
            let program = super::super::super::type1::tests::sample(false);
            let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Sample /FirstChar 65 \
                /LastChar 66 /Widths [600 500] /FontDescriptor 5 0 R >>";
            let descriptor = b"<< /Type /FontDescriptor /FontName /Sample /Flags 4 \
                /FontFile 6 0 R >>"
                .to_vec();
            text_file(
                b"BT /F1 10 Tf (AB) Tj ET",
                font,
                vec![
                    descriptor,
                    stream_object("/Length1 0 /Length2 0 /Length3 0", &program),
                ],
            )
        }

        #[test]
        fn damaged_embedded_type1_fonts_are_errors_or_skipped_text() {
            let bytes = type1_file();
            assert_eq!(import(&bytes, 1.0, None).expect("imports").0.len(), 2);
            for cut in 0..bytes.len() {
                let _ = import(&bytes[..cut], 1.0, None);
            }
            let mut seed = 0x51ed_270b_u32;
            for _ in 0..3000 {
                let mut damaged = bytes.clone();
                for _ in 0..3 {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let at = seed as usize % damaged.len();
                    damaged[at] = (seed >> 8) as u8;
                }
                let _ = import(&damaged, 1.0, None);
            }
        }
    }
}
