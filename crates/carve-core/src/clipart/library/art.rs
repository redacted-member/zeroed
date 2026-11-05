//! Drawing helpers the library's generators share: closed outlines as
//! point rings, smooth curves through a few points, unions and cut-outs,
//! and the reliefs grown from them.
//!
//! Every length is millimetres. A ring is a closed polygon without its
//! first point repeated; a set of rings is read by the even-odd rule, so a
//! ring inside another is a hole.

use std::f64::consts::{PI, TAU};

use crate::error::CoreError;
use crate::geom::{
    fit_arcs, flatten_path, offset::outline_open, reduce_subpath, subtract, weld, BBox, Mat3, Path,
    Point, Polyline, Region, SubPath, Tolerance,
};
use crate::model::field::{distance_to, Field};
use crate::model::shape::{create_shape, inside_cells, Grid, Profile, ShapeParams};

/// Cell size of every relief in the library, millimetres: fine enough that
/// a small ball nose finishing pass shows no steps at the clip's own size.
pub const CELL: f64 = 0.1;

/// How far a fitted curve may stray from the drawn one, millimetres.
pub const FIT: f64 = 0.02;

/// One closed outline.
pub type Ring = Vec<Point>;

/// Search words of an item.
pub type Tags = &'static [&'static str];

/// A named design: name, tags and the function that draws it.
pub type Design<T> = (&'static str, Tags, fn() -> T);

/// Leaf outlines with the base and tip of each leaf, in the same order.
pub type Leaves = (Vec<Ring>, Vec<(Point, Point)>);

/// Tolerance the helpers flatten curves to.
pub fn tol() -> Tolerance {
    Tolerance::FLATTEN
}

/// A point.
pub fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// A circle of radius `r` round `c`, `n` points.
pub fn circle(c: Point, r: f64, n: usize) -> Ring {
    ellipse(c, r, r, n)
}

/// An axis aligned ellipse.
pub fn ellipse(c: Point, rx: f64, ry: f64, n: usize) -> Ring {
    (0..n)
        .map(|k| {
            let t = TAU * k as f64 / n as f64;
            p(c.x + rx * t.cos(), c.y + ry * t.sin())
        })
        .collect()
}

/// A ring whose radius at angle `theta` (radians, from +X counter
/// clockwise) is `r(theta)`, centred on the origin.
pub fn polar(n: usize, r: impl Fn(f64) -> f64) -> Ring {
    (0..n)
        .map(|k| {
            let t = TAU * k as f64 / n as f64;
            let radius = r(t);
            p(radius * t.cos(), radius * t.sin())
        })
        .collect()
}

/// A regular polygon with `sides`, circumradius `r`, a corner straight up
/// turned by `turn` radians.
pub fn polygon(sides: usize, r: f64, turn: f64) -> Ring {
    (0..sides)
        .map(|k| {
            let t = PI / 2.0 + turn + TAU * k as f64 / sides as f64;
            p(r * t.cos(), r * t.sin())
        })
        .collect()
}

/// A star with `points` spikes of radius `outer`, notches at `inner`.
pub fn star(points: usize, outer: f64, inner: f64, turn: f64) -> Ring {
    (0..points * 2)
        .map(|k| {
            let t = PI / 2.0 + turn + PI * k as f64 / points as f64;
            let r = if k % 2 == 0 { outer } else { inner };
            p(r * t.cos(), r * t.sin())
        })
        .collect()
}

/// A rectangle centred on `c`.
pub fn rect(c: Point, w: f64, h: f64) -> Ring {
    let (a, b) = (w / 2.0, h / 2.0);
    vec![
        p(c.x - a, c.y - b),
        p(c.x + a, c.y - b),
        p(c.x + a, c.y + b),
        p(c.x - a, c.y + b),
    ]
}

/// A rectangle centred on `c` with corners rounded to `r`.
pub fn rounded_rect(c: Point, w: f64, h: f64, r: f64) -> Ring {
    let r = r.min(w / 2.0).min(h / 2.0);
    let (a, b) = (w / 2.0 - r, h / 2.0 - r);
    let mut out = Vec::new();
    for (k, (cx, cy)) in [(a, -b), (a, b), (-a, b), (-a, -b)].iter().enumerate() {
        let start = -PI / 2.0 + k as f64 * PI / 2.0;
        for s in 0..=8 {
            let t = start + PI / 2.0 * s as f64 / 8.0;
            out.push(p(c.x + cx + r * t.cos(), c.y + cy + r * t.sin()));
        }
    }
    out
}

/// A closed Catmull-Rom curve through `points`, `per` samples per span.
pub fn smooth(points: &[Point], per: usize) -> Ring {
    spline(points, per, true)
}

/// An open Catmull-Rom curve through `points`, `per` samples per span.
pub fn smooth_open(points: &[Point], per: usize) -> Vec<Point> {
    spline(points, per, false)
}

fn spline(points: &[Point], per: usize, closed: bool) -> Vec<Point> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let at = |i: i64| -> Point {
        if closed {
            points[i.rem_euclid(n as i64) as usize]
        } else {
            points[i.clamp(0, n as i64 - 1) as usize]
        }
    };
    let spans = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(spans * per + 1);
    for s in 0..spans as i64 {
        let (p0, p1, p2, p3) = (at(s - 1), at(s), at(s + 1), at(s + 2));
        for k in 0..per {
            let t = k as f64 / per as f64;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push(p(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
        }
    }
    if !closed {
        out.push(points[n - 1]);
    }
    out
}

/// `n` points evenly spaced along the closed ring.
pub fn resample(ring: &[Point], n: usize) -> Ring {
    let m = ring.len();
    let mut lengths = Vec::with_capacity(m + 1);
    let mut total = 0.0;
    lengths.push(0.0);
    for k in 0..m {
        total += ring[k].distance(ring[(k + 1) % m]);
        lengths.push(total);
    }
    let mut out = Vec::with_capacity(n);
    let mut seg = 0;
    for i in 0..n {
        let want = total * i as f64 / n as f64;
        while seg + 1 < m && lengths[seg + 1] < want {
            seg += 1;
        }
        let span = (lengths[seg + 1] - lengths[seg]).max(1e-12);
        let t = (want - lengths[seg]) / span;
        out.push(ring[seg].lerp(ring[(seg + 1) % m], t));
    }
    out
}

/// A symmetric closed outline from its right half: `half` runs from the
/// top of the centre line down the right side to the bottom of the centre
/// line; the left side is its mirror. Points are smoothed when `per` > 1.
pub fn mirrored(half: &[Point], per: usize) -> Ring {
    let mut full: Vec<Point> = half.to_vec();
    for q in half.iter().rev().skip(1).take(half.len().saturating_sub(2)) {
        full.push(p(-q.x, q.y));
    }
    if per > 1 {
        smooth(&full, per)
    } else {
        full
    }
}

/// `ring` moved by `m`.
pub fn moved(ring: &[Point], m: &Mat3) -> Ring {
    ring.iter().map(|q| m.apply(*q)).collect()
}

/// Every ring of `rings` moved by `m`.
pub fn moved_all(rings: &[Ring], m: &Mat3) -> Vec<Ring> {
    rings.iter().map(|r| moved(r, m)).collect()
}

/// Turn by `angle` radians about the origin, then shift by `(dx, dy)`.
pub fn place(angle: f64, dx: f64, dy: f64) -> Mat3 {
    Mat3::rotate(angle).then(&Mat3::translate(crate::geom::Vec2::new(dx, dy)))
}

/// Scale by `sx`, `sy` about the origin.
pub fn scaled(sx: f64, sy: f64) -> Mat3 {
    Mat3::scale(sx, sy)
}

/// `count` copies of `rings` turned evenly about the origin.
pub fn around(rings: &[Ring], count: usize, turn: f64) -> Vec<Ring> {
    (0..count)
        .flat_map(|k| moved_all(rings, &Mat3::rotate(turn + TAU * k as f64 / count as f64)))
        .collect()
}

/// A closed path for a ring.
pub fn ring_path(ring: &[Point]) -> Path {
    let mut sub = SubPath::open(ring[0]);
    for q in &ring[1..] {
        sub.line_to(*q);
    }
    sub.line_to(ring[0]);
    sub.closed = true;
    sub.into_path()
}

/// A clean closed path per ring: nodes closer than a few microns merged,
/// then lines and arcs fitted within [`FIT`] so curves carry few nodes.
pub fn paths(rings: &[Ring]) -> Vec<Path> {
    let mut clean: Vec<Ring> = Vec::with_capacity(rings.len());
    for ring in rings {
        let ring = deduped(ring, true);
        if ring.len() < 3 {
            continue;
        }
        // A loop that crosses itself (a tight curl of a tapered stroke) is
        // traced round its outside instead.
        if crosses_itself(&ring) {
            if let Ok(welded) = weld(&[ring_path(&ccw(&ring))], tol()) {
                clean.extend(rings_of(&welded).into_iter().map(|r| deduped(&r, true)));
                continue;
            }
        }
        clean.push(ring);
    }
    clean
        .iter()
        .filter(|r| r.len() >= 3)
        .map(|r| fitted(r, true))
        .collect()
}

/// Whether two edges of the closed ring cross, other than neighbors.
pub fn crosses_itself(ring: &[Point]) -> bool {
    let n = ring.len();
    let edge = |k: usize| (ring[k], ring[(k + 1) % n]);
    let side = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    for i in 0..n {
        let (p0, p1) = edge(i);
        let (lo_x, hi_x) = (p0.x.min(p1.x), p0.x.max(p1.x));
        let (lo_y, hi_y) = (p0.y.min(p1.y), p0.y.max(p1.y));
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (q0, q1) = edge(j);
            if q0.x.max(q1.x) < lo_x
                || q0.x.min(q1.x) > hi_x
                || q0.y.max(q1.y) < lo_y
                || q0.y.min(q1.y) > hi_y
            {
                continue;
            }
            let (d1, d2) = (side(p0, p1, q0), side(p0, p1, q1));
            let (d3, d4) = (side(q0, q1, p0), side(q0, q1, p1));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                return true;
            }
        }
    }
    false
}

/// `points` without nodes closer than a few microns to the one before
/// (and, for a loop, the last to the first).
fn deduped(points: &[Point], closed: bool) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for q in points {
        if out.last().is_none_or(|last| last.distance(*q) > 0.005) {
            out.push(*q);
        }
    }
    while closed && out.len() > 1 && out[0].distance(out[out.len() - 1]) <= 0.005 {
        out.pop();
    }
    out
}

/// Whether any segment of `sub` ends where it starts.
fn has_zero_length(sub: &SubPath) -> bool {
    let mut at = sub.start;
    sub.segments.iter().any(|seg| {
        let short = at.distance(seg.end()) < 1e-3;
        at = seg.end();
        short
    })
}

/// Lines and arcs fitted to `points`.
fn fitted(points: &[Point], closed: bool) -> Path {
    let tol = Tolerance::new(FIT).unwrap_or(Tolerance::FLATTEN);
    let line = Polyline::new(points.to_vec(), closed);
    match fit_arcs(&line, tol) {
        Some(sub) if !sub.segments.is_empty() => {
            // Merging can fold a whole circle into one arc that starts and
            // ends on the same node; keep the fitted pieces then.
            let reduced = reduce_subpath(&sub, tol);
            let mut sub = if has_zero_length(&reduced) {
                sub
            } else {
                reduced
            };
            sub.closed = closed;
            sub.into_path()
        }
        _ if closed => ring_path(points),
        _ => {
            let mut sub = SubPath::open(points[0]);
            for q in &points[1..] {
                sub.line_to(*q);
            }
            sub.into_path()
        }
    }
}

/// A clean open path through `points`, lines and arcs fitted as
/// [`paths`] does.
pub fn open_path(points: &[Point]) -> Path {
    fitted(&deduped(points, false), false)
}

fn rings_of(paths: &[Path]) -> Vec<Ring> {
    paths
        .iter()
        .flat_map(|path| flatten_path(path, tol()))
        .filter(|line| line.points.len() >= 3)
        .map(|line| {
            let mut pts = line.points;
            if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 1e-9 {
                pts.pop();
            }
            pts
        })
        .collect()
}

/// Twice the signed area of a ring, positive when counter clockwise.
pub fn signed_area2(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// `ring` running counter clockwise.
pub fn ccw(ring: &[Point]) -> Ring {
    let mut out = ring.to_vec();
    if signed_area2(&out) < 0.0 {
        out.reverse();
    }
    out
}

fn filled(rings: &[Ring]) -> Vec<Path> {
    rings
        .iter()
        .filter(|r| r.len() >= 3)
        .map(|r| ring_path(&ccw(r)))
        .collect()
}

/// The union of `rings`, each read on its own as a filled outline.
///
/// # Errors
/// [`CoreError::Geometry`] when there is nothing closed to weld.
pub fn union(rings: &[Ring]) -> Result<Vec<Ring>, CoreError> {
    Ok(rings_of(&weld(&filled(rings), tol())?))
}

/// The union of shapes whose rings keep their direction: outer rings
/// counter clockwise and holes clockwise, as [`cut`], [`band`] and
/// [`union`] answer them.
///
/// # Errors
/// [`CoreError::Geometry`] when there is nothing closed to weld.
pub fn merge(shapes: &[Vec<Ring>]) -> Result<Vec<Ring>, CoreError> {
    // One path holding every loop, so the fill rule sees the holes with
    // the loops round them rather than each loop on its own.
    let mut all = Path::new();
    for ring in shapes.iter().flatten().filter(|r| r.len() >= 3) {
        all.extend(ring_path(ring));
    }
    Ok(rings_of(&weld(&[all], tol())?))
}

/// What is left of `from` once the union of `cut` is taken away. `from`
/// is one loop, or a shape whose outer loops run counter clockwise and
/// holes clockwise, as [`union`] and [`band`] answer them.
///
/// # Errors
/// [`CoreError::Geometry`] when there is nothing closed to cut.
pub fn cut(from: &[Ring], cut: &[Ring]) -> Result<Vec<Ring>, CoreError> {
    // A single loop is filled whichever way it runs; several loops keep
    // their directions, so the holes of a welded shape stay holes.
    let a = if from.len() == 1 {
        weld(&filled(from), tol())?
    } else {
        let mut all = Path::new();
        for ring in from.iter().filter(|r| r.len() >= 3) {
            all.extend(ring_path(ring));
        }
        weld(&[all], tol())?
    };
    let b = weld(&filled(cut), tol())?;
    Ok(rings_of(&subtract(&a, &b, tol())?))
}

/// A band `width` wide along the open line `points`, round ended.
///
/// # Errors
/// [`CoreError::Geometry`] for a width that is not positive.
pub fn stroke(points: &[Point], width: f64) -> Result<Vec<Ring>, CoreError> {
    let path = open_path(points);
    let mut out = Vec::new();
    for sub in &path.subpaths {
        out.extend(rings_of(&outline_open(sub, width / 2.0, tol())?));
    }
    Ok(out)
}

/// A band `width` wide round the closed ring (an outline drawn with a
/// thick pen): the outer ring and its hole.
///
/// # Errors
/// [`CoreError::Geometry`] when the ring cannot be offset.
pub fn band(ring: &[Point], width: f64) -> Result<Vec<Ring>, CoreError> {
    let region = Region::from_paths(&[ring_path(ring)], tol());
    // The region's offset shrinks for a positive distance.
    let outer = rings_of(&[region.offset(-width / 2.0, tol()).as_path()]);
    let inner = rings_of(&[region.offset(width / 2.0, tol()).as_path()]);
    if inner.is_empty() {
        return union(&outer);
    }
    cut(&outer, &inner)
}

/// The ring and a copy `gap` inside it, for a double outline drawn as
/// lines.
pub fn double(ring: &[Point], gap: f64) -> Vec<Ring> {
    let mut out = vec![ring.to_vec()];
    out.extend(offset(&[ring.to_vec()], -gap));
    out
}

/// `rings` grown (positive) or shrunk (negative) by `by`.
pub fn offset(rings: &[Ring], by: f64) -> Vec<Ring> {
    let region = Region::from_paths(&paths(rings), tol());
    rings_of(&[region.offset(-by, tol()).as_path()])
}

/// The box round every ring.
pub fn bounds(rings: &[Ring]) -> Option<BBox> {
    rings
        .iter()
        .filter_map(|r| BBox::of_points(r.iter().copied()))
        .reduce(BBox::union)
}

/// Every ring moved so its box is centred on the origin.
pub fn centred(rings: &[Ring]) -> Vec<Ring> {
    match bounds(rings) {
        Some(b) => {
            let c = b.centre();
            moved_all(rings, &Mat3::translate(crate::geom::Vec2::new(-c.x, -c.y)))
        }
        None => rings.to_vec(),
    }
}

/// Every ring scaled about the origin so the box's longer side is `size`.
pub fn fit(rings: &[Ring], size: f64) -> Vec<Ring> {
    let rings = centred(rings);
    match bounds(&rings) {
        Some(b) if b.width().max(b.height()) > 0.0 => {
            let k = size / b.width().max(b.height());
            moved_all(&rings, &Mat3::scale(k, k))
        }
        _ => rings,
    }
}

// ------------------------------------------------------------------ reliefs

/// A grid of [`CELL`] cells covering `bbox` plus `margin` on every side.
/// The lattice is anchored on whole cells from the origin, so reliefs built
/// apart line up cell for cell when merged.
pub fn grid_over(bbox: BBox, margin: f64) -> Grid {
    let x0 = ((bbox.min.x - margin) / CELL).floor() * CELL;
    let y0 = ((bbox.min.y - margin) / CELL).floor() * CELL;
    let w = ((bbox.max.x + margin - x0) / CELL).ceil() as usize + 1;
    let h = ((bbox.max.y + margin - y0) / CELL).ceil() as usize + 1;
    Grid {
        width: w.max(1),
        height: h.max(1),
        cell: CELL,
        origin: p(x0, y0),
    }
}

/// A field of `width` by `height` millimetres centred on the origin with
/// heights `height_at(x, y)`; NaN or below zero leaves a cell empty.
pub fn field_fn(width: f64, height: f64, height_at: impl Fn(f64, f64) -> f64 + Sync) -> Field {
    let w = (width / CELL).round().max(1.0) as usize;
    let h = (height / CELL).round().max(1.0) as usize;
    let origin = p(
        -(((w - 1) / 2) as f64) * CELL,
        -(((h - 1) / 2) as f64) * CELL,
    );
    let mut field = Field::empty(w, h, CELL, origin);
    fill(&mut field, height_at);
    field.cropped().unwrap_or(field)
}

/// A tile exactly `width` by `height` millimetres for texturing: cell
/// centres sit half a cell in from the edges, so the tile repeats with no
/// seam when `height_at` repeats every `width` and `height`. Every cell
/// holds material.
pub fn tile_fn(width: f64, height: f64, height_at: impl Fn(f64, f64) -> f64 + Sync) -> Field {
    let w = (width / CELL).round().max(1.0) as usize;
    let h = (height / CELL).round().max(1.0) as usize;
    let mut field = Field::empty(w, h, CELL, p(CELL / 2.0, CELL / 2.0));
    fill(&mut field, |x, y| height_at(x, y).max(0.0));
    field
}

fn fill(field: &mut Field, height_at: impl Fn(f64, f64) -> f64 + Sync) {
    use rayon::prelude::*;
    let (w, cell, origin) = (field.width, field.cell, field.origin);
    field.z.par_chunks_mut(w).enumerate().for_each(|(j, row)| {
        for (i, z) in row.iter_mut().enumerate() {
            let v = height_at(origin.x + i as f64 * cell, origin.y + j as f64 * cell);
            if v.is_finite() && v >= 0.0 {
                *z = v as f32;
            }
        }
    });
}

/// Create Shape on `rings`: a relief with the given section and edge
/// angle, scaled so its highest point is `height` when that is positive.
///
/// # Errors
/// [`CoreError::Validation`] when the rings enclose no cell.
pub fn shape(
    rings: &[Ring],
    profile: Profile,
    angle: f64,
    height: f64,
) -> Result<Field, CoreError> {
    let bbox = bounds(rings).ok_or_else(|| CoreError::validation("the outline is empty"))?;
    let grid = grid_over(bbox, CELL * 2.0);
    let region = Region::from_paths(&paths(rings), tol());
    let params = ShapeParams {
        profile,
        angle,
        scale: height > 0.0,
        scale_height: height.max(0.1),
        ..ShapeParams::default()
    };
    let field = create_shape(&region, &grid, &params, tol())?;
    let mut out = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    out.z = field.z;
    out.cropped()
        .ok_or_else(|| CoreError::validation("the outline encloses no cell"))
}

/// A rounded relief of `rings`, `height` high.
///
/// # Errors
/// As [`shape`].
pub fn dome(rings: &[Ring], height: f64) -> Result<Field, CoreError> {
    shape(rings, Profile::Round, 90.0, height)
}

/// A soft, pillowy relief of `rings`, `height` high: the distance to the
/// edge smoothed out so no crease runs along the middle of the shape, then
/// rounded over at the edge.
///
/// # Errors
/// [`CoreError::Validation`] when the rings enclose no cell.
pub fn puffy(rings: &[Ring], height: f64) -> Result<Field, CoreError> {
    let bbox = bounds(rings).ok_or_else(|| CoreError::validation("the outline is empty"))?;
    let grid = grid_over(bbox, CELL * 2.0);
    let (inside, depth) = inside_and_depth(rings, &grid);
    let deepest = depth.iter().copied().fold(0.0, f64::max);
    if deepest <= 0.0 {
        return Err(CoreError::validation("the outline encloses no cell"));
    }
    let radius = ((deepest / 2.0) / CELL).round().max(1.0) as usize;
    let mut soft = depth.clone();
    for _ in 0..3 {
        soft = box_blur(&soft, &inside, grid.width, grid.height, radius);
    }
    // Keep the edge where it is: blend back to the true distance near it.
    let top = soft.iter().copied().fold(0.0, f64::max).max(1e-9);
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for k in 0..inside.len() {
        if inside[k] {
            let edge = (depth[k] / (deepest * 0.35)).min(1.0);
            let t = (soft[k] / top).min(1.0) * edge + (1.0 - edge) * (depth[k] / deepest);
            field.z[k] = (height * round_cap(t.max(0.0))) as f32;
        }
    }
    field
        .cropped()
        .ok_or_else(|| CoreError::validation("the outline encloses no cell"))
}

/// A box blur of `values` over the inside cells, `radius` cells each way,
/// rows then columns; outside cells count as zero.
fn box_blur(values: &[f64], inside: &[bool], w: usize, h: usize, radius: usize) -> Vec<f64> {
    let pass = |src: &[f64], along_rows: bool| -> Vec<f64> {
        let mut out = vec![0.0; src.len()];
        let (lines, len) = if along_rows { (h, w) } else { (w, h) };
        let at = |line: usize, k: usize| {
            if along_rows {
                line * w + k
            } else {
                k * w + line
            }
        };
        let span = (2 * radius + 1) as f64;
        for line in 0..lines {
            let mut sum = 0.0;
            for k in 0..=radius.min(len - 1) {
                sum += src[at(line, k)];
            }
            for k in 0..len {
                out[at(line, k)] = sum / span;
                if k + radius + 1 < len {
                    sum += src[at(line, k + radius + 1)];
                }
                if k >= radius {
                    sum -= src[at(line, k - radius)];
                }
            }
        }
        out
    };
    let rows = pass(values, true);
    let mut both = pass(&rows, false);
    for (v, &i) in both.iter_mut().zip(inside) {
        if !i {
            *v = 0.0;
        }
    }
    both
}

/// Which cells of `grid` lie inside `rings`, and each inside cell's
/// distance to the edge in millimetres (zero outside).
pub fn inside_and_depth(rings: &[Ring], grid: &Grid) -> (Vec<bool>, Vec<f64>) {
    let region = Region::from_paths(&paths(rings), tol());
    let inside = inside_cells(&region, grid, tol());
    let outside: Vec<bool> = inside.iter().map(|c| !c).collect();
    let d = distance_to(&outside, grid.width, grid.height);
    let depth = d
        .iter()
        .zip(&inside)
        .map(|(d, &i)| {
            if i {
                (d - 0.5).max(0.0) * grid.cell
            } else {
                0.0
            }
        })
        .collect();
    (inside, depth)
}

/// A relief of `rings` whose height at each cell is `profile(depth)`,
/// depth being the distance in millimetres from the edge. A NaN from the
/// profile leaves the cell empty.
///
/// # Errors
/// [`CoreError::Validation`] when the rings enclose no cell.
pub fn by_depth(rings: &[Ring], profile: impl Fn(f64) -> f64) -> Result<Field, CoreError> {
    let bbox = bounds(rings).ok_or_else(|| CoreError::validation("the outline is empty"))?;
    let grid = grid_over(bbox, CELL * 2.0);
    let (inside, depth) = inside_and_depth(rings, &grid);
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for k in 0..inside.len() {
        if inside[k] {
            let z = profile(depth[k]);
            if z.is_finite() {
                field.z[k] = z.max(0.0) as f32;
            }
        }
    }
    field
        .cropped()
        .ok_or_else(|| CoreError::validation("the outline encloses no cell"))
}

/// A field on the same lattice as `a` and `b` holding, cell by cell, the
/// higher of the two (material where either has some).
pub fn highest(a: &Field, b: &Field) -> Field {
    merge_fields(a, b, f32::max)
}

/// `a` and `b` added cell by cell where both have material, else whichever
/// has.
pub fn added(a: &Field, b: &Field) -> Field {
    merge_fields(a, b, |x, y| x + y)
}

fn merge_fields(a: &Field, b: &Field, op: impl Fn(f32, f32) -> f32) -> Field {
    let cell = a.cell;
    let ax1 = a.origin.x + (a.width as f64 - 1.0) * cell;
    let ay1 = a.origin.y + (a.height as f64 - 1.0) * cell;
    let bx1 = b.origin.x + (b.width as f64 - 1.0) * cell;
    let by1 = b.origin.y + (b.height as f64 - 1.0) * cell;
    let x0 = a.origin.x.min(b.origin.x);
    let y0 = a.origin.y.min(b.origin.y);
    let w = ((ax1.max(bx1) - x0) / cell).round() as usize + 1;
    let h = ((ay1.max(by1) - y0) / cell).round() as usize + 1;
    let mut out = Field::empty(w, h, cell, p(x0, y0));
    let put = |out: &mut Field, f: &Field, first: bool| {
        let di = ((f.origin.x - x0) / cell).round() as usize;
        let dj = ((f.origin.y - y0) / cell).round() as usize;
        for j in 0..f.height {
            for i in 0..f.width {
                let v = f.z[j * f.width + i];
                if v.is_nan() {
                    continue;
                }
                let slot = &mut out.z[(j + dj) * w + i + di];
                *slot = if first || slot.is_nan() {
                    v
                } else {
                    op(*slot, v)
                };
            }
        }
    };
    put(&mut out, a, true);
    put(&mut out, b, false);
    out
}

/// `field` smoothed with a small weighted blur over the cells that hold
/// material, `passes` times: one cell spikes, pits and creases sharper than
/// a cutter can follow are eased, empty cells stay empty and edges stay
/// where they are.
pub fn soften(field: Field, passes: usize) -> Field {
    blur(field, passes, false)
}

/// [`soften`] for a tile that repeats: the blur reads across each edge from
/// the opposite one, so copies laid side by side still meet without a seam.
pub fn soften_tile(field: Field, passes: usize) -> Field {
    blur(field, passes, true)
}

fn blur(mut field: Field, passes: usize, wrap: bool) -> Field {
    let (w, h) = (field.width, field.height);
    for _ in 0..passes {
        let src = field.z.clone();
        for j in 0..h {
            for i in 0..w {
                let k = j * w + i;
                if src[k].is_nan() {
                    continue;
                }
                let (mut sum, mut weight) = (0.0f32, 0.0f32);
                for dj in -1i64..=1 {
                    for di in -1i64..=1 {
                        let (mut x, mut y) = (i as i64 + di, j as i64 + dj);
                        if wrap {
                            x = x.rem_euclid(w as i64);
                            y = y.rem_euclid(h as i64);
                        } else if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                            continue;
                        }
                        let v = src[y as usize * w + x as usize];
                        if v.is_nan() {
                            continue;
                        }
                        let wt = match (di == 0, dj == 0) {
                            (true, true) => 4.0,
                            (true, false) | (false, true) => 2.0,
                            _ => 1.0,
                        };
                        sum += v * wt;
                        weight += wt;
                    }
                }
                field.z[k] = sum / weight;
            }
        }
    }
    field
}

/// A band along the open line `points` whose width runs from `start` to
/// `end`, with a round cap at the wide end and a point at the narrow one.
pub fn tapered(points: &[Point], start: f64, end: f64) -> Ring {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let mut lengths = vec![0.0];
    for k in 1..n {
        lengths.push(lengths[k - 1] + points[k - 1].distance(points[k]));
    }
    let total = lengths[n - 1].max(1e-9);
    let normal = |k: usize| {
        let (a, b) = if k == 0 {
            (points[0], points[1])
        } else if k == n - 1 {
            (points[n - 2], points[n - 1])
        } else {
            (points[k - 1], points[k + 1])
        };
        let d = a.to(b);
        let len = d.length().max(1e-12);
        (-d.y / len, d.x / len)
    };
    let half = |k: usize| {
        let t = lengths[k] / total;
        0.5 * (start + (end - start) * t)
    };
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for (k, q) in points.iter().enumerate() {
        let (nx, ny) = normal(k);
        let w = half(k);
        left.push(p(q.x + nx * w, q.y + ny * w));
        right.push(p(q.x - nx * w, q.y - ny * w));
    }
    let mut ring = left;
    ring.extend(right.into_iter().rev());
    // A round cap at the start, from the right side back to the left.
    let (nx, ny) = normal(0);
    let w0 = half(0);
    let a0 = ny.atan2(nx);
    for s in 1..12 {
        let a = a0 + PI - PI * s as f64 / 12.0;
        ring.push(p(points[0].x + w0 * a.cos(), points[0].y + w0 * a.sin()));
    }
    ring
}

/// `extra` added onto `base` where `base` has material, clamped at zero;
/// `extra` outside `base` is dropped.
pub fn added_onto(base: &Field, extra: &Field) -> Field {
    let mut out = base.clone();
    for j in 0..base.height {
        for i in 0..base.width {
            let k = j * base.width + i;
            if base.z[k].is_nan() {
                continue;
            }
            let v = extra.at(base.centre(i, j));
            if !v.is_nan() {
                out.z[k] = (base.z[k] + v.max(0.0)).max(0.0);
            }
        }
    }
    out
}

/// `field` with its heights times `k`.
pub fn lifted(mut field: Field, k: f64) -> Field {
    let k = k as f32;
    for z in &mut field.z {
        *z *= k;
    }
    field
}

/// `field` with `by` added to every height.
pub fn raised(mut field: Field, by: f64) -> Field {
    let by = by as f32;
    for z in &mut field.z {
        *z += by;
    }
    field
}

/// A round tube of radius `radius` along each line of points, each point
/// carrying the height of the tube's axis above the base: `(x, y, z)`.
/// Where lines cross, the higher tube wins, which draws over and under
/// crossings from the heights alone.
pub fn tubes(lines: &[Vec<(Point, f64)>], radius: f64) -> Result<Field, CoreError> {
    let bbox = lines
        .iter()
        .flat_map(|l| l.iter().map(|(q, _)| *q))
        .fold(None::<BBox>, |acc, q| {
            let b = BBox::of_points([q]);
            match (acc, b) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (None, b) => b,
                (a, None) => a,
            }
        })
        .ok_or_else(|| CoreError::validation("there is no line to follow"))?;
    let grid = grid_over(bbox, radius + CELL * 2.0);
    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    let r2 = radius * radius;
    for line in lines {
        for pair in line.windows(2) {
            let ((a, za), (b, zb)) = (pair[0], pair[1]);
            let lo_x = a.x.min(b.x) - radius;
            let hi_x = a.x.max(b.x) + radius;
            let lo_y = a.y.min(b.y) - radius;
            let hi_y = a.y.max(b.y) + radius;
            let i0 = (((lo_x - grid.origin.x) / CELL).floor().max(0.0)) as usize;
            let i1 = ((((hi_x - grid.origin.x) / CELL).ceil()) as usize).min(grid.width - 1);
            let j0 = (((lo_y - grid.origin.y) / CELL).floor().max(0.0)) as usize;
            let j1 = ((((hi_y - grid.origin.y) / CELL).ceil()) as usize).min(grid.height - 1);
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let len2 = (ex * ex + ey * ey).max(1e-12);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let x = grid.origin.x + i as f64 * CELL;
                    let y = grid.origin.y + j as f64 * CELL;
                    let t = (((x - a.x) * ex + (y - a.y) * ey) / len2).clamp(0.0, 1.0);
                    let (dx, dy) = (x - a.x - t * ex, y - a.y - t * ey);
                    let d2 = dx * dx + dy * dy;
                    if d2 >= r2 {
                        continue;
                    }
                    let z = (za + t * (zb - za) + (r2 - d2).sqrt()) as f32;
                    let slot = &mut field.z[j * grid.width + i];
                    if slot.is_nan() || z > *slot {
                        *slot = z;
                    }
                }
            }
        }
    }
    field
        .cropped()
        .ok_or_else(|| CoreError::validation("the tube covers no cell"))
}

/// Smooth step from 0 at `a` to 1 at `b`.
pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A round cap: `height` times the circle section at `t` in 0..1 from the
/// edge to the middle.
pub fn round_cap(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (1.0 - t) * (1.0 - t)).sqrt()
}
