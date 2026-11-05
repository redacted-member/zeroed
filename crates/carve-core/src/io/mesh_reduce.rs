//! The model's surface as triangles for mesh export, either one pair per
//! grid square or reduced to fewer triangles.
//!
//! Reduction inserts cell centres greedily: it starts from the four corners,
//! triangulates (Delaunay), finds in each triangle the cell whose height is
//! furthest from the triangle, and adds those cells, round after round,
//! until every cell is within the tolerance or the triangle budget is spent.
//! The surface stays one triangulation of the whole rectangle, so the solid
//! built on it is watertight.

use delaunator::{triangulate, Point as DPoint, EMPTY};

use crate::model::field::Field;

/// How far the surface may be simplified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reduction {
    /// Every cell centre is a corner: two triangles per grid square.
    Full,
    /// Largest height difference allowed at any cell centre, millimetres.
    Tolerance(f64),
    /// Most triangles the surface may have.
    MaxTriangles(usize),
}

/// A triangulated surface over the model's cells.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Surface {
    /// Corner positions in job millimetres.
    pub vertices: Vec<[f64; 3]>,
    /// Triangles, counter clockwise seen from above.
    pub triangles: Vec<[usize; 3]>,
    /// The outer border, counter clockwise seen from above.
    pub ring: Vec<usize>,
    /// Largest height difference between the model and the surface at any
    /// cell centre, millimetres.
    pub deviation: f64,
    /// A second height per corner, from the field given to
    /// [`surface_pair`]; empty otherwise.
    pub back: Vec<f64>,
}

/// The surface of `field`, with empty cells at `empty_z`. A field smaller
/// than two by two cells, or whose heights do not match its size, gives an
/// empty surface.
pub fn surface(field: &Field, empty_z: f64, reduction: Reduction) -> Surface {
    let (w, h) = (field.width, field.height);
    if w < 2 || h < 2 || field.z.len() != w * h {
        return Surface::default();
    }
    let height = |k: usize| -> f64 {
        let z = field.z[k];
        if z.is_nan() {
            empty_z
        } else {
            f64::from(z)
        }
    };
    build(field, &[&height], reduction)
}

/// The surfaces of two fields over the same cells sharing one set of
/// triangles, reduced so that both stay within the tolerance. The second
/// field's heights (empty cells at `empty_z`) land in [`Surface::back`].
/// Empty when the fields differ in size.
pub fn surface_pair(front: &Field, back: &Field, empty_z: f64, reduction: Reduction) -> Surface {
    let (w, h) = (front.width, front.height);
    if w < 2
        || h < 2
        || front.z.len() != w * h
        || back.width != w
        || back.height != h
        || back.z.len() != w * h
    {
        return Surface::default();
    }
    let of = |field: &Field, k: usize| -> f64 {
        let z = field.z[k];
        if z.is_nan() {
            empty_z
        } else {
            f64::from(z)
        }
    };
    let top = |k: usize| of(front, k);
    let bottom = |k: usize| of(back, k);
    build(front, &[&top, &bottom], reduction)
}

/// Cells a reduced surface keeps whatever their error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Only the four corners to start from.
    Corners,
    /// Every cell of the bottom and top rows, so a surface wrapped round a
    /// rotary blank along X meets itself cell for cell.
    EdgeRows,
    /// Every cell of the left and right columns, for a blank along Y.
    EdgeColumns,
    /// For a surface wrapped round a blank: the whole edge rows (or
    /// columns, when `rows` is false) where it meets itself, and every
    /// `step`th cell in both directions, so no triangle spans more than
    /// `step` cells round the blank and cuts across it as a chord.
    Wrap { rows: bool, step: usize },
}

/// [`surface`], keeping the cells `keep` says in a reduced surface.
pub fn surface_keeping(field: &Field, empty_z: f64, reduction: Reduction, keep: Keep) -> Surface {
    let (w, h) = (field.width, field.height);
    if w < 2 || h < 2 || field.z.len() != w * h {
        return Surface::default();
    }
    let height = |k: usize| -> f64 {
        let z = field.z[k];
        if z.is_nan() {
            empty_z
        } else {
            f64::from(z)
        }
    };
    build_keeping(field, &[&height], reduction, keep)
}

fn build(field: &Field, heights: &[&dyn Fn(usize) -> f64], reduction: Reduction) -> Surface {
    build_keeping(field, heights, reduction, Keep::Corners)
}

fn build_keeping(
    field: &Field,
    heights: &[&dyn Fn(usize) -> f64],
    reduction: Reduction,
    keep: Keep,
) -> Surface {
    let mut surface = match reduction {
        Reduction::Full => full(field, heights[0]),
        Reduction::Tolerance(tolerance) => {
            greedy(field, heights, tolerance.max(0.0), usize::MAX, keep)
        }
        Reduction::MaxTriangles(count) => greedy(field, heights, 0.0, count.max(2), keep),
    };
    if let (Some(back), Some(cells)) = (heights.get(1), surface_cells(field, &surface)) {
        surface.back = cells.into_iter().map(back).collect();
    }
    surface
}

/// The cell index of each corner of a surface built on `field`.
fn surface_cells(field: &Field, surface: &Surface) -> Option<Vec<usize>> {
    let cell = if field.cell > 0.0 {
        field.cell
    } else {
        return None;
    };
    Some(
        surface
            .vertices
            .iter()
            .map(|v| {
                let i = ((v[0] - field.origin.x) / cell).round() as usize;
                let j = ((v[1] - field.origin.y) / cell).round() as usize;
                j * field.width + i
            })
            .collect(),
    )
}

fn full(field: &Field, height: &dyn Fn(usize) -> f64) -> Surface {
    let (w, h) = (field.width, field.height);
    let vertices = (0..w * h)
        .map(|k| {
            let c = field.centre(k % w, k / w);
            [c.x, c.y, height(k)]
        })
        .collect();
    let mut triangles = Vec::with_capacity(2 * (w - 1) * (h - 1));
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let (a, b, c, d) = (
                j * w + i,
                j * w + i + 1,
                (j + 1) * w + i + 1,
                (j + 1) * w + i,
            );
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    let mut ring = Vec::with_capacity(2 * (w + h));
    ring.extend(0..w - 1);
    ring.extend((0..h - 1).map(|j| j * w + w - 1));
    ring.extend((1..w).rev().map(|i| (h - 1) * w + i));
    ring.extend((1..h).rev().map(|j| j * w));
    Surface {
        vertices,
        triangles,
        ring,
        deviation: 0.0,
        back: Vec::new(),
    }
}

/// One triangle's plane over the grid: `z = a i + b j + c`.
struct Plane {
    a: f64,
    b: f64,
    c: f64,
}

impl Plane {
    fn through(p: [(f64, f64, f64); 3]) -> Option<Plane> {
        let (x1, y1, z1) = (p[1].0 - p[0].0, p[1].1 - p[0].1, p[1].2 - p[0].2);
        let (x2, y2, z2) = (p[2].0 - p[0].0, p[2].1 - p[0].1, p[2].2 - p[0].2);
        let det = x1 * y2 - x2 * y1;
        if det.abs() < 1e-12 {
            return None;
        }
        let a = (z1 * y2 - z2 * y1) / det;
        let b = (x1 * z2 - x2 * z1) / det;
        Some(Plane {
            a,
            b,
            c: p[0].2 - a * p[0].0 - b * p[0].1,
        })
    }

    fn at(&self, i: f64, j: f64) -> f64 {
        self.a * i + self.b * j + self.c
    }
}

/// Twice the signed area of a triangle of grid points; positive when
/// counter clockwise.
fn area2(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)
}

fn greedy(
    field: &Field,
    heights: &[&dyn Fn(usize) -> f64],
    tolerance: f64,
    budget: usize,
    keep: Keep,
) -> Surface {
    let height = heights[0];
    let (w, h) = (field.width, field.height);
    let mut chosen: Vec<usize> = vec![0, w - 1, (h - 1) * w + w - 1, (h - 1) * w];
    match keep {
        Keep::Corners => {}
        Keep::EdgeRows => {
            chosen.extend(1..w - 1);
            chosen.extend((1..w - 1).map(|i| (h - 1) * w + i));
        }
        Keep::EdgeColumns => {
            chosen.extend((1..h - 1).map(|j| j * w));
            chosen.extend((1..h - 1).map(|j| j * w + w - 1));
        }
        Keep::Wrap { rows, step } => {
            if rows {
                chosen.extend(1..w - 1);
                chosen.extend((1..w - 1).map(|i| (h - 1) * w + i));
            } else {
                chosen.extend((1..h - 1).map(|j| j * w));
                chosen.extend((1..h - 1).map(|j| j * w + w - 1));
            }
            let step = step.max(1);
            let lines = |n: usize| {
                let mut at: Vec<usize> = (0..n).step_by(step).collect();
                if at.last() != Some(&(n - 1)) {
                    at.push(n - 1);
                }
                at
            };
            for j in lines(h) {
                for i in lines(w) {
                    chosen.push(j * w + i);
                }
            }
            chosen.sort_unstable();
            chosen.dedup();
        }
    }
    let mut used = vec![false; w * h];
    for &k in &chosen {
        used[k] = true;
    }
    loop {
        let points: Vec<DPoint> = chosen
            .iter()
            .map(|&k| DPoint {
                x: (k % w) as f64,
                y: (k / w) as f64,
            })
            .collect();
        let mesh = triangulate(&points);
        let triangle_count = mesh.triangles.len() / 3;
        // The cell furthest from each triangle.
        let mut worst: Vec<(f64, usize)> = Vec::with_capacity(triangle_count);
        let mut deviation: f64 = 0.0;
        for t in mesh.triangles.chunks_exact(3) {
            let planes: Vec<Plane> = heights
                .iter()
                .filter_map(|channel| {
                    let corner = |n: usize| {
                        let k = chosen[t[n]];
                        ((k % w) as f64, (k / w) as f64, channel(k))
                    };
                    Plane::through([corner(0), corner(1), corner(2)])
                })
                .collect();
            if planes.len() != heights.len() {
                continue;
            }
            let at = |n: usize| {
                let k = chosen[t[n]];
                ((k % w) as f64, (k / w) as f64)
            };
            let flat = [at(0), at(1), at(2)];
            let sign = area2(flat[0], flat[1], flat[2]).signum();
            let lo_i = flat.iter().map(|q| q.0).fold(f64::MAX, f64::min) as usize;
            let hi_i = flat.iter().map(|q| q.0).fold(f64::MIN, f64::max) as usize;
            let lo_j = flat.iter().map(|q| q.1).fold(f64::MAX, f64::min) as usize;
            let hi_j = flat.iter().map(|q| q.1).fold(f64::MIN, f64::max) as usize;
            let mut best = (0.0, usize::MAX);
            for j in lo_j..=hi_j {
                for i in lo_i..=hi_i {
                    let q = (i as f64, j as f64);
                    let inside =
                        (0..3).all(|e| sign * area2(flat[e], flat[(e + 1) % 3], q) >= -1e-9);
                    if !inside {
                        continue;
                    }
                    let k = j * w + i;
                    let error = heights
                        .iter()
                        .zip(&planes)
                        .map(|(channel, plane)| (channel(k) - plane.at(q.0, q.1)).abs())
                        .fold(0.0, f64::max);
                    if error > best.0 {
                        best = (error, k);
                    }
                }
            }
            deviation = deviation.max(best.0);
            if best.1 != usize::MAX && !used[best.1] && best.0 > tolerance {
                worst.push(best);
            }
        }
        // Each new corner inside the rectangle adds two triangles.
        let room = budget.saturating_sub(triangle_count) / 2;
        if worst.is_empty() || room == 0 {
            return finish(
                field,
                height,
                &chosen,
                &mesh.triangles,
                &mesh.halfedges,
                deviation,
            );
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        worst.dedup_by_key(|entry| entry.1);
        worst.truncate(room);
        for (_, k) in worst {
            if !used[k] {
                used[k] = true;
                chosen.push(k);
            }
        }
    }
}

/// Turns the final triangulation into a surface in job millimetres.
fn finish(
    field: &Field,
    height: &dyn Fn(usize) -> f64,
    chosen: &[usize],
    triangles: &[usize],
    halfedges: &[usize],
    deviation: f64,
) -> Surface {
    let w = field.width;
    let vertices: Vec<[f64; 3]> = chosen
        .iter()
        .map(|&k| {
            let c = field.centre(k % w, k / w);
            [c.x, c.y, height(k)]
        })
        .collect();
    let flat = |v: usize| (vertices[v][0], vertices[v][1]);
    // The triangulation winds every triangle the same way; turn them all
    // counter clockwise together, keeping flat slivers along the border,
    // which join the heights of corners lying in a straight line.
    let clockwise = triangles
        .chunks_exact(3)
        .map(|t| area2(flat(t[0]), flat(t[1]), flat(t[2])))
        .find(|turn| *turn != 0.0)
        .is_some_and(|turn| turn < 0.0);
    let out: Vec<[usize; 3]> = triangles
        .chunks_exact(3)
        .map(|t| {
            if clockwise {
                [t[0], t[2], t[1]]
            } else {
                [t[0], t[1], t[2]]
            }
        })
        .collect();
    // Border edges are the half edges with no twin.
    let mut next = vec![usize::MAX; vertices.len()];
    let mut start = usize::MAX;
    for (e, twin) in halfedges.iter().enumerate() {
        if *twin == EMPTY {
            let from = triangles[e];
            let to = triangles[if e % 3 == 2 { e - 2 } else { e + 1 }];
            next[from] = to;
            start = start.min(from);
        }
    }
    let mut ring = Vec::new();
    if start != usize::MAX {
        let mut at = start;
        loop {
            ring.push(at);
            at = next[at];
            if at == start || at == usize::MAX || ring.len() > vertices.len() {
                break;
            }
        }
    }
    let ring_area: f64 = ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(&a, &b)| flat(a).0 * flat(b).1 - flat(b).0 * flat(a).1)
        .sum();
    if ring_area < 0.0 {
        ring.reverse();
    }
    Surface {
        vertices,
        triangles: out,
        ring,
        deviation,
        back: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn field(w: usize, h: usize, z: impl Fn(usize, usize) -> f32) -> Field {
        let mut f = Field::empty(w, h, 0.5, Point::new(10.0, 20.0));
        for j in 0..h {
            for i in 0..w {
                f.z[j * w + i] = z(i, j);
            }
        }
        f
    }

    /// Height of `s` at cell `(i, j)` by walking its triangles.
    fn height_on(s: &Surface, x: f64, y: f64) -> Option<f64> {
        for t in &s.triangles {
            let p: Vec<(f64, f64, f64)> = t
                .iter()
                .map(|&v| (s.vertices[v][0], s.vertices[v][1], s.vertices[v][2]))
                .collect();
            let flat = [(p[0].0, p[0].1), (p[1].0, p[1].1), (p[2].0, p[2].1)];
            if (0..3).all(|e| area2(flat[e], flat[(e + 1) % 3], (x, y)) >= -1e-9) {
                return Plane::through([p[0], p[1], p[2]]).map(|plane| plane.at(x, y));
            }
        }
        None
    }

    fn check_closed_ring(s: &Surface, f: &Field) {
        // The ring visits the four corners and runs counter clockwise.
        let area: f64 = s
            .ring
            .iter()
            .zip(s.ring.iter().cycle().skip(1))
            .map(|(&a, &b)| {
                s.vertices[a][0] * s.vertices[b][1] - s.vertices[b][0] * s.vertices[a][1]
            })
            .sum::<f64>()
            / 2.0;
        let span_x = (f.width - 1) as f64 * f.cell;
        let span_y = (f.height - 1) as f64 * f.cell;
        assert!((area - span_x * span_y).abs() < 1e-9, "{area}");
        for t in &s.triangles {
            let [a, b, c] = t.map(|v| (s.vertices[v][0], s.vertices[v][1]));
            assert!(area2(a, b, c) >= 0.0);
        }
    }

    #[test]
    fn the_full_surface_has_two_triangles_a_square() {
        let f = field(4, 3, |i, j| (i + j) as f32);
        let s = surface(&f, 0.0, Reduction::Full);
        assert_eq!(s.triangles.len(), 2 * 3 * 2);
        assert_eq!(s.ring.len(), 2 * (3 + 2));
        assert_eq!(s.deviation, 0.0);
        check_closed_ring(&s, &f);
    }

    #[test]
    fn a_plane_reduces_to_two_triangles() {
        let f = field(30, 20, |i, j| 0.1 * i as f32 + 0.2 * j as f32);
        let s = surface(&f, 0.0, Reduction::Tolerance(0.001));
        assert_eq!(s.triangles.len(), 2);
        assert_eq!(s.vertices.len(), 4);
        assert!(s.deviation < 1e-5);
        check_closed_ring(&s, &f);
    }

    #[test]
    fn a_dome_stays_within_the_tolerance() {
        let dome = |i: usize, j: usize| {
            let (x, y) = (i as f32 - 20.0, j as f32 - 15.0);
            (100.0 - x * x - y * y).max(0.0) / 20.0
        };
        let f = field(41, 31, dome);
        for tolerance in [0.5, 0.1, 0.02] {
            let s = surface(&f, 0.0, Reduction::Tolerance(tolerance));
            assert!(
                s.deviation <= tolerance + 1e-9,
                "{tolerance}: {}",
                s.deviation
            );
            assert!(s.triangles.len() < 2 * 40 * 30);
            check_closed_ring(&s, &f);
            // Every cell is within the tolerance of the surface.
            for j in 0..31 {
                for i in 0..41 {
                    let c = f.centre(i, j);
                    let z = height_on(&s, c.x, c.y).expect("covered");
                    assert!((z - f64::from(dome(i, j))).abs() <= tolerance + 1e-6);
                }
            }
        }
    }

    #[test]
    fn a_triangle_budget_is_kept() {
        let f = field(41, 31, |i, j| ((i * 7 + j * 13) % 5) as f32);
        for budget in [2, 50, 400] {
            let s = surface(&f, 0.0, Reduction::MaxTriangles(budget));
            assert!(
                s.triangles.len() <= budget,
                "{budget}: {}",
                s.triangles.len()
            );
            check_closed_ring(&s, &f);
        }
        let coarse = surface(&f, 0.0, Reduction::MaxTriangles(50));
        let fine = surface(&f, 0.0, Reduction::MaxTriangles(400));
        assert!(fine.deviation <= coarse.deviation);
    }

    #[test]
    fn a_pair_shares_its_triangles_and_keeps_both_within_the_tolerance() {
        let front = field(31, 21, |i, _| 0.2 * i as f32);
        let back = field(31, 21, |i, j| {
            if (10..20).contains(&i) && j == 10 {
                3.0
            } else {
                0.0
            }
        });
        let s = surface_pair(&front, &back, 0.0, Reduction::Tolerance(0.01));
        assert_eq!(s.back.len(), s.vertices.len());
        assert!(s.deviation <= 0.01);
        // The front alone is a plane; the back's ridge forces more corners.
        assert!(
            s.triangles.len()
                > surface(&front, 0.0, Reduction::Tolerance(0.01))
                    .triangles
                    .len()
        );
        let full = surface_pair(&front, &back, 0.0, Reduction::Full);
        assert_eq!(full.back[10 * 31 + 12], 3.0);
        assert!(
            surface_pair(&front, &field(3, 3, |_, _| 0.0), 0.0, Reduction::Full)
                .vertices
                .is_empty()
        );
    }

    #[test]
    fn empty_cells_sit_at_the_given_height() {
        let f = field(3, 3, |i, _| if i == 1 { f32::NAN } else { 2.0 });
        let s = surface(&f, -1.0, Reduction::Full);
        assert_eq!(s.vertices[1][2], -1.0);
        assert!(surface(&field(1, 3, |_, _| 0.0), 0.0, Reduction::Full)
            .triangles
            .is_empty());
    }
}
