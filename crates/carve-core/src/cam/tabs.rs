//! Tab placement for the Edit Tabs dialog: tabs placed automatically along
//! the selected vectors (a constant number or a constant distance apart,
//! with the first at the machining start point or avoiding corners and
//! curved regions), added, deleted and moved by clicks in the 2D view, and
//! laid onto a profile's cut when the toolpath is calculated.
//!
//! Tab positions are points on the vectors. A profile's cut runs beside
//! its vector, so each position is carried to the nearest point of the cut
//! within reach of it. Each calculation also anchors every position to its
//! vector (which loop, how far along it), so that tabs follow a vector that
//! was moved, scaled or edited since they were placed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::{Point, Tolerance};

use super::chain::Chain;
use super::common::{TabPlacement, TabSpec};

/// Turn at a joint, in degrees, above which the joint is a corner (or part
/// of a flattened curve) that automatic tabs keep away from.
const CORNER_TURN: f64 = 1.0;

/// Tab intervals (from, to along the chain, in millimetres, sorted) for the
/// given tab positions: each position within `reach` of the chain puts a
/// tab of the spec's length centred on the nearest point of the chain.
pub fn intervals_at(
    spec: &TabSpec,
    chain: &Chain,
    positions: &[Point],
    reach: f64,
) -> Vec<(f64, f64)> {
    let length = chain.length();
    if !spec.enabled || spec.length <= 0.0 || length <= 0.0 {
        return Vec::new();
    }
    let half = (spec.length / 2.0).min(length / 2.0);
    let mut spans: Vec<(f64, f64)> = positions
        .iter()
        .filter_map(|p| chain.nearest(*p))
        .filter(|(_, gap)| *gap <= reach)
        .map(|(s, _)| {
            let centre = s.clamp(half, length - half);
            (centre - half, centre + half)
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

/// Stretches of a chain that automatic tabs avoid: arcs, and joints that
/// turn by more than [`CORNER_TURN`] (as points).
fn corners(chain: &Chain) -> Vec<(f64, f64)> {
    let mut bad = Vec::new();
    let mut walked = 0.0;
    let count = chain.segs.len();
    for (i, seg) in chain.segs.iter().enumerate() {
        let length = seg.length();
        if seg.arc().is_some() {
            bad.push((walked, walked + length));
        }
        walked += length;
        let next = if i + 1 < count {
            chain.segs.get(i + 1)
        } else if chain.closed {
            chain.segs.first()
        } else {
            None
        };
        if let Some(next) = next {
            if seg.arc().is_none() && next.arc().is_none() {
                let a = (seg.to.y - seg.from.y).atan2(seg.to.x - seg.from.x);
                let b = (next.to.y - next.from.y).atan2(next.to.x - next.from.x);
                let mut turn = (b - a).abs() % std::f64::consts::TAU;
                if turn > std::f64::consts::PI {
                    turn = std::f64::consts::TAU - turn;
                }
                if turn.to_degrees() > CORNER_TURN {
                    let at = if i + 1 < count { walked } else { 0.0 };
                    bad.push((at, at));
                }
            }
        }
    }
    bad
}

/// Whether a tab centred at `s` (with a margin each side) keeps clear of
/// the corners on a closed chain `length` long.
fn clear_of(s: f64, half: f64, margin: f64, length: f64, bad: &[(f64, f64)]) -> bool {
    let (lo, hi) = (s - half - margin, s + half + margin);
    bad.iter().all(|&(a, b)| {
        [-length, 0.0, length]
            .iter()
            .all(|shift| b + shift < lo || a + shift > hi)
    })
}

/// Tab centres along one closed chain, as the spec places them.
fn centres(spec: &TabSpec, chain: &Chain) -> Vec<f64> {
    let length = chain.length();
    let count = match spec.spacing {
        // Tabs closer than their own length would overlap: a tiny spacing
        // places them a tab length apart instead of millions per loop.
        Some(apart) if apart.is_finite() && apart > 1e-9 => {
            ((length / apart.max(spec.length)).round() as usize).max(1)
        }
        _ => spec.count,
    };
    if count == 0 || spec.length <= 0.0 || length <= 0.0 {
        return Vec::new();
    }
    let spacing = length / count as f64;
    let half = (spec.length / 2.0).min(spacing / 2.0);
    let even: Vec<f64> = (0..count)
        .map(|i| match spec.placement {
            TabPlacement::FirstAtStart => half + spacing * i as f64,
            _ => spacing * (i as f64 + 0.5),
        })
        .collect();
    if spec.placement != TabPlacement::AvoidCorners {
        return even;
    }
    let bad = corners(chain);
    let margin = half;
    let step = (length / 1000.0).max(0.1);
    let limit = spacing / 2.0 - half;
    even.into_iter()
        .map(|s| {
            let mut k = 0.0;
            while k * step <= limit {
                for off in [k * step, -k * step] {
                    let at = (s + off).rem_euclid(length);
                    if clear_of(at, half, margin, length, &bad) {
                        return at;
                    }
                }
                k += 1.0;
            }
            s
        })
        .collect()
}

/// Tab positions placed automatically on every closed chain.
pub fn auto_positions(spec: &TabSpec, chains: &[Chain]) -> Vec<Point> {
    chains
        .iter()
        .filter(|c| c.closed && !c.is_empty())
        .flat_map(|chain| {
            centres(spec, chain)
                .into_iter()
                .filter_map(|s| chain.point_at(s))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The nearest point on any of the chains to `p`, with the chain's index
/// and the distance.
fn nearest_on(chains: &[Chain], p: Point) -> Option<(usize, Point, f64)> {
    chains
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let (s, gap) = c.nearest(p)?;
            Some((i, c.point_at(s)?, gap))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
}

fn nearest_tab(positions: &[Point], p: Point, reach: f64) -> Option<usize> {
    positions
        .iter()
        .enumerate()
        .map(|(i, q)| (i, q.distance(p)))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Furthest a tab position may lie from its vector to be anchored to it,
/// in millimetres (placed tabs lie on the vector).
const ANCHOR_REACH: f64 = 0.5;

/// A tab kept on its vector: which vector, which of its closed loops, how
/// far along the loop (a fraction of its length), and the point it gave,
/// which tells an anchored position from one edited since.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabAnchor {
    /// The vector.
    pub id: u64,
    /// Which of the vector's closed loops, in order.
    pub loop_index: usize,
    /// How far along the loop, 0 at its start to 1 at its end.
    pub along: f64,
    /// Where the tab was when it was anchored.
    pub at: Point,
}

/// The closed loops of the vectors `ids`, with each one's vector and index.
fn loops_of(doc: &Document, ids: &[u64], tol: Tolerance) -> Vec<(u64, usize, Chain)> {
    let mut out = Vec::new();
    for id in ids {
        let closed = doc
            .paths_of(*id)
            .into_iter()
            .flat_map(|(_, path)| path.subpaths.into_iter().filter(|s| s.closed))
            .map(|s| Chain::from_subpath(&s, tol))
            .enumerate();
        out.extend(
            closed
                .filter(|(_, c)| !c.is_empty())
                .map(|(i, c)| (*id, i, c)),
        );
    }
    out
}

/// The anchor of a tab position on the nearest loop within reach of it.
fn anchor(loops: &[(u64, usize, Chain)], p: Point) -> Option<TabAnchor> {
    loops
        .iter()
        .filter_map(|(id, index, chain)| {
            let (s, gap) = chain.nearest(p)?;
            let length = chain.length();
            (gap <= ANCHOR_REACH && length > 0.0).then_some((*id, *index, s / length, gap))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(id, loop_index, along, _)| TabAnchor {
            id,
            loop_index,
            along,
            at: p,
        })
}

/// Where an anchor puts its tab on the loops as they are now.
fn resolve(loops: &[(u64, usize, Chain)], a: &TabAnchor) -> Option<Point> {
    let (_, _, chain) = loops
        .iter()
        .find(|(id, index, _)| *id == a.id && *index == a.loop_index)?;
    chain.point_at(a.along.clamp(0.0, 1.0) * chain.length())
}

/// Profile params with their tab positions carried along with their
/// vectors: a position still where its anchor last put it moves to where
/// the anchor puts it now; any other position (placed or edited since) is
/// anchored where it is. Writes both `tabPositions` and `tabAnchors`;
/// params without tab positions come back as they are.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the positions or anchors do not parse.
pub fn anchor_positions(doc: &Document, ids: &[u64], params: &mut Value) -> Result<(), CoreError> {
    let positions: Option<Vec<Point>> = serde_json::from_value(params["tabPositions"].clone())
        .map_err(|e| CoreError::invalid_args("tabPositions", e))?;
    let Some(positions) = positions else {
        return Ok(());
    };
    // Anchors that do not parse (from a newer version) are made again.
    let anchors: Vec<Option<TabAnchor>> =
        serde_json::from_value(params["tabAnchors"].clone()).unwrap_or_default();
    let tol = Tolerance::new(super::compute::CUT_TOLERANCE)?;
    let loops = loops_of(doc, ids, tol);
    let mut moved = Vec::with_capacity(positions.len());
    let mut kept = Vec::with_capacity(positions.len());
    for p in positions {
        let known = anchors
            .iter()
            .flatten()
            .find(|a| a.at.distance(p) < 1e-6)
            .and_then(|a| resolve(&loops, a));
        let at = known.unwrap_or(p);
        moved.push(at);
        kept.push(anchor(&loops, at));
    }
    params["tabPositions"] = json!(moved);
    params["tabAnchors"] = json!(kept);
    Ok(())
}

/// What the Edit Tabs dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TabOp {
    /// Add Tabs: place them by the spec, replacing the ones there were.
    Auto,
    /// A click in the 2D view: delete the tab there, or add one on the
    /// vector there.
    Click,
    /// A drag: move the tab at `from` along its vector to `to`.
    Move,
    /// Delete All Tabs.
    Clear,
}

/// `cam.tabs`: `{ ids, tabs, positions, op, at?, from?, reach }` answers
/// `{ positions }`, the tab positions after the operation.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::NotFound`] for an id that is not in the document.
pub fn tabs_command(doc: &Document, args: &str) -> Result<Value, CoreError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        #[serde(default)]
        ids: Vec<u64>,
        #[serde(default)]
        tabs: TabSpec,
        #[serde(default)]
        positions: Vec<Point>,
        op: TabOp,
        at: Option<Point>,
        from: Option<Point>,
        #[serde(default)]
        reach: f64,
    }
    let args: Args =
        serde_json::from_str(args).map_err(|e| CoreError::invalid_args("cam.tabs", e))?;
    let tol = Tolerance::new(super::compute::CUT_TOLERANCE)?;
    let mut chains = Vec::new();
    for id in &args.ids {
        doc.entity(*id)?;
        for (_, path) in doc.paths_of(*id) {
            chains.extend(
                path.subpaths
                    .iter()
                    .filter(|s| s.closed)
                    .map(|s| Chain::from_subpath(s, tol))
                    .filter(|c| !c.is_empty()),
            );
        }
    }
    let mut positions = args.positions;
    match args.op {
        TabOp::Auto => positions = auto_positions(&args.tabs, &chains),
        TabOp::Clear => positions.clear(),
        TabOp::Click => {
            if let Some(at) = args.at {
                if let Some(i) = nearest_tab(&positions, at, args.reach) {
                    positions.remove(i);
                } else if let Some((_, p, gap)) = nearest_on(&chains, at) {
                    if gap <= args.reach {
                        positions.push(p);
                    }
                }
            }
        }
        TabOp::Move => {
            if let (Some(from), Some(to)) = (args.from, args.at) {
                if let Some(i) = nearest_tab(&positions, from, args.reach) {
                    let old = positions.remove(i);
                    let chain = nearest_on(&chains, old).map(|(c, _, _)| c);
                    let moved = chain
                        .and_then(|c| chains.get(c))
                        .and_then(|c| c.nearest(to).and_then(|(s, _)| c.point_at(s)));
                    positions.push(moved.unwrap_or(old));
                }
            }
        }
    }
    Ok(json!({ "positions": positions }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, rect};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn square_chain() -> Chain {
        let path = rect(Point::new(0.0, 0.0), Point::new(100.0, 100.0)).expect("rect");
        Chain::from_subpath(&path.subpaths[0], tol())
    }

    fn spec(count: usize, placement: TabPlacement) -> TabSpec {
        TabSpec {
            enabled: true,
            count,
            length: 10.0,
            placement,
            ..TabSpec::default()
        }
    }

    fn near_corner(p: Point) -> bool {
        let dx = p.x.min(100.0 - p.x);
        let dy = p.y.min(100.0 - p.y);
        dx < 10.0 && dy < 10.0
    }

    #[test]
    fn chains_find_their_nearest_point_on_lines_and_arcs() {
        let chain = square_chain();
        let (s, gap) = chain.nearest(Point::new(50.0, -3.0)).expect("near");
        assert!((gap - 3.0).abs() < 1e-9);
        assert!((chain.point_at(s).expect("point").x - 50.0).abs() < 1e-9);
        let round = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let ring = Chain::from_subpath(&round.subpaths[0], tol());
        let (s, gap) = ring.nearest(Point::new(0.0, 15.0)).expect("near");
        assert!((gap - 5.0).abs() < 1e-6);
        let p = ring.point_at(s).expect("point");
        assert!(p.distance(Point::new(0.0, 10.0)) < 1e-6, "{p:?}");
    }

    #[test]
    fn even_tabs_can_land_on_corners_unless_avoided() {
        let chain = square_chain();
        let even = auto_positions(&spec(4, TabPlacement::Even), std::slice::from_ref(&chain));
        assert_eq!(even.len(), 4);
        let avoided = auto_positions(&spec(4, TabPlacement::AvoidCorners), &[chain]);
        assert_eq!(avoided.len(), 4);
        // Square starts at a corner: even centres sit mid side, already clear.
        assert!(avoided.iter().all(|p| !near_corner(*p)), "{avoided:?}");
        // Three tabs: the middle one would sit on the far corner.
        let chain = square_chain();
        let three = auto_positions(&spec(3, TabPlacement::Even), std::slice::from_ref(&chain));
        assert!(three.iter().any(|p| near_corner(*p)), "{three:?}");
        let clear = auto_positions(&spec(3, TabPlacement::AvoidCorners), &[chain]);
        assert!(clear.iter().all(|p| !near_corner(*p)), "{clear:?}");
    }

    #[test]
    fn a_tiny_spacing_places_tabs_a_tab_length_apart() {
        let chain = square_chain();
        let tight = TabSpec {
            spacing: Some(0.0001),
            ..spec(0, TabPlacement::Even)
        };
        // A 400 mm loop with 10 mm tabs holds 40, not four million.
        assert_eq!(auto_positions(&tight, &[chain]).len(), 40);
    }

    #[test]
    fn the_first_tab_can_sit_at_the_start_point() {
        let chain = square_chain();
        let at_start = auto_positions(&spec(2, TabPlacement::FirstAtStart), &[chain]);
        assert!(
            at_start[0].distance(Point::new(5.0, 0.0)) < 1e-9,
            "{at_start:?}"
        );
    }

    #[test]
    fn positions_become_tabs_on_a_cut_beside_the_vector() {
        let path = rect(Point::new(-3.0, -3.0), Point::new(103.0, 103.0)).expect("rect");
        let cut = Chain::from_subpath(&path.subpaths[0], tol());
        let spans = intervals_at(
            &spec(0, TabPlacement::Even),
            &cut,
            &[Point::new(50.0, 0.0), Point::new(50.0, 50.0)],
            4.0,
        );
        // Only the tab on the edge is within reach of the cut.
        assert_eq!(spans.len(), 1);
        let (a, b) = spans[0];
        assert!((b - a - 10.0).abs() < 1e-9);
        assert!(((a + b) / 2.0 - 53.0).abs() < 1e-9);
    }
}
