//! The Text Selection tool's adjustments to straight text: extra space after
//! single letters (kerning), extra space under single lines, and arching
//! the text over the top of a circle or round the bottom of one.
//!
//! Gaps are fractions of the text height, so they keep their look when the
//! size changes. Letter gaps are counted over the letters that draw
//! something on each line (spaces are skipped), which is also how
//! `text.glyphs` numbers them. The arch is the height of the bend in
//! millimetres: positive lifts the middle of the line (letters stand out
//! from the circle), negative drops it (letters point in to the centre).

use serde::Serialize;

use super::TextSpecJson;
use crate::error::CoreError;
use crate::geom::{
    self, path_bbox, transform_path, BBox, FontLibrary, Mat3, Path, Point, TextAlign, TextSpec,
    Vec2,
};

/// One drawn letter as `text.glyphs` reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphBox {
    /// Line, from 0 at the top.
    pub line: usize,
    /// Drawn letter on that line, from 0.
    pub index: usize,
    /// `[minX, minY, maxX, maxY]` in job millimetres.
    pub bbox: [f64; 4],
}

/// True when the spec asks for any Text Selection adjustment.
pub fn is_adjusted(json: &TextSpecJson) -> bool {
    json.letter_gaps
        .iter()
        .any(|line| line.iter().any(|g| *g != 0.0))
        || json.line_gaps.iter().any(|g| *g != 0.0)
        || json.arch.is_some_and(|a| a != 0.0)
}

/// Straight text laid out with its letter and line gaps, at `origin`, with
/// each drawn letter's line and index.
pub fn spaced_glyphs(
    fonts: &FontLibrary,
    spec: &TextSpec,
    json: &TextSpecJson,
    origin: Point,
) -> Result<Vec<(usize, usize, Path)>, CoreError> {
    let placed = geom::layout_glyphs(fonts, spec)?;
    let height = json.size_mm;
    let lines = placed.iter().map(|g| g.line + 1).max().unwrap_or(0);
    // Extra width each line gains, to keep centred and right text in place.
    let mut added = vec![0.0; lines];
    let mut out = Vec::new();
    let mut shifts: Vec<f64> = Vec::with_capacity(placed.len());
    let mut drawn = vec![0usize; lines];
    let mut run = vec![0.0; lines];
    for glyph in &placed {
        shifts.push(run[glyph.line]);
        if !glyph.outline.is_empty() {
            let k = drawn[glyph.line];
            let gap = json
                .letter_gaps
                .get(glyph.line)
                .and_then(|l| l.get(k))
                .copied()
                .unwrap_or(0.0);
            run[glyph.line] += gap * height;
            drawn[glyph.line] += 1;
        }
    }
    for (line, extra) in added.iter_mut().enumerate() {
        // The gap after the last letter adds nothing visible.
        let last = drawn[line].saturating_sub(1);
        let tail = json
            .letter_gaps
            .get(line)
            .and_then(|l| l.get(last))
            .copied()
            .unwrap_or(0.0);
        *extra = run[line] - tail * height;
    }
    let mut index = vec![0usize; lines];
    for (glyph, shift) in placed.iter().zip(shifts) {
        if glyph.outline.is_empty() {
            continue;
        }
        let align = match spec.alignment {
            TextAlign::Left => 0.0,
            TextAlign::Centre => -added[glyph.line] * 0.5,
            TextAlign::Right => -added[glyph.line],
        };
        let down: f64 = json.line_gaps.iter().take(glyph.line).sum::<f64>() * height;
        let m = Mat3::translate(Vec2::new(
            origin.x + glyph.x + shift + align,
            origin.y + glyph.y - down,
        ));
        out.push((
            glyph.line,
            index[glyph.line],
            transform_path(&glyph.outline, &m),
        ));
        index[glyph.line] += 1;
    }
    Ok(out)
}

fn ink(paths: &[Path]) -> Option<BBox> {
    paths.iter().filter_map(path_bbox).reduce(|a, b| a.union(b))
}

/// Where arched text sits: the circle its baseline follows and the angles
/// of the two ends of that baseline, for the Text Selection handles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcInfo {
    /// Centre of the circle, in job millimetres.
    pub centre: Point,
    /// Radius of the baseline circle, in millimetres.
    pub radius: f64,
    /// Angle of the start of the line (its left end), radians.
    pub start: f64,
    /// Angle of the end of the line (its right end), radians.
    pub end: f64,
    /// Length of the line along the arc: the straight width of the text.
    pub width: f64,
    /// True when the letters stand out from the circle (arched upwards).
    pub outward: bool,
}

/// Glyph outlines with their line and letter index.
type Glyphs = Vec<(usize, usize, Path)>;

/// Bends straight glyphs onto a circle, each letter kept rigid and placed
/// by its distance along the line, so letter gaps survive the arch. The bend
/// is the height of the arch over the width of the text; the radius follows
/// from them as for a chord of that width.
fn arch_glyphs(straight: Glyphs, bend: f64, origin: Point) -> (Glyphs, Option<ArcInfo>) {
    let paths: Vec<Path> = straight.iter().map(|(_, _, p)| p.clone()).collect();
    let Some(bbox) = ink(&paths) else {
        return (straight, None);
    };
    if bend.abs() < 1e-9 {
        return (straight, None);
    }
    let width = bbox.width().max(1e-6);
    let half = 2.0 * (2.0 * bend.abs() / width).atan();
    let radius = width / 2.0 / half.sin().max(1e-9);
    let mid = bbox.centre().x;
    let outward = bend > 0.0;
    let centre = if outward {
        Point::new(mid, origin.y + bend - radius)
    } else {
        Point::new(mid, origin.y - bend.abs() + radius)
    };
    let top = std::f64::consts::FRAC_PI_2;
    let out = straight
        .into_iter()
        .map(|(line, index, path)| {
            let Some(b) = path_bbox(&path) else {
                return (line, index, path);
            };
            let u = b.centre().x - mid;
            let turn = if outward { -u / radius } else { u / radius };
            let phi = if outward { top + turn } else { -top + turn };
            let on = Point::new(centre.x + radius * phi.cos(), centre.y + radius * phi.sin());
            let m = Mat3::translate(Vec2::new(-b.centre().x, -origin.y))
                .then(&Mat3::rotate(turn))
                .then(&Mat3::translate(Vec2::new(on.x, on.y)));
            (line, index, transform_path(&path, &m))
        })
        .collect();
    let reach = width / 2.0 / radius;
    let (start, end) = if outward {
        (top + reach, top - reach)
    } else {
        (-top - reach, -top + reach)
    };
    let info = ArcInfo {
        centre,
        radius,
        start,
        end,
        width,
        outward,
    };
    (out, Some(info))
}

/// Straight text with its gaps, arched when asked, with each letter's line
/// and index and the arc it follows.
fn laid_glyphs(
    fonts: &FontLibrary,
    spec: &TextSpec,
    json: &TextSpecJson,
    origin: Point,
) -> Result<(Glyphs, Option<ArcInfo>), CoreError> {
    let origin = anchored_origin(fonts, spec, json, origin)?;
    let straight = spaced_glyphs(fonts, spec, json, origin)?;
    Ok(arch_glyphs(straight, json.arch.unwrap_or(0.0), origin))
}

/// The pen origin that puts the anchor point of the unadjusted text's box on
/// `origin`, as plain text is placed, so that changing the spacing or the
/// arch leaves the text where it was.
fn anchored_origin(
    fonts: &FontLibrary,
    spec: &TextSpec,
    json: &TextSpecJson,
    origin: Point,
) -> Result<Point, CoreError> {
    let Some(anchor) = json.anchor else {
        return Ok(origin);
    };
    let Some(b) = ink(&geom::layout_text(fonts, spec)?) else {
        return Ok(origin);
    };
    let (fx, fy) = anchor.fractions();
    Ok(Point::new(
        origin.x - (b.min.x + b.width() * fx),
        origin.y - (b.min.y + b.height() * fy),
    ))
}

/// Straight text with its adjustments, arched when asked. Letter and line
/// gaps are kept on arched text.
pub fn adjusted_text(
    fonts: &FontLibrary,
    spec: &TextSpec,
    json: &TextSpecJson,
    origin: Point,
) -> Result<Vec<Path>, CoreError> {
    Ok(laid_glyphs(fonts, spec, json, origin)?
        .0
        .into_iter()
        .map(|(_, _, p)| p)
        .collect())
}

/// The drawn letters of the text as it is laid out (arched too), turned by
/// `rotation` radians about the origin, for the Text Selection tool; and the
/// arc arched text follows, turned the same way.
pub fn glyph_boxes(
    fonts: &FontLibrary,
    spec: &TextSpec,
    json: &TextSpecJson,
    origin: Point,
    rotation: f64,
) -> Result<(Vec<GlyphBox>, Option<ArcInfo>), CoreError> {
    let (glyphs, arc) = laid_glyphs(fonts, spec, json, origin)?;
    let m = Mat3::rotate_about(rotation, origin);
    let boxes = glyphs
        .into_iter()
        .filter_map(|(line, index, path)| {
            path_bbox(&transform_path(&path, &m)).map(|b| GlyphBox {
                line,
                index,
                bbox: [b.min.x, b.min.y, b.max.x, b.max.y],
            })
        })
        .collect();
    let arc = arc.map(|a| ArcInfo {
        centre: m.apply(a.centre),
        start: a.start + rotation,
        end: a.end + rotation,
        ..a
    });
    Ok((boxes, arc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64) -> (usize, usize, Path) {
        let path = geom::rect(Point::new(x, 0.0), Point::new(x + 4.0, 5.0)).expect("rect");
        (0, 0, path)
    }

    #[test]
    fn arching_keeps_the_letters_on_the_circle_at_their_spacing() {
        // Three letters, the last one pushed away by a wider gap.
        let straight = vec![square(0.0), square(6.0), square(20.0)];
        let (bent, arc) = arch_glyphs(straight, 6.0, Point::new(0.0, 0.0));
        let arc = arc.expect("arched");
        assert!(arc.outward);
        // Each letter's baseline middle sits on the circle, and the arc
        // distances between letter middles match the straight ones.
        let mids: Vec<f64> = bent
            .iter()
            .map(|(_, _, p)| {
                let b = path_bbox(p).expect("box");
                let c = b.centre();
                let d = arc.centre.to(c);
                d.y.atan2(d.x)
            })
            .collect();
        let gap1 = (mids[0] - mids[1]) * arc.radius;
        let gap2 = (mids[1] - mids[2]) * arc.radius;
        assert!((gap2 / gap1 - 14.0 / 6.0).abs() < 0.05, "{gap1} {gap2}");
        // Every letter stands on the circle: its middle is half its height
        // out from the baseline.
        for (_, _, p) in &bent {
            let nodes = p.subpaths[0].nodes();
            let mut sum = Point::new(0.0, 0.0);
            for n in &nodes[..4] {
                sum = Point::new(sum.x + n.x / 4.0, sum.y + n.y / 4.0);
            }
            assert!((arc.centre.distance(sum) - (arc.radius + 2.5)).abs() < 1e-6);
        }
    }

    #[test]
    fn a_downward_arch_hangs_the_letters_inside_the_circle() {
        let straight = vec![square(0.0), square(10.0)];
        let (bent, arc) = arch_glyphs(straight, -4.0, Point::new(0.0, 0.0));
        let arc = arc.expect("arched");
        assert!(!arc.outward);
        for (_, _, p) in &bent {
            let c = path_bbox(p).expect("box").centre();
            assert!(arc.centre.distance(c) < arc.radius);
        }
    }
}
