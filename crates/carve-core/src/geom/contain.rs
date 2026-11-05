//! Containment tests and the checks behind the vector validator.

use serde::{Deserialize, Serialize};

use super::flatten::flatten_subpath;
use super::intersect::{path_intersections, subpath_self_intersects};
use super::metrics::{nearest_point_on_path, path_bbox, path_length, segment_length};
use super::path::{Path, Polyline, SubPath};
use super::point::{Point, Tolerance};

/// How a filled area decides which side of a boundary is inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillRule {
    /// A point is inside when a ray from it crosses the boundary an odd
    /// number of times.
    EvenOdd,
    /// A point is inside when the signed crossings do not cancel out.
    NonZero,
}

/// What the validator found wrong with a vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationKind {
    /// The vector is not closed, so it cannot be machined as an area.
    Open,
    /// The vector crosses itself.
    SelfIntersecting,
    /// A segment has no length.
    ZeroLengthSegment,
    /// Two nodes sit on top of each other.
    DuplicateNode,
    /// Another vector in the set lies on top of this one.
    OverlappingDuplicate,
}

/// One problem found by [`validate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// What is wrong.
    pub kind: ValidationKind,
    /// Index of the path in the set that was checked.
    pub path_index: usize,
    /// Index of the subpath inside that path.
    pub subpath_index: usize,
    /// Index of the segment, when the problem is on one segment.
    pub segment_index: Option<usize>,
    /// Where the problem is, in millimetres, when there is one place.
    pub point: Option<Point>,
    /// A sentence describing the problem, for the report list.
    pub message: String,
    /// For a duplicate, the index of the vector it lies on top of.
    pub other_index: Option<usize>,
}

fn crossings(polyline: &Polyline, p: Point) -> (i32, i32) {
    let n = polyline.points.len();
    if n < 3 {
        return (0, 0);
    }
    let mut parity = 0;
    let mut winding = 0;
    for i in 0..n {
        let a = polyline.points[i];
        let b = polyline.points[(i + 1) % n];
        if (a.y > p.y) == (b.y > p.y) {
            continue;
        }
        let t = (p.y - a.y) / (b.y - a.y);
        let x = a.x + t * (b.x - a.x);
        if x > p.x {
            parity += 1;
            winding += if b.y > a.y { 1 } else { -1 };
        }
    }
    (parity, winding)
}

/// True when the point lies within `band` millimetres of any segment of the
/// path (open or closed): the tolerance band hit tests use for points on a
/// boundary, where [`point_in_path`] may answer either way.
pub fn point_on_path(path: &Path, p: Point, band: f64, tol: Tolerance) -> bool {
    nearest_point_on_path(path, p, tol).is_some_and(|near| near.distance <= band)
}

/// True when the point lies inside the closed subpaths of the path.
///
/// Open subpaths are ignored. A point exactly on the boundary may come out
/// either way, which is why callers that care use a tolerance band.
pub fn point_in_path(path: &Path, p: Point, rule: FillRule, tol: Tolerance) -> bool {
    let mut parity = 0;
    let mut winding = 0;
    for sub in &path.subpaths {
        if !sub.closed {
            continue;
        }
        let poly = flatten_subpath(sub, tol);
        let (pa, wi) = crossings(&poly, p);
        parity += pa;
        winding += wi;
    }
    match rule {
        FillRule::EvenOdd => parity % 2 == 1,
        FillRule::NonZero => winding != 0,
    }
}

/// True when `inner` lies entirely inside `outer` and the two do not cross.
pub fn path_in_path(inner: &Path, outer: &Path, tol: Tolerance) -> bool {
    if inner.subpaths.is_empty() || outer.subpaths.is_empty() {
        return false;
    }
    if !path_intersections(inner, outer, tol).is_empty() {
        return false;
    }
    inner.subpaths.iter().all(|sub| {
        let poly = flatten_subpath(sub, tol);
        poly.points
            .iter()
            .all(|p| point_in_path(outer, *p, FillRule::NonZero, tol))
    })
}

/// Checks one vector and lists what is wrong with it.
pub fn validate(path: &Path, tol: Tolerance) -> Vec<ValidationIssue> {
    validate_indexed(path, 0, tol)
}

fn validate_indexed(path: &Path, path_index: usize, tol: Tolerance) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (si, sub) in path.subpaths.iter().enumerate() {
        if !sub.closed {
            issues.push(ValidationIssue {
                kind: ValidationKind::Open,
                path_index,
                subpath_index: si,
                segment_index: None,
                point: Some(sub.end()),
                message: format!(
                    "the vector is open, with a {:.3} mm gap between its ends",
                    sub.end().distance(sub.start)
                ),
                other_index: None,
            });
        }
        for (gi, (start, seg)) in sub.iter().enumerate() {
            if segment_length(start, seg, tol) <= tol.get() {
                issues.push(ValidationIssue {
                    kind: ValidationKind::ZeroLengthSegment,
                    path_index,
                    subpath_index: si,
                    segment_index: Some(gi),
                    point: Some(start),
                    message: "a segment is shorter than the tolerance".to_string(),
                    other_index: None,
                });
            }
        }
        let nodes = sub.nodes();
        for (ni, node) in nodes.iter().enumerate() {
            let next = nodes[(ni + 1) % nodes.len()];
            if nodes.len() > 1 && node.approx_eq(next, tol) {
                issues.push(ValidationIssue {
                    kind: ValidationKind::DuplicateNode,
                    path_index,
                    subpath_index: si,
                    segment_index: Some(ni),
                    point: Some(*node),
                    message: "two nodes sit on the same place".to_string(),
                    other_index: None,
                });
            }
        }
        if subpath_self_intersects(sub, tol) {
            issues.push(ValidationIssue {
                kind: ValidationKind::SelfIntersecting,
                path_index,
                subpath_index: si,
                segment_index: None,
                point: None,
                message: "the vector crosses itself".to_string(),
                other_index: None,
            });
        }
    }
    issues
}

/// Checks a whole set of vectors, adding the duplicate check that only makes
/// sense across vectors.
pub fn validate_set(paths: &[Path], tol: Tolerance) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        issues.extend(validate_indexed(path, i, tol));
    }
    for i in 0..paths.len() {
        for j in (i + 1)..paths.len() {
            if lies_on_top_of(&paths[i], &paths[j], tol) {
                issues.push(ValidationIssue {
                    kind: ValidationKind::OverlappingDuplicate,
                    path_index: j,
                    subpath_index: 0,
                    segment_index: None,
                    point: path_bbox(&paths[j]).map(|b| b.centre()),
                    message: "this vector lies on top of another".to_string(),
                    other_index: Some(i),
                });
            }
        }
    }
    issues
}

/// True when two vectors cover the same outline inside the tolerance.
fn lies_on_top_of(a: &Path, b: &Path, tol: Tolerance) -> bool {
    let (Some(ba), Some(bb)) = (path_bbox(a), path_bbox(b)) else {
        return false;
    };
    if !ba.min.approx_eq(bb.min, tol) || !ba.max.approx_eq(bb.max, tol) {
        return false;
    }
    if (path_length(a, tol) - path_length(b, tol)).abs() > tol.get() {
        return false;
    }
    let pa: Vec<Point> = a
        .subpaths
        .iter()
        .flat_map(|s| flatten_subpath(s, tol).points)
        .collect();
    let pb: Vec<Point> = b
        .subpaths
        .iter()
        .flat_map(|s| flatten_subpath(s, tol).points)
        .collect();
    if pa.is_empty() || pb.is_empty() {
        return false;
    }
    pa.iter().all(|p| pb.iter().any(|q| p.approx_eq(*q, tol)))
        && pb.iter().all(|q| pa.iter().any(|p| q.approx_eq(*p, tol)))
}

/// True when the subpath encloses an area, which the area strategies need.
pub fn encloses_area(subpath: &SubPath, tol: Tolerance) -> bool {
    subpath.closed && super::metrics::subpath_signed_area(subpath).abs() > tol.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_on_a_boundary_is_on_the_path_within_the_band() {
        let square =
            crate::geom::construct::rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let tol = Tolerance::FLATTEN;
        assert!(point_on_path(&square, Point::new(10.0, 10.0), 0.01, tol));
        assert!(point_on_path(&square, Point::new(5.0, 0.005), 0.01, tol));
        assert!(!point_on_path(&square, Point::new(5.0, 5.0), 0.01, tol));
    }
    use crate::geom::construct::{circle, rect};
    use crate::geom::edit::reverse;

    const FLAT: Tolerance = Tolerance::FLATTEN;

    fn donut() -> Path {
        let mut path = circle(Point::ORIGIN, 10.0).expect("circle");
        path.extend(reverse(&circle(Point::ORIGIN, 5.0).expect("circle")));
        path
    }

    #[test]
    fn a_point_inside_a_square_is_inside() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(point_in_path(
            &square,
            Point::new(5.0, 5.0),
            FillRule::NonZero,
            FLAT
        ));
        assert!(point_in_path(
            &square,
            Point::new(5.0, 5.0),
            FillRule::EvenOdd,
            FLAT
        ));
    }

    #[test]
    fn a_point_outside_a_square_is_outside() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(!point_in_path(
            &square,
            Point::new(15.0, 5.0),
            FillRule::NonZero,
            FLAT
        ));
        assert!(!point_in_path(
            &square,
            Point::new(-1.0, 5.0),
            FillRule::EvenOdd,
            FLAT
        ));
    }

    #[test]
    fn a_hole_reads_as_outside_under_both_rules() {
        let path = donut();
        assert!(!point_in_path(
            &path,
            Point::ORIGIN,
            FillRule::NonZero,
            FLAT
        ));
        assert!(!point_in_path(
            &path,
            Point::ORIGIN,
            FillRule::EvenOdd,
            FLAT
        ));
        assert!(point_in_path(
            &path,
            Point::new(7.5, 0.0),
            FillRule::NonZero,
            FLAT
        ));
    }

    #[test]
    fn same_direction_nested_loops_differ_between_the_rules() {
        let mut path = circle(Point::ORIGIN, 10.0).expect("circle");
        path.extend(circle(Point::ORIGIN, 5.0).expect("circle"));
        assert!(point_in_path(&path, Point::ORIGIN, FillRule::NonZero, FLAT));
        assert!(!point_in_path(
            &path,
            Point::ORIGIN,
            FillRule::EvenOdd,
            FLAT
        ));
    }

    #[test]
    fn open_subpaths_do_not_take_part() {
        let mut sub = SubPath::open(Point::new(-5.0, 0.0));
        sub.line_to(Point::new(5.0, 0.0));
        assert!(!point_in_path(
            &sub.into_path(),
            Point::ORIGIN,
            FillRule::NonZero,
            FLAT
        ));
    }

    #[test]
    fn a_small_circle_is_inside_a_big_one() {
        let big = circle(Point::ORIGIN, 10.0).expect("circle");
        let small = circle(Point::ORIGIN, 4.0).expect("circle");
        assert!(path_in_path(&small, &big, FLAT));
        assert!(!path_in_path(&big, &small, FLAT));
    }

    #[test]
    fn crossing_vectors_are_not_contained() {
        let a = circle(Point::ORIGIN, 10.0).expect("circle");
        let b = circle(Point::new(8.0, 0.0), 5.0).expect("circle");
        assert!(!path_in_path(&b, &a, FLAT));
        assert!(!path_in_path(&Path::new(), &a, FLAT));
    }

    #[test]
    fn a_clean_square_has_no_issues() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(validate(&square, FLAT).is_empty());
    }

    #[test]
    fn an_open_vector_is_reported() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let issues = validate(&sub.into_path(), FLAT);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, ValidationKind::Open);
    }

    #[test]
    fn a_zero_length_segment_is_reported() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .line_to(Point::ORIGIN);
        sub.closed = true;
        let issues = validate(&sub.into_path(), FLAT);
        assert!(issues
            .iter()
            .any(|i| i.kind == ValidationKind::ZeroLengthSegment));
        assert!(issues
            .iter()
            .any(|i| i.kind == ValidationKind::DuplicateNode));
    }

    #[test]
    fn a_bowtie_is_reported_as_self_intersecting() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 10.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(0.0, 10.0))
            .line_to(Point::new(0.0, 0.0));
        sub.closed = true;
        let issues = validate(&sub.into_path(), FLAT);
        assert!(issues
            .iter()
            .any(|i| i.kind == ValidationKind::SelfIntersecting));
    }

    #[test]
    fn duplicate_vectors_are_reported_once() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let issues = validate_set(&[a, b], FLAT);
        let dupes = issues
            .iter()
            .filter(|i| i.kind == ValidationKind::OverlappingDuplicate)
            .count();
        assert_eq!(dupes, 1);
    }

    #[test]
    fn different_vectors_are_not_duplicates() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::ORIGIN, Point::new(10.0, 9.0)).expect("rect");
        let issues = validate_set(&[a, b], FLAT);
        assert!(issues.is_empty());
        assert!(validate_set(&[], FLAT).is_empty());
    }

    #[test]
    fn encloses_area_rejects_open_and_flat_vectors() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(encloses_area(&square.subpaths[0], FLAT));
        let mut line = SubPath::open(Point::ORIGIN);
        line.line_to(Point::new(1.0, 0.0));
        assert!(!encloses_area(&line, FLAT));
    }
}
