//! Where a piece let go in the Assembly View lands: flush against the face
//! under the pointer (a piece's top, underside or a wall, else the ground).
//! Of the six sides of the dropped piece's part box, the one that faces
//! most nearly the other way turns to face the face and its middle sits on
//! the point; a piece that already faces it that way keeps its turns. When
//! the piece would then run into any piece (a pocket, a piece in the way),
//! it slides along the face the least that clears them all (one of four
//! ways, never down, at most its own size), so it stays against the face's
//! plane; only when no slide clears does it move out along the normal.

use crate::cam::sim::Stock;

use super::pieces::{self, face_at, MateOptions, PickedFace, Piece, PlacedRelief};
use super::pose::{self, Pose, Vec3};

/// Sides this close to facing each other (cosine of 2 degrees) keep the
/// dropped piece's turns.
const FACING: f64 = 0.999_39;
/// Coarse steps tried along each way out of an overlap.
const STEPS: usize = 48;
/// Halvings of the last coarse step, down to where the overlap ends.
const HALVINGS: usize = 8;
/// Cell size of the copies the coarse search runs on, mm.
const COARSE_CELL: f64 = 1.0;
/// How far one piece may sink into another and still count as touching, mm.
pub const TOUCH: f64 = 0.002;

/// The piece a drop lands on: its pose and relief.
pub struct Surface<'a> {
    /// Its local frame to assembly space.
    pub pose: Pose,
    /// Its relief, everything but its part cut away.
    pub stock: &'a Stock,
}

/// The side of a part box of `size` along `axis`, facing `sign`: its
/// middle and its outward normal, in the piece's frame.
fn box_side(size: Vec3, axis: usize, sign: f64) -> PickedFace {
    let mut point = pose::scale(size, 0.5);
    point[axis] = if sign > 0.0 { size[axis] } else { 0.0 };
    let mut normal = [0.0; 3];
    normal[axis] = sign;
    PickedFace { point, normal }
}

/// A unit direction along a face of normal `n`: world X on a level face,
/// world Z on an upright one.
fn along(n: Vec3) -> Vec3 {
    let guide = if n[2].abs() > 0.7 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    pose::normalized(pose::sub(guide, pose::scale(n, pose::dot(guide, n))))
}

fn moved(piece: &Piece, by: Vec3) -> Piece {
    Piece {
        position: pose::add(piece.position, by).map(|v| (v * 1e9).round() / 1e9),
        ..*piece
    }
}

/// Where `moving` (part box `size`, relief `stock`) lands when let go at
/// the assembly `point`, on `onto` (the piece under the pointer) or on the
/// ground (Z up) when that is `None` or has no face there, kept clear of
/// every piece in `others`.
pub fn drop_place(
    onto: Option<&Surface>,
    point: Vec3,
    moving: &Piece,
    size: Vec3,
    stock: &Stock,
    others: &[PlacedRelief],
) -> Piece {
    let ground = || {
        (
            Pose::translation([0.0; 3]),
            PickedFace {
                point,
                normal: [0.0, 0.0, 1.0],
            },
        )
    };
    let (target_pose, target) = onto
        .and_then(|s| face_at(s.stock, s.pose.inverse().apply(point)).map(|face| (s.pose, face)))
        .unwrap_or_else(ground);
    let at = target_pose.apply(target.point);
    let normal = pose::normalized(target_pose.apply_vector(target.normal));

    // The side facing most nearly against the face.
    let now = Pose::euler(moving.rotation);
    let (side, facing) = (0..3)
        .flat_map(|axis| [-1.0, 1.0].map(|sign| box_side(size, axis, sign)))
        .map(|side| {
            let d = pose::dot(now.apply_vector(side.normal), normal);
            (side, d)
        })
        .fold(None::<(PickedFace, f64)>, |best, (side, d)| match best {
            Some((_, b)) if b <= d + 1e-12 => best,
            _ => Some((side, d)),
        })
        .unwrap_or((box_side(size, 2, -1.0), -1.0));
    let placed = if facing <= -FACING {
        let lever = now.apply_vector(pose::sub(side.point, pose::scale(size, 0.5)));
        moved(
            &Piece {
                position: pose::sub(at, lever),
                ..*moving
            },
            [0.0; 3],
        )
    } else {
        pieces::mate(
            &target_pose,
            &target,
            moving,
            size,
            &side,
            MateOptions {
                offset: 0.0,
                turn: None,
            },
        )
        .0
    };

    let placed_relief = |piece: &Piece| PlacedRelief {
        key: piece.key(),
        stock,
        pose: piece.pose(size),
    };
    // Fine: any surface sunk in.
    let sunk = |piece: &Piece| {
        let me = placed_relief(piece);
        others.iter().any(|other| {
            pieces::runs_into(&me, other, TOUCH) || pieces::runs_into(other, &me, TOUCH)
        })
    };
    if !sunk(&placed) {
        return placed;
    }
    // Far enough along any way to clear every box.
    let mut lo = pieces::piece_bounds(&placed, size)[0];
    let mut hi = pieces::piece_bounds(&placed, size)[1];
    for other in others {
        let [a, b] = other.bounds();
        for k in 0..3 {
            lo[k] = lo[k].min(a[k]);
            hi[k] = hi[k].max(b[k]);
        }
    }
    let reach = pose::sub(hi, lo).iter().map(|v| v * v).sum::<f64>().sqrt() + 1.0;
    // Coarse: material running into material, on copies with larger cells.
    let rough = coarse(stock);
    let rough_others: Vec<Stock> = others.iter().map(|o| coarse(o.stock)).collect();
    let clashes = |piece: &Piece| {
        let me = PlacedRelief {
            key: piece.key(),
            stock: &rough,
            pose: piece.pose(size),
        };
        others.iter().zip(&rough_others).any(|(other, s)| {
            let them = PlacedRelief {
                key: other.key,
                stock: s,
                pose: other.pose,
            };
            pieces::overlaps(&me, &them)
        })
    };
    let (way, from) = if clashes(&placed) {
        // A slide as far as the piece is across.
        let [a, b] = pieces::piece_bounds(&placed, size);
        let across = pose::sub(b, a).iter().map(|v| v * v).sum::<f64>().sqrt();
        coarse_way(&placed, normal, across, reach, &clashes)
    } else {
        (normal, 0.0)
    };
    // Then the fine test, out along that way from where the coarse one
    // clears to where nothing is sunk in.
    let cells = others
        .iter()
        .map(|o| o.stock.cell)
        .fold(stock.cell, f64::max);
    let at = |d: f64| moved(&placed, pose::scale(way, d));
    let (mut near, mut clear) = (from, from + 2.0 * cells);
    let mut step = 2.0 * cells;
    while sunk(&at(clear)) {
        if clear > from + reach {
            return moved(&placed, pose::scale(normal, reach));
        }
        near = clear;
        step *= 2.0;
        clear += step;
    }
    if !sunk(&at(near)) {
        return at(near);
    }
    while clear - near > TOUCH / 2.0 {
        let mid = (near + clear) / 2.0;
        if sunk(&at(mid)) {
            near = mid;
        } else {
            clear = mid;
        }
    }
    at(clear)
}

/// A copy of `stock` with cells about [`COARSE_CELL`] across, each
/// taking the heights of the cell at its middle.
fn coarse(stock: &Stock) -> Stock {
    let k = ((COARSE_CELL / stock.cell).floor() as usize).max(1);
    let width = stock.width.div_ceil(k);
    let height = stock.height.div_ceil(k);
    let mut z = Vec::with_capacity(width * height);
    let mut floor = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let fx = (x * k + k / 2).min(stock.width - 1);
            let fy = (y * k + k / 2).min(stock.height - 1);
            let i = fy * stock.width + fx;
            z.push(stock.z[i]);
            floor.push(stock.floor[i]);
        }
    }
    Stock {
        width,
        height,
        cell: stock.cell * k as f64,
        origin_x: stock.origin_x,
        origin_y: stock.origin_y,
        top: stock.top,
        bottom: stock.bottom,
        z,
        floor,
        owner: vec![0; width * height],
        burn: Vec::new(),
    }
}

/// The way out of an overlap that clears it soonest by the coarse test,
/// and how far: sliding along the face in one of four ways (never down)
/// within `slide`, so the piece stays against the face's plane; else out
/// along the face's `normal` within `reach`.
fn coarse_way(
    placed: &Piece,
    normal: Vec3,
    slide: f64,
    reach: f64,
    clashes: &dyn Fn(&Piece) -> bool,
) -> (Vec3, f64) {
    let t = along(normal);
    let u = pose::cross(normal, t);
    let slides: Vec<Vec3> = [t, pose::scale(t, -1.0), u, pose::scale(u, -1.0)]
        .into_iter()
        .filter(|w| w[2] > -0.5)
        .collect();
    nearest_clear(placed, &slides, slide, clashes)
        .or_else(|| nearest_clear(placed, &[normal], reach, clashes))
        .unwrap_or((normal, reach))
}

/// The first of `ways` to clear the coarse test within `reach`, and how
/// far along it.
fn nearest_clear(
    placed: &Piece,
    ways: &[Vec3],
    reach: f64,
    clashes: &dyn Fn(&Piece) -> bool,
) -> Option<(Vec3, f64)> {
    let step = reach / STEPS as f64;
    for k in 1..=STEPS {
        let far = k as f64 * step;
        let mut best: Option<(Vec3, f64)> = None;
        for way in ways {
            if clashes(&moved(placed, pose::scale(*way, far))) {
                continue;
            }
            // The overlap ends between the last step and this one.
            let (mut near, mut clear) = (far - step, far);
            for _ in 0..HALVINGS {
                let mid = (near + clear) / 2.0;
                if clashes(&moved(placed, pose::scale(*way, mid))) {
                    near = mid;
                } else {
                    clear = mid;
                }
            }
            if best.is_none_or(|(_, d)| clear < d) {
                best = Some((*way, clear));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}
