//! Turning a sketched pointer trail into a smooth vector.

use super::flatten::fit_cubics;
use super::path::SubPath;
use super::point::{Point, Tolerance};
use crate::error::CoreError;

/// Smooths a pointer trail and fits cubic curves through it.
///
/// `smoothing` is how many averaging passes run over the points before the
/// fit (0 keeps the trail as drawn); each pass pulls every point towards its
/// neighbours. `tol` is how far, in millimetres, the fitted curve may stray
/// from the smoothed points. A `closed` sketch joins its end back to its
/// start. The first and last points of an open sketch never move.
///
/// # Errors
/// [`CoreError::Geometry`] when the trail has fewer than two distinct points.
pub fn freehand_subpath(
    points: &[Point],
    smoothing: usize,
    tol: Tolerance,
    closed: bool,
) -> Result<SubPath, CoreError> {
    let mut pts: Vec<Point> = Vec::with_capacity(points.len() + 1);
    for p in points {
        if pts
            .last()
            .is_none_or(|last: &Point| last.distance(*p) > 1e-9)
        {
            pts.push(*p);
        }
    }
    let closed = closed && pts.len() >= 3;
    if closed {
        // The loop closes on its first point, so a last point sitting on it
        // is dropped and the wrap is smoothed like any other corner.
        while pts.len() > 3 && pts.last().is_some_and(|l| l.distance(pts[0]) < 1e-9) {
            pts.pop();
        }
    }
    if pts.len() < 2 {
        return Err(CoreError::geometry(
            "a freehand sketch needs the pointer to move while the button is down",
        ));
    }
    for _ in 0..smoothing {
        pts = smooth_pass(&pts, closed);
    }
    if closed {
        pts.push(pts[0]);
    }
    let mut sub = fit_cubics(&pts, tol)?;
    if closed {
        sub.closed = true;
    }
    Ok(sub)
}

/// One pass of a 1-2-1 average. Open trails keep their ends.
fn smooth_pass(pts: &[Point], closed: bool) -> Vec<Point> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            let (prev, next) = if closed {
                (pts[(i + n - 1) % n], pts[(i + 1) % n])
            } else if i == 0 || i == n - 1 {
                return pts[i];
            } else {
                (pts[i - 1], pts[i + 1])
            };
            let p = pts[i];
            Point::new(
                (prev.x + 2.0 * p.x + next.x) / 4.0,
                (prev.y + 2.0 * p.y + next.y) / 4.0,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::nearest_point_on_path;
    use crate::geom::path::Path;

    fn tol(v: f64) -> Tolerance {
        Tolerance::new(v).expect("tolerance")
    }

    fn wobbly_line() -> Vec<Point> {
        (0..=100)
            .map(|i| {
                let x = f64::from(i);
                let jitter = if i % 2 == 0 { 0.2 } else { -0.2 };
                Point::new(x, if i == 0 || i == 100 { 0.0 } else { jitter })
            })
            .collect()
    }

    #[test]
    fn a_sketch_becomes_a_few_curves_that_follow_it() {
        let pts: Vec<Point> = (0..=200)
            .map(|i| {
                let a = f64::from(i) / 200.0 * std::f64::consts::PI;
                Point::new(50.0 * a.cos(), 50.0 * a.sin())
            })
            .collect();
        let sub = freehand_subpath(&pts, 0, tol(0.05), false).expect("fit");
        assert!(sub.segments.len() < 20, "{} segments", sub.segments.len());
        assert!(sub.start.distance(pts[0]) < 1e-9);
        assert!(sub.end().distance(pts[200]) < 1e-9);
        let path = Path::from_subpath(sub);
        for p in &pts {
            let near = nearest_point_on_path(&path, *p, Tolerance::default()).expect("near");
            assert!(near.distance < 0.1, "{}", near.distance);
        }
    }

    #[test]
    fn smoothing_irons_out_the_jitter_and_keeps_the_ends() {
        let pts = wobbly_line();
        let rough = freehand_subpath(&pts, 0, tol(0.01), false).expect("fit");
        let smooth = freehand_subpath(&pts, 8, tol(0.05), false).expect("fit");
        assert!(smooth.segments.len() < rough.segments.len());
        assert!(smooth.start.distance(Point::new(0.0, 0.0)) < 1e-9);
        assert!(smooth.end().distance(Point::new(100.0, 0.0)) < 1e-9);
        let path = Path::from_subpath(smooth);
        let mid = nearest_point_on_path(&path, Point::new(50.0, 0.0), Tolerance::default())
            .expect("near");
        assert!(mid.distance < 0.1, "{}", mid.distance);
    }

    #[test]
    fn a_closed_sketch_ends_on_its_start() {
        let mut pts: Vec<Point> = (0..60)
            .map(|i| {
                let a = f64::from(i) / 60.0 * std::f64::consts::TAU;
                Point::new(20.0 * a.cos(), 20.0 * a.sin())
            })
            .collect();
        pts.push(Point::new(20.0, 0.0));
        let sub = freehand_subpath(&pts, 2, tol(0.05), true).expect("fit");
        assert!(sub.closed);
        assert!(sub.end().distance(sub.start) < 1e-9);
    }

    #[test]
    fn a_click_without_movement_is_refused() {
        let p = Point::new(3.0, 4.0);
        assert!(freehand_subpath(&[p, p, p], 2, tol(0.1), false).is_err());
    }
}
