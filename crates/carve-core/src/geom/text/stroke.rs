//! Single line (engraving) fonts.
//!
//! A single line font draws each letter as a few open strokes that a cutter
//! follows once, instead of outlines round a filled area. The bundled ones
//! are kept in a small text format of our own (see the files next to the
//! other bundled fonts): a header with the family name and metrics, then one
//! `glyph` line per character with its advance and its strokes.

use std::collections::BTreeMap;

use crate::error::CoreError;
use crate::geom::path::{Path, SubPath};
use crate::geom::point::Point;

/// One character of a single line font, in font units.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeGlyph {
    /// How far the pen moves after this glyph.
    pub advance: f64,
    /// Open polylines, each drawn as one stroke. Y runs up from the baseline.
    pub strokes: Vec<Vec<Point>>,
}

/// A parsed single line font.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeFont {
    family: String,
    em: f64,
    ascender: f64,
    descender: f64,
    cap: f64,
    glyphs: BTreeMap<char, StrokeGlyph>,
}

impl StrokeFont {
    /// Reads a font in the bundled text format.
    ///
    /// # Errors
    /// Returns [`CoreError::Font`] when a line cannot be read or the header
    /// is incomplete.
    pub fn parse(source: &str) -> Result<StrokeFont, CoreError> {
        let mut family = None;
        let mut em = None;
        let mut ascender = None;
        let mut descender = None;
        let mut cap = None;
        let mut glyphs = BTreeMap::new();
        for (number, raw) in source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || CoreError::font(format!("single line font, line {}: {line}", number + 1));
            let (key, rest) = line.split_once(' ').ok_or_else(bad)?;
            let number_of = |text: &str| text.trim().parse::<f64>().map_err(|_| bad());
            match key {
                "family" => family = Some(rest.trim().to_string()),
                "em" => em = Some(number_of(rest)?),
                "ascender" => ascender = Some(number_of(rest)?),
                "descender" => descender = Some(number_of(rest)?),
                "cap" => cap = Some(number_of(rest)?),
                "glyph" => {
                    let mut words = rest.splitn(3, ' ');
                    let code = words.next().ok_or_else(bad)?;
                    let code = u32::from_str_radix(code, 16).map_err(|_| bad())?;
                    let ch = char::from_u32(code).ok_or_else(bad)?;
                    let advance = number_of(words.next().ok_or_else(bad)?)?;
                    let mut strokes = Vec::new();
                    for stroke in words.next().unwrap_or("").split(';') {
                        let mut points = Vec::new();
                        for pair in stroke.split_whitespace() {
                            let (x, y) = pair.split_once(',').ok_or_else(bad)?;
                            points.push(Point::new(number_of(x)?, number_of(y)?));
                        }
                        if points.len() >= 2 {
                            strokes.push(points);
                        }
                    }
                    glyphs.insert(ch, StrokeGlyph { advance, strokes });
                }
                _ => return Err(bad()),
            }
        }
        let missing = |what: &str| CoreError::font(format!("single line font without {what}"));
        let em = em.ok_or_else(|| missing("an em size"))?;
        if em <= 0.0 {
            return Err(CoreError::font("single line font with no em size"));
        }
        Ok(StrokeFont {
            family: family.ok_or_else(|| missing("a family name"))?,
            em,
            ascender: ascender.ok_or_else(|| missing("an ascender"))?,
            descender: descender.ok_or_else(|| missing("a descender"))?,
            cap: cap.ok_or_else(|| missing("a cap height"))?,
            glyphs,
        })
    }

    /// Family name.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Distance between baselines at `size_mm` (the em size), in millimetres.
    pub fn line_height(&self, size_mm: f64) -> f64 {
        (self.ascender - self.descender) * size_mm / self.em
    }

    /// Cap height at `size_mm` (the em size), in millimetres.
    pub fn cap_height(&self, size_mm: f64) -> f64 {
        self.cap * size_mm / self.em
    }

    /// The glyph drawn for `ch`: its own, a space's advance for other
    /// white space, else a question mark.
    pub fn glyph(&self, ch: char) -> Option<&StrokeGlyph> {
        self.glyphs
            .get(&ch)
            .or_else(|| {
                if ch.is_whitespace() {
                    self.glyphs.get(&' ')
                } else {
                    None
                }
            })
            .or_else(|| self.glyphs.get(&'?'))
    }

    /// Advance of `ch` at `size_mm`, in millimetres.
    pub fn advance(&self, ch: char, size_mm: f64) -> f64 {
        self.glyph(ch).map_or(0.0, |g| g.advance) * size_mm / self.em
    }

    /// The strokes of `ch` at `size_mm` as open subpaths, the glyph origin
    /// at (0, 0), in millimetres. Empty for a space.
    pub fn outline(&self, ch: char, size_mm: f64) -> Path {
        let scale = size_mm / self.em;
        let mut path = Path::new();
        let Some(glyph) = self.glyph(ch) else {
            return path;
        };
        for stroke in &glyph.strokes {
            let mut points = stroke.iter().map(|p| Point::new(p.x * scale, p.y * scale));
            let Some(start) = points.next() else {
                continue;
            };
            let mut sub = SubPath::open(start);
            for to in points {
                sub.line_to(to);
            }
            path.push(sub);
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# comment\nfamily Test Line\nem 30\nascender 24\ndescender -9\ncap 21\n\
        glyph 0020 16\nglyph 003F 18 3,16 9,21 ; 9,2 9,0\nglyph 0041 18 9,21 1,0 ; 9,21 17,0 ; 4,7 14,7\n";

    #[test]
    fn reads_the_header_and_glyphs() {
        let font = StrokeFont::parse(SAMPLE).expect("font");
        assert_eq!(font.family(), "Test Line");
        assert!((font.cap_height(30.0) - 21.0).abs() < 1e-12);
        assert!((font.line_height(30.0) - 33.0).abs() < 1e-12);
        assert!((font.advance('A', 15.0) - 9.0).abs() < 1e-12);
        let a = font.outline('A', 30.0);
        assert_eq!(a.subpaths.len(), 3);
        assert!(a.subpaths.iter().all(|s| !s.closed));
        assert!(font.outline(' ', 30.0).is_empty());
    }

    #[test]
    fn a_missing_character_draws_a_question_mark() {
        let font = StrokeFont::parse(SAMPLE).expect("font");
        assert_eq!(font.outline('Z', 30.0).subpaths.len(), 2);
        assert!((font.advance('\t', 30.0) - 16.0).abs() < 1e-12);
    }

    #[test]
    fn a_bad_line_is_an_error() {
        assert!(StrokeFont::parse("family X\nem 30\nglyph zz 3").is_err());
        assert!(StrokeFont::parse("family X\nascender 1\ndescender 1\ncap 1").is_err());
    }
}
