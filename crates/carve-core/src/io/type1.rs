//! Type 1 font programs, as PDF files embed them (`FontFile`), read for
//! their glyph outlines.
//!
//! The format is the one Adobe publishes in "Adobe Type 1 Font Format": a
//! clear text part with the font matrix and encoding, then a private part
//! encrypted with the eexec cipher (binary or hex) holding the subroutines
//! and one charstring per glyph, each encrypted again with the charstring
//! key. Charstrings are run with the Type 1 operators, flex and hint
//! replacement through the usual other subroutines included, and `seac`
//! accented characters built from their two parts.

use std::collections::BTreeMap;

use rustybuzz::ttf_parser::OutlineBuilder;

use super::pdf_encoding::STANDARD;
use crate::geom::Mat3;

/// eexec key for the private part.
const EEXEC_KEY: u16 = 55665;
/// Key for each charstring and subroutine.
const CHARSTRING_KEY: u16 = 4330;
/// Subroutine calls followed before a charstring is given up on.
const MAX_CALL_DEPTH: usize = 10;
/// Operand stack size of the Type 1 interpreter.
const MAX_STACK: usize = 48;

/// A parsed Type 1 font.
pub(super) struct Type1Font {
    matrix: Mat3,
    /// Glyph names by character code for the font's own encoding.
    encoding: Vec<Option<String>>,
    /// Glyph names, sorted; a glyph's id is its index here.
    names: Vec<String>,
    /// Decrypted charstrings in the order of `names`.
    charstrings: Vec<Vec<u8>>,
    /// Decrypted subroutines by number.
    subrs: Vec<Vec<u8>>,
}

fn decrypt(bytes: &[u8], key: u16, skip: usize) -> Vec<u8> {
    let mut r = key;
    let mut out = Vec::with_capacity(bytes.len());
    for &cipher in bytes {
        let plain = cipher ^ (r >> 8) as u8;
        r = (u16::from(cipher).wrapping_add(r))
            .wrapping_mul(52845)
            .wrapping_add(22719);
        out.push(plain);
    }
    if skip <= out.len() {
        out.drain(..skip);
    } else {
        out.clear();
    }
    out
}

/// Joins the segments of a PFB file; other data is returned as it is.
fn unwrap_pfb(data: &[u8]) -> Vec<u8> {
    if data.first() != Some(&0x80) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut at = 0;
    while at + 6 <= data.len() && data[at] == 0x80 && matches!(data[at + 1], 1 | 2) {
        let len = u32::from_le_bytes([data[at + 2], data[at + 3], data[at + 4], data[at + 5]]);
        let start = at + 6;
        let end = start.saturating_add(len as usize).min(data.len());
        out.extend_from_slice(&data[start..end]);
        at = end;
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | 0)
}

/// A tiny reader over PostScript text for the parts of a font program that
/// matter here.
struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Scan<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Scan { bytes, at: 0 }
    }

    fn token(&mut self) -> Option<&'a [u8]> {
        while self.at < self.bytes.len() {
            let b = self.bytes[self.at];
            if is_space(b) {
                self.at += 1;
            } else if b == b'%' {
                while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b'\r' | b'\n') {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        if self.at >= self.bytes.len() {
            return None;
        }
        let start = self.at;
        let first = self.bytes[self.at];
        self.at += 1;
        if matches!(first, b'[' | b']' | b'{' | b'}') {
            return Some(&self.bytes[start..self.at]);
        }
        while self.at < self.bytes.len() {
            let b = self.bytes[self.at];
            if is_space(b) || matches!(b, b'[' | b']' | b'{' | b'}' | b'/' | b'%') {
                break;
            }
            self.at += 1;
        }
        Some(&self.bytes[start..self.at])
    }

    fn number(&mut self) -> Option<f64> {
        std::str::from_utf8(self.token()?).ok()?.parse().ok()
    }

    /// Reads the `len` binary bytes that follow an `RD` token and its
    /// single separating space.
    fn binary(&mut self, len: usize) -> Option<&'a [u8]> {
        let start = self.at + 1;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        self.at = end;
        Some(&self.bytes[start..end])
    }
}

fn number_text(token: &[u8]) -> Option<f64> {
    std::str::from_utf8(token).ok()?.parse().ok()
}

impl Type1Font {
    /// Reads a Type 1 font program. `None` when it is not one.
    pub(super) fn parse(data: &[u8]) -> Option<Type1Font> {
        let data = unwrap_pfb(data);
        let eexec = find(&data, b"eexec")?;
        let clear = &data[..eexec];
        let matrix = font_matrix(clear).unwrap_or(Mat3::scale(0.001, 0.001));
        let encoding = encoding(clear);

        let mut start = eexec + 5;
        while start < data.len() && is_space(data[start]) {
            start += 1;
        }
        let rest = &data[start..];
        let hex = rest.len() >= 4 && rest[..4].iter().all(u8::is_ascii_hexdigit);
        let cipher: Vec<u8> = if hex {
            let digits: Vec<u8> = rest
                .iter()
                .copied()
                .filter(|b| !is_space(*b))
                .take_while(u8::is_ascii_hexdigit)
                .collect();
            digits
                .chunks_exact(2)
                .filter_map(|pair| {
                    let text = std::str::from_utf8(pair).ok()?;
                    u8::from_str_radix(text, 16).ok()
                })
                .collect()
        } else {
            rest.to_vec()
        };
        let private = decrypt(&cipher, EEXEC_KEY, 4);

        let len_iv = find(&private, b"/lenIV")
            .and_then(|at| {
                let mut scan = Scan::new(&private[at + 6..]);
                scan.number()
            })
            .map_or(4, |n| n as i64);
        let key_skip = |bytes: &[u8]| -> Vec<u8> {
            if len_iv < 0 {
                bytes.to_vec()
            } else {
                decrypt(bytes, CHARSTRING_KEY, len_iv as usize)
            }
        };

        let mut subrs: Vec<Vec<u8>> = Vec::new();
        if let Some(at) = find(&private, b"/Subrs") {
            let mut scan = Scan::new(&private[at + 6..]);
            let count = scan.number()? as usize;
            subrs.resize(count.min(1 << 16), Vec::new());
            while let Some(token) = scan.token() {
                if token == b"dup" {
                    let (Some(index), Some(len)) = (scan.number(), scan.number()) else {
                        break;
                    };
                    scan.token()?;
                    let bytes = scan.binary(len as usize)?;
                    if let Some(slot) = subrs.get_mut(index as usize) {
                        *slot = key_skip(bytes);
                    }
                } else if token == b"/CharStrings" || token == b"end" && count == 0 {
                    break;
                } else if token == b"readonly" || token == b"def" || token == b"ND" {
                    // After the last entry.
                    if subrs.iter().all(|s| !s.is_empty()) {
                        break;
                    }
                }
            }
        }

        let at = find(&private, b"/CharStrings")?;
        let mut scan = Scan::new(&private[at + 12..]);
        let mut glyphs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        while let Some(token) = scan.token() {
            if let Some(name) = token.strip_prefix(b"/") {
                let Some(len) = scan.number() else {
                    continue;
                };
                if scan.token().is_none() {
                    break;
                }
                let Some(bytes) = scan.binary(len as usize) else {
                    break;
                };
                glyphs.insert(String::from_utf8_lossy(name).into_owned(), key_skip(bytes));
            } else if token == b"end" && !glyphs.is_empty() {
                break;
            }
        }
        if glyphs.is_empty() {
            return None;
        }
        let (names, charstrings) = glyphs.into_iter().unzip();
        Some(Type1Font {
            matrix,
            encoding,
            names,
            charstrings,
            subrs,
        })
    }

    /// Glyph units to text space.
    pub(super) fn matrix(&self) -> Mat3 {
        self.matrix
    }

    /// The glyph with this name.
    pub(super) fn glyph_by_name(&self, name: &str) -> Option<u16> {
        let index = self.names.binary_search_by(|n| n.as_str().cmp(name)).ok()?;
        u16::try_from(index).ok()
    }

    /// The glyph the font's own encoding gives a code.
    pub(super) fn glyph_by_code(&self, code: u8) -> Option<u16> {
        let name = self.encoding.get(usize::from(code))?.as_deref()?;
        self.glyph_by_name(name)
    }

    /// Advance width of a glyph in glyph units.
    pub(super) fn advance(&self, id: u16) -> Option<u16> {
        let mut run = Interpreter::new(self, None);
        run.run_glyph(id)?;
        let width = run.width.round();
        (0.0..=f64::from(u16::MAX))
            .contains(&width)
            .then_some(width as u16)
    }

    /// Draws a glyph into `sink` in glyph units. `None` when the glyph is
    /// not in the font or its charstring is broken.
    pub(super) fn outline(&self, id: u16, sink: &mut dyn OutlineBuilder) -> Option<()> {
        let mut run = Interpreter::new(self, Some(sink));
        run.run_glyph(id)?;
        run.finish();
        Some(())
    }
}

fn font_matrix(clear: &[u8]) -> Option<Mat3> {
    let at = find(clear, b"/FontMatrix")?;
    let mut scan = Scan::new(&clear[at + 11..]);
    let mut values = Vec::with_capacity(6);
    while let Some(token) = scan.token() {
        match token {
            b"[" | b"{" => {}
            b"]" | b"}" => break,
            other => values.push(number_text(other)?),
        }
        if values.len() == 6 {
            break;
        }
    }
    let [a, b, c, d, e, f] = values[..] else {
        return None;
    };
    Some(Mat3::new(a, b, c, d, e, f))
}

fn encoding(clear: &[u8]) -> Vec<Option<String>> {
    let standard = || {
        STANDARD
            .iter()
            .map(|n| (!n.is_empty()).then(|| (*n).to_string()))
            .collect()
    };
    let Some(at) = find(clear, b"/Encoding") else {
        return standard();
    };
    let mut scan = Scan::new(&clear[at + 9..]);
    let mut names: Vec<Option<String>> = vec![None; 256];
    let mut custom = false;
    while let Some(token) = scan.token() {
        match token {
            b"StandardEncoding" if !custom => return standard(),
            b"dup" => {
                let code = scan.number();
                let name = scan.token();
                if let (Some(code), Some(name)) = (code, name.and_then(|n| n.strip_prefix(b"/"))) {
                    if (0.0..256.0).contains(&code) {
                        names[code as usize] = Some(String::from_utf8_lossy(name).into_owned());
                        custom = true;
                    }
                }
            }
            b"readonly" | b"def" if custom => break,
            b"def" => break,
            _ => {}
        }
    }
    if custom {
        names
    } else {
        standard()
    }
}

/// Runs charstrings, drawing into an optional sink.
struct Interpreter<'a, 's> {
    font: &'a Type1Font,
    sink: Option<&'s mut dyn OutlineBuilder>,
    stack: Vec<f64>,
    /// Values other subroutines hand back to `pop`.
    ps_stack: Vec<f64>,
    x: f64,
    y: f64,
    /// Offset of the part being drawn (the accent of a `seac`).
    dx: f64,
    dy: f64,
    open: bool,
    flex: Option<Vec<(f64, f64)>>,
    width: f64,
    sbx: f64,
    done: bool,
}

impl<'a, 's> Interpreter<'a, 's> {
    fn new(font: &'a Type1Font, sink: Option<&'s mut dyn OutlineBuilder>) -> Self {
        Interpreter {
            font,
            sink,
            stack: Vec::new(),
            ps_stack: Vec::new(),
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            open: false,
            flex: None,
            width: 0.0,
            sbx: 0.0,
            done: false,
        }
    }

    fn run_glyph(&mut self, id: u16) -> Option<()> {
        self.run_glyph_at(id, 0)
    }

    /// Runs a glyph's charstring `depth` calls deep, so the parts of a
    /// `seac` count against the call limit and a glyph built from itself
    /// ends.
    fn run_glyph_at(&mut self, id: u16, depth: usize) -> Option<()> {
        let code = self.font.charstrings.get(usize::from(id))?;
        self.done = false;
        self.execute(code, depth)
    }

    fn finish(&mut self) {
        if self.open {
            if let Some(sink) = self.sink.as_deref_mut() {
                sink.close();
            }
            self.open = false;
        }
    }

    fn point(&self) -> (f32, f32) {
        ((self.x + self.dx) as f32, (self.y + self.dy) as f32)
    }

    fn move_to(&mut self, x: f64, y: f64) {
        if let Some(points) = self.flex.as_mut() {
            points.push((x, y));
            self.x = x;
            self.y = y;
            return;
        }
        self.finish();
        self.x = x;
        self.y = y;
        let (px, py) = self.point();
        if let Some(sink) = self.sink.as_deref_mut() {
            sink.move_to(px, py);
        }
        self.open = true;
    }

    fn ensure_open(&mut self) {
        if !self.open {
            let (px, py) = self.point();
            if let Some(sink) = self.sink.as_deref_mut() {
                sink.move_to(px, py);
            }
            self.open = true;
        }
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.ensure_open();
        self.x = x;
        self.y = y;
        let (px, py) = self.point();
        if let Some(sink) = self.sink.as_deref_mut() {
            sink.line_to(px, py);
        }
    }

    fn curve_to(&mut self, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) {
        self.ensure_open();
        let (dx, dy) = (self.dx, self.dy);
        self.x = to.0;
        self.y = to.1;
        if let Some(sink) = self.sink.as_deref_mut() {
            sink.curve_to(
                (c1.0 + dx) as f32,
                (c1.1 + dy) as f32,
                (c2.0 + dx) as f32,
                (c2.1 + dy) as f32,
                (to.0 + dx) as f32,
                (to.1 + dy) as f32,
            );
        }
    }

    fn pop(&mut self) -> Option<f64> {
        self.stack.pop()
    }

    fn arg(&self, index: usize) -> f64 {
        self.stack.get(index).copied().unwrap_or(0.0)
    }

    fn execute(&mut self, code: &[u8], depth: usize) -> Option<()> {
        if depth > MAX_CALL_DEPTH {
            return None;
        }
        let mut at = 0;
        while at < code.len() && !self.done {
            let v = code[at];
            at += 1;
            if v >= 32 {
                let value = match v {
                    32..=246 => f64::from(v) - 139.0,
                    247..=250 => {
                        let w = *code.get(at)?;
                        at += 1;
                        (f64::from(v) - 247.0) * 256.0 + f64::from(w) + 108.0
                    }
                    251..=254 => {
                        let w = *code.get(at)?;
                        at += 1;
                        -(f64::from(v) - 251.0) * 256.0 - f64::from(w) - 108.0
                    }
                    _ => {
                        let bytes = code.get(at..at + 4)?;
                        at += 4;
                        f64::from(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    }
                };
                if self.stack.len() >= MAX_STACK {
                    return None;
                }
                self.stack.push(value);
                continue;
            }
            let op = if v == 12 {
                let e = *code.get(at)?;
                at += 1;
                1200 + u16::from(e)
            } else {
                u16::from(v)
            };
            match op {
                // hsbw
                13 => {
                    self.sbx = self.arg(0);
                    self.width = self.arg(1);
                    self.x = self.sbx;
                    self.y = 0.0;
                    self.stack.clear();
                }
                // sbw
                1207 => {
                    self.sbx = self.arg(0);
                    self.width = self.arg(2);
                    self.x = self.sbx;
                    self.y = self.arg(1);
                    self.stack.clear();
                }
                // rmoveto, hmoveto, vmoveto
                21 => {
                    let (x, y) = (self.x + self.arg(0), self.y + self.arg(1));
                    self.move_to(x, y);
                    self.stack.clear();
                }
                22 => {
                    let (x, y) = (self.x + self.arg(0), self.y);
                    self.move_to(x, y);
                    self.stack.clear();
                }
                4 => {
                    let (x, y) = (self.x, self.y + self.arg(0));
                    self.move_to(x, y);
                    self.stack.clear();
                }
                // rlineto, hlineto, vlineto
                5 => {
                    let (x, y) = (self.x + self.arg(0), self.y + self.arg(1));
                    self.line_to(x, y);
                    self.stack.clear();
                }
                6 => {
                    let (x, y) = (self.x + self.arg(0), self.y);
                    self.line_to(x, y);
                    self.stack.clear();
                }
                7 => {
                    let (x, y) = (self.x, self.y + self.arg(0));
                    self.line_to(x, y);
                    self.stack.clear();
                }
                // rrcurveto
                8 => {
                    let c1 = (self.x + self.arg(0), self.y + self.arg(1));
                    let c2 = (c1.0 + self.arg(2), c1.1 + self.arg(3));
                    let to = (c2.0 + self.arg(4), c2.1 + self.arg(5));
                    self.curve_to(c1, c2, to);
                    self.stack.clear();
                }
                // vhcurveto
                30 => {
                    let c1 = (self.x, self.y + self.arg(0));
                    let c2 = (c1.0 + self.arg(1), c1.1 + self.arg(2));
                    let to = (c2.0 + self.arg(3), c2.1);
                    self.curve_to(c1, c2, to);
                    self.stack.clear();
                }
                // hvcurveto
                31 => {
                    let c1 = (self.x + self.arg(0), self.y);
                    let c2 = (c1.0 + self.arg(1), c1.1 + self.arg(2));
                    let to = (c2.0, c2.1 + self.arg(3));
                    self.curve_to(c1, c2, to);
                    self.stack.clear();
                }
                // closepath
                9 => {
                    self.finish();
                    self.stack.clear();
                }
                // callsubr
                10 => {
                    let index = self.pop()?;
                    let subr = self.font.subrs.get(index as usize)?;
                    self.execute(subr, depth + 1)?;
                }
                // return
                11 => return Some(()),
                // endchar
                14 => {
                    self.finish();
                    self.done = true;
                    self.stack.clear();
                }
                // seac
                1206 => {
                    let (asb, adx, ady) = (self.arg(0), self.arg(1), self.arg(2));
                    let (base, accent) = (self.arg(3), self.arg(4));
                    self.stack.clear();
                    let glyph = |code: f64| {
                        let name = STANDARD
                            .get(code as usize)
                            .copied()
                            .filter(|n| !n.is_empty())?;
                        self.font.glyph_by_name(name)
                    };
                    let (base, accent) = (glyph(base)?, glyph(accent)?);
                    let width = self.width;
                    self.run_glyph_at(base, depth + 1)?;
                    self.finish();
                    let base_sbx = self.sbx;
                    self.dx = adx - asb + base_sbx;
                    self.dy = ady;
                    self.run_glyph_at(accent, depth + 1)?;
                    self.finish();
                    self.dx = 0.0;
                    self.dy = 0.0;
                    self.width = width;
                    self.done = true;
                }
                // div
                1212 => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(if b == 0.0 { 0.0 } else { a / b });
                }
                // callothersubr
                1216 => {
                    let number = self.pop()?;
                    let count = self.pop()? as usize;
                    if count > self.stack.len() {
                        return None;
                    }
                    let mut args = Vec::with_capacity(count);
                    for _ in 0..count {
                        args.push(self.pop()?);
                    }
                    // args holds them last first.
                    match number as i64 {
                        0 => {
                            // End of flex: two curves from the seven points.
                            if let Some(points) = self.flex.take() {
                                if points.len() >= 7 {
                                    self.curve_to(points[1], points[2], points[3]);
                                    self.curve_to(points[4], points[5], points[6]);
                                }
                            }
                            // Hands back the end point for `pop pop
                            // setcurrentpoint`: x comes off first.
                            let y = args.first().copied().unwrap_or(self.y);
                            let x = args.get(1).copied().unwrap_or(self.x);
                            self.ps_stack = vec![y, x];
                        }
                        1 => {
                            self.flex = Some(Vec::with_capacity(7));
                            self.ps_stack.clear();
                        }
                        2 => self.ps_stack.clear(),
                        _ => {
                            self.ps_stack = args;
                        }
                    }
                }
                // pop
                1217 => {
                    let value = self.ps_stack.pop().unwrap_or(0.0);
                    self.stack.push(value);
                }
                // setcurrentpoint
                1233 => {
                    self.x = self.arg(0);
                    self.y = self.arg(1);
                    self.stack.clear();
                }
                // hstem, vstem, dotsection, vstem3, hstem3 and anything else
                _ => self.stack.clear(),
            }
        }
        Some(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    fn encrypt(plain: &[u8], key: u16) -> Vec<u8> {
        let mut r = key;
        plain
            .iter()
            .map(|&p| {
                let c = p ^ (r >> 8) as u8;
                r = (u16::from(c).wrapping_add(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                c
            })
            .collect()
    }

    fn num(v: i32) -> Vec<u8> {
        match v {
            -107..=107 => vec![(v + 139) as u8],
            108..=1131 => {
                let v = v - 108;
                vec![(v / 256 + 247) as u8, (v % 256) as u8]
            }
            -1131..=-108 => {
                let v = -v - 108;
                vec![(v / 256 + 251) as u8, (v % 256) as u8]
            }
            _ => {
                let mut out = vec![255];
                out.extend_from_slice(&v.to_be_bytes());
                out
            }
        }
    }

    fn charstring(parts: &[&[i32]]) -> Vec<u8> {
        // Each part: numbers, then the operator as a negative value
        // (-op, or -(1200 + op) for escaped ones).
        let mut out = vec![0u8; 4];
        for part in parts {
            let (op, args) = part.split_last().expect("an operator");
            for a in args {
                out.extend(num(*a));
            }
            let op = -op;
            if op >= 1200 {
                out.extend([12, (op - 1200) as u8]);
            } else {
                out.push(op as u8);
            }
        }
        encrypt(&out, CHARSTRING_KEY)
    }

    /// A small Type 1 font with a square `A`, a `B` drawn through a
    /// subroutine and a flex, and an `Aacute` built with `seac`.
    pub(crate) fn sample(hex: bool) -> Vec<u8> {
        let square = charstring(&[
            &[50, 600, -13],
            &[0, 0, -21],
            &[500, -6],
            &[700, -7],
            &[-500, -6],
            &[-9],
            &[-14],
        ]);
        let b = charstring(&[&[0, 500, -13], &[0, -10], &[-14]]);
        // Subr 0 draws a triangle with a flex curve across its top.
        let flex = [
            &[0, 1, -1216][..],
            &[0, 0, -21],
            &[100, 0, -21],
            &[100, 50, -21],
            &[100, 0, -21],
            &[100, 0, -21],
            &[100, -50, -21],
            &[100, 0, -21],
            &[50, 600, 0, 3, 0, -1216],
            &[-1217],
            &[-1217],
            &[-1233],
        ];
        let mut subr0_parts: Vec<&[i32]> = vec![&[0, 0, -21]];
        subr0_parts.extend(flex.iter());
        subr0_parts.push(&[0, 300, -5]);
        subr0_parts.push(&[-9]);
        subr0_parts.push(&[-11]);
        let subr0 = charstring(&subr0_parts);
        let accent = charstring(&[&[0, 200, -13], &[0, 800, -21], &[100, -6], &[-9], &[-14]]);
        let aacute = charstring(&[&[50, 600, -13], &[0, 100, 0, 65, 194, -1206]]);

        let mut private = b"0000/Private 8 dict dup begin /lenIV 4 def\n/Subrs 1 array\n".to_vec();
        private.extend(format!("dup 0 {} RD ", subr0.len()).bytes());
        private.extend(&subr0);
        private.extend(b" NP\nND\n2 index /CharStrings 4 dict dup begin\n");
        for (name, bytes) in [
            ("A", &square),
            ("B", &b),
            ("acute", &accent),
            ("Aacute", &aacute),
        ] {
            private.extend(format!("/{name} {} RD ", bytes.len()).bytes());
            private.extend(bytes.iter());
            private.extend(b" ND\n");
        }
        private.extend(b"end\nend\nmark currentfile closefile\n");
        let cipher = encrypt(&private, EEXEC_KEY);
        let mut out = b"%!PS-AdobeFont-1.0: Sample\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
            /Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\ndup 65 /A put\ndup 66 /B put\n\
            readonly def\ncurrentfile eexec\n"
            .to_vec();
        if hex {
            for byte in cipher {
                out.extend(format!("{byte:02x}").bytes());
            }
        } else {
            out.extend(cipher);
        }
        out.extend(b"\n0000000000000000\ncleartomark\n");
        out
    }

    struct Record(Vec<String>);

    impl OutlineBuilder for Record {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push(format!("M{x} {y}"));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push(format!("L{x} {y}"));
        }
        fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) {
            self.0.push(format!("Q{x} {y}"));
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
            self.0.push(format!("C{x} {y}"));
        }
        fn close(&mut self) {
            self.0.push("Z".into());
        }
    }

    #[test]
    fn a_binary_and_a_hex_font_read_the_same() {
        for hex in [false, true] {
            let font = Type1Font::parse(&sample(hex)).expect("parses");
            let a = font.glyph_by_code(65).expect("A");
            assert_eq!(font.glyph_by_name("A"), Some(a));
            assert_eq!(font.advance(a), Some(600));
            let mut record = Record(Vec::new());
            font.outline(a, &mut record).expect("draws");
            assert_eq!(record.0, ["M50 0", "L550 0", "L550 700", "L50 700", "Z"]);
            assert!(
                (font.matrix().apply(crate::geom::Point::new(1000.0, 0.0)).x - 1.0).abs() < 1e-12
            );
        }
    }

    #[test]
    fn subroutines_flex_and_accents_are_followed() {
        let font = Type1Font::parse(&sample(false)).expect("parses");
        let b = font.glyph_by_name("B").expect("B");
        let mut record = Record(Vec::new());
        font.outline(b, &mut record).expect("draws");
        assert_eq!(record.0, ["M0 0", "C300 50", "C600 0", "L600 300", "Z"]);
        let aacute = font.glyph_by_name("Aacute").expect("Aacute");
        let mut record = Record(Vec::new());
        font.outline(aacute, &mut record).expect("draws");
        // The square, then the accent moved by adx - asb + the base's sbx.
        assert_eq!(
            record.0[..5],
            ["M50 0", "L550 0", "L550 700", "L50 700", "Z"]
        );
        assert_eq!(record.0[5..], ["M150 800", "L250 800", "Z"]);
        assert_eq!(font.advance(aacute), Some(600));
    }

    #[test]
    fn other_data_is_not_a_font() {
        assert!(Type1Font::parse(b"%!PS nothing here").is_none());
        assert!(Type1Font::parse(b"currentfile eexec 00").is_none());
    }

    mod type1_review_tests {
        use super::*;

        /// A font with the given subroutines and glyphs.
        fn font(subrs: &[Vec<u8>], glyphs: &[(&str, Vec<u8>)]) -> Vec<u8> {
            let mut private = b"0000/Private 8 dict dup begin /lenIV 4 def\n".to_vec();
            private.extend(format!("/Subrs {} array\n", subrs.len()).bytes());
            for (i, subr) in subrs.iter().enumerate() {
                private.extend(format!("dup {i} {} RD ", subr.len()).bytes());
                private.extend(subr);
                private.extend(b" NP\n");
            }
            private.extend(b"ND\n2 index /CharStrings 4 dict dup begin\n");
            for (name, bytes) in glyphs {
                private.extend(format!("/{name} {} RD ", bytes.len()).bytes());
                private.extend(bytes.iter());
                private.extend(b" ND\n");
            }
            private.extend(b"end\nend\n");
            let mut out =
                b"%!PS-AdobeFont-1.0: Test\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
                /Encoding StandardEncoding def\ncurrentfile eexec\n"
                    .to_vec();
            out.extend(encrypt(&private, EEXEC_KEY));
            out
        }

        #[test]
        fn a_seac_that_names_itself_is_refused_not_followed_forever() {
            // `A` (standard code 65) built from itself as base and accent.
            let a = charstring(&[&[0, 600, -13], &[0, 0, 0, 65, 65, -1206]]);
            let data = font(&[], &[("A", a)]);
            let font = Type1Font::parse(&data).expect("parses");
            let id = font.glyph_by_name("A").expect("A");
            let mut record = Record(Vec::new());
            assert!(font.outline(id, &mut record).is_none());
            assert!(font.advance(id).is_none());
        }

        #[test]
        fn subroutines_calling_themselves_many_times_give_up_quickly() {
            // Subr 0 calls itself 200 times: 200^10 calls without a budget.
            let mut parts: Vec<Vec<i32>> = vec![vec![0, 0, -21]];
            for _ in 0..200 {
                parts.push(vec![0, -10]);
            }
            parts.push(vec![-11]);
            let refs: Vec<&[i32]> = parts.iter().map(Vec::as_slice).collect();
            let subr = charstring(&refs);
            let a = charstring(&[&[0, 600, -13], &[0, -10], &[-14]]);
            let data = font(&[subr], &[("A", a)]);
            let font = Type1Font::parse(&data).expect("parses");
            let id = font.glyph_by_name("A").expect("A");
            let started = std::time::Instant::now();
            let mut record = Record(Vec::new());
            let _ = font.outline(id, &mut record);
            assert!(started.elapsed().as_secs_f64() < 2.0);
        }

        #[test]
        fn a_huge_othersubr_argument_count_does_not_allocate_it() {
            // 2^31 - 1 arguments asked of callothersubr 3.
            let a = charstring(&[&[0, 600, -13], &[i32::MAX, 3, -1216], &[-14]]);
            let data = font(&[], &[("A", a)]);
            let font = Type1Font::parse(&data).expect("parses");
            let id = font.glyph_by_name("A").expect("A");
            let mut record = Record(Vec::new());
            assert!(font.outline(id, &mut record).is_none());
        }

        #[test]
        fn truncated_and_scrambled_fonts_never_panic() {
            let sample = sample(false);
            for end in 0..sample.len() {
                if let Some(font) = Type1Font::parse(&sample[..end]) {
                    for id in 0..4 {
                        let mut record = Record(Vec::new());
                        let _ = font.outline(id, &mut record);
                        let _ = font.advance(id);
                    }
                }
            }
            let mut seed = 0x2545_f491_u32;
            for _ in 0..2000 {
                let mut bytes = sample.clone();
                for _ in 0..4 {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let at = (seed >> 8) as usize % bytes.len();
                    bytes[at] = (seed >> 24) as u8;
                }
                if let Some(font) = Type1Font::parse(&bytes) {
                    for id in 0..4 {
                        let mut record = Record(Vec::new());
                        let _ = font.outline(id, &mut record);
                        let _ = font.advance(id);
                    }
                }
            }
        }
    }
}
