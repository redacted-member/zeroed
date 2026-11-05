//! Splitting a component by closed vectors: the cells whose centres fall
//! inside the vectors go one way, the rest the other, so one component can
//! become two independent ones or lose the part inside the vectors. An open
//! vector splits too: its ends are carried on in a straight line past the
//! edges of the component, and the cells on its left make the inside.

use serde::Deserialize;

use crate::geom::{Path, Point, Region, SubPath, Tolerance};

use super::field::Field;
use super::shape::{inside_cells, Grid};

/// What Split Component makes of the two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitMode {
    /// Two components: the part inside the vectors and the part outside.
    #[default]
    Split,
    /// One component: the part inside the vectors is removed.
    ClearInside,
    /// One component: the part outside the vectors is removed.
    ClearOutside,
}

/// Which cells of `field` have their centre inside `region`.
pub fn inside_mask(field: &Field, region: &Region, tol: Tolerance) -> Vec<bool> {
    let grid = Grid {
        width: field.width,
        height: field.height,
        cell: field.cell,
        origin: field.origin,
    };
    inside_cells(region, &grid, tol)
}

/// The closed outline that stands for an open vector through `points` (its
/// flattened nodes, in millimetres) when splitting `field`: the vector with
/// each end carried on along its last direction to a circle round the
/// whole field and the vector, then closed along that circle
/// anticlockwise. The cells it holds are the ones to the left of the vector
/// as it runs from its start to its end. `None` when the vector has no
/// length.
pub fn open_cut(field: &Field, points: &[Point]) -> Option<Path> {
    let first = *points.first()?;
    let last = *points.last()?;
    let away = |from: Point, to: Point| {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = dx.hypot(dy);
        (length > 1e-9).then(|| (dx / length, dy / length))
    };
    // The first direction off each end, skipping repeated nodes.
    let start_dir = points.iter().skip(1).find_map(|p| away(*p, first))?;
    let end_dir = points.iter().rev().skip(1).find_map(|p| away(*p, last))?;
    let half = field.cell * 0.5;
    let low = Point::new(field.origin.x - half, field.origin.y - half);
    let high = Point::new(
        field.origin.x + field.cell * field.width as f64 - half,
        field.origin.y + field.cell * field.height as f64 - half,
    );
    let centre = Point::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5);
    let reach = points
        .iter()
        .chain([&low, &high])
        .map(|p| (p.x - centre.x).hypot(p.y - centre.y))
        .fold(0.0, f64::max);
    let radius = reach * 2.0 + field.cell;
    // Where the ray from `from` along `dir` leaves the circle.
    let exit = |from: Point, (dx, dy): (f64, f64)| {
        let (fx, fy) = (from.x - centre.x, from.y - centre.y);
        let b = fx * dx + fy * dy;
        let c = fx * fx + fy * fy - radius * radius;
        let t = -b + (b * b - c).max(0.0).sqrt();
        Point::new(from.x + dx * t, from.y + dy * t)
    };
    let head = exit(first, start_dir);
    let tail = exit(last, end_dir);
    let mut sub = SubPath::open(head);
    for p in points {
        sub.line_to(*p);
    }
    sub.line_to(tail);
    // Round the circle anticlockwise from the tail back to the head.
    let angle = |p: Point| (p.y - centre.y).atan2(p.x - centre.x);
    let from = angle(tail);
    let mut sweep = angle(head) - from;
    while sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    let steps = ((sweep / std::f64::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    for k in 1..steps {
        let a = from + sweep * k as f64 / steps as f64;
        sub.line_to(Point::new(
            centre.x + radius * a.cos(),
            centre.y + radius * a.sin(),
        ));
    }
    sub.line_to(head);
    sub.closed = true;
    Some(sub.into_path())
}

/// The part of `field` inside the mask and the part outside it, each
/// cropped to its own cells, `None` for a part with no cells.
pub fn split(field: &Field, inside: &[bool]) -> (Option<Field>, Option<Field>) {
    let mut ins = field.clone();
    let mut out = field.clone();
    for (index, &is_inside) in inside.iter().enumerate().take(field.z.len()) {
        if is_inside {
            out.z[index] = f32::NAN;
        } else {
            ins.z[index] = f32::NAN;
        }
    }
    // A mask shorter than the field leaves the rest outside.
    for index in inside.len()..field.z.len() {
        ins.z[index] = f32::NAN;
    }
    (ins.cropped(), out.cropped())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{rect, Point};

    fn slab() -> Field {
        let mut field = Field::empty(10, 10, 1.0, Point::new(0.5, 0.5));
        field.z.fill(2.0);
        field
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    #[test]
    fn the_vectors_divide_the_cells() {
        let field = slab();
        let square = rect(Point::new(0.0, 0.0), Point::new(4.0, 10.0)).expect("rect");
        let region = Region::from_paths(&[square], tol());
        let mask = inside_mask(&field, &region, tol());
        assert_eq!(mask.iter().filter(|m| **m).count(), 40);
        let (inside, outside) = split(&field, &mask);
        let inside = inside.expect("inside part");
        let outside = outside.expect("outside part");
        assert_eq!((inside.width, inside.height), (4, 10));
        assert_eq!((outside.width, outside.height), (6, 10));
        assert_eq!(outside.origin, Point::new(4.5, 0.5));
        assert_eq!(inside.at(Point::new(1.5, 1.5)), 2.0);
        assert!(outside.at(Point::new(1.5, 1.5)).is_nan());
    }

    #[test]
    fn vectors_round_the_whole_component_leave_nothing_outside() {
        let field = slab();
        let square = rect(Point::new(-1.0, -1.0), Point::new(20.0, 20.0)).expect("rect");
        let region = Region::from_paths(&[square], tol());
        let (inside, outside) = split(&field, &inside_mask(&field, &region, tol()));
        assert!(inside.is_some());
        assert!(outside.is_none());
    }

    #[test]
    fn an_open_vector_splits_along_its_extended_line() {
        let field = slab();
        // A short stroke up the middle, well inside the component: carried
        // on past both edges, the left half (x < 5) is the inside.
        let points = [Point::new(5.0, 4.0), Point::new(5.0, 6.0)];
        let path = open_cut(&field, &points).expect("cut");
        let region = Region::from_paths(&[path], tol());
        let mask = inside_mask(&field, &region, tol());
        assert_eq!(mask.iter().filter(|m| **m).count(), 50);
        let (inside, outside) = split(&field, &mask);
        assert_eq!(inside.expect("left").width, 5);
        assert_eq!(outside.expect("right").origin, Point::new(5.5, 0.5));
        // A bent stroke: along y = 3 then up to the top right.
        let bent = [
            Point::new(0.0, 3.0),
            Point::new(6.0, 3.0),
            Point::new(9.0, 9.0),
        ];
        let region = Region::from_paths(&[open_cut(&field, &bent).expect("bent")], tol());
        let mask = inside_mask(&field, &region, tol());
        let at = |x: usize, y: usize| mask[y * 10 + x];
        assert!(at(1, 8), "above the line is on the left");
        assert!(!at(1, 1), "below the line is on the right");
        assert!(!at(9, 5), "right of the rising part");
        assert!(open_cut(&field, &[Point::new(1.0, 1.0), Point::new(1.0, 1.0)]).is_none());
    }

    #[test]
    fn empty_cells_stay_empty_on_both_sides() {
        let mut field = slab();
        field.z[0] = f32::NAN;
        let mask = vec![true; 100];
        let (inside, outside) = split(&field, &mask);
        let inside = inside.expect("inside");
        assert!(inside.get(0, 0).is_nan());
        assert!(outside.is_none());
    }
}
