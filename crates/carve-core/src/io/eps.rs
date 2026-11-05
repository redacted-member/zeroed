//! Encapsulated PostScript and PostScript Illustrator import and export.
//!
//! Import runs a small PostScript subset: enough to draw paths, not a full
//! interpreter. It understands the path operators (`moveto`, `lineto`,
//! `curveto`, `closepath`, `arc`, `arcn` and the relative forms), the
//! graphics state (`gsave`, `grestore`, `translate`, `scale`, `rotate`,
//! `concat`, `setmatrix`), the painting operators, and the short names that
//! Illustrator and most EPS writers use (`m l c v y h re cm q Q f S` and so
//! on). Procedures a file defines with `/name {...} def` or aliases made with
//! `/name /op load def` are followed, so prologs that wrap the real
//! operators work. Anything else is skipped without failing: text and images
//! are reported as warnings, loops and conditionals are not run.
//!
//! PostScript user space is in points with Y going up, like the job, so a
//! point becomes 25.4 / 72 millimetres and the bounding box corner becomes
//! the job origin.
//!
//! Export writes a Level 2 EPS in points with one layer block per layer, in
//! the form Illustrator writes them (`(name) Ln`), so layer names come back
//! on import. The Illustrator flavour adds the Illustrator header comments.
//!
//! The lexer and the path builder here are shared with the PDF importer,
//! whose content streams use the same token syntax.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::rc::Rc;

use super::svg::{ExportLayer, MM_PER_PT};
use super::{ImportWarning, ImportedPath};
use crate::error::CoreError;
use crate::geom::{
    arc_to_cubic_segments, transform_path, BBox, Mat3, Path, Point, Segment, SubPath, Tolerance,
    Vec2,
};

/// Layer name for vectors a file does not put on a named layer.
pub(super) const DEFAULT_LAYER: &str = "Layer 1";

/// Procedures nested deeper than this are not followed.
const MAX_DEPTH: usize = 32;
/// Operators run before the importer gives up on a file, which stops a
/// file whose procedures call each other from running for ever.
const MAX_OPERATIONS: usize = 10_000_000;
/// Operands kept on the stack. Operators this importer skips leave their
/// operands behind, so the bottom of the stack is dropped past this.
const MAX_STACK: usize = 65_536;

// ------------------------------------------------------------------- lexer

/// One PostScript token.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    /// A number, integer or real, including radix numbers like `16#FF`.
    Number(f64),
    /// An executable name: an operator or a procedure to run.
    Name(String),
    /// A literal name, written with a leading slash.
    Literal(String),
    /// A string, from `(...)` or `<hex>`.
    Str(Vec<u8>),
    /// `[`
    ArrayOpen,
    /// `]`
    ArrayClose,
    /// `{`
    ProcOpen,
    /// `}`
    ProcClose,
    /// `<<`
    DictOpen,
    /// `>>`
    DictClose,
    /// A comment, without its first `%`.
    Comment(String),
}

/// Splits PostScript or PDF content stream bytes into tokens.
pub(super) struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

fn is_white(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

impl<'a> Lexer<'a> {
    /// A lexer at the start of `bytes`.
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Lexer { bytes, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Skips `count` bytes, for binary data a file declares up front.
    pub(super) fn skip_bytes(&mut self, count: usize) {
        self.pos = self.pos.saturating_add(count).min(self.bytes.len());
    }

    /// Skips `count` lines, for data a file declares in lines.
    fn skip_lines(&mut self, count: usize) {
        for _ in 0..count {
            if self.pos >= self.bytes.len() {
                break;
            }
            self.skip_line();
        }
    }

    /// Moves past the end of the current line, taking `\r\n` as one ending.
    fn skip_line(&mut self) {
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'\n' {
                return;
            }
            if byte == b'\r' {
                if self.peek() == Some(b'\n') {
                    self.pos += 1;
                }
                return;
            }
        }
    }

    /// Skips the data of a PDF inline image, which follows `ID` and ends
    /// with `EI` between white space.
    pub(super) fn skip_inline_image(&mut self) {
        // One white space byte separates ID from the data.
        if self.peek().is_some_and(is_white) {
            self.pos += 1;
        }
        let bytes = self.bytes;
        let mut at = self.pos;
        while at + 1 < bytes.len() {
            let before_ok = at == 0 || is_white(bytes[at - 1]);
            let after_ok = bytes.get(at + 2).is_none_or(|b| is_white(*b));
            if before_ok && after_ok && bytes[at] == b'E' && bytes[at + 1] == b'I' {
                self.pos = at + 2;
                return;
            }
            at += 1;
        }
        self.pos = bytes.len();
    }

    /// The next token, or `None` at the end of the input.
    pub(super) fn next_token(&mut self) -> Option<Token> {
        loop {
            while self.peek().is_some_and(is_white) {
                self.pos += 1;
            }
            let byte = self.peek()?;
            self.pos += 1;
            match byte {
                b'%' => {
                    let start = self.pos;
                    let mut end = start;
                    while let Some(b) = self.peek() {
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                        self.pos += 1;
                        end = self.pos;
                    }
                    let text = String::from_utf8_lossy(&self.bytes[start..end]).into_owned();
                    self.skip_line();
                    return Some(Token::Comment(text));
                }
                b'(' => return Some(Token::Str(self.literal_string())),
                b'<' => match self.peek() {
                    Some(b'<') => {
                        self.pos += 1;
                        return Some(Token::DictOpen);
                    }
                    Some(b'~') => {
                        self.skip_ascii85();
                        return Some(Token::Str(Vec::new()));
                    }
                    _ => return Some(Token::Str(self.hex_string())),
                },
                b'>' => {
                    if self.peek() == Some(b'>') {
                        self.pos += 1;
                        return Some(Token::DictClose);
                    }
                    // A stray `>` is not a token; skip it.
                }
                b')' => {}
                b'[' => return Some(Token::ArrayOpen),
                b']' => return Some(Token::ArrayClose),
                b'{' => return Some(Token::ProcOpen),
                b'}' => return Some(Token::ProcClose),
                b'/' => {
                    // `//name` is an immediately evaluated name; treat it
                    // like a literal.
                    if self.peek() == Some(b'/') {
                        self.pos += 1;
                    }
                    return Some(Token::Literal(self.regular()));
                }
                _ => {
                    self.pos -= 1;
                    let text = self.regular();
                    if text.is_empty() {
                        self.pos += 1;
                        continue;
                    }
                    return Some(match parse_number(&text) {
                        Some(n) => Token::Number(n),
                        None => Token::Name(text),
                    });
                }
            }
        }
    }

    /// Reads regular characters up to white space or a delimiter.
    fn regular(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if is_white(b) || is_delimiter(b) {
                break;
            }
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned()
    }

    /// Reads a `(...)` string after its opening bracket.
    fn literal_string(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut depth = 1usize;
        while let Some(byte) = self.peek() {
            self.pos += 1;
            match byte {
                b'(' => {
                    depth += 1;
                    out.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(byte);
                }
                b'\\' => {
                    let Some(next) = self.peek() else { break };
                    self.pos += 1;
                    match next {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut value = u32::from(next - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        self.pos += 1;
                                        value = value * 8 + u32::from(d - b'0');
                                    }
                                    _ => break,
                                }
                            }
                            out.push((value & 0xff) as u8);
                        }
                        other => out.push(other),
                    }
                }
                _ => out.push(byte),
            }
        }
        out
    }

    /// Reads a `<hex>` string after its opening bracket.
    fn hex_string(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut high: Option<u8> = None;
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'>' {
                break;
            }
            let Some(nibble) = (byte as char).to_digit(16) else {
                continue;
            };
            let nibble = nibble as u8;
            match high.take() {
                Some(h) => out.push(h << 4 | nibble),
                None => high = Some(nibble),
            }
        }
        if let Some(h) = high {
            out.push(h << 4);
        }
        out
    }

    /// Skips an ASCII85 string after its `<~`.
    fn skip_ascii85(&mut self) {
        self.pos += 1;
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'~' && self.peek() == Some(b'>') {
                self.pos += 1;
                return;
            }
        }
    }
}

/// A PostScript number: decimal, real with exponent, or `base#digits`.
fn parse_number(text: &str) -> Option<f64> {
    if let Some((base, digits)) = text.split_once('#') {
        let base: u32 = base.parse().ok()?;
        if !(2..=36).contains(&base) {
            return None;
        }
        return i64::from_str_radix(digits, base).ok().map(|n| n as f64);
    }
    let first = text.as_bytes().first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

// ------------------------------------------------------------ path builder

/// Collects the current path the way PostScript and PDF build it, in page
/// points after the current transform.
#[derive(Debug, Clone, Default)]
pub(super) struct PathBuilder {
    done: Vec<SubPath>,
    current: Option<SubPath>,
    point: Option<Point>,
}

impl PathBuilder {
    /// The current point, if there is one.
    pub(super) fn current_point(&self) -> Option<Point> {
        self.point
    }

    fn finish_current(&mut self) {
        if let Some(sub) = self.current.take() {
            if !sub.segments.is_empty() {
                self.done.push(sub);
            }
        }
    }

    /// Starts a new subpath at `p`.
    pub(super) fn move_to(&mut self, p: Point) {
        self.finish_current();
        self.current = Some(SubPath::open(p));
        self.point = Some(p);
    }

    /// The subpath to append to, starting one at the current point after a
    /// `closepath`. `None` when there is no current point.
    fn open_subpath(&mut self) -> Option<&mut SubPath> {
        if self.current.is_none() {
            let start = self.point?;
            self.current = Some(SubPath::open(start));
        }
        self.current.as_mut()
    }

    /// A straight line to `p`. Ignored without a current point.
    pub(super) fn line_to(&mut self, p: Point) {
        if let Some(sub) = self.open_subpath() {
            sub.line_to(p);
            self.point = Some(p);
        }
    }

    /// A cubic Bezier to `p`. Ignored without a current point.
    pub(super) fn curve_to(&mut self, c1: Point, c2: Point, p: Point) {
        if let Some(sub) = self.open_subpath() {
            sub.cubic_to(c1, c2, p);
            self.point = Some(p);
        }
    }

    /// Closes the current subpath with a line back to its start.
    pub(super) fn close(&mut self) {
        if let Some(mut sub) = self.current.take() {
            if !sub.segments.is_empty() {
                close_exactly(&mut sub);
                self.point = Some(sub.start);
                self.done.push(sub);
            } else {
                self.point = Some(sub.start);
            }
        }
    }

    /// A closed rectangle, as `re` draws it.
    pub(super) fn rect(&mut self, corners: [Point; 4]) {
        self.move_to(corners[0]);
        for corner in &corners[1..] {
            self.line_to(*corner);
        }
        self.close();
    }

    /// Takes the finished path and starts a new one. With `close_all` every
    /// open subpath is closed, which is what painting a fill does.
    pub(super) fn take(&mut self, close_all: bool) -> Option<Path> {
        self.finish_current();
        let mut subpaths = std::mem::take(&mut self.done);
        self.point = None;
        if close_all {
            for sub in &mut subpaths {
                if !sub.closed && encloses_area(sub) {
                    close_exactly(sub);
                }
            }
        }
        subpaths.retain(|sub| !sub.segments.is_empty());
        if subpaths.is_empty() {
            None
        } else {
            Some(Path::from_subpaths(subpaths))
        }
    }

    /// Drops the current path, as `newpath` does.
    pub(super) fn clear(&mut self) {
        self.done.clear();
        self.current = None;
        self.point = None;
    }
}

/// True unless the subpath is a single straight line, which has no inside.
fn encloses_area(sub: &SubPath) -> bool {
    !(sub.segments.len() == 1 && matches!(sub.segments[0], Segment::Line { .. }))
}

/// Closes a subpath so that its last segment ends exactly on its start.
fn close_exactly(sub: &mut SubPath) {
    sub.close_with_line(Tolerance::EQUALITY);
    let start = sub.start;
    if let Some(last) = sub.segments.last_mut() {
        last.set_end(start);
    }
}

/// Cubic segments for a circular arc in user space, from `a1` to `a2`
/// degrees, counter clockwise unless `clockwise`.
pub(super) fn arc_points(
    centre: Point,
    radius: f64,
    a1: f64,
    a2: f64,
    clockwise: bool,
) -> (Point, Vec<Segment>) {
    let (start, mut end) = (a1.to_radians(), a2.to_radians());
    let turn = std::f64::consts::TAU;
    if clockwise {
        while end > start {
            end -= turn;
        }
        while start - end > turn {
            end += turn;
        }
    } else {
        while end < start {
            end += turn;
        }
        while end - start > turn {
            end -= turn;
        }
    }
    let at = |angle: f64| {
        Point::new(
            centre.x + radius * angle.cos(),
            centre.y + radius * angle.sin(),
        )
    };
    let from = at(start);
    let sweep = end - start;
    if sweep.abs() < 1e-12 || radius <= 0.0 {
        return (from, Vec::new());
    }
    // A full turn cannot be one bulge, so draw it as two halves.
    let halves = if sweep.abs() > std::f64::consts::PI {
        2
    } else {
        1
    };
    let mut segments = Vec::new();
    let mut here = from;
    for i in 0..halves {
        let a = start + sweep * f64::from(i + 1) / f64::from(halves);
        let to = at(a);
        let bulge = (sweep / f64::from(halves) / 4.0).tan();
        segments.extend(arc_to_cubic_segments(here, to, bulge));
        here = to;
    }
    (from, segments)
}

/// Maps one segment through a transform.
pub(super) fn map_segment(m: &Mat3, segment: &Segment) -> Segment {
    match *segment {
        Segment::Line { to } => Segment::Line { to: m.apply(to) },
        Segment::Arc { to, bulge } => Segment::Arc {
            to: m.apply(to),
            bulge,
        },
        Segment::Cubic { c1, c2, to } => Segment::Cubic {
            c1: m.apply(c1),
            c2: m.apply(c2),
            to: m.apply(to),
        },
    }
}

/// Adds a path to the output unless it repeats the one before it, which is
/// what `gsave fill grestore stroke` produces.
pub(super) fn push_path(out: &mut Vec<ImportedPath>, layer: &str, path: Path) {
    if let Some(last) = out.last() {
        if last.layer == layer {
            let mut closed = path.clone();
            for sub in &mut closed.subpaths {
                if !sub.closed && encloses_area(sub) {
                    close_exactly(sub);
                }
            }
            if last.path == path || last.path == closed {
                return;
            }
        }
    }
    out.push(ImportedPath {
        layer: layer.to_string(),
        name: None,
        path,
    });
}

/// Adds a warning once.
pub(super) fn warn(warnings: &mut Vec<ImportWarning>, warning: ImportWarning) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

/// The transform from page points with the given origin to job millimetres.
pub(super) fn page_to_mm(origin: Point, scale: f64) -> Mat3 {
    let scale = if scale.is_finite() && scale != 0.0 {
        scale
    } else {
        1.0
    };
    Mat3::translate(Vec2::new(-origin.x, -origin.y))
        .then(&Mat3::scale(MM_PER_PT * scale, MM_PER_PT * scale))
}

// ------------------------------------------------------------- interpreter

#[derive(Debug, Clone)]
enum Value {
    Num(f64),
    Str(Vec<u8>),
    Literal(String),
    Array(Vec<Value>),
    Proc(Rc<Vec<Token>>),
    /// An operator pushed by `load`, run when a name bound to it is used.
    Operator(String),
    Mark,
    Other,
}

#[derive(Debug, Clone, Default)]
struct GState {
    ctm: Mat3,
    path: PathBuilder,
}

struct Interpreter {
    stack: Vec<Value>,
    gs: GState,
    saved: Vec<GState>,
    dict: BTreeMap<String, Value>,
    layer: String,
    out: Vec<ImportedPath>,
    warnings: Vec<ImportWarning>,
    budget: usize,
    bbox: Option<[f64; 4]>,
    hires: Option<[f64; 4]>,
}

/// What a comment asked the lexer to do.
enum CommentAction {
    None,
    SkipBytes(usize),
    SkipLines(usize),
}

impl Interpreter {
    fn new() -> Self {
        Interpreter {
            stack: Vec::new(),
            gs: GState::default(),
            saved: Vec::new(),
            dict: BTreeMap::new(),
            layer: DEFAULT_LAYER.to_string(),
            out: Vec::new(),
            warnings: Vec::new(),
            budget: MAX_OPERATIONS,
            bbox: None,
            hires: None,
        }
    }

    fn push(&mut self, value: Value) {
        if self.stack.len() >= MAX_STACK {
            self.stack.drain(..MAX_STACK / 2);
        }
        self.stack.push(value);
    }

    /// Pops `N` numbers. When the top of the stack does not hold them the
    /// operator is skipped and whatever was there is dropped.
    fn nums<const N: usize>(&mut self) -> Option<[f64; N]> {
        if self.stack.len() < N {
            self.stack.clear();
            return None;
        }
        let tail = self.stack.split_off(self.stack.len() - N);
        let mut out = [0.0; N];
        for (slot, value) in out.iter_mut().zip(tail) {
            match value {
                Value::Num(n) => *slot = n,
                _ => return None,
            }
        }
        Some(out)
    }

    fn pop(&mut self) -> Option<Value> {
        self.stack.pop()
    }

    fn user(&self, x: f64, y: f64) -> Point {
        self.gs.ctm.apply(Point::new(x, y))
    }

    fn relative(&self, dx: f64, dy: f64) -> Option<Point> {
        let here = self.gs.path.current_point()?;
        let d = self.gs.ctm.apply_vector(Vec2::new(dx, dy));
        Some(Point::new(here.x + d.x, here.y + d.y))
    }

    fn concat(&mut self, m: [f64; 6]) {
        let m = Mat3::new(m[0], m[1], m[2], m[3], m[4], m[5]);
        self.gs.ctm = m.then(&self.gs.ctm);
    }

    fn matrix_operand(&mut self) -> Option<[f64; 6]> {
        match self.pop()? {
            Value::Array(items) if items.len() == 6 => {
                let mut out = [0.0; 6];
                for (slot, item) in out.iter_mut().zip(items) {
                    match item {
                        Value::Num(n) => *slot = n,
                        _ => return None,
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn paint(&mut self, close_first: bool, fill: bool) {
        if close_first {
            self.gs.path.close();
        }
        if let Some(path) = self.gs.path.take(fill) {
            push_path(&mut self.out, &self.layer, path);
        }
    }

    fn emit_rect(&mut self, fill: bool) {
        if let Some([x, y, w, h]) = self.nums::<4>() {
            let saved = std::mem::take(&mut self.gs.path);
            self.gs.path.rect([
                self.user(x, y),
                self.user(x + w, y),
                self.user(x + w, y + h),
                self.user(x, y + h),
            ]);
            self.paint(false, fill);
            self.gs.path = saved;
        }
    }

    fn arc(&mut self, clockwise: bool) {
        let Some([x, y, r, a1, a2]) = self.nums::<5>() else {
            return;
        };
        let (from, segments) = arc_points(Point::new(x, y), r, a1, a2, clockwise);
        let from = self.gs.ctm.apply(from);
        if self.gs.path.current_point().is_some() {
            self.gs.path.line_to(from);
        } else {
            self.gs.path.move_to(from);
        }
        for segment in &segments {
            match map_segment(&self.gs.ctm, segment) {
                Segment::Cubic { c1, c2, to } => self.gs.path.curve_to(c1, c2, to),
                other => self.gs.path.line_to(other.end()),
            }
        }
    }

    /// Runs an operator this importer knows. Returns false for any other
    /// name, so the caller can look in the file's own definitions.
    fn builtin(&mut self, name: &str) -> bool {
        match name {
            "moveto" | "m" => {
                if let Some([x, y]) = self.nums() {
                    let p = self.user(x, y);
                    self.gs.path.move_to(p);
                }
            }
            "lineto" | "l" | "L" => {
                if let Some([x, y]) = self.nums() {
                    let p = self.user(x, y);
                    self.gs.path.line_to(p);
                }
            }
            "curveto" | "c" | "C" => {
                if let Some([x1, y1, x2, y2, x3, y3]) = self.nums() {
                    let (c1, c2, p) = (self.user(x1, y1), self.user(x2, y2), self.user(x3, y3));
                    self.gs.path.curve_to(c1, c2, p);
                }
            }
            "v" | "V" => {
                if let Some([x2, y2, x3, y3]) = self.nums() {
                    if let Some(c1) = self.gs.path.current_point() {
                        let (c2, p) = (self.user(x2, y2), self.user(x3, y3));
                        self.gs.path.curve_to(c1, c2, p);
                    }
                }
            }
            "y" | "Y" => {
                if let Some([x1, y1, x3, y3]) = self.nums() {
                    let (c1, p) = (self.user(x1, y1), self.user(x3, y3));
                    self.gs.path.curve_to(c1, p, p);
                }
            }
            "rmoveto" => {
                if let Some([dx, dy]) = self.nums() {
                    if let Some(p) = self.relative(dx, dy) {
                        self.gs.path.move_to(p);
                    }
                }
            }
            "rlineto" => {
                if let Some([dx, dy]) = self.nums() {
                    if let Some(p) = self.relative(dx, dy) {
                        self.gs.path.line_to(p);
                    }
                }
            }
            "rcurveto" => {
                if let Some([x1, y1, x2, y2, x3, y3]) = self.nums() {
                    if let (Some(c1), Some(c2), Some(p)) = (
                        self.relative(x1, y1),
                        self.relative(x2, y2),
                        self.relative(x3, y3),
                    ) {
                        self.gs.path.curve_to(c1, c2, p);
                    }
                }
            }
            "closepath" | "h" => self.gs.path.close(),
            "re" => {
                if let Some([x, y, w, h]) = self.nums() {
                    let corners = [
                        self.user(x, y),
                        self.user(x + w, y),
                        self.user(x + w, y + h),
                        self.user(x, y + h),
                    ];
                    self.gs.path.rect(corners);
                }
            }
            "arc" => self.arc(false),
            "arcn" => self.arc(true),
            "newpath" | "n" | "N" => self.gs.path.clear(),
            "fill" | "eofill" | "f" | "F" | "f*" | "B" | "B*" => self.paint(false, true),
            "b" | "b*" => self.paint(true, true),
            "stroke" | "S" => self.paint(false, false),
            "s" => self.paint(true, false),
            "rectfill" => self.emit_rect(true),
            "rectstroke" => self.emit_rect(false),
            "clip" | "eoclip" | "W" | "W*" | "rectclip" => {
                if name == "rectclip" {
                    let _ = self.nums::<4>();
                }
            }
            "gsave" | "q" => self.saved.push(self.gs.clone()),
            "save" => {
                self.saved.push(self.gs.clone());
                self.push(Value::Other);
            }
            "grestore" | "Q" | "restore" => {
                if name == "restore" {
                    let _ = self.pop();
                }
                if let Some(gs) = self.saved.pop() {
                    self.gs = gs;
                }
            }
            "grestoreall" => {
                if let Some(first) = self.saved.drain(..).next() {
                    self.gs = first;
                }
            }
            "translate" => {
                if let Some([tx, ty]) = self.nums() {
                    self.concat([1.0, 0.0, 0.0, 1.0, tx, ty]);
                }
            }
            "scale" => {
                if let Some([sx, sy]) = self.nums() {
                    self.concat([sx, 0.0, 0.0, sy, 0.0, 0.0]);
                }
            }
            "rotate" => {
                if let Some([degrees]) = self.nums() {
                    let (s, c) = degrees.to_radians().sin_cos();
                    self.concat([c, s, -s, c, 0.0, 0.0]);
                }
            }
            "concat" => {
                if let Some(m) = self.matrix_operand() {
                    self.concat(m);
                }
            }
            "cm" => {
                if let Some(m) = self.nums::<6>() {
                    self.concat(m);
                }
            }
            "setmatrix" => {
                if let Some(m) = self.matrix_operand() {
                    self.gs.ctm = Mat3::new(m[0], m[1], m[2], m[3], m[4], m[5]);
                }
            }
            "initmatrix" | "initgraphics" => self.gs.ctm = Mat3::IDENTITY,
            "matrix" => self.push(Value::Array(
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
                    .into_iter()
                    .map(Value::Num)
                    .collect(),
            )),
            "show" | "ashow" | "widthshow" | "awidthshow" | "kshow" | "xshow" | "yshow"
            | "xyshow" | "glyphshow" | "cshow" | "charpath" | "Tx" | "TX" | "Tj" | "TJ" | "Tk" => {
                warn(&mut self.warnings, ImportWarning::TextSkipped);
            }
            "image" | "imagemask" | "colorimage" | "XI" => {
                warn(&mut self.warnings, ImportWarning::ImageSkipped);
            }
            "Ln" => {
                if let Some(Value::Str(bytes)) = self.pop() {
                    let name = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !name.is_empty() {
                        self.layer = name;
                    }
                }
            }
            "def" => {
                let value = self.pop();
                if let (Some(Value::Literal(key)), Some(value)) = (self.pop(), value) {
                    self.dict.insert(key, value);
                }
            }
            "load" => {
                if let Some(Value::Literal(key)) = self.pop() {
                    let value = self.dict.get(&key).cloned().unwrap_or(Value::Operator(key));
                    self.push(value);
                }
            }
            "bind" | "showpage" | "begin" | "end" => {
                if name == "begin" {
                    let _ = self.pop();
                }
            }
            "exec" => {
                if let Some(Value::Proc(body)) = self.pop() {
                    self.run_tokens(&body, 1);
                }
            }
            "pop" => {
                let _ = self.pop();
            }
            "exch" => {
                let n = self.stack.len();
                if n >= 2 {
                    self.stack.swap(n - 1, n - 2);
                }
            }
            "dup" => {
                if let Some(top) = self.stack.last().cloned() {
                    self.push(top);
                }
            }
            "neg" => {
                if let Some([a]) = self.nums() {
                    self.push(Value::Num(-a));
                }
            }
            "add" | "sub" | "mul" | "div" => {
                if let Some([a, b]) = self.nums() {
                    let r = match name {
                        "add" => a + b,
                        "sub" => a - b,
                        "mul" => a * b,
                        _ => a / b,
                    };
                    self.push(Value::Num(if r.is_finite() { r } else { 0.0 }));
                }
            }
            "index" => {
                if let Some([n]) = self.nums() {
                    let len = self.stack.len();
                    if n >= 0.0 && (n as usize) < len {
                        let value = self.stack[len - 1 - n as usize].clone();
                        self.push(value);
                    }
                }
            }
            "roll" => {
                if let Some([n, j]) = self.nums() {
                    let len = self.stack.len();
                    if n > 0.0 && (n as usize) <= len && j.abs() < 1e9 {
                        let n = n as usize;
                        let slice = &mut self.stack[len - n..];
                        let shift = (j as i64).rem_euclid(n as i64) as usize;
                        slice.rotate_right(shift);
                    }
                }
            }
            "mark" => self.push(Value::Mark),
            "cleartomark" => {
                while let Some(value) = self.pop() {
                    if matches!(value, Value::Mark) {
                        break;
                    }
                }
            }
            "array" => {
                if let Some([n]) = self.nums() {
                    let n = if n.is_sign_positive() { n as usize } else { 0 };
                    self.push(Value::Array(vec![Value::Other; n.min(MAX_STACK)]));
                }
            }
            "astore" => {
                if let Some(Value::Array(items)) = self.pop() {
                    let n = items.len();
                    if n <= self.stack.len() {
                        let values = self.stack.split_off(self.stack.len() - n);
                        self.push(Value::Array(values));
                    }
                }
            }
            "if" => {
                let _ = self.pop();
                let _ = self.pop();
            }
            "ifelse" => {
                let _ = self.pop();
                let _ = self.pop();
                let _ = self.pop();
            }
            _ => return false,
        }
        true
    }

    fn execute_name(&mut self, name: &str, depth: usize) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        if self.builtin(name) {
            return;
        }
        match self.dict.get(name).cloned() {
            Some(Value::Proc(body)) => {
                if depth < MAX_DEPTH {
                    self.run_tokens(&body, depth + 1);
                }
            }
            Some(Value::Operator(op)) => {
                if depth < MAX_DEPTH && op != name {
                    self.execute_name(&op, depth + 1);
                }
            }
            Some(value @ (Value::Num(_) | Value::Str(_) | Value::Array(_) | Value::Literal(_))) => {
                self.push(value);
            }
            _ => {}
        }
    }

    fn close_array(&mut self) {
        let mut items = Vec::new();
        while let Some(value) = self.pop() {
            if matches!(value, Value::Mark) {
                break;
            }
            items.push(value);
        }
        items.reverse();
        self.push(Value::Array(items));
    }

    fn close_dict(&mut self) {
        while let Some(value) = self.pop() {
            if matches!(value, Value::Mark) {
                break;
            }
        }
        self.push(Value::Other);
    }

    /// Runs one token that is not the start of a procedure.
    fn run_token(&mut self, token: &Token, depth: usize) {
        match token {
            Token::Number(n) => self.push(Value::Num(*n)),
            Token::Str(bytes) => self.push(Value::Str(bytes.clone())),
            Token::Literal(name) => self.push(Value::Literal(name.clone())),
            Token::ArrayOpen | Token::DictOpen => self.push(Value::Mark),
            Token::ArrayClose => self.close_array(),
            Token::DictClose => self.close_dict(),
            Token::Name(name) => self.execute_name(name, depth),
            Token::ProcOpen | Token::ProcClose | Token::Comment(_) => {}
        }
    }

    /// Runs the body of a procedure.
    fn run_tokens(&mut self, tokens: &[Token], depth: usize) {
        let mut index = 0;
        while index < tokens.len() {
            if tokens[index] == Token::ProcOpen {
                let mut nested = 1usize;
                let start = index + 1;
                let mut end = start;
                while end < tokens.len() {
                    match tokens[end] {
                        Token::ProcOpen => nested += 1,
                        Token::ProcClose => {
                            nested -= 1;
                            if nested == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                let body = tokens[start..end.min(tokens.len())].to_vec();
                self.push(Value::Proc(Rc::new(body)));
                index = end + 1;
                continue;
            }
            self.run_token(&tokens[index], depth);
            index += 1;
        }
    }

    fn comment(&mut self, text: &str) -> CommentAction {
        let Some(rest) = text.strip_prefix('%') else {
            if text.starts_with("AI5_BeginRaster") {
                warn(&mut self.warnings, ImportWarning::ImageSkipped);
            }
            return CommentAction::None;
        };
        let (key, value) = rest.split_once(':').unwrap_or((rest, ""));
        let numbers = || -> Option<[f64; 4]> {
            let parts: Vec<f64> = value
                .split_whitespace()
                .map(|p| p.parse::<f64>().ok().filter(|n| n.is_finite()))
                .collect::<Option<Vec<f64>>>()?;
            (parts.len() == 4).then(|| [parts[0], parts[1], parts[2], parts[3]])
        };
        match key.trim() {
            "BoundingBox" => {
                if let Some(b) = numbers() {
                    self.bbox = Some(b);
                }
            }
            "HiResBoundingBox" => {
                if let Some(b) = numbers() {
                    self.hires = Some(b);
                }
            }
            "BeginBinary" => {
                if let Some(Ok(n)) = value.split_whitespace().next().map(str::parse::<usize>) {
                    return CommentAction::SkipBytes(n);
                }
            }
            "BeginData" => {
                let mut parts = value.split_whitespace();
                if let Some(Ok(n)) = parts.next().map(str::parse::<usize>) {
                    let _kind = parts.next();
                    return if parts.next() == Some("Lines") {
                        CommentAction::SkipLines(n)
                    } else {
                        CommentAction::SkipBytes(n)
                    };
                }
            }
            _ => {}
        }
        CommentAction::None
    }

    /// Runs a whole file from the lexer.
    fn run(&mut self, lexer: &mut Lexer) {
        while let Some(token) = lexer.next_token() {
            if self.budget == 0 {
                break;
            }
            match token {
                Token::Comment(text) => match self.comment(&text) {
                    CommentAction::None => {}
                    CommentAction::SkipBytes(n) => lexer.skip_bytes(n),
                    CommentAction::SkipLines(n) => lexer.skip_lines(n),
                },
                Token::ProcOpen => {
                    let mut body = Vec::new();
                    let mut nested = 1usize;
                    while let Some(inner) = lexer.next_token() {
                        match inner {
                            Token::ProcOpen => nested += 1,
                            Token::ProcClose => {
                                nested -= 1;
                                if nested == 0 {
                                    break;
                                }
                            }
                            Token::Comment(_) => continue,
                            _ => {}
                        }
                        body.push(inner);
                    }
                    self.push(Value::Proc(Rc::new(body)));
                }
                other => self.run_token(&other, 0),
            }
        }
    }
}

/// The PostScript part of a file, taking it out of a DOS EPS binary
/// wrapper when there is one.
fn postscript_section(bytes: &[u8]) -> Result<&[u8], CoreError> {
    if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        let word = |at: usize| -> Option<usize> {
            let b = bytes.get(at..at + 4)?;
            Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let (Some(offset), Some(length)) = (word(4), word(8)) else {
            return Err(CoreError::format("EPS", "the binary header is cut short"));
        };
        return offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
            .ok_or_else(|| CoreError::format("EPS", "the binary header points past the file"));
    }
    Ok(bytes)
}

/// Reads vectors out of an EPS file or a PostScript based Illustrator file.
///
/// `scale` multiplies every length after points become millimetres. The
/// lower left corner of the bounding box lands on the job origin.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not PostScript.
pub fn import(
    bytes: &[u8],
    scale: f64,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let body = postscript_section(bytes)?;
    let start = body.iter().position(|b| !is_white(*b)).unwrap_or(0);
    if !body[start..].starts_with(b"%!") {
        return Err(CoreError::format(
            "EPS",
            "this is not a PostScript file: it does not start with %!",
        ));
    }
    let mut interpreter = Interpreter::new();
    interpreter.run(&mut Lexer::new(body));
    if interpreter.budget == 0 {
        return Err(CoreError::format(
            "EPS",
            "the file runs too many operations to import",
        ));
    }
    let origin = interpreter
        .hires
        .or(interpreter.bbox)
        .map_or(Point::new(0.0, 0.0), |b| {
            Point::new(b[0].min(b[2]), b[1].min(b[3]))
        });
    let to_mm = page_to_mm(origin, scale);
    let paths = interpreter
        .out
        .into_iter()
        .map(|imported| ImportedPath {
            path: transform_path(&imported.path, &to_mm),
            ..imported
        })
        .collect();
    Ok((paths, interpreter.warnings))
}

// ------------------------------------------------------------------ export

/// Which header an EPS export carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    /// Plain Encapsulated PostScript.
    Eps,
    /// EPS with the header comments Illustrator writes, saved as `.ai`.
    Illustrator,
}

/// Points in one millimetre.
pub(super) const PT_PER_MM: f64 = 72.0 / 25.4;

/// Writes vectors as Encapsulated PostScript in points.
///
/// The page covers `bounds`, with its lower left corner at the PostScript
/// origin. Arcs are written as cubic Beziers.
pub fn export(layers: &[ExportLayer], bounds: BBox, flavour: Flavour) -> String {
    let width = bounds.width().max(1.0) * PT_PER_MM;
    let height = bounds.height().max(1.0) * PT_PER_MM;
    let used: Vec<&ExportLayer> = layers.iter().filter(|l| !l.paths.is_empty()).collect();
    let mut out = String::with_capacity(4096);
    out.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    out.push_str("%%Creator: CarveStudio\n");
    let _ = writeln!(
        out,
        "%%BoundingBox: 0 0 {} {}",
        width.ceil() as i64,
        height.ceil() as i64
    );
    let _ = writeln!(
        out,
        "%%HiResBoundingBox: 0 0 {} {}",
        number(width),
        number(height)
    );
    out.push_str("%%LanguageLevel: 2\n");
    out.push_str("%%DocumentData: Clean7Bit\n");
    out.push_str("%%Pages: 1\n");
    if flavour == Flavour::Illustrator {
        out.push_str("%AI3_ColorUsage: Color\n");
        let _ = writeln!(out, "%AI5_ArtSize: {} {}", number(width), number(height));
        out.push_str("%AI5_RulerUnits: 1\n");
        let _ = writeln!(out, "%AI5_NumLayers: {}", used.len());
    }
    out.push_str("%%EndComments\n");
    out.push_str("%%BeginProlog\n");
    out.push_str("/CarveDict 12 dict def\nCarveDict begin\n");
    out.push_str("/m {moveto} bind def\n/l {lineto} bind def\n/c {curveto} bind def\n");
    out.push_str("/h {closepath} bind def\n/n {newpath} bind def\n/S {stroke} bind def\n");
    out.push_str("/Ln {pop} bind def\nend\n");
    out.push_str("%%EndProlog\n");
    out.push_str("%%Page: 1 1\n");
    out.push_str("CarveDict begin\ngsave\n");
    out.push_str("0.28346 setlinewidth 1 setlinejoin 1 setlinecap\n");
    let to_pt = Mat3::translate(Vec2::new(-bounds.min.x, -bounds.min.y))
        .then(&Mat3::scale(PT_PER_MM, PT_PER_MM));
    for layer in used {
        out.push_str("%AI5_BeginLayer\n");
        let _ = writeln!(out, "({}) Ln", ps_string(&layer.name));
        let (r, g, b) = rgb(&layer.colour);
        let _ = writeln!(out, "{} {} {} setrgbcolor", number(r), number(g), number(b));
        for path in &layer.paths {
            let body = path_operators(path, &to_pt, "m", "l", "c", "h");
            if body.is_empty() {
                continue;
            }
            out.push_str("n\n");
            out.push_str(&body);
            out.push_str("S\n");
        }
        out.push_str("%AI5_EndLayer--\n");
    }
    out.push_str("grestore\nend\nshowpage\n%%Trailer\n%%EOF\n");
    out
}

/// Writes a path as move, line, curve and close operators, one per line,
/// mapping points through `m`. Arcs become cubics. A closed subpath leaves
/// out a closing straight line and lets the close operator draw it.
pub(super) fn path_operators(
    path: &Path,
    m: &Mat3,
    move_op: &str,
    line_op: &str,
    curve_op: &str,
    close_op: &str,
) -> String {
    let mut out = String::new();
    for sub in &path.subpaths {
        if sub.segments.is_empty() {
            continue;
        }
        let start = m.apply(sub.start);
        let _ = writeln!(out, "{} {} {move_op}", number(start.x), number(start.y));
        let mut segments: Vec<Segment> = Vec::with_capacity(sub.segments.len());
        for (from, segment) in sub.iter() {
            match *segment {
                Segment::Arc { to, bulge } => {
                    segments.extend(arc_to_cubic_segments(from, to, bulge));
                }
                other => segments.push(other),
            }
        }
        if sub.closed && matches!(segments.last(), Some(Segment::Line { .. })) {
            segments.pop();
        }
        for segment in &segments {
            match map_segment(m, segment) {
                Segment::Line { to } => {
                    let _ = writeln!(out, "{} {} {line_op}", number(to.x), number(to.y));
                }
                Segment::Cubic { c1, c2, to } => {
                    let _ = writeln!(
                        out,
                        "{} {} {} {} {} {} {curve_op}",
                        number(c1.x),
                        number(c1.y),
                        number(c2.x),
                        number(c2.y),
                        number(to.x),
                        number(to.y)
                    );
                }
                Segment::Arc { to, .. } => {
                    let _ = writeln!(out, "{} {} {line_op}", number(to.x), number(to.y));
                }
            }
        }
        if sub.closed {
            out.push_str(close_op);
            out.push('\n');
        }
    }
    out
}

/// A number with at most five decimals, which is well under a micron in
/// points, without trailing zeros.
pub(super) fn number(value: f64) -> String {
    let text = format!("{value:.5}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A layer colour `#rrggbb` as fractions, black when it cannot be read.
pub(super) fn rgb(colour: &str) -> (f64, f64, f64) {
    let hex = colour.trim().trim_start_matches('#');
    let channel = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .map(|v| f64::from(v) / 255.0)
    };
    match (hex.len(), channel(0), channel(2), channel(4)) {
        (6, Some(r), Some(g), Some(b)) => (r, g, b),
        _ => (0.0, 0.0, 0.0),
    }
}

/// Text for inside a PostScript `(...)` string: brackets and backslashes
/// escaped, anything outside printable ASCII as octal bytes.
pub(super) fn ps_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            _ => {
                let _ = write!(out, "\\{byte:03o}");
            }
        }
    }
    out
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::geom::{arcs_to_cubics, Tolerance};

    fn tokens(text: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(text.as_bytes());
        std::iter::from_fn(|| lexer.next_token()).collect()
    }

    fn eps(body: &str) -> String {
        format!("%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 200 200\n{body}\n")
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-4
    }

    fn mm(pt: f64) -> f64 {
        pt * MM_PER_PT
    }

    #[test]
    fn the_lexer_reads_numbers_names_and_strings() {
        assert_eq!(
            tokens("12 -3.5 .5 1e2 16#ff /name name (a(b)c\\)\\101) <48 69> [ ] { } << >> % hi"),
            vec![
                Token::Number(12.0),
                Token::Number(-3.5),
                Token::Number(0.5),
                Token::Number(100.0),
                Token::Number(255.0),
                Token::Literal("name".into()),
                Token::Name("name".into()),
                Token::Str(b"a(b)c)A".to_vec()),
                Token::Str(b"Hi".to_vec()),
                Token::ArrayOpen,
                Token::ArrayClose,
                Token::ProcOpen,
                Token::ProcClose,
                Token::DictOpen,
                Token::DictClose,
                Token::Comment(" hi".into()),
            ]
        );
        assert_eq!(
            tokens("f* -inf"),
            vec![Token::Name("f*".into()), Token::Name("-inf".into())]
        );
    }

    #[test]
    fn inline_image_data_is_skipped() {
        let mut lexer = Lexer::new(b"BI /W 1 ID \x00(\xff EI 5 m");
        while let Some(token) = lexer.next_token() {
            if token == Token::Name("ID".into()) {
                lexer.skip_inline_image();
                break;
            }
        }
        assert_eq!(lexer.next_token(), Some(Token::Number(5.0)));
    }

    #[test]
    fn lines_and_relative_moves_become_millimetres() {
        let (paths, warnings) = import(
            eps("newpath 72 72 moveto 72 0 rlineto 0 72 rlineto closepath stroke").as_bytes(),
            1.0,
        )
        .expect("imports");
        assert!(warnings.is_empty());
        assert_eq!(paths.len(), 1);
        let sub = &paths[0].path.subpaths[0];
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3);
        assert!(close(sub.start, Point::new(25.4, 25.4)));
        assert!(close(sub.segments[0].end(), Point::new(50.8, 25.4)));
        assert!(close(sub.segments[1].end(), Point::new(50.8, 50.8)));
        assert_eq!(sub.segments[2].end(), sub.start);
        assert_eq!(paths[0].layer, DEFAULT_LAYER);
    }

    #[test]
    fn transforms_and_graphics_state_apply() {
        let body = "gsave 100 0 translate 90 rotate 2 2 scale \
                    0 0 m 10 0 l S grestore \
                    [1 0 0 1 5 5] concat 0 0 m 1 0 l S";
        let (paths, _) = import(eps(body).as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 2);
        let first = &paths[0].path.subpaths[0];
        assert!(close(first.start, Point::new(mm(100.0), 0.0)));
        // Rotated a quarter turn and doubled: 10 along X ends 20 up Y.
        assert!(close(
            first.segments[0].end(),
            Point::new(mm(100.0), mm(20.0))
        ));
        // grestore dropped the rotation, so the concat only translates.
        let second = &paths[1].path.subpaths[0];
        assert!(close(second.start, Point::new(mm(5.0), mm(5.0))));
    }

    #[test]
    fn curves_v_y_and_re_are_read() {
        let body = "0 0 m 10 10 20 0 v 30 10 40 0 y 50 0 60 10 70 0 c S 0 0 10 20 re f";
        let (paths, _) = import(eps(body).as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 2);
        let segs = &paths[0].path.subpaths[0].segments;
        assert_eq!(segs.len(), 3);
        match (segs[0], segs[1]) {
            (Segment::Cubic { c1, .. }, Segment::Cubic { c2, to, .. }) => {
                assert!(close(c1, Point::new(0.0, 0.0)), "v takes the current point");
                assert_eq!(c2, to, "y takes the end point");
            }
            other => panic!("expected cubics, got {other:?}"),
        }
        let rect = &paths[1].path.subpaths[0];
        assert!(rect.closed);
        assert_eq!(rect.segments.len(), 4);
    }

    #[test]
    fn arcs_become_cubics_on_the_circle() {
        let (paths, _) =
            import(eps("50 50 20 0 360 arc closepath S").as_bytes(), 1.0).expect("imports");
        let sub = &paths[0].path.subpaths[0];
        assert!(sub.closed);
        let centre = Point::new(mm(50.0), mm(50.0));
        for segment in &sub.segments {
            assert!((segment.end().distance(centre) - mm(20.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn prolog_aliases_and_procedures_are_followed() {
        let body = "/mo /moveto load def /li /lineto load def /cl {closepath} bind def \
                    /box { 4 dict begin mo 10 0 rlineto 0 10 rlineto -10 0 rlineto cl end } def \
                    /fi /fill load def \
                    5 5 box fi 0 0 mo 3 3 li stroke";
        let (paths, _) = import(eps(body).as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 2);
        assert!(paths[0].path.subpaths[0].closed);
        assert!(close(
            paths[0].path.subpaths[0].start,
            Point::new(mm(5.0), mm(5.0))
        ));
        assert!(close(
            paths[1].path.subpaths[0].segments[0].end(),
            Point::new(mm(3.0), mm(3.0))
        ));
    }

    #[test]
    fn fill_then_stroke_of_the_same_path_is_one_vector() {
        let body = "0 0 m 10 0 l 10 10 l gsave fill grestore stroke";
        let (paths, _) = import(eps(body).as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 1);
        assert!(paths[0].path.subpaths[0].closed);
    }

    #[test]
    fn text_and_images_are_reported() {
        let body = "/Helvetica findfont 12 scalefont setfont 10 10 moveto (Hi) show \
                    1 1 8 [1 0 0 1 0 0] {<ff>} image 0 0 m 1 1 l S";
        let (paths, warnings) = import(eps(body).as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 1);
        assert_eq!(
            warnings,
            vec![ImportWarning::TextSkipped, ImportWarning::ImageSkipped]
        );
    }

    #[test]
    fn the_bounding_box_corner_is_the_origin_and_scale_applies() {
        let text = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 100 200 300 400\n\
                    %%HiResBoundingBox: 100.5 200 300 400\n100.5 200 m 172.5 200 l S\n";
        let (paths, _) = import(text.as_bytes(), 2.0).expect("imports");
        let sub = &paths[0].path.subpaths[0];
        assert!(close(sub.start, Point::new(0.0, 0.0)));
        assert!(close(sub.segments[0].end(), Point::new(50.8, 0.0)));
    }

    #[test]
    fn binary_data_declared_in_comments_is_skipped() {
        let text = "%!PS-Adobe-3.0\n%%BeginBinary: 6\n(((({\n%%EndBinary\n0 0 m 1 1 l S\n";
        let (paths, _) = import(text.as_bytes(), 1.0).expect("imports");
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn a_dos_eps_wrapper_is_opened() {
        let ps = eps("0 0 m 10 0 l S");
        let mut bytes = vec![0xC5, 0xD0, 0xD3, 0xC6];
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&(ps.len() as u32).to_le_bytes());
        bytes.resize(32, 0);
        bytes.extend_from_slice(ps.as_bytes());
        bytes.extend_from_slice(b"\x00\x01binary preview");
        let (paths, _) = import(&bytes, 1.0).expect("imports");
        assert_eq!(paths.len(), 1);
        let bad = [0xC5, 0xD0, 0xD3, 0xC6, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0];
        assert!(import(&bad, 1.0).is_err());
    }

    #[test]
    fn malformed_input_is_an_error_or_empty_and_never_panics() {
        assert!(matches!(
            import(b"hello", 1.0),
            Err(CoreError::Format { .. })
        ));
        assert!(import(b"", 1.0).is_err());
        for text in [
            "%!\n(unterminated",
            "%!\n{ { {",
            "%!\n} } ] >> ) >",
            "%!\n/a {a} def a",
            "%!\n1 2 3 4 5 6 cm 0 0 m 1e400 1 l S",
            "%!\n<abc",
            "%!\n<~ never ends",
            "%!\n0 0 m 1 1 l 0 0 0 0 0 arc 5 roll -3 index 1e20 array astore S",
        ] {
            let _ = import(text.as_bytes(), 1.0);
        }
        // Pseudo random bytes after a valid header.
        let mut seed = 0x2545_f491_u32;
        for _ in 0..200 {
            let mut bytes = b"%!PS\n".to_vec();
            for _ in 0..256 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let alphabet = b" 0123456789.-/(){}[]<>%mlcvyhreSfqQ";
                bytes.push(alphabet[seed as usize % alphabet.len()]);
            }
            let _ = import(&bytes, 1.0);
        }
    }

    #[test]
    fn a_self_calling_procedure_stops() {
        let result = import(b"%!\n/a {a a} def a 0 0 m 1 1 l S", 1.0);
        assert!(matches!(result, Err(CoreError::Format { .. })));
    }

    pub(crate) fn sample_layers() -> Vec<ExportLayer> {
        let mut square = SubPath::open(Point::new(10.0, 10.0));
        square
            .line_to(Point::new(40.0, 10.0))
            .line_to(Point::new(40.0, 30.0))
            .line_to(Point::new(10.0, 30.0))
            .line_to(Point::new(10.0, 10.0));
        square.closed = true;
        let mut wave = SubPath::open(Point::new(5.0, 50.0));
        wave.cubic_to(
            Point::new(15.0, 70.0),
            Point::new(25.0, 30.0),
            Point::new(35.0, 50.0),
        )
        .arc_to(Point::new(55.0, 50.0), 1.0);
        let mut ring = SubPath::open(Point::new(80.0, 20.0));
        ring.arc_to(Point::new(60.0, 20.0), 1.0)
            .arc_to(Point::new(80.0, 20.0), 1.0);
        ring.closed = true;
        vec![
            ExportLayer {
                name: "Cut (outside)".into(),
                colour: "#ff0000".into(),
                paths: vec![Path::from_subpaths(vec![square, ring])],
            },
            ExportLayer {
                name: "Engrave".into(),
                colour: "#0000ff".into(),
                paths: vec![Path::from_subpath(wave)],
            },
        ]
    }

    /// Asserts two paths are the same within a micron, arcs compared as the
    /// cubics they are written as.
    pub(crate) fn assert_same_geometry(expected: &Path, actual: &Path) {
        let expected = arcs_to_cubics(expected);
        assert_eq!(expected.subpaths.len(), actual.subpaths.len());
        for (e, a) in expected.subpaths.iter().zip(&actual.subpaths) {
            assert_eq!(e.closed, a.closed);
            assert!(e
                .start
                .approx_eq(a.start, Tolerance::new(1e-3).expect("tol")));
            assert_eq!(e.segments.len(), a.segments.len(), "{e:?}\n{a:?}");
            for (se, sa) in e.segments.iter().zip(&a.segments) {
                assert_eq!(se.kind(), sa.kind());
                let tol = Tolerance::new(1e-3).expect("tol");
                assert!(se.end().approx_eq(sa.end(), tol), "{se:?} {sa:?}");
                if let (Segment::Cubic { c1, c2, .. }, Segment::Cubic { c1: d1, c2: d2, .. }) =
                    (se, sa)
                {
                    assert!(c1.approx_eq(*d1, tol) && c2.approx_eq(*d2, tol));
                }
            }
        }
    }

    #[test]
    fn export_then_import_gives_the_same_geometry_and_layers() {
        let layers = sample_layers();
        let bounds = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 80.0));
        for flavour in [Flavour::Eps, Flavour::Illustrator] {
            let text = export(&layers, bounds, flavour);
            assert!(text.contains("%%BoundingBox: 0 0 284 227"), "{text}");
            assert_eq!(
                text.contains("%AI5_ArtSize"),
                flavour == Flavour::Illustrator
            );
            let (paths, warnings) = import(text.as_bytes(), 1.0).expect("imports");
            assert!(warnings.is_empty());
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0].layer, "Cut (outside)");
            assert_eq!(paths[1].layer, "Engrave");
            assert_same_geometry(&layers[0].paths[0], &paths[0].path);
            assert_same_geometry(&layers[1].paths[0], &paths[1].path);
        }
    }

    #[test]
    fn exported_strings_and_numbers_are_escaped() {
        assert_eq!(ps_string("a(b)\\é"), "a\\(b\\)\\\\\\303\\251");
        assert_eq!(number(-0.000001), "0");
        assert_eq!(number(1.5), "1.5");
        assert_eq!(rgb("#ff8000"), (1.0, 128.0 / 255.0, 0.0));
        assert_eq!(rgb("red"), (0.0, 0.0, 0.0));
        let (paths, _) = import(b"%!\n(Caf\\303\\251) Ln 0 0 m 1 0 l S", 1.0).expect("imports");
        assert_eq!(paths[0].layer, "Caf\u{e9}");
    }
}
