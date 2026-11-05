//! Dimension geometry: extension lines, the dimension line, arrowheads and
//! the measured value set as text.
//!
//! Every function returns the pieces separately so the 2D view can draw them
//! in different weights, and `all_paths` gives the lot when they are wanted
//! as one set of vectors.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::path::{bulge_from_sweep, Path, SubPath};
use super::point::{Mat3, Point, Tolerance, Vec2};
use super::text::{layout_text, FontLibrary, TextAlign, TextSpec};
use super::transform::transform_path;

/// Which units a dimension reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionUnits {
    /// Millimetres, the kernel's own unit.
    Millimetres,
    /// Inches, converted at 25.4 mm to the inch.
    Inches,
}

impl DimensionUnits {
    /// The suffix shown after the number.
    pub fn suffix(self) -> &'static str {
        match self {
            DimensionUnits::Millimetres => "mm",
            DimensionUnits::Inches => "in",
        }
    }

    /// Converts a length in millimetres into these units.
    pub fn from_mm(self, mm: f64) -> f64 {
        match self {
            DimensionUnits::Millimetres => mm,
            DimensionUnits::Inches => mm / 25.4,
        }
    }
}

/// Everything about how a dimension looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionStyle {
    /// Font settings for the value. The alignment is overridden to centre
    /// for everything except a note, which is set to the left.
    pub text: TextSpec,
    /// Length of an arrowhead along the dimension line, in millimetres.
    pub arrow_length: f64,
    /// Full width of an arrowhead across the dimension line, in millimetres.
    pub arrow_width: f64,
    /// Gap between the measured point and where its extension line starts.
    pub extension_gap: f64,
    /// How far an extension line runs past the dimension line.
    pub extension_overshoot: f64,
    /// Gap between the dimension line and the bottom of the text.
    pub text_gap: f64,
    /// Decimal places shown.
    pub precision: usize,
    /// Units the value reads in.
    pub units: DimensionUnits,
    /// Show the unit suffix after the number.
    pub show_units: bool,
    /// Decimal places of an angle, separate from lengths.
    #[serde(default = "two")]
    pub angle_precision: usize,
    /// Text shown in place of the measured value, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_text: Option<String>,
    /// Place Dimensions on Layer: new dimensions go on the layer named
    /// `layer_name` (made mid grey when missing) instead of the active one.
    #[serde(default = "yes")]
    pub use_layer: bool,
    /// The layer new dimensions go on.
    #[serde(default = "dimensions_name")]
    pub layer_name: String,
}

fn two() -> usize {
    2
}

fn yes() -> bool {
    true
}

fn dimensions_name() -> String {
    "Dimensions".to_string()
}

impl Default for DimensionStyle {
    fn default() -> Self {
        DimensionStyle {
            text: TextSpec {
                size_mm: 5.0,
                alignment: TextAlign::Centre,
                ..TextSpec::default()
            },
            arrow_length: 3.0,
            arrow_width: 1.6,
            extension_gap: 1.0,
            extension_overshoot: 1.5,
            text_gap: 1.0,
            precision: 2,
            units: DimensionUnits::Millimetres,
            show_units: true,
            angle_precision: 2,
            custom_text: None,
            use_layer: true,
            layer_name: dimensions_name(),
        }
    }
}

impl DimensionStyle {
    fn check(&self) -> Result<(), CoreError> {
        for (name, value) in [
            ("arrow length", self.arrow_length),
            ("arrow width", self.arrow_width),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(CoreError::geometry(format!(
                    "{name} must be a positive number of millimeters, got {value}"
                )));
            }
        }
        for (name, value) in [
            ("extension gap", self.extension_gap),
            ("extension overshoot", self.extension_overshoot),
            ("text gap", self.text_gap),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(CoreError::geometry(format!(
                    "{name} cannot be negative, got {value}"
                )));
            }
        }
        if self.angle_precision > 6 {
            return Err(CoreError::geometry(format!(
                "an angle shows at most six decimal places, got {}",
                self.angle_precision
            )));
        }
        if self.precision > 6 {
            return Err(CoreError::geometry(format!(
                "a dimension shows at most six decimal places, got {}",
                self.precision
            )));
        }
        Ok(())
    }

    /// Formats a length in millimetres for display.
    pub fn format_length(&self, mm: f64) -> String {
        if let Some(text) = &self.custom_text {
            return text.clone();
        }
        let value = self.units.from_mm(mm);
        if self.show_units {
            format!("{:.*} {}", self.precision, value, self.units.suffix())
        } else {
            format!("{:.*}", self.precision, value)
        }
    }

    /// Formats an angle in radians for display, in degrees.
    pub fn format_angle(&self, radians: f64) -> String {
        if let Some(text) = &self.custom_text {
            return text.clone();
        }
        format!("{:.*}\u{b0}", self.angle_precision, radians.to_degrees())
    }
}

/// The pieces that make up one dimension.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    /// The thin lines running from the measured points out to the dimension
    /// line. A note has none.
    pub extension_lines: Vec<Path>,
    /// The line or arc carrying the arrowheads, or the leader of a note.
    pub dimension_line: Path,
    /// Closed arrowhead outlines.
    pub arrows: Vec<Path>,
    /// One path per glyph of the value text.
    pub text: Vec<Path>,
    /// The measured value: millimetres for a length, radians for an angle.
    pub value: f64,
    /// The text that was set.
    pub label: String,
}

impl Dimension {
    /// Every piece as one list of vectors, ready to add to the drawing.
    pub fn all_paths(&self) -> Vec<Path> {
        let mut out = Vec::new();
        out.extend(self.extension_lines.iter().cloned());
        out.push(self.dimension_line.clone());
        out.extend(self.arrows.iter().cloned());
        out.extend(self.text.iter().cloned());
        out
    }
}

fn segment(a: Point, b: Point) -> Path {
    let mut sub = SubPath::open(a);
    sub.line_to(b);
    sub.into_path()
}

/// A closed arrowhead with its tip at `tip`, pointing along `direction`.
fn arrowhead(tip: Point, direction: Vec2, style: &DimensionStyle) -> Result<Path, CoreError> {
    let dir = direction
        .normalised()
        .ok_or_else(|| CoreError::geometry("an arrowhead needs a direction"))?;
    let back = tip.offset(dir.scaled(-style.arrow_length));
    let side = dir.perp().scaled(style.arrow_width * 0.5);
    let mut sub = SubPath::open(tip);
    sub.line_to(back.offset(side))
        .line_to(back.offset(side.scaled(-1.0)))
        .line_to(tip);
    sub.closed = true;
    Ok(sub.into_path())
}

/// Lays the label out and sits it on the dimension line.
///
/// The text is kept the right way up: a dimension line that runs backwards
/// has its text turned round rather than set upside down.
fn place_text(
    library: &FontLibrary,
    style: &DimensionStyle,
    label: &str,
    anchor: Point,
    along: Vec2,
    align: TextAlign,
) -> Result<Vec<Path>, CoreError> {
    let mut spec = style.text.clone();
    spec.text = label.to_string();
    spec.alignment = align;
    let glyphs = layout_text(library, &spec)?;
    let dir = along.normalised().unwrap_or(Vec2::new(1.0, 0.0));
    let flip = dir.x < 0.0 || (dir.x.abs() < 1e-12 && dir.y < 0.0);
    let dir = if flip { dir.scaled(-1.0) } else { dir };
    let place = anchor.offset(dir.perp().scaled(style.text_gap));
    let m = Mat3::rotate(dir.angle()).then(&Mat3::translate(Vec2::new(place.x, place.y)));
    Ok(glyphs.iter().map(|g| transform_path(g, &m)).collect())
}

/// A dimension measuring the straight distance between two points.
///
/// `offset` moves the dimension line to the left of the direction from `a`
/// to `b`, in millimetres. A negative value puts it on the other side.
pub fn linear_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    a: Point,
    b: Point,
    offset: f64,
) -> Result<Dimension, CoreError> {
    style.check()?;
    let along = a
        .to(b)
        .normalised()
        .ok_or_else(|| CoreError::geometry("a dimension needs two different points"))?;
    let normal = along.perp();
    let value = a.distance(b);
    let a_end = a.offset(normal.scaled(offset));
    let b_end = b.offset(normal.scaled(offset));

    let sign = if offset >= 0.0 { 1.0 } else { -1.0 };
    let extension_lines = vec![
        segment(
            a.offset(normal.scaled(sign * style.extension_gap)),
            a_end.offset(normal.scaled(sign * style.extension_overshoot)),
        ),
        segment(
            b.offset(normal.scaled(sign * style.extension_gap)),
            b_end.offset(normal.scaled(sign * style.extension_overshoot)),
        ),
    ];
    let arrows = vec![
        arrowhead(a_end, along.scaled(-1.0), style)?,
        arrowhead(b_end, along, style)?,
    ];
    let label = style.format_length(value);
    let text = place_text(
        library,
        style,
        &label,
        a_end.midpoint(b_end),
        along,
        TextAlign::Centre,
    )?;
    Ok(Dimension {
        extension_lines,
        dimension_line: segment(a_end, b_end),
        arrows,
        text,
        value,
        label,
    })
}

/// A dimension measuring the X distance between two points, with the
/// dimension line running along `y`.
pub fn horizontal_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    a: Point,
    b: Point,
    y: f64,
) -> Result<Dimension, CoreError> {
    style.check()?;
    let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
    let value = (b.x - a.x).abs();
    if value <= Tolerance::EQUALITY.get() {
        return Err(CoreError::geometry(
            "a horizontal dimension needs two points with different X values",
        ));
    }
    let a_end = Point::new(left.x, y);
    let b_end = Point::new(right.x, y);
    let extension_lines = vec![
        extension_to(left, a_end, style),
        extension_to(right, b_end, style),
    ];
    let along = Vec2::new(1.0, 0.0);
    let arrows = vec![
        arrowhead(a_end, along.scaled(-1.0), style)?,
        arrowhead(b_end, along, style)?,
    ];
    let label = style.format_length(value);
    let text = place_text(
        library,
        style,
        &label,
        a_end.midpoint(b_end),
        along,
        TextAlign::Centre,
    )?;
    Ok(Dimension {
        extension_lines,
        dimension_line: segment(a_end, b_end),
        arrows,
        text,
        value,
        label,
    })
}

/// A dimension measuring the Y distance between two points, with the
/// dimension line running along `x`.
pub fn vertical_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    a: Point,
    b: Point,
    x: f64,
) -> Result<Dimension, CoreError> {
    style.check()?;
    let (low, high) = if a.y <= b.y { (a, b) } else { (b, a) };
    let value = (b.y - a.y).abs();
    if value <= Tolerance::EQUALITY.get() {
        return Err(CoreError::geometry(
            "a vertical dimension needs two points with different Y values",
        ));
    }
    let a_end = Point::new(x, low.y);
    let b_end = Point::new(x, high.y);
    let extension_lines = vec![
        extension_to(low, a_end, style),
        extension_to(high, b_end, style),
    ];
    let along = Vec2::new(0.0, 1.0);
    let arrows = vec![
        arrowhead(a_end, along.scaled(-1.0), style)?,
        arrowhead(b_end, along, style)?,
    ];
    let label = style.format_length(value);
    let text = place_text(
        library,
        style,
        &label,
        a_end.midpoint(b_end),
        along,
        TextAlign::Centre,
    )?;
    Ok(Dimension {
        extension_lines,
        dimension_line: segment(a_end, b_end),
        arrows,
        text,
        value,
        label,
    })
}

/// An extension line from a measured point out to the dimension line, with
/// the gap at the start and the overshoot at the end.
fn extension_to(from: Point, to: Point, style: &DimensionStyle) -> Path {
    match from.to(to).normalised() {
        Some(dir) => segment(
            from.offset(dir.scaled(style.extension_gap)),
            to.offset(dir.scaled(style.extension_overshoot)),
        ),
        None => segment(from, to),
    }
}

/// A dimension measuring the angle at `centre` between two points.
///
/// The angle is taken counter clockwise from `from` to `to`, and the
/// dimension line is an arc of the given radius around the centre.
pub fn angular_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    centre: Point,
    from: Point,
    to: Point,
    radius: f64,
) -> Result<Dimension, CoreError> {
    style.check()?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(CoreError::geometry(format!(
            "an angular dimension needs a positive radius, got {radius}"
        )));
    }
    let start_dir = centre
        .to(from)
        .normalised()
        .ok_or_else(|| CoreError::geometry("the first point cannot sit on the center"))?;
    let end_dir = centre
        .to(to)
        .normalised()
        .ok_or_else(|| CoreError::geometry("the second point cannot sit on the center"))?;
    let start_angle = start_dir.angle();
    let mut sweep = end_dir.angle() - start_angle;
    while sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    if sweep >= std::f64::consts::TAU - 1e-12 {
        return Err(CoreError::geometry(
            "an angular dimension needs two different directions",
        ));
    }

    let arc_start = centre.offset(start_dir.scaled(radius));
    let arc_end = centre.offset(end_dir.scaled(radius));
    let mut arc = SubPath::open(arc_start);
    arc.arc_to(arc_end, bulge_from_sweep(sweep));

    let extension_lines = vec![
        extension_to(from, centre.offset(start_dir.scaled(radius)), style),
        extension_to(to, centre.offset(end_dir.scaled(radius)), style),
    ];
    let arrows = vec![
        arrowhead(arc_start, start_dir.perp().scaled(-1.0), style)?,
        arrowhead(arc_end, end_dir.perp(), style)?,
    ];
    let mid_dir = Vec2::from_angle(start_angle + sweep * 0.5);
    let label = style.format_angle(sweep);
    let text = place_text(
        library,
        style,
        &label,
        centre.offset(mid_dir.scaled(radius)),
        mid_dir.perp(),
        TextAlign::Centre,
    )?;
    Ok(Dimension {
        extension_lines,
        dimension_line: arc.into_path(),
        arrows,
        text,
        value: sweep,
        label,
    })
}

/// A dimension measuring the length along a circular arc.
///
/// `offset` moves the dimension arc out from the measured arc by that many
/// millimetres. Angles are radians, `sweep` positive counter clockwise.
pub fn arc_length_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    centre: Point,
    radius: f64,
    start_angle: f64,
    sweep: f64,
    offset: f64,
) -> Result<Dimension, CoreError> {
    style.check()?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(CoreError::geometry(format!(
            "an arc length dimension needs a positive radius, got {radius}"
        )));
    }
    if !sweep.is_finite() || sweep.abs() < 1e-9 {
        return Err(CoreError::geometry(
            "an arc length dimension needs a sweep that is not zero",
        ));
    }
    let dim_radius = radius + offset;
    if dim_radius <= 0.0 {
        return Err(CoreError::geometry(
            "the dimension arc would fall through the center",
        ));
    }
    let value = radius * sweep.abs();
    let start_dir = Vec2::from_angle(start_angle);
    let end_dir = Vec2::from_angle(start_angle + sweep);
    let arc_start = centre.offset(start_dir.scaled(dim_radius));
    let arc_end = centre.offset(end_dir.scaled(dim_radius));
    let mut arc = SubPath::open(arc_start);
    arc.arc_to(arc_end, bulge_from_sweep(sweep));

    let extension_lines = vec![
        extension_to(centre.offset(start_dir.scaled(radius)), arc_start, style),
        extension_to(centre.offset(end_dir.scaled(radius)), arc_end, style),
    ];
    let turn = if sweep >= 0.0 { 1.0 } else { -1.0 };
    let arrows = vec![
        arrowhead(arc_start, start_dir.perp().scaled(-turn), style)?,
        arrowhead(arc_end, end_dir.perp().scaled(turn), style)?,
    ];
    let mid_dir = Vec2::from_angle(start_angle + sweep * 0.5);
    let label = style.format_length(value);
    let text = place_text(
        library,
        style,
        &label,
        centre.offset(mid_dir.scaled(dim_radius)),
        mid_dir.perp().scaled(turn),
        TextAlign::Centre,
    )?;
    Ok(Dimension {
        extension_lines,
        dimension_line: arc.into_path(),
        arrows,
        text,
        value,
        label,
    })
}

/// A radius or diameter dimension of a circle or arc.
///
/// The dimension line runs from the centre (radius) or from the far side
/// (diameter) out through the arc towards `toward`, with an arrow on the
/// arc; when `toward` lies outside the circle the line carries on to it and
/// the value sits there, otherwise the value sits on the arc.
pub fn radius_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    centre: Point,
    radius: f64,
    toward: Point,
    diameter: bool,
) -> Result<Dimension, CoreError> {
    style.check()?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(CoreError::geometry(format!(
            "a radius dimension needs a positive radius, got {radius}"
        )));
    }
    let dir = centre
        .to(toward)
        .normalised()
        .unwrap_or(Vec2::new(1.0, 0.0));
    let on_arc = centre.offset(dir.scaled(radius));
    let outside = centre.distance(toward) > radius;
    let end = if outside { toward } else { on_arc };
    let start = if diameter {
        centre.offset(dir.scaled(-radius))
    } else {
        centre
    };
    let mut arrows = vec![arrowhead(on_arc, dir, style)?];
    if diameter {
        arrows.push(arrowhead(start, dir.scaled(-1.0), style)?);
    }
    let value = if diameter { radius * 2.0 } else { radius };
    let label = match &style.custom_text {
        Some(text) => text.clone(),
        None => format!(
            "{}{}",
            if diameter { "\u{d8}" } else { "R" },
            style.format_length(value)
        ),
    };
    let text = place_text(library, style, &label, end, dir, TextAlign::Centre)?;
    Ok(Dimension {
        extension_lines: Vec::new(),
        dimension_line: segment(start, end),
        arrows,
        text,
        value,
        label,
    })
}

/// A note: an arrow pointing at something, a bend, a horizontal shelf and
/// the text sitting above the shelf.
///
/// `shelf` is the length of the horizontal run in millimetres, going to the
/// right when it is positive and to the left when it is negative.
pub fn note_dimension(
    library: &FontLibrary,
    style: &DimensionStyle,
    at: Point,
    elbow: Point,
    shelf: f64,
    note: &str,
) -> Result<Dimension, CoreError> {
    style.check()?;
    if !shelf.is_finite() || shelf.abs() <= Tolerance::EQUALITY.get() {
        return Err(CoreError::geometry(format!(
            "a note needs a shelf with some length, got {shelf}"
        )));
    }
    let leader_dir = elbow
        .to(at)
        .normalised()
        .ok_or_else(|| CoreError::geometry("a note needs an elbow away from the point"))?;
    let end = Point::new(elbow.x + shelf, elbow.y);
    let mut leader = SubPath::open(at);
    leader.line_to(elbow).line_to(end);

    let arrows = vec![arrowhead(at, leader_dir, style)?];
    let align = if shelf >= 0.0 {
        TextAlign::Left
    } else {
        TextAlign::Right
    };
    let text = place_text(library, style, note, elbow, Vec2::new(1.0, 0.0), align)?;
    Ok(Dimension {
        extension_lines: Vec::new(),
        dimension_line: leader.into_path(),
        arrows,
        text,
        value: at.distance(elbow),
        label: note.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::{path_area, path_bbox, path_length};
    use crate::geom::text::outlines_bbox;

    const FLAT: Tolerance = Tolerance::FLATTEN;

    fn library() -> FontLibrary {
        FontLibrary::bundled()
    }

    #[test]
    fn a_linear_dimension_has_all_its_pieces() {
        let dim = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(100.0, 0.0),
            20.0,
        )
        .expect("dimension");
        assert_eq!(dim.extension_lines.len(), 2);
        assert_eq!(dim.arrows.len(), 2);
        assert!(!dim.text.is_empty());
        assert!((dim.value - 100.0).abs() < 1e-9);
        assert_eq!(dim.label, "100.00 mm");
        assert!((path_length(&dim.dimension_line, FLAT) - 100.0).abs() < 1e-9);
        assert_eq!(dim.all_paths().len(), 2 + 1 + 2 + dim.text.len());
    }

    #[test]
    fn a_linear_dimension_line_sits_at_the_offset() {
        let dim = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(100.0, 0.0),
            20.0,
        )
        .expect("dimension");
        let bbox = path_bbox(&dim.dimension_line).expect("bbox");
        assert!((bbox.min.y - 20.0).abs() < 1e-9);
        let below = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(100.0, 0.0),
            -20.0,
        )
        .expect("dimension");
        let bb = path_bbox(&below.dimension_line).expect("bbox");
        assert!((bb.min.y + 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_diagonal_dimension_measures_the_true_distance() {
        let dim = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(30.0, 40.0),
            10.0,
        )
        .expect("dimension");
        assert!((dim.value - 50.0).abs() < 1e-9);
        assert_eq!(dim.label, "50.00 mm");
    }

    #[test]
    fn arrowheads_are_closed_triangles() {
        let dim = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(50.0, 0.0),
            10.0,
        )
        .expect("dimension");
        for arrow in &dim.arrows {
            assert!(arrow.subpaths[0].closed);
            assert!(path_area(arrow, FLAT).abs() > 0.0);
        }
    }

    #[test]
    fn a_linear_dimension_needs_two_points() {
        assert!(linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::ORIGIN,
            10.0
        )
        .is_err());
    }

    #[test]
    fn a_horizontal_dimension_ignores_the_height_difference() {
        let dim = horizontal_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::new(10.0, 0.0),
            Point::new(70.0, 40.0),
            -10.0,
        )
        .expect("dimension");
        assert!((dim.value - 60.0).abs() < 1e-9);
        let bbox = path_bbox(&dim.dimension_line).expect("bbox");
        assert!((bbox.min.y + 10.0).abs() < 1e-9);
        assert!((bbox.width() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_vertical_dimension_ignores_the_width_difference() {
        let dim = vertical_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::new(0.0, 5.0),
            Point::new(40.0, 45.0),
            -12.0,
        )
        .expect("dimension");
        assert!((dim.value - 40.0).abs() < 1e-9);
        let bbox = path_bbox(&dim.dimension_line).expect("bbox");
        assert!((bbox.min.x + 12.0).abs() < 1e-9);
        assert!((bbox.height() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn flat_and_upright_dimensions_are_rejected() {
        assert!(horizontal_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(0.0, 10.0),
            5.0
        )
        .is_err());
        assert!(vertical_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(10.0, 0.0),
            5.0
        )
        .is_err());
    }

    #[test]
    fn an_angular_dimension_measures_a_right_angle() {
        let dim = angular_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
            20.0,
        )
        .expect("dimension");
        assert!((dim.value - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(dim.label, "90.00\u{b0}");
        let arc = path_length(&dim.dimension_line, FLAT);
        assert!((arc - 20.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn an_angular_dimension_goes_counter_clockwise() {
        let dim = angular_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(0.0, 10.0),
            Point::new(10.0, 0.0),
            20.0,
        )
        .expect("dimension");
        assert!((dim.value - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn an_angular_dimension_needs_real_directions() {
        assert!(angular_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::ORIGIN,
            Point::new(1.0, 0.0),
            10.0
        )
        .is_err());
        assert!(angular_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            10.0
        )
        .is_err());
        assert!(angular_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            0.0
        )
        .is_err());
    }

    #[test]
    fn an_arc_length_dimension_measures_the_arc() {
        let dim = arc_length_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            50.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
            10.0,
        )
        .expect("dimension");
        let expected = 50.0 * std::f64::consts::FRAC_PI_2;
        assert!((dim.value - expected).abs() < 1e-9);
        assert_eq!(dim.label, format!("{expected:.2} mm"));
        let drawn = path_length(&dim.dimension_line, FLAT);
        assert!((drawn - 60.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(dim.extension_lines.len(), 2);
    }

    #[test]
    fn an_arc_length_dimension_handles_a_clockwise_sweep() {
        let dim = arc_length_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            50.0,
            0.0,
            -std::f64::consts::FRAC_PI_2,
            10.0,
        )
        .expect("dimension");
        assert!(dim.value > 0.0);
        let bbox = path_bbox(&dim.dimension_line).expect("bbox");
        assert!(bbox.min.y < 0.0);
    }

    #[test]
    fn an_arc_length_dimension_rejects_bad_geometry() {
        let style = DimensionStyle::default();
        assert!(
            arc_length_dimension(&library(), &style, Point::ORIGIN, 0.0, 0.0, 1.0, 5.0).is_err()
        );
        assert!(
            arc_length_dimension(&library(), &style, Point::ORIGIN, 10.0, 0.0, 0.0, 5.0).is_err()
        );
        assert!(
            arc_length_dimension(&library(), &style, Point::ORIGIN, 10.0, 0.0, 1.0, -20.0).is_err()
        );
    }

    #[test]
    fn a_note_draws_a_leader_and_its_text() {
        let dim = note_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::new(10.0, 10.0),
            Point::new(30.0, 30.0),
            25.0,
            "drill through",
        )
        .expect("note");
        assert!(dim.extension_lines.is_empty());
        assert_eq!(dim.arrows.len(), 1);
        assert_eq!(dim.label, "drill through");
        assert_eq!(dim.dimension_line.subpaths[0].segments.len(), 2);
        let bbox = outlines_bbox(&dim.text).expect("bbox");
        assert!(bbox.min.x >= 30.0 - 1e-6, "text starts at {}", bbox.min.x);
        // The baseline sits a text gap above the shelf. Descenders drop below
        // it, so check the body of the text rather than the very bottom.
        assert!(bbox.max.y > 31.0, "text top at {}", bbox.max.y);
    }

    #[test]
    fn a_note_can_run_to_the_left() {
        let dim = note_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::new(50.0, 10.0),
            Point::new(30.0, 30.0),
            -25.0,
            "pocket",
        )
        .expect("note");
        let bbox = outlines_bbox(&dim.text).expect("bbox");
        assert!(bbox.max.x <= 30.0 + 1e-6, "text ends at {}", bbox.max.x);
    }

    #[test]
    fn a_note_needs_a_shelf_and_an_elbow() {
        let style = DimensionStyle::default();
        assert!(note_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            Point::new(10.0, 10.0),
            0.0,
            "x"
        )
        .is_err());
        assert!(
            note_dimension(&library(), &style, Point::ORIGIN, Point::ORIGIN, 10.0, "x").is_err()
        );
    }

    #[test]
    fn inches_convert_and_carry_their_suffix() {
        let style = DimensionStyle {
            units: DimensionUnits::Inches,
            precision: 3,
            ..DimensionStyle::default()
        };
        assert_eq!(style.format_length(25.4), "1.000 in");
        assert_eq!(DimensionUnits::Inches.suffix(), "in");
        assert!((DimensionUnits::Millimetres.from_mm(5.0) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn units_can_be_left_off_and_precision_changed() {
        let style = DimensionStyle {
            show_units: false,
            precision: 0,
            angle_precision: 0,
            ..DimensionStyle::default()
        };
        assert_eq!(style.format_length(12.4), "12");
        assert_eq!(style.format_angle(std::f64::consts::PI), "180\u{b0}");
    }

    #[test]
    fn a_bad_style_is_rejected() {
        let mut style = DimensionStyle {
            arrow_length: 0.0,
            ..DimensionStyle::default()
        };
        assert!(linear_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            Point::new(10.0, 0.0),
            5.0
        )
        .is_err());
        style.arrow_length = 3.0;
        style.extension_gap = -1.0;
        assert!(linear_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            Point::new(10.0, 0.0),
            5.0
        )
        .is_err());
        style.extension_gap = 1.0;
        style.precision = 9;
        assert!(linear_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            Point::new(10.0, 0.0),
            5.0
        )
        .is_err());
    }

    #[test]
    fn text_stays_the_right_way_up_on_a_backwards_dimension() {
        let forward = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(100.0, 0.0),
            10.0,
        )
        .expect("dimension");
        let backward = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::new(100.0, 0.0),
            Point::ORIGIN,
            -10.0,
        )
        .expect("dimension");
        let a = outlines_bbox(&forward.text).expect("bbox");
        let b = outlines_bbox(&backward.text).expect("bbox");
        assert!((a.centre().x - b.centre().x).abs() < 1e-6);
        assert!((a.height() - b.height()).abs() < 1e-6);
        assert!(a.min.y > 10.0 && b.min.y > 10.0);
    }

    #[test]
    fn dimensions_are_deterministic() {
        let a = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(42.0, 17.0),
            8.0,
        )
        .expect("dimension");
        let b = linear_dimension(
            &library(),
            &DimensionStyle::default(),
            Point::ORIGIN,
            Point::new(42.0, 17.0),
            8.0,
        )
        .expect("dimension");
        assert_eq!(a, b);
    }

    #[test]
    fn radius_and_diameter_dimensions_read_their_value() {
        let style = DimensionStyle::default();
        let radius = radius_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            10.0,
            Point::new(20.0, 0.0),
            false,
        )
        .expect("radius");
        assert_eq!(radius.label, "R10.00 mm");
        assert!((radius.value - 10.0).abs() < 1e-12);
        let diameter = radius_dimension(
            &library(),
            &style,
            Point::ORIGIN,
            10.0,
            Point::new(0.0, 5.0),
            true,
        )
        .expect("diameter");
        assert_eq!(diameter.arrows.len(), 2);
        assert!((diameter.value - 20.0).abs() < 1e-12);
        let custom = DimensionStyle {
            custom_text: Some("HOLE".into()),
            ..DimensionStyle::default()
        };
        let named = radius_dimension(
            &library(),
            &custom,
            Point::ORIGIN,
            3.0,
            Point::new(9.0, 0.0),
            false,
        )
        .expect("custom");
        assert_eq!(named.label, "HOLE");
        assert_eq!(custom.format_angle(1.0), "HOLE");
    }
}
