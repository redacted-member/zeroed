//! Merging toolpaths that use the same tool into one, so the machine runs
//! them without a tool change or a trip home in between.
//!
//! Each toolpath starts at home, travels to its first cut and returns home
//! at the end. The merged toolpath keeps the first one's approach and the
//! last one's return, and joins the rest with a lift to the safe height, a
//! rapid across and a drop to where the next toolpath would have arrived
//! from its own approach.
//!
//! [`merge_ordered`] is the Merge Toolpaths form: it cuts the parts' contours
//! in the quickest of the orderings the user ticked, optionally grouping the
//! contours that lie in one part (Merge By Part), each at its own toolpath's
//! feeds and speed.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::CoreError;

use super::estimate::TimeEstimate;
use super::toolpath::{BankFiring, Move, MoveKind, Point3, RateChange, StrategyKind, Toolpath};

/// An order the Merge Toolpaths form can try for the contours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOrder {
    /// By the left edge of each contour, left to right.
    LeftToRight,
    /// By the bottom edge, bottom to top.
    BottomToTop,
    /// Row by row from the bottom, alternately left to right and back.
    Grid,
    /// Always the nearest contour next, starting from home.
    ShortestPath,
}

/// The Merge Toolpaths choices.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeOptions {
    /// Orders to try; the one with the least travel is kept. None keeps the
    /// contours in list order.
    pub orders: Vec<MergeOrder>,
    /// Contours of different toolpaths that lie in one part are cut
    /// together, in list order, before moving to the next part.
    pub by_part: bool,
}

/// Joins `parts`, in order, into one toolpath named `name`.
///
/// The result takes the first part's tool, heights and home, and runs each
/// part at that part's feeds and spindle speed (`Toolpath::rates`); its
/// sources are every part's sources, so editing any of their vectors marks
/// it out of date. It records the parts' ids and names but cannot be
/// recalculated itself.
///
/// # Errors
/// Returns [`CoreError::Validation`] with fewer than two parts, a part with
/// no moves, or parts cut with different tools.
pub fn merge(parts: &[&Toolpath], name: &str) -> Result<Toolpath, CoreError> {
    let first = match parts {
        [first, _, ..] => *first,
        _ => {
            return Err(CoreError::validation(
                "choose two or more toolpaths to merge",
            ))
        }
    };
    for part in parts {
        if part.moves.is_empty() {
            return Err(CoreError::validation(format!(
                "{} has no moves to merge",
                part.name
            )));
        }
        // A drill bank names the drills at every hole; a plain toolpath
        // cut after one would run with whatever drills were left down.
        if part.firings.is_empty() != first.firings.is_empty() {
            return Err(CoreError::validation(
                "a drill bank toolpath merges only with other drill bank toolpaths",
            ));
        }
        if part.tool.tool_type != first.tool.tool_type {
            return Err(CoreError::validation(
                "only toolpaths cut with the same tool can be merged",
            ));
        }
    }

    let mut merged = Toolpath::new(0, name, first.tool.clone(), StrategyKind::Custom);
    merged.safe_z = first.safe_z;
    merged.home = first.home;
    merged.rapid_clearance_gap = first.rapid_clearance_gap;
    merged.tool_number = first.tool_number;
    merged.spindle = first.spindle;
    merged.feed = first.feed;
    merged.plunge = first.plunge;
    merged.visible = true;

    let safe_z = parts.iter().map(|p| p.safe_z).fold(first.safe_z, f64::max);
    let count = parts.len();
    let mut starts: Vec<PartStart> = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let lead = if index == 0 { 0 } else { leading_rapids(part) };
        let trail = if index + 1 == count {
            part.moves.len()
        } else {
            part.moves.len() - trailing_rapids(part)
        };
        let join = merged.moves.len();
        if index > 0 {
            // Where this part's own approach would have left the tool.
            let arrive = if lead == 0 {
                part.home
            } else {
                part.moves[lead - 1].to
            };
            let here = merged.moves.last().map_or(merged.home, |m| m.to);
            let high = safe_z.max(here.z).max(arrive.z);
            merged
                .moves
                .push(Move::rapid(Point3::new(here.x, here.y, high)));
            merged
                .moves
                .push(Move::rapid(Point3::new(arrive.x, arrive.y, high)));
            if arrive.z < high {
                merged.moves.push(Move::rapid(arrive));
            }
        }
        starts.push(PartStart {
            join,
            kept: merged.moves.len(),
            part,
            first: lead,
            last: trail.max(lead),
        });
        merged
            .moves
            .extend_from_slice(&part.moves[lead..trail.max(lead)]);
    }

    for part in parts {
        for source in &part.sources {
            if !merged.sources.iter().any(|s| s.id == source.id) {
                merged.sources.push(*source);
            }
        }
    }
    let model = parts
        .iter()
        .find_map(|p| p.params_json.get("model").cloned());
    merged.params_json = json!({
        "merged": parts.iter().map(|p| json!({ "id": p.id, "name": p.name })).collect::<Vec<_>>(),
    });
    if let Some(model) = model {
        merged.params_json["model"] = model;
    }
    set_rates(&mut merged, &starts);
    // Each part's firings, in the order its holes now come.
    merged.firings = parts.iter().flat_map(|p| p.firings.clone()).collect();
    merged.recompute_bounds();
    TimeEstimate::default().apply(&mut merged);
    Ok(merged)
}

/// Where one part's moves sit in a merged toolpath: `join` is the index of
/// the first travel move towards it (none for the first part, whose own
/// approach is kept), `kept` the index its moves `first..last` start at.
struct PartStart<'a> {
    join: usize,
    kept: usize,
    part: &'a Toolpath,
    first: usize,
    last: usize,
}

/// Gives `merged` the rates of its parts: from the travel towards each
/// part its own rates at its first kept move, so the spindle settles on the
/// way, and each rate change the part carries inside its kept moves (a
/// part that was merged itself) at the move it now sits on. A change that
/// repeats the rates before it is left out, and the first part's rates
/// become the toolpath's own.
fn set_rates(merged: &mut Toolpath, starts: &[PartStart]) {
    let mut rates: Vec<RateChange> = Vec::new();
    let mut push = |from: usize, feed: f64, plunge: f64, spindle: f64| {
        let same = rates.last().is_some_and(|last| {
            last.feed == feed && last.plunge == plunge && last.spindle == spindle
        });
        if !same {
            rates.push(RateChange {
                from,
                feed,
                plunge,
                spindle,
            });
        }
    };
    for start in starts {
        let own = start.part.rates_at(start.first);
        push(start.join, own.feed, own.plunge, own.spindle);
        for change in &start.part.rates {
            if change.from > start.first && change.from < start.last {
                push(
                    start.kept + change.from - start.first,
                    change.feed,
                    change.plunge,
                    change.spindle,
                );
            }
        }
    }
    if let Some(first) = rates.first() {
        merged.feed = first.feed;
        merged.plunge = first.plunge;
        merged.spindle = first.spindle;
    }
    // The first entry is the toolpath's own rates: it says nothing new.
    if !rates.is_empty() {
        rates.remove(0);
    }
    merged.rates = rates;
}

/// One contour of a part: the moves from just after a travel at the safe
/// height to just before the next one.
struct Chunk<'a> {
    part: usize,
    moves: &'a [Move],
    /// Index of its first move in the part.
    start: usize,
    /// Where the travel before it arrived, in XY.
    entry: (f64, f64),
    /// XY box of its moves: min x, min y, max x, max y.
    bounds: [f64; 4],
}

/// A run of chunks cut one after the other: one chunk, or a part's chunks
/// with Merge By Part.
struct Unit {
    chunks: Vec<usize>,
    bounds: [f64; 4],
}

/// Joins `parts` as the Merge Toolpaths form asks. With no order ticked and
/// Merge By Part off it is [`merge`]. The result keeps `options` in its
/// settings, so recalculating it merges the same way.
///
/// # Errors
/// As [`merge`].
pub fn merge_ordered(
    parts: &[&Toolpath],
    name: &str,
    options: &MergeOptions,
) -> Result<Toolpath, CoreError> {
    let mut merged = merge(parts, name)?;
    if options.orders.is_empty() && !options.by_part {
        merged.params_json["options"] = json!(options);
        return Ok(merged);
    }
    let first = parts[0];
    let safe_z = parts.iter().map(|p| p.safe_z).fold(first.safe_z, f64::max);
    let chunks: Vec<Chunk> = parts
        .iter()
        .enumerate()
        .flat_map(|(index, part)| chunks_of(index, part))
        .collect();
    let units = units_of(&chunks, options.by_part);
    let home = (first.home.x, first.home.y);
    let mut best: Option<(f64, Vec<usize>)> = None;
    let mut orders = options.orders.clone();
    orders.sort();
    orders.dedup();
    if orders.is_empty() {
        // Merge By Part alone keeps the parts in the order they come.
        best = Some((0.0, (0..units.len()).collect()));
    }
    for order in orders {
        let sequence = ordered(&units, &chunks, order, home, options.by_part);
        let travel = travel_length(&sequence, &units, &chunks, home);
        if best
            .as_ref()
            .is_none_or(|(length, _)| travel < *length - 1e-9)
        {
            best = Some((travel, sequence));
        }
    }
    let sequence = best.map(|(_, s)| s).unwrap_or_default();

    let mut moves: Vec<Move> = vec![Move::rapid(first.home)];
    let mut starts: Vec<PartStart> = Vec::new();
    let mut here = first.home;
    // Drill bank firings not yet placed, part by part.
    let mut unplaced: Vec<Vec<Option<&BankFiring>>> = parts
        .iter()
        .map(|p| p.firings.iter().map(Some).collect())
        .collect();
    let mut firings: Vec<BankFiring> = Vec::new();
    for unit in &sequence {
        for &index in &units[*unit].chunks {
            let chunk = &chunks[index];
            let part = parts[chunk.part];
            let entry = Point3::new(chunk.entry.0, chunk.entry.1, part.safe_z);
            let high = if moves.len() == 1 {
                first.home.z.max(safe_z)
            } else {
                safe_z.max(here.z)
            };
            let travels = (here.x - entry.x).abs() > 1e-9 || (here.y - entry.y).abs() > 1e-9;
            let join = moves.len();
            if travels {
                if here.z < high {
                    moves.push(Move::rapid(Point3::new(here.x, here.y, high)));
                }
                moves.push(Move::rapid(Point3::new(entry.x, entry.y, high)));
                if entry.z < high {
                    moves.push(Move::rapid(entry));
                }
            }
            // A drill bank hole is entered from its firing's position: the
            // post names the drills on the rapid that arrives there.
            let firing = unplaced[chunk.part].iter_mut().find(|f| {
                f.is_some_and(|f| {
                    (f.at.x - entry.x).abs() < 1e-6 && (f.at.y - entry.y).abs() < 1e-6
                })
            });
            if let Some(firing) = firing.and_then(Option::take) {
                if !travels {
                    moves.push(Move::rapid(Point3::new(entry.x, entry.y, here.z)));
                }
                firings.push(firing.clone());
            }
            starts.push(PartStart {
                join,
                kept: moves.len(),
                part,
                first: chunk.start,
                last: chunk.start + chunk.moves.len(),
            });
            moves.extend_from_slice(chunk.moves);
            here = moves.last().map_or(here, |m| m.to);
        }
    }
    if here.z < first.home.z {
        moves.push(Move::rapid(Point3::new(here.x, here.y, first.home.z)));
    }
    moves.push(Move::rapid(first.home));
    merged.moves = moves;
    set_rates(&mut merged, &starts);
    merged.firings = firings;
    merged.params_json["options"] = json!(options);
    merged.recompute_bounds();
    TimeEstimate::default().apply(&mut merged);
    Ok(merged)
}

/// The options a merged toolpath was made with; none for older ones.
pub fn options_of(toolpath: &Toolpath) -> MergeOptions {
    toolpath
        .params_json
        .get("options")
        .and_then(|o| serde_json::from_value(o.clone()).ok())
        .unwrap_or_default()
}

/// A part's contours, split at every rapid that travels in XY at or above
/// its safe height. Stretches that cut nothing (the trip from and back to
/// home) are left out.
fn chunks_of(index: usize, part: &Toolpath) -> Vec<Chunk<'_>> {
    let clear = part.safe_z - 1e-6;
    let mut out = Vec::new();
    let mut start = 0;
    let mut entry = (part.home.x, part.home.y);
    let mut from = part.home;
    for (i, m) in part.moves.iter().enumerate() {
        let travels = m.kind == MoveKind::Rapid
            && ((m.to.x - from.x).abs() > 1e-9 || (m.to.y - from.y).abs() > 1e-9)
            && m.to.z >= clear
            && from.z >= clear;
        if travels {
            out.extend(chunk(index, &part.moves[start..i], start, entry));
            start = i + 1;
            entry = (m.to.x, m.to.y);
        }
        from = m.to;
    }
    out.extend(chunk(index, &part.moves[start..], start, entry));
    out
}

/// A chunk of `moves`, from move `start` of the part, or none when they
/// cut nothing.
fn chunk(part: usize, moves: &[Move], start: usize, entry: (f64, f64)) -> Option<Chunk<'_>> {
    if !moves.iter().any(|m| m.kind.is_cutting()) {
        return None;
    }
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for m in moves {
        bounds[0] = bounds[0].min(m.to.x);
        bounds[1] = bounds[1].min(m.to.y);
        bounds[2] = bounds[2].max(m.to.x);
        bounds[3] = bounds[3].max(m.to.y);
    }
    Some(Chunk {
        part,
        moves,
        start,
        entry,
        bounds,
    })
}

/// The units the orders arrange: each chunk on its own, in list order
/// within each part toolpath; or, with Merge By Part, groups of chunks whose
/// boxes overlap, each group cut in list order then contour order.
fn units_of(chunks: &[Chunk], by_part: bool) -> Vec<Unit> {
    if !by_part {
        // Toolpath by toolpath: the orders rearrange a toolpath's contours
        // but the toolpaths stay in list order (see `ordered`).
        return chunks
            .iter()
            .enumerate()
            .map(|(i, c)| Unit {
                chunks: vec![i],
                bounds: c.bounds,
            })
            .collect();
    }
    let mut group: Vec<usize> = (0..chunks.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    for a in 0..chunks.len() {
        for b in a + 1..chunks.len() {
            if overlaps(&chunks[a].bounds, &chunks[b].bounds) {
                let (ra, rb) = (root(&mut group, a), root(&mut group, b));
                if ra != rb {
                    group[ra.max(rb)] = ra.min(rb);
                }
            }
        }
    }
    let mut units: Vec<Unit> = Vec::new();
    let mut unit_of: Vec<Option<usize>> = vec![None; chunks.len()];
    for (i, chunk) in chunks.iter().enumerate() {
        let r = root(&mut group, i);
        let at = match unit_of[r] {
            Some(at) => at,
            None => {
                units.push(Unit {
                    chunks: Vec::new(),
                    bounds: [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
                });
                unit_of[r] = Some(units.len() - 1);
                units.len() - 1
            }
        };
        let unit = &mut units[at];
        // Chunks come part by part and contour by contour, so pushing them
        // in turn keeps list order within the unit.
        unit.chunks.push(i);
        let b = &chunk.bounds;
        unit.bounds = [
            unit.bounds[0].min(b[0]),
            unit.bounds[1].min(b[1]),
            unit.bounds[2].max(b[2]),
            unit.bounds[3].max(b[3]),
        ];
    }
    units
}

fn overlaps(a: &[f64; 4], b: &[f64; 4]) -> bool {
    const TOUCH: f64 = 1e-6;
    a[0] <= b[2] + TOUCH && b[0] <= a[2] + TOUCH && a[1] <= b[3] + TOUCH && b[1] <= a[3] + TOUCH
}

/// The units in `order`. Without Merge By Part a unit is one chunk and the
/// toolpaths stay in list order: the order applies within each toolpath.
fn ordered(
    units: &[Unit],
    chunks: &[Chunk],
    order: MergeOrder,
    home: (f64, f64),
    by_part: bool,
) -> Vec<usize> {
    // One batch per toolpath without Merge By Part, one batch of every part
    // with it.
    let mut batches: Vec<Vec<usize>> = Vec::new();
    if !by_part {
        for (i, unit) in units.iter().enumerate() {
            let part = chunks[unit.chunks[0]].part;
            match batches.last_mut() {
                Some(batch) if chunks[units[batch[0]].chunks[0]].part == part => batch.push(i),
                _ => batches.push(vec![i]),
            }
        }
    } else {
        batches.push((0..units.len()).collect());
    }
    let mut out = Vec::with_capacity(units.len());
    let mut at = home;
    for mut batch in batches {
        arrange(&mut batch, units, chunks, order, at);
        if let Some(last) = batch.last().and_then(|u| units[*u].chunks.last()) {
            let end = chunks[*last].moves.last().map_or(at, |m| (m.to.x, m.to.y));
            at = end;
        }
        out.extend(batch);
    }
    out
}

fn arrange(
    batch: &mut [usize],
    units: &[Unit],
    chunks: &[Chunk],
    order: MergeOrder,
    from: (f64, f64),
) {
    let key = |u: usize, i: usize| units[u].bounds[i];
    match order {
        MergeOrder::LeftToRight => batch.sort_by(|a, b| {
            key(*a, 0)
                .total_cmp(&key(*b, 0))
                .then(key(*a, 1).total_cmp(&key(*b, 1)))
                .then(a.cmp(b))
        }),
        MergeOrder::BottomToTop => batch.sort_by(|a, b| {
            key(*a, 1)
                .total_cmp(&key(*b, 1))
                .then(key(*a, 0).total_cmp(&key(*b, 0)))
                .then(a.cmp(b))
        }),
        MergeOrder::Grid => {
            // Rows as tall as the tallest unit, from the bottom; every other
            // row runs right to left.
            let height = batch
                .iter()
                .map(|u| key(*u, 3) - key(*u, 1))
                .fold(0.0_f64, f64::max)
                .max(1e-3);
            let bottom = batch.iter().map(|u| key(*u, 1)).fold(f64::MAX, f64::min);
            let row = |u: usize| ((key(u, 1) - bottom) / height).floor() as i64;
            batch.sort_by(|a, b| {
                let (ra, rb) = (row(*a), row(*b));
                let across = if ra % 2 == 0 {
                    key(*a, 0).total_cmp(&key(*b, 0))
                } else {
                    key(*b, 0).total_cmp(&key(*a, 0))
                };
                ra.cmp(&rb).then(across).then(a.cmp(b))
            });
        }
        MergeOrder::ShortestPath => {
            let mut at = from;
            for i in 0..batch.len() {
                let mut pick = i;
                let mut nearest = f64::MAX;
                for (j, &u) in batch.iter().enumerate().skip(i) {
                    let entry = chunks[units[u].chunks[0]].entry;
                    let d = (entry.0 - at.0).hypot(entry.1 - at.1);
                    if d < nearest - 1e-9 {
                        nearest = d;
                        pick = j;
                    }
                }
                batch.swap(i, pick);
                let last = *units[batch[i]].chunks.last().unwrap_or(&0);
                at = chunks[last].moves.last().map_or(at, |m| (m.to.x, m.to.y));
            }
        }
    }
}

/// Length of the XY travel from home through `sequence` and back.
fn travel_length(sequence: &[usize], units: &[Unit], chunks: &[Chunk], home: (f64, f64)) -> f64 {
    let mut at = home;
    let mut length = 0.0;
    for unit in sequence {
        for index in &units[*unit].chunks {
            let chunk = &chunks[*index];
            length += (chunk.entry.0 - at.0).hypot(chunk.entry.1 - at.1);
            at = chunk.moves.last().map_or(at, |m| (m.to.x, m.to.y));
        }
    }
    length + (home.0 - at.0).hypot(home.1 - at.1)
}

/// How many rapids the toolpath opens with, its approach from home.
fn leading_rapids(toolpath: &Toolpath) -> usize {
    toolpath
        .moves
        .iter()
        .take_while(|m| m.kind == MoveKind::Rapid)
        .count()
}

/// How many rapids the toolpath ends with, its lift and trip home.
fn trailing_rapids(toolpath: &Toolpath) -> usize {
    toolpath
        .moves
        .iter()
        .rev()
        .take_while(|m| m.kind == MoveKind::Rapid)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::SourceRef;

    fn part(id: u64, x: f64, tool: &Tool) -> Toolpath {
        let mut t = Toolpath::new(id, format!("P{id}"), tool.clone(), StrategyKind::Profile);
        t.home = Point3::new(0.0, 0.0, 20.0);
        t.safe_z = 5.0;
        t.feed = 1000.0;
        t.plunge = 300.0;
        t.moves = vec![
            Move::rapid(Point3::new(0.0, 0.0, 20.0)),
            Move::rapid(Point3::new(x, 0.0, 20.0)),
            Move::rapid(Point3::new(x, 0.0, 2.0)),
            Move::plunge(Point3::new(x, 0.0, -3.0)),
            Move::feed(Point3::new(x + 10.0, 0.0, -3.0)),
            Move::rapid(Point3::new(x + 10.0, 0.0, 5.0)),
            Move::rapid(Point3::new(x + 10.0, 0.0, 20.0)),
            Move::rapid(Point3::new(0.0, 0.0, 20.0)),
        ];
        t.sources = vec![SourceRef {
            id: id * 10,
            revision: 1,
        }];
        t.recompute_bounds();
        t
    }

    fn tool(diameter: f64) -> Tool {
        Tool {
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
            ..Tool::default()
        }
    }

    #[test]
    fn parts_are_joined_without_going_home_in_between() {
        let t = tool(6.0);
        let (a, b) = (part(1, 10.0, &t), part(2, 50.0, &t));
        let merged = merge(&[&a, &b], "Merged").expect("merges");
        let home_visits = merged
            .moves
            .iter()
            .filter(|m| m.to == Point3::new(0.0, 0.0, 20.0))
            .count();
        assert_eq!(home_visits, 2, "leaves home once and comes back once");
        let cuts: Vec<Point3> = merged
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| m.to)
            .collect();
        assert_eq!(cuts.len(), 4);
        assert_eq!(cuts[2], Point3::new(50.0, 0.0, -3.0));
        // Before the second plunge the tool arrives where b's approach left it.
        let plunge = merged
            .moves
            .iter()
            .rposition(|m| m.kind == MoveKind::Plunge)
            .expect("plunge");
        assert_eq!(merged.moves[plunge - 1].to, Point3::new(50.0, 0.0, 2.0));
        // Nothing travels across below the safe height.
        let mut from = merged.home;
        for m in &merged.moves {
            if m.kind == MoveKind::Rapid && (m.to.x != from.x || m.to.y != from.y) {
                assert!(
                    m.to.z >= 5.0 && from.z >= 2.0 - 1e-9,
                    "{from:?} -> {:?}",
                    m.to
                );
            }
            from = m.to;
        }
        assert_eq!(merged.sources.len(), 2);
        assert_eq!(merged.params_json["merged"][1]["name"], "P2");
        assert!(merged.estimated_seconds > 0.0);
        assert!(merged.bounds.is_some());
    }

    #[test]
    fn different_tools_or_too_few_parts_are_refused() {
        let a = part(1, 10.0, &tool(6.0));
        let b = part(2, 50.0, &tool(3.0));
        assert!(merge(&[&a, &b], "M").is_err());
        assert!(merge(&[&a], "M").is_err());
        let mut empty = part(3, 0.0, &tool(6.0));
        empty.moves.clear();
        assert!(merge(&[&a, &empty], "M").is_err());
    }

    /// A toolpath cutting a short line at each of `xs`, travelling at its
    /// safe height between them.
    fn contours(id: u64, xs: &[f64], y: f64, tool: &Tool) -> Toolpath {
        let mut t = Toolpath::new(id, format!("C{id}"), tool.clone(), StrategyKind::Pocket);
        t.home = Point3::new(0.0, 0.0, 20.0);
        t.safe_z = 5.0;
        t.feed = 1000.0 * id as f64;
        t.plunge = 300.0;
        t.spindle = 10000.0 * id as f64;
        t.moves = vec![Move::rapid(Point3::new(0.0, 0.0, 20.0))];
        for (i, x) in xs.iter().enumerate() {
            let z = if i == 0 { 20.0 } else { 5.0 };
            t.moves.push(Move::rapid(Point3::new(*x, y, z)));
            t.moves.push(Move::rapid(Point3::new(*x, y, 2.0)));
            t.moves.push(Move::plunge(Point3::new(*x, y, -1.0)));
            t.moves.push(Move::feed(Point3::new(x + 5.0, y, -1.0)));
            t.moves.push(Move::rapid(Point3::new(x + 5.0, y, 5.0)));
        }
        t.moves
            .push(Move::rapid(Point3::new(xs[xs.len() - 1] + 5.0, y, 20.0)));
        t.moves.push(Move::rapid(Point3::new(0.0, 0.0, 20.0)));
        t.recompute_bounds();
        t
    }

    fn cut_starts(t: &Toolpath) -> Vec<(f64, f64)> {
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| (m.to.x, m.to.y))
            .collect()
    }

    #[test]
    fn each_part_keeps_its_own_feeds_and_speed() {
        let t = tool(6.0);
        let (a, b) = (contours(1, &[10.0], 0.0, &t), contours(2, &[50.0], 0.0, &t));
        let merged = merge(&[&a, &b], "M").expect("merges");
        // The first part's rates are the toolpath's own; the change to the
        // second part's sits on the travel towards it, so the spindle
        // settles on the way.
        assert_eq!(merged.rates.len(), 1);
        assert_eq!(merged.feed, 1000.0);
        assert_eq!(merged.spindle, 10000.0);
        let change = merged.rates[0].from;
        assert_eq!(merged.moves[change].kind, MoveKind::Rapid);
        assert!(
            merged.moves[change].to.z >= 5.0,
            "{:?}",
            merged.moves[change]
        );
        assert_eq!(merged.rates_at(change).spindle, 20000.0);
        let cut = |x: f64| {
            merged
                .moves
                .iter()
                .position(|m| m.kind == MoveKind::Feed && m.to.x == x)
                .expect("cut")
        };
        assert_eq!(merged.rates_at(cut(15.0)).feed, 1000.0);
        assert_eq!(merged.rates_at(cut(55.0)).feed, 2000.0);
        assert_eq!(merged.rates_at(cut(55.0)).spindle, 20000.0);
        // The estimate follows each part's feed: the second part's 5 mm cut
        // at 2000 mm per minute is quicker than at the first part's rate.
        let mut slow = merged.clone();
        slow.rates.clear();
        TimeEstimate::default().apply(&mut slow);
        assert!(merged.estimated_seconds < slow.estimated_seconds);
    }

    #[test]
    fn a_merged_part_keeps_its_own_rate_changes_when_merged_again() {
        let t = tool(6.0);
        let (a, b) = (contours(1, &[10.0], 0.0, &t), contours(2, &[50.0], 0.0, &t));
        let c = contours(3, &[90.0], 0.0, &t);
        let inner = merge(&[&a, &b], "AB").expect("merges");
        let cut = |merged: &Toolpath, x: f64| {
            merged
                .moves
                .iter()
                .position(|m| m.kind == MoveKind::Feed && m.to.x == x)
                .expect("cut")
        };
        for merged in [
            merge(&[&inner, &c], "ABC").expect("merges"),
            merge(&[&c, &inner], "CAB").expect("merges"),
            merge_ordered(
                &[&inner, &c],
                "ABC ordered",
                &MergeOptions {
                    orders: vec![MergeOrder::LeftToRight],
                    by_part: false,
                },
            )
            .expect("merges"),
        ] {
            assert_eq!(
                merged.rates_at(cut(&merged, 15.0)).feed,
                1000.0,
                "{}",
                merged.name
            );
            assert_eq!(
                merged.rates_at(cut(&merged, 55.0)).feed,
                2000.0,
                "{}",
                merged.name
            );
            assert_eq!(merged.rates_at(cut(&merged, 55.0)).spindle, 20000.0);
            assert_eq!(
                merged.rates_at(cut(&merged, 95.0)).feed,
                3000.0,
                "{}",
                merged.name
            );
            assert_eq!(merged.rates_at(cut(&merged, 95.0)).spindle, 30000.0);
        }
    }

    #[test]
    fn the_quickest_ticked_order_is_kept() {
        let t = tool(6.0);
        // Drawn right to left: left to right order saves travel.
        let a = contours(1, &[90.0, 50.0, 10.0], 0.0, &t);
        let b = contours(1, &[200.0], 0.0, &t);
        let options = MergeOptions {
            orders: vec![MergeOrder::LeftToRight],
            by_part: false,
        };
        let merged = merge_ordered(&[&a, &b], "M", &options).expect("merges");
        assert_eq!(
            cut_starts(&merged),
            vec![(10.0, 0.0), (50.0, 0.0), (90.0, 0.0), (200.0, 0.0)]
        );
        // Without Merge By Part the toolpaths stay in list order.
        let back = merge_ordered(&[&b, &a], "M", &options).expect("merges");
        assert_eq!(cut_starts(&back)[0], (200.0, 0.0));
        // Bottom to top ties on Y and falls back to X; shortest path from
        // home also runs left to right here: either way the travel is the
        // same, so the first ticked (in order) is kept.
        let every = MergeOptions {
            orders: vec![
                MergeOrder::ShortestPath,
                MergeOrder::Grid,
                MergeOrder::BottomToTop,
                MergeOrder::LeftToRight,
            ],
            by_part: false,
        };
        let merged = merge_ordered(&[&a, &b], "M", &every).expect("merges");
        assert_eq!(cut_starts(&merged)[0], (10.0, 0.0));
        // Travel never crosses below the safe height, and it starts and
        // ends at home.
        let mut from = merged.home;
        for m in &merged.moves {
            if m.kind == MoveKind::Rapid && (m.to.x != from.x || m.to.y != from.y) {
                assert!(
                    m.to.z >= 5.0 && from.z >= 5.0 - 1e-9,
                    "{from:?} -> {:?}",
                    m.to
                );
            }
            from = m.to;
        }
        assert_eq!(merged.moves.last().map(|m| m.to), Some(merged.home));
        assert_eq!(options_of(&merged), every);
    }

    #[test]
    fn merge_by_part_cuts_each_part_before_the_next() {
        let t = tool(6.0);
        // Pocket passes in two parts, then a profile of each part.
        let pocket = contours(1, &[10.0, 100.0], 0.0, &t);
        let mut profile = contours(2, &[8.0, 98.0], 0.0, &t);
        // The profiles reach a little wider than the pockets.
        for m in &mut profile.moves {
            if m.kind == MoveKind::Feed {
                m.to.x += 4.0;
            }
        }
        let by_part = MergeOptions {
            orders: vec![MergeOrder::LeftToRight],
            by_part: true,
        };
        let merged = merge_ordered(&[&pocket, &profile], "M", &by_part).expect("merges");
        assert_eq!(
            cut_starts(&merged),
            vec![(10.0, 0.0), (8.0, 0.0), (100.0, 0.0), (98.0, 0.0)]
        );
        // Each run of cuts keeps its own toolpath's feed.
        let feeds: Vec<f64> = merged
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind == MoveKind::Feed)
            .map(|(i, _)| merged.rates_at(i).feed)
            .collect();
        assert_eq!(feeds, vec![1000.0, 2000.0, 1000.0, 2000.0]);
        // Off, every pocket pass comes before the profiles.
        let apart = MergeOptions {
            orders: vec![MergeOrder::LeftToRight],
            by_part: false,
        };
        let merged = merge_ordered(&[&pocket, &profile], "M", &apart).expect("merges");
        assert_eq!(
            cut_starts(&merged),
            vec![(10.0, 0.0), (100.0, 0.0), (8.0, 0.0), (98.0, 0.0)]
        );
    }

    /// A drill bank toolpath firing once at each of `xs`.
    fn bank(id: u64, xs: &[f64], tool: &Tool) -> Toolpath {
        let mut t = contours(id, xs, 0.0, tool);
        t.firings = xs
            .iter()
            .map(|x| BankFiring {
                at: crate::geom::Point::new(*x, 0.0),
                drills: vec![id as u32, 10 + *x as u32],
                along_x: Vec::new(),
                along_y: Vec::new(),
            })
            .collect();
        t
    }

    /// The firings in the order the post meets them: on each rapid that
    /// arrives over the next firing's position, never on one straight up or
    /// down.
    fn met(t: &Toolpath) -> Vec<Vec<u32>> {
        let mut out = Vec::new();
        let mut next = 0;
        let mut from = t.home;
        for m in &t.moves {
            let plumb = m.to.x == from.x && m.to.y == from.y && m.to.z != from.z;
            if m.kind == MoveKind::Rapid && !plumb {
                if let Some(f) = t.firings.get(next) {
                    if f.at.x == m.to.x && f.at.y == m.to.y {
                        out.push(f.drills.clone());
                        next += 1;
                    }
                }
            }
            from = m.to;
        }
        out
    }

    #[test]
    fn drill_bank_toolpaths_merge_with_their_firings_in_cutting_order() {
        let drill = tool(5.0);
        let a = bank(1, &[10.0, 30.0], &drill);
        let b = bank(2, &[20.0, 40.0], &drill);
        let merged = merge(&[&a, &b], "M").expect("merges");
        assert_eq!(
            met(&merged),
            vec![vec![1, 20], vec![1, 40], vec![2, 30], vec![2, 50]]
        );
        let options = MergeOptions {
            orders: vec![MergeOrder::LeftToRight],
            by_part: true,
        };
        let ordered = merge_ordered(&[&a, &b], "M", &options).expect("merges");
        assert_eq!(
            cut_starts(&ordered),
            vec![(10.0, 0.0), (20.0, 0.0), (30.0, 0.0), (40.0, 0.0)]
        );
        assert_eq!(
            met(&ordered),
            vec![vec![1, 20], vec![2, 30], vec![1, 40], vec![2, 50]]
        );
        // Two banks firing at the same place each name their own drills.
        let c = bank(3, &[10.0], &drill);
        let same = merge_ordered(&[&a, &c], "M", &options).expect("merges");
        assert_eq!(same.firings.len(), 3);
        assert_eq!(met(&same).len(), 3, "{:?}", same.moves);
        // A plain toolpath would run with the bank's drills left down.
        let plain = contours(4, &[60.0], 0.0, &drill);
        assert!(merge(&[&a, &plain], "M").is_err());
        assert!(merge(&[&plain, &a], "M").is_err());
    }
}
