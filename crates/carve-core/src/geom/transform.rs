//! Applying affine transforms to paths.
//!
//! A circular arc stays a circular arc only under a similarity: a rotation, a
//! uniform scale, a mirror and a translation. Under a non uniform scale or a
//! shear the arc becomes an ellipse, which this model does not carry, so such
//! arcs are converted to cubic Beziers before the transform is applied. Cubic
//! segments transform exactly by moving their control points.

use super::path::{ArcGeom, Path, Polyline, Segment, SubPath};
use super::point::{Mat3, Point};

/// Splits an arc segment into cubic Bezier segments, at most a quarter turn
/// each, so the result stays within about 0.02 percent of the true arc.
pub fn arc_to_cubic_segments(start: Point, to: Point, bulge: f64) -> Vec<Segment> {
    let Some(arc) = ArcGeom::from_bulge(start, to, bulge) else {
        return vec![Segment::Line { to }];
    };
    let pieces = ((arc.sweep.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = arc.sweep / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::with_capacity(pieces);
    for i in 0..pieces {
        let a0 = arc.start_angle + step * i as f64;
        let a1 = a0 + step;
        let p0 = Point::new(
            arc.centre.x + arc.radius * a0.cos(),
            arc.centre.y + arc.radius * a0.sin(),
        );
        let p3 = if i + 1 == pieces {
            to
        } else {
            Point::new(
                arc.centre.x + arc.radius * a1.cos(),
                arc.centre.y + arc.radius * a1.sin(),
            )
        };
        let c1 = Point::new(
            p0.x - k * arc.radius * a0.sin(),
            p0.y + k * arc.radius * a0.cos(),
        );
        let c2 = Point::new(
            p3.x + k * arc.radius * a1.sin(),
            p3.y - k * arc.radius * a1.cos(),
        );
        out.push(Segment::Cubic { c1, c2, to: p3 });
    }
    out
}

/// Applies a transform to one segment, given its start point.
///
/// Returns the segments that replace it: one for a line or cubic, and one or
/// more for an arc that had to become cubics.
pub fn transform_segment(start: Point, segment: &Segment, m: &Mat3) -> Vec<Segment> {
    match *segment {
        Segment::Line { to } => vec![Segment::Line { to: m.apply(to) }],
        Segment::Cubic { c1, c2, to } => vec![Segment::Cubic {
            c1: m.apply(c1),
            c2: m.apply(c2),
            to: m.apply(to),
        }],
        Segment::Arc { to, bulge } => {
            if m.is_similarity() {
                let sign = if m.is_mirroring() { -1.0 } else { 1.0 };
                vec![Segment::Arc {
                    to: m.apply(to),
                    bulge: bulge * sign,
                }]
            } else {
                let mut cursor = start;
                arc_to_cubic_segments(start, to, bulge)
                    .into_iter()
                    .map(|seg| {
                        let from = cursor;
                        cursor = seg.end();
                        transform_segment(from, &seg, m)
                    })
                    .reduce(|mut acc, mut next| {
                        acc.append(&mut next);
                        acc
                    })
                    .unwrap_or_default()
            }
        }
    }
}

/// Applies a transform to one subpath.
pub fn transform_subpath(subpath: &SubPath, m: &Mat3) -> SubPath {
    let mut out = SubPath::open(m.apply(subpath.start));
    for (start, seg) in subpath.iter() {
        out.segments.extend(transform_segment(start, seg, m));
    }
    out.closed = subpath.closed;
    out
}

/// Applies a transform to every subpath of a path.
pub fn transform_path(path: &Path, m: &Mat3) -> Path {
    Path::from_subpaths(
        path.subpaths
            .iter()
            .map(|s| transform_subpath(s, m))
            .collect(),
    )
}

/// Applies a transform to a flattened polyline.
pub fn transform_polyline(polyline: &Polyline, m: &Mat3) -> Polyline {
    Polyline::new(
        polyline.points.iter().map(|p| m.apply(*p)).collect(),
        polyline.closed,
    )
}

/// Replaces every arc in a path with cubic segments.
pub fn arcs_to_cubics(path: &Path) -> Path {
    let mut out = Path::new();
    for sub in &path.subpaths {
        let mut new_sub = SubPath::open(sub.start);
        for (start, seg) in sub.iter() {
            match *seg {
                Segment::Arc { to, bulge } => {
                    new_sub
                        .segments
                        .extend(arc_to_cubic_segments(start, to, bulge));
                }
                other => new_sub.segments.push(other),
            }
        }
        new_sub.closed = sub.closed;
        out.push(new_sub);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::metrics::{path_area, path_bbox, path_length};
    use crate::geom::point::{Tolerance, Vec2};

    const FLAT: Tolerance = Tolerance::FLATTEN;

    #[test]
    fn translation_moves_the_bbox() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 5.0)).expect("rect");
        let moved = transform_path(&path, &Mat3::translate(Vec2::new(3.0, 4.0)));
        let b = path_bbox(&moved).expect("bbox");
        assert!((b.min.x - 3.0).abs() < 1e-12);
        assert!((b.min.y - 4.0).abs() < 1e-12);
        assert!((path_area(&moved, FLAT) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn uniform_scale_keeps_arcs_as_arcs() {
        let path = circle(Point::ORIGIN, 4.0).expect("circle");
        let scaled = transform_path(&path, &Mat3::scale(2.0, 2.0));
        assert_eq!(scaled.subpaths[0].segments.len(), 2);
        assert!(scaled.subpaths[0]
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Arc { .. })));
        let area = path_area(&scaled, FLAT);
        assert!((area - std::f64::consts::PI * 64.0).abs() < 1e-9);
    }

    #[test]
    fn mirroring_flips_the_bulge_sign() {
        let path = circle(Point::ORIGIN, 4.0).expect("circle");
        let m = Mat3::mirror_across_line(Point::ORIGIN, Vec2::new(0.0, 1.0)).expect("mirror");
        let mirrored = transform_path(&path, &m);
        let bulges: Vec<f64> = mirrored.subpaths[0]
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Arc { bulge, .. } => Some(*bulge),
                _ => None,
            })
            .collect();
        assert_eq!(bulges, vec![-1.0, -1.0]);
        assert!(path_area(&mirrored, FLAT) < 0.0);
    }

    #[test]
    fn non_uniform_scale_turns_arcs_into_cubics() {
        let path = circle(Point::ORIGIN, 4.0).expect("circle");
        let squashed = transform_path(&path, &Mat3::scale(2.0, 1.0));
        assert!(squashed.subpaths[0]
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Cubic { .. })));
        let area = path_area(&squashed, FLAT).abs();
        let expected = std::f64::consts::PI * 8.0 * 4.0;
        assert!((area - expected).abs() / expected < 0.01, "area {area}");
    }

    #[test]
    fn arc_to_cubic_segments_covers_a_full_circle() {
        let segs = arc_to_cubic_segments(Point::new(2.0, 0.0), Point::new(-2.0, 0.0), 1.0);
        assert_eq!(segs.len(), 2);
        let last = segs.last().expect("segments").end();
        assert!(last.approx_eq(Point::new(-2.0, 0.0), Tolerance::EQUALITY));
    }

    #[test]
    fn arc_to_cubic_keeps_the_length() {
        let path = circle(Point::ORIGIN, 10.0).expect("circle");
        let cubics = arcs_to_cubics(&path);
        assert_eq!(cubics.subpaths[0].segments.len(), 4);
        let len = path_length(&cubics, Tolerance::new(1e-6).expect("tolerance"));
        let exact = std::f64::consts::TAU * 10.0;
        assert!((len - exact).abs() / exact < 5e-4, "len {len}");
    }

    #[test]
    fn zero_bulge_arc_converts_to_a_line() {
        let segs = arc_to_cubic_segments(Point::ORIGIN, Point::new(3.0, 0.0), 0.0);
        assert_eq!(segs.len(), 1);
        assert!(matches!(segs[0], Segment::Line { .. }));
    }

    #[test]
    fn rotation_preserves_length() {
        let path = rect(Point::ORIGIN, Point::new(10.0, 5.0)).expect("rect");
        let turned = transform_path(&path, &Mat3::rotate(0.7));
        assert!((path_length(&turned, FLAT) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn transform_polyline_moves_every_point() {
        let poly = Polyline::new(vec![Point::ORIGIN, Point::new(1.0, 1.0)], false);
        let moved = transform_polyline(&poly, &Mat3::translate(Vec2::new(1.0, 0.0)));
        assert!(moved.points[0].approx_eq(Point::new(1.0, 0.0), Tolerance::EQUALITY));
        assert!(!moved.closed);
    }
}
