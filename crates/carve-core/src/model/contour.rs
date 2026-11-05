//! Boundaries of relief components: closed vectors round the cells of a
//! heightfield that stand above a level, traced by marching squares on the
//! cell centres, as Create Vector Boundary makes them.

use crate::error::CoreError;
use crate::geom::metrics::subpath_signed_area;
use crate::geom::path::Polyline;
use crate::geom::{simplify_polyline, Path, Point, Region, SubPath, Tolerance};

use super::field::Field;

/// No crossing leaves this edge.
const NONE: usize = usize::MAX;

/// Closed vectors round the cells whose height is above `level` (in
/// millimetres; empty cells count as below it).
///
/// The contour runs through the cell centres' grid: between a cell above
/// the level and one below, it crosses where the heights interpolate to
/// `level`, and halfway when one side is empty, so a block of cells on an
/// empty field is outlined along its cell edges. Where the four cells round
/// a grid square alternate (a saddle), their mean height decides: above the
/// level the two high cells join, otherwise they stay apart. Loops are
/// simplified to about a quarter of a cell, then grown outward by `offset`
/// millimetres when it is positive. Each loop is its own path; outer loops
/// run counter clockwise and holes clockwise. `tol` is the geometry
/// tolerance in millimetres used to close loops and to offset them.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the field's cell size is not a
/// positive number, its heights do not match its size, or `offset` is not
/// finite.
pub fn boundary(
    field: &Field,
    level: f32,
    offset: f64,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    if !field.cell.is_finite() || field.cell <= 0.0 {
        return Err(CoreError::validation(
            "a relief's cell size must be a positive number of millimeters",
        ));
    }
    if field.z.len() != field.width * field.height {
        return Err(CoreError::validation(
            "a relief's heights do not match its size",
        ));
    }
    if !offset.is_finite() {
        return Err(CoreError::validation(
            "the boundary offset must be a number of millimeters",
        ));
    }
    let tracer = Tracer { field, level };
    let loops = tracer.loops();
    let simplify_tol = Tolerance::new(field.cell * 0.25)?;
    let mut subpaths = Vec::with_capacity(loops.len());
    for points in loops {
        let simple = simplify_polyline(&Polyline::new(points, true), simplify_tol);
        if simple.points.len() < 3 {
            continue;
        }
        let mut sub = SubPath::open(simple.points[0]);
        for p in &simple.points[1..] {
            sub.line_to(*p);
        }
        sub.close_with_line(tol);
        if subpath_signed_area(&sub).abs() > tol.get() * tol.get() {
            subpaths.push(sub);
        }
    }
    if offset > 0.0 && !subpaths.is_empty() {
        let paths: Vec<Path> = subpaths.into_iter().map(SubPath::into_path).collect();
        let grown = Region::from_paths(&paths, tol).offset(-offset, tol);
        subpaths = grown.loops;
    }
    Ok(subpaths.into_iter().map(SubPath::into_path).collect())
}

/// Marching squares over the field padded with one ring of empty cells, so
/// every loop closes. Corners are cell centres `(i, j)` with `i` from -1 to
/// `width` and `j` from -1 to `height`.
struct Tracer<'a> {
    field: &'a Field,
    level: f32,
}

impl Tracer<'_> {
    /// Corners across, padding included.
    fn cols(&self) -> usize {
        self.field.width + 2
    }

    /// Corners up, padding included.
    fn rows(&self) -> usize {
        self.field.height + 2
    }

    /// Height at a corner, NaN in the padding.
    fn value(&self, i: i64, j: i64) -> f32 {
        self.field.get(i, j)
    }

    /// True when the corner is material above the level.
    fn inside(&self, i: i64, j: i64) -> bool {
        self.value(i, j) > self.level
    }

    /// Index of the edge leaving corner `(i, j)` along X (`dir` 0) or Y
    /// (`dir` 1).
    fn key(&self, i: i64, j: i64, dir: usize) -> usize {
        let col = (i + 1) as usize;
        let row = (j + 1) as usize;
        (row * self.cols() + col) * 2 + dir
    }

    /// Job position where the contour crosses an edge, from its key.
    fn crossing(&self, key: usize) -> Point {
        let dir = key % 2;
        let corner = key / 2;
        let i = (corner % self.cols()) as i64 - 1;
        let j = (corner / self.cols()) as i64 - 1;
        let (i2, j2) = if dir == 0 { (i + 1, j) } else { (i, j + 1) };
        let a = self.value(i, j);
        let b = self.value(i2, j2);
        let t = if a.is_finite() && b.is_finite() && a != b {
            (f64::from(self.level - a) / f64::from(b - a)).clamp(1e-3, 1.0 - 1e-3)
        } else {
            0.5
        };
        let cell = self.field.cell;
        let x = self.field.origin.x + i as f64 * cell;
        let y = self.field.origin.y + j as f64 * cell;
        if dir == 0 {
            Point::new(x + t * cell, y)
        } else {
            Point::new(x, y + t * cell)
        }
    }

    /// Every closed contour as its crossing points, material on the left.
    fn loops(&self) -> Vec<Vec<Point>> {
        let mut next = vec![NONE; self.cols() * self.rows() * 2];
        let w = self.field.width as i64;
        let h = self.field.height as i64;
        let mut any = false;
        for j in -1..h {
            for i in -1..w {
                any |= self.square(i, j, &mut next);
            }
        }
        if !any {
            return Vec::new();
        }
        let mut seen = vec![false; next.len()];
        let mut out = Vec::new();
        for start in 0..next.len() {
            if next[start] == NONE || seen[start] {
                continue;
            }
            let mut points = Vec::new();
            let mut at = start;
            while at != NONE && !seen[at] {
                seen[at] = true;
                let p = self.crossing(at);
                if points
                    .last()
                    .is_none_or(|q: &Point| q.distance_squared(p) > 1e-18)
                {
                    points.push(p);
                }
                at = next[at];
            }
            if points.len() > 1 && points[0].distance_squared(points[points.len() - 1]) <= 1e-18 {
                points.pop();
            }
            if points.len() >= 3 {
                out.push(points);
            }
        }
        out
    }

    /// Adds the contour pieces of the grid square whose lowest corner is
    /// `(i, j)` to `next`, each from the edge where the walk counter
    /// clockwise round the square leaves material to the edge where it
    /// comes back. Returns true when the square holds any.
    fn square(&self, i: i64, j: i64, next: &mut [usize]) -> bool {
        let corners = [
            self.inside(i, j),
            self.inside(i + 1, j),
            self.inside(i + 1, j + 1),
            self.inside(i, j + 1),
        ];
        let edges = [
            self.key(i, j, 0),
            self.key(i + 1, j, 1),
            self.key(i, j + 1, 0),
            self.key(i, j, 1),
        ];
        let mut crossings = [(0usize, false); 4];
        let mut count = 0;
        for k in 0..4 {
            if corners[k] != corners[(k + 1) % 4] {
                crossings[count] = (edges[k], corners[k]);
                count += 1;
            }
        }
        match count {
            2 => {
                let (exit, entry) = if crossings[0].1 {
                    (crossings[0].0, crossings[1].0)
                } else {
                    (crossings[1].0, crossings[0].0)
                };
                next[exit] = entry;
                true
            }
            4 => {
                let joined = self.saddle_joined(i, j);
                for k in 0..4 {
                    if crossings[k].1 {
                        let partner = if joined { (k + 1) % 4 } else { (k + 3) % 4 };
                        next[crossings[k].0] = crossings[partner].0;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// True when the middle of a saddle square counts as material: the mean
    /// of its four heights is above the level, none being empty.
    fn saddle_joined(&self, i: i64, j: i64) -> bool {
        let sum = self.value(i, j)
            + self.value(i + 1, j)
            + self.value(i + 1, j + 1)
            + self.value(i, j + 1);
        sum.is_finite() && sum * 0.25 > self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::subpath_bbox;

    fn tol() -> Tolerance {
        Tolerance::new(1e-3).expect("tolerance")
    }

    fn field_with(width: usize, height: usize, cells: impl Fn(usize, usize) -> bool) -> Field {
        let mut f = Field::empty(width, height, 1.0, Point::new(0.5, 0.5));
        for j in 0..height {
            for i in 0..width {
                if cells(i, j) {
                    f.z[j * width + i] = 2.0;
                }
            }
        }
        f
    }

    fn area(path: &Path) -> f64 {
        path.subpaths.iter().map(subpath_signed_area).sum()
    }

    #[test]
    fn a_square_block_gives_one_square_loop() {
        let f = field_with(12, 12, |i, j| (3..9).contains(&i) && (3..9).contains(&j));
        let paths = boundary(&f, 0.0, 0.0, tol()).expect("boundary");
        assert_eq!(paths.len(), 1);
        let sub = &paths[0].subpaths[0];
        assert!(sub.closed);
        assert!(
            subpath_signed_area(sub) > 0.0,
            "outer loop is counter clockwise"
        );
        let b = subpath_bbox(sub).expect("bbox");
        assert!((b.min.x - 3.0).abs() <= 1.0 && (b.max.x - 9.0).abs() <= 1.0);
        assert!((b.min.y - 3.0).abs() <= 1.0 && (b.max.y - 9.0).abs() <= 1.0);
        assert!((area(&paths[0]) - 36.0).abs() < 6.0);
    }

    #[test]
    fn a_ring_gives_an_outer_loop_and_a_hole() {
        let f = field_with(14, 14, |i, j| {
            let d = (i as i64 - 7).abs().max((j as i64 - 7).abs());
            (2..=5).contains(&d)
        });
        let paths = boundary(&f, 0.0, 0.0, tol()).expect("boundary");
        assert_eq!(paths.len(), 2);
        let outers = paths.iter().filter(|p| area(p) > 0.0).count();
        let holes = paths.iter().filter(|p| area(p) < 0.0).count();
        assert_eq!((outers, holes), (1, 1));
    }

    #[test]
    fn two_blobs_give_two_loops() {
        let f = field_with(16, 8, |i, j| {
            (2..5).contains(&j) && ((1..5).contains(&i) || (9..14).contains(&i))
        });
        let paths = boundary(&f, 0.0, 0.0, tol()).expect("boundary");
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| area(p) > 0.0));
    }

    #[test]
    fn offset_grows_the_loop() {
        let f = field_with(12, 12, |i, j| (3..9).contains(&i) && (3..9).contains(&j));
        let plain = boundary(&f, 0.0, 0.0, tol()).expect("boundary");
        let grown = boundary(&f, 0.0, 2.0, tol()).expect("boundary");
        assert_eq!(grown.len(), 1);
        let a = subpath_bbox(&plain[0].subpaths[0]).expect("bbox");
        let b = subpath_bbox(&grown[0].subpaths[0]).expect("bbox");
        assert!((b.width() - a.width() - 4.0).abs() < 1e-2);
        assert!((b.height() - a.height() - 4.0).abs() < 1e-2);
        assert!(area(&grown[0]) > area(&plain[0]));
    }

    #[test]
    fn heights_at_or_below_the_level_are_left_out() {
        let mut f = field_with(10, 10, |i, j| (2..8).contains(&i) && (2..8).contains(&j));
        for j in 4..6 {
            for i in 4..6 {
                f.z[j * 10 + i] = 0.5;
            }
        }
        let paths = boundary(&f, 1.0, 0.0, tol()).expect("boundary");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn an_empty_field_gives_no_paths() {
        let f = Field::empty(8, 8, 1.0, Point::new(0.0, 0.0));
        assert!(boundary(&f, 0.0, 0.0, tol()).expect("boundary").is_empty());
        let none = Field::empty(0, 0, 1.0, Point::new(0.0, 0.0));
        assert!(boundary(&none, 0.0, 1.0, tol())
            .expect("boundary")
            .is_empty());
    }
}
