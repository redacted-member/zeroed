//! Turns a read drawing's model space into vectors, exploding blocks.

use super::super::nurbs;
use super::super::{ImportWarning, ImportedPath};
use super::objects::{Drawing, Entity, Kind, P3};
use crate::geom::{
    bulge_from_sweep, transform_path, ArcGeom, Mat3, Path, Point, SubPath, Tolerance, Vec2,
};

/// How deep block references are followed.
const MAX_BLOCK_DEPTH: usize = 8;
/// Most block instances one file may explode into, which stops a file of
/// nested arrays from exhausting memory.
const MAX_INSTANCES: usize = 200_000;
/// Most segments one file may draw: a small block placed by a large array
/// would otherwise ask for far more memory than the file suggests.
pub(super) const MAX_SEGMENTS: usize = 5_000_000;

/// The vectors of a drawing's model space, in drawing units.
pub(super) fn paths(drawing: &Drawing) -> (Vec<ImportedPath>, Vec<ImportWarning>) {
    let mut walk = Walk {
        drawing,
        out: Vec::new(),
        warnings: Vec::new(),
        instances: 0,
        segments: 0,
    };
    for entity in &drawing.model {
        walk.entity(entity, Mat3::IDENTITY, None, 0);
    }
    (walk.out, walk.warnings)
}

struct Walk<'a> {
    drawing: &'a Drawing,
    out: Vec<ImportedPath>,
    warnings: Vec<ImportWarning>,
    instances: usize,
    segments: usize,
}

fn point(v: &P3) -> Point {
    Point::new(v[0], v[1])
}

fn finite(v: &P3) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// Turns a plan drawn with its extrusion pointing down back over: the
/// arbitrary axis of (0, 0, -1) runs X the other way.
fn object_space(normal: &P3) -> Mat3 {
    if normal[2] < 0.0 {
        Mat3::scale(-1.0, 1.0)
    } else {
        Mat3::IDENTITY
    }
}

impl Walk<'_> {
    fn push(&mut self, layer: &str, path: Path, place: &Mat3) {
        let count: usize = path.subpaths.iter().map(|s| s.segments.len()).sum();
        if self.segments.saturating_add(count) > MAX_SEGMENTS {
            self.segments = MAX_SEGMENTS;
            self.warnings.push(ImportWarning::BlockTooDeep);
            return;
        }
        self.segments += count;
        if !path.is_empty() {
            self.out.push(ImportedPath {
                layer: layer.to_string(),
                name: None,
                path: transform_path(&path, place),
            });
        }
    }

    /// Reads one entity. `inherited` is the layer of the block reference
    /// being exploded, which entities on layer 0 draw on.
    fn entity(&mut self, entity: &Entity, place: Mat3, inherited: Option<&str>, depth: usize) {
        let own = entity.layer.as_str();
        let layer = match inherited {
            Some(parent) if own == "0" || own.is_empty() => parent.to_string(),
            _ => own.to_string(),
        };
        match &entity.kind {
            Kind::Line { start, end } => {
                if finite(start) && finite(end) {
                    let mut sub = SubPath::open(point(start));
                    sub.line_to(point(end));
                    self.push(&layer, sub.into_path(), &place);
                }
            }
            Kind::Circle {
                centre,
                radius,
                normal,
            } => {
                if finite(centre) {
                    if let Ok(path) = crate::geom::circle(point(centre), *radius) {
                        let m = object_space(normal).then(&place);
                        self.push(&layer, path, &m);
                    }
                }
            }
            Kind::Arc {
                centre,
                radius,
                start,
                end,
                normal,
            } => {
                if let Some(sub) = arc_subpath(centre, *radius, *start, *end) {
                    let m = object_space(normal).then(&place);
                    self.push(&layer, sub.into_path(), &m);
                }
            }
            Kind::Ellipse {
                centre,
                major,
                normal,
                ratio,
                start,
                end,
            } => {
                if let Some(sub) = ellipse_subpath(centre, major, normal, *ratio, *start, *end) {
                    self.push(&layer, sub.into_path(), &place);
                }
            }
            Kind::Polyline {
                points,
                closed,
                normal,
            } => {
                let points: Vec<(Point, f64)> = points
                    .iter()
                    .filter(|(x, y, b)| x.is_finite() && y.is_finite() && b.is_finite())
                    .map(|&(x, y, b)| (Point::new(x, y), b))
                    .collect();
                if let Some(sub) = bulge_subpath(&points, *closed) {
                    let m = object_space(normal).then(&place);
                    self.push(&layer, sub.into_path(), &m);
                }
            }
            Kind::Spline {
                degree,
                knots,
                control,
                weights,
                fit,
                closed,
            } => {
                let curve = nurbs::Spline {
                    degree: *degree,
                    knots: knots.clone(),
                    control: control.iter().map(point).collect(),
                    weights: weights.clone(),
                    fit: fit.iter().map(point).collect(),
                    closed: *closed,
                };
                if let Some(sub) = nurbs::subpath(&curve) {
                    self.push(&layer, sub.into_path(), &place);
                }
            }
            Kind::Text => self.warnings.push(ImportWarning::TextSkipped),
            Kind::Image => self.warnings.push(ImportWarning::ImageSkipped),
            Kind::Insert { .. } => self.insert(&entity.kind, &layer, place, depth),
        }
    }

    fn insert(&mut self, insert: &Kind, layer: &str, place: Mat3, depth: usize) {
        let Kind::Insert {
            block,
            at,
            scale,
            rotation,
            normal,
            array,
        } = insert
        else {
            return;
        };
        if depth >= MAX_BLOCK_DEPTH {
            self.warnings.push(ImportWarning::BlockTooDeep);
            return;
        }
        let Some(definition) = self.drawing.blocks.get(block) else {
            self.warnings.push(ImportWarning::BlockMissing);
            return;
        };
        let base = point(&definition.base);
        let non_zero = |v: f64| if v.is_finite() && v != 0.0 { v } else { 1.0 };
        let rotation = if rotation.is_finite() { *rotation } else { 0.0 };
        let (columns, rows, dx, dy) = array.unwrap_or((1, 1, 0.0, 0.0));
        let (sin, cos) = rotation.sin_cos();
        for row in 0..rows {
            for column in 0..columns {
                self.instances += 1;
                if self.instances > MAX_INSTANCES || self.segments >= MAX_SEGMENTS {
                    self.warnings.push(ImportWarning::BlockTooDeep);
                    return;
                }
                // Array steps run along the rotated axes.
                let (u, v) = (column as f64 * dx, row as f64 * dy);
                let origin = Vec2::new(at[0] + u * cos - v * sin, at[1] + u * sin + v * cos);
                if !(origin.x.is_finite() && origin.y.is_finite()) {
                    continue;
                }
                let local = Mat3::translate(Vec2::new(-base.x, -base.y))
                    .then(&Mat3::scale(non_zero(scale[0]), non_zero(scale[1])))
                    .then(&Mat3::rotate(rotation))
                    .then(&Mat3::translate(origin))
                    .then(&object_space(normal))
                    .then(&place);
                for inner in &definition.entities {
                    self.entity(inner, local, Some(layer), depth + 1);
                }
            }
        }
    }
}

fn arc_subpath(centre: &P3, radius: f64, start: f64, end: f64) -> Option<SubPath> {
    if !(finite(centre) && radius.is_finite() && start.is_finite() && end.is_finite()) {
        return None;
    }
    let mut sweep = (end - start) % std::f64::consts::TAU;
    if sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    let geom = ArcGeom::from_angles(point(centre), radius, start, sweep);
    let mut sub = SubPath::open(geom.start_point());
    sub.arc_to(geom.end_point(), bulge_from_sweep(sweep));
    Some(sub)
}

/// A point of an ellipse at parameter `t`, and its derivative.
fn ellipse_at(centre: Point, a: Vec2, b: Vec2, t: f64) -> (Point, Vec2) {
    let (s, c) = t.sin_cos();
    (
        Point::new(centre.x + a.x * c + b.x * s, centre.y + a.y * c + b.y * s),
        Vec2::new(-a.x * s + b.x * c, -a.y * s + b.y * c),
    )
}

/// An ellipse, whole or from its start to its end parameter, as cubics of
/// at most a quarter turn each.
pub(super) fn ellipse_subpath(
    centre: &P3,
    major: &P3,
    normal: &P3,
    ratio: f64,
    start: f64,
    end: f64,
) -> Option<SubPath> {
    let centre = point(centre);
    let a = Vec2::new(major[0], major[1]);
    if !(a.length() > 0.0 && ratio > 0.0 && start.is_finite() && end.is_finite())
        || !(centre.x.is_finite() && centre.y.is_finite() && ratio.is_finite())
    {
        return None;
    }
    // The minor axis is the normal crossed with the major axis.
    let turn = if normal[2] < 0.0 { -1.0 } else { 1.0 };
    let b = Vec2::new(-a.y * turn, a.x * turn).scaled(ratio);
    let mut sweep = (end - start) % std::f64::consts::TAU;
    if sweep <= 1e-12 {
        sweep += std::f64::consts::TAU;
    }
    let sweep = sweep.min(std::f64::consts::TAU);
    let pieces = (sweep / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let (first, _) = ellipse_at(centre, a, b, start);
    let mut sub = SubPath::open(first);
    for i in 0..pieces {
        let t0 = start + step * i as f64;
        let t1 = t0 + step;
        let (p0, d0) = ellipse_at(centre, a, b, t0);
        let (p1, d1) = ellipse_at(centre, a, b, t1);
        sub.cubic_to(
            Point::new(p0.x + d0.x * k, p0.y + d0.y * k),
            Point::new(p1.x - d1.x * k, p1.y - d1.y * k),
            p1,
        );
    }
    sub.closed = (sweep - std::f64::consts::TAU).abs() < 1e-9;
    Some(sub)
}

/// A subpath from vertices that each carry the bulge of the segment that
/// leaves them; a closed one runs back to its first vertex.
pub(super) fn bulge_subpath(points: &[(Point, f64)], closed: bool) -> Option<SubPath> {
    let (first, _) = *points.first()?;
    let mut sub = SubPath::open(first);
    let count = points.len();
    let edges = if closed {
        count
    } else {
        count.saturating_sub(1)
    };
    for i in 0..edges {
        let (from, bulge) = points[i];
        let (to, _) = points[(i + 1) % count];
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
