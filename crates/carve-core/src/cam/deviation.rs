//! Gouge and leftover check: the previewed stock compared with the design.
//!
//! Each cut cell is held to the design of the toolpath that cut it deepest
//! (its owner): the model surface for 3D roughing and finishing, the floor
//! inside a pocket's vectors and the untouched top outside them, the part
//! a profile keeps at the top and the cut depth beside it, the V a V-groove
//! cuts (as deep as the vectors are wide for the tool's angle, down to its
//! flat depth) and the holes a drilling toolpath drills. A cell cut below
//! that by more than the tolerance is a gouge. A cell a pocket or a 3D
//! finish means to clear that is still above its design by more than the
//! tolerance is leftover; where the tool of that toolpath cannot reach the
//! design from above at all (a corner tighter than it, a groove narrower)
//! the leftover is an undercut, which only a smaller tool can clear.
//!
//! Each cell is compared with the lowest (for a gouge) or the highest (for
//! leftover) design over the cells round it, so the one cell the grid
//! rounds a tool edge by never counts. Flagged cells join into areas; each
//! area is one issue with its worst point and the move of its toolpath
//! nearest to it.
//!
//! On a double sided job the other side's toolpaths are held to that
//! side's design: the stock is turned over, checked in that side's job
//! space and the findings turned back.

use std::collections::HashMap;

use serde::Serialize;

use crate::doc::Document;
use crate::geom::{path_bbox, Point, Region, Tolerance};
use crate::model::field::distance_to;
use crate::model::shape::{inside_cells, Grid};

use super::collide::{IssueKind, Severity};
use super::drill::DrillParams;
use super::pocket::PocketParams;
use super::profile::{ProfileParams, ProfileSide};
use super::sim::{point_on, Footprint, Stock, THROUGH};
use super::surface::Surface;
use super::toolpath::{MoveKind, Point3, StrategyKind, Toolpath};
use super::vgroove::{v_shape, VGrooveParams};

/// Code of a cell in the check map: nothing found.
pub const CLEAN: u8 = 0;
/// Code of a gouged cell.
pub const GOUGE: u8 = 1;
/// Code of a cell with material left.
pub const LEFTOVER: u8 = 2;
/// Code of a cell with material left the tool cannot reach.
pub const UNDERCUT: u8 = 3;

/// Most issues listed; the map shows every cell.
const MOST_ISSUES: usize = 100;
/// Leftover smaller than this, square millimetres, shows on the map but is
/// not listed (the corners every end mill leaves in a square pocket).
const LEAST_LISTED_LEFT: f64 = 2.0;
/// Cells of an area tested for reach.
const REACH_SAMPLES: usize = 48;

/// One area the check found.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceIssue {
    /// Gouge, leftover or undercut.
    pub kind: IssueKind,
    /// How much it matters.
    pub severity: Severity,
    /// The toolpath that cut it (a gouge) or means to clear it.
    pub toolpath_id: u64,
    /// That toolpath's move passing nearest the worst point.
    pub move_index: usize,
    /// The worst point: job X and Y, and the material height there.
    pub x: f64,
    /// The worst point's job Y.
    pub y: f64,
    /// The material height at the worst point, job Z.
    pub z: f64,
    /// How far the worst point lies from the design, millimetres.
    pub depth: f64,
    /// The area's size, square millimetres.
    pub area: f64,
}

/// What the check found: a code per stock cell and the issues.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// [`CLEAN`], [`GOUGE`], [`LEFTOVER`] or [`UNDERCUT`] per cell.
    pub map: Vec<u8>,
    /// The areas, the worst first, at most [`MOST_ISSUES`].
    pub issues: Vec<SurfaceIssue>,
    /// How many areas were found in all.
    pub found: usize,
}

/// A toolpath's design over the stock cells.
enum Design {
    /// The model surface ([`Checker::model`]).
    Model,
    /// `inside` cells at `inner`, the rest at `outer`, job Z.
    Split {
        inside: Vec<bool>,
        inner: f32,
        outer: f32,
    },
    /// A height per cell, job Z.
    Heights(Vec<f32>),
    /// Nothing to hold the cells to.
    Free,
}

struct Checker<'a> {
    doc: &'a Document,
    stock: &'a Stock,
    grid: Grid,
    tol: Tolerance,
    top: f32,
    /// The model surface in job Z on the stock cells, made when first wanted.
    model: Option<Vec<f32>>,
}

impl<'a> Checker<'a> {
    fn new(doc: &'a Document, stock: &'a Stock) -> Option<Checker<'a>> {
        Some(Checker {
            doc,
            stock,
            grid: Grid {
                width: stock.width,
                height: stock.height,
                cell: stock.cell,
                origin: Point::new(stock.origin_x, stock.origin_y),
            },
            tol: Tolerance::new(0.01).ok()?,
            top: stock.top as f32,
            model: None,
        })
    }

    fn model(&mut self) -> &[f32] {
        let (stock, doc) = (self.stock, self.doc);
        self.model.get_or_insert_with(|| {
            let surface = Surface::of(doc);
            (0..stock.width * stock.height)
                .map(|i| {
                    let p = Point::new(
                        stock.origin_x + (i % stock.width) as f64 * stock.cell,
                        stock.origin_y + (i / stock.width) as f64 * stock.cell,
                    );
                    surface.z_at(p) as f32
                })
                .collect()
        })
    }

    /// The vectors a toolpath was made from.
    fn paths(&self, toolpath: &Toolpath) -> Option<Vec<crate::geom::Path>> {
        let ids = toolpath.params_json.get("ids")?.as_array()?;
        Some(
            ids.iter()
                .filter_map(serde_json::Value::as_u64)
                .flat_map(|id| self.doc.paths_of(id).into_iter().map(|(_, p)| p))
                .collect(),
        )
    }

    /// The cells inside the closed vectors a toolpath was made from.
    fn inside(&self, toolpath: &Toolpath) -> Option<Vec<bool>> {
        let region = Region::from_paths(&self.paths(toolpath)?, self.tol);
        (!region.is_empty()).then(|| inside_cells(&region, &self.grid, self.tol))
    }

    /// True when a toolpath was made by `strategy` on the flat job, not
    /// projected onto the model (whose design depth this check does not
    /// know).
    fn flat_strategy(toolpath: &Toolpath, strategy: &str) -> bool {
        toolpath
            .params_json
            .get("strategy")
            .and_then(|s| s.as_str())
            == Some(strategy)
            && toolpath.params_json.get("model").is_none()
    }

    /// The V a V-groove cuts: inside its vectors as deep as the distance to
    /// their edge allows for the tool's angle (less half a flat tip), from
    /// the start depth down to the flat depth; outside, the top, less where
    /// a start depth widens the V past the edge.
    fn vgroove_design(&self, toolpath: &Toolpath) -> Option<Design> {
        let params: VGrooveParams =
            serde_json::from_value(toolpath.params_json.get("params")?.clone()).ok()?;
        let (half, tip) = v_shape(&toolpath.tool).ok()?;
        let slope = half.tan();
        if slope <= 0.0 {
            return None;
        }
        let inside = self.inside(toolpath)?;
        let outside: Vec<bool> = inside.iter().map(|i| !i).collect();
        let (w, h, cell) = (self.grid.width, self.grid.height, self.stock.cell);
        // Cell centres to the nearest cell across the edge: never nearer
        // than the edge itself, so the design is never shallower than the
        // V (a cell rounding only lets a gouge through, never makes one).
        let to_edge = distance_to(&outside, w, h);
        let beyond = (params.start_depth > 0.0).then(|| distance_to(&inside, w, h));
        let flat = params.flat_depth.unwrap_or(f64::INFINITY);
        let top = f64::from(self.top);
        let bottom = self.stock.bottom;
        let heights = (0..w * h)
            .map(|i| {
                let depth = if inside[i] {
                    let d = to_edge[i] * cell;
                    params.start_depth + ((d - tip / 2.0).max(0.0) / slope).min(flat)
                } else {
                    // A cell past the edge: the V's wall reaching up from
                    // the start depth at the edge.
                    beyond.as_ref().map_or(0.0, |b| {
                        let d = (b[i] - 1.0).max(0.0) * cell;
                        (params.start_depth - d / slope).max(0.0)
                    })
                };
                (top - depth).max(bottom) as f32
            })
            .collect();
        Some(Design::Heights(heights))
    }

    /// The holes a drilling toolpath drills: the tool's width round the
    /// centre of each closed vector at the hole depth, the top elsewhere.
    fn drill_design(&self, toolpath: &Toolpath) -> Option<Design> {
        let params: DrillParams =
            serde_json::from_value(toolpath.params_json.get("params")?.clone()).ok()?;
        let paths = self.paths(toolpath)?;
        let stock = self.stock;
        let radius = toolpath.tool.radius() + stock.cell * 0.5;
        let floor = (f64::from(self.top) - params.cut_depth).max(stock.bottom) as f32;
        let mut heights = vec![self.top; stock.width * stock.height];
        let reach = (radius / stock.cell).ceil() as i64;
        for sub in paths.iter().flat_map(|p| p.subpaths.iter()) {
            if !sub.closed {
                continue;
            }
            let Some(centre) = path_bbox(&sub.clone().into_path()).map(|b| b.centre()) else {
                continue;
            };
            let (cx, cy) = stock.cell_of(centre.x, centre.y);
            for y in (cy - reach).max(0)..=(cy + reach).min(stock.height as i64 - 1) {
                for x in (cx - reach).max(0)..=(cx + reach).min(stock.width as i64 - 1) {
                    let at = (
                        stock.origin_x + x as f64 * stock.cell,
                        stock.origin_y + y as f64 * stock.cell,
                    );
                    if (at.0 - centre.x).hypot(at.1 - centre.y) <= radius {
                        heights[y as usize * stock.width + x as usize] = floor;
                    }
                }
            }
        }
        Some(Design::Heights(heights))
    }

    /// The design a toolpath holds its cells to, and what it means to
    /// clear: (cell, height) pairs as a list of cells for leftover.
    fn design(&mut self, toolpath: &Toolpath) -> (Design, Vec<(usize, f32)>) {
        let surface = self.top;
        let bottom = self.stock.bottom as f32;
        let floor = |depth: f64| (surface - depth as f32).max(bottom);
        let params = toolpath.params_json.get("params").cloned();
        match toolpath.kind {
            StrategyKind::Rough3d => (Design::Model, Vec::new()),
            StrategyKind::Finish3d => {
                let claim = self.finish_area(toolpath);
                (Design::Model, claim)
            }
            StrategyKind::Pocket => {
                let Some(params) =
                    params.and_then(|p| serde_json::from_value::<PocketParams>(p).ok())
                else {
                    return (Design::Free, Vec::new());
                };
                let Some(inside) = self.inside(toolpath) else {
                    return (Design::Free, Vec::new());
                };
                let inner = floor(params.cut_depth);
                // Walls left at the allowance are not to be cleared.
                let keep = params.allowance.max(0.0) / self.stock.cell;
                let outside: Vec<bool> = inside.iter().map(|i| !i).collect();
                let near =
                    (keep > 0.0).then(|| distance_to(&outside, self.grid.width, self.grid.height));
                let claim = inside
                    .iter()
                    .enumerate()
                    .filter(|(i, inn)| **inn && near.as_ref().is_none_or(|d| d[*i] > keep + 1.0))
                    .map(|(i, _)| (i, inner))
                    .collect();
                (
                    Design::Split {
                        inside,
                        inner,
                        outer: surface,
                    },
                    claim,
                )
            }
            StrategyKind::Profile => {
                let Some(params) =
                    params.and_then(|p| serde_json::from_value::<ProfileParams>(p).ok())
                else {
                    return (Design::Free, Vec::new());
                };
                let cut = floor(params.cut_depth);
                let design = match params.side {
                    ProfileSide::On => Design::Split {
                        inside: Vec::new(),
                        inner: cut,
                        outer: cut,
                    },
                    side => match self.inside(toolpath) {
                        Some(inside) => {
                            let (inner, outer) = if side == ProfileSide::Outside {
                                (surface, cut)
                            } else {
                                (cut, surface)
                            };
                            Design::Split {
                                inside,
                                inner,
                                outer,
                            }
                        }
                        None => Design::Free,
                    },
                };
                (design, Vec::new())
            }
            StrategyKind::VGroove if Self::flat_strategy(toolpath, "vGroove") => (
                self.vgroove_design(toolpath).unwrap_or(Design::Free),
                Vec::new(),
            ),
            StrategyKind::Drill if Self::flat_strategy(toolpath, "drill") => (
                self.drill_design(toolpath).unwrap_or(Design::Free),
                Vec::new(),
            ),
            _ => (Design::Free, Vec::new()),
        }
    }

    /// The cells a 3D finish means to clear: those under the span of its
    /// cutting moves, at the model surface.
    fn finish_area(&mut self, toolpath: &Toolpath) -> Vec<(usize, f32)> {
        let mut span = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (_, one) in toolpath.iter_moves() {
            if one.kind != MoveKind::Rapid {
                span = [
                    span[0].min(one.to.x),
                    span[1].min(one.to.y),
                    span[2].max(one.to.x),
                    span[3].max(one.to.y),
                ];
            }
        }
        if span[0] > span[2] {
            return Vec::new();
        }
        let stock = self.stock;
        let (x0, y0) = stock.cell_of(span[0], span[1]);
        let (x1, y1) = stock.cell_of(span[2], span[3]);
        let clampx = |v: i64| v.clamp(0, stock.width as i64 - 1) as usize;
        let clampy = |v: i64| v.clamp(0, stock.height as i64 - 1) as usize;
        let model = self.model().to_vec();
        let mut out = Vec::new();
        for y in clampy(y0)..=clampy(y1) {
            for x in clampx(x0)..=clampx(x1) {
                let i = y * stock.width + x;
                out.push((i, model[i]));
            }
        }
        out
    }
}

/// The design height of cell `i` under `design`.
fn design_at(design: &Design, model: Option<&[f32]>, i: usize) -> Option<f32> {
    match design {
        Design::Model => model.map(|m| m[i]),
        Design::Split {
            inside,
            inner,
            outer,
        } => Some(if inside.get(i).copied().unwrap_or(false) {
            *inner
        } else {
            *outer
        }),
        Design::Heights(heights) => heights.get(i).copied(),
        Design::Free => None,
    }
}

/// The cells round cell `i` (itself included) on a `width` by `height`
/// grid.
fn around(i: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % width) as i64, (i / width) as i64);
    (-1..=1).flat_map(move |dy| {
        (-1..=1).filter_map(move |dx| {
            let (nx, ny) = (x + dx, y + dy);
            (nx >= 0 && ny >= 0 && nx < width as i64 && ny < height as i64)
                .then(|| ny as usize * width + nx as usize)
        })
    })
}

/// Compares `stock` with the design of the toolpaths of `doc` that cut it,
/// those of both sides of a double sided job, and answers what is more
/// than `tolerance` millimetres off.
pub fn check(doc: &Document, stock: &Stock, tolerance: f64) -> Report {
    let mut report = check_side(doc, stock, tolerance);
    let active = doc.active_side();
    let toolpaths = doc.toolpaths();
    let mut owners: Vec<u16> = stock.owner.clone();
    owners.sort_unstable();
    owners.dedup();
    let theirs: Vec<&Toolpath> = owners
        .iter()
        .filter_map(|&owner| toolpaths.get(usize::from(owner).wrapping_sub(1)).copied())
        .filter(|t| t.side != active)
        .collect();
    // The other side's model comes only for its 3D toolpaths.
    let model = theirs
        .iter()
        .any(|t| matches!(t.kind, StrategyKind::Rough3d | StrategyKind::Finish3d));
    let turned_doc = (!theirs.is_empty())
        .then(|| doc.turned_over(model))
        .flatten();
    let Some(turned_doc) = turned_doc else {
        return report;
    };
    let setup = doc.job_setup();
    let turned = stock.turned(setup, active);
    let theirs = check_side(&turned_doc, &turned, tolerance);
    for (j, &code) in theirs.map.iter().enumerate() {
        let i = turned.turned_cell(j, setup);
        if code != CLEAN && report.map[i] == CLEAN {
            report.map[i] = code;
        }
    }
    for mut issue in theirs.issues {
        (issue.x, issue.y) = setup.flip_xy(issue.x, issue.y);
        issue.z = setup.flip_z(issue.z, active.other());
        report.issues.push(issue);
    }
    report.found += theirs.found;
    worst_first(&mut report.issues);
    report
}

/// Sorts issues the most severe and largest first and keeps the first
/// [`MOST_ISSUES`].
fn worst_first(issues: &mut Vec<SurfaceIssue>) {
    let rank = |s: Severity| match s {
        Severity::High => 0,
        Severity::Medium => 1,
        Severity::Low => 2,
    };
    issues.sort_by(|a, b| {
        rank(a.severity)
            .cmp(&rank(b.severity))
            .then((b.depth * b.area).total_cmp(&(a.depth * a.area)))
    });
    issues.truncate(MOST_ISSUES);
}

/// [`check`] for the toolpaths of the active side of `doc`, `stock` seen
/// from that side.
fn check_side(doc: &Document, stock: &Stock, tolerance: f64) -> Report {
    let cells = stock.width * stock.height;
    let mut map = vec![CLEAN; cells];
    let Some(mut checker) = Checker::new(doc, stock) else {
        return Report {
            map,
            issues: Vec::new(),
            found: 0,
        };
    };
    let toolpaths = doc.toolpaths();
    let active = doc.active_side();
    // The toolpaths that cut the stock, by owner tag.
    let mut owners: Vec<u16> = stock.owner.clone();
    owners.sort_unstable();
    owners.dedup();
    let mut designs: HashMap<u16, Design> = HashMap::new();
    // The height each cell is meant to be cleared to, and by which owner.
    let mut want = vec![f32::NAN; cells];
    let mut claimer = vec![0u16; cells];
    for &owner in owners.iter().filter(|o| **o > 0) {
        let Some(toolpath) = toolpaths.get(usize::from(owner) - 1) else {
            continue;
        };
        if toolpath.side != active {
            continue;
        }
        let (design, claim) = checker.design(toolpath);
        if matches!(design, Design::Model) {
            checker.model();
        }
        for (i, z) in claim {
            if want[i].is_nan() || z < want[i] {
                want[i] = z;
                claimer[i] = owner;
            }
        }
        designs.insert(owner, design);
    }
    let model = checker.model.as_deref();
    let tol = tolerance.max(0.0) as f32;
    let through = |i: usize| stock.floor[i] >= stock.z[i] - THROUGH;
    let (w, h) = (stock.width, stock.height);
    let mut excess = vec![0.0f32; cells];
    for i in 0..cells {
        if through(i) {
            continue;
        }
        let z = stock.z[i];
        let owner = stock.owner[i];
        if let Some(design) = designs.get(&owner) {
            let low = around(i, w, h)
                .filter_map(|j| design_at(design, model, j))
                .fold(f32::INFINITY, f32::min);
            if low.is_finite() && z < low - tol {
                map[i] = GOUGE;
                excess[i] = low - z;
                continue;
            }
        }
        if !want[i].is_nan() {
            let high = around(i, w, h)
                .map(|j| {
                    if want[j].is_nan() {
                        checker.top
                    } else {
                        want[j]
                    }
                })
                .fold(f32::NEG_INFINITY, f32::max);
            if z > high + tol {
                map[i] = LEFTOVER;
                excess[i] = z - want[i];
            }
        }
    }
    // Join the flagged cells into areas.
    let mut areas = Vec::new();
    let mut seen = vec![false; cells];
    for start in 0..cells {
        if map[start] == CLEAN || seen[start] {
            continue;
        }
        let code = map[start];
        let mut members = Vec::new();
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            members.push(i);
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize, stack: &mut Vec<usize>| {
                if !seen[j] && map[j] == code {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1, &mut stack);
            }
            if x + 1 < w {
                visit(i + 1, &mut stack);
            }
            if y > 0 {
                visit(i - w, &mut stack);
            }
            if y + 1 < h {
                visit(i + w, &mut stack);
            }
        }
        areas.push((code, members));
    }
    let found = areas.len();
    let mut reach = Reach::default();
    let mut issues = Vec::new();
    for (code, members) in areas {
        let worst = members
            .iter()
            .copied()
            .max_by(|a, b| excess[*a].total_cmp(&excess[*b]))
            .unwrap_or(members[0]);
        let owner = if code == GOUGE {
            stock.owner[worst]
        } else {
            claimer[worst]
        };
        let Some(toolpath) = toolpaths.get(usize::from(owner).wrapping_sub(1)) else {
            continue;
        };
        let mut kind = if code == GOUGE {
            IssueKind::Gouge
        } else {
            IssueKind::Leftover
        };
        if code == LEFTOVER {
            let stride = (members.len() / REACH_SAMPLES).max(1);
            let sampled: Vec<usize> = members.iter().copied().step_by(stride).collect();
            let blocked = sampled
                .iter()
                .filter(|&&i| !reach.reaches(toolpath, owner, stock, &want, checker.top, i, tol))
                .count();
            if blocked * 2 > sampled.len() {
                kind = IssueKind::Undercut;
                for &i in &members {
                    map[i] = UNDERCUT;
                }
            }
        }
        let depth = f64::from(excess[worst]);
        let area = members.len() as f64 * stock.cell * stock.cell;
        if kind != IssueKind::Gouge && area < LEAST_LISTED_LEFT {
            continue;
        }
        let severity = match kind {
            IssueKind::Gouge if depth >= 0.5 || area >= 25.0 => Severity::High,
            IssueKind::Gouge => Severity::Medium,
            IssueKind::Leftover if depth >= 1.0 => Severity::Medium,
            other => other.severity(),
        };
        let (x, y) = (
            stock.origin_x + (worst % w) as f64 * stock.cell,
            stock.origin_y + (worst / w) as f64 * stock.cell,
        );
        issues.push(SurfaceIssue {
            kind,
            severity,
            toolpath_id: toolpath.id,
            move_index: nearest_move(toolpath, x, y),
            x,
            y,
            z: f64::from(stock.z[worst]),
            depth,
            area,
        });
    }
    worst_first(&mut issues);
    Report { map, issues, found }
}

/// The index of the cutting move of `toolpath` passing nearest job point
/// (`x`, `y`), the deepest of those about as near (the last pass over it).
pub fn nearest_move(toolpath: &Toolpath, x: f64, y: f64) -> usize {
    let at = Point3::new(x, y, 0.0);
    let flat = |p: Point3| (p.x - at.x).hypot(p.y - at.y);
    // (distance, depth, index)
    let mut best = (f64::INFINITY, f64::INFINITY, 0);
    for (index, (from, one)) in toolpath.iter_moves().enumerate() {
        if matches!(one.kind, MoveKind::Rapid | MoveKind::Dwell { .. }) {
            continue;
        }
        let d = match one.kind {
            MoveKind::Feed | MoveKind::Plunge => {
                let (dx, dy) = (one.to.x - from.x, one.to.y - from.y);
                let span = dx * dx + dy * dy;
                let t = if span <= 0.0 {
                    0.0
                } else {
                    (((at.x - from.x) * dx + (at.y - from.y) * dy) / span).clamp(0.0, 1.0)
                };
                (from.x + dx * t - at.x).hypot(from.y + dy * t - at.y)
            }
            _ => (0..=16)
                .filter_map(|k| point_on(from, one, f64::from(k) / 16.0))
                .map(flat)
                .fold(f64::INFINITY, f64::min),
        };
        let low = from.z.min(one.to.z);
        let nearer = d < best.0 - NEAR_ENOUGH;
        let as_near_and_deeper = d <= best.0 + NEAR_ENOUGH && low < best.1 - 1e-9;
        if nearer || as_near_and_deeper {
            best = (d.min(best.0), low, index);
        }
    }
    best.2
}

/// Moves this much nearer or farther, millimetres, count as passing as
/// near.
const NEAR_ENOUGH: f64 = 0.05;

/// Whether a toolpath's tool can reach the design, with the lowest tip
/// height each tool can stand at, kept per tool as it is worked out.
#[derive(Default)]
struct Reach {
    /// By owner: the footprint on its own coarse grid and the tip height
    /// over each of that grid's cells.
    tools: HashMap<u16, ToolReach>,
}

/// One tool's footprint, the cell size it is laid out on and the tip
/// height over each cell of that grid worked out so far.
type ToolReach = (Footprint, f64, HashMap<(i64, i64), f32>);

impl Reach {
    /// True when the tool of `toolpath` can stand somewhere so that its
    /// bottom meets cell `i`'s design `want` without cutting below the
    /// design (or the uncut top where nothing is meant to be cut) anywhere.
    #[allow(clippy::too_many_arguments)]
    fn reaches(
        &mut self,
        toolpath: &Toolpath,
        owner: u16,
        stock: &Stock,
        want: &[f32],
        top: f32,
        i: usize,
        tol: f32,
    ) -> bool {
        let radius = toolpath.tool.radius();
        let entry = self.tools.entry(owner).or_insert_with(|| {
            // At most about thirty cells across the tool.
            let cell = stock.cell.max(radius / 15.0);
            let footprint = Footprint::of(&toolpath.tool, cell).unwrap_or_else(|_| {
                Footprint::of(&super::tool::Tool::default(), cell).expect("a default tool")
            });
            (footprint, cell, HashMap::new())
        });
        let (footprint, cell, tips) = entry;
        let cell = *cell;
        let design = |x: f64, y: f64| -> Option<f32> {
            let (cx, cy) = stock.cell_of(x, y);
            if cx < 0 || cy < 0 || cx >= stock.width as i64 || cy >= stock.height as i64 {
                return None;
            }
            let v = want[cy as usize * stock.width + cx as usize];
            Some(if v.is_nan() { top } else { v })
        };
        let px = stock.origin_x + (i % stock.width) as f64 * stock.cell;
        let py = stock.origin_y + (i / stock.width) as f64 * stock.cell;
        let Some(goal) = design(px, py) else {
            return true;
        };
        let (gx, gy) = ((px / cell).round() as i64, (py / cell).round() as i64);
        for &(dx, dy, rise) in footprint.cells() {
            // The tool centred at (gx - dx, gy - dy) meets the cell with
            // its bottom `rise` above the tip.
            let (cx, cy) = (gx - i64::from(dx), gy - i64::from(dy));
            let tip = *tips.entry((cx, cy)).or_insert_with(|| {
                footprint
                    .cells()
                    .iter()
                    .filter_map(|&(ox, oy, r)| {
                        design(
                            (cx + i64::from(ox)) as f64 * cell,
                            (cy + i64::from(oy)) as f64 * cell,
                        )
                        .map(|d| d - r)
                    })
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            if tip.is_finite() && tip + rise <= goal + tol {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::sim::Quality;
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::cam::toolpath::Move;
    use crate::doc::{Entity, EntityKind, JobSetup};
    use crate::geom::rect;
    use serde_json::json;

    fn job() -> JobSetup {
        JobSetup {
            width: 100.0,
            height: 80.0,
            thickness: 20.0,
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

    /// A document with a rectangle vector and a pocket of it, `depth` down,
    /// whose moves are `moves`.
    fn pocket_doc(tool: Tool, depth: f64, moves: Vec<Move>) -> Document {
        let mut doc = Document::new();
        doc.set_job(job()).expect("job");
        add_pocket(&mut doc, tool, depth, moves);
        doc
    }

    /// Adds the rectangle and its pocket to the active side of `doc`.
    fn add_pocket(doc: &mut Document, tool: Tool, depth: f64, moves: Vec<Move>) {
        let square = rect(Point::new(20.0, 20.0), Point::new(60.0, 50.0)).expect("rect");
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path: square },
        });
        doc.commit();
        let mut tp = Toolpath::new(1, "pocket", tool, StrategyKind::Pocket);
        tp.params_json = json!({
            "strategy": "pocket",
            "params": { "cutDepth": depth },
            "ids": [id],
        });
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.moves = moves;
        doc.add_toolpath(tp);
    }

    /// Raster lines across the square at `z`, `step` apart, the tool
    /// `inset` in from each side, and a last pass round the sides.
    fn raster(z: f64, inset: f64, step: f64) -> Vec<Move> {
        let mut moves = vec![Move::rapid(Point3::new(20.0 + inset, 20.0 + inset, 5.0))];
        moves.push(Move::plunge(Point3::new(20.0 + inset, 20.0 + inset, z)));
        let mut y = 20.0 + inset;
        let mut right = true;
        while y <= 50.0 - inset + 1e-9 {
            let x = if right { 60.0 - inset } else { 20.0 + inset };
            moves.push(Move::feed(Point3::new(x, y, z)));
            y += step;
            if y <= 50.0 - inset + 1e-9 {
                moves.push(Move::feed(Point3::new(x, y, z)));
            }
            right = !right;
        }
        let (a, b) = (20.0 + inset, 60.0 - inset);
        let (c, d) = (20.0 + inset, 50.0 - inset);
        for (x, y) in [(a, c), (b, c), (b, d), (a, d), (a, c)] {
            moves.push(Move::feed(Point3::new(x, y, z)));
        }
        moves.push(Move::rapid(Point3::new(20.0, 20.0, 5.0)));
        moves
    }

    fn cut(doc: &Document) -> Stock {
        let mut stock = Stock::new(doc.job(), Quality::Standard);
        for (k, tp) in doc.toolpaths().into_iter().enumerate() {
            stock.cut(tp, (k + 1) as u16).expect("cuts");
        }
        stock
    }

    #[test]
    fn a_pocket_cut_as_designed_has_no_issues() {
        let doc = pocket_doc(end_mill(6.0), 3.0, raster(-3.0, 3.0, 2.0));
        let report = check(&doc, &cut(&doc), 0.1);
        // Only the round corners an end mill leaves in a square pocket,
        // painted as undercuts but too small to list.
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(report.found, 4);
        assert!(report.map.contains(&UNDERCUT));
        assert!(!report.map.contains(&GOUGE));
    }

    #[test]
    fn an_oversize_finish_tool_gouges_the_walls() {
        // A pass meant for a 6 mm tool cut with a 10 mm one: 2 mm into
        // every wall.
        let doc = pocket_doc(end_mill(10.0), 3.0, raster(-3.0, 3.0, 2.0));
        let report = check(&doc, &cut(&doc), 0.1);
        let gouge = report
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::Gouge)
            .expect("a gouge");
        assert_eq!(gouge.severity, Severity::High);
        assert!((gouge.depth - 3.0).abs() < 0.2, "{gouge:?}");
        assert_eq!(gouge.toolpath_id, doc.toolpaths()[0].id);
        // Outside the square, where the pocket must leave the top.
        let inside = gouge.x > 20.0 && gouge.x < 60.0 && gouge.y > 20.0 && gouge.y < 50.0;
        assert!(!inside, "{gouge:?}");
        assert!(report.map.contains(&GOUGE));
    }

    #[test]
    fn a_pocket_cut_too_deep_is_a_gouge() {
        let doc = pocket_doc(end_mill(6.0), 3.0, raster(-3.6, 3.0, 2.0));
        let report = check(&doc, &cut(&doc), 0.1);
        let gouge = report.issues.first().expect("an issue");
        assert_eq!(gouge.kind, IssueKind::Gouge);
        assert!((gouge.depth - 0.6).abs() < 0.05, "{gouge:?}");
        // Nothing at a looser tolerance.
        let loose = check(&doc, &cut(&doc), 1.0);
        assert!(loose.issues.iter().all(|i| i.kind != IssueKind::Gouge));
    }

    /// The leftover the preview check paints after an offset pocket of
    /// `shape` with a 6 mm end mill a `stepover` apart: the stock cells
    /// marked leftover, and the areas it lists.
    fn offset_pocket_leftover(
        shape: &crate::geom::Path,
        stepover: f64,
    ) -> (usize, Vec<SurfaceIssue>) {
        use crate::cam::pocket::{pocket, PocketParams};
        use crate::geom::Tolerance;
        let tool = end_mill(6.0);
        let params = PocketParams {
            cut_depth: 3.0,
            pass_depth: Some(3.0),
            stepover: Some(stepover),
            ..PocketParams::default()
        };
        let tol = Tolerance::new(0.01).expect("tolerance");
        let done =
            pocket(std::slice::from_ref(shape), &job(), &tool, &params, tol).expect("pockets");
        let mut doc = Document::new();
        doc.set_job(job()).expect("job");
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: shape.clone(),
            },
        });
        doc.commit();
        let mut tp = Toolpath::new(1, "pocket", tool, StrategyKind::Pocket);
        tp.params_json = json!({
            "strategy": "pocket",
            "params": { "cutDepth": 3.0 },
            "ids": [id],
        });
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.moves = done.moves;
        doc.add_toolpath(tp);
        let report = check(&doc, &cut(&doc), 0.1);
        // The clean up never runs into the walls.
        assert!(!report.map.contains(&GOUGE), "a gouge at {stepover}");
        let cells = report.map.iter().filter(|c| **c == LEFTOVER).count();
        let listed = report
            .issues
            .into_iter()
            .filter(|i| i.kind == IssueKind::Leftover)
            .collect();
        (cells, listed)
    }

    #[test]
    fn an_offset_pocket_cleans_its_corners_when_the_stepover_is_wide() {
        // A stepover past the tool radius leaves the corners of the rings
        // standing unless each corner is cleaned up; the preview check
        // paints no leftover once it is.
        let square = rect(Point::new(20.0, 20.0), Point::new(60.0, 50.0)).expect("rect");
        let triangle = crate::geom::polyline_path(
            &[
                Point::new(10.0, 10.0),
                Point::new(90.0, 10.0),
                Point::new(30.0, 60.0),
            ],
            true,
        )
        .expect("triangle");
        for (name, shape) in [("square", &square), ("triangle", &triangle)] {
            for stepover in [2.0, 3.5, 4.5, 5.5] {
                // Without the clean up the square at 5.5 and the triangle at
                // 4.5 and 5.5 left ring corners (and the triangle's middle).
                let (cells, listed) = offset_pocket_leftover(shape, stepover);
                assert_eq!(cells, 0, "{name} at {stepover}: {listed:?}");
            }
        }
    }

    #[test]
    fn a_pass_that_stops_short_leaves_leftover_the_tool_could_reach() {
        let mut moves = raster(-3.0, 3.0, 2.0);
        // Drop the second half of the raster.
        moves.truncate(moves.len() / 2);
        let doc = pocket_doc(end_mill(6.0), 3.0, moves);
        let report = check(&doc, &cut(&doc), 0.1);
        let left = report
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::Leftover)
            .expect("leftover");
        assert!(left.area > 300.0, "{left:?}");
        assert!((left.depth - 3.0).abs() < 1e-3, "{left:?}");
        assert!(report.map.contains(&LEFTOVER));
        assert!(left.move_index > 0);
    }

    #[test]
    fn the_other_sides_pocket_is_held_to_its_own_design() {
        use crate::doc::{JobKind, Side, ZZero};
        // The pocket cut 0.6 too deep from the bottom of a double sided
        // job whose bottom side has Z zero on the bed (its top at 20).
        let mut setup = JobSetup {
            kind: JobKind::DoubleSided,
            ..job()
        };
        setup.double_sided.bottom_z_zero = ZZero::Bottom;
        let side_doc = |depth: f64| {
            let mut doc = Document::new();
            doc.set_job(setup).expect("job");
            doc.set_active_side(Side::Bottom).expect("bottom");
            let mut moves = raster(depth, 3.0, 2.0);
            for one in &mut moves {
                one.to.z += 20.0;
            }
            add_pocket(&mut doc, end_mill(6.0), 3.0, moves);
            doc.set_active_side(Side::Top).expect("top");
            let mut stock = Stock::new(doc.job(), Quality::Standard);
            let tp = doc.toolpaths()[0];
            stock
                .cut_for(doc.job_setup(), Side::Top, tp, 1)
                .expect("cuts");
            (doc, stock)
        };
        let (doc, stock) = side_doc(-3.6);
        let report = check(&doc, &stock, 0.1);
        let gouge = report.issues.first().expect("an issue");
        assert_eq!(gouge.kind, IssueKind::Gouge);
        assert!((gouge.depth - 0.6).abs() < 0.05, "{gouge:?}");
        assert_eq!(gouge.toolpath_id, doc.toolpaths()[0].id);
        // Where the bottom side's pocket lies, turned to the top's space,
        // at the underside's height there.
        let (x, y) = setup.flip_xy(gouge.x, gouge.y);
        assert!(x > 20.0 && x < 60.0 && y > 20.0 && y < 50.0, "{gouge:?}");
        assert!((gouge.z - (-20.0 + 3.6)).abs() < 0.05, "{gouge:?}");
        let (cx, cy) = stock.cell_of(gouge.x, gouge.y);
        assert_eq!(report.map[cy as usize * stock.width + cx as usize], GOUGE);
        // Cut as designed: only the corners too small to list.
        let (doc, stock) = side_doc(-3.0);
        let report = check(&doc, &stock, 0.1);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(report.found, 4);
        assert!(!report.map.contains(&GOUGE));
    }

    #[test]
    fn the_other_sides_3d_finish_is_held_to_its_own_model() {
        use crate::doc::{JobKind, Side};
        use crate::model::field::Field;
        let setup = JobSetup {
            kind: JobKind::DoubleSided,
            ..job()
        };
        // A plateau under the whole finish on the bottom side, finished
        // `below` under the model.
        let side_doc = |below: f64| {
            let mut doc = Document::new();
            doc.set_job(setup).expect("job");
            doc.set_active_side(Side::Bottom).expect("bottom");
            doc.begin("Plateau");
            doc.add_component(crate::model::Component {
                id: 0,
                name: "plateau".into(),
                visible: true,
                combine: crate::model::Combine::default(),
                height_scale: 1.0,
                level: 0,
                extra: crate::model::tree::ComponentExtra::default(),
                field: Field {
                    width: 140,
                    height: 120,
                    cell: 0.5,
                    origin: Point::new(5.0, 5.0),
                    z: vec![2.0; 140 * 120],
                },
            });
            doc.commit();
            let flat = Surface::of(&doc).z_at(Point::new(40.0, 35.0));
            let mut tp = Toolpath::new(1, "finish", end_mill(6.0), StrategyKind::Finish3d);
            tp.home = Point3::new(0.0, 0.0, 20.0);
            tp.moves = raster(flat - below, 3.0, 2.0);
            doc.add_toolpath(tp);
            doc.set_active_side(Side::Top).expect("top");
            let mut stock = Stock::new(doc.job(), Quality::Standard);
            let tp = doc.toolpaths()[0];
            stock
                .cut_for(doc.job_setup(), Side::Top, tp, 1)
                .expect("cuts");
            (doc, stock)
        };
        // Cut to the model: nothing gouged. Without the model the cut
        // would read as leftover above a flat design.
        let (doc, stock) = side_doc(0.0);
        let report = check(&doc, &stock, 0.1);
        assert!(!report.map.contains(&GOUGE), "{:?}", report.issues);
        let (doc, stock) = side_doc(1.0);
        let report = check(&doc, &stock, 0.1);
        let gouge = report.issues.first().expect("an issue");
        assert_eq!(gouge.kind, IssueKind::Gouge);
        assert!((gouge.depth - 1.0).abs() < 0.05, "{gouge:?}");
    }

    /// A document with a rectangle vector and a toolpath of `strategy`
    /// cut with `tool` along `moves`, `params` as its form sent them.
    fn made_doc(
        strategy: &str,
        kind: StrategyKind,
        tool: Tool,
        params: serde_json::Value,
        moves: impl FnOnce(&[crate::geom::Path], &JobSetup) -> Vec<Move>,
    ) -> Document {
        let mut doc = Document::new();
        doc.set_job(job()).expect("job");
        let square = rect(Point::new(20.0, 20.0), Point::new(60.0, 50.0)).expect("rect");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: square.clone(),
            },
        });
        let mut tp = Toolpath::new(1, strategy, tool, kind);
        tp.params_json = json!({ "strategy": strategy, "params": params, "ids": [id] });
        tp.home = Point3::new(0.0, 0.0, 20.0);
        tp.moves = moves(&[square], doc.job());
        doc.add_toolpath(tp);
        doc
    }

    /// Moves `down` millimetres deeper wherever they cut.
    fn lowered(moves: Vec<Move>, down: f64) -> Vec<Move> {
        moves
            .into_iter()
            .map(|mut one| {
                if one.to.z < -1e-9 {
                    one.to.z -= down;
                }
                one
            })
            .collect()
    }

    fn gouges(report: &Report) -> Vec<&SurfaceIssue> {
        report
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Gouge)
            .collect()
    }

    #[test]
    fn a_vgroove_is_held_to_its_v() {
        let tool = Tool {
            name: "v".into(),
            tool_type: ToolGeometry::VBit {
                diameter: 40.0,
                included_angle: 90.0,
            },
            pass_depth: 20.0,
            ..Tool::default()
        };
        let tol = Tolerance::new(0.01).expect("tol");
        for (start, flat) in [(0.0, None), (1.0, None), (0.0, Some(6.0))] {
            let params = crate::cam::vgroove::VGrooveParams {
                start_depth: start,
                flat_depth: flat,
                ..Default::default()
            };
            let form = serde_json::to_value(&params).expect("params");
            let carve = |down: f64| {
                let (tool, params) = (tool.clone(), params.clone());
                made_doc(
                    "vGroove",
                    StrategyKind::VGroove,
                    tool.clone(),
                    form.clone(),
                    move |paths, job| {
                        let cut = crate::cam::vgroove::vgroove(paths, job, &tool, &params, tol)
                            .expect("carves");
                        lowered(cut.moves, down)
                    },
                )
            };
            // Cut as calculated: nothing, a start depth widening the V past
            // the vectors and a flat depth included.
            let doc = carve(0.0);
            let report = check(&doc, &cut(&doc), 0.1);
            assert!(
                gouges(&report).is_empty(),
                "{start} {flat:?} {:?}",
                report.issues
            );
            assert!(!report.map.contains(&GOUGE), "{start} {flat:?}");
            // Half a millimetre deeper everywhere: a gouge that deep.
            let doc = carve(0.5);
            let report = check(&doc, &cut(&doc), 0.1);
            let found = gouges(&report);
            assert!(!found.is_empty(), "{start} {flat:?}");
            let worst = found.iter().map(|g| g.depth).fold(0.0, f64::max);
            assert!(worst > 0.3 && worst < 0.7, "{start} {flat:?} {found:?}");
        }
    }

    #[test]
    fn a_drilled_hole_is_held_to_its_depth() {
        let tool = end_mill(6.0);
        let params = crate::cam::drill::DrillParams {
            cut_depth: 5.0,
            ..Default::default()
        };
        let form = serde_json::to_value(params).expect("params");
        let drilled = |down: f64| {
            made_doc(
                "drill",
                StrategyKind::Drill,
                tool.clone(),
                form.clone(),
                move |paths, job| {
                    let cut = crate::cam::drill::drill(paths, job, &params).expect("drills");
                    lowered(cut.moves, down)
                },
            )
        };
        let doc = drilled(0.0);
        let report = check(&doc, &cut(&doc), 0.1);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        let doc = drilled(0.6);
        let report = check(&doc, &cut(&doc), 0.1);
        let gouge = report.issues.first().expect("a gouge");
        assert_eq!(gouge.kind, IssueKind::Gouge);
        assert!((gouge.depth - 0.6).abs() < 0.05, "{gouge:?}");
        // At the centre of the rectangle.
        assert!((gouge.x - 40.0).abs() < 3.5 && (gouge.y - 35.0).abs() < 3.5);
        // A toolpath projected onto the model is not held to a flat depth.
        let mut projected = drilled(0.6);
        let id = projected.toolpaths()[0].id;
        let mut tp = projected.toolpaths()[0].clone();
        tp.params_json["model"] = json!("fingerprint");
        projected.delete_toolpaths(&[id]).expect("removes");
        projected.add_toolpath(tp);
        let report = check(&projected, &cut(&projected), 0.1);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
    }

    #[test]
    fn an_issue_goes_to_the_deepest_pass_over_it() {
        let mut tp = Toolpath::new(1, "passes", end_mill(6.0), StrategyKind::Profile);
        tp.home = Point3::new(0.0, 0.0, 10.0);
        tp.moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, 5.0)),
            Move::plunge(Point3::new(10.0, 10.0, -1.0)),
            Move::feed(Point3::new(50.0, 10.0, -1.0)),
            Move::plunge(Point3::new(50.0, 10.0, -3.0)),
            Move::feed(Point3::new(10.0, 10.0, -3.0)),
            Move::rapid(Point3::new(10.0, 10.0, 5.0)),
        ];
        assert_eq!(nearest_move(&tp, 30.0, 11.0), 4);
        assert_eq!(nearest_move(&tp, 60.0, 10.0), 3);
    }

    #[test]
    fn leftover_from_a_too_large_tool_is_found_where_it_cannot_reach() {
        // A 40 by 30 pocket plunged at its middle with a 40 mm tool: the
        // tool is too wide to go into the pocket without cutting its walls,
        // and its four corners stay where no placement could reach them.
        let tool = end_mill(40.0);
        let moves = vec![
            Move::rapid(Point3::new(40.0, 35.0, 5.0)),
            Move::plunge(Point3::new(40.0, 35.0, -3.0)),
            Move::rapid(Point3::new(40.0, 35.0, 5.0)),
        ];
        let doc = pocket_doc(tool, 3.0, moves);
        let report = check(&doc, &cut(&doc), 0.1);
        let corners: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Undercut)
            .collect();
        assert_eq!(corners.len(), 4, "{:?}", report.issues);
        assert!(corners.iter().all(|c| (c.depth - 3.0).abs() < 1e-3));
        assert!(report.map.contains(&UNDERCUT));
    }
}
