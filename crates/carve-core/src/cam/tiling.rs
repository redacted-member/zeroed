//! Toolpath tiling: cutting a job longer or wider than the machine in
//! pieces. The job is split into tiles no bigger than the machine can reach;
//! for each tile the toolpaths are clipped to it and moved so the tile's
//! own corner sits on the job origin, and the material is moved along
//! between tiles.
//!
//! Three layouts: individual tiles split the job both ways into separate
//! pieces; feeding through in X or in Y splits it one way only, for one long
//! board moved along the machine between setups. Each tile reaches back
//! into the tile before it by the overlap, so a tile's cuts can start a
//! little below its corner: the overlap is cut at negative coordinates.
//!
//! Clipped cuts leave and enter the tile through its edges; between two
//! pieces the tool lifts to the safe height, travels across and plunges
//! again, so no cut is made outside the tile. Arcs that cross an edge stay
//! arcs.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{BBox, Point};

use super::estimate::TimeEstimate;
use super::toolpath::{BankFiring, Move, MoveKind, Point3, RateChange, Toolpath};

/// How the job is divided into tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TilingMethod {
    /// Split along X and Y into separate pieces.
    #[default]
    Individual,
    /// One strip fed through along X: only the tile width applies.
    FeedX,
    /// One strip fed through along Y: only the tile height applies.
    FeedY,
}

/// How the job is divided. A size of zero leaves that axis whole.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TileSpec {
    /// The layout.
    pub method: TilingMethod,
    /// Largest tile width along X, in millimetres.
    pub tile_width: f64,
    /// Largest tile height along Y, in millimetres.
    pub tile_height: f64,
    /// How far each tile reaches back into the one before it, in
    /// millimetres, so cuts meet without a seam.
    pub overlap: f64,
    /// Put the part size tile that is left over first instead of last.
    pub smallest_first: bool,
}

impl Default for TileSpec {
    fn default() -> Self {
        TileSpec {
            method: TilingMethod::Individual,
            tile_width: 0.0,
            tile_height: 0.0,
            overlap: 0.0,
            smallest_first: false,
        }
    }
}

/// The document's tiling: whether it is on, how the job is divided and
/// which tile the views show.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tiling {
    /// Tiling is on: the views show the active tile and saving divides the
    /// toolpaths into tiles.
    pub enabled: bool,
    /// The layout and sizes.
    pub spec: TileSpec,
    /// Index into [`tiles`] of the tile the views show.
    pub active: usize,
    /// Draw the active tile's moves where they sit in the finished job
    /// rather than moved to the origin where they are cut.
    pub original_position: bool,
}

impl Default for Tiling {
    fn default() -> Self {
        Tiling {
            enabled: false,
            spec: TileSpec {
                method: TilingMethod::Individual,
                tile_width: 600.0,
                tile_height: 600.0,
                overlap: 0.0,
                smallest_first: false,
            },
            active: 0,
            original_position: false,
        }
    }
}

impl Tiling {
    /// True for the default, which the file leaves out.
    pub fn is_default(&self) -> bool {
        *self == Tiling::default()
    }
}

/// One tile: the part of the job it cuts and the shift that brings it to
/// the origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    /// Row and column, from the job's bottom left, from 0.
    pub row: usize,
    pub column: usize,
    /// The tile's own area, without the overlap, in job millimetres.
    pub nominal: BBox,
    /// The area the tile cuts, overlap included, in job millimetres.
    pub area: BBox,
    /// Added to X and Y of every move in the tile.
    pub shift: (f64, f64),
}

impl Tile {
    /// The same tile with no shift: its cuts stay where they are in the job.
    #[must_use]
    pub fn in_place(mut self) -> Tile {
        self.shift = (0.0, 0.0);
        self
    }
}

/// Most tiles along one axis, a guard against a tiny tile size.
const MAX_TILES_PER_AXIS: usize = 1000;
/// Chord tolerance for arcs broken into lines before clipping, millimetres.
const ARC_TOLERANCE: f64 = 0.01;
/// Arc pieces shorter than this sweep, in radians, are dropped.
const MIN_SWEEP: f64 = 1e-9;

/// The tiles covering `job`, in rows from the bottom and columns from the
/// left.
///
/// # Errors
/// Returns [`CoreError::Validation`] for negative or non-finite sizes, an
/// overlap as big as a tile, or more than a thousand tiles along an axis.
pub fn tiles(job: BBox, spec: &TileSpec) -> Result<Vec<Tile>, CoreError> {
    for (value, what) in [
        (spec.tile_width, "tile width"),
        (spec.tile_height, "tile height"),
        (spec.overlap, "tile overlap"),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(CoreError::validation(format!(
                "the {what} must be zero or more"
            )));
        }
    }
    let (width, height) = match spec.method {
        TilingMethod::Individual => (spec.tile_width, spec.tile_height),
        TilingMethod::FeedX => (spec.tile_width, 0.0),
        TilingMethod::FeedY => (0.0, spec.tile_height),
    };
    let columns = split(job.min.x, job.width(), width, spec)?;
    let rows = split(job.min.y, job.height(), height, spec)?;
    let mut out = Vec::with_capacity(columns.len() * rows.len());
    for (row, (y0, y1)) in rows.iter().enumerate() {
        for (column, (x0, x1)) in columns.iter().enumerate() {
            let reach_x = if column == 0 { 0.0 } else { spec.overlap };
            let reach_y = if row == 0 { 0.0 } else { spec.overlap };
            out.push(Tile {
                row,
                column,
                nominal: BBox::new(Point::new(*x0, *y0), Point::new(*x1, *y1)),
                area: BBox::new(Point::new(x0 - reach_x, y0 - reach_y), Point::new(*x1, *y1)),
                shift: (job.min.x - x0, job.min.y - y0),
            });
        }
    }
    Ok(out)
}

/// The spans one axis of `length` from `start` is cut into, without the
/// overlap.
fn split(
    start: f64,
    length: f64,
    size: f64,
    spec: &TileSpec,
) -> Result<Vec<(f64, f64)>, CoreError> {
    if size <= 0.0 || size >= length - 1e-9 {
        return Ok(vec![(start, start + length)]);
    }
    if spec.overlap >= size {
        return Err(CoreError::validation(
            "the overlap must be smaller than the tile",
        ));
    }
    let whole = (length / size + 1e-9).floor() as usize;
    let rest = length - whole as f64 * size;
    let rest = if rest > 1e-9 { rest } else { 0.0 };
    let count = whole + usize::from(rest > 0.0);
    if count > MAX_TILES_PER_AXIS {
        return Err(CoreError::validation("that tile size makes too many tiles"));
    }
    let mut sizes = vec![size; whole];
    if rest > 0.0 {
        if spec.smallest_first {
            sizes.insert(0, rest);
        } else {
            sizes.push(rest);
        }
    }
    let mut out = Vec::with_capacity(sizes.len());
    let mut lo = start;
    for (i, s) in sizes.iter().enumerate() {
        let hi = if i + 1 == sizes.len() {
            start + length
        } else {
            lo + s
        };
        out.push((lo, hi));
        lo = hi;
    }
    Ok(out)
}

/// A piece of a cutting move, before the tile's shift.
#[derive(Debug, Clone, Copy)]
enum Piece {
    Line(Point3, Point3),
    Arc {
        from: Point3,
        to: Point3,
        centre: Point,
        clockwise: bool,
    },
}

impl Piece {
    fn start(&self) -> Point3 {
        match *self {
            Piece::Line(a, _) => a,
            Piece::Arc { from, .. } => from,
        }
    }

    fn end(&self) -> Point3 {
        match *self {
            Piece::Line(_, b) => b,
            Piece::Arc { to, .. } => to,
        }
    }
}

/// The part of `toolpath` inside `tile`, moved by the tile's shift, or
/// `None` when nothing is cut in the tile. The machine's home is left
/// where it is. Each clipped move keeps the feeds and speed the source
/// move ran at (`Toolpath::rates`), and a drill bank's firings inside the
/// tile move with their holes.
pub fn clip(toolpath: &Toolpath, tile: &Tile) -> Option<Toolpath> {
    let area = tile.area;
    let (dx, dy) = tile.shift;
    let moved = |p: Point3| Point3::new(p.x + dx, p.y + dy, p.z);
    let moved_centre = |c: Point| Point::new(c.x + dx, c.y + dy);
    let safe = toolpath.safe_z;

    let mut out: Vec<Move> = Vec::new();
    // Where the output tool is, in job coordinates before the shift.
    let mut at: Option<Point3> = None;
    let mut any_cut = false;
    let mut rates: Vec<RateChange> = Vec::new();
    let mut current = toolpath.rates_at(0);

    let mut from = toolpath.home;
    for (index, one) in toolpath.moves.iter().enumerate() {
        // The rates the source move ran at, taken up by the first move
        // written for it.
        let wanted = toolpath.rates_at(index);
        let mut take_rates = |out: &Vec<Move>| {
            if wanted != current {
                rates.push(RateChange {
                    from: out.len(),
                    feed: wanted.feed,
                    plunge: wanted.plunge,
                    spindle: wanted.spindle,
                });
                current = wanted;
            }
        };
        let pieces: Vec<Piece> = match one.kind {
            MoveKind::Rapid | MoveKind::Dwell { .. } => Vec::new(),
            MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. } => clip_arc(from, one, area),
            MoveKind::Feed | MoveKind::Plunge => clip_segment(from, one.to, area)
                .map(|(p, q)| Piece::Line(p, q))
                .into_iter()
                .collect(),
        };
        for piece in pieces {
            take_rates(&out);
            let (p, q) = (piece.start(), piece.end());
            let joined = at.is_some_and(|here| here.distance(p) <= 1e-9);
            if !joined {
                if let Some(here) = at {
                    out.push(Move::rapid(moved(Point3::new(here.x, here.y, safe))));
                } else {
                    out.push(Move::rapid(Point3::new(
                        toolpath.home.x,
                        toolpath.home.y,
                        safe.max(toolpath.home.z),
                    )));
                }
                out.push(Move::rapid(moved(Point3::new(p.x, p.y, safe))));
                out.push(with_power(Move::plunge(moved(p)), one.power));
            }
            let next = match piece {
                Piece::Line(p, q) => {
                    let vertical = (q.x - p.x).abs() <= 1e-12 && (q.y - p.y).abs() <= 1e-12;
                    if vertical && q.z < p.z {
                        Move::plunge(moved(q))
                    } else {
                        Move::feed(moved(q))
                    }
                }
                Piece::Arc {
                    to,
                    centre,
                    clockwise,
                    ..
                } => {
                    if clockwise {
                        Move::arc_cw(moved_centre(centre), moved(to))
                    } else {
                        Move::arc_ccw(moved_centre(centre), moved(to))
                    }
                }
            };
            out.push(with_power(next, one.power));
            at = Some(q);
            any_cut = true;
        }
        if let MoveKind::Dwell { seconds } = one.kind {
            if let Some(here) = at {
                if here.distance(one.to) <= 1e-9 {
                    take_rates(&out);
                    out.push(Move::dwell(seconds, moved(one.to)));
                }
            }
        }
        from = one.to;
    }
    if !any_cut {
        return None;
    }
    if let Some(here) = at {
        let high = safe.max(toolpath.home.z);
        out.push(Move::rapid(moved(Point3::new(here.x, here.y, high))));
    }
    out.push(Move::rapid(toolpath.home));

    let inside = |p: Point| {
        p.x >= area.min.x - 1e-9
            && p.x <= area.max.x + 1e-9
            && p.y >= area.min.y - 1e-9
            && p.y <= area.max.y + 1e-9
    };
    let mut clipped = toolpath.clone();
    clipped.moves = out;
    clipped.rates = rates;
    clipped.firings = toolpath
        .firings
        .iter()
        .filter(|f| inside(f.at))
        .map(|f| BankFiring {
            at: Point::new(f.at.x + dx, f.at.y + dy),
            ..f.clone()
        })
        .collect();
    clipped.recompute_bounds();
    TimeEstimate::default().apply(&mut clipped);
    Some(clipped)
}

fn with_power(mut one: Move, power: Option<f64>) -> Move {
    one.power = power;
    one
}

/// The parts of an arc move inside `area`, as arcs about the same centre
/// with Z following the sweep. An arc too small to split is followed as
/// straight chords.
fn clip_arc(from: Point3, arc: &Move, area: BBox) -> Vec<Piece> {
    let (centre, clockwise, sweep) = match (arc.kind, arc.arc_sweep(from)) {
        (MoveKind::ArcCw { centre }, Some(sweep)) => (centre, true, sweep),
        (MoveKind::ArcCcw { centre }, Some(sweep)) => (centre, false, sweep),
        _ => return Vec::new(),
    };
    let radius = centre.distance(from.xy());
    if radius <= ARC_TOLERANCE {
        return chords(from, arc, centre, sweep)
            .into_iter()
            .filter_map(|(a, b)| clip_segment(a, b, area))
            .map(|(p, q)| Piece::Line(p, q))
            .collect();
    }
    let start = centre.to(from.xy()).angle();
    let tau = std::f64::consts::TAU;
    // Fractions of the sweep where the circle crosses an edge of the area.
    let mut cuts = vec![0.0, 1.0];
    let mut crossing = |angle: f64| {
        let delta = if clockwise {
            (start - angle).rem_euclid(tau)
        } else {
            (angle - start).rem_euclid(tau)
        };
        let t = delta / sweep.abs();
        if t > 1e-12 && t < 1.0 - 1e-12 {
            cuts.push(t);
        }
    };
    for (line, vertical) in [
        (area.min.x, true),
        (area.max.x, true),
        (area.min.y, false),
        (area.max.y, false),
    ] {
        let offset = if vertical {
            line - centre.x
        } else {
            line - centre.y
        };
        let ratio = offset / radius;
        if ratio.abs() > 1.0 {
            continue;
        }
        if vertical {
            let a = ratio.acos();
            crossing(a);
            crossing(-a);
        } else {
            let a = ratio.asin();
            crossing(a);
            crossing(std::f64::consts::PI - a);
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);
    let at = |t: f64| -> Point3 {
        if t >= 1.0 {
            return arc.to;
        }
        if t <= 0.0 {
            return from;
        }
        let angle = start + sweep * t;
        Point3::new(
            centre.x + radius * angle.cos(),
            centre.y + radius * angle.sin(),
            from.z + (arc.to.z - from.z) * t,
        )
    };
    let inside = |p: Point3| {
        p.x >= area.min.x - 1e-9
            && p.x <= area.max.x + 1e-9
            && p.y >= area.min.y - 1e-9
            && p.y <= area.max.y + 1e-9
    };
    let mut out: Vec<Piece> = Vec::new();
    for pair in cuts.windows(2) {
        let (t0, t1) = (pair[0], pair[1]);
        if (t1 - t0) * sweep.abs() < MIN_SWEEP || !inside(at((t0 + t1) / 2.0)) {
            continue;
        }
        let (p, q) = (at(t0), at(t1));
        // Pieces that meet become one arc again.
        if let Some(Piece::Arc { to, .. }) = out.last_mut() {
            if to.distance(p) <= 1e-9 {
                *to = q;
                continue;
            }
        }
        out.push(Piece::Arc {
            from: p,
            to: q,
            centre,
            clockwise,
        });
    }
    out
}

/// An arc move as straight chords within [`ARC_TOLERANCE`].
fn chords(from: Point3, arc: &Move, centre: Point, sweep: f64) -> Vec<(Point3, Point3)> {
    let radius = centre.distance(from.xy());
    let step = if radius > ARC_TOLERANCE {
        2.0 * (1.0 - ARC_TOLERANCE / radius).acos()
    } else {
        sweep.abs()
    };
    let count = ((sweep.abs() / step.max(1e-6)).ceil() as usize).clamp(1, 10_000);
    let start = centre.to(from.xy()).angle();
    let mut out = Vec::with_capacity(count);
    let mut prev = from;
    for i in 1..=count {
        let t = i as f64 / count as f64;
        let next = if i == count {
            arc.to
        } else {
            let angle = start + sweep * t;
            Point3::new(
                centre.x + radius * angle.cos(),
                centre.y + radius * angle.sin(),
                from.z + (arc.to.z - from.z) * t,
            )
        };
        out.push((prev, next));
        prev = next;
    }
    out
}

/// The part of the segment from `a` to `b` whose XY lies in `area`, with Z
/// interpolated (Liang and Barsky). A vertical move is kept whole when its
/// XY is inside.
fn clip_segment(a: Point3, b: Point3, area: BBox) -> Option<(Point3, Point3)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, a.x - area.min.x),
        (dx, area.max.x - a.x),
        (-dy, a.y - area.min.y),
        (dy, area.max.y - a.y),
    ] {
        if p.abs() <= 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let at = |t: f64| Point3::new(a.x + dx * t, a.y + dy * t, a.z + (b.z - a.z) * t);
    let (p, q) = (at(t0), at(t1));
    if p.distance(q) <= 1e-12 && a.distance(b) > 1e-12 {
        return None;
    }
    Some((p, q))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::StrategyKind;

    fn job() -> BBox {
        BBox::new(Point::new(0.0, 0.0), Point::new(1000.0, 300.0))
    }

    fn wide(width: f64, overlap: f64) -> TileSpec {
        TileSpec {
            tile_width: width,
            overlap,
            ..TileSpec::default()
        }
    }

    #[test]
    fn a_long_job_splits_into_overlapping_tiles_moved_to_the_origin() {
        let tiles = tiles(job(), &wide(400.0, 10.0)).expect("tiles");
        assert_eq!(tiles.len(), 3);
        assert_eq!(tiles[0].area.min.x, 0.0);
        assert_eq!(tiles[1].nominal.min.x, 400.0);
        assert_eq!(tiles[1].area.min.x, 390.0);
        assert_eq!(tiles[1].area.max.x, 800.0);
        assert_eq!(tiles[2].area.max.x, 1000.0);
        // The tile's own corner goes to the origin; its overlap is cut at
        // negative X.
        assert_eq!(tiles[1].shift, (-400.0, 0.0));
        assert!(tiles.iter().all(|t| t.area.height() == 300.0));
        let whole = super::tiles(job(), &TileSpec::default()).expect("one");
        assert_eq!(whole.len(), 1);
        assert!(super::tiles(job(), &wide(10.0, 10.0)).is_err());
        assert!(super::tiles(job(), &wide(0.5, 0.0)).is_err());
    }

    #[test]
    fn feeding_through_splits_one_way_and_the_small_tile_can_come_first() {
        let spec = TileSpec {
            method: TilingMethod::FeedY,
            tile_width: 400.0,
            tile_height: 120.0,
            overlap: 5.0,
            smallest_first: false,
        };
        let rows = tiles(job(), &spec).expect("tiles");
        assert_eq!(rows.len(), 3, "the width is ignored");
        assert!(rows
            .iter()
            .all(|t| t.column == 0 && t.area.width() == 1000.0));
        assert_eq!(rows[2].nominal.height(), 60.0);
        let first = tiles(
            job(),
            &TileSpec {
                smallest_first: true,
                ..spec
            },
        )
        .expect("tiles");
        assert!((first[0].nominal.height() - 60.0).abs() < 1e-9);
        assert!((first[1].nominal.min.y - 60.0).abs() < 1e-9);
        assert_eq!(first[2].nominal.max.y, 300.0);
        let columns = tiles(
            job(),
            &TileSpec {
                method: TilingMethod::FeedX,
                ..spec
            },
        )
        .expect("tiles");
        assert_eq!(columns.len(), 3, "the height is ignored");
        let both = tiles(
            job(),
            &TileSpec {
                method: TilingMethod::Individual,
                ..spec
            },
        )
        .expect("tiles");
        assert_eq!(both.len(), 9);
        assert_eq!((both[4].row, both[4].column), (1, 1));
        assert_eq!(both[4].area.min.y, 115.0);
    }

    fn line_cut() -> Toolpath {
        let mut t = Toolpath::new(1, "Line", Tool::default(), StrategyKind::Profile);
        t.home = Point3::new(0.0, 0.0, 20.0);
        t.safe_z = 5.0;
        t.feed = 1000.0;
        t.plunge = 300.0;
        t.moves = vec![
            Move::rapid(Point3::new(100.0, 150.0, 20.0)),
            Move::rapid(Point3::new(100.0, 150.0, 5.0)),
            Move::plunge(Point3::new(100.0, 150.0, -3.0)),
            Move::feed(Point3::new(900.0, 150.0, -3.0)),
            Move::rapid(Point3::new(900.0, 150.0, 20.0)),
            Move::rapid(Point3::new(0.0, 0.0, 20.0)),
        ];
        t
    }

    #[test]
    fn a_cut_through_a_tile_is_clipped_to_it_and_shifted() {
        let tiles = tiles(job(), &wide(400.0, 10.0)).expect("tiles");
        let middle = clip(&line_cut(), &tiles[1]).expect("cuts in the middle tile");
        let cuts: Vec<&Move> = middle
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .collect();
        // Plunge in the overlap, left of the tile's corner, then feed to
        // its right edge.
        assert_eq!(cuts[0].to, Point3::new(-10.0, 150.0, -3.0));
        assert_eq!(
            cuts.last().expect("feed").to,
            Point3::new(400.0, 150.0, -3.0)
        );
        let mut from = middle.home;
        for m in &middle.moves {
            if m.kind == MoveKind::Rapid && (m.to.x != from.x || m.to.y != from.y) {
                assert!(m.to.z >= 5.0 && from.z >= 5.0, "{from:?} -> {:?}", m.to);
            }
            from = m.to;
        }
        assert_eq!(middle.moves.last().expect("home").to, middle.home);
        let first = clip(&line_cut(), &tiles[0]).expect("first tile");
        let feed_end = first
            .moves
            .iter()
            .rfind(|m| m.kind == MoveKind::Feed)
            .expect("feed");
        assert_eq!(feed_end.to, Point3::new(400.0, 150.0, -3.0));
        let kept = clip(&line_cut(), &tiles[1].in_place()).expect("in place");
        let first_cut = kept.moves.iter().find(|m| m.kind.is_cutting());
        assert_eq!(
            first_cut.map(|m| m.to),
            Some(Point3::new(390.0, 150.0, -3.0))
        );
    }

    #[test]
    fn a_tile_keeps_the_rates_of_the_moves_it_holds_and_its_own_firings() {
        // Two cuts, the second at other rates; a firing at each.
        let mut t = Toolpath::new(1, "bank", Tool::default(), StrategyKind::Drill);
        t.home = Point3::new(0.0, 0.0, 20.0);
        t.safe_z = 5.0;
        t.feed = 1000.0;
        t.spindle = 10000.0;
        t.moves = vec![
            Move::rapid(Point3::new(20.0, 10.0, 20.0)),
            Move::rapid(Point3::new(20.0, 10.0, 5.0)),
            Move::plunge(Point3::new(20.0, 10.0, -2.0)),
            Move::rapid(Point3::new(20.0, 10.0, 5.0)),
            Move::rapid(Point3::new(120.0, 10.0, 5.0)),
            Move::plunge(Point3::new(120.0, 10.0, -2.0)),
            Move::rapid(Point3::new(120.0, 10.0, 20.0)),
            Move::rapid(Point3::new(0.0, 0.0, 20.0)),
        ];
        t.rates = vec![RateChange {
            from: 4,
            feed: 2000.0,
            plunge: 500.0,
            spindle: 20000.0,
        }];
        t.firings = vec![
            BankFiring {
                at: Point::new(20.0, 10.0),
                drills: vec![1],
                along_x: vec![1],
                along_y: vec![],
            },
            BankFiring {
                at: Point::new(120.0, 10.0),
                drills: vec![2],
                along_x: vec![2],
                along_y: vec![],
            },
        ];
        let spec = TileSpec {
            method: TilingMethod::FeedX,
            tile_width: 80.0,
            ..TileSpec::default()
        };
        let job = BBox::new(Point::ORIGIN, Point::new(160.0, 40.0));
        let tiles = tiles(job, &spec).expect("tiles");
        let first = clip(&t, &tiles[0]).expect("a cut");
        let second = clip(&t, &tiles[1]).expect("a cut");
        let plunge = |c: &Toolpath| {
            c.moves
                .iter()
                .position(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
                .expect("a plunge")
        };
        assert_eq!(first.rates_at(plunge(&first)).spindle, 10000.0);
        assert!(first.rates.is_empty(), "{:?}", first.rates);
        assert_eq!(second.rates_at(plunge(&second)).spindle, 20000.0);
        assert_eq!(second.rates_at(plunge(&second)).feed, 2000.0);
        assert_eq!(second.rates_at(0).spindle, 20000.0, "from the first move");
        assert_eq!(first.firings.len(), 1);
        assert_eq!(first.firings[0].at, Point::new(20.0, 10.0));
        assert_eq!(second.firings.len(), 1);
        assert_eq!(second.firings[0].drills, vec![2]);
        assert_eq!(
            second.firings[0].at,
            Point::new(40.0, 10.0),
            "moved with the tile"
        );
    }

    #[test]
    fn a_tile_with_no_cut_gives_nothing_and_arcs_stay_arcs() {
        let mut t = line_cut();
        t.moves = vec![
            Move::rapid(Point3::new(350.0, 150.0, 5.0)),
            Move::plunge(Point3::new(350.0, 150.0, -1.0)),
            Move::arc_ccw(Point::new(400.0, 150.0), Point3::new(450.0, 150.0, -1.0)),
            Move::rapid(Point3::new(450.0, 150.0, 20.0)),
        ];
        let tiles = tiles(job(), &wide(400.0, 0.0)).expect("tiles");
        assert!(clip(&t, &tiles[2]).is_none());
        let left = clip(&t, &tiles[0]).expect("left half");
        let arcs: Vec<&Move> = left.moves.iter().filter(|m| m.kind.is_arc()).collect();
        assert_eq!(arcs.len(), 1, "{:?}", left.moves);
        // The lower half circle goes from 180 to 270 degrees in the first
        // tile, ending on its right edge at the bottom of the circle.
        assert!((arcs[0].to.x - 400.0).abs() < 1e-9);
        assert!((arcs[0].to.y - 100.0).abs() < 1e-9);
        let right = clip(&t, &tiles[1]).expect("right half");
        let arc = right
            .moves
            .iter()
            .find(|m| m.kind.is_arc())
            .expect("an arc");
        assert_eq!(
            arc.kind,
            MoveKind::ArcCcw {
                centre: Point::new(0.0, 150.0)
            },
            "the centre moves with the tile"
        );
        assert!((arc.to.x - 50.0).abs() < 1e-9 && (arc.to.y - 150.0).abs() < 1e-9);
    }

    #[test]
    fn a_full_circle_crossing_an_edge_splits_into_arcs_on_each_side() {
        let mut t = line_cut();
        t.moves = vec![
            Move::rapid(Point3::new(450.0, 150.0, 5.0)),
            Move::plunge(Point3::new(450.0, 150.0, -2.0)),
            Move::arc_cw(Point::new(400.0, 150.0), Point3::new(450.0, 150.0, -2.0)),
        ];
        let tiles = tiles(job(), &wide(400.0, 0.0)).expect("tiles");
        let right = clip(&t, &tiles[1]).expect("right");
        let arcs: Vec<&Move> = right.moves.iter().filter(|m| m.kind.is_arc()).collect();
        // Clockwise from 0 degrees: down to the bottom (edge), then later from
        // the top back to the start; two arcs in the right hand tile.
        assert_eq!(arcs.len(), 2, "{:?}", right.moves);
        let left = clip(&t, &tiles[0]).expect("left");
        assert_eq!(left.moves.iter().filter(|m| m.kind.is_arc()).count(), 1);
    }
}
