//! The material as a heightfield: one top height per grid cell, lowered
//! wherever a cutter sweeps through it.

use crate::cutter::Cutter;

/// The cells a heightfield covers, lengths in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Cells along X.
    pub width: usize,
    /// Cells along Y.
    pub height: usize,
    /// Size of one square cell.
    pub cell: f64,
    /// X of the centre of the first cell.
    pub origin_x: f64,
    /// Y of the centre of the first cell.
    pub origin_y: f64,
    /// Z of the uncut material top.
    pub top: f64,
    /// Z of the underside of the material; no cell goes below it.
    pub bottom: f64,
}

impl Grid {
    /// Centre of cell (`i`, `j`).
    pub fn centre(&self, i: usize, j: usize) -> (f64, f64) {
        (
            self.origin_x + i as f64 * self.cell,
            self.origin_y + j as f64 * self.cell,
        )
    }

    /// Range of cell columns whose centres lie within `r` of `x`.
    fn columns(&self, x: f64, r: f64) -> Option<(usize, usize)> {
        span(x, r, self.origin_x, self.cell, self.width)
    }

    /// Range of cell rows whose centres lie within `r` of `y`.
    fn rows(&self, y: f64, r: f64) -> Option<(usize, usize)> {
        span(y, r, self.origin_y, self.cell, self.height)
    }
}

fn span(v: f64, r: f64, origin: f64, cell: f64, count: usize) -> Option<(usize, usize)> {
    let lo = ((v - r - origin) / cell).ceil();
    let hi = ((v + r - origin) / cell).floor();
    if hi < 0.0 || lo > (count as f64 - 1.0) || hi < lo {
        return None;
    }
    Some((lo.max(0.0) as usize, (hi as usize).min(count - 1)))
}

/// Material heights over a grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The cells.
    pub grid: Grid,
    /// Height per cell, row by row from the lowest Y.
    pub z: Vec<f64>,
}

impl Field {
    /// Uncut material over `grid`.
    pub fn new(grid: Grid) -> Field {
        Field {
            grid,
            z: vec![grid.top; grid.width * grid.height],
        }
    }

    /// Height at the cell nearest (`x`, `y`), `None` off the grid.
    pub fn height_at(&self, x: f64, y: f64) -> Option<f64> {
        let i = ((x - self.grid.origin_x) / self.grid.cell).round();
        let j = ((y - self.grid.origin_y) / self.grid.cell).round();
        if i < 0.0 || j < 0.0 || i >= self.grid.width as f64 || j >= self.grid.height as f64 {
            return None;
        }
        Some(self.z[j as usize * self.grid.width + i as usize])
    }

    /// Points along a straight move from `from` to `to`, close enough that
    /// the cutter's swept shape has no gaps between them.
    fn samples(&self, from: [f64; 3], to: [f64; 3]) -> usize {
        let across = (to[0] - from[0]).hypot(to[1] - from[1]);
        let step = self.grid.cell / 2.0;
        ((across / step).ceil() as usize).max(1)
    }

    /// Lowers the material wherever the cutter passes on its way from
    /// `from` to `to`, tip positions.
    pub fn cut(&mut self, from: [f64; 3], to: [f64; 3], cutter: &Cutter) {
        let n = self.samples(from, to);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let p = lerp(from, to, t);
            self.cut_at(p, cutter);
        }
    }

    fn cut_at(&mut self, p: [f64; 3], cutter: &Cutter) {
        let r = cutter.radius();
        if p[2] >= self.grid.top {
            return;
        }
        let (Some((i0, i1)), Some((j0, j1))) =
            (self.grid.columns(p[0], r), self.grid.rows(p[1], r))
        else {
            return;
        };
        let bottom = self.grid.bottom;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (cx, cy) = self.grid.centre(i, j);
                let d = (cx - p[0]).hypot(cy - p[1]);
                if let Some(h) = cutter.height_at(d) {
                    let z = (p[2] + h).max(bottom);
                    let cell = &mut self.z[j * self.grid.width + i];
                    if z < *cell {
                        *cell = z;
                    }
                }
            }
        }
    }

    /// How far the cutter's bottom stays above the material on a straight
    /// move, the smallest gap found; negative when it would cut. `None`
    /// when the move never comes over the grid.
    pub fn clearance(&self, from: [f64; 3], to: [f64; 3], cutter: &Cutter) -> Option<f64> {
        let n = self.samples(from, to);
        let r = cutter.radius();
        let mut lowest: Option<f64> = None;
        for k in 0..=n {
            let p = lerp(from, to, k as f64 / n as f64);
            if p[2] > self.grid.top + 1.0 {
                continue;
            }
            let (Some((i0, i1)), Some((j0, j1))) =
                (self.grid.columns(p[0], r), self.grid.rows(p[1], r))
            else {
                continue;
            };
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let (cx, cy) = self.grid.centre(i, j);
                    let d = (cx - p[0]).hypot(cy - p[1]);
                    if let Some(h) = cutter.height_at(d) {
                        let material = self.z[j * self.grid.width + i];
                        // Material already cut through leaves nothing to hit.
                        if material <= self.grid.bottom + 1e-9 {
                            continue;
                        }
                        let gap = p[2] + h - material;
                        lowest = Some(lowest.map_or(gap, |l: f64| l.min(gap)));
                    }
                }
            }
        }
        lowest
    }
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Steepest slope, rise over run, that widens the comparison band.
pub const SLOPE_LIMIT: f64 = 2.0;

/// How two heightfields over the same grid differ.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Comparison {
    /// Largest height difference cell for cell.
    pub max_abs: f64,
    /// Mean height difference cell for cell.
    pub mean_abs: f64,
    /// Cells that differ by more than the tolerance even when either field
    /// is allowed to shift by one cell.
    pub over: usize,
    /// The worst of those cells: X, Y, height in the first field, height in
    /// the second and the amount beyond the tolerance.
    pub worst: Option<(f64, f64, f64, f64, f64)>,
}

/// Compares heights `a` and `b` over `grid`, row by row from the lowest Y.
/// A cell passes when its height in one field lies within the heights the
/// other field has in the cell and its eight neighbours, widened by
/// `tolerance` plus what a flank of steepness `slope` (rise over run, up to
/// [`SLOPE_LIMIT`]) climbs over half a cell's diagonal: a field that places
/// its cutter at the nearest cell centre may put an edge a cell away and a
/// V groove's bottom that much off, and the two still agree.
///
/// # Panics
/// Panics when either field does not have one height per cell.
pub fn compare(grid: &Grid, a: &[f64], b: &[f64], tolerance: f64, slope: f64) -> Comparison {
    let band =
        tolerance + slope.clamp(0.0, SLOPE_LIMIT) * grid.cell * std::f64::consts::FRAC_1_SQRT_2;
    let n = grid.width * grid.height;
    assert_eq!(a.len(), n, "the first field has one height per cell");
    assert_eq!(b.len(), n, "the second field has one height per cell");
    let clamp = |v: f64| v.max(grid.bottom).min(grid.top);
    let mut out = Comparison::default();
    let mut sum = 0.0;
    for j in 0..grid.height {
        for i in 0..grid.width {
            let c = j * grid.width + i;
            let (va, vb) = (clamp(a[c]), clamp(b[c]));
            let d = (va - vb).abs();
            sum += d;
            out.max_abs = out.max_abs.max(d);
            let beyond = |value: f64, other: &[f64]| -> f64 {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for jj in j.saturating_sub(1)..=(j + 1).min(grid.height - 1) {
                    for ii in i.saturating_sub(1)..=(i + 1).min(grid.width - 1) {
                        let v = clamp(other[jj * grid.width + ii]);
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
                (lo - band - value).max(value - hi - band).max(0.0)
            };
            let excess = beyond(va, b).max(beyond(vb, a));
            if excess > 0.0 {
                out.over += 1;
                if out.worst.is_none_or(|w| excess > w.4) {
                    let (x, y) = grid.centre(i, j);
                    out.worst = Some((x, y, va, vb, excess));
                }
            }
        }
    }
    out.mean_abs = if n == 0 { 0.0 } else { sum / n as f64 };
    out
}
