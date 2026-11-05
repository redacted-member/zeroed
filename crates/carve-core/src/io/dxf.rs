//! DXF import and export.
//!
//! DXF shares our convention of Y up and a flat XY plane, so coordinates need
//! no flipping: only the drawing units have to be reconciled, which `$INSUNITS`
//! declares. Import reads LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE with bulges,
//! POLYLINE, SPLINE and INSERT, exploding blocks as it goes, and keeps the
//! layer each entity was on. TEXT and MTEXT are reported and skipped, since
//! laying them out needs the font they were written with.
//!
//! Export writes R2000 with LWPOLYLINE bulges, ARC and CIRCLE, one DXF layer
//! per document layer.

use std::path::Path as FsPath;

use dxf::entities::{Entity as DxfEntity, EntityType};
use dxf::enums::{AcadVersion, Units};
use dxf::tables::Layer as DxfLayer;
use dxf::{Drawing, LwPolylineVertex, Point as DxfPoint};

use super::{ImportWarning, ImportedPath};
use crate::error::CoreError;
use crate::geom::{
    bulge_from_sweep, flatten_subpath, sweep_from_bulge, transform_path, ArcGeom, Mat3, Path,
    Point, Segment, SubPath, Tolerance, Vec2,
};

/// Millimetres per drawing unit for the `$INSUNITS` values we can honour.
///
/// Anything else, including "unitless", is taken as millimetres, which is
/// what most CAM packages assume when a file does not say.
pub fn units_to_mm(units: Units) -> f64 {
    match units {
        Units::Inches => 25.4,
        Units::Feet => 304.8,
        Units::Millimeters => 1.0,
        Units::Centimeters => 10.0,
        Units::Meters => 1000.0,
        Units::Mils => 0.0254,
        Units::Microinches => 2.54e-5,
        Units::Yards => 914.4,
        Units::Decimeters => 100.0,
        Units::Microns => 0.001,
        _ => 1.0,
    }
}

/// Parses the bytes of a DXF file. The parser is third party code working
/// on untrusted bytes; a panic in it (a damaged header date overflows its
/// date arithmetic) ends as an error for this file instead of a crash.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes cannot be parsed as DXF.
pub fn load(bytes: &[u8]) -> Result<Drawing, CoreError> {
    std::panic::catch_unwind(|| {
        let mut reader = bytes;
        Drawing::load(&mut reader)
    })
    .map_err(|_| CoreError::format("DXF", "the file is damaged and could not be read"))?
    .map_err(|error| CoreError::format("DXF", format!("{error}")))
}

/// Reads vectors out of a DXF file.
///
/// `scale` multiplies every length after `$INSUNITS` handling. A `units_hint`
/// of `mm` or `inch` overrides what the file says, which is what the import
/// dialog offers when a file lies about its units.
///
/// # Errors
/// Returns [`CoreError::Format`] when the file cannot be parsed as DXF.
pub fn import(
    path: &FsPath,
    scale: f64,
    units_hint: Option<&str>,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let bytes = std::fs::read(path).map_err(|error| CoreError::io(path.display(), error))?;
    let drawing = load(&bytes)?;
    let per_unit = match units_hint {
        Some("mm") => 1.0,
        Some("inch") | Some("in") => 25.4,
        _ => units_to_mm(drawing.header.default_drawing_units),
    };
    let scale = if scale.is_finite() && scale != 0.0 {
        scale
    } else {
        1.0
    };
    let factor = per_unit * scale;
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for entity in drawing.entities() {
        read_entity(&drawing, entity, Mat3::IDENTITY, 0, &mut out, &mut warnings);
    }
    if factor != 1.0 {
        let m = Mat3::scale(factor, factor);
        for imported in &mut out {
            imported.path = transform_path(&imported.path, &m);
        }
    }
    warnings.sort_by_key(|w| format!("{w:?}"));
    warnings.dedup();
    Ok((out, warnings))
}

/// How deep block references are followed before giving up, which stops a
/// file whose blocks refer to each other from looping forever.
const MAX_BLOCK_DEPTH: usize = 8;

fn read_entity(
    drawing: &Drawing,
    entity: &DxfEntity,
    parent: Mat3,
    depth: usize,
    out: &mut Vec<ImportedPath>,
    warnings: &mut Vec<ImportWarning>,
) {
    let layer = entity.common.layer.clone();
    let mut push = |path: Path| {
        if !path.is_empty() {
            out.push(ImportedPath {
                layer: layer.clone(),
                name: None,
                path: transform_path(&path, &parent),
            });
        }
    };
    match &entity.specific {
        EntityType::Line(line) => {
            let mut sub = SubPath::open(point_of(&line.p1));
            sub.line_to(point_of(&line.p2));
            push(sub.into_path());
        }
        EntityType::Circle(circle) => {
            if let Ok(path) = crate::geom::circle(point_of(&circle.center), circle.radius) {
                push(in_ocs(&path, &circle.normal));
            }
        }
        EntityType::Arc(arc) => {
            let start = arc.start_angle.to_radians();
            let end = arc.end_angle.to_radians();
            let mut sweep = end - start;
            while sweep <= 0.0 {
                sweep += std::f64::consts::TAU;
            }
            let centre = point_of(&arc.center);
            let geom = ArcGeom::from_angles(centre, arc.radius, start, sweep);
            let mut sub = SubPath::open(geom.start_point());
            sub.arc_to(geom.end_point(), bulge_from_sweep(sweep));
            push(in_ocs(&sub.into_path(), &arc.normal));
        }
        EntityType::Ellipse(ellipse) => {
            if let Some(sub) = ellipse_subpath(ellipse) {
                push(sub.into_path());
            }
        }
        EntityType::LwPolyline(polyline) => {
            let mut points: Vec<(Point, f64)> = polyline
                .vertices
                .iter()
                .map(|v| (Point::new(v.x, v.y), v.bulge))
                .collect();
            if polyline.is_closed() {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            if let Some(sub) = from_bulge_points(&points, polyline.is_closed()) {
                push(in_ocs(&sub.into_path(), &polyline.extrusion_direction));
            }
        }
        EntityType::Polyline(polyline) => {
            let mut points: Vec<(Point, f64)> = polyline
                .vertices()
                .map(|v| (point_of(&v.location), v.bulge))
                .collect();
            let closed = polyline.is_closed();
            if closed {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            if let Some(sub) = from_bulge_points(&points, closed) {
                // A 3D polyline's vertices are in world coordinates already.
                if polyline.is_3d_polyline() || polyline.is_3d_polygon_mesh() {
                    push(sub.into_path());
                } else {
                    push(in_ocs(&sub.into_path(), &polyline.normal));
                }
            }
        }
        EntityType::Spline(spline) => {
            if let Some(sub) = spline_subpath(spline) {
                push(sub.into_path());
            }
        }
        EntityType::Text(_) | EntityType::MText(_) | EntityType::ArcAlignedText(_) => {
            warnings.push(ImportWarning::TextSkipped);
        }
        EntityType::Insert(insert) => {
            if depth >= MAX_BLOCK_DEPTH {
                warnings.push(ImportWarning::BlockTooDeep);
                return;
            }
            let Some(block) = drawing.blocks().find(|b| b.name == insert.name) else {
                warnings.push(ImportWarning::BlockMissing);
                return;
            };
            // The insertion point, rotation and scales are in the reference's
            // object coordinate system, which a mirrored reference flips.
            let ocs = ocs_matrix(&insert.extrusion_direction).unwrap_or(Mat3::IDENTITY);
            let base = point_of(&block.base_point);
            let local = Mat3::translate(Vec2::new(-base.x, -base.y))
                .then(&Mat3::scale(
                    non_zero(insert.x_scale_factor),
                    non_zero(insert.y_scale_factor),
                ))
                .then(&Mat3::rotate(insert.rotation.to_radians()))
                .then(&Mat3::translate(Vec2::new(
                    insert.location.x,
                    insert.location.y,
                )))
                .then(&ocs)
                .then(&parent);
            for inner in &block.entities {
                read_entity(drawing, inner, local, depth + 1, out, warnings);
            }
        }
        EntityType::Image(_) => warnings.push(ImportWarning::ImageSkipped),
        _ => {}
    }
}

/// The plan view of an object coordinate system: where the object's own X
/// and Y axes (in which a circle, arc, 2D polyline or block reference is
/// written) lie in the drawing, by the DXF arbitrary axis rule for the
/// extrusion `normal`. A mirrored object (normal 0,0,-1) has its X axis reversed. None for the
/// usual normal 0,0,1, or one that is not a direction.
fn ocs_matrix(normal: &dxf::Vector) -> Option<Mat3> {
    let length = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
    if !length.is_finite() || length < 1e-12 {
        return None;
    }
    let n = [normal.x / length, normal.y / length, normal.z / length];
    if (n[2] - 1.0).abs() < 1e-12 {
        return None;
    }
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let world = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let ax = cross(world, n);
    let ax_len = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
    let ax = [ax[0] / ax_len, ax[1] / ax_len, ax[2] / ax_len];
    let ay = cross(n, ax);
    Some(Mat3::new(ax[0], ax[1], ay[0], ay[1], 0.0, 0.0))
}

/// A path written in the object coordinate system of `normal`, in the
/// drawing's coordinates.
fn in_ocs(path: &Path, normal: &dxf::Vector) -> Path {
    match ocs_matrix(normal) {
        Some(m) => transform_path(path, &m),
        None => path.clone(),
    }
}

/// An ELLIPSE, whole or the arc between its start and end parameters, as
/// cubic spans of at most a quarter turn each. Its centre and major axis are
/// in drawing coordinates; the minor axis lies a quarter turn from the major
/// one about the extrusion direction, so a mirrored ellipse runs the other
/// way round.
fn ellipse_subpath(ellipse: &dxf::entities::Ellipse) -> Option<SubPath> {
    let centre = point_of(&ellipse.center);
    let major = Vec2::new(ellipse.major_axis.x, ellipse.major_axis.y);
    let ratio = ellipse.minor_axis_ratio;
    if !(major.length() > 0.0 && ratio > 0.0 && ratio.is_finite()) {
        return None;
    }
    let turn = if ellipse.normal.z < 0.0 { -1.0 } else { 1.0 };
    let minor = Vec2::new(-major.y * turn * ratio, major.x * turn * ratio);
    let tau = std::f64::consts::TAU;
    let (start, end) = (ellipse.start_parameter, ellipse.end_parameter);
    let mut sweep = (end - start) % tau;
    if sweep <= 1e-9 {
        sweep += tau;
    }
    let whole = (sweep - tau).abs() < 1e-9;
    let at = |t: f64| {
        Point::new(
            centre.x + major.x * t.cos() + minor.x * t.sin(),
            centre.y + major.y * t.cos() + minor.y * t.sin(),
        )
    };
    let along = |t: f64| {
        Vec2::new(
            -major.x * t.sin() + minor.x * t.cos(),
            -major.y * t.sin() + minor.y * t.cos(),
        )
    };
    let spans = (sweep / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / spans as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut sub = SubPath::open(at(start));
    for i in 0..spans {
        let t0 = start + step * i as f64;
        let t1 = t0 + step;
        let (p0, p3) = (
            at(t0),
            if whole && i + 1 == spans {
                at(start)
            } else {
                at(t1)
            },
        );
        let (d0, d1) = (along(t0), along(t1));
        sub.cubic_to(
            Point::new(p0.x + k * d0.x, p0.y + k * d0.y),
            Point::new(p3.x - k * d1.x, p3.y - k * d1.y),
            p3,
        );
    }
    sub.closed = whole;
    Some(sub)
}

fn non_zero(value: f64) -> f64 {
    if value.is_finite() && value != 0.0 {
        value
    } else {
        1.0
    }
}

fn point_of(p: &DxfPoint) -> Point {
    Point::new(p.x, p.y)
}

/// Builds a subpath from vertices that each carry the bulge of the segment
/// that leaves them, which is the DXF polyline convention.
fn from_bulge_points(points: &[(Point, f64)], closed: bool) -> Option<SubPath> {
    let (first, _) = *points.first()?;
    if points.len() < 2 {
        return None;
    }
    let mut sub = SubPath::open(first);
    for window in points.windows(2) {
        let (from, bulge) = window[0];
        let (to, _) = window[1];
        if from.approx_eq(to, Tolerance::EQUALITY) {
            continue;
        }
        if bulge.abs() < 1e-12 {
            sub.line_to(to);
        } else {
            sub.arc_to(to, bulge);
        }
    }
    if sub.segments.is_empty() {
        return None;
    }
    sub.closed = closed;
    Some(sub)
}

/// Turns a DXF spline into a path through [`super::nurbs`]: exactly for
/// clamped non-rational splines up to cubic, finely sampled otherwise, or
/// through its fit points when it has no control points.
fn spline_subpath(spline: &dxf::entities::Spline) -> Option<SubPath> {
    super::nurbs::subpath(&super::nurbs::Spline {
        degree: usize::try_from(spline.degree_of_curve).unwrap_or(0),
        knots: spline.knot_values.clone(),
        control: spline.control_points.iter().map(point_of).collect(),
        weights: spline.weight_values.clone(),
        fit: spline.fit_points.iter().map(point_of).collect(),
        closed: spline.is_closed() || spline.is_periodic(),
    })
}

/// One group of vectors to write out.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLayer {
    /// Layer name.
    pub name: String,
    /// The vectors on it.
    pub paths: Vec<Path>,
}

/// Writes vectors to a DXF file in millimetres.
///
/// # Errors
/// Returns [`CoreError::Io`] when the file cannot be written.
pub fn export(layers: &[ExportLayer], path: &FsPath) -> Result<(), CoreError> {
    let mut drawing = Drawing::new();
    drawing.header.version = AcadVersion::R2000;
    drawing.header.default_drawing_units = Units::Millimeters;
    for layer in layers {
        if drawing.layers().all(|l| l.name != layer.name) {
            drawing.add_layer(DxfLayer {
                name: layer.name.clone(),
                ..Default::default()
            });
        }
        for vector in &layer.paths {
            for sub in &vector.subpaths {
                for entity in subpath_entities(sub) {
                    let mut entity = DxfEntity::new(entity);
                    entity.common.layer = layer.name.clone();
                    drawing.add_entity(entity);
                }
            }
        }
    }
    drawing
        .save_file(path)
        .map_err(|error| CoreError::io(path.display(), error))?;
    Ok(())
}

/// The DXF entities that carry one subpath.
///
/// A full circle becomes a CIRCLE, a subpath made only of lines and arcs
/// becomes one LWPOLYLINE with bulges, and anything with a cubic in it is
/// flattened into an LWPOLYLINE of straight segments, since DXF has no cubic
/// Bezier entity that every reader understands.
fn subpath_entities(sub: &SubPath) -> Vec<EntityType> {
    if sub.segments.is_empty() {
        return Vec::new();
    }
    if let Some(circle) = whole_circle(sub) {
        return vec![EntityType::Circle(dxf::entities::Circle {
            center: DxfPoint::new(circle.centre.x, circle.centre.y, 0.0),
            radius: circle.radius,
            ..Default::default()
        })];
    }
    let has_cubic = sub
        .segments
        .iter()
        .any(|s| matches!(s, Segment::Cubic { .. }));
    let mut vertices: Vec<LwPolylineVertex> = Vec::new();
    if has_cubic {
        let polyline = flatten_subpath(sub, Tolerance::default());
        let points = if sub.closed && polyline.points.len() > 1 {
            &polyline.points[..polyline.points.len()]
        } else {
            &polyline.points[..]
        };
        for point in points {
            vertices.push(vertex(*point, 0.0));
        }
    } else {
        let last = sub.segments.len().saturating_sub(1);
        vertices.push(vertex(sub.start, bulge_of(&sub.segments[0])));
        for (index, segment) in sub.segments.iter().enumerate() {
            if sub.closed && index == last {
                break;
            }
            let next = sub
                .segments
                .get(index + 1)
                .map(bulge_of)
                .unwrap_or_default();
            vertices.push(vertex(segment.end(), next));
        }
    }
    if vertices.len() < 2 {
        return Vec::new();
    }
    let mut polyline = dxf::entities::LwPolyline {
        vertices,
        ..Default::default()
    };
    polyline.set_is_closed(sub.closed);
    vec![EntityType::LwPolyline(polyline)]
}

fn vertex(point: Point, bulge: f64) -> LwPolylineVertex {
    LwPolylineVertex {
        x: point.x,
        y: point.y,
        bulge,
        ..Default::default()
    }
}

fn bulge_of(segment: &Segment) -> f64 {
    match segment {
        Segment::Arc { bulge, .. } => *bulge,
        _ => 0.0,
    }
}

/// The circle a closed subpath describes, when it is two half circle arcs,
/// which is how `geom::circle` builds one.
fn whole_circle(sub: &SubPath) -> Option<ArcGeom> {
    if !sub.closed || sub.segments.len() != 2 {
        return None;
    }
    let total: f64 = sub
        .segments
        .iter()
        .map(|s| sweep_from_bulge(bulge_of(s)))
        .sum();
    if (total.abs() - std::f64::consts::TAU).abs() > 1e-9 {
        return None;
    }
    match sub.segments[0] {
        Segment::Arc { to, bulge } => ArcGeom::from_bulge(sub.start, to, bulge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, path_bbox, path_length, rect};

    fn scratch(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("carve-dxf-{}-{name}.dxf", std::process::id()));
        path
    }

    fn write_and_read(name: &str, paths: Vec<Path>) -> Vec<ImportedPath> {
        let file = scratch(name);
        export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                paths,
            }],
            &file,
        )
        .expect("exports");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        back
    }

    fn import_entities(name: &str, entities: Vec<EntityType>) -> Vec<ImportedPath> {
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        for specific in entities {
            drawing.add_entity(DxfEntity::new(specific));
        }
        let file = scratch(name);
        drawing.save_file(&file).expect("saves");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        back
    }

    fn ends(path: &Path) -> (Point, Point) {
        (
            path.start_point().expect("start"),
            path.end_point().expect("end"),
        )
    }

    fn near(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    // Review 033 row 4: an object written with extrusion 0,0,-1 is mirrored
    // across the Y axis, as the drawing shows it.
    #[test]
    fn mirrored_arcs_circles_and_polylines_land_on_the_mirrored_side() {
        let down = dxf::Vector::new(0.0, 0.0, -1.0);
        let mut arc = dxf::entities::Arc::new(DxfPoint::new(10.0, 0.0, 0.0), 5.0, 0.0, 90.0);
        arc.normal = down.clone();
        let mut circle = dxf::entities::Circle::new(DxfPoint::new(10.0, 0.0, 0.0), 5.0);
        circle.normal = down.clone();
        let mut poly = dxf::entities::LwPolyline::default();
        for (x, y) in [(10.0, 0.0), (20.0, 0.0), (20.0, 10.0)] {
            poly.vertices.push(LwPolylineVertex {
                x,
                y,
                ..Default::default()
            });
        }
        poly.extrusion_direction = down;
        let back = import_entities(
            "mirrored",
            vec![
                EntityType::Arc(arc),
                EntityType::Circle(circle),
                EntityType::LwPolyline(poly),
            ],
        );
        assert_eq!(back.len(), 3);
        let (s, e) = ends(&back[0].path);
        assert!(
            near(s, Point::new(-15.0, 0.0)) && near(e, Point::new(-10.0, 5.0)),
            "{s:?} {e:?}"
        );
        // The arc bulges away from its centre at (-10, 0).
        let mid =
            crate::geom::path_point_at(&back[0].path, 0.5, Tolerance::default()).expect("mid");
        let r = 0.5f64.sqrt() * 5.0;
        assert!(near(mid, Point::new(-10.0 - r, r)), "{mid:?}");
        let b = path_bbox(&back[1].path).expect("bbox");
        assert!(
            near(b.min, Point::new(-15.0, -5.0)) && near(b.max, Point::new(-5.0, 5.0)),
            "{b:?}"
        );
        let (s, e) = ends(&back[2].path);
        assert!(
            near(s, Point::new(-10.0, 0.0)) && near(e, Point::new(-20.0, 10.0)),
            "{s:?} {e:?}"
        );
    }

    // A block reference written with extrusion 0,0,-1 places, turns and
    // scales its block in its own object coordinate system, so the whole
    // block lands mirrored across the Y axis.
    #[test]
    fn a_mirrored_block_reference_imports_mirrored() {
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        let line = dxf::entities::Line {
            p1: DxfPoint::new(1.0, 1.0, 0.0),
            p2: DxfPoint::new(11.0, 6.0, 0.0),
            ..Default::default()
        };
        drawing.add_block(dxf::Block {
            name: "PART".to_string(),
            base_point: DxfPoint::new(1.0, 1.0, 0.0),
            entities: vec![DxfEntity::new(EntityType::Line(line))],
            ..Default::default()
        });
        let flat = dxf::entities::Insert {
            name: "PART".to_string(),
            location: DxfPoint::new(20.0, 3.0, 0.0),
            x_scale_factor: 2.0,
            y_scale_factor: 3.0,
            rotation: 90.0,
            ..Default::default()
        };
        let mut mirrored = flat.clone();
        mirrored.extrusion_direction = dxf::Vector::new(0.0, 0.0, -1.0);
        drawing.add_entity(DxfEntity::new(EntityType::Insert(flat)));
        drawing.add_entity(DxfEntity::new(EntityType::Insert(mirrored)));
        let file = scratch("mirrored-insert");
        drawing.save_file(&file).expect("saves");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        assert_eq!(back.len(), 2);
        // (10, 5) from the base, scaled to (20, 15), turned a quarter to
        // (-15, 20), moved by (20, 3): (5, 23). Mirrored: x negated.
        let (s, e) = ends(&back[0].path);
        assert!(
            near(s, Point::new(20.0, 3.0)) && near(e, Point::new(5.0, 23.0)),
            "{s:?} {e:?}"
        );
        let (s, e) = ends(&back[1].path);
        assert!(
            near(s, Point::new(-20.0, 3.0)) && near(e, Point::new(-5.0, 23.0)),
            "{s:?} {e:?}"
        );
    }

    // Review 033 row 5: an elliptical arc comes in open, between its
    // parameters.
    #[test]
    fn an_elliptical_arc_comes_in_as_the_arc_only() {
        let half = dxf::entities::Ellipse {
            center: DxfPoint::new(0.0, 0.0, 0.0),
            major_axis: dxf::Vector::new(10.0, 0.0, 0.0),
            minor_axis_ratio: 0.5,
            start_parameter: 0.0,
            end_parameter: std::f64::consts::PI,
            ..Default::default()
        };
        let mut whole = half.clone();
        whole.end_parameter = std::f64::consts::TAU;
        let mut flipped = half.clone();
        flipped.normal = dxf::Vector::new(0.0, 0.0, -1.0);
        let back = import_entities(
            "ellipse-arc",
            vec![
                EntityType::Ellipse(half),
                EntityType::Ellipse(whole),
                EntityType::Ellipse(flipped),
            ],
        );
        assert_eq!(back.len(), 3);
        assert!(!back[0].path.is_closed());
        let (s, e) = ends(&back[0].path);
        assert!(
            near(s, Point::new(10.0, 0.0)) && near(e, Point::new(-10.0, 0.0)),
            "{s:?} {e:?}"
        );
        let b = path_bbox(&back[0].path).expect("bbox");
        assert!(
            b.min.y.abs() < 1e-9 && (b.max.y - 5.0).abs() < 1e-3,
            "{b:?}"
        );
        assert!(back[1].path.is_closed());
        let b = path_bbox(&back[1].path).expect("bbox");
        assert!(
            (b.min.y + 5.0).abs() < 1e-3 && (b.max.x - 10.0).abs() < 1e-9,
            "{b:?}"
        );
        // Mirrored, the same parameters run under the axis.
        let b = path_bbox(&back[2].path).expect("bbox");
        assert!(
            (b.min.y + 5.0).abs() < 1e-3 && b.max.y.abs() < 1e-9,
            "{b:?}"
        );
    }

    #[test]
    fn a_rectangle_survives_an_export_and_import() {
        let source = rect(Point::new(10.0, 20.0), Point::new(60.0, 50.0)).expect("rect");
        let back = write_and_read("rect", vec![source.clone()]);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].layer, "Layer 1");
        let a = path_bbox(&source).expect("bbox");
        let b = path_bbox(&back[0].path).expect("bbox");
        assert!((a.min.x - b.min.x).abs() < 1e-9, "{a:?} {b:?}");
        assert!((a.max.y - b.max.y).abs() < 1e-9, "{a:?} {b:?}");
        assert!(back[0].path.is_closed());
    }

    #[test]
    fn a_circle_comes_back_as_a_circle() {
        let source = circle(Point::new(40.0, 30.0), 12.5).expect("circle");
        let back = write_and_read("circle", vec![source.clone()]);
        assert_eq!(back.len(), 1);
        let tol = Tolerance::default();
        assert!(
            (path_length(&source, tol) - path_length(&back[0].path, tol)).abs() < 1e-6,
            "the circumference is kept"
        );
    }

    #[test]
    fn an_arc_keeps_its_bulge() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.arc_to(Point::new(20.0, 0.0), 0.5);
        let back = write_and_read("arc", vec![sub.into_path()]);
        let segments = &back[0].path.subpaths[0].segments;
        match segments[0] {
            Segment::Arc { bulge, .. } => assert!((bulge - 0.5).abs() < 1e-9, "{bulge}"),
            ref other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn an_open_polyline_stays_open() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let back = write_and_read("open", vec![sub.into_path()]);
        assert!(!back[0].path.is_closed());
        assert_eq!(back[0].path.subpaths[0].segments.len(), 2);
    }

    #[test]
    fn a_curve_is_flattened_into_a_polyline() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(10.0, 30.0),
            Point::new(30.0, -30.0),
            Point::new(40.0, 0.0),
        );
        let source = sub.into_path();
        let back = write_and_read("curve", vec![source.clone()]);
        let tol = Tolerance::default();
        let a = path_length(&source, tol);
        let b = path_length(&back[0].path, tol);
        assert!((a - b).abs() < 0.5, "{a} vs {b}");
    }

    #[test]
    fn layers_are_written_and_read_back() {
        let file = scratch("layers");
        export(
            &[
                ExportLayer {
                    name: "Cut".to_string(),
                    paths: vec![rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")],
                },
                ExportLayer {
                    name: "Engrave".to_string(),
                    paths: vec![circle(Point::new(20.0, 20.0), 5.0).expect("circle")],
                },
            ],
            &file,
        )
        .expect("exports");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        let mut layers: Vec<String> = back.iter().map(|p| p.layer.clone()).collect();
        layers.sort();
        assert_eq!(layers, vec!["Cut".to_string(), "Engrave".to_string()]);
    }

    #[test]
    fn a_damaged_date_ends_as_an_error_not_a_panic() {
        // The parser adds a header date's days to its epoch; a damaged
        // value overflows inside it (found by tests/fuzz_readers.rs).
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        let mut bytes = Vec::new();
        drawing.save(&mut bytes).expect("saves");
        let text = String::from_utf8(bytes).expect("ASCII DXF");
        let at = text.find("$TDCREATE").expect("a creation date");
        let value = text[at..].find(" 40\r\n").expect("its value") + at + 5;
        let end = value + text[value..].find("\r\n").expect("line end");
        let damaged = format!("{}1e308{}", &text[..value], &text[end..]);
        let file = scratch("damaged-date");
        std::fs::write(&file, damaged.as_bytes()).expect("written");
        let result = import(&file, 1.0, None);
        std::fs::remove_file(&file).ok();
        assert!(
            matches!(&result, Err(CoreError::Format { format, .. }) if format == "DXF"),
            "{result:?}"
        );
        assert!(matches!(
            super::super::dxf_mesh::read(damaged.as_bytes()),
            Err(CoreError::Format { .. })
        ));
    }

    #[test]
    fn inch_units_are_converted_to_millimetres() {
        let file = scratch("inches");
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        drawing.header.default_drawing_units = Units::Inches;
        drawing.add_entity(DxfEntity::new(EntityType::Line(dxf::entities::Line {
            p1: DxfPoint::new(0.0, 0.0, 0.0),
            p2: DxfPoint::new(1.0, 0.0, 0.0),
            ..Default::default()
        })));
        drawing.save_file(&file).expect("saves");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        let bbox = path_bbox(&back[0].path).expect("bbox");
        assert!((bbox.width() - 25.4).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn a_units_hint_overrides_what_the_file_says() {
        let file = scratch("hint");
        let mut drawing = Drawing::new();
        drawing.header.default_drawing_units = Units::Inches;
        drawing.add_entity(DxfEntity::new(EntityType::Line(dxf::entities::Line {
            p1: DxfPoint::new(0.0, 0.0, 0.0),
            p2: DxfPoint::new(1.0, 0.0, 0.0),
            ..Default::default()
        })));
        drawing.save_file(&file).expect("saves");
        let (back, _) = import(&file, 1.0, Some("mm")).expect("imports");
        std::fs::remove_file(&file).ok();
        let bbox = path_bbox(&back[0].path).expect("bbox");
        assert!((bbox.width() - 1.0).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn a_spline_follows_its_knots() {
        let file = scratch("spline");
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        drawing.header.default_drawing_units = Units::Millimeters;
        drawing.add_entity(DxfEntity::new(EntityType::Spline(dxf::entities::Spline {
            degree_of_curve: 3,
            knot_values: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
            control_points: vec![
                DxfPoint::new(0.0, 0.0, 0.0),
                DxfPoint::new(10.0, 30.0, 0.0),
                DxfPoint::new(20.0, -10.0, 0.0),
                DxfPoint::new(40.0, 20.0, 0.0),
                DxfPoint::new(50.0, 0.0, 0.0),
            ],
            ..Default::default()
        })));
        drawing.save_file(&file).expect("saves");
        let (back, _) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        let sub = &back[0].path.subpaths[0];
        // Two Bezier pieces, ending on the last control point.
        assert_eq!(sub.segments.len(), 2);
        assert!(sub.end().distance(Point::new(50.0, 0.0)) < 1e-9);
        // The curve stays inside its control polygon's box.
        let bbox = path_bbox(&back[0].path).expect("bbox");
        assert!(bbox.max.y < 30.0 && bbox.min.y > -10.0, "{bbox:?}");
    }

    #[test]
    fn text_is_skipped_and_reported() {
        let file = scratch("text");
        let mut drawing = Drawing::new();
        drawing.add_entity(DxfEntity::new(EntityType::Text(dxf::entities::Text {
            value: "hello".to_string(),
            ..Default::default()
        })));
        drawing.save_file(&file).expect("saves");
        let (back, warnings) = import(&file, 1.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        assert!(back.is_empty());
        assert_eq!(warnings, vec![ImportWarning::TextSkipped]);
    }

    #[test]
    fn a_file_that_is_not_dxf_is_reported() {
        let file = scratch("bad");
        std::fs::write(&file, b"this is not a drawing").expect("writes");
        let error = import(&file, 1.0, None).expect_err("not a drawing");
        std::fs::remove_file(&file).ok();
        assert!(matches!(error, CoreError::Format { .. }), "{error}");
    }

    #[test]
    fn the_scale_option_multiplies_everything() {
        let file = scratch("scale");
        export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                paths: vec![rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")],
            }],
            &file,
        )
        .expect("exports");
        let (back, _) = import(&file, 2.0, None).expect("imports");
        std::fs::remove_file(&file).ok();
        let bbox = path_bbox(&back[0].path).expect("bbox");
        assert!((bbox.width() - 20.0).abs() < 1e-9, "{bbox:?}");
    }
}
