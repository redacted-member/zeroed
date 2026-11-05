//! Letters and monograms from the bundled Fira Sans faces (SIL Open Font
//! License, see `assets/fonts/README.md`): capitals, numbers, monograms in
//! a ring, and raised letters rounded over with Create Shape. Glyph
//! outlines keep the font's own curves.

use crate::error::CoreError;
use crate::geom::text::{layout_text, FontLibrary, TextSpec};
use crate::geom::{flatten_path, transform_path, BBox, Mat3, Path, Tolerance, Vec2};

use super::art::{self, p, Ring};
use super::{Adder, Entry};

const SOURCE: &str = "letters.rs";

/// The outline of `text` in the bundled face, `size` millimetres to the
/// em, centred on the origin.
fn glyphs(text: &str, size: f64, bold: bool, italic: bool) -> Result<Vec<Path>, CoreError> {
    let library = FontLibrary::bundled();
    let spec = TextSpec {
        text: text.to_string(),
        bold,
        italic,
        size_mm: size,
        ..TextSpec::default()
    };
    let paths = layout_text(&library, &spec)?;
    let bbox = paths
        .iter()
        .flat_map(|p| flatten_path(p, art::tol()))
        .filter_map(|l| BBox::of_points(l.points))
        .reduce(BBox::union)
        .ok_or_else(|| CoreError::validation("the text draws nothing"))?;
    let c = bbox.centre();
    let m = Mat3::translate(Vec2::new(-c.x, -c.y));
    Ok(paths.iter().map(|p| transform_path(p, &m)).collect())
}

/// Glyph outlines as rings, for reliefs and booleans.
fn rings(paths: &[Path]) -> Vec<Ring> {
    let tol = Tolerance::new(0.01).unwrap_or(Tolerance::FLATTEN);
    paths
        .iter()
        .flat_map(|p| flatten_path(p, tol))
        .map(|l| l.points)
        .filter(|r| r.len() >= 3)
        .collect()
}

const CAPITALS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub(super) fn entries(out: &mut Vec<Entry>) {
    let mut add = Adder {
        out,
        category: "Letters and Monograms/Bold Capitals",
        source: SOURCE,
    };
    for ch in CAPITALS.chars() {
        let text = ch.to_string();
        add.vectors(
            &format!("Bold {ch}"),
            &["letter", "capital", "bold"],
            move || glyphs(&text, 60.0, true, false),
        );
    }
    let mut add = Adder {
        out: add.out,
        category: "Letters and Monograms/Italic Capitals",
        source: SOURCE,
    };
    for ch in CAPITALS.chars() {
        let text = ch.to_string();
        add.vectors(
            &format!("Italic {ch}"),
            &["letter", "capital", "italic"],
            move || glyphs(&text, 60.0, false, true),
        );
    }
    let mut add = Adder {
        out: add.out,
        category: "Letters and Monograms/Numbers",
        source: SOURCE,
    };
    for (ch, word) in [
        ('0', "Zero"),
        ('1', "One"),
        ('2', "Two"),
        ('3', "Three"),
        ('4', "Four"),
        ('5', "Five"),
        ('6', "Six"),
        ('7', "Seven"),
        ('8', "Eight"),
        ('9', "Nine"),
        ('&', "Ampersand"),
        ('#', "Number Sign"),
    ] {
        let text = ch.to_string();
        add.vectors(word, &["number", "digit", "house number"], move || {
            glyphs(&text, 60.0, true, false)
        });
    }
    let mut add = Adder {
        out: add.out,
        category: "Letters and Monograms/Monograms",
        source: SOURCE,
    };
    for ch in CAPITALS.chars() {
        let text = ch.to_string();
        add.vectors(
            &format!("Monogram {ch}"),
            &["monogram", "initial", "ring"],
            move || {
                let mut paths = art::paths(&art::band(&art::circle(p(0.0, 0.0), 30.0, 360), 3.0)?);
                paths.extend(glyphs(&text, 36.0, true, false)?);
                Ok(paths)
            },
        );
    }
    let mut add = Adder {
        out: add.out,
        category: "Letters and Monograms/Raised Letters",
        source: SOURCE,
    };
    for ch in CAPITALS.chars() {
        let text = ch.to_string();
        add.relief(
            &format!("Raised {ch}"),
            &["letter", "capital", "raised", "rounded"],
            move || art::dome(&rings(&glyphs(&text, 50.0, true, false)?), 0.0),
        );
    }
    let mut add = Adder {
        out: add.out,
        category: "Letters and Monograms/Monogram Reliefs",
        source: SOURCE,
    };
    for ch in CAPITALS.chars() {
        let text = ch.to_string();
        add.relief(
            &format!("Monogram {ch} Relief"),
            &["monogram", "initial", "medallion"],
            move || {
                let letter = art::raised(
                    art::dome(&rings(&glyphs(&text, 34.0, true, false)?), 0.0)?,
                    1.0,
                );
                let disc = art::field_fn(64.0, 64.0, |x, y| {
                    let r = x.hypot(y);
                    if r > 31.5 {
                        f64::NAN
                    } else if r > 28.0 {
                        1.0 + 1.5 * art::round_cap(((31.5 - r).min(r - 28.0) / 1.75).min(1.0))
                    } else {
                        1.0
                    }
                });
                Ok(art::highest(&disc, &letter))
            },
        );
    }
}
