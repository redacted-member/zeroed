//! Boolean operations on sets of closed vectors, built on `i_overlay`.
//!
//! The shapes are flattened inside the tolerance, combined with the non zero
//! fill rule, and arcs are fitted back onto the result. Every result comes
//! back with its outer loop counter clockwise and its holes clockwise, as one
//! [`Path`] per resulting shape.

use i_overlay::core::fill_rule::FillRule as OverlayFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::clip::FloatClip;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::string::clip::ClipRule;

use crate::error::CoreError;

use super::flatten::{fit_arcs, flatten_subpath};
use super::path::{Path, Polyline, SubPath, Winding};
use super::point::{Point, Tolerance};
use super::region::wind_by_nesting;

type Contour = Vec<[f64; 2]>;

/// Flattens every closed subpath of the set into contours for the solver.
/// Each vector's loops are wound by nesting first (outer loops counter
/// clockwise, holes clockwise), so a vector drawn clockwise or a donut with
/// both loops drawn the same way keeps its holes under the non zero rule.
fn to_contours(paths: &[Path], tol: Tolerance) -> Vec<Contour> {
    let mut out = Vec::new();
    for path in paths {
        let closed: Vec<SubPath> = path
            .subpaths
            .iter()
            .filter(|s| s.closed && !s.segments.is_empty())
            .cloned()
            .collect();
        for sub in &wind_by_nesting(&closed, tol) {
            let poly = flatten_subpath(sub, tol);
            if poly.points.len() < 3 {
                continue;
            }
            out.push(poly.points.iter().map(|p| [p.x, p.y]).collect());
        }
    }
    out
}

/// Flattens every open subpath of the set into lines for the clipper.
fn to_open_lines(paths: &[Path], tol: Tolerance) -> Vec<Contour> {
    let mut out = Vec::new();
    for path in paths {
        for sub in &path.subpaths {
            if sub.closed {
                continue;
            }
            let poly = flatten_subpath(sub, tol);
            if poly.points.len() < 2 {
                continue;
            }
            out.push(poly.points.iter().map(|p| [p.x, p.y]).collect());
        }
    }
    out
}

fn contour_to_polyline(contour: &[[f64; 2]], winding: Winding) -> Option<Polyline> {
    if contour.len() < 3 {
        return None;
    }
    let mut poly = Polyline::new(
        contour.iter().map(|p| Point::new(p[0], p[1])).collect(),
        true,
    );
    poly.orient(winding);
    Some(poly)
}

/// Turns solver output back into paths, fitting arcs and fixing winding.
fn from_shapes(shapes: Vec<Vec<Contour>>, tol: Tolerance) -> Vec<Path> {
    let mut out = Vec::new();
    for shape in shapes {
        let mut path = Path::new();
        for (i, contour) in shape.iter().enumerate() {
            let want = if i == 0 { Winding::Ccw } else { Winding::Cw };
            let Some(poly) = contour_to_polyline(contour, want) else {
                continue;
            };
            if let Some(sub) = fit_arcs(&poly, tol) {
                path.push(sub);
            }
        }
        if !path.is_empty() {
            out.push(path);
        }
    }
    out
}

fn run(a: &[Path], b: &[Path], rule: OverlayRule, tol: Tolerance) -> Vec<Path> {
    let subject = to_contours(a, tol);
    let clip = to_contours(b, tol);
    if subject.is_empty() && clip.is_empty() {
        return Vec::new();
    }
    let shapes = subject.overlay(&clip, rule, OverlayFillRule::NonZero);
    from_shapes(shapes, tol)
}

fn require_closed(paths: &[Path], op: &str) -> Result<(), CoreError> {
    if paths.iter().all(|p| p.subpaths.iter().all(|s| !s.closed)) {
        return Err(CoreError::geometry(format!(
            "{op} needs at least one closed vector"
        )));
    }
    Ok(())
}

/// Union: merges overlapping shapes into single outlines.
///
/// Only closed subpaths take part: open subpaths in the input are left out
/// of the result.
pub fn weld(paths: &[Path], tol: Tolerance) -> Result<Vec<Path>, CoreError> {
    require_closed(paths, "weld")?;
    let (first, rest) = paths.split_at(1);
    Ok(run(first, rest, OverlayRule::Union, tol))
}

/// Difference: what is left of `from` once `cut` is taken away.
///
/// Only closed subpaths take part: open subpaths in either set are left out
/// of the result.
pub fn subtract(from: &[Path], cut: &[Path], tol: Tolerance) -> Result<Vec<Path>, CoreError> {
    require_closed(from, "subtract")?;
    Ok(run(from, cut, OverlayRule::Difference, tol))
}

/// Intersection: only the area both sets cover.
///
/// Only closed subpaths take part: open subpaths in either set are left out
/// of the result.
pub fn intersect_paths(a: &[Path], b: &[Path], tol: Tolerance) -> Result<Vec<Path>, CoreError> {
    require_closed(a, "intersect")?;
    require_closed(b, "intersect")?;
    Ok(run(a, b, OverlayRule::Intersect, tol))
}

/// Exclusive or: the area exactly one of the two sets covers.
///
/// Only closed subpaths take part: open subpaths in either set are left out
/// of the result.
pub fn xor(a: &[Path], b: &[Path], tol: Tolerance) -> Result<Vec<Path>, CoreError> {
    require_closed(a, "exclusive or")?;
    Ok(run(a, b, OverlayRule::Xor, tol))
}

/// Crops a set of vectors to a boundary.
///
/// Closed vectors are clipped to the boundary area. Open vectors are cut as
/// well, and the pieces that lie inside the boundary are kept.
pub fn crop(paths: &[Path], boundary: &[Path], tol: Tolerance) -> Result<Vec<Path>, CoreError> {
    require_closed(boundary, "crop")?;
    let mut out = run(paths, boundary, OverlayRule::Intersect, tol);

    let lines = to_open_lines(paths, tol);
    if !lines.is_empty() {
        let shape = to_contours(boundary, tol);
        let rule = ClipRule {
            invert: false,
            boundary_included: true,
        };
        let kept = lines.clip_by(&shape, OverlayFillRule::NonZero, rule);
        for line in kept {
            if line.len() < 2 {
                continue;
            }
            let poly = Polyline::new(line.iter().map(|p| Point::new(p[0], p[1])).collect(), false);
            if let Some(sub) = fit_arcs(&poly, tol) {
                out.push(sub.into_path());
            }
        }
    }
    Ok(out)
}

/// Trim Objects on one path: what lies inside the `boundary` (with
/// `clear_inside`) or outside it is removed. Closed loops are clipped as
/// areas and open ones as lines. With `refit`, arcs are fitted back onto
/// the pieces; otherwise they stay as the short lines the clipping made.
/// All pieces come back as one path, empty when nothing is left.
pub fn clip_to_boundary(
    path: &Path,
    boundary: &[Path],
    clear_inside: bool,
    refit: bool,
    tol: Tolerance,
) -> Path {
    let one = std::slice::from_ref(path);
    let shape = to_contours(boundary, tol);
    let mut out = Path::new();
    let subject = to_contours(one, tol);
    if !subject.is_empty() {
        let rule = if clear_inside {
            OverlayRule::Difference
        } else {
            OverlayRule::Intersect
        };
        for contours in subject.overlay(&shape, rule, OverlayFillRule::NonZero) {
            for (i, contour) in contours.iter().enumerate() {
                let want = if i == 0 { Winding::Ccw } else { Winding::Cw };
                if let Some(poly) = contour_to_polyline(contour, want) {
                    push_piece(&mut out, &poly, refit, tol);
                }
            }
        }
    }
    let lines = to_open_lines(one, tol);
    if !lines.is_empty() {
        let rule = ClipRule {
            invert: clear_inside,
            boundary_included: !clear_inside,
        };
        for line in lines.clip_by(&shape, OverlayFillRule::NonZero, rule) {
            if line.len() < 2 {
                continue;
            }
            let poly = Polyline::new(line.iter().map(|p| Point::new(p[0], p[1])).collect(), false);
            push_piece(&mut out, &poly, refit, tol);
        }
    }
    out
}

fn push_piece(out: &mut Path, poly: &Polyline, refit: bool, tol: Tolerance) {
    let sub = if refit {
        fit_arcs(poly, tol)
    } else {
        poly.to_subpath()
    };
    if let Some(sub) = sub {
        out.push(sub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::contain::{point_in_path, FillRule};
    use crate::geom::edit::reverse;
    use crate::geom::metrics::{path_area, path_bbox, path_length};
    use crate::geom::path::SubPath;

    const FLAT: Tolerance = Tolerance::FLATTEN;

    fn fine() -> Tolerance {
        Tolerance::new(0.002).expect("tolerance")
    }

    #[test]
    fn weld_of_two_overlapping_circles_gives_one_shape() {
        let a = circle(Point::new(-3.0, 0.0), 5.0).expect("circle");
        let b = circle(Point::new(3.0, 0.0), 5.0).expect("circle");
        let out = weld(&[a, b], fine()).expect("weld");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subpaths.len(), 1);
        let area = path_area(&out[0], FLAT);
        // Two discs of radius 5 with centres 6 apart, less the lens they share.
        let r: f64 = 5.0;
        let d: f64 = 6.0;
        let lens = 2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt();
        let expected = 2.0 * std::f64::consts::PI * r * r - lens;
        assert!((area - expected).abs() / expected < 0.01, "area {area}");
    }

    #[test]
    fn weld_of_separate_shapes_keeps_them_apart() {
        let a = circle(Point::new(-50.0, 0.0), 5.0).expect("circle");
        let b = circle(Point::new(50.0, 0.0), 5.0).expect("circle");
        let out = weld(&[a, b], fine()).expect("weld");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn weld_needs_a_closed_vector() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert!(weld(&[sub.into_path()], FLAT).is_err());
    }

    #[test]
    fn subtract_makes_a_hole() {
        let outer = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
        let inner = rect(Point::new(10.0, 10.0), Point::new(30.0, 30.0)).expect("rect");
        let out = subtract(&[outer], &[inner], FLAT).expect("subtract");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subpaths.len(), 2);
        assert_eq!(out[0].subpaths[0].nodes().len(), 4);
        let area = path_area(&out[0], FLAT);
        assert!((area - (1600.0 - 400.0)).abs() < 0.5, "area {area}");
        assert!(!point_in_path(
            &out[0],
            Point::new(20.0, 20.0),
            FillRule::NonZero,
            FLAT
        ));
        assert!(point_in_path(
            &out[0],
            Point::new(5.0, 5.0),
            FillRule::NonZero,
            FLAT
        ));
    }

    #[test]
    fn subtract_can_remove_everything() {
        let small = rect(Point::new(5.0, 5.0), Point::new(6.0, 6.0)).expect("rect");
        let big = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
        assert!(subtract(&[small], &[big], FLAT)
            .expect("subtract")
            .is_empty());
    }

    #[test]
    fn subtract_of_nothing_keeps_the_shape() {
        let square = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let out = subtract(&[square], &[], FLAT).expect("subtract");
        assert_eq!(out.len(), 1);
        assert!((path_area(&out[0], FLAT) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn intersect_keeps_only_the_common_part() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::new(5.0, 5.0), Point::new(20.0, 20.0)).expect("rect");
        let out = intersect_paths(&[a], &[b], FLAT).expect("intersect");
        assert_eq!(out.len(), 1);
        assert!((path_area(&out[0], FLAT) - 25.0).abs() < 1e-6);
        let bbox = path_bbox(&out[0]).expect("bbox");
        assert!((bbox.min.x - 5.0).abs() < 1e-9);
        assert!((bbox.max.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn intersect_of_disjoint_shapes_is_empty() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::new(50.0, 50.0), Point::new(60.0, 60.0)).expect("rect");
        assert!(intersect_paths(&[a], &[b], FLAT)
            .expect("intersect")
            .is_empty());
    }

    #[test]
    fn xor_leaves_the_parts_that_do_not_overlap() {
        let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let b = rect(Point::new(5.0, 0.0), Point::new(15.0, 10.0)).expect("rect");
        let out = xor(&[a], &[b], FLAT).expect("xor");
        let total: f64 = out.iter().map(|p| path_area(p, FLAT)).sum();
        assert!((total - 100.0).abs() < 1e-6, "total {total}");
    }

    #[test]
    fn crop_clips_a_closed_shape_to_the_boundary() {
        let shape = circle(Point::ORIGIN, 10.0).expect("circle");
        let boundary = rect(Point::ORIGIN, Point::new(20.0, 20.0)).expect("rect");
        let out = crop(&[shape], &[boundary], fine()).expect("crop");
        assert_eq!(out.len(), 1);
        let area = path_area(&out[0], FLAT);
        let expected = std::f64::consts::PI * 100.0 / 4.0;
        assert!((area - expected).abs() / expected < 0.01, "area {area}");
    }

    #[test]
    fn crop_keeps_the_inside_piece_of_an_open_vector() {
        let mut sub = SubPath::open(Point::new(-20.0, 5.0));
        sub.line_to(Point::new(20.0, 5.0));
        let boundary = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        let out = crop(&[sub.into_path()], &[boundary], FLAT).expect("crop");
        assert_eq!(out.len(), 1);
        assert!(!out[0].subpaths[0].closed);
        assert!((path_length(&out[0], FLAT) - 10.0).abs() < 1e-6);
        let bbox = path_bbox(&out[0]).expect("bbox");
        assert!(bbox.min.x >= -1e-6 && bbox.max.x <= 10.0 + 1e-6);
    }

    #[test]
    fn crop_drops_an_open_vector_that_misses() {
        let mut sub = SubPath::open(Point::new(-20.0, 50.0));
        sub.line_to(Point::new(20.0, 50.0));
        let boundary = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
        assert!(crop(&[sub.into_path()], &[boundary], FLAT)
            .expect("crop")
            .is_empty());
    }

    #[test]
    fn crop_needs_a_closed_boundary() {
        let shape = circle(Point::ORIGIN, 5.0).expect("circle");
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert!(crop(&[shape], &[sub.into_path()], FLAT).is_err());
    }

    #[test]
    fn a_shape_with_a_hole_survives_a_union() {
        let mut donut = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
        donut.extend(reverse(
            &rect(Point::new(10.0, 10.0), Point::new(30.0, 30.0)).expect("rect"),
        ));
        let other = rect(Point::new(35.0, 0.0), Point::new(60.0, 40.0)).expect("rect");
        let out = weld(&[donut, other], FLAT).expect("weld");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subpaths.len(), 2);
        let area = path_area(&out[0], FLAT);
        assert!(
            (area - (1600.0 + 25.0 * 40.0 - 400.0 - 5.0 * 40.0)).abs() < 1.0,
            "area {area}"
        );
    }

    #[test]
    fn results_are_wound_outer_counter_clockwise() {
        let outer = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
        let inner = circle(Point::new(20.0, 20.0), 8.0).expect("circle");
        let out = subtract(&[outer], &[inner], fine()).expect("subtract");
        assert_eq!(out[0].subpaths.len(), 2);
        assert!(crate::geom::metrics::subpath_signed_area(&out[0].subpaths[0]) > 0.0);
        assert!(crate::geom::metrics::subpath_signed_area(&out[0].subpaths[1]) < 0.0);
    }

    #[test]
    fn boolean_results_are_deterministic() {
        let a = circle(Point::new(-3.0, 0.0), 5.0).expect("circle");
        let b = circle(Point::new(3.0, 0.0), 5.0).expect("circle");
        let first = weld(&[a.clone(), b.clone()], fine()).expect("weld");
        let second = weld(&[a, b], fine()).expect("weld");
        assert_eq!(first, second);
    }
}
