//! Collision checks while the preview cuts: the parts of the tool that do
//! not cut (the shank above the flutes and the holder) meeting the stock or
//! a clamp, cutting deeper than the flutes are long, and a tool that cannot
//! cut at its centre plunging straight into material. Rapids through
//! material are found by the sweep itself ([`super::sim::Stock::cut_move`]).
//!
//! A [`Guard`] is made per toolpath from its tool. It looks at the stock as
//! it is before the move part is cut, a cell at a time along the move, and
//! skips at once whenever the part it checks lies above the material top,
//! which is almost always, so the animation keeps its pace. Each kind of
//! event is reported once per move. A toolpath of the other side of a
//! double sided job is checked the same way from underneath: its tool
//! points up into the underside and its parts reach down from the tip.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::doc::Document;
use crate::geom::Tolerance;
use crate::model::shape::{inside_cells, Grid};

use super::sim::{point_on, Collision, MovePart, Stock};
use super::tool::Tool;
use super::toolpath::{MoveKind, Point3};

/// What a check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    /// A rapid move passing through material.
    #[default]
    Rapid,
    /// A tool that cannot cut at its centre plunging into material.
    Plunge,
    /// Material above the flutes: cutting deeper than the flute length.
    Flute,
    /// The shank, wider than the cutter, touching material.
    Shank,
    /// The holder touching material.
    Holder,
    /// Any part of the tool inside a clamp (a keep-out zone) below the
    /// toolpath's safe height.
    Clamp,
    /// Cut below the design by more than the tolerance.
    Gouge,
    /// Material left above the design by more than the tolerance.
    Leftover,
    /// Material left where the tool cannot reach the design from above.
    Undercut,
}

impl IssueKind {
    /// How much the kind of event matters when nothing else says.
    pub fn severity(self) -> Severity {
        match self {
            IssueKind::Rapid
            | IssueKind::Flute
            | IssueKind::Shank
            | IssueKind::Holder
            | IssueKind::Clamp
            | IssueKind::Gouge => Severity::High,
            IssueKind::Plunge | IssueKind::Undercut => Severity::Medium,
            IssueKind::Leftover => Severity::Low,
        }
    }
}

/// How much an issue matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Breaks a tool, the part or the machine.
    #[default]
    High,
    /// Spoils the part.
    Medium,
    /// Worth a look.
    Low,
}

/// How far material must reach into a part of the tool before it counts,
/// millimetres: more than `f32` rounding of the heights, less than any real
/// contact.
const REACH: f64 = 0.02;

/// Cell offsets within `radius` of the centre, millimetres, and beyond
/// `inner` (none inside for zero).
fn ring(inner: f64, radius: f64, cell: f64) -> Vec<(i32, i32)> {
    let reach = (radius / cell).ceil() as i32;
    let mut out = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let r = f64::from(dx).hypot(f64::from(dy)) * cell;
            if r <= radius + 1e-9 && (inner <= 0.0 || r > inner + 1e-9) {
                out.push((dx, dy));
            }
        }
    }
    out
}

/// The clamps of a job as stock cells: keep-out zones rasterised once.
#[derive(Debug, Clone, PartialEq)]
pub struct Clamps {
    width: usize,
    height: usize,
    cells: Vec<bool>,
    /// First and last column and row holding a clamp cell.
    bounds: [i64; 4],
}

impl Clamps {
    /// The keep-out zones of `doc` over the cells of `stock`, or `None`
    /// when there are none.
    pub fn of(doc: &Document, stock: &Stock) -> Option<Clamps> {
        let tol = Tolerance::new(0.01).ok()?;
        let zones = super::keepout::keep_outs(doc, tol);
        if zones.is_empty() {
            return None;
        }
        let grid = Grid {
            width: stock.width,
            height: stock.height,
            cell: stock.cell,
            origin: crate::geom::Point::new(stock.origin_x, stock.origin_y),
        };
        let mut cells = vec![false; stock.width * stock.height];
        for zone in &zones {
            for (i, inside) in inside_cells(&zone.region, &grid, tol)
                .into_iter()
                .enumerate()
            {
                cells[i] |= inside;
            }
        }
        Clamps::from_cells(stock, cells)
    }

    /// The keep-out zones of the other side of a double sided job, turned
    /// over onto the cells of `stock` (seen from the active side), or
    /// `None` when that side has none.
    pub fn of_other_side(doc: &Document, stock: &Stock) -> Option<Clamps> {
        let turned_doc = doc.turned_over(false)?;
        let setup = doc.job_setup();
        let turned = stock.turned(setup, doc.active_side());
        let theirs = Clamps::of(&turned_doc, &turned)?;
        let cells = (0..stock.width * stock.height)
            .map(|i| theirs.cells[stock.turned_cell(i, setup)])
            .collect();
        Clamps::from_cells(stock, cells)
    }

    fn from_cells(stock: &Stock, cells: Vec<bool>) -> Option<Clamps> {
        let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        for (i, _) in cells.iter().enumerate().filter(|(_, c)| **c) {
            let (x, y) = ((i % stock.width) as i64, (i / stock.width) as i64);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        (bounds[0] <= bounds[2]).then_some(Clamps {
            width: stock.width,
            height: stock.height,
            cells,
            bounds,
        })
    }

    /// True when a cell of `disk` round (`cx`, `cy`) is a clamp cell.
    fn touched(&self, (cx, cy): (i64, i64), disk: &Disk) -> bool {
        let r = i64::from(disk.reach);
        if cx + r < self.bounds[0]
            || cy + r < self.bounds[1]
            || cx - r > self.bounds[2]
            || cy - r > self.bounds[3]
        {
            return false;
        }
        disk.cells.iter().any(|&(dx, dy)| {
            let (x, y) = (cx + i64::from(dx), cy + i64::from(dy));
            x >= 0
                && y >= 0
                && x < self.width as i64
                && y < self.height as i64
                && self.cells[y as usize * self.width + x as usize]
        })
    }
}

/// A part of the tool as cell offsets and the height of its bottom above
/// the tip, millimetres.
#[derive(Debug, Clone)]
struct Disk {
    cells: Vec<(i32, i32)>,
    reach: i32,
    bottom: f64,
}

impl Disk {
    fn new(cells: Vec<(i32, i32)>, bottom: f64) -> Option<Disk> {
        let reach = cells.iter().map(|&(dx, dy)| dx.abs().max(dy.abs())).max()?;
        Some(Disk {
            cells,
            reach,
            bottom,
        })
    }

    /// True when material under a cell of the disk round `centre` reaches
    /// above the disk's bottom with the tip at `tip`, or with `below` (a
    /// tool pointing up into the underside) below the disk's end.
    fn buried(&self, stock: &Stock, centre: (i64, i64), tip: f64, below: bool) -> bool {
        if below {
            return self.buried_below(stock, centre, tip);
        }
        let level = (tip + self.bottom + REACH) as f32;
        self.cells.iter().any(|&(dx, dy)| {
            let (x, y) = (centre.0 + i64::from(dx), centre.1 + i64::from(dy));
            x >= 0
                && y >= 0
                && x < stock.width as i64
                && y < stock.height as i64
                && stock.z[y as usize * stock.width + x as usize] > level
                // A cell cut through from both sides holds no material.
                && !stock.is_through_cell(y as usize * stock.width + x as usize)
        })
    }

    /// [`Disk::buried`] for a tool pointing up into the underside: material
    /// below the disk's end.
    fn buried_below(&self, stock: &Stock, centre: (i64, i64), tip: f64) -> bool {
        let level = (tip - self.bottom - REACH) as f32;
        self.cells.iter().any(|&(dx, dy)| {
            let (x, y) = (centre.0 + i64::from(dx), centre.1 + i64::from(dy));
            x >= 0
                && y >= 0
                && x < stock.width as i64
                && y < stock.height as i64
                && stock.floor[y as usize * stock.width + x as usize] < level
                && !stock.is_through_cell(y as usize * stock.width + x as usize)
        })
    }
}

/// The collision checks for one toolpath's tool.
#[derive(Debug, Clone)]
pub struct Guard {
    /// The cutter up to its flute length, as wide as the shank at most:
    /// material over it is cut deeper than the flutes reach.
    flute: Option<Disk>,
    /// The shank where it is wider than the cutter.
    shank: Option<Disk>,
    /// The holder.
    holder: Option<Disk>,
    /// Every part of the tool with the height of its bottom, for clamps.
    whole: Vec<Disk>,
    /// The tool cannot cut at its centre.
    no_plunge: bool,
    clamps: Option<Arc<Clamps>>,
    /// The toolpath's safe height, job Z: clamps reach up to it.
    safe_z: f64,
}

impl Guard {
    /// The checks for `tool` on cells of `cell` millimetres, with the
    /// job's `clamps` up to `safe_z`.
    pub fn new(tool: &Tool, cell: f64, clamps: Option<Arc<Clamps>>, safe_z: f64) -> Guard {
        let radius = tool.radius();
        let shank = tool.shank_radius();
        let flutes = tool.flute_length;
        let checked = flutes > 0.0;
        let flute = if checked {
            Disk::new(ring(0.0, radius.min(shank), cell), flutes)
        } else {
            None
        };
        let wide_shank = if checked && shank > radius + cell * 0.5 {
            Disk::new(ring(radius, shank, cell), flutes)
        } else {
            None
        };
        let holder = tool
            .holder()
            .and_then(|(r, bottom, _)| Disk::new(ring(0.0, r, cell), bottom));
        let mut whole: Vec<Disk> = Disk::new(ring(0.0, radius, cell), 0.0)
            .into_iter()
            .collect();
        if checked {
            whole.extend(Disk::new(ring(0.0, shank, cell), flutes));
        }
        whole.extend(holder.clone());
        let no_plunge = !tool.center_cutting;
        Guard {
            flute,
            shank: wide_shank,
            holder,
            whole,
            no_plunge,
            clamps,
            safe_z,
        }
    }

    /// True when the guard has nothing to check.
    pub fn is_idle(&self) -> bool {
        self.flute.is_none()
            && self.shank.is_none()
            && self.holder.is_none()
            && !self.no_plunge
            && self.clamps.is_none()
    }

    /// Checks `part` against `stock` as it is before the part is cut and
    /// adds what it finds to `out`, each kind once a move (`seen`). With
    /// `below` the toolpath cuts up from the underside (the other side of a
    /// double sided job, flipped): up is into the material there.
    pub fn check(
        &self,
        stock: &Stock,
        part: &MovePart<'_>,
        below: bool,
        seen: &mut HashSet<(u64, usize, IssueKind)>,
        out: &mut Vec<Collision>,
    ) {
        if self.is_idle() {
            return;
        }
        // Heights measured into the material: down from the top, or up
        // from the underside.
        let into = if below { -1.0 } else { 1.0 };
        let (from, one) = (part.from, part.one);
        let cutting = matches!(
            one.kind,
            MoveKind::Feed | MoveKind::Plunge | MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. }
        );
        let key = |kind| (part.toolpath_id, part.index, kind);
        let open = |seen: &HashSet<_>, kind| !seen.contains(&key(kind));
        // Straight down into material with a tool that cannot plunge.
        let straight_down = (from.z - one.to.z) * into > 1e-9
            && (one.to.x - from.x).hypot(one.to.y - from.y) < 1e-6
            && cutting;
        let lowest_bottom = [&self.flute, &self.shank, &self.holder]
            .into_iter()
            .flatten()
            .map(|d| d.bottom)
            .fold(f64::INFINITY, f64::min);
        let length = one.length(from) * (part.t1 - part.t0);
        let steps = (length / stock.cell).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let t = part.t0 + (part.t1 - part.t0) * k as f64 / steps as f64;
            let Some(p) = point_on(from, one, t) else {
                return;
            };
            let centre = stock.cell_of(p.x, p.y);
            let mut emit = |kind: IssueKind, seen: &mut HashSet<_>| {
                seen.insert(key(kind));
                out.push(collision(part, p, kind));
            };
            let face = if below { stock.bottom } else { stock.top };
            if cutting && (face - (p.z + lowest_bottom * into)) * into > REACH {
                for (disk, kind) in [
                    (&self.flute, IssueKind::Flute),
                    (&self.shank, IssueKind::Shank),
                    (&self.holder, IssueKind::Holder),
                ] {
                    if let Some(disk) = disk {
                        if open(seen, kind) && disk.buried(stock, centre, p.z, below) {
                            emit(kind, seen);
                        }
                    }
                }
            }
            if self.no_plunge && straight_down && open(seen, IssueKind::Plunge) {
                let material = if below {
                    stock.floor_at(p.x, p.y).map_or(f64::INFINITY, f64::from)
                } else {
                    stock
                        .height_at(p.x, p.y)
                        .map_or(f64::NEG_INFINITY, f64::from)
                };
                if (material - p.z) * into > REACH && !stock.is_through(p.x, p.y) {
                    emit(IssueKind::Plunge, seen);
                }
            }
            if let Some(clamps) = &self.clamps {
                if open(seen, IssueKind::Clamp)
                    && self.whole.iter().any(|disk| {
                        (self.safe_z - (p.z + disk.bottom * into)) * into > 1e-6
                            && clamps.touched(centre, disk)
                    })
                {
                    emit(IssueKind::Clamp, seen);
                }
            }
        }
    }
}

fn collision(part: &MovePart<'_>, p: Point3, kind: IssueKind) -> Collision {
    Collision {
        toolpath_id: part.toolpath_id,
        move_index: part.index,
        x: p.x,
        y: p.y,
        z: p.z,
        kind,
        severity: kind.severity(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::sim::{Cutter, Footprint, Quality, Sweep};
    use crate::cam::tool::ToolGeometry;
    use crate::cam::toolpath::Move;
    use crate::doc::JobSetup;

    fn job() -> JobSetup {
        JobSetup {
            width: 100.0,
            height: 60.0,
            thickness: 30.0,
            ..JobSetup::default()
        }
    }

    fn end_mill(diameter: f64) -> Tool {
        Tool {
            name: "t".into(),
            tool_type: ToolGeometry::EndMill { diameter },
            ..Tool::default()
        }
    }

    /// Cuts `moves` from `home` with `tool` and checks each whole move
    /// first, as the preview does.
    fn run(tool: &Tool, home: Point3, moves: &[Move]) -> (Stock, Vec<Collision>) {
        run_from(tool, home, moves, false)
    }

    /// [`run`] down from the top or, with `below`, up from the underside.
    fn run_from(tool: &Tool, home: Point3, moves: &[Move], below: bool) -> (Stock, Vec<Collision>) {
        let mut stock = Stock::new(&job(), Quality::Standard);
        let safe = if below {
            stock.bottom - 5.0
        } else {
            stock.top + 5.0
        };
        let guard = Guard::new(tool, stock.cell, None, safe);
        let cutter = Cutter::new(&Footprint::of(tool, stock.cell).expect("footprint"));
        let mut sweep = Sweep::default();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut from = home;
        for (index, one) in moves.iter().enumerate() {
            let part = MovePart {
                toolpath_id: 7,
                index,
                from,
                one,
                t0: 0.0,
                t1: 1.0,
            };
            guard.check(&stock, &part, below, &mut seen, &mut out);
            stock.finish_sweep(&cutter, &mut sweep, 1, below);
            if let Some(hit) = stock.cut_move(&cutter, &mut sweep, &part, 1, below) {
                out.push(hit);
            }
            stock.finish_sweep(&cutter, &mut sweep, 1, below);
            from = one.to;
        }
        (stock, out)
    }

    fn kinds(found: &[Collision]) -> Vec<IssueKind> {
        found.iter().map(|c| c.kind).collect()
    }

    #[test]
    fn a_wide_shank_hits_the_walls_of_a_deep_narrow_pocket() {
        let top = job().surface_z();
        // A 4 mm slot cut in 3 mm passes by a 4 mm cutter with 15 mm flutes
        // on a 6 mm shank: down to 15 the shank stays above the material.
        let mut tool = end_mill(4.0);
        tool.flute_length = 15.0;
        tool.shank_diameter = 6.0;
        let passes = |depth_steps: &[f64]| {
            let mut moves = vec![Move::rapid(Point3::new(20.0, 30.0, top + 5.0))];
            for depth in depth_steps {
                moves.push(Move::plunge(Point3::new(20.0, 30.0, top - depth)));
                moves.push(Move::feed(Point3::new(80.0, 30.0, top - depth)));
                moves.push(Move::rapid(Point3::new(80.0, 30.0, top + 5.0)));
                moves.push(Move::rapid(Point3::new(20.0, 30.0, top + 5.0)));
            }
            moves
        };
        let home = Point3::new(0.0, 0.0, top + 20.0);
        let (_, found) = run(&tool, home, &passes(&[3.0, 6.0, 9.0, 12.0]));
        assert!(found.is_empty(), "{found:?}");
        // Twenty one deep: the shank goes 6 mm into the slot and rubs its
        // walls on the last two passes.
        let deep: Vec<f64> = (1..=7).map(|k| f64::from(k) * 3.0).collect();
        let (_, found) = run(&tool, home, &passes(&deep));
        assert!(found.len() >= 2, "{found:?}");
        assert!(
            found.iter().all(|c| c.kind == IssueKind::Shank),
            "{found:?}"
        );
        // First on the plunge of the pass at 18 down, as the shank enters.
        let first = found[0];
        assert_eq!(first.move_index, 1 + 4 * 5, "{first:?}");
        assert!(first.z < top - 15.0 && first.z > top - 15.5, "{first:?}");
        assert_eq!(first.toolpath_id, 7);
        assert_eq!(first.severity, Severity::High);
    }

    #[test]
    fn cutting_deeper_than_the_flutes_and_a_holder_in_the_material_are_found() {
        let top = job().surface_z();
        let mut tool = end_mill(6.0);
        tool.flute_length = 8.0;
        tool.length_below_holder = 12.0;
        tool.holder_diameter = 30.0;
        tool.holder_length = 40.0;
        let home = Point3::new(0.0, 0.0, top + 20.0);
        let moves = vec![
            Move::rapid(Point3::new(20.0, 30.0, top + 5.0)),
            Move::plunge(Point3::new(20.0, 30.0, top - 10.0)),
            Move::feed(Point3::new(60.0, 30.0, top - 10.0)),
            Move::plunge(Point3::new(60.0, 30.0, top - 14.0)),
        ];
        let (_, found) = run(&tool, home, &moves);
        let at = |kind| found.iter().find(|c| c.kind == kind).copied();
        let flute = at(IssueKind::Flute).expect("too deep for the flutes");
        assert_eq!(flute.move_index, 1);
        let holder = at(IssueKind::Holder).expect("the holder meets the top");
        assert_eq!(holder.move_index, 3);
        assert!(holder.z < top - 12.0 + 1e-6, "{holder:?}");
    }

    #[test]
    fn the_other_side_is_checked_from_underneath() {
        // The flute and holder test turned over: the tool points up into
        // the underside and its parts reach down from the tip.
        let bottom = job().surface_z() - job().thickness;
        let mut tool = end_mill(6.0);
        tool.flute_length = 8.0;
        tool.length_below_holder = 12.0;
        tool.holder_diameter = 30.0;
        tool.holder_length = 40.0;
        tool.center_cutting = false;
        let home = Point3::new(0.0, 0.0, bottom - 20.0);
        let moves = vec![
            Move::rapid(Point3::new(20.0, 30.0, bottom - 5.0)),
            Move::plunge(Point3::new(20.0, 30.0, bottom + 10.0)),
            Move::feed(Point3::new(60.0, 30.0, bottom + 10.0)),
            Move::plunge(Point3::new(60.0, 30.0, bottom + 14.0)),
        ];
        let (stock, found) = run_from(&tool, home, &moves, true);
        let at = |kind| found.iter().find(|c| c.kind == kind).copied();
        let plunge = at(IssueKind::Plunge).expect("a plunge up into the underside");
        assert_eq!(plunge.move_index, 1);
        let flute = at(IssueKind::Flute).expect("too deep for the flutes");
        assert_eq!(flute.move_index, 1);
        let holder = at(IssueKind::Holder).expect("the holder meets the underside");
        assert_eq!(holder.move_index, 3);
        assert!(holder.z > bottom + 12.0 - 1e-6, "{holder:?}");
        // The underside was cut, the top left alone.
        assert!(stock.floor_at(40.0, 30.0).expect("inside") > bottom as f32 + 9.9);
        assert_eq!(stock.height_at(40.0, 30.0), Some(stock.top as f32));
        // Shallow passes from underneath meet nothing.
        let shallow = vec![
            Move::rapid(Point3::new(20.0, 30.0, bottom - 5.0)),
            Move::feed(Point3::new(40.0, 30.0, bottom + 3.0)),
            Move::feed(Point3::new(60.0, 30.0, bottom + 3.0)),
        ];
        let (_, found) = run_from(&tool, home, &shallow, true);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_other_sides_clamps_are_turned_over_onto_the_stock() {
        use crate::doc::{Document, Entity, EntityKind, JobKind, Side};
        use crate::geom::{rect, Point};
        let mut doc = Document::new();
        doc.set_job(JobSetup {
            kind: JobKind::DoubleSided,
            ..job()
        })
        .expect("job");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let layer = doc.add_layer(Some("Clamps".into()), None);
        doc.set_layer_keep_out(layer, true).expect("keep out");
        // A clamp over x 5 to 15 of the bottom side: x 85 to 95 seen from
        // the top once the job is turned over (left to right).
        let zone = rect(Point::new(5.0, 0.0), Point::new(15.0, 60.0)).expect("rect");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path: zone },
        });
        doc.set_active_side(Side::Top).expect("top");
        let stock = Stock::new(doc.job(), Quality::Standard);
        assert!(Clamps::of(&doc, &stock).is_none());
        let clamps = Clamps::of_other_side(&doc, &stock).expect("the bottom's clamps");
        let cell = |x: f64, y: f64| {
            let (cx, cy) = stock.cell_of(x, y);
            clamps.cells[cy as usize * stock.width + cx as usize]
        };
        assert!(cell(90.0, 30.0));
        assert!(!cell(10.0, 30.0));
        // A pass from underneath reaching into the turned clamp.
        let bottom = stock.bottom;
        let guard = Guard::new(
            &end_mill(6.0),
            stock.cell,
            Some(Arc::new(clamps)),
            bottom - 5.0,
        );
        let through = Move::feed(Point3::new(95.0, 30.0, bottom + 1.0));
        let part = MovePart {
            toolpath_id: 1,
            index: 0,
            from: Point3::new(50.0, 30.0, bottom + 1.0),
            one: &through,
            t0: 0.0,
            t1: 1.0,
        };
        let (mut seen, mut out) = (HashSet::new(), Vec::new());
        guard.check(&stock, &part, true, &mut seen, &mut out);
        assert_eq!(kinds(&out), vec![IssueKind::Clamp]);
        assert!((out[0].x - 82.0).abs() < 1.0, "{:?}", out[0]);
        // Below the safe height seen from underneath: nothing.
        let under = Move::feed(Point3::new(95.0, 30.0, bottom - 6.0));
        let part = MovePart {
            from: Point3::new(50.0, 30.0, bottom - 6.0),
            one: &under,
            ..part
        };
        let (mut seen, mut out) = (HashSet::new(), Vec::new());
        guard.check(&stock, &part, true, &mut seen, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_tool_that_cannot_cut_at_its_centre_may_ramp_but_not_plunge() {
        let top = job().surface_z();
        let mut tool = end_mill(6.0);
        tool.center_cutting = false;
        let home = Point3::new(0.0, 0.0, top + 20.0);
        let ramp = vec![
            Move::rapid(Point3::new(20.0, 30.0, top + 1.0)),
            Move::feed(Point3::new(40.0, 30.0, top - 2.0)),
            Move::feed(Point3::new(60.0, 30.0, top - 2.0)),
            // Straight down again inside the groove it cut: nothing to hit.
            Move::plunge(Point3::new(60.0, 30.0, top - 2.0)),
        ];
        let (_, found) = run(&tool, home, &ramp);
        assert!(found.is_empty(), "{found:?}");
        let plunge = vec![
            Move::rapid(Point3::new(20.0, 30.0, top + 1.0)),
            Move::plunge(Point3::new(20.0, 30.0, top - 2.0)),
        ];
        let (_, found) = run(&tool, home, &plunge);
        assert_eq!(kinds(&found), vec![IssueKind::Plunge]);
        assert_eq!(found[0].move_index, 1);
        // A centre cutting tool plunges fine.
        let (_, found) = run(&end_mill(6.0), home, &plunge);
        assert!(found.is_empty());
    }

    #[test]
    fn a_rapid_through_uncut_stock_is_found_with_its_kind() {
        let top = job().surface_z();
        let tool = end_mill(6.0);
        let home = Point3::new(0.0, 0.0, top + 20.0);
        let moves = vec![
            Move::rapid(Point3::new(20.0, 30.0, top + 5.0)),
            Move::plunge(Point3::new(20.0, 30.0, top - 3.0)),
            Move::rapid(Point3::new(60.0, 30.0, top - 3.0)),
        ];
        let (_, found) = run(&tool, home, &moves);
        assert_eq!(kinds(&found), vec![IssueKind::Rapid]);
        assert_eq!(found[0].move_index, 2);
        assert_eq!(found[0].severity, Severity::High);
    }

    #[test]
    fn clamps_are_found_below_the_safe_height_only() {
        let job = job();
        let stock = Stock::new(&job, Quality::Standard);
        let mut cells = vec![false; stock.width * stock.height];
        // A clamp over x 45 to 55 (job coordinates from the origin).
        let (x0, x1) = (
            ((45.0 - stock.origin_x) / stock.cell) as usize,
            ((55.0 - stock.origin_x) / stock.cell) as usize,
        );
        for y in 0..stock.height {
            for x in x0..x1 {
                cells[y * stock.width + x] = true;
            }
        }
        let clamps = Clamps {
            width: stock.width,
            height: stock.height,
            cells,
            bounds: [x0 as i64, 0, x1 as i64 - 1, stock.height as i64 - 1],
        };
        let top = job.surface_z();
        let guard = Guard::new(
            &end_mill(6.0),
            stock.cell,
            Some(Arc::new(clamps)),
            top + 5.0,
        );
        let (over, through) = (
            Move::rapid(Point3::new(90.0, 30.0, top + 6.0)),
            Move::feed(Point3::new(90.0, 30.0, top - 1.0)),
        );
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for one in [&over, &through] {
            let part = MovePart {
                toolpath_id: 1,
                index: usize::from(one.kind != MoveKind::Rapid),
                from: Point3::new(10.0, 30.0, one.to.z),
                one,
                t0: 0.0,
                t1: 1.0,
            };
            guard.check(&stock, &part, false, &mut seen, &mut out);
        }
        assert_eq!(kinds(&out), vec![IssueKind::Clamp]);
        assert_eq!(out[0].move_index, 1);
        // Found where the cutter's edge first reaches the clamp.
        assert!((out[0].x - 42.0).abs() < 1.0, "{:?}", out[0]);
    }
    #[test]
    fn the_holder_is_found_from_a_fiftieth_of_a_millimetre_into_the_top() {
        let top = job().surface_z();
        let holder_found = |depth: f64| {
            let mut tool = end_mill(6.0);
            tool.flute_length = 20.0;
            tool.length_below_holder = 12.0;
            tool.holder_diameter = 30.0;
            tool.holder_length = 40.0;
            let home = Point3::new(0.0, 0.0, top + 20.0);
            let moves = vec![
                Move::rapid(Point3::new(40.0, 30.0, top + 5.0)),
                Move::plunge(Point3::new(40.0, 30.0, top - depth)),
            ];
            let (_, found) = run(&tool, home, &moves);
            found.iter().any(|c| c.kind == IssueKind::Holder)
        };
        // The holder starts 12 above the tip: 0.01 into the top is within
        // the reach the check allows, 0.05 is a collision.
        assert!(!holder_found(11.9));
        assert!(!holder_found(12.01));
        assert!(holder_found(12.05));
    }
}
