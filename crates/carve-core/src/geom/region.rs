//! Regions: areas bounded by closed loops, with holes. A region's outer
//! loops run counter clockwise and its holes clockwise, whatever direction
//! the vectors it was built from were drawn in. Toolpaths use regions to
//! offset a whole shape at once, so that a letter's counter or a pocket's
//! island moves together with the outline around it.

use cavalier_contours::polyline::PlineSource;
use cavalier_contours::shape_algorithms::{Shape, ShapeOffsetOptions};

use super::contain::{point_in_path, FillRule};
use super::edit::reverse_subpath;
use super::intersect::path_intersections;
use super::metrics::subpath_bbox;
use super::metrics::subpath_signed_area;
use super::offset::{from_pline, to_pline};
use super::path::{Path, SubPath};
use super::point::{Point, Tolerance};

/// An area made of outer loops (counter clockwise) and holes (clockwise).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Region {
    /// Every loop, outer loops and holes alike.
    pub loops: Vec<SubPath>,
}

impl Region {
    /// Builds a region from the closed subpaths of `paths`. Nesting decides
    /// what is a hole: a loop inside an odd number of other loops is one,
    /// the even-odd rule a drawing's text and cut-outs follow. Open subpaths
    /// are left out.
    pub fn from_paths(paths: &[Path], tol: Tolerance) -> Region {
        let closed: Vec<SubPath> = paths
            .iter()
            .flat_map(|p| p.subpaths.iter())
            .filter(|s| s.closed && !s.segments.is_empty())
            .filter(|s| subpath_signed_area(s).abs() > tol.get() * tol.get())
            .cloned()
            .collect();
        Region {
            loops: wind_by_nesting(&closed, tol),
        }
    }

    /// True when the region has no area.
    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    /// Outer loops, counter clockwise.
    pub fn outers(&self) -> impl Iterator<Item = &SubPath> {
        self.loops.iter().filter(|s| subpath_signed_area(s) > 0.0)
    }

    /// Holes, clockwise.
    pub fn holes(&self) -> impl Iterator<Item = &SubPath> {
        self.loops.iter().filter(|s| subpath_signed_area(s) < 0.0)
    }

    /// Net area in square millimetres: outer loops less holes.
    pub fn area(&self) -> f64 {
        self.loops.iter().map(subpath_signed_area).sum()
    }

    /// True when `p` is inside the region.
    pub fn contains(&self, p: Point, tol: Tolerance) -> bool {
        point_in_path(&self.as_path(), p, FillRule::EvenOdd, tol)
    }

    /// Every loop as one path.
    pub fn as_path(&self) -> Path {
        Path {
            subpaths: self.loops.clone(),
        }
    }

    /// The region grown or shrunk by `delta` millimetres: positive shrinks
    /// (outer loops move in, holes grow), negative grows. Loops that
    /// collapse disappear and loops that meet merge, as a tool of radius
    /// `delta` would see them.
    pub fn offset(&self, delta: f64, tol: Tolerance) -> Region {
        if self.loops.is_empty() || delta.abs() < 1e-12 {
            return self.clone();
        }
        let plines = self.loops.iter().map(|s| to_pline(s, tol));
        let shape = Shape::from_plines(plines);
        let result = shape.parallel_offset(delta, &ShapeOffsetOptions::default());
        let loops = result
            .ccw_plines
            .iter()
            .chain(result.cw_plines.iter())
            .filter(|p| p.polyline.vertex_count() > 1)
            .filter_map(|p| from_pline(&p.polyline))
            .filter(|s| subpath_signed_area(s).abs() > tol.get() * tol.get())
            .collect();
        Region { loops }
    }
}

/// The loops wound by nesting: one inside an odd number of the others
/// runs clockwise, the rest counter clockwise. A loop is inside another
/// only when it lies wholly within it: loops that cross (two overlapping
/// shapes in one vector) are both outer loops. A loop with no area to speak
/// of (a figure of eight) is kept as drawn.
pub(crate) fn wind_by_nesting(closed: &[SubPath], tol: Tolerance) -> Vec<SubPath> {
    let areas: Vec<f64> = closed
        .iter()
        .map(|s| subpath_signed_area(s).abs())
        .collect();
    let alone: Vec<Path> = closed.iter().map(|s| s.clone().into_path()).collect();
    let boxes: Vec<_> = closed.iter().map(subpath_bbox).collect();
    let mut loops = Vec::with_capacity(closed.len());
    for (i, sub) in closed.iter().enumerate() {
        if areas[i] <= tol.get() * tol.get() {
            loops.push(sub.clone());
            continue;
        }
        let probe = sub.start;
        let depth = alone
            .iter()
            .enumerate()
            .filter(|(j, other)| {
                *j != i
                    && areas[*j] > areas[i]
                    && boxes[i]
                        .zip(boxes[*j])
                        .is_some_and(|(a, b)| b.intersects(a))
                    && point_in_path(other, probe, FillRule::EvenOdd, tol)
                    && path_intersections(&alone[i], other, tol).is_empty()
            })
            .count();
        let hole = depth % 2 == 1;
        let ccw = subpath_signed_area(sub) > 0.0;
        loops.push(if ccw == hole {
            reverse_subpath(sub)
        } else {
            sub.clone()
        });
    }
    loops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, rect};

    const TOL: f64 = 1e-3;

    fn tol() -> Tolerance {
        Tolerance::new(TOL).expect("tolerance")
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
        rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect")
    }

    #[test]
    fn a_loop_inside_another_is_a_hole_whatever_way_it_was_drawn() {
        let outer = square(0.0, 0.0, 100.0, 100.0);
        let inner = square(40.0, 40.0, 60.0, 60.0);
        let region = Region::from_paths(&[outer, inner], tol());
        assert_eq!(region.outers().count(), 1);
        assert_eq!(region.holes().count(), 1);
        assert!((region.area() - (10_000.0 - 400.0)).abs() < 1e-6);
        assert!(!region.contains(Point::new(50.0, 50.0), tol()));
        assert!(region.contains(Point::new(10.0, 10.0), tol()));
    }

    #[test]
    fn shrinking_moves_outer_loops_in_and_holes_out() {
        let region = Region::from_paths(
            &[
                square(0.0, 0.0, 100.0, 100.0),
                square(40.0, 40.0, 60.0, 60.0),
            ],
            tol(),
        );
        let shrunk = region.offset(5.0, tol());
        let outer = shrunk
            .outers()
            .next()
            .and_then(crate::geom::subpath_bbox)
            .expect("outer");
        assert!((outer.min.x - 5.0).abs() < 1e-6 && (outer.max.x - 95.0).abs() < 1e-6);
        let hole = shrunk
            .holes()
            .next()
            .and_then(crate::geom::subpath_bbox)
            .expect("hole");
        assert!((hole.min.x - 35.0).abs() < 1e-6 && (hole.max.x - 65.0).abs() < 1e-6);
    }

    #[test]
    fn a_region_vanishes_when_shrunk_past_its_middle() {
        let region = Region::from_paths(&[square(0.0, 0.0, 10.0, 10.0)], tol());
        assert!(region.offset(6.0, tol()).is_empty());
    }

    #[test]
    fn loops_that_grow_into_each_other_merge() {
        let region = Region::from_paths(
            &[square(0.0, 0.0, 10.0, 10.0), square(12.0, 0.0, 22.0, 10.0)],
            tol(),
        );
        assert_eq!(region.outers().count(), 2);
        assert_eq!(region.offset(-2.0, tol()).outers().count(), 1);
    }

    #[test]
    fn circles_stay_arcs_when_offset() {
        let c = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let grown = Region::from_paths(&[c], tol()).offset(-3.0, tol());
        let area = grown.area();
        let want = std::f64::consts::PI * 13.0 * 13.0;
        assert!((area - want).abs() < 1e-6, "{area} vs {want}");
    }
}
