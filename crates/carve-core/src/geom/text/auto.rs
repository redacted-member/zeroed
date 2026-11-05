//! Auto Layout Text: a block of text sized to fill a box.
//!
//! The text is laid out as usual, then scaled as a whole (keeping its
//! proportions) until it fits inside the box less its margin. Space left
//! over can then be taken up: across each line by widening the gaps between
//! words, the gaps between letters, the letters themselves or the whole
//! line; and down the block by spreading the lines apart or stretching the
//! letters taller. Lines are aligned in the box by the text's alignment and
//! the block is centred in it vertically.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::super::bbox::BBox;
use super::super::metrics::path_bbox;
use super::super::path::Path;
use super::super::point::{Mat3, Point, Vec2};
use super::super::transform::transform_path;
use super::{layout_glyphs, FontLibrary, TextAlign, TextSpec};

/// How much of the box is kept clear round the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutMargin {
    /// The text fills the box.
    None,
    /// A tenth of the box on each side (the default).
    #[default]
    Normal,
    /// A fifth of the box on each side.
    Wide,
}

impl LayoutMargin {
    fn fraction(self) -> f64 {
        match self {
            LayoutMargin::None => 0.0,
            LayoutMargin::Normal => 0.1,
            LayoutMargin::Wide => 0.2,
        }
    }
}

/// How spare height is taken up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalStretch {
    /// Leave it (the default).
    #[default]
    None,
    /// Spread the lines apart.
    LineSpacing,
    /// Make the letters taller.
    CharacterHeight,
}

/// How spare width on each line is taken up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalStretch {
    /// Leave it (the default).
    #[default]
    None,
    /// Widen the gaps between words.
    WordSpacing,
    /// Widen the gaps between letters.
    LetterSpacing,
    /// Make the letters wider.
    CharacterWidth,
    /// Make the whole line larger.
    CharacterSize,
}

/// Where and how text is fitted, in job millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoLayout {
    /// `[minX, minY, maxX, maxY]` of the box the text fills.
    pub bounds: [f64; 4],
    /// Clear border inside the box.
    #[serde(default)]
    pub margin: LayoutMargin,
    /// Use of spare height.
    #[serde(default)]
    pub vertical: VerticalStretch,
    /// Use of spare width.
    #[serde(default)]
    pub horizontal: HorizontalStretch,
}

/// One glyph as drawn: its outline already placed, and where it sits.
struct Glyph {
    path: Path,
    line: usize,
    /// True for a space: nothing drawn, but a word break.
    space: bool,
}

fn ink(paths: &[&Path]) -> Option<BBox> {
    paths
        .iter()
        .filter_map(|p| path_bbox(p))
        .reduce(|a, b| a.union(b))
}

fn apply(glyphs: &mut [Glyph], m: &Mat3, pick: impl Fn(&Glyph) -> bool) {
    for glyph in glyphs.iter_mut().filter(|g| pick(g)) {
        glyph.path = transform_path(&glyph.path, m);
    }
}

fn line_box(glyphs: &[Glyph], line: usize) -> Option<BBox> {
    ink(&glyphs
        .iter()
        .filter(|g| g.line == line && !g.space)
        .map(|g| &g.path)
        .collect::<Vec<_>>())
}

/// Lays `spec` out to fill `auto.bounds`, answering one path per drawn glyph.
///
/// # Errors
/// [`CoreError::Validation`] for a box with no area or text with nothing
/// to draw, and the errors of [`layout_glyphs`].
pub fn layout_auto(
    library: &FontLibrary,
    spec: &TextSpec,
    auto: &AutoLayout,
) -> Result<Vec<Path>, CoreError> {
    let [x0, y0, x1, y1] = auto.bounds;
    if !(x1 - x0 > 0.0 && y1 - y0 > 0.0) || auto.bounds.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation(
            "the text box must have a width and height",
        ));
    }
    let m = auto.margin.fraction();
    let inner = BBox::new(
        Point::new(x0 + (x1 - x0) * m, y0 + (y1 - y0) * m),
        Point::new(x1 - (x1 - x0) * m, y1 - (y1 - y0) * m),
    );
    let placed = layout_glyphs(library, spec)?;
    let mut glyphs: Vec<Glyph> = placed
        .into_iter()
        .map(|g| Glyph {
            space: g.outline.is_empty(),
            path: transform_path(&g.outline, &Mat3::translate(Vec2::new(g.x, g.y))),
            line: g.line,
        })
        .collect();
    let lines = glyphs.iter().map(|g| g.line + 1).max().unwrap_or(0);
    let all = |glyphs: &[Glyph]| {
        ink(&glyphs
            .iter()
            .filter(|g| !g.space)
            .map(|g| &g.path)
            .collect::<Vec<_>>())
    };
    let natural =
        all(&glyphs).ok_or_else(|| CoreError::validation("there is no text to lay out"))?;

    // Fit the whole block, keeping its proportions.
    let f = (inner.width() / natural.width()).min(inner.height() / natural.height());
    let fit = Mat3::translate(Vec2::new(-natural.min.x, -natural.min.y))
        .then(&Mat3::scale(f, f))
        .then(&Mat3::translate(Vec2::new(inner.min.x, inner.min.y)));
    apply(&mut glyphs, &fit, |_| true);

    // Spare width, line by line.
    for line in 0..lines {
        let Some(bbox) = line_box(&glyphs, line) else {
            continue;
        };
        let spare = inner.width() - bbox.width();
        if spare <= 1e-9 || bbox.width() <= 0.0 {
            continue;
        }
        let on_line = |g: &Glyph| g.line == line;
        match auto.horizontal {
            HorizontalStretch::None => {}
            HorizontalStretch::CharacterWidth => {
                let s = inner.width() / bbox.width();
                let m = Mat3::translate(Vec2::new(-bbox.min.x, 0.0))
                    .then(&Mat3::scale(s, 1.0))
                    .then(&Mat3::translate(Vec2::new(inner.min.x, 0.0)));
                apply(&mut glyphs, &m, on_line);
            }
            HorizontalStretch::CharacterSize => {
                let s = inner.width() / bbox.width();
                let m = Mat3::translate(Vec2::new(-bbox.min.x, -bbox.min.y))
                    .then(&Mat3::scale(s, s))
                    .then(&Mat3::translate(Vec2::new(inner.min.x, bbox.min.y)));
                apply(&mut glyphs, &m, on_line);
            }
            HorizontalStretch::LetterSpacing | HorizontalStretch::WordSpacing => {
                let words = auto.horizontal == HorizontalStretch::WordSpacing;
                let indices: Vec<usize> = (0..glyphs.len())
                    .filter(|i| glyphs[*i].line == line)
                    .collect();
                // Gaps: between drawn letters, or at each run of spaces.
                let mut steps = Vec::with_capacity(indices.len());
                let mut gaps = 0usize;
                let mut seen_ink = false;
                let mut pending_space = false;
                for &i in &indices {
                    if glyphs[i].space {
                        pending_space = true;
                        steps.push(gaps);
                        continue;
                    }
                    if seen_ink && (!words || pending_space) {
                        gaps += 1;
                    }
                    seen_ink = true;
                    pending_space = false;
                    steps.push(gaps);
                }
                if gaps > 0 {
                    let each = spare / gaps as f64;
                    for (&i, &step) in indices.iter().zip(&steps) {
                        let shift = Mat3::translate(Vec2::new(
                            each * step as f64 - (bbox.min.x - inner.min.x),
                            0.0,
                        ));
                        glyphs[i].path = transform_path(&glyphs[i].path, &shift);
                    }
                }
            }
        }
    }

    // Spare height down the block.
    if let Some(block) = all(&glyphs) {
        let spare = inner.height() - block.height();
        if spare > 1e-9 {
            match auto.vertical {
                VerticalStretch::None => {}
                VerticalStretch::CharacterHeight => {
                    let s = inner.height() / block.height();
                    let m = Mat3::translate(Vec2::new(0.0, -block.max.y))
                        .then(&Mat3::scale(1.0, s))
                        .then(&Mat3::translate(Vec2::new(0.0, block.max.y)));
                    apply(&mut glyphs, &m, |_| true);
                }
                VerticalStretch::LineSpacing if lines > 1 => {
                    let each = spare / (lines - 1) as f64;
                    for line in 1..lines {
                        let m = Mat3::translate(Vec2::new(0.0, -each * line as f64));
                        apply(&mut glyphs, &m, |g| g.line == line);
                    }
                }
                VerticalStretch::LineSpacing => {}
            }
        }
    }

    // Align each line across the box and centre the block down it.
    for line in 0..lines {
        let Some(bbox) = line_box(&glyphs, line) else {
            continue;
        };
        let dx = match spec.alignment {
            TextAlign::Left => inner.min.x - bbox.min.x,
            TextAlign::Centre => inner.centre().x - bbox.centre().x,
            TextAlign::Right => inner.max.x - bbox.max.x,
        };
        apply(&mut glyphs, &Mat3::translate(Vec2::new(dx, 0.0)), |g| {
            g.line == line
        });
    }
    if let Some(block) = all(&glyphs) {
        let dy = inner.centre().y - block.centre().y;
        apply(&mut glyphs, &Mat3::translate(Vec2::new(0.0, dy)), |_| true);
    }
    Ok(glyphs
        .into_iter()
        .filter(|g| !g.space)
        .map(|g| g.path)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> TextSpec {
        TextSpec {
            text: text.into(),
            font_family: FontLibrary::FALLBACK_FAMILY.into(),
            bold: false,
            italic: false,
            size_mm: 10.0,
            alignment: TextAlign::Left,
            line_spacing: 1.0,
            char_spacing: 0.0,
            kerning: true,
        }
    }

    fn fitted(text: &str, auto: AutoLayout) -> BBox {
        let library = FontLibrary::bundled();
        let paths = layout_auto(&library, &spec(text), &auto).expect("layout");
        ink(&paths.iter().collect::<Vec<_>>()).expect("ink")
    }

    fn auto(
        margin: LayoutMargin,
        vertical: VerticalStretch,
        horizontal: HorizontalStretch,
    ) -> AutoLayout {
        AutoLayout {
            bounds: [0.0, 0.0, 200.0, 100.0],
            margin,
            vertical,
            horizontal,
        }
    }

    #[test]
    fn text_fills_the_box_inside_its_margin() {
        let none = fitted(
            "WIDE TEXT",
            auto(
                LayoutMargin::None,
                VerticalStretch::None,
                HorizontalStretch::None,
            ),
        );
        assert!(
            (none.width() - 200.0).abs() < 1e-6 || (none.height() - 100.0).abs() < 1e-6,
            "{none:?}"
        );
        let normal = fitted(
            "WIDE TEXT",
            auto(
                LayoutMargin::Normal,
                VerticalStretch::None,
                HorizontalStretch::None,
            ),
        );
        assert!(
            normal.min.x >= 20.0 - 1e-6 && normal.max.x <= 180.0 + 1e-6,
            "{normal:?}"
        );
        // Centred down the box.
        assert!((normal.centre().y - 50.0).abs() < 1e-6, "{normal:?}");
    }

    #[test]
    fn spare_width_and_height_are_taken_up() {
        for horizontal in [
            HorizontalStretch::WordSpacing,
            HorizontalStretch::LetterSpacing,
            HorizontalStretch::CharacterWidth,
        ] {
            let b = fitted(
                "AB CD\nA LONGER LINE",
                auto(LayoutMargin::None, VerticalStretch::None, horizontal),
            );
            assert!((b.width() - 200.0).abs() < 1e-6, "{horizontal:?} {b:?}");
        }
        for vertical in [
            VerticalStretch::LineSpacing,
            VerticalStretch::CharacterHeight,
        ] {
            let b = fitted(
                "ONE\nTWO",
                auto(LayoutMargin::None, vertical, HorizontalStretch::None),
            );
            assert!(
                (b.height() - 100.0).abs() < 1e-6 || (b.width() - 200.0).abs() < 1e-6,
                "{vertical:?} {b:?}"
            );
            assert!(b.height() <= 100.0 + 1e-6);
        }
    }

    #[test]
    fn a_box_without_area_is_refused() {
        let library = FontLibrary::bundled();
        let mut bad = auto(
            LayoutMargin::None,
            VerticalStretch::None,
            HorizontalStretch::None,
        );
        bad.bounds = [0.0, 0.0, 0.0, 10.0];
        assert!(layout_auto(&library, &spec("A"), &bad).is_err());
    }
}
