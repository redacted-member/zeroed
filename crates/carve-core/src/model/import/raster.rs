//! Vertical rays through a mesh: for every cell centre of a grid, the
//! heights where the mesh crosses the vertical line there and which way
//! each crossing faces. From those the highest and lowest surface, and for
//! a closed mesh the spans that are inside the solid, which is how a model
//! cut by planes gets its cut faces filled in without building them.

use crate::geom::Point;
use crate::io::mesh::Triangle;

use super::super::shape::Grid;
use super::vec3::{self, V3};

/// Keeps the points `p` with `normal . p >= offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfSpace {
    /// Points into the kept side.
    pub normal: V3,
    /// Plane offset along `normal`, millimetres.
    pub offset: f64,
}

impl HalfSpace {
    /// The side of the plane through `point` that `normal` points into.
    pub fn through(point: V3, normal: V3) -> HalfSpace {
        let normal = vec3::unit(normal);
        HalfSpace {
            normal,
            offset: vec3::dot(normal, point),
        }
    }

    /// The other side of the same plane.
    pub fn flipped(self) -> HalfSpace {
        HalfSpace {
            normal: [-self.normal[0], -self.normal[1], -self.normal[2]],
            offset: -self.offset,
        }
    }

    /// Signed distance of `p` from the plane, positive on the kept side.
    pub fn distance(&self, p: V3) -> f64 {
        vec3::dot(self.normal, p) - self.offset
    }

    /// The same half space after `turn` is applied to every point.
    pub fn turned(self, turn: impl Fn(V3) -> V3) -> HalfSpace {
        let origin = vec3::add_scaled([0.0; 3], self.normal, self.offset);
        HalfSpace::through(turn(origin), turn(self.normal))
    }
}

/// The span of heights on the vertical line at `(x, y)` that every half
/// space keeps, within `lo..=hi`, or `None` when they keep nothing there.
pub fn z_range(spaces: &[HalfSpace], x: f64, y: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (lo, hi);
    for s in spaces {
        let rest = s.offset - s.normal[0] * x - s.normal[1] * y;
        let nz = s.normal[2];
        if nz.abs() < 1e-12 {
            if rest > 1e-9 {
                return None;
            }
        } else if nz > 0.0 {
            lo = lo.max(rest / nz);
        } else {
            hi = hi.min(rest / nz);
        }
        if lo > hi {
            return None;
        }
    }
    Some((lo, hi))
}

/// Mesh crossings of the vertical line at each cell centre of a grid.
#[derive(Debug, Clone)]
pub struct Hits {
    /// The grid the lines stand on.
    pub grid: Grid,
    /// Index into `z` and `up` of each cell's first crossing; one more
    /// entry than there are cells.
    start: Vec<u32>,
    /// Crossing heights, sorted upward within each cell.
    z: Vec<f32>,
    /// Whether the crossing's face looks up (the line leaves the solid).
    up: Vec<bool>,
    /// Lowest and highest mesh points, for spans left open at either end.
    pub z_lo: f64,
    /// See `z_lo`.
    pub z_hi: f64,
}

/// Calls `hit(cell, z, up)` for every cell centre each triangle covers.
fn each_hit(triangles: &[Triangle], grid: &Grid, mut hit: impl FnMut(usize, f64, bool)) {
    for t in triangles {
        let (a, b, c) = (t[0], t[1], t[2]);
        // Twice the signed area seen from above; skip edge-on triangles.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-12 {
            continue;
        }
        let up = area > 0.0;
        let min_x = a[0].min(b[0]).min(c[0]);
        let max_x = a[0].max(b[0]).max(c[0]);
        let min_y = a[1].min(b[1]).min(c[1]);
        let max_y = a[1].max(b[1]).max(c[1]);
        let i0 = ((min_x - grid.origin.x) / grid.cell).ceil().max(0.0);
        let j0 = ((min_y - grid.origin.y) / grid.cell).ceil().max(0.0);
        let i1 = ((max_x - grid.origin.x) / grid.cell)
            .floor()
            .min(grid.width as f64 - 1.0);
        let j1 = ((max_y - grid.origin.y) / grid.cell)
            .floor()
            .min(grid.height as f64 - 1.0);
        if i1 < i0 || j1 < j0 {
            continue;
        }
        for j in (j0 as usize)..=(j1 as usize) {
            let y = grid.origin.y + j as f64 * grid.cell;
            for i in (i0 as usize)..=(i1 as usize) {
                let x = grid.origin.x + i as f64 * grid.cell;
                let w_a = ((b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y)) / area;
                let w_b = ((c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y)) / area;
                let w_c = 1.0 - w_a - w_b;
                if w_a < -1e-9 || w_b < -1e-9 || w_c < -1e-9 {
                    continue;
                }
                hit(j * grid.width + i, w_a * a[2] + w_b * b[2] + w_c * c[2], up);
            }
        }
    }
}

impl Hits {
    /// Casts the lines of `grid` through `triangles`.
    pub fn cast(triangles: &[Triangle], grid: &Grid) -> Hits {
        let cells = grid.width * grid.height;
        let mut start = vec![0u32; cells + 1];
        each_hit(triangles, grid, |cell, _, _| start[cell + 1] += 1);
        for k in 0..cells {
            start[k + 1] += start[k];
        }
        let total = start[cells] as usize;
        let mut fill = start.clone();
        let mut z = vec![0f32; total];
        let mut up = vec![false; total];
        each_hit(triangles, grid, |cell, h, u| {
            let at = fill[cell] as usize;
            z[at] = h as f32;
            up[at] = u;
            fill[cell] += 1;
        });
        let mut pairs: Vec<(f32, bool)> = Vec::new();
        for cell in 0..cells {
            let (s, e) = (start[cell] as usize, start[cell + 1] as usize);
            if e - s > 1 {
                pairs.clear();
                pairs.extend((s..e).map(|k| (z[k], up[k])));
                // Entering faces first at equal heights keeps thin shells open.
                pairs.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
                for (k, (h, u)) in pairs.iter().enumerate() {
                    z[s + k] = *h;
                    up[s + k] = *u;
                }
            }
        }
        let (mut z_lo, mut z_hi) = (f64::MAX, f64::MIN);
        for v in triangles.iter().flatten() {
            z_lo = z_lo.min(v[2]);
            z_hi = z_hi.max(v[2]);
        }
        Hits {
            grid: *grid,
            start,
            z,
            up,
            z_lo,
            z_hi,
        }
    }

    /// Job position of a cell's centre.
    pub fn centre(&self, cell: usize) -> Point {
        let (i, j) = (cell % self.grid.width, cell / self.grid.width);
        Point::new(
            self.grid.origin.x + i as f64 * self.grid.cell,
            self.grid.origin.y + j as f64 * self.grid.cell,
        )
    }

    fn cell_hits(&self, cell: usize) -> impl Iterator<Item = (f64, bool)> + '_ {
        let (s, e) = (self.start[cell] as usize, self.start[cell + 1] as usize);
        (s..e).map(move |k| (f64::from(self.z[k]), self.up[k]))
    }

    /// Whether any surface crosses the cell's line.
    pub fn any(&self, cell: usize) -> bool {
        self.start[cell + 1] > self.start[cell]
    }

    /// Spans of the line inside the solid, bottom up. A face looking down
    /// opens a span and one looking up closes it; a span left open at
    /// either end runs to the mesh's lowest or highest point, so an open
    /// surface counts as solid below it.
    fn spans(&self, cell: usize, mut out: impl FnMut(f64, f64)) {
        let mut depth = 0i32;
        let mut opened = self.z_lo;
        let mut closed_any = false;
        for (h, up) in self.cell_hits(cell) {
            if up {
                if depth <= 1 {
                    out(if depth == 1 { opened } else { self.z_lo }, h);
                    closed_any = true;
                    depth = 0;
                } else {
                    depth -= 1;
                }
            } else {
                if depth == 0 {
                    opened = h;
                }
                depth += 1;
            }
        }
        if depth > 0 {
            out(opened, self.z_hi);
        } else if !closed_any && self.any(cell) {
            // Only faces looking down: treat them as a surface too.
            for (h, _) in self.cell_hits(cell) {
                out(self.z_lo, h);
            }
        }
    }

    /// The highest point on the cell's line within `range`: of the solid
    /// when `caps` is set (so cut faces read as flat), else of the surface.
    pub fn highest(&self, cell: usize, range: (f64, f64), caps: bool) -> Option<f64> {
        let (lo, hi) = range;
        let mut best: Option<f64> = None;
        if caps {
            self.spans(cell, |a, b| {
                let (a, b) = (a.max(lo), b.min(hi));
                if a <= b + 1e-9 {
                    best = Some(best.map_or(b, |x: f64| x.max(b)));
                }
            });
        } else {
            for (h, _) in self.cell_hits(cell) {
                if h >= lo - 1e-9 && h <= hi + 1e-9 {
                    best = Some(best.map_or(h, |x: f64| x.max(h)));
                }
            }
        }
        best
    }

    /// The lowest point on the cell's line within `range`; see [`Self::highest`].
    pub fn lowest(&self, cell: usize, range: (f64, f64), caps: bool) -> Option<f64> {
        let (lo, hi) = range;
        let mut best: Option<f64> = None;
        if caps {
            self.spans(cell, |a, b| {
                let (a, b) = (a.max(lo), b.min(hi));
                if a <= b + 1e-9 {
                    best = Some(best.map_or(a, |x: f64| x.min(a)));
                }
            });
        } else {
            for (h, _) in self.cell_hits(cell) {
                if h >= lo - 1e-9 && h <= hi + 1e-9 {
                    best = Some(best.map_or(h, |x: f64| x.min(h)));
                }
            }
        }
        best
    }
}

/// The part of `grid` covering `min..max` (job millimetres), sharing its
/// cells, or `None` when they do not overlap.
pub fn window(grid: &Grid, min: Point, max: Point) -> Option<Grid> {
    let i0 = ((min.x - grid.origin.x) / grid.cell).floor().max(0.0);
    let j0 = ((min.y - grid.origin.y) / grid.cell).floor().max(0.0);
    let i1 = ((max.x - grid.origin.x) / grid.cell)
        .ceil()
        .min(grid.width as f64 - 1.0);
    let j1 = ((max.y - grid.origin.y) / grid.cell)
        .ceil()
        .min(grid.height as f64 - 1.0);
    if i1 < i0 || j1 < j0 {
        return None;
    }
    Some(Grid {
        width: (i1 - i0) as usize + 1,
        height: (j1 - j0) as usize + 1,
        cell: grid.cell,
        origin: Point::new(
            grid.origin.x + i0 * grid.cell,
            grid.origin.y + j0 * grid.cell,
        ),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An axis aligned closed box as twelve outward facing triangles.
    pub fn cube(lo: V3, hi: V3) -> Vec<Triangle> {
        let p = |x: usize, y: usize, z: usize| {
            [
                if x == 0 { lo[0] } else { hi[0] },
                if y == 0 { lo[1] } else { hi[1] },
                if z == 0 { lo[2] } else { hi[2] },
            ]
        };
        // Each quad counter clockwise seen from outside.
        let quads = [
            [p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0)],
            [p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)],
            [p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1)],
            [p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0)],
            [p(0, 0, 0), p(0, 0, 1), p(0, 1, 1), p(0, 1, 0)],
            [p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1)],
        ];
        quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect()
    }

    pub fn grid(width: usize, cell: f64) -> Grid {
        Grid {
            width,
            height: width,
            cell,
            origin: Point::new(cell / 2.0, cell / 2.0),
        }
    }

    #[test]
    fn a_box_gives_one_span_from_bottom_to_top() {
        let hits = Hits::cast(&cube([2.0, 2.0, 1.0], [8.0, 8.0, 5.0]), &grid(10, 1.0));
        let inside = 5 * 10 + 5;
        let all = (f64::MIN, f64::MAX);
        assert_eq!(hits.highest(inside, all, true), Some(5.0));
        assert_eq!(hits.lowest(inside, all, true), Some(1.0));
        assert_eq!(hits.highest(0, all, true), None);
        // Cut at z 3, keeping below: the cap is at 3 with caps, and without
        // them the line sees only the bottom face.
        assert_eq!(hits.highest(inside, (f64::MIN, 3.0), true), Some(3.0));
        assert_eq!(hits.highest(inside, (f64::MIN, 3.0), false), Some(1.0));
    }

    #[test]
    fn an_open_surface_counts_as_solid_below_it() {
        let top = vec![
            [[0.0, 0.0, 2.0], [10.0, 0.0, 2.0], [10.0, 10.0, 2.0]],
            [[0.0, 0.0, 2.0], [10.0, 10.0, 2.0], [0.0, 10.0, 2.0]],
        ];
        let hits = Hits::cast(&top, &grid(10, 1.0));
        assert_eq!(hits.highest(55, (f64::MIN, f64::MAX), true), Some(2.0));
    }

    #[test]
    fn half_spaces_narrow_the_line() {
        let below = HalfSpace::through([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]);
        assert_eq!(z_range(&[below], 1.0, 1.0, 0.0, 10.0), Some((0.0, 3.0)));
        let above = below.flipped();
        assert_eq!(z_range(&[above], 1.0, 1.0, 0.0, 10.0), Some((3.0, 10.0)));
        assert_eq!(
            z_range(&[above, below], 1.0, 1.0, 0.0, 10.0),
            Some((3.0, 3.0))
        );
        let left = HalfSpace::through([5.0, 0.0, 0.0], [-1.0, 0.0, 0.0]);
        assert!(z_range(&[left], 6.0, 0.0, 0.0, 10.0).is_none());
        assert!(z_range(&[left], 4.0, 0.0, 0.0, 10.0).is_some());
        let turned = left.turned(|v| [-v[0], v[1], v[2]]);
        assert!(z_range(&[turned], -6.0, 0.0, 0.0, 10.0).is_none());
    }

    #[test]
    fn a_window_shares_the_grid_cells() {
        let g = grid(100, 0.5);
        let w = window(&g, Point::new(10.0, 20.0), Point::new(12.0, 21.0)).expect("overlaps");
        assert_eq!(w.cell, 0.5);
        assert!(((w.origin.x - g.origin.x) / 0.5).fract().abs() < 1e-9);
        assert!(w.origin.x <= 10.0 && w.origin.x + (w.width - 1) as f64 * 0.5 >= 12.0);
        assert!(window(&g, Point::new(-20.0, -20.0), Point::new(-10.0, -10.0)).is_none());
    }
}
