//! Text layout: turning a string and a font into vector outlines.
//!
//! Sizes are millimetres. `size_mm` is the font size, the em size, which is
//! the number the text tool shows, not the cap height. The baseline of the
//! first line sits on y = 0 and later lines run downward.

pub mod auto;
pub mod font;
pub mod stroke;

use serde::{Deserialize, Serialize};

pub use font::{FontLibrary, LoadedFont};
use rustybuzz::ttf_parser::{self, OutlineBuilder};
pub use stroke::StrokeFont;

use crate::error::CoreError;

use super::bbox::BBox;
use super::edit::reverse;
use super::metrics::{path_length, path_point_at, path_tangent_at, subpath_signed_area};
use super::path::{Path, SubPath};
use super::point::{Mat3, Point, Tolerance, Vec2};
use super::transform::transform_path;

/// How lines of text line up with each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Line starts are flush.
    Left,
    /// Lines are centred on the text origin.
    Centre,
    /// Line ends are flush.
    Right,
}

/// Which side of a curve text sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSide {
    /// Left of the direction of travel, reading along it.
    Left,
    /// Right of the direction of travel, reading against it, which is what
    /// keeps text the right way up under a curve.
    Right,
}

/// Everything the text tool needs to lay out a piece of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSpec {
    /// The text itself. A newline starts a new line.
    pub text: String,
    /// Family name, matched without regard to case.
    pub font_family: String,
    /// Ask for a bold face.
    pub bold: bool,
    /// Ask for an italic face.
    pub italic: bool,
    /// Font size in millimetres, the em size.
    pub size_mm: f64,
    /// How lines line up with each other.
    pub alignment: TextAlign,
    /// Line pitch as a multiple of the font's own default, 1 being default.
    pub line_spacing: f64,
    /// Extra space between characters as a fraction of the font size.
    pub char_spacing: f64,
    /// Let the font's kerning pairs move letters closer together.
    pub kerning: bool,
}

impl Default for TextSpec {
    fn default() -> Self {
        TextSpec {
            text: String::new(),
            font_family: FontLibrary::FALLBACK_FAMILY.to_string(),
            bold: false,
            italic: false,
            size_mm: 10.0,
            alignment: TextAlign::Left,
            line_spacing: 1.0,
            char_spacing: 0.0,
            kerning: true,
        }
    }
}

/// A text spec plus the column width paragraphs wrap to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlockSpec {
    /// The text and its font settings.
    pub text: TextSpec,
    /// Column width in millimetres. Words that do not fit start a new line.
    pub width_mm: f64,
}

/// One glyph, with its outline at the origin and where it belongs.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedGlyph {
    /// The outline with its origin at the glyph origin, in millimetres.
    /// Empty for a space or any other glyph that draws nothing.
    pub outline: Path,
    /// Baseline X of the glyph origin, in millimetres.
    pub x: f64,
    /// Baseline Y of the glyph origin, in millimetres. Zero on the first
    /// line and negative on later ones.
    pub y: f64,
    /// How far the pen moves after this glyph, in millimetres.
    pub advance: f64,
    /// Which line of the text the glyph is on, counting from zero.
    pub line: usize,
}

struct OutlineSink {
    path: Path,
    current: Option<SubPath>,
    scale: f64,
}

impl OutlineSink {
    fn new(scale: f64) -> Self {
        OutlineSink {
            path: Path::new(),
            current: None,
            scale,
        }
    }

    fn point(&self, x: f32, y: f32) -> Point {
        Point::new(x as f64 * self.scale, y as f64 * self.scale)
    }

    fn flush(&mut self) {
        if let Some(mut sub) = self.current.take() {
            if !sub.segments.is_empty() {
                sub.close_with_line(Tolerance::EQUALITY);
                self.path.push(sub);
            }
        }
    }

    /// Normalises winding so the outer loop of the glyph runs counter
    /// clockwise, whichever way round the font stores it.
    fn finish(mut self) -> Path {
        self.flush();
        let mut biggest = 0.0;
        let mut sign = 1.0;
        for sub in &self.path.subpaths {
            let area = subpath_signed_area(sub);
            if area.abs() > biggest {
                biggest = area.abs();
                sign = area.signum();
            }
        }
        if sign < 0.0 {
            return reverse(&self.path);
        }
        self.path
    }
}

impl OutlineBuilder for OutlineSink {
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
            // A quadratic is the cubic whose controls sit two thirds of the
            // way from each end towards the quadratic control point.
            let c1 = from.lerp(control, 2.0 / 3.0);
            let c2 = to.lerp(control, 2.0 / 3.0);
            sub.cubic_to(c1, c2, to);
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let to = self.point(x, y);
        if let Some(sub) = self.current.as_mut() {
            sub.cubic_to(c1, c2, to);
        }
    }

    fn close(&mut self) {
        if let Some(mut sub) = self.current.take() {
            if !sub.segments.is_empty() {
                sub.close_with_line(Tolerance::EQUALITY);
                self.path.push(sub);
            }
        }
    }
}

fn check_spec(spec: &TextSpec) -> Result<(), CoreError> {
    if !spec.size_mm.is_finite() || spec.size_mm <= 0.0 {
        return Err(CoreError::font(format!(
            "text size must be a positive number of millimeters, got {}",
            spec.size_mm
        )));
    }
    if !spec.line_spacing.is_finite() || spec.line_spacing <= 0.0 {
        return Err(CoreError::font(format!(
            "line spacing must be a positive multiple, got {}",
            spec.line_spacing
        )));
    }
    if !spec.char_spacing.is_finite() {
        return Err(CoreError::font("character spacing must be a number"));
    }
    Ok(())
}

fn shape_line(face: &rustybuzz::Face<'_>, line: &str, kerning: bool) -> rustybuzz::GlyphBuffer {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(line);
    buffer.set_direction(rustybuzz::Direction::LeftToRight);
    let features = if kerning {
        Vec::new()
    } else {
        vec![rustybuzz::Feature::new(
            ttf_parser::Tag::from_bytes(b"kern"),
            0,
            ..,
        )]
    };
    rustybuzz::shape(face, &features, buffer)
}

/// Default distance between baselines for a face, in millimetres.
pub fn line_height(font: &LoadedFont, size_mm: f64) -> Result<f64, CoreError> {
    if let Some(stroke) = font.stroke() {
        return Ok(stroke.line_height(size_mm));
    }
    let face = font.face()?;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Err(CoreError::font("the font reports no em size"));
    }
    let scale = size_mm / upem;
    let ascender = face.ascender() as f64;
    let descender = face.descender() as f64;
    let gap = face.line_gap() as f64;
    Ok((ascender - descender + gap) * scale)
}

/// Cap height of a face in millimetres, falling back to a typical share of
/// the em when the font does not say.
pub fn cap_height(font: &LoadedFont, size_mm: f64) -> Result<f64, CoreError> {
    if let Some(stroke) = font.stroke() {
        return Ok(stroke.cap_height(size_mm));
    }
    let face = font.face()?;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Err(CoreError::font("the font reports no em size"));
    }
    let cap = face
        .capital_height()
        .map(|v| v as f64)
        .unwrap_or(0.7 * upem);
    Ok(cap * size_mm / upem)
}

/// Width of one line of text in millimetres, without laying out outlines.
pub fn line_width(library: &FontLibrary, spec: &TextSpec, line: &str) -> Result<f64, CoreError> {
    check_spec(spec)?;
    let font = library.load(&spec.font_family, spec.bold, spec.italic)?;
    let extra = spec.char_spacing * spec.size_mm;
    if let Some(stroke) = font.stroke() {
        return Ok(line
            .chars()
            .map(|ch| stroke.advance(ch, spec.size_mm) + extra)
            .sum());
    }
    let face = font.face()?;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Err(CoreError::font("the font reports no em size"));
    }
    let scale = spec.size_mm / upem;
    let shaped = shape_line(&face, line, spec.kerning);
    Ok(shaped
        .glyph_positions()
        .iter()
        .map(|p| p.x_advance as f64 * scale + extra)
        .sum())
}

/// Lays the text out and reports every glyph with where it belongs.
pub fn layout_glyphs(
    library: &FontLibrary,
    spec: &TextSpec,
) -> Result<Vec<PlacedGlyph>, CoreError> {
    check_spec(spec)?;
    let font = library.load(&spec.font_family, spec.bold, spec.italic)?;
    let pitch = line_height(&font, spec.size_mm)? * spec.line_spacing;
    let extra = spec.char_spacing * spec.size_mm;
    if let Some(stroke) = font.stroke() {
        return Ok(layout_strokes(stroke, spec, pitch, extra));
    }
    let face = font.face()?;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Err(CoreError::font("the font reports no em size"));
    }
    let scale = spec.size_mm / upem;

    let mut lines: Vec<Vec<PlacedGlyph>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();

    for (line_index, line) in spec.text.split('\n').enumerate() {
        let shaped = shape_line(&face, line, spec.kerning);
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        let mut cursor = 0.0;
        let mut glyphs = Vec::with_capacity(infos.len());
        for (info, position) in infos.iter().zip(positions.iter()) {
            let advance = position.x_advance as f64 * scale + extra;
            let mut sink = OutlineSink::new(scale);
            let glyph = ttf_parser::GlyphId(info.glyph_id as u16);
            let has_outline = face.outline_glyph(glyph, &mut sink).is_some();
            let outline = if has_outline {
                sink.finish()
            } else {
                Path::new()
            };
            glyphs.push(PlacedGlyph {
                outline,
                x: cursor + position.x_offset as f64 * scale,
                y: -(line_index as f64) * pitch + position.y_offset as f64 * scale,
                advance,
                line: line_index,
            });
            cursor += advance;
        }
        widths.push(cursor);
        lines.push(glyphs);
    }

    Ok(aligned(spec.alignment, lines, widths))
}

/// Shifts each line of glyphs so the lines line up as `alignment` says.
fn aligned(
    alignment: TextAlign,
    lines: Vec<Vec<PlacedGlyph>>,
    widths: Vec<f64>,
) -> Vec<PlacedGlyph> {
    let mut out = Vec::new();
    for (glyphs, width) in lines.into_iter().zip(widths) {
        let shift = match alignment {
            TextAlign::Left => 0.0,
            TextAlign::Centre => -width * 0.5,
            TextAlign::Right => -width,
        };
        for mut glyph in glyphs {
            glyph.x += shift;
            out.push(glyph);
        }
    }
    out
}

/// Lays out a single line font: one glyph per character, each drawn as
/// open strokes, with no shaping or kerning.
fn layout_strokes(
    stroke: &StrokeFont,
    spec: &TextSpec,
    pitch: f64,
    extra: f64,
) -> Vec<PlacedGlyph> {
    let mut lines: Vec<Vec<PlacedGlyph>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    for (line_index, line) in spec.text.split('\n').enumerate() {
        let mut cursor = 0.0;
        let mut glyphs = Vec::new();
        for ch in line.chars() {
            let advance = stroke.advance(ch, spec.size_mm) + extra;
            glyphs.push(PlacedGlyph {
                outline: stroke.outline(ch, spec.size_mm),
                x: cursor,
                y: -(line_index as f64) * pitch,
                advance,
                line: line_index,
            });
            cursor += advance;
        }
        widths.push(cursor);
        lines.push(glyphs);
    }
    aligned(spec.alignment, lines, widths)
}

/// Lays out text and returns one path per glyph that draws something.
///
/// Spaces and other blank glyphs are left out, so the number of paths is the
/// number of visible characters.
pub fn layout_text(library: &FontLibrary, spec: &TextSpec) -> Result<Vec<Path>, CoreError> {
    let glyphs = layout_glyphs(library, spec)?;
    let mut out = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        if glyph.outline.is_empty() {
            continue;
        }
        let m = Mat3::translate(Vec2::new(glyph.x, glyph.y));
        out.push(transform_path(&glyph.outline, &m));
    }
    Ok(out)
}

/// Wraps text to a column width and lays it out.
///
/// Words are kept whole. A single word wider than the column is left on its
/// own line rather than being broken.
pub fn layout_text_block(
    library: &FontLibrary,
    spec: &TextBlockSpec,
) -> Result<Vec<Path>, CoreError> {
    if !spec.width_mm.is_finite() || spec.width_mm <= 0.0 {
        return Err(CoreError::font(format!(
            "the column width must be a positive number of millimeters, got {}",
            spec.width_mm
        )));
    }
    let wrapped = wrap_text(library, &spec.text, spec.width_mm)?;
    let mut inner = spec.text.clone();
    inner.text = wrapped;
    layout_text(library, &inner)
}

/// Breaks text into lines that fit a column width.
pub fn wrap_text(
    library: &FontLibrary,
    spec: &TextSpec,
    width_mm: f64,
) -> Result<String, CoreError> {
    let mut out = String::new();
    for (i, paragraph) in spec.text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && line_width(library, spec, &candidate)? > width_mm {
                out.push_str(&line);
                out.push('\n');
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        out.push_str(&line);
    }
    Ok(out)
}

/// How text is set along a curve by [`text_on_curve_with`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveLayout {
    /// How far the baseline is lifted off the curve, in millimetres.
    pub offset: f64,
    /// Which side of the curve the text sits on.
    pub side: TextSide,
    /// Where the run sits along the curve (from the start node on a closed
    /// curve).
    pub align: TextAlign,
    /// Shifts the run along the curve by this many millimetres.
    pub start_at: f64,
    /// Spreads the letters out (over 1) or draws them in (under 1).
    pub spacing: f64,
    /// Scales the text so that the run fills the whole curve.
    pub fill: bool,
    /// Keeps each letter upright instead of turning it with the curve.
    pub keep_vertical: bool,
    /// Drops the baseline under the curve by this many millimetres of the
    /// unscaled text (half the text height centres the text on the curve).
    pub drop: f64,
}

impl CurveLayout {
    /// The plain layout: the text sits on the curve at its natural size.
    pub fn along(offset: f64, side: TextSide, align: TextAlign, start_at: f64) -> Self {
        CurveLayout {
            offset,
            side,
            align,
            start_at,
            spacing: 1.0,
            fill: false,
            keep_vertical: false,
            drop: 0.0,
        }
    }
}

/// Sets text along a curve.
///
/// `offset` lifts the baseline off the curve, in millimetres. `side` picks
/// which side of the curve the text sits on. `align` places the run along the
/// curve, and `start_at` shifts it by that many millimetres of arc length.
#[allow(clippy::too_many_arguments)]
pub fn text_on_curve(
    library: &FontLibrary,
    spec: &TextSpec,
    path: &Path,
    offset: f64,
    side: TextSide,
    align: TextAlign,
    start_at: f64,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    let layout = CurveLayout::along(offset, side, align, start_at);
    text_on_curve_with(library, spec, path, &layout, tol)
}

/// Sets text along a curve as `layout` says.
pub fn text_on_curve_with(
    library: &FontLibrary,
    spec: &TextSpec,
    path: &Path,
    layout: &CurveLayout,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    let track = match layout.side {
        TextSide::Left => path.clone(),
        TextSide::Right => reverse(path),
    };
    let total = path_length(&track, tol);
    if total <= 0.0 {
        return Err(CoreError::geometry(
            "text needs a curve with some length to sit on",
        ));
    }
    let closed = !track.subpaths.is_empty() && track.subpaths.iter().all(|sub| sub.closed);
    let glyphs = layout_glyphs(library, spec)?;
    let spacing = if layout.spacing.is_finite() && layout.spacing > 0.0 {
        layout.spacing
    } else {
        1.0
    };
    let first = glyphs.iter().map(|g| g.x).fold(0.0_f64, f64::min);
    // The run once the gaps are spread: the last letter keeps its own width.
    let run = glyphs
        .iter()
        .map(|g| (g.x - first) * spacing + g.advance)
        .fold(0.0_f64, f64::max);
    let scale = if layout.fill && run > 0.0 {
        total / run
    } else {
        1.0
    };
    let width = run * scale;
    let base = if layout.fill {
        layout.start_at
    } else {
        match layout.align {
            TextAlign::Left => layout.start_at,
            TextAlign::Centre => layout.start_at + (total - width) * 0.5,
            TextAlign::Right => layout.start_at + (total - width),
        }
    };

    let mut out = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        if glyph.outline.is_empty() {
            continue;
        }
        let mut s = base + (glyph.x - first) * spacing * scale;
        // A closed curve has no ends: text that runs past the start wraps
        // round. On an open curve, letters past an end carry on in a straight
        // line along the end's direction instead of piling up on the end.
        if closed {
            s = s.rem_euclid(total);
        }
        let overshoot = if s < 0.0 {
            s
        } else if s > total {
            s - total
        } else {
            0.0
        };
        let t = (s / total).clamp(0.0, 1.0);
        let Some(end) = path_point_at(&track, t, tol) else {
            continue;
        };
        let Some(tangent) = path_tangent_at(&track, t, tol) else {
            continue;
        };
        let anchor = end.offset(tangent.scaled(overshoot));
        let normal = tangent.perp();
        let lift = layout.offset + (glyph.y - layout.drop) * scale;
        let place = anchor.offset(normal.scaled(lift));
        let turn = if layout.keep_vertical {
            0.0
        } else {
            tangent.angle()
        };
        let m = Mat3::scale(scale, scale)
            .then(&Mat3::rotate(turn))
            .then(&Mat3::translate(Vec2::new(place.x, place.y)));
        out.push(transform_path(&glyph.outline, &m));
    }
    Ok(out)
}

/// Sets text round a circular arc.
///
/// `start_angle` and `sweep` are radians, `sweep` positive counter clockwise.
/// `inside` puts the letters pointing in towards the centre instead of out.
#[allow(clippy::too_many_arguments)]
pub fn arc_text(
    library: &FontLibrary,
    spec: &TextSpec,
    centre: Point,
    radius: f64,
    start_angle: f64,
    sweep: f64,
    inside: bool,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(CoreError::geometry(format!(
            "arc text needs a positive radius in millimeters, got {radius}"
        )));
    }
    if !sweep.is_finite() || sweep.abs() < 1e-9 {
        return Err(CoreError::geometry(
            "arc text needs a sweep that is not zero",
        ));
    }
    let start = Point::new(
        centre.x + radius * start_angle.cos(),
        centre.y + radius * start_angle.sin(),
    );
    let end = Point::new(
        centre.x + radius * (start_angle + sweep).cos(),
        centre.y + radius * (start_angle + sweep).sin(),
    );
    let mut sub = SubPath::open(start);
    sub.arc_to(end, super::path::bulge_from_sweep(sweep));
    let path = sub.into_path();
    // Left of travel points at the centre on a counter clockwise arc, so that
    // is the side where letters point inward.
    let side = match (sweep > 0.0, inside) {
        (true, true) | (false, false) => TextSide::Left,
        (true, false) | (false, true) => TextSide::Right,
    };
    text_on_curve(library, spec, &path, 0.0, side, TextAlign::Centre, 0.0, tol)
}

/// Bounding box of a set of glyph outlines, in millimetres.
pub fn outlines_bbox(paths: &[Path]) -> Option<BBox> {
    let mut result: Option<BBox> = None;
    for path in paths {
        if let Some(b) = super::metrics::path_bbox(path) {
            result = Some(match result {
                Some(existing) => existing.union(b),
                None => b,
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::path_area;

    const FLAT: Tolerance = Tolerance::FLATTEN;

    fn spec(text: &str) -> TextSpec {
        TextSpec {
            text: text.to_string(),
            ..TextSpec::default()
        }
    }

    #[test]
    fn a_word_gives_one_path_per_visible_glyph() {
        let library = FontLibrary::bundled();
        let paths = layout_text(&library, &spec("HELLO")).expect("layout");
        assert_eq!(paths.len(), 5);
        assert!(paths.iter().all(|p| !p.is_empty()));
    }

    #[test]
    fn a_single_line_font_strokes_open_paths_at_its_cap_height() {
        let library = FontLibrary::bundled();
        let mut s = spec("HI there");
        s.font_family = "Single Line Sans".to_string();
        s.size_mm = 30.0;
        let paths = layout_text(&library, &s).expect("layout");
        // H, I and the five letters of "there": the space draws nothing.
        assert_eq!(paths.len(), 7);
        assert!(paths
            .iter()
            .flat_map(|p| p.subpaths.iter())
            .all(|sub| !sub.closed));
        let h = outlines_bbox(&paths[..1]).expect("bbox");
        let font = library
            .load("Single Line Sans", false, false)
            .expect("font");
        let cap = cap_height(&font, 30.0).expect("cap");
        assert!((h.max.y - h.min.y - cap).abs() < 1e-9, "{h:?} {cap}");
        let width = line_width(&library, &s, "HI there").expect("width");
        let glyphs = layout_glyphs(&library, &s).expect("glyphs");
        let last = glyphs.last().expect("glyph");
        assert!((last.x + last.advance - width).abs() < 1e-9);
    }

    #[test]
    fn spaces_draw_nothing() {
        let library = FontLibrary::bundled();
        let paths = layout_text(&library, &spec("A B")).expect("layout");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn capital_height_matches_the_nominal_size() {
        let library = FontLibrary::bundled();
        let mut s = spec("HELLO");
        s.size_mm = 20.0;
        let paths = layout_text(&library, &s).expect("layout");
        let bbox = outlines_bbox(&paths).expect("bbox");
        let font = library
            .load(FontLibrary::FALLBACK_FAMILY, false, false)
            .expect("font");
        let nominal = cap_height(&font, 20.0).expect("cap height");
        let error = (bbox.height() - nominal).abs() / nominal;
        assert!(error < 0.05, "height {} vs cap {nominal}", bbox.height());
    }

    #[test]
    fn size_scales_the_outlines() {
        let library = FontLibrary::bundled();
        let mut small = spec("H");
        small.size_mm = 10.0;
        let mut big = spec("H");
        big.size_mm = 20.0;
        let a = outlines_bbox(&layout_text(&library, &small).expect("layout")).expect("bbox");
        let b = outlines_bbox(&layout_text(&library, &big).expect("layout")).expect("bbox");
        assert!((b.height() / a.height() - 2.0).abs() < 0.01);
    }

    #[test]
    fn glyph_outlines_are_wound_outer_counter_clockwise() {
        let library = FontLibrary::bundled();
        let paths = layout_text(&library, &spec("O")).expect("layout");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths.len(), 2);
        assert!(path_area(&paths[0], FLAT) > 0.0);
        assert!(subpath_signed_area(&paths[0].subpaths[0]) > 0.0);
    }

    #[test]
    fn the_baseline_sits_on_y_zero() {
        let library = FontLibrary::bundled();
        let paths = layout_text(&library, &spec("H")).expect("layout");
        let bbox = outlines_bbox(&paths).expect("bbox");
        assert!(bbox.min.y.abs() < 1e-9, "baseline at {}", bbox.min.y);
    }

    #[test]
    fn a_second_line_sits_below_the_first() {
        let library = FontLibrary::bundled();
        let glyphs = layout_glyphs(&library, &spec("A\nB")).expect("layout");
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[0].line, 0);
        assert_eq!(glyphs[1].line, 1);
        assert!(glyphs[1].y < glyphs[0].y);
    }

    #[test]
    fn line_spacing_moves_the_second_line() {
        let library = FontLibrary::bundled();
        let mut wide = spec("A\nB");
        wide.line_spacing = 2.0;
        let tight = layout_glyphs(&library, &spec("A\nB")).expect("layout");
        let loose = layout_glyphs(&library, &wide).expect("layout");
        assert!((loose[1].y / tight[1].y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn centre_alignment_balances_the_lines() {
        let library = FontLibrary::bundled();
        let mut s = spec("IIII\nI");
        s.alignment = TextAlign::Centre;
        let glyphs = layout_glyphs(&library, &s).expect("layout");
        let first: Vec<&PlacedGlyph> = glyphs.iter().filter(|g| g.line == 0).collect();
        let second: Vec<&PlacedGlyph> = glyphs.iter().filter(|g| g.line == 1).collect();
        let centre_of = |row: &[&PlacedGlyph]| {
            let lo = row.iter().map(|g| g.x).fold(f64::INFINITY, f64::min);
            let hi = row
                .iter()
                .map(|g| g.x + g.advance)
                .fold(f64::NEG_INFINITY, f64::max);
            (lo + hi) * 0.5
        };
        assert!((centre_of(&first) - centre_of(&second)).abs() < 1e-9);
    }

    #[test]
    fn right_alignment_makes_the_line_ends_flush() {
        let library = FontLibrary::bundled();
        let mut s = spec("IIII\nI");
        s.alignment = TextAlign::Right;
        let glyphs = layout_glyphs(&library, &s).expect("layout");
        let end_of = |line: usize| {
            glyphs
                .iter()
                .filter(|g| g.line == line)
                .map(|g| g.x + g.advance)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        assert!((end_of(0) - end_of(1)).abs() < 1e-9);
    }

    #[test]
    fn character_spacing_widens_the_run() {
        let library = FontLibrary::bundled();
        let tight = line_width(&library, &spec("AV"), "AV").expect("width");
        let mut loose_spec = spec("AV");
        loose_spec.char_spacing = 0.5;
        let loose = line_width(&library, &loose_spec, "AV").expect("width");
        assert!((loose - tight - 2.0 * 0.5 * 10.0).abs() < 1e-9);
    }

    #[test]
    fn turning_kerning_off_does_not_make_text_narrower() {
        let library = FontLibrary::bundled();
        let with = line_width(&library, &spec("AV"), "AV").expect("width");
        let mut off = spec("AV");
        off.kerning = false;
        let without = line_width(&library, &off, "AV").expect("width");
        assert!(without >= with - 1e-9, "{without} vs {with}");
    }

    #[test]
    fn bold_is_at_least_as_wide_as_regular() {
        let library = FontLibrary::bundled();
        let plain = line_width(&library, &spec("Handgloves"), "Handgloves").expect("width");
        let mut bold = spec("Handgloves");
        bold.bold = true;
        let heavy = line_width(&library, &bold, "Handgloves").expect("width");
        assert!(heavy >= plain - 1e-9);
    }

    #[test]
    fn a_bad_size_is_rejected() {
        let library = FontLibrary::bundled();
        let mut s = spec("A");
        s.size_mm = 0.0;
        assert!(layout_text(&library, &s).is_err());
        s.size_mm = 10.0;
        s.line_spacing = 0.0;
        assert!(layout_text(&library, &s).is_err());
        s.line_spacing = 1.0;
        s.char_spacing = f64::NAN;
        assert!(layout_text(&library, &s).is_err());
    }

    #[test]
    fn empty_text_lays_out_to_nothing() {
        let library = FontLibrary::bundled();
        assert!(layout_text(&library, &spec("")).expect("layout").is_empty());
    }

    #[test]
    fn a_block_wraps_at_the_column_width() {
        let library = FontLibrary::bundled();
        let block = TextBlockSpec {
            text: spec("the quick brown fox jumps over the lazy dog"),
            width_mm: 60.0,
        };
        let wrapped = wrap_text(&library, &block.text, block.width_mm).expect("wrap");
        assert!(wrapped.lines().count() > 1);
        for line in wrapped.lines() {
            assert!(line_width(&library, &block.text, line).expect("width") <= 60.0 + 1e-6);
        }
        let paths = layout_text_block(&library, &block).expect("layout");
        assert!(!paths.is_empty());
    }

    #[test]
    fn a_block_keeps_paragraph_breaks() {
        let library = FontLibrary::bundled();
        let wrapped = wrap_text(&library, &spec("one\ntwo"), 200.0).expect("wrap");
        assert_eq!(wrapped, "one\ntwo");
    }

    #[test]
    fn a_block_rejects_a_bad_width() {
        let library = FontLibrary::bundled();
        let block = TextBlockSpec {
            text: spec("hello"),
            width_mm: 0.0,
        };
        assert!(layout_text_block(&library, &block).is_err());
    }

    #[test]
    fn text_on_a_straight_line_matches_plain_layout() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(200.0, 0.0));
        let path = sub.into_path();
        let on_curve = text_on_curve(
            &library,
            &spec("HELLO"),
            &path,
            0.0,
            TextSide::Left,
            TextAlign::Left,
            0.0,
            FLAT,
        )
        .expect("layout");
        let plain = layout_text(&library, &spec("HELLO")).expect("layout");
        assert_eq!(on_curve.len(), plain.len());
        let a = outlines_bbox(&on_curve).expect("bbox");
        let b = outlines_bbox(&plain).expect("bbox");
        assert!((a.width() - b.width()).abs() < 1e-6);
        assert!((a.min.x - b.min.x).abs() < 1e-6);
    }

    #[test]
    fn text_on_a_curve_follows_the_offset() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(200.0, 0.0));
        let path = sub.into_path();
        let lifted = text_on_curve(
            &library,
            &spec("HELLO"),
            &path,
            5.0,
            TextSide::Left,
            TextAlign::Left,
            0.0,
            FLAT,
        )
        .expect("layout");
        let bbox = outlines_bbox(&lifted).expect("bbox");
        // Round letters overshoot the baseline a little, so allow for that.
        assert!((bbox.min.y - 5.0).abs() < 0.2, "min y {}", bbox.min.y);
    }

    #[test]
    fn text_on_a_curve_can_be_centred_along_it() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(200.0, 0.0));
        let path = sub.into_path();
        let centred = text_on_curve(
            &library,
            &spec("HELLO"),
            &path,
            0.0,
            TextSide::Left,
            TextAlign::Centre,
            0.0,
            FLAT,
        )
        .expect("layout");
        let bbox = outlines_bbox(&centred).expect("bbox");
        assert!(
            (bbox.centre().x - 100.0).abs() < 1.0,
            "centre {}",
            bbox.centre().x
        );
    }

    #[test]
    fn curve_text_spreads_fills_and_stays_upright() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(200.0, 0.0));
        let path = sub.into_path();
        let plain = CurveLayout::along(0.0, TextSide::Left, TextAlign::Left, 0.0);
        let width = |layout: &CurveLayout| {
            let laid =
                text_on_curve_with(&library, &spec("HELLO"), &path, layout, FLAT).expect("layout");
            outlines_bbox(&laid).expect("bbox")
        };
        let base = width(&plain);
        let spread = width(&CurveLayout {
            spacing: 2.0,
            ..plain
        });
        assert!(spread.width() > base.width() * 1.5, "{spread:?} {base:?}");
        let filled = width(&CurveLayout {
            fill: true,
            ..plain
        });
        assert!((filled.width() - 200.0).abs() < 20.0, "{filled:?}");
        let mut arc = SubPath::open(Point::new(0.0, 0.0));
        arc.line_to(Point::new(0.0, 200.0));
        let upright = text_on_curve_with(
            &library,
            &spec("HELLO"),
            &arc.into_path(),
            &CurveLayout {
                keep_vertical: true,
                ..plain
            },
            FLAT,
        )
        .expect("layout");
        let first = outlines_bbox(&upright[..1]).expect("bbox");
        assert!(
            first.height() > first.width() * 0.8,
            "letters stay upright: {first:?}"
        );
    }

    #[test]
    fn text_longer_than_an_open_curve_carries_on_past_the_end() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(20.0, 0.0));
        let path = sub.into_path();
        let long = text_on_curve(
            &library,
            &spec("HELLO WORLD"),
            &path,
            0.0,
            TextSide::Left,
            TextAlign::Left,
            0.0,
            FLAT,
        )
        .expect("layout");
        let lefts: Vec<f64> = long
            .iter()
            .filter_map(|p| outlines_bbox(std::slice::from_ref(p)))
            .map(|b| b.min.x)
            .collect();
        assert!(
            lefts.windows(2).all(|w| w[1] > w[0]),
            "letters keep moving on: {lefts:?}"
        );
        assert!(lefts.last().copied().unwrap_or(0.0) > 20.0);
    }

    #[test]
    fn text_on_a_closed_curve_wraps_round_instead_of_stopping() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(20.0, 0.0));
        sub.line_to(Point::new(20.0, 20.0));
        sub.line_to(Point::new(0.0, 20.0));
        sub.close_with_line(FLAT);
        let square = sub.into_path();
        let wrapped = text_on_curve(
            &library,
            &spec("ABCDEFGHIJKLMNOP"),
            &square,
            0.0,
            TextSide::Left,
            TextAlign::Left,
            0.0,
            FLAT,
        )
        .expect("layout");
        let bbox = outlines_bbox(&wrapped).expect("bbox");
        // Every letter stays on the square rather than trailing off past it.
        assert!(bbox.max.x < 40.0 && bbox.min.x > -20.0, "{bbox:?}");
    }

    #[test]
    fn text_on_the_right_side_runs_the_other_way() {
        let library = FontLibrary::bundled();
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(200.0, 0.0));
        let path = sub.into_path();
        let flipped = text_on_curve(
            &library,
            &spec("HELLO"),
            &path,
            5.0,
            TextSide::Right,
            TextAlign::Left,
            0.0,
            FLAT,
        )
        .expect("layout");
        let bbox = outlines_bbox(&flipped).expect("bbox");
        assert!((bbox.max.y + 5.0).abs() < 0.2, "max y {}", bbox.max.y);
        assert!(bbox.min.y < bbox.max.y);
    }

    #[test]
    fn text_needs_a_curve_with_length() {
        let library = FontLibrary::bundled();
        assert!(text_on_curve(
            &library,
            &spec("HI"),
            &Path::new(),
            0.0,
            TextSide::Left,
            TextAlign::Left,
            0.0,
            FLAT
        )
        .is_err());
    }

    #[test]
    fn arc_text_sits_on_the_circle() {
        let library = FontLibrary::bundled();
        let paths = arc_text(
            &library,
            &spec("HELLO"),
            Point::ORIGIN,
            60.0,
            0.0,
            std::f64::consts::PI,
            false,
            FLAT,
        )
        .expect("layout");
        assert_eq!(paths.len(), 5);
        let bbox = outlines_bbox(&paths).expect("bbox");
        assert!(bbox.max.x <= 75.0 && bbox.min.x >= -75.0);
        for path in &paths {
            let b = crate::geom::metrics::path_bbox(path).expect("bbox");
            let r = b.centre().distance(Point::ORIGIN);
            assert!(r > 55.0 && r < 75.0, "glyph radius {r}");
        }
    }

    #[test]
    fn arc_text_inside_flips_the_letters() {
        let library = FontLibrary::bundled();
        let outside = arc_text(
            &library,
            &spec("HELLO"),
            Point::ORIGIN,
            60.0,
            0.0,
            std::f64::consts::PI,
            false,
            FLAT,
        )
        .expect("layout");
        let inside = arc_text(
            &library,
            &spec("HELLO"),
            Point::ORIGIN,
            60.0,
            0.0,
            std::f64::consts::PI,
            true,
            FLAT,
        )
        .expect("layout");
        let a = outlines_bbox(&outside).expect("bbox");
        let b = outlines_bbox(&inside).expect("bbox");
        assert!(a.max.y > b.max.y, "outside {} inside {}", a.max.y, b.max.y);
    }

    #[test]
    fn arc_text_rejects_bad_geometry() {
        let library = FontLibrary::bundled();
        assert!(arc_text(
            &library,
            &spec("HI"),
            Point::ORIGIN,
            0.0,
            0.0,
            1.0,
            false,
            FLAT
        )
        .is_err());
        assert!(arc_text(
            &library,
            &spec("HI"),
            Point::ORIGIN,
            10.0,
            0.0,
            0.0,
            false,
            FLAT
        )
        .is_err());
    }

    #[test]
    fn outlines_bbox_of_nothing_is_none() {
        assert!(outlines_bbox(&[]).is_none());
    }

    #[test]
    fn layout_is_deterministic() {
        let library = FontLibrary::bundled();
        let a = layout_text(&library, &spec("Deterministic")).expect("layout");
        let b = layout_text(&library, &spec("Deterministic")).expect("layout");
        assert_eq!(a, b);
    }
}
