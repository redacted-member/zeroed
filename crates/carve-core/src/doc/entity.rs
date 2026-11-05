//! Entities: the drawable objects a document holds.
//!
//! An entity is a path, a piece of text, a placed bitmap, a dimension or a
//! group of other entities. Every entity carries the layer and sheet it
//! belongs to, its own revision (bumped whenever its geometry changes) and an
//! optional name shown in the object list.
//!
//! The JSON shape on the wire flattens the kind into the object, so a path
//! entity is `{"id": 1, "kind": "path", "layer": 1, "sheet": 1, "revision": 3,
//! "path": {...}}`. Lengths are millimetres.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{
    self, path_bbox, transform_path, BBox, Mat3, Path, Point, TextAlign, TextSpec, Tolerance,
};

/// Default sheet every entity lands on when the client does not say.
pub const DEFAULT_SHEET: u64 = 1;

/// A text entity: the spec it was typed with, where it sits and the glyph
/// outlines that were laid out from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEntity {
    /// Font, size and spacing the text was laid out with.
    pub spec: TextSpecJson,
    /// Lower left of the first line, in millimetres.
    pub origin: Point,
    /// Glyph outlines, one path per glyph, already placed in job space.
    #[serde(default)]
    pub paths: Vec<Path>,
}

/// A bitmap placed on the sheet by its four corners.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitmapRef {
    /// Id of the image in the document's image store.
    pub image_id: u64,
    /// Corners in order lower left, lower right, upper right, upper left.
    pub quad: Vec<Point>,
    /// Opacity from 0 (invisible) to 1 (opaque).
    pub opacity: f64,
}

/// A dimension entity: the numbers it was built from plus the parts that draw
/// it. The parts are produced by `geom::dimension` and cached here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionEntity {
    /// What the dimension measures and how it is styled.
    pub spec: DimensionSpec,
    /// Lines, arrows and the text outline, in job space.
    #[serde(default)]
    pub paths: Vec<Path>,
    /// Text shown in place of the measured value, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_text: Option<String>,
    /// Where the text was dragged to along the dimension line (the value
    /// sits in the middle when absent). Linear, horizontal, vertical and
    /// angle dimensions; radius dimensions and notes move their own points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_at: Option<Point>,
    /// `[minX, minY, maxX, maxY]` round the value text, for picking it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_box: Option<[f64; 4]>,
}

/// What a dimension measures. Mirrors the forms of `geom::dimension`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DimensionSpec {
    /// Distance between two points, measured along the line joining them.
    Linear {
        /// First measured point.
        from: Point,
        /// Second measured point.
        to: Point,
        /// How far the dimension line sits from the measured line, in mm.
        offset: f64,
    },
    /// Distance between two points, measured along X.
    Horizontal {
        /// First measured point.
        from: Point,
        /// Second measured point.
        to: Point,
        /// Job Y of the dimension line, in mm.
        offset: f64,
    },
    /// Distance between two points, measured along Y.
    Vertical {
        /// First measured point.
        from: Point,
        /// Second measured point.
        to: Point,
        /// Job X of the dimension line, in mm.
        offset: f64,
    },
    /// Angle at `centre` between the two arms, in degrees.
    Angular {
        /// Corner the angle is measured at.
        centre: Point,
        /// A point on the first arm.
        from: Point,
        /// A point on the second arm.
        to: Point,
        /// Radius of the arc that carries the text, in mm.
        radius: f64,
    },
    /// Radius or diameter of a circle or arc.
    Radius {
        /// Centre of the circle.
        centre: Point,
        /// Radius of the circle, in mm.
        radius: f64,
        /// Where the dimension line points and the value sits.
        at: Point,
        /// Show the diameter instead of the radius.
        #[serde(default)]
        diameter: bool,
    },
    /// A leader line with a note at its end.
    Note {
        /// Where the leader points.
        at: Point,
        /// Where the text sits.
        text_at: Point,
        /// The note itself.
        text: String,
    },
}

/// Text settings on the wire. Mirrors [`TextSpec`] in camelCase, which is what
/// the kernel API contract asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpecJson {
    /// The text itself. A newline starts a new line.
    pub text: String,
    /// Family name, matched without regard to case.
    pub font_family: String,
    /// Ask for a bold face.
    #[serde(default)]
    pub bold: bool,
    /// Ask for an italic face.
    #[serde(default)]
    pub italic: bool,
    /// Height of the capital letters in millimetres (the Text Height
    /// field). The kernel works out the em size from the font.
    pub size_mm: f64,
    /// How lines line up with each other.
    #[serde(default = "default_alignment")]
    pub alignment: TextAlign,
    /// Line pitch as a multiple of the font default.
    #[serde(default = "one")]
    pub line_spacing: f64,
    /// Extra space between characters as a fraction of the size.
    #[serde(default)]
    pub char_spacing: f64,
    /// Let the font's kerning pairs move letters closer together.
    #[serde(default = "yes")]
    pub kerning: bool,
    /// Column width in millimetres for a text block: words that do not fit
    /// start a new line. Plain text when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_width: Option<f64>,
    /// Auto Layout Text: the box the text is sized to fill and how, in job
    /// millimetres. The origin plays no part while it is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_layout: Option<geom::text::auto::AutoLayout>,
    /// Text Selection kerning: per line, extra space after each drawn
    /// letter as a fraction of the text height.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub letter_gaps: Vec<Vec<f64>>,
    /// Text Selection line spacing: extra space under each line as a
    /// fraction of the text height.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_gaps: Vec<f64>,
    /// Text Selection arching: the bend height in millimetres, positive to
    /// lift the middle of the line, negative to drop it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<f64>,
    /// The point of the text's box that sits at its origin. Absent, the
    /// origin is the start of the first line's baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<TextAnchor>,
    /// Text On A Curve: the vector the text is set along and how. The text
    /// is laid out on that vector again whenever it is edited, and goes
    /// back to its origin when it is taken off the curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_curve: Option<OnCurve>,
}

/// Where text sits against the curve it is set along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CurvePosition {
    /// The baseline on the curve (lifted by the offset).
    #[default]
    Above,
    /// The middle of the letters on the curve.
    On,
    /// The tops of the letters on the curve.
    Below,
}

/// Text On A Curve settings kept on the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnCurve {
    /// The vector the text follows.
    pub path_id: u64,
    /// Above, on or below the curve.
    #[serde(default)]
    pub position: CurvePosition,
    /// Text on other side: reads the other way along the curve.
    #[serde(default)]
    pub other_side: bool,
    /// Offset Distance, millimetres away from the curve.
    #[serde(default)]
    pub offset: f64,
    /// Left, middle or right of the curve (of the start node on a closed
    /// one).
    #[serde(default = "centred")]
    pub align: TextAlign,
    /// Keep Vertical instead of Align to Curve.
    #[serde(default)]
    pub keep_vertical: bool,
    /// Scale text to fill curve instead of Maintain text Size.
    #[serde(default)]
    pub fill: bool,
    /// Text Spacing, percent of the laid out spacing.
    #[serde(default = "hundred")]
    pub spacing: f64,
    /// Millimetres the text was slid along the curve by its anchor handle.
    #[serde(default)]
    pub start_at: f64,
}

fn centred() -> TextAlign {
    TextAlign::Centre
}

fn hundred() -> f64 {
    100.0
}

/// Where a text's origin sits on the box round its letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAnchor {
    /// Top left corner.
    TopLeft,
    /// Middle of the top edge.
    TopCentre,
    /// Top right corner.
    TopRight,
    /// Middle of the left edge.
    MiddleLeft,
    /// Centre of the box.
    Centre,
    /// Middle of the right edge.
    MiddleRight,
    /// Bottom left corner.
    BottomLeft,
    /// Middle of the bottom edge.
    BottomCentre,
    /// Bottom right corner.
    BottomRight,
}

impl TextAnchor {
    /// The anchor's fractions across and up the box, 0 to 1.
    pub fn fractions(self) -> (f64, f64) {
        match self {
            TextAnchor::TopLeft => (0.0, 1.0),
            TextAnchor::TopCentre => (0.5, 1.0),
            TextAnchor::TopRight => (1.0, 1.0),
            TextAnchor::MiddleLeft => (0.0, 0.5),
            TextAnchor::Centre => (0.5, 0.5),
            TextAnchor::MiddleRight => (1.0, 0.5),
            TextAnchor::BottomLeft => (0.0, 0.0),
            TextAnchor::BottomCentre => (0.5, 0.0),
            TextAnchor::BottomRight => (1.0, 0.0),
        }
    }
}

fn default_alignment() -> TextAlign {
    TextAlign::Left
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

impl From<&TextSpecJson> for TextSpec {
    fn from(spec: &TextSpecJson) -> TextSpec {
        TextSpec {
            text: spec.text.clone(),
            font_family: spec.font_family.clone(),
            bold: spec.bold,
            italic: spec.italic,
            size_mm: spec.size_mm,
            alignment: spec.alignment,
            line_spacing: spec.line_spacing,
            char_spacing: spec.char_spacing,
            kerning: spec.kerning,
        }
    }
}

impl From<&TextSpec> for TextSpecJson {
    fn from(spec: &TextSpec) -> TextSpecJson {
        TextSpecJson {
            text: spec.text.clone(),
            font_family: spec.font_family.clone(),
            bold: spec.bold,
            italic: spec.italic,
            size_mm: spec.size_mm,
            alignment: spec.alignment,
            line_spacing: spec.line_spacing,
            char_spacing: spec.char_spacing,
            kerning: spec.kerning,
            wrap_width: None,
            auto_layout: None,
            letter_gaps: Vec::new(),
            line_gaps: Vec::new(),
            arch: None,
            anchor: None,
            on_curve: None,
        }
    }
}

/// The shape specific part of an entity, tagged with `kind` on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EntityKind {
    /// Plain vector geometry.
    Path {
        /// The geometry, in job millimetres.
        path: Path,
    },
    /// Text with its cached glyph outlines.
    Text(TextEntity),
    /// A bitmap placed by its corner quad.
    Bitmap(BitmapRef),
    /// A dimension with its cached parts.
    Dimension(DimensionEntity),
    /// A group. The children are entities of their own, kept out of the
    /// top level z-order while the group holds them.
    Group {
        /// Ids of the members, in z-order.
        children: Vec<u64>,
    },
}

impl EntityKind {
    /// The `kind` string used on the wire.
    pub fn tag(&self) -> &'static str {
        match self {
            EntityKind::Path { .. } => "path",
            EntityKind::Text(_) => "text",
            EntityKind::Bitmap(_) => "bitmap",
            EntityKind::Dimension(_) => "dimension",
            EntityKind::Group { .. } => "group",
        }
    }
}

/// One object in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    /// Document wide id, never reused. Absent (0) on `entities.create`.
    #[serde(default)]
    pub id: u64,
    /// Layer the entity draws on. 0 means the active layer on create.
    #[serde(default)]
    pub layer: u64,
    /// Sheet the entity belongs to. 0 means the active sheet on create.
    #[serde(default)]
    pub sheet: u64,
    /// Bumped whenever the entity's geometry or placement changes. Absent
    /// on `entities.create`.
    #[serde(default)]
    pub revision: u64,
    /// Optional name shown in the object list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Group this entity is a member of, if any. Not sent to the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    /// The object's own rotation in degrees, counter clockwise. Its bounds,
    /// Set Size and the scale handles work along this frame, so a rotated
    /// object keeps its rotated box through moves, rotations and scales
    /// along its own axes. A transform that skews the frame resets it to 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// The shape specific part.
    #[serde(flatten)]
    pub kind: EntityKind,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// The rotation frame, in degrees, of an object with frame `rotation` after
/// the affine transform `m`.
///
/// Rotations add their angle, translations and uniform scales keep it, and
/// a scale along the frame's own axes keeps it too. A mirror keeps the frame
/// lines, folded into -90 to 90 degrees. A transform that no longer maps
/// the frame axes to perpendicular directions resets the frame to 0.
pub fn rotated_frame(rotation: f64, m: &Mat3) -> f64 {
    let (s, c) = rotation.to_radians().sin_cos();
    let u = (m.a * c + m.c * s, m.b * c + m.d * s);
    let v = (-m.a * s + m.c * c, -m.b * s + m.d * c);
    let lu = u.0.hypot(u.1);
    let lv = v.0.hypot(v.1);
    if lu < 1e-12 || lv < 1e-12 || (u.0 * v.0 + u.1 * v.1).abs() > 1e-6 * lu * lv {
        return 0.0;
    }
    let mut angle = u.1.atan2(u.0).to_degrees();
    if m.determinant() < 0.0 {
        while angle > 90.0 + 1e-9 {
            angle -= 180.0;
        }
        while angle <= -90.0 + 1e-9 {
            angle += 180.0;
        }
    } else if angle <= -180.0 + 1e-9 {
        angle += 360.0;
    }
    if angle.abs() < 1e-7 {
        0.0
    } else {
        angle
    }
}

impl Entity {
    /// The geometry of the entity, if it has any of its own. Groups have
    /// none: their members carry it.
    pub fn path(&self) -> Option<Path> {
        match &self.kind {
            EntityKind::Path { path } => Some(path.clone()),
            EntityKind::Text(text) => Some(merge(&text.paths)),
            EntityKind::Dimension(dim) => Some(merge(&dim.paths)),
            EntityKind::Bitmap(bitmap) => quad_path(&bitmap.quad),
            EntityKind::Group { .. } => None,
        }
    }

    /// Replaces the entity's own geometry. Only path entities accept one.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for any other kind.
    pub fn set_path(&mut self, path: Path) -> Result<(), CoreError> {
        match &mut self.kind {
            EntityKind::Path { path: existing } => {
                *existing = path;
                Ok(())
            }
            other => Err(CoreError::validation(format!(
                "a {} entity has no editable path",
                other.tag()
            ))),
        }
    }

    /// Applies an affine transform to the entity's own geometry.
    pub fn transform(&mut self, m: &Mat3) {
        match &mut self.kind {
            EntityKind::Path { path } => *path = transform_path(path, m),
            EntityKind::Text(text) => {
                text.paths = text.paths.iter().map(|p| transform_path(p, m)).collect();
                text.origin = m.apply(text.origin);
                if let Some(auto) = &mut text.spec.auto_layout {
                    // The box travels with the text, so editing it later
                    // lays it out where it now is.
                    let [x0, y0, x1, y1] = auto.bounds;
                    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                        .map(|(x, y)| m.apply(Point::new(x, y)));
                    let xs = corners.map(|p| p.x);
                    let ys = corners.map(|p| p.y);
                    auto.bounds = [
                        xs.iter().copied().fold(f64::INFINITY, f64::min),
                        ys.iter().copied().fold(f64::INFINITY, f64::min),
                        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    ];
                }
            }
            EntityKind::Dimension(dim) => {
                dim.paths = dim.paths.iter().map(|p| transform_path(p, m)).collect();
                dim.spec.transform(m);
                dim.text_at = dim.text_at.map(|p| m.apply(p));
                dim.text_box = dim.text_box.map(|[x0, y0, x1, y1]| {
                    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                        .map(|(x, y)| m.apply(Point::new(x, y)));
                    let bbox = corners[1..]
                        .iter()
                        .fold(BBox::from_point(corners[0]), |b, p| {
                            b.union(BBox::from_point(*p))
                        });
                    [bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y]
                });
            }
            EntityKind::Bitmap(bitmap) => {
                for corner in &mut bitmap.quad {
                    *corner = m.apply(*corner);
                }
            }
            EntityKind::Group { .. } => {}
        }
    }

    /// Applies an affine transform to the geometry and carries the
    /// rotation frame along with it.
    pub fn transform_with_frame(&mut self, m: &Mat3) {
        self.transform(m);
        self.rotation = rotated_frame(self.rotation, m);
    }

    /// Bounding box of the entity's own geometry, in millimetres.
    pub fn bbox(&self) -> Option<BBox> {
        self.path().as_ref().and_then(path_bbox)
    }

    /// True when every subpath of the entity's geometry is closed.
    pub fn is_closed(&self) -> bool {
        match self.path() {
            Some(path) if !path.is_empty() => path.is_closed(),
            _ => false,
        }
    }
}

impl DimensionSpec {
    /// Moves the measured points with an affine transform.
    pub fn transform(&mut self, m: &Mat3) {
        match self {
            DimensionSpec::Linear { from, to, .. } => {
                *from = m.apply(*from);
                *to = m.apply(*to);
            }
            // The line position is a job coordinate, so it moves with the
            // points rather than staying where it was.
            DimensionSpec::Horizontal { from, to, offset } => {
                *offset = m.apply(Point::new(from.x, *offset)).y;
                *from = m.apply(*from);
                *to = m.apply(*to);
            }
            DimensionSpec::Vertical { from, to, offset } => {
                *offset = m.apply(Point::new(*offset, from.y)).x;
                *from = m.apply(*from);
                *to = m.apply(*to);
            }
            DimensionSpec::Angular {
                centre, from, to, ..
            } => {
                *centre = m.apply(*centre);
                *from = m.apply(*from);
                *to = m.apply(*to);
            }
            DimensionSpec::Note { at, text_at, .. } => {
                *at = m.apply(*at);
                *text_at = m.apply(*text_at);
            }
            DimensionSpec::Radius {
                centre, radius, at, ..
            } => {
                *centre = m.apply(*centre);
                *at = m.apply(*at);
                *radius *= m.determinant().abs().sqrt();
            }
        }
    }

    /// Builds the drawable parts of the dimension with the given style.
    ///
    /// # Errors
    /// Returns [`CoreError::Geometry`] when the measured points do not define
    /// the dimension, for instance two coincident points, and
    /// [`CoreError::Font`] when the label cannot be laid out.
    pub fn build(
        &self,
        library: &geom::FontLibrary,
        style: &geom::DimensionStyle,
    ) -> Result<Vec<Path>, CoreError> {
        Ok(self.build_parts(library, style, None)?.0)
    }

    /// Builds the drawable parts with the text moved along the dimension
    /// line to where `text_at` projects onto it (the middle when `None`),
    /// and answers the box round the text as well.
    ///
    /// # Errors
    /// As [`DimensionSpec::build`].
    pub fn build_parts(
        &self,
        library: &geom::FontLibrary,
        style: &geom::DimensionStyle,
        text_at: Option<Point>,
    ) -> Result<(Vec<Path>, Option<[f64; 4]>), CoreError> {
        let mut dimension = self.dimension(library, style)?;
        if let Some(at) = text_at {
            if let Some(m) = self.text_move(at) {
                dimension.text = dimension
                    .text
                    .iter()
                    .map(|p| transform_path(p, &m))
                    .collect();
            }
        }
        let text_box = dimension
            .text
            .iter()
            .filter_map(path_bbox)
            .reduce(BBox::union)
            .map(|b| [b.min.x, b.min.y, b.max.x, b.max.y]);
        Ok((dimension.all_paths(), text_box))
    }

    /// The move that takes the text from the middle of the dimension line to
    /// where `at` projects onto it: a slide along a straight line, a turn
    /// round the centre of an angle's arc.
    fn text_move(&self, at: Point) -> Option<Mat3> {
        let slide = |from: Point, to: Point| {
            let along = from.to(to).normalised()?;
            let mid = from.midpoint(to);
            let by = along.scaled(mid.to(at).dot(along));
            Some(Mat3::translate(by))
        };
        match self {
            DimensionSpec::Linear { from, to, .. } => slide(*from, *to),
            DimensionSpec::Horizontal { from, to, .. } => Some(Mat3::translate(geom::Vec2::new(
                at.x - (from.x + to.x) * 0.5,
                0.0,
            ))),
            DimensionSpec::Vertical { from, to, .. } => Some(Mat3::translate(geom::Vec2::new(
                0.0,
                at.y - (from.y + to.y) * 0.5,
            ))),
            DimensionSpec::Angular {
                centre, from, to, ..
            } => {
                let start = centre.to(*from).normalised()?.angle();
                let mut sweep = centre.to(*to).normalised()?.angle() - start;
                while sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                let target = centre.to(at).normalised()?.angle();
                Some(Mat3::rotate_about(target - (start + sweep * 0.5), *centre))
            }
            DimensionSpec::Radius { .. } | DimensionSpec::Note { .. } => None,
        }
    }

    /// The dimension's parts, text in the middle.
    fn dimension(
        &self,
        library: &geom::FontLibrary,
        style: &geom::DimensionStyle,
    ) -> Result<geom::Dimension, CoreError> {
        let dimension = match self {
            DimensionSpec::Linear { from, to, offset } => {
                geom::linear_dimension(library, style, *from, *to, *offset)
            }
            DimensionSpec::Horizontal { from, to, offset } => {
                geom::horizontal_dimension(library, style, *from, *to, *offset)
            }
            DimensionSpec::Vertical { from, to, offset } => {
                geom::vertical_dimension(library, style, *from, *to, *offset)
            }
            DimensionSpec::Angular {
                centre,
                from,
                to,
                radius,
            } => geom::angular_dimension(library, style, *centre, *from, *to, *radius),
            DimensionSpec::Note { at, text_at, text } => {
                let shelf = if text_at.x >= at.x { 10.0 } else { -10.0 };
                geom::note_dimension(library, style, *at, *text_at, shelf, text)
            }
            DimensionSpec::Radius {
                centre,
                radius,
                at,
                diameter,
            } => geom::radius_dimension(library, style, *centre, *radius, *at, *diameter),
        }?;
        Ok(dimension)
    }
}

/// Folds several paths into one.
pub fn merge(paths: &[Path]) -> Path {
    let mut out = Path::new();
    for path in paths {
        for sub in &path.subpaths {
            out.push(sub.clone());
        }
    }
    out
}

/// The outline of a bitmap placement quad, as a closed path.
fn quad_path(quad: &[Point]) -> Option<Path> {
    if quad.len() < 3 {
        return None;
    }
    geom::polyline_path(quad, true).ok()
}

/// Tolerance used whenever a command does not carry one.
pub const DISPLAY_TOLERANCE: f64 = 0.05;

/// The display tolerance as a [`Tolerance`], falling back to the geometry
/// default when the caller asks for something unusable.
pub fn tolerance_or_default(value: Option<f64>) -> Tolerance {
    value
        .and_then(|v| Tolerance::new(v).ok())
        .unwrap_or_else(|| Tolerance::new(DISPLAY_TOLERANCE).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, Point};

    fn path_entity() -> Entity {
        Entity {
            rotation: 0.0,
            id: 7,
            layer: 1,
            sheet: 1,
            revision: 2,
            name: Some("Circle".to_string()),
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(10.0, 10.0), 5.0).expect("circle"),
            },
        }
    }

    #[test]
    fn a_path_entity_round_trips_through_json_in_camel_case() {
        let entity = path_entity();
        let text = serde_json::to_string(&entity).expect("entity serialises");
        assert!(text.contains("\"kind\":\"path\""), "{text}");
        assert!(text.contains("\"path\":"), "{text}");
        let back: Entity = serde_json::from_str(&text).expect("entity parses");
        assert_eq!(back, entity);
    }

    #[test]
    fn a_group_entity_carries_its_children() {
        let entity = Entity {
            rotation: 0.0,
            id: 3,
            layer: 1,
            sheet: 1,
            revision: 1,
            name: None,
            parent: None,
            kind: EntityKind::Group {
                children: vec![1, 2],
            },
        };
        let text = serde_json::to_string(&entity).expect("serialises");
        assert!(text.contains("\"kind\":\"group\""), "{text}");
        assert!(text.contains("\"children\":[1,2]"), "{text}");
    }

    #[test]
    fn a_text_spec_uses_camel_case_field_names() {
        let spec = TextSpecJson {
            text: "Hi".to_string(),
            font_family: "Fira Sans".to_string(),
            bold: false,
            italic: false,
            size_mm: 12.0,
            alignment: TextAlign::Centre,
            line_spacing: 1.0,
            char_spacing: 0.0,
            kerning: true,
            wrap_width: None,
            auto_layout: None,
            letter_gaps: Vec::new(),
            line_gaps: Vec::new(),
            arch: None,
            anchor: None,
            on_curve: None,
        };
        let text = serde_json::to_string(&spec).expect("serialises");
        assert!(!text.contains("wrapWidth"), "plain text keeps its old form");
        assert!(text.contains("\"fontFamily\""), "{text}");
        assert!(text.contains("\"sizeMm\""), "{text}");
        let geom_spec: TextSpec = (&spec).into();
        assert_eq!(geom_spec.font_family, "Fira Sans");
        assert_eq!(TextSpecJson::from(&geom_spec), spec);
    }

    #[test]
    fn transforming_a_path_entity_moves_its_bbox() {
        let mut entity = path_entity();
        entity.transform(&Mat3::translate(geom::Vec2::new(5.0, 0.0)));
        let bbox = entity.bbox().expect("a circle has a bbox");
        assert!((bbox.centre().x - 15.0).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn a_group_has_no_geometry_of_its_own() {
        let entity = Entity {
            rotation: 0.0,
            id: 1,
            layer: 1,
            sheet: 1,
            revision: 1,
            name: None,
            parent: None,
            kind: EntityKind::Group { children: vec![] },
        };
        assert!(entity.path().is_none());
        assert!(!entity.is_closed());
    }

    #[test]
    fn setting_a_path_on_a_group_is_refused() {
        let mut entity = Entity {
            rotation: 0.0,
            id: 1,
            layer: 1,
            sheet: 1,
            revision: 1,
            name: None,
            parent: None,
            kind: EntityKind::Group { children: vec![] },
        };
        let error = entity
            .set_path(Path::new())
            .expect_err("a group has no path");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn a_dimension_spec_builds_its_parts() {
        let mut moved = DimensionSpec::Horizontal {
            from: Point::new(0.0, 0.0),
            to: Point::new(50.0, 0.0),
            offset: 10.0,
        };
        moved.transform(&Mat3::translate(crate::geom::Vec2::new(5.0, 20.0)));
        assert_eq!(
            moved,
            DimensionSpec::Horizontal {
                from: Point::new(5.0, 20.0),
                to: Point::new(55.0, 20.0),
                offset: 30.0,
            },
            "the dimension line moves with its points"
        );
        let spec = DimensionSpec::Horizontal {
            from: Point::new(0.0, 0.0),
            to: Point::new(50.0, 0.0),
            offset: 10.0,
        };
        let parts = spec
            .build(
                &geom::FontLibrary::bundled(),
                &geom::DimensionStyle::default(),
            )
            .expect("a horizontal dimension builds");
        assert!(!parts.is_empty());
    }

    #[test]
    fn frames_follow_rotations_mirrors_and_frame_scales() {
        let turn = |deg: f64| Mat3::rotate(deg.to_radians());
        assert!((rotated_frame(0.0, &turn(30.0)) - 30.0).abs() < 1e-9);
        assert!((rotated_frame(170.0, &turn(20.0)) + 170.0).abs() < 1e-9);
        assert_eq!(rotated_frame(0.0, &turn(360.0)), 0.0);
        assert!(
            (rotated_frame(30.0, &Mat3::translate(geom::Vec2::new(4.0, 1.0))) - 30.0).abs() < 1e-9
        );
        assert!((rotated_frame(30.0, &Mat3::scale(3.0, 3.0)) - 30.0).abs() < 1e-9);
        // Mirrors keep the frame lines: -30 after either flip of a 30 frame.
        assert!((rotated_frame(30.0, &Mat3::scale(-1.0, 1.0)) + 30.0).abs() < 1e-9);
        assert!((rotated_frame(30.0, &Mat3::scale(1.0, -1.0)) + 30.0).abs() < 1e-9);
        assert_eq!(rotated_frame(0.0, &Mat3::scale(1.0, -1.0)), 0.0);
        // A stretch across the frame axes skews it.
        assert_eq!(rotated_frame(30.0, &Mat3::scale(2.0, 1.0)), 0.0);
        let along = turn(-30.0).then(&Mat3::scale(2.0, 0.5)).then(&turn(30.0));
        assert!((rotated_frame(30.0, &along) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn the_rotation_travels_on_the_wire_only_when_set() {
        let mut entity = path_entity();
        let json = serde_json::to_value(&entity).expect("json");
        assert!(json.get("rotation").is_none());
        entity.transform_with_frame(&Mat3::rotate(0.5));
        let json = serde_json::to_value(&entity).expect("json");
        let back: Entity = serde_json::from_value(json).expect("entity");
        assert!((back.rotation - 0.5f64.to_degrees()).abs() < 1e-9);
    }
}
