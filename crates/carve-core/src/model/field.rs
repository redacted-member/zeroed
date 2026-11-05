//! Heightfields for the relief model: a grid of heights in millimetres over
//! a rectangle of job space, with NaN where there is no material, and the
//! distance transforms shapes are built from.

use serde::{Deserialize, Serialize};

use crate::geom::Point;

/// Heights on a regular grid. Cell `(i, j)` is centred on
/// `origin + (i, j) * cell`; rows run from the lowest Y up. The heights are
/// not part of the serde form: `.carve` files keep them as raw binary next
/// to `document.json`, since JSON has no NaN.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Cells across (X).
    pub width: usize,
    /// Cells down (Y).
    pub height: usize,
    /// Cell size in millimetres.
    pub cell: f64,
    /// Job position of the first cell's centre.
    pub origin: Point,
    /// Heights, NaN where there is nothing.
    #[serde(skip)]
    pub z: Vec<f32>,
}

/// Two fields are equal when their grids match and every cell holds the
/// same height, empty cells (NaN) counting as equal to each other.
impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.cell == other.cell
            && self.origin == other.origin
            && self.z.len() == other.z.len()
            && self
                .z
                .iter()
                .zip(&other.z)
                .all(|(a, b)| a == b || (a.is_nan() && b.is_nan()))
    }
}

impl Field {
    /// A field of `width` by `height` cells, all empty.
    pub fn empty(width: usize, height: usize, cell: f64, origin: Point) -> Field {
        Field {
            width,
            height,
            cell,
            origin,
            z: vec![f32::NAN; width * height],
        }
    }

    /// Job position of a cell's centre.
    pub fn centre(&self, i: usize, j: usize) -> Point {
        Point::new(
            self.origin.x + i as f64 * self.cell,
            self.origin.y + j as f64 * self.cell,
        )
    }

    /// Height at a cell, NaN when empty or outside.
    pub fn get(&self, i: i64, j: i64) -> f32 {
        if i < 0 || j < 0 || i >= self.width as i64 || j >= self.height as i64 {
            return f32::NAN;
        }
        self.z[j as usize * self.width + i as usize]
    }

    /// Height under a job position (nearest cell), NaN when empty or outside.
    pub fn at(&self, p: Point) -> f32 {
        let i = ((p.x - self.origin.x) / self.cell).round() as i64;
        let j = ((p.y - self.origin.y) / self.cell).round() as i64;
        self.get(i, j)
    }

    /// The heights as little endian bytes, for saving.
    pub fn z_bytes(&self) -> Vec<u8> {
        self.z.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Fills the heights from bytes written by [`Field::z_bytes`].
    ///
    /// # Errors
    /// Returns [`crate::CoreError::Validation`] when the byte count does not
    /// match the field's size.
    pub fn set_z_bytes(&mut self, bytes: &[u8]) -> Result<(), crate::CoreError> {
        if bytes.len() != self.width * self.height * 4 {
            return Err(crate::CoreError::validation(
                "a saved relief does not match its size; the file is damaged",
            ));
        }
        self.z = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(())
    }

    /// Highest height, or `None` when the field is empty.
    pub fn max(&self) -> Option<f32> {
        self.z
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(None, |best, v| Some(best.map_or(v, |b: f32| b.max(v))))
    }

    /// Lowest height, or `None` when the field is empty.
    pub fn min(&self) -> Option<f32> {
        self.z
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(None, |best, v| Some(best.map_or(v, |b: f32| b.min(v))))
    }

    /// The smallest field holding every non-empty cell, sharing this
    /// field's grid, or `None` when nothing is set.
    pub fn cropped(&self) -> Option<Field> {
        let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for j in 0..self.height {
            for i in 0..self.width {
                if !self.z[j * self.width + i].is_nan() {
                    i0 = i0.min(i);
                    j0 = j0.min(j);
                    i1 = i1.max(i);
                    j1 = j1.max(j);
                }
            }
        }
        if i0 == usize::MAX {
            return None;
        }
        let (w, h) = (i1 - i0 + 1, j1 - j0 + 1);
        let mut z = Vec::with_capacity(w * h);
        for j in j0..=j1 {
            z.extend_from_slice(&self.z[j * self.width + i0..=j * self.width + i1]);
        }
        Some(Field {
            width: w,
            height: h,
            cell: self.cell,
            origin: self.centre(i0, j0),
            z,
        })
    }
}

/// A distance too large for any real field, standing in for infinity in
/// the distance transform.
const FAR: f64 = 1e20;

/// One dimensional squared distance transform of Felzenszwalb and
/// Huttenlocher: for each sample, the smallest `(i - q)^2 + f[q]`.
fn edt_line(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    edt_line_from(f, out, None, v, z);
}

/// [`edt_line`] that also records, for each sample, the index of the
/// sample its distance comes from.
fn edt_line_from(
    f: &[f64],
    out: &mut [f64],
    mut from: Option<&mut [usize]>,
    v: &mut [usize],
    z: &mut [f64],
) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let meet = |q: usize, p: usize| {
        ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64)
    };
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s = meet(q, v[k]);
        // z[0] is minus infinity, so this stops before k would underflow.
        while s <= z[k] {
            k -= 1;
            s = meet(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, slot) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *slot = d * d + f[v[k]];
        if let Some(from) = from.as_deref_mut() {
            from[q] = v[k];
        }
    }
}

/// Exact Euclidean distance, in cells, from every cell to the nearest seed
/// cell (`seed[i]` true). Cells when there is no seed at all get a very
/// large distance.
pub fn distance_to(seed: &[bool], width: usize, height: usize) -> Vec<f64> {
    let mut grid: Vec<f64> = seed.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    let longest = width.max(height);
    let (mut f, mut out) = (vec![0.0; longest], vec![0.0; longest]);
    let (mut v, mut z) = (vec![0usize; longest], vec![0.0; longest + 1]);
    // Columns, then rows.
    for i in 0..width {
        for j in 0..height {
            f[j] = grid[j * width + i];
        }
        edt_line(&f[..height], &mut out[..height], &mut v, &mut z);
        for j in 0..height {
            grid[j * width + i] = out[j];
        }
    }
    for j in 0..height {
        f[..width].copy_from_slice(&grid[j * width..(j + 1) * width]);
        edt_line(&f[..width], &mut out[..width], &mut v, &mut z);
        grid[j * width..(j + 1) * width].copy_from_slice(&out[..width]);
    }
    grid.iter().map(|d| d.sqrt()).collect()
}

/// Exact Euclidean distance transform that also names, for every cell,
/// the seed cell nearest to it (its index into `seed`). Cells when there is
/// no seed at all get a very large distance and index 0.
pub fn nearest_seed(seed: &[bool], width: usize, height: usize) -> (Vec<f64>, Vec<usize>) {
    let mut grid: Vec<f64> = seed.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    // The seed row nearest each cell within its column, then the column
    // whose (row distance, column distance) wins across each row.
    let mut row_of = vec![0usize; width * height];
    let longest = width.max(height);
    let (mut f, mut out) = (vec![0.0; longest], vec![0.0; longest]);
    let mut from = vec![0usize; longest];
    let (mut v, mut z) = (vec![0usize; longest], vec![0.0; longest + 1]);
    for i in 0..width {
        for j in 0..height {
            f[j] = grid[j * width + i];
        }
        edt_line_from(
            &f[..height],
            &mut out[..height],
            Some(&mut from[..height]),
            &mut v,
            &mut z,
        );
        for j in 0..height {
            grid[j * width + i] = out[j];
            row_of[j * width + i] = from[j];
        }
    }
    let mut nearest = vec![0usize; width * height];
    for j in 0..height {
        f[..width].copy_from_slice(&grid[j * width..(j + 1) * width]);
        edt_line_from(
            &f[..width],
            &mut out[..width],
            Some(&mut from[..width]),
            &mut v,
            &mut z,
        );
        for i in 0..width {
            let column = from[i];
            nearest[j * width + i] = row_of[j * width + column] * width + column;
        }
        grid[j * width..(j + 1) * width].copy_from_slice(&out[..width]);
    }
    (grid.iter().map(|d| d.sqrt()).collect(), nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_exact_euclidean() {
        let (w, h) = (9, 7);
        let mut seed = vec![false; w * h];
        seed[3 * w + 4] = true;
        let d = distance_to(&seed, w, h);
        assert_eq!(d[3 * w + 4], 0.0);
        assert!((d[3 * w + 7] - 3.0).abs() < 1e-12);
        assert!((d[0] - (16.0f64 + 9.0).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn the_nearest_of_several_seeds_wins() {
        let (w, h) = (20, 1);
        let mut seed = vec![false; w * h];
        seed[2] = true;
        seed[15] = true;
        let d = distance_to(&seed, w, h);
        assert_eq!(d[8], 6.0);
        assert_eq!(d[9], 6.0);
        assert_eq!(d[19], 4.0);
    }

    #[test]
    fn the_nearest_seed_is_named_with_its_distance() {
        let (w, h) = (12, 9);
        let mut seed = vec![false; w * h];
        seed[w + 2] = true;
        seed[7 * w + 10] = true;
        let (d, nearest) = nearest_seed(&seed, w, h);
        assert_eq!(d, distance_to(&seed, w, h));
        assert_eq!(nearest[2 * w + 3], w + 2);
        assert_eq!(nearest[6 * w + 9], 7 * w + 10);
        assert_eq!(nearest[w + 2], w + 2);
    }

    #[test]
    fn cropping_keeps_only_the_cells_in_use() {
        let mut field = Field::empty(10, 10, 0.5, Point::new(0.0, 0.0));
        field.z[3 * 10 + 4] = 1.0;
        field.z[5 * 10 + 6] = 2.0;
        let small = field.cropped().expect("something set");
        assert_eq!((small.width, small.height), (3, 3));
        assert_eq!(small.origin, Point::new(2.0, 1.5));
        assert_eq!(small.at(Point::new(3.0, 2.5)), 2.0);
        assert_eq!(small.max(), Some(2.0));
        assert!(Field::empty(3, 3, 1.0, Point::ORIGIN).cropped().is_none());
    }
}
