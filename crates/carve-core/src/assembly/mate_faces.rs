//! The faces a mate takes from a click on a piece: a flat face, from the
//! piece's exact solid when it has one (so a mate lays the cut faces
//! together, not the grid's), else from its simulation grid; or a round
//! axis (a drilled hole, a pin, a dowel), from the round wall under the
//! click. Every piece face is upright or level in its own frame, so round
//! axes run along the piece's Z.

use std::collections::{BTreeSet, VecDeque};

use crate::cam::sim::{Stock, THROUGH};

use super::exact::ExactSolid;
use super::mates::MateFace;
use super::measure;
use super::pieces::{self, PickedFace};
use super::pose::{self, Vec3};

/// A ring is round when no corner strays further than this from the
/// circle through them, as a share of the radius (and at least
/// [`ROUND_SLACK`]).
const ROUND_SHARE: f64 = 0.005;

/// Least stray allowed of a round ring, mm.
const ROUND_SLACK: f64 = 0.02;

/// Fewest corners a round ring has.
pub(crate) const ROUND_CORNERS: usize = 12;

/// A click this far from a ring does not pick it, mm.
const RING_REACH: f64 = 2.0;

/// Widest round hole or pin read from the grid, cells across.
const GRID_REACH: usize = 600;

/// The flat face at a local point: the exact solid's nearest face, else
/// the grid's.
pub fn plane_face(stock: &Stock, solid: Option<&ExactSolid>, local: Vec3) -> Option<PickedFace> {
    if let Some(solid) = solid {
        if let Some((point, normal)) = measure::nearest_face(solid, local) {
            return Some(PickedFace { point, normal });
        }
    }
    pieces::face_at(stock, local)
}

/// The face on the other side of the material from `face` (see
/// [`pieces::opposite_face`]), snapped to the exact solid when there is one.
pub fn opposite_plane(stock: &Stock, solid: Option<&ExactSolid>, face: &PickedFace) -> PickedFace {
    let far = pieces::opposite_face(stock, face);
    match solid.and_then(|s| measure::nearest_face(s, far.point)) {
        Some((point, normal)) if pose::dot(normal, far.normal) > 0.9 => {
            PickedFace { point, normal }
        }
        _ => far,
    }
}

/// The circle through points, least squares: its centre and radius, and
/// the furthest any point strays from it.
pub fn fit_circle(points: &[[f64; 2]]) -> Option<([f64; 2], f64, f64)> {
    if points.len() < 3 {
        return None;
    }
    // Shift to the points' middle so the sums stay well scaled.
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let my = points.iter().map(|p| p[1]).sum::<f64>() / n;
    // x^2 + y^2 + D x + E y + F = 0.
    let mut m = [[0.0_f64; 3]; 3];
    let mut r = [0.0_f64; 3];
    for p in points {
        let (x, y) = (p[0] - mx, p[1] - my);
        let row = [x, y, 1.0];
        let rhs = -(x * x + y * y);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            r[i] += row[i] * rhs;
        }
    }
    let det = |a: &[[f64; 3]; 3]| {
        a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
            - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-18 {
        return None;
    }
    let mut solved = [0.0; 3];
    for (k, value) in solved.iter_mut().enumerate() {
        let mut a = m;
        for i in 0..3 {
            a[i][k] = r[i];
        }
        *value = det(&a) / d;
    }
    let (cx, cy) = (-solved[0] / 2.0, -solved[1] / 2.0);
    let radius2 = cx * cx + cy * cy - solved[2];
    if radius2 <= 0.0 {
        return None;
    }
    let radius = radius2.sqrt();
    let stray = points
        .iter()
        .map(|p| ((p[0] - mx - cx).hypot(p[1] - my - cy) - radius).abs())
        .fold(0.0, f64::max);
    Some(([cx + mx, cy + my], radius, stray))
}

pub(crate) fn round(stray: f64, radius: f64) -> bool {
    stray <= (radius * ROUND_SHARE).max(ROUND_SLACK)
}

/// Fewest edges a partial arc runs over.
const ARC_EDGES: usize = 4;

/// Most one edge of an arc turns about its centre, degrees: a polygon's
/// corners lie on a circle too, but its edges turn further.
const ARC_EDGE_DEGREES: f64 = 30.0;

/// Least a partial arc turns about its centre, degrees.
const ARC_DEGREES: f64 = 30.0;

/// A run of a ring's corners on one circle: the round end of a slot or a
/// tab, a fillet, or (running all round) a round hole or pin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RingArc {
    /// Its first corner, an index into the ring.
    pub(crate) first: usize,
    /// How many edges it runs over, from its first corner on (wrapping).
    pub(crate) edges: usize,
    pub(crate) centre: [f64; 2],
    pub(crate) radius: f64,
    /// Where it starts, radians about the centre.
    pub(crate) start: f64,
    /// How far it turns, radians, positive anticlockwise; a whole turn
    /// for a round ring.
    pub(crate) sweep: f64,
}

impl RingArc {
    /// Whether it runs all round.
    pub(crate) fn closed(&self) -> bool {
        self.sweep.abs() >= std::f64::consts::TAU - 1e-9
    }

    /// Whether the material lies on its centre's side (a pin, a tab's
    /// round end), else away from it (a hole, a slot's round end). The
    /// material lies left of a ring's run, so an arc turning anticlockwise
    /// has it round the centre.
    pub(crate) fn pin(&self) -> bool {
        self.sweep > 0.0
    }

    /// Whether it runs over edge `edge` (corner `edge` to the next) of a
    /// ring of `n` corners.
    pub(crate) fn has_edge(&self, edge: usize, n: usize) -> bool {
        n > 0 && (edge + n - self.first % n) % n < self.edges
    }

    /// Whether the direction `angle` (radians about the centre) points at
    /// the arc.
    pub(crate) fn covers(&self, angle: f64) -> bool {
        use std::f64::consts::TAU;
        if self.closed() {
            return true;
        }
        let t = if self.sweep > 0.0 {
            (angle - self.start).rem_euclid(TAU)
        } else {
            (self.start - angle).rem_euclid(TAU)
        };
        t <= self.sweep.abs() + 1e-9
    }

    /// Its corners in order, first to last (all of a round ring's).
    pub(crate) fn corners(&self, ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
        let n = ring.len();
        let count = if self.edges >= n { n } else { self.edges + 1 };
        (0..count).map(|k| ring[(self.first + k) % n]).collect()
    }
}

/// The arc through `points` when they lie on one circle no wider than
/// `widest` across, each edge turning the same way and no further than
/// [`ARC_EDGE_DEGREES`]: its centre, radius, start and sweep.
fn arc_through(points: &[[f64; 2]], widest: f64) -> Option<([f64; 2], f64, f64, f64)> {
    let (centre, radius, stray) = fit_circle(points)?;
    if !round(stray, radius) || radius * 2.0 > widest {
        return None;
    }
    let angle = |p: &[f64; 2]| (p[1] - centre[1]).atan2(p[0] - centre[0]);
    let limit = ARC_EDGE_DEGREES.to_radians();
    let mut sweep = 0.0;
    for pair in points.windows(2) {
        let (a, b) = (angle(&pair[0]), angle(&pair[1]));
        let turn =
            (b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        if turn.abs() > limit || turn.abs() < 1e-12 || (sweep != 0.0 && turn * sweep < 0.0) {
            return None;
        }
        sweep += turn;
    }
    Some((centre, radius, angle(&points[0]), sweep))
}

/// The arcs of a ring: the whole ring when it is round, else each longest
/// run of at least [`ARC_EDGES`] edges on one circle turning at least
/// [`ARC_DEGREES`] (a round-ended slot has two).
pub(crate) fn ring_arcs(ring: &[[f64; 2]]) -> Vec<RingArc> {
    let n = ring.len();
    if n < ARC_EDGES + 1 {
        return Vec::new();
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in ring {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let widest = (hi[0] - lo[0]).hypot(hi[1] - lo[1]) * 1.01;
    if n >= ROUND_CORNERS {
        if let Some((centre, radius, stray)) = fit_circle(ring) {
            if round(stray, radius) {
                let sweep = std::f64::consts::TAU.copysign(super::exact::ring_area(ring));
                let start = (ring[0][1] - centre[1]).atan2(ring[0][0] - centre[0]);
                return vec![RingArc {
                    first: 0,
                    edges: n,
                    centre,
                    radius,
                    start,
                    sweep,
                }];
            }
        }
    }
    // Start at the longest edge, which an arc of short edges does not run
    // over, so no arc is cut in two where the ring closes.
    let length = |i: usize| {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        (b[0] - a[0]).hypot(b[1] - a[1])
    };
    let longest = (0..n)
        .max_by(|a, b| length(*a).total_cmp(&length(*b)))
        .unwrap_or(0);
    let from = (longest + 1) % n;
    let at = |k: usize| ring[(from + k) % n];
    let mut out = Vec::new();
    let mut i = 0;
    while i + ARC_EDGES <= n {
        let mut best = None;
        let mut j = i + 2;
        while j <= n {
            let points: Vec<[f64; 2]> = (i..=j).map(at).collect();
            match arc_through(&points, widest) {
                Some(found) => best = Some((j, found)),
                None => break,
            }
            j += 1;
        }
        match best {
            Some((j, (centre, radius, start, sweep)))
                if j - i >= ARC_EDGES && sweep.abs() >= ARC_DEGREES.to_radians() =>
            {
                out.push(RingArc {
                    first: (from + i) % n,
                    edges: j - i,
                    centre,
                    radius,
                    start,
                    sweep,
                });
                i = j;
            }
            _ => i += 1,
        }
    }
    out
}

/// The round axis under a local point: the round wall of a hole, a pin or
/// a dowel there, from the exact solid's rings, else from the grid.
pub fn round_axis(stock: &Stock, solid: Option<&ExactSolid>, local: Vec3) -> Option<MateFace> {
    let axis = |centre: [f64; 2], radius: f64| MateFace {
        point: [centre[0], centre[1], local[2]],
        normal: [0.0, 0.0, 1.0],
        radius: Some((radius * 1e6).round() / 1e6),
    };
    if let Some(solid) = solid {
        let z = local[2].clamp(1e-6, solid.thickness - 1e-6);
        let slab = solid.levels.windows(2).position(|w| z >= w[0] && z < w[1]);
        if let Some(k) = slab {
            let xy = [local[0], local[1]];
            let nearest = solid.sections[k]
                .iter()
                .flat_map(|s| s.iter())
                .map(|ring| {
                    (
                        super::exact::edge_distance(&vec![vec![ring.clone()]], xy),
                        ring,
                    )
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((distance, ring)) = nearest {
                if distance <= RING_REACH {
                    let edge = nearest_edge(ring, xy);
                    let n = ring.len();
                    if let Some(arc) = ring_arcs(ring).iter().find(|a| a.has_edge(edge, n)) {
                        return Some(axis(arc.centre, arc.radius));
                    }
                }
            }
        }
    }
    grid_axis(stock, local)
        .or_else(|| grid_arc(stock, local))
        .map(|(centre, radius)| axis(centre, radius))
}

/// The edge of a ring nearest a point: its first corner's index.
fn nearest_edge(ring: &[[f64; 2]], p: [f64; 2]) -> usize {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let d = super::exact::edge_distance(&vec![vec![vec![ring[i], ring[(i + 1) % n]]]], p);
            (d, i)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0, |(_, i)| i)
}

/// The round region at a point's height on the grid: the hole the face
/// under the point looks into, else the pin it is the side of.
fn grid_axis(stock: &Stock, local: Vec3) -> Option<([f64; 2], f64)> {
    let face = pieces::face_at(stock, local)?;
    if face.normal[2].abs() > 0.7 {
        return None;
    }
    let s = stock;
    let cell = s.cell;
    let z = local[2];
    let solid_at = |i: usize| {
        s.floor[i] < s.z[i] - THROUGH
            && f64::from(s.floor[i]) - s.bottom <= z
            && z <= f64::from(s.z[i]) - s.bottom
    };
    let cell_of = |x: f64, y: f64| {
        let (fx, fy) = (x / cell, y / cell);
        (fx >= 0.0 && fy >= 0.0 && fx < s.width as f64 && fy < s.height as f64)
            .then(|| fy as usize * s.width + fx as usize)
    };
    let step = cell * 0.75;
    let into_void = cell_of(
        face.point[0] + face.normal[0] * step,
        face.point[1] + face.normal[1] * step,
    );
    let into_material = cell_of(
        face.point[0] - face.normal[0] * step,
        face.point[1] - face.normal[1] * step,
    );
    let tries = [(into_void, false), (into_material, true)];
    for (start, material) in tries {
        let Some(start) = start else {
            continue;
        };
        if solid_at(start) != material {
            continue;
        }
        if let Some(found) = closed_region(s, start, |i| solid_at(i) == material) {
            let points: Vec<[f64; 2]> = found
                .iter()
                .map(|i| {
                    [
                        ((i % s.width) as f64 + 0.5) * cell,
                        ((i / s.width) as f64 + 0.5) * cell,
                    ]
                })
                .collect();
            if let Some((centre, radius, _)) = fit_circle(&points) {
                // Round: every edge cell within a cell and a half of the circle.
                let stray = points
                    .iter()
                    .map(|p| ((p[0] - centre[0]).hypot(p[1] - centre[1]) - radius).abs())
                    .fold(0.0, f64::max);
                if stray <= cell * 1.5 && radius >= cell * 2.0 {
                    // The wall lies half a cell beyond the edge cells'
                    // middles, for a hole's void and a pin's material alike.
                    return Some((centre, radius + cell / 2.0));
                }
            }
        }
    }
    None
}

/// A round wall that is only part of a circle on the grid (the round end
/// of a slot or a tab): the edge cells round the wall under the point,
/// taken ever further out while they still lie on one circle turning at
/// least a sixth of the way round.
fn grid_arc(stock: &Stock, local: Vec3) -> Option<([f64; 2], f64)> {
    let face = pieces::face_at(stock, local)?;
    if face.normal[2].abs() > 0.7 {
        return None;
    }
    let s = stock;
    let (w, h, cell) = (s.width, s.height, s.cell);
    let z = local[2];
    let solid_at = |i: usize| {
        s.floor[i] < s.z[i] - THROUGH
            && f64::from(s.floor[i]) - s.bottom <= z
            && z <= f64::from(s.z[i]) - s.bottom
    };
    let cell_of = |x: f64, y: f64| {
        let (fx, fy) = (x / cell, y / cell);
        (fx >= 0.0 && fy >= 0.0 && fx < w as f64 && fy < h as f64)
            .then(|| fy as usize * w + fx as usize)
    };
    let middle = |i: usize| [((i % w) as f64 + 0.5) * cell, ((i / w) as f64 + 0.5) * cell];
    let step = cell * 0.75;
    let at = [face.point[0], face.point[1]];
    for (sign, material) in [(1.0, false), (-1.0, true)] {
        let Some(start) = cell_of(
            at[0] + sign * face.normal[0] * step,
            at[1] + sign * face.normal[1] * step,
        ) else {
            continue;
        };
        if solid_at(start) != material {
            continue;
        }
        let inside = |i: usize| solid_at(i) == material;
        let mut found = None;
        let mut reach = 4.0;
        while reach <= GRID_REACH as f64 / 4.0 {
            let limit = reach * cell;
            // The region's cells within reach of the point, and its edge cells.
            let mut seen = BTreeSet::from([start]);
            let mut queue = VecDeque::from([start]);
            let mut edge = Vec::new();
            while let Some(i) = queue.pop_front() {
                let (x, y) = (i % w, i / w);
                let mut is_edge = false;
                let near = [
                    (x > 0).then(|| i - 1),
                    (x + 1 < w).then(|| i + 1),
                    (y > 0).then(|| i - w),
                    (y + 1 < h).then(|| i + w),
                ];
                for j in near {
                    match j {
                        Some(j) if inside(j) => {
                            let m = middle(j);
                            if !seen.contains(&j) && (m[0] - at[0]).hypot(m[1] - at[1]) <= limit {
                                seen.insert(j);
                                queue.push_back(j);
                            }
                        }
                        _ => is_edge = true,
                    }
                }
                if is_edge {
                    edge.push(middle(i));
                }
            }
            let fits = fit_circle(&edge).filter(|(centre, radius, _)| {
                let stray = edge
                    .iter()
                    .map(|p| ((p[0] - centre[0]).hypot(p[1] - centre[1]) - radius).abs())
                    .fold(0.0, f64::max);
                stray <= cell * 1.5
                    && *radius >= cell * 2.0
                    && *radius <= limit * 2.0
                    && turned(&edge, *centre) >= std::f64::consts::FRAC_PI_3
            });
            match fits {
                Some((centre, radius, _)) => found = Some((centre, radius + cell / 2.0)),
                None if found.is_some() => break,
                None => {}
            }
            reach *= 1.5;
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// How far round a centre points reach: a whole turn less the widest gap
/// between their directions, radians.
fn turned(points: &[[f64; 2]], centre: [f64; 2]) -> f64 {
    let mut angles: Vec<f64> = points
        .iter()
        .map(|p| (p[1] - centre[1]).atan2(p[0] - centre[0]))
        .collect();
    if angles.len() < 2 {
        return 0.0;
    }
    angles.sort_by(f64::total_cmp);
    let mut gap = angles[0] + std::f64::consts::TAU - angles[angles.len() - 1];
    for pair in angles.windows(2) {
        gap = gap.max(pair[1] - pair[0]);
    }
    std::f64::consts::TAU - gap
}

/// The edge cells of the region `inside` holds that `start` lies in, when
/// the region does not reach the grid's edge or run wider than
/// [`GRID_REACH`] cells.
fn closed_region(s: &Stock, start: usize, inside: impl Fn(usize) -> bool) -> Option<Vec<usize>> {
    let (w, h) = (s.width, s.height);
    let (sx, sy) = (start % w, start / w);
    let mut seen = vec![false; w * h];
    let mut queue = VecDeque::from([start]);
    seen[start] = true;
    let mut edge = Vec::new();
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
            return None;
        }
        if x.abs_diff(sx) > GRID_REACH || y.abs_diff(sy) > GRID_REACH {
            return None;
        }
        let mut is_edge = false;
        for j in [i - 1, i + 1, i - w, i + w] {
            if inside(j) {
                if !seen[j] {
                    seen[j] = true;
                    queue.push_back(j);
                }
            } else {
                is_edge = true;
            }
        }
        if is_edge {
            edge.push(i);
        }
    }
    Some(edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(c: [f64; 2], r: f64, n: usize) -> Vec<[f64; 2]> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
            .collect()
    }

    #[test]
    fn a_circle_fits_its_own_points_and_a_square_does_not() {
        let (c, r, stray) = fit_circle(&circle([12.0, -4.0], 5.0, 64)).expect("fit");
        assert!((c[0] - 12.0).abs() < 1e-9 && (c[1] + 4.0).abs() < 1e-9);
        assert!((r - 5.0).abs() < 1e-9 && stray < 1e-9);
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let (_, r, stray) = fit_circle(&square).expect("fit");
        assert!(!round(stray, r) || square.len() < ROUND_CORNERS);
    }

    #[test]
    fn a_click_on_a_drilled_hole_gives_its_axis_exactly() {
        let mut outline = vec![vec![[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]]];
        let mut hole = circle([20.0, 15.0], 4.0, 128);
        hole.reverse();
        outline.push(hole);
        let solid = ExactSolid::new(vec![outline], &[], 18.0);
        let stock = Stock {
            width: 1,
            height: 1,
            cell: 1.0,
            origin_x: 0.0,
            origin_y: 0.0,
            top: 18.0,
            bottom: 0.0,
            z: vec![18.0],
            floor: vec![0.0],
            owner: vec![0],
            burn: Vec::new(),
        };
        let axis = round_axis(&stock, Some(&solid), [24.0, 15.0, 9.0]).expect("axis");
        assert!((axis.point[0] - 20.0).abs() < 1e-9 && (axis.point[1] - 15.0).abs() < 1e-9);
        assert!((axis.radius.unwrap_or(0.0) - 4.0).abs() < 1e-6);
        // The panel's straight outline is not round.
        assert!(round_axis(&stock, Some(&solid), [60.0, 20.0, 9.0]).is_none());
    }

    #[test]
    fn a_hole_on_the_grid_gives_its_middle() {
        let (w, h, cell) = (80, 80, 0.5);
        let mut z = vec![18.0_f32; w * h];
        let (cx, cy, r) = (20.0, 21.0, 6.0);
        for y in 0..h {
            for x in 0..w {
                let (px, py) = ((x as f64 + 0.5) * cell, (y as f64 + 0.5) * cell);
                if (px - cx).hypot(py - cy) < r {
                    z[y * w + x] = 0.0;
                }
            }
        }
        let stock = Stock {
            width: w,
            height: h,
            cell,
            origin_x: 0.0,
            origin_y: 0.0,
            top: 18.0,
            bottom: 0.0,
            z,
            floor: vec![0.0; w * h],
            owner: vec![0; w * h],
            burn: Vec::new(),
        };
        let axis = round_axis(&stock, None, [cx + r, cy, 9.0]).expect("axis");
        assert!(
            (axis.point[0] - cx).abs() < 0.2 && (axis.point[1] - cy).abs() < 0.2,
            "{axis:?}"
        );
        let radius = axis.radius.unwrap_or(0.0);
        assert!((radius - r).abs() < 0.6, "{radius}");
    }

    /// A slot from (x0, y) to (x1, y) with round ends of radius `r`,
    /// anticlockwise, `k` corners to each end.
    fn stadium(x0: f64, x1: f64, y: f64, r: f64, k: usize) -> Vec<[f64; 2]> {
        let mut out = Vec::new();
        for i in 0..=k {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / k as f64;
            out.push([x1 + r * a.cos(), y + r * a.sin()]);
        }
        for i in 0..=k {
            let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / k as f64;
            out.push([x0 + r * a.cos(), y + r * a.sin()]);
        }
        out
    }

    fn one_cell() -> Stock {
        Stock {
            width: 1,
            height: 1,
            cell: 1.0,
            origin_x: 0.0,
            origin_y: 0.0,
            top: 18.0,
            bottom: 0.0,
            z: vec![18.0],
            floor: vec![0.0],
            owner: vec![0],
            burn: Vec::new(),
        }
    }

    #[test]
    fn a_round_ended_slot_has_two_arcs_and_filleted_corners_four() {
        let slot = stadium(10.0, 30.0, 20.0, 4.0, 16);
        let arcs = ring_arcs(&slot);
        assert_eq!(arcs.len(), 2, "{arcs:?}");
        let mut centres: Vec<f64> = arcs.iter().map(|a| a.centre[0]).collect();
        centres.sort_by(f64::total_cmp);
        assert!((centres[0] - 10.0).abs() < 1e-6 && (centres[1] - 30.0).abs() < 1e-6);
        for arc in &arcs {
            assert!((arc.radius - 4.0).abs() < 1e-6);
            assert!((arc.sweep - std::f64::consts::PI).abs() < 1e-6, "{arc:?}");
            // Anticlockwise, material round the centre: a tab's round end.
            assert!(arc.pin() && !arc.closed());
            // Its straight sides are not on it; its middle is.
            let mid = [arc.centre[0], arc.centre[1]];
            let away = if mid[0] > 20.0 {
                0.0
            } else {
                std::f64::consts::PI
            };
            assert!(arc.covers(away) && !arc.covers(away + std::f64::consts::PI));
        }
        // The same slot run clockwise is a hole's: its ends are not pins.
        let mut hole = slot.clone();
        hole.reverse();
        assert!(ring_arcs(&hole).iter().all(|a| !a.pin()));
        // A rectangle with round corners: four quarter arcs.
        let mut filleted = Vec::new();
        for (cx, cy, from) in [
            (37.0, 3.0, -90.0_f64),
            (37.0, 17.0, 0.0),
            (3.0, 17.0, 90.0),
            (3.0, 3.0, 180.0),
        ] {
            for i in 0..=8 {
                let a = (from + 90.0 * f64::from(i) / 8.0).to_radians();
                filleted.push([cx + 3.0 * a.cos(), cy + 3.0 * a.sin()]);
            }
        }
        let arcs = ring_arcs(&filleted);
        assert_eq!(arcs.len(), 4, "{arcs:?}");
        assert!(arcs
            .iter()
            .all(|a| (a.sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-6));
        // A square's corners lie on a circle, but it has no arc.
        assert!(ring_arcs(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]).is_empty());
    }

    #[test]
    fn a_click_on_a_slots_round_end_gives_that_ends_axis() {
        let outline = vec![[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]];
        let mut slot = stadium(15.0, 40.0, 20.0, 4.2, 24);
        slot.reverse();
        let solid = ExactSolid::new(vec![vec![outline, slot]], &[], 18.0);
        let stock = one_cell();
        let axis = round_axis(&stock, Some(&solid), [44.2, 20.5, 9.0]).expect("the round end");
        assert!(
            (axis.point[0] - 40.0).abs() < 1e-6 && (axis.point[1] - 20.0).abs() < 1e-6,
            "{axis:?}"
        );
        assert!((axis.radius.unwrap_or(0.0) - 4.2).abs() < 1e-6);
        let other = round_axis(&stock, Some(&solid), [10.8, 19.0, 9.0]).expect("the other end");
        assert!((other.point[0] - 15.0).abs() < 1e-6, "{other:?}");
        // The slot's straight side is not round.
        assert!(round_axis(&stock, Some(&solid), [27.0, 24.2, 9.0]).is_none());
    }

    #[test]
    fn a_round_end_on_the_grid_gives_its_middle() {
        let (w, h, cell) = (120, 60, 0.5);
        let mut z = vec![18.0_f32; w * h];
        // A slot 30 mm long between its ends' middles, 12 mm wide.
        let (x0, x1, cy, r) = (12.0, 42.0, 15.0, 6.0);
        for y in 0..h {
            for x in 0..w {
                let (px, py) = ((x as f64 + 0.5) * cell, (y as f64 + 0.5) * cell);
                let along = px.clamp(x0, x1);
                if (px - along).hypot(py - cy) < r {
                    z[y * w + x] = 0.0;
                }
            }
        }
        let stock = Stock {
            width: w,
            height: h,
            cell,
            origin_x: 0.0,
            origin_y: 0.0,
            top: 18.0,
            bottom: 0.0,
            z,
            floor: vec![0.0; w * h],
            owner: vec![0; w * h],
            burn: Vec::new(),
        };
        let axis = round_axis(&stock, None, [x1 + r, cy, 9.0]).expect("axis");
        assert!(
            (axis.point[0] - x1).abs() < 0.3 && (axis.point[1] - cy).abs() < 0.3,
            "{axis:?}"
        );
        let radius = axis.radius.unwrap_or(0.0);
        assert!((radius - r).abs() < 0.6, "{radius}");
        // The straight side gives no axis.
        assert!(round_axis(&stock, None, [27.0, cy + r, 9.0]).is_none());
    }
}
