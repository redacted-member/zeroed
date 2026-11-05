//! Node and segment editing: the operations behind the node edit tool, the
//! join and close commands, trim, extend, fillet and chamfer.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use super::intersect::path_intersections;
use super::metrics::{
    nearest_on_segment, path_bbox, segment_length, segment_point_at, segment_tangent_at,
};
use super::path::{
    bulge_from_sweep, sweep_from_bulge, ArcGeom, Path, Segment, SegmentKind, SubPath,
};
use super::point::{Point, Tolerance, Vec2};
use super::transform::arc_to_cubic_segments;

/// How an open subpath is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseStyle {
    /// Join the ends with a straight line.
    Straight,
    /// Join the ends with a cubic that keeps both end tangents.
    Smooth,
    /// The same, with the symmetrical shallower curve.
    SmoothShallow,
}

/// Which relief shape a fillet cuts at a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilletKind {
    /// A plain tangent arc, the corner is rounded.
    Normal,
    /// A relief circle through the sharp corner, its centre on the bisector
    /// one radius in, so a round tool of the same radius reaches the corner.
    #[serde(alias = "dog_bone")]
    DogBone,
    /// A relief half circle along one wall ending at the corner, so a round
    /// tool of the same radius reaches the corner from that wall.
    #[serde(alias = "t_bone")]
    TBone,
    /// An external loop for plasma tools and drag knives: the path runs on
    /// past the corner, turns round a circle of the radius outside it and
    /// comes back onto the next wall, so the cut still reaches the corner.
    Plasma,
}

/// Which wall a T-Bone relief runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TBoneSide {
    /// Along the segment coming into the corner.
    Incoming,
    /// Along the segment going out of the corner.
    Outgoing,
}

/// Reverses a chain of segments, returning the new start and segments.
pub fn reverse_chain(start: Point, segments: &[Segment]) -> (Point, Vec<Segment>) {
    if segments.is_empty() {
        return (start, Vec::new());
    }
    let mut starts = Vec::with_capacity(segments.len());
    let mut cursor = start;
    for seg in segments {
        starts.push(cursor);
        cursor = seg.end();
    }
    let new_start = cursor;
    let mut out = Vec::with_capacity(segments.len());
    for (seg, from) in segments.iter().zip(starts.iter()).rev() {
        out.push(match *seg {
            Segment::Line { .. } => Segment::Line { to: *from },
            Segment::Arc { bulge, .. } => Segment::Arc {
                to: *from,
                bulge: -bulge,
            },
            Segment::Cubic { c1, c2, .. } => Segment::Cubic {
                c1: c2,
                c2: c1,
                to: *from,
            },
        });
    }
    (new_start, out)
}

/// The same subpath travelled the other way round.
pub fn reverse_subpath(subpath: &SubPath) -> SubPath {
    let (start, segments) = reverse_chain(subpath.start, &subpath.segments);
    SubPath::new(start, segments, subpath.closed)
}

/// The same path with every subpath travelled the other way round.
pub fn reverse(path: &Path) -> Path {
    Path::from_subpaths(path.subpaths.iter().map(reverse_subpath).collect())
}

/// Moves the start of a closed subpath to the node nearest `near`.
///
/// Returns an error when the subpath is open or has no segments.
pub fn set_start_node(
    subpath: &SubPath,
    near: Point,
    tol: Tolerance,
) -> Result<SubPath, CoreError> {
    let _ = tol;
    if !subpath.closed {
        return Err(CoreError::geometry(
            "only a closed vector can have its start point moved",
        ));
    }
    if subpath.segments.is_empty() {
        return Err(CoreError::geometry("the vector has no segments"));
    }
    let nodes = subpath.nodes();
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, node) in nodes.iter().enumerate() {
        let d = node.distance_squared(near);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    if best == 0 {
        return Ok(subpath.clone());
    }
    let mut segments = Vec::with_capacity(subpath.segments.len());
    segments.extend_from_slice(&subpath.segments[best..]);
    segments.extend_from_slice(&subpath.segments[..best]);
    Ok(SubPath::new(nodes[best], segments, true))
}

/// Splits one segment at parameter `t`, keeping its shape.
pub fn split_segment(
    start: Point,
    segment: &Segment,
    t: f64,
) -> Result<(Segment, Segment), CoreError> {
    if !(0.0..=1.0).contains(&t) || !t.is_finite() {
        return Err(CoreError::geometry(format!(
            "segment parameter must be between 0 and 1, got {t}"
        )));
    }
    Ok(match *segment {
        Segment::Line { to } => {
            let mid = start.lerp(to, t);
            (Segment::Line { to: mid }, Segment::Line { to })
        }
        Segment::Arc { to, bulge } => match ArcGeom::from_bulge(start, to, bulge) {
            Some(arc) => {
                let mid = arc.point_at(t);
                let sweep = sweep_from_bulge(bulge);
                (
                    Segment::Arc {
                        to: mid,
                        bulge: bulge_from_sweep(sweep * t),
                    },
                    Segment::Arc {
                        to,
                        bulge: bulge_from_sweep(sweep * (1.0 - t)),
                    },
                )
            }
            None => {
                let mid = start.lerp(to, t);
                (Segment::Line { to: mid }, Segment::Line { to })
            }
        },
        Segment::Cubic { c1, c2, to } => {
            let p01 = start.lerp(c1, t);
            let p12 = c1.lerp(c2, t);
            let p23 = c2.lerp(to, t);
            let p012 = p01.lerp(p12, t);
            let p123 = p12.lerp(p23, t);
            let mid = p012.lerp(p123, t);
            (
                Segment::Cubic {
                    c1: p01,
                    c2: p012,
                    to: mid,
                },
                Segment::Cubic {
                    c1: p123,
                    c2: p23,
                    to,
                },
            )
        }
    })
}

/// Adds a node part way along a segment, keeping the shape of the curve.
pub fn insert_node(subpath: &SubPath, segment_index: usize, t: f64) -> Result<SubPath, CoreError> {
    let start = subpath
        .segment_start(segment_index)
        .ok_or_else(|| CoreError::geometry(format!("no segment {segment_index}")))?;
    let seg = subpath
        .segments
        .get(segment_index)
        .ok_or_else(|| CoreError::geometry(format!("no segment {segment_index}")))?;
    let (first, second) = split_segment(start, seg, t)?;
    let mut out = subpath.clone();
    out.segments[segment_index] = first;
    out.segments.insert(segment_index + 1, second);
    Ok(out)
}

/// Removes a node, joining the segments on either side of it.
///
/// Two cubics merge into one cubic that keeps their outer control points.
/// Any other pair merges into a straight line.
pub fn delete_node(subpath: &SubPath, node_index: usize) -> Result<SubPath, CoreError> {
    let n = subpath.segments.len();
    if n == 0 {
        return Err(CoreError::geometry("the vector has no segments"));
    }
    let nodes = subpath.node_count();
    if node_index >= nodes {
        return Err(CoreError::geometry(format!("no node {node_index}")));
    }
    if subpath.closed {
        if nodes <= 3 {
            return Err(CoreError::geometry(
                "a closed vector needs at least three nodes",
            ));
        }
        let in_idx = (node_index + n - 1) % n;
        let out_idx = node_index;
        let merged = merge_pair(
            &subpath.segments[in_idx],
            &subpath.segments[out_idx],
            subpath.segments[out_idx].end(),
        );
        let mut segments = Vec::with_capacity(n - 1);
        for (i, seg) in subpath.segments.iter().enumerate() {
            if i == in_idx {
                segments.push(merged);
            } else if i == out_idx {
                continue;
            } else {
                segments.push(*seg);
            }
        }
        let start = if node_index == 0 {
            subpath.segments[out_idx].end()
        } else {
            subpath.start
        };
        if node_index == 0 {
            // The merged segment now ends on the new start point.
            let (head, tail) = segments.split_at(in_idx);
            let mut rotated: Vec<Segment> = tail.to_vec();
            rotated.extend_from_slice(head);
            let mut fixed = rotated;
            if let Some(last) = fixed.last_mut() {
                last.set_end(start);
            }
            return Ok(SubPath::new(start, fixed, true));
        }
        return Ok(SubPath::new(start, segments, true));
    }

    if nodes <= 2 {
        return Err(CoreError::geometry(
            "an open vector needs at least two nodes",
        ));
    }
    if node_index == 0 {
        let mut segments = subpath.segments.clone();
        let first = segments.remove(0);
        return Ok(SubPath::new(first.end(), segments, false));
    }
    if node_index == nodes - 1 {
        let mut segments = subpath.segments.clone();
        segments.pop();
        return Ok(SubPath::new(subpath.start, segments, false));
    }
    let in_idx = node_index - 1;
    let out_idx = node_index;
    let merged = merge_pair(
        &subpath.segments[in_idx],
        &subpath.segments[out_idx],
        subpath.segments[out_idx].end(),
    );
    let mut segments = subpath.segments.clone();
    segments[in_idx] = merged;
    segments.remove(out_idx);
    Ok(SubPath::new(subpath.start, segments, false))
}

fn merge_pair(first: &Segment, second: &Segment, to: Point) -> Segment {
    match (first, second) {
        (Segment::Cubic { c1, .. }, Segment::Cubic { c2, .. }) => Segment::Cubic {
            c1: *c1,
            c2: *c2,
            to,
        },
        _ => Segment::Line { to },
    }
}

/// Changes the shape of one segment while keeping both of its end points.
///
/// Converting an arc of more than a quarter turn to cubics produces more than
/// one segment, so the returned subpath can be longer than the input.
pub fn convert_segment(
    subpath: &SubPath,
    segment_index: usize,
    kind: SegmentKind,
) -> Result<SubPath, CoreError> {
    let start = subpath
        .segment_start(segment_index)
        .ok_or_else(|| CoreError::geometry(format!("no segment {segment_index}")))?;
    let seg = *subpath
        .segments
        .get(segment_index)
        .ok_or_else(|| CoreError::geometry(format!("no segment {segment_index}")))?;
    let end = seg.end();
    let replacement: Vec<Segment> = match kind {
        SegmentKind::Line => vec![Segment::Line { to: end }],
        SegmentKind::Arc => {
            let mid = segment_point_at(start, &seg, 0.5);
            vec![Segment::Arc {
                to: end,
                bulge: bulge_through(start, mid, end),
            }]
        }
        SegmentKind::Cubic => match seg {
            Segment::Cubic { .. } => vec![seg],
            Segment::Line { to } => vec![Segment::Cubic {
                c1: start.lerp(to, 1.0 / 3.0),
                c2: start.lerp(to, 2.0 / 3.0),
                to,
            }],
            Segment::Arc { to, bulge } => arc_to_cubic_segments(start, to, bulge),
        },
    };
    let mut segments = Vec::with_capacity(subpath.segments.len() + replacement.len());
    segments.extend_from_slice(&subpath.segments[..segment_index]);
    segments.extend(replacement);
    segments.extend_from_slice(&subpath.segments[segment_index + 1..]);
    Ok(SubPath::new(subpath.start, segments, subpath.closed))
}

/// Bulge of the arc from `start` to `end` that passes through `via`.
fn bulge_through(start: Point, via: Point, end: Point) -> f64 {
    let cross = start.to(via).cross(start.to(end));
    let chord = start.distance(end);
    if chord < 1e-12 {
        return 0.0;
    }
    let scale = chord.max(1.0);
    if cross.abs() <= 1e-12 * scale * scale {
        return 0.0;
    }
    let sagitta = cross / chord;
    let half = chord * 0.5;
    let radius = (half * half + sagitta * sagitta) / (2.0 * sagitta.abs());
    let sweep = 2.0 * (half / radius).clamp(-1.0, 1.0).asin();
    let sweep = if sagitta.abs() > radius {
        std::f64::consts::TAU - sweep
    } else {
        sweep
    };
    bulge_from_sweep(if cross > 0.0 { -sweep } else { sweep })
}

/// Makes the curve pass smoothly through a node by lining up the control
/// points on either side of it with the chord between its neighbours.
///
/// Both neighbouring segments become cubics.
pub fn smooth_node(subpath: &SubPath, node_index: usize) -> Result<SubPath, CoreError> {
    let n = subpath.segments.len();
    if n < 2 {
        return Err(CoreError::geometry(
            "smoothing a node needs a segment on each side of it",
        ));
    }
    let nodes = subpath.node_count();
    if node_index >= nodes {
        return Err(CoreError::geometry(format!("no node {node_index}")));
    }
    let (in_idx, out_idx) = adjacent_segments(subpath, node_index)?;

    let mut work = convert_segment(subpath, out_idx.max(in_idx), SegmentKind::Cubic)?;
    let shift = work.segments.len() - subpath.segments.len();
    let lower = out_idx.min(in_idx);
    work = convert_segment(&work, lower, SegmentKind::Cubic)?;
    let extra = work.segments.len() - subpath.segments.len() - shift;

    let (in_idx, out_idx) = if in_idx < out_idx {
        (in_idx, out_idx + extra)
    } else {
        (in_idx + extra + shift, out_idx)
    };

    let corner = work
        .segments
        .get(in_idx)
        .map(Segment::end)
        .ok_or_else(|| CoreError::geometry("the node has no incoming segment"))?;
    let prev = work
        .segment_start(in_idx)
        .ok_or_else(|| CoreError::geometry("the node has no incoming segment"))?;
    let next = work
        .segments
        .get(out_idx)
        .map(Segment::end)
        .ok_or_else(|| CoreError::geometry("the node has no outgoing segment"))?;

    let dir = prev
        .to(next)
        .normalised()
        .unwrap_or_else(|| Vec2::new(1.0, 0.0));

    if let Some(Segment::Cubic { c2, .. }) = work.segments.get_mut(in_idx) {
        let d = c2.distance(corner);
        *c2 = corner.offset(dir.scaled(-d));
    }
    if let Some(Segment::Cubic { c1, .. }) = work.segments.get_mut(out_idx) {
        let d = c1.distance(corner);
        *c1 = corner.offset(dir.scaled(d));
    }
    Ok(work)
}

/// The segment arriving at a node and the segment leaving it.
fn adjacent_segments(subpath: &SubPath, node_index: usize) -> Result<(usize, usize), CoreError> {
    let n = subpath.segments.len();
    if n == 0 {
        return Err(CoreError::geometry(
            "the vector has no segments, so it has no corner",
        ));
    }
    if subpath.closed {
        Ok(((node_index + n - 1) % n, node_index % n))
    } else if node_index == 0 || node_index >= n {
        Err(CoreError::geometry(
            "an end node of an open vector has no corner",
        ))
    } else {
        Ok((node_index - 1, node_index))
    }
}

/// Cuts a subpath at parameter `t` of one segment.
///
/// An open subpath becomes two pieces. A closed subpath becomes one open
/// piece that starts and ends at the cut.
pub fn split_at(
    subpath: &SubPath,
    segment_index: usize,
    t: f64,
) -> Result<Vec<SubPath>, CoreError> {
    let expanded = insert_node(subpath, segment_index, t)?;
    let cut = segment_index + 1;
    if subpath.closed {
        let mut segments = Vec::with_capacity(expanded.segments.len());
        segments.extend_from_slice(&expanded.segments[cut..]);
        segments.extend_from_slice(&expanded.segments[..cut]);
        let start = expanded.segments[segment_index].end();
        return Ok(vec![SubPath::new(start, segments, false)]);
    }
    let first = SubPath::new(expanded.start, expanded.segments[..cut].to_vec(), false);
    let second = SubPath::new(
        expanded.segments[segment_index].end(),
        expanded.segments[cut..].to_vec(),
        false,
    );
    Ok(vec![first, second])
}

/// Joins open subpaths whose ends are within `tol` of each other.
///
/// Subpaths are reversed when that is what makes the ends meet. A chain whose
/// two ends come back together is closed. Closed subpaths pass through
/// unchanged. The order of the result follows the order of the input.
pub fn join_paths(paths: &[Path], tol: Tolerance) -> Vec<Path> {
    join_paths_with(paths, tol, false)
}

/// [`join_paths`], and with `smooth` set, ends that do not quite meet are
/// bridged by a curve that carries on the direction of both vectors instead
/// of moving the first node of the second vector onto the first's end.
pub fn join_paths_with(paths: &[Path], tol: Tolerance, smooth: bool) -> Vec<Path> {
    let mut closed: Vec<SubPath> = Vec::new();
    let mut open: Vec<SubPath> = Vec::new();
    for path in paths {
        for sub in &path.subpaths {
            if sub.segments.is_empty() {
                continue;
            }
            if sub.closed {
                closed.push(sub.clone());
            } else {
                open.push(sub.clone());
            }
        }
    }

    let mut out: Vec<Path> = closed.into_iter().map(SubPath::into_path).collect();
    let mut used = vec![false; open.len()];
    for i in 0..open.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = open[i].clone();
        loop {
            let mut joined = false;
            for (j, candidate) in open.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let head = chain.start;
                let tail = chain.end();
                if tail.approx_eq(candidate.start, tol) {
                    chain = appended(chain, candidate, smooth);
                } else if tail.approx_eq(candidate.end(), tol) {
                    chain = appended(chain, &reverse_subpath(candidate), smooth);
                } else if head.approx_eq(candidate.end(), tol) {
                    chain = appended(candidate.clone(), &chain, smooth);
                } else if head.approx_eq(candidate.start, tol) {
                    chain = appended(reverse_subpath(candidate), &chain, smooth);
                } else {
                    continue;
                }
                used[j] = true;
                joined = true;
                break;
            }
            if !joined {
                break;
            }
        }
        if chain.end().approx_eq(chain.start, tol) && chain.segments.len() > 1 {
            if let Some(last) = chain.segments.last_mut() {
                last.set_end(chain.start);
            }
            chain.closed = true;
        }
        out.push(chain.into_path());
    }
    out
}

/// `first` carried on by `next`, whose start is within the join tolerance
/// of `first`'s end. With `smooth`, a gap between them becomes a cubic
/// tangent to both; otherwise `next`'s first segment starts at `first`'s end.
fn appended(first: SubPath, next: &SubPath, smooth: bool) -> SubPath {
    appended_reach(first, next, smooth.then_some(SMOOTHER_REACH))
}

/// How far along each end's direction the smoother join's curve keeps it,
/// as a share of the gap: a full, rounded bridge.
pub const SMOOTHER_REACH: f64 = 0.5;
/// The same for the symmetrical, shallower join: a flatter bridge.
pub const SHALLOW_REACH: f64 = 0.25;

/// [`appended`] with the smooth bridge's handles `reach` times the gap long.
fn appended_reach(mut first: SubPath, next: &SubPath, reach: Option<f64>) -> SubPath {
    let end = first.end();
    let bridge = reach.filter(|_| {
        end.distance(next.start) > 1e-9 && !first.segments.is_empty() && !next.segments.is_empty()
    });
    if let Some(reach) = bridge {
        let last_index = first.segments.len() - 1;
        let last_start = first.segment_start(last_index).unwrap_or(first.start);
        let out_tangent = segment_tangent_at(last_start, &first.segments[last_index], 1.0);
        let in_tangent = segment_tangent_at(next.start, &next.segments[0], 0.0);
        let d = end.distance(next.start) * reach;
        let c1 = end.offset(out_tangent.scaled(d));
        let c2 = next.start.offset(in_tangent.scaled(-d));
        first.cubic_to(c1, c2, next.start);
    }
    first.segments.extend_from_slice(&next.segments);
    first
}

/// Closes an open subpath, straight or smooth.
pub fn close_subpath(subpath: &SubPath, style: CloseStyle, tol: Tolerance) -> SubPath {
    if subpath.closed || subpath.segments.is_empty() {
        return subpath.clone();
    }
    let mut out = subpath.clone();
    let end = out.end();
    if end.approx_eq(out.start, tol) {
        out.closed = true;
        return out;
    }
    match style {
        CloseStyle::Straight => {
            let start = out.start;
            out.line_to(start);
        }
        CloseStyle::Smooth | CloseStyle::SmoothShallow => {
            let last_index = out.segments.len() - 1;
            let last_start = out.segment_start(last_index).unwrap_or(out.start);
            let out_tangent = segment_tangent_at(last_start, &out.segments[last_index], 1.0);
            let first_start = out.start;
            let in_tangent = segment_tangent_at(first_start, &out.segments[0], 0.0);
            let reach = if style == CloseStyle::SmoothShallow {
                SHALLOW_REACH
            } else {
                SMOOTHER_REACH
            };
            let d = end.distance(first_start) * reach;
            let c1 = end.offset(out_tangent.scaled(d));
            let c2 = first_start.offset(in_tangent.scaled(-d));
            out.cubic_to(c1, c2, first_start);
        }
    }
    out.closed = true;
    out
}

/// How one of the three Join / Close tools bridges a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JoinHow {
    /// A straight line across the gap.
    Straight,
    /// A curve that carries on the direction of both ends (the smoother
    /// method: a full, rounded bridge).
    Smooth,
    /// The symmetrical, shallower smooth method: a flatter bridge.
    SmoothShallow,
    /// Both ends move to the point halfway between them.
    MoveEnds,
}

/// Closes one open subpath the way `how` says. With [`JoinHow::MoveEnds`]
/// both end points move to their midpoint instead of a segment being added.
pub fn close_with(subpath: &SubPath, how: JoinHow, tol: Tolerance) -> SubPath {
    match how {
        JoinHow::Straight => close_subpath(subpath, CloseStyle::Straight, tol),
        JoinHow::Smooth => close_subpath(subpath, CloseStyle::Smooth, tol),
        JoinHow::SmoothShallow => close_subpath(subpath, CloseStyle::SmoothShallow, tol),
        JoinHow::MoveEnds => {
            if subpath.closed || subpath.segments.is_empty() {
                return subpath.clone();
            }
            let mut out = subpath.clone();
            let mid = out.start.midpoint(out.end());
            out.start = mid;
            if let Some(last) = out.segments.last_mut() {
                last.set_end(mid);
            }
            out.closed = true;
            out
        }
    }
}

/// Joins two open subpaths at their closest pair of end points, whatever
/// the gap, bridging it the way `how` says. The answer starts at the far
/// end of `a` and runs through to the far end of `b`.
pub fn join_two(a: &SubPath, b: &SubPath, how: JoinHow) -> SubPath {
    let pairs = [
        (a.end(), b.start, false, false),
        (a.end(), b.end(), false, true),
        (a.start, b.start, true, false),
        (a.start, b.end(), true, true),
    ];
    let mut best = pairs[0];
    for pair in pairs.iter().skip(1) {
        if pair.0.distance(pair.1) < best.0.distance(best.1) - 1e-12 {
            best = *pair;
        }
    }
    let (_, _, flip_a, flip_b) = best;
    let mut first = if flip_a {
        reverse_subpath(a)
    } else {
        a.clone()
    };
    let mut second = if flip_b {
        reverse_subpath(b)
    } else {
        b.clone()
    };
    let gap = first.end().distance(second.start);
    match how {
        JoinHow::MoveEnds => {
            let mid = first.end().midpoint(second.start);
            if let Some(last) = first.segments.last_mut() {
                last.set_end(mid);
            }
            second.start = mid;
            first.segments.extend_from_slice(&second.segments);
            first
        }
        JoinHow::Straight => {
            if gap > 1e-9 {
                first.line_to(second.start);
            }
            first.segments.extend_from_slice(&second.segments);
            first
        }
        JoinHow::Smooth => appended_reach(first, &second, Some(SMOOTHER_REACH)),
        JoinHow::SmoothShallow => appended_reach(first, &second, Some(SHALLOW_REACH)),
    }
}

/// Cuts a subpath at every place it meets `cutter` and drops the piece that
/// contains `pick`.
///
/// Returns the pieces that remain. Returns an error when the two do not meet.
pub fn trim_to_intersection(
    target: &SubPath,
    cutter: &Path,
    pick: Point,
    tol: Tolerance,
) -> Result<Vec<SubPath>, CoreError> {
    let target_path = Path::from_subpath(target.clone());
    let hits = path_intersections(&target_path, cutter, tol);
    if hits.is_empty() {
        return Err(CoreError::geometry(
            "the two vectors do not cross, so there is nothing to trim to",
        ));
    }
    // A crossing on a node (a cut line snapped to a corner, or through a
    // closed vector's start) cuts there too: it is taken as the start of the
    // segment that leaves the node. An open vector's two ends cut nothing.
    let n = target.segments.len();
    let mut cuts: Vec<(usize, f64)> = hits
        .iter()
        .filter_map(|h| {
            let segment = if h.a_t <= 1e-9 {
                h.a_segment
            } else if h.a_t >= 1.0 - 1e-9 {
                h.a_segment + 1
            } else {
                return Some((h.a_segment, h.a_t));
            };
            if target.closed {
                Some((segment % n.max(1), 0.0))
            } else {
                (segment > 0 && segment < n).then_some((segment, 0.0))
            }
        })
        .collect();
    cuts.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    cuts.dedup_by(|a, b| a.0 == b.0 && (a.1 - b.1).abs() < 1e-9);
    if cuts.is_empty() {
        return Err(CoreError::geometry(
            "the two vectors only touch at an end point",
        ));
    }

    let pieces = split_many(target, &cuts)?;
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, piece) in pieces.iter().enumerate() {
        let path = Path::from_subpath(piece.clone());
        if let Some(near) = super::metrics::nearest_point_on_path(&path, pick, tol) {
            if near.distance < best_d {
                best_d = near.distance;
                best = i;
            }
        }
    }
    Ok(pieces
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != best)
        .map(|(_, p)| p)
        .collect())
}

/// Splits a subpath at several (segment, parameter) places at once.
fn split_many(subpath: &SubPath, cuts: &[(usize, f64)]) -> Result<Vec<SubPath>, CoreError> {
    let mut pieces: Vec<SubPath> = Vec::new();
    let mut current = SubPath::open(subpath.start);
    // A cut on the start node of a closed subpath: its first and last
    // pieces stay apart.
    let cut_at_start = cuts.iter().any(|&(i, t)| i == 0 && t <= 1e-12);
    for (gi, (start, seg)) in subpath.iter().enumerate() {
        let at_node = cuts.iter().any(|&(i, t)| i == gi && t <= 1e-12);
        if at_node && !current.segments.is_empty() {
            pieces.push(std::mem::replace(&mut current, SubPath::open(start)));
        }
        let mut local: Vec<f64> = cuts
            .iter()
            .filter(|(i, t)| *i == gi && *t > 1e-12)
            .map(|(_, t)| *t)
            .collect();
        local.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut piece_start = start;
        let mut remaining = *seg;
        let mut consumed = 0.0f64;
        for t in local {
            let local_t = if (1.0 - consumed).abs() < 1e-12 {
                1.0
            } else {
                ((t - consumed) / (1.0 - consumed)).clamp(0.0, 1.0)
            };
            let (head, tail) = split_segment(piece_start, &remaining, local_t)?;
            current.segments.push(head);
            pieces.push(std::mem::replace(&mut current, SubPath::open(head.end())));
            piece_start = head.end();
            remaining = tail;
            consumed = t;
        }
        current.segments.push(remaining);
    }
    pieces.push(current);
    if subpath.closed && pieces.len() > 1 && !cut_at_start {
        let last = pieces.pop().unwrap_or_else(|| SubPath::open(subpath.start));
        if let Some(first) = pieces.first_mut() {
            let mut segments = last.segments;
            segments.extend_from_slice(&first.segments);
            *first = SubPath::new(last.start, segments, false);
        }
    }
    Ok(pieces.into_iter().filter(|p| !p.is_empty()).collect())
}

/// Extends the open end of a subpath in a straight line until it meets
/// `targets`, and adds that piece.
///
/// `at_start` extends the first node instead of the last one.
pub fn extend_to_nearest(
    subpath: &SubPath,
    targets: &Path,
    at_start: bool,
    tol: Tolerance,
) -> Result<SubPath, CoreError> {
    if subpath.closed {
        return Err(CoreError::geometry("a closed vector has no end to extend"));
    }
    if subpath.segments.is_empty() {
        return Err(CoreError::geometry("the vector has no segments"));
    }
    let working = if at_start {
        reverse_subpath(subpath)
    } else {
        subpath.clone()
    };
    let last = working.segments.len() - 1;
    let from = working.end();
    let seg_start = working.segment_start(last).unwrap_or(working.start);
    let dir = segment_tangent_at(seg_start, &working.segments[last], 1.0);
    let reach = path_bbox(targets)
        .map(|b| b.size().length() + from.distance(b.centre()))
        .unwrap_or(0.0)
        + 1.0;
    let ray_end = from.offset(dir.scaled(reach));
    let mut ray = SubPath::open(from);
    ray.line_to(ray_end);
    let hits = path_intersections(&Path::from_subpath(ray), targets, tol);
    let mut best: Option<Point> = None;
    let mut best_d = f64::INFINITY;
    for hit in hits {
        let d = from.distance(hit.point);
        if d > tol.get() && d < best_d {
            best_d = d;
            best = Some(hit.point);
        }
    }
    let meeting = best.ok_or_else(|| {
        CoreError::geometry("the vector does not reach the other one when extended")
    })?;
    let mut extended = working;
    extended.line_to(meeting);
    Ok(if at_start {
        reverse_subpath(&extended)
    } else {
        extended
    })
}

/// Replaces a corner with a fillet of the given radius and kind.
///
/// Both segments meeting at the corner must be straight lines.
pub fn fillet_corner(
    subpath: &SubPath,
    node_index: usize,
    radius: f64,
    kind: FilletKind,
) -> Result<SubPath, CoreError> {
    fillet_corner_on(subpath, node_index, radius, kind, None)
}

/// As [`fillet_corner`], with the wall a T-Bone runs along; `None` takes
/// the longer of the two.
pub fn fillet_corner_on(
    subpath: &SubPath,
    node_index: usize,
    radius: f64,
    kind: FilletKind,
    side: Option<TBoneSide>,
) -> Result<SubPath, CoreError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(CoreError::geometry(format!(
            "fillet radius must be a positive number of millimeters, got {radius}"
        )));
    }
    let corner = Corner::at(subpath, node_index)?;
    let (cut_in, cut_out, segment) = match kind {
        FilletKind::Normal => {
            let half = corner.turn.abs() * 0.5;
            if half <= 1e-9 || (std::f64::consts::PI - half).abs() <= 1e-9 {
                return Err(CoreError::geometry(
                    "a straight join has no corner to fillet",
                ));
            }
            let cut = radius / half.tan();
            (
                cut,
                cut,
                Segment::Arc {
                    to: Point::ORIGIN,
                    bulge: bulge_from_sweep(corner.turn),
                },
            )
        }
        FilletKind::DogBone => {
            // The circle through the corner meets each wall a chord of
            // 2 r sin(turn / 2) back from it and sweeps twice the turn.
            if corner.turn.abs() <= 1e-9 {
                return Err(CoreError::geometry(
                    "a straight join has no corner to fillet",
                ));
            }
            let cut = 2.0 * radius * (corner.turn.abs() * 0.5).sin();
            (
                cut,
                cut,
                Segment::Arc {
                    to: Point::ORIGIN,
                    bulge: bulge_from_sweep(2.0 * corner.turn),
                },
            )
        }
        FilletKind::TBone => {
            let along = side.unwrap_or(if corner.len_in >= corner.len_out {
                TBoneSide::Incoming
            } else {
                TBoneSide::Outgoing
            });
            let (cut_in, cut_out) = match along {
                TBoneSide::Incoming => (2.0 * radius, 0.0),
                TBoneSide::Outgoing => (0.0, 2.0 * radius),
            };
            (
                cut_in,
                cut_out,
                Segment::Arc {
                    to: Point::ORIGIN,
                    bulge: bulge_from_sweep(corner.turn.signum() * std::f64::consts::PI),
                },
            )
        }
        FilletKind::Plasma => {
            // The circle sits in the angle opposite the corner, touching
            // both walls carried on past it, r tan(turn / 2) from the
            // corner; the path goes the long way round it, turning back.
            let half = corner.turn.abs() * 0.5;
            if half <= 1e-6 || (std::f64::consts::FRAC_PI_2 - half) <= 1e-3 {
                return Err(CoreError::geometry(
                    "that corner is too straight or too sharp for a loop",
                ));
            }
            let reach = radius * half.tan();
            let sweep = -corner.turn.signum() * (std::f64::consts::TAU - corner.turn.abs());
            (
                -reach,
                -reach,
                Segment::Arc {
                    to: Point::ORIGIN,
                    bulge: bulge_from_sweep(sweep),
                },
            )
        }
    };
    corner.apply(subpath, cut_in, cut_out, segment)
}

/// The wall a click picks for a T-Bone: the one it is clearly nearer to,
/// or `None` (the longer wall) for a click about as near to both.
pub fn tbone_side_near(subpath: &SubPath, node_index: usize, click: Point) -> Option<TBoneSide> {
    let corner = Corner::at(subpath, node_index).ok()?;
    let off = corner.point.to(click);
    let d_in = corner.dir_in.cross(off).abs();
    let d_out = corner.dir_out.cross(off).abs();
    if d_in < 0.5 * d_out {
        Some(TBoneSide::Incoming)
    } else if d_out < 0.5 * d_in {
        Some(TBoneSide::Outgoing)
    } else {
        None
    }
}

/// Takes a fillet back out: the arc at `segment_index`, between two
/// straight segments, goes and the straight segments are extended to meet
/// in a sharp corner again. Rounded, dog-bone, t-bone and plasma fillets
/// all come out this way, since each ends on the lines of the walls it joins.
///
/// # Errors
/// Returns [`CoreError::Geometry`] when the segment is not an arc between
/// two straight segments, or the two walls are parallel.
pub fn remove_fillet(subpath: &SubPath, segment_index: usize) -> Result<SubPath, CoreError> {
    let n = subpath.segments.len();
    let not_fillet = || CoreError::geometry("that is not a fillet between two straight walls");
    if segment_index >= n || n < 3 {
        return Err(not_fillet());
    }
    let (prev, next) = if subpath.closed {
        ((segment_index + n - 1) % n, (segment_index + 1) % n)
    } else if segment_index == 0 || segment_index + 1 >= n {
        return Err(not_fillet());
    } else {
        (segment_index - 1, segment_index + 1)
    };
    let seg = &subpath.segments;
    if seg[segment_index].kind() != SegmentKind::Arc
        || seg[prev].kind() != SegmentKind::Line
        || seg[next].kind() != SegmentKind::Line
    {
        return Err(not_fillet());
    }
    let a0 = subpath.segment_start(prev).ok_or_else(not_fillet)?;
    let a1 = seg[prev].end();
    let b0 = seg[segment_index].end();
    let b1 = seg[next].end();
    let (da, db) = (a0.to(a1), b0.to(b1));
    let denom = da.cross(db);
    if denom.abs() <= 1e-12 * da.length().max(1.0) * db.length().max(1.0) {
        return Err(CoreError::geometry(
            "the walls either side of that arc are parallel, so they meet in no corner",
        ));
    }
    let t = a0.to(b0).cross(db) / denom;
    let corner = a0.offset(da.scaled(t));
    let mut segments = seg.clone();
    segments[prev].set_end(corner);
    segments.remove(segment_index);
    // The start moves to the corner when the arc began or ended the loop.
    let start = if subpath.closed && (segment_index == 0 || segment_index + 1 == n) {
        corner
    } else {
        subpath.start
    };
    Ok(SubPath::new(start, segments, subpath.closed))
}

/// Replaces a corner with a straight chamfer that cuts back `distance` along
/// both of the segments meeting there.
pub fn chamfer_corner(
    subpath: &SubPath,
    node_index: usize,
    distance: f64,
) -> Result<SubPath, CoreError> {
    if !distance.is_finite() || distance <= 0.0 {
        return Err(CoreError::geometry(format!(
            "chamfer distance must be a positive number of millimeters, got {distance}"
        )));
    }
    let corner = Corner::at(subpath, node_index)?;
    corner.apply(
        subpath,
        distance,
        distance,
        Segment::Line { to: Point::ORIGIN },
    )
}

struct Corner {
    in_idx: usize,
    out_idx: usize,
    point: Point,
    dir_in: Vec2,
    dir_out: Vec2,
    turn: f64,
    len_in: f64,
    len_out: f64,
}

impl Corner {
    fn at(subpath: &SubPath, node_index: usize) -> Result<Corner, CoreError> {
        let (in_idx, out_idx) = adjacent_segments(subpath, node_index)?;
        let in_seg = subpath
            .segments
            .get(in_idx)
            .ok_or_else(|| CoreError::geometry("the node has no incoming segment"))?;
        let out_seg = subpath
            .segments
            .get(out_idx)
            .ok_or_else(|| CoreError::geometry("the node has no outgoing segment"))?;
        if in_seg.kind() != SegmentKind::Line || out_seg.kind() != SegmentKind::Line {
            return Err(CoreError::unsupported(
                "filleting and chamfering need a straight segment on each side of the corner",
            ));
        }
        let prev = subpath
            .segment_start(in_idx)
            .ok_or_else(|| CoreError::geometry("the node has no incoming segment"))?;
        let point = in_seg.end();
        let next = out_seg.end();
        let dir_in = prev
            .to(point)
            .normalised()
            .ok_or_else(|| CoreError::geometry("the incoming segment has zero length"))?;
        let dir_out = point
            .to(next)
            .normalised()
            .ok_or_else(|| CoreError::geometry("the outgoing segment has zero length"))?;
        Ok(Corner {
            in_idx,
            out_idx,
            point,
            dir_in,
            dir_out,
            turn: dir_in.cross(dir_out).atan2(dir_in.dot(dir_out)),
            len_in: prev.distance(point),
            len_out: point.distance(next),
        })
    }

    fn apply(
        &self,
        subpath: &SubPath,
        cut_in: f64,
        cut_out: f64,
        mut corner_segment: Segment,
    ) -> Result<SubPath, CoreError> {
        if cut_in > self.len_in + 1e-9 || cut_out > self.len_out + 1e-9 {
            return Err(CoreError::geometry(
                "the corner treatment does not fit the segments meeting at that node",
            ));
        }
        let p_start = self.point.offset(self.dir_in.scaled(-cut_in));
        let p_end = self.point.offset(self.dir_out.scaled(cut_out));
        corner_segment.set_end(p_end);

        let mut segments = subpath.segments.clone();
        segments[self.in_idx].set_end(p_start);
        let wraps = self.in_idx > self.out_idx;
        if wraps {
            segments.push(corner_segment);
            Ok(SubPath::new(p_end, segments, subpath.closed))
        } else {
            segments.insert(self.out_idx, corner_segment);
            Ok(SubPath::new(subpath.start, segments, subpath.closed))
        }
    }
}

/// Distance between the ends of an open subpath, in millimetres.
pub fn gap_length(subpath: &SubPath) -> f64 {
    subpath.end().distance(subpath.start)
}

/// Total length of a subpath, a convenience wrapper used by the edit tools.
pub fn subpath_span(subpath: &SubPath, tol: Tolerance) -> f64 {
    subpath.iter().map(|(s, g)| segment_length(s, g, tol)).sum()
}

/// Nearest node index to a point, used to pick which node an edit acts on.
pub fn nearest_node(subpath: &SubPath, near: Point) -> Option<usize> {
    let nodes = subpath.nodes();
    let mut best = None;
    let mut best_d = f64::INFINITY;
    for (i, node) in nodes.iter().enumerate() {
        let d = node.distance_squared(near);
        if d < best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
}

/// Parameter along a segment nearest to a point, used when picking a place
/// to insert a node.
pub fn parameter_near(start: Point, segment: &Segment, near: Point) -> f64 {
    nearest_on_segment(start, segment, near).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::construct::{circle, rect};
    use crate::geom::metrics::{path_area, path_length, subpath_bbox};

    const FLAT: Tolerance = Tolerance::FLATTEN;
    const EQ: Tolerance = Tolerance::EQUALITY;

    fn square() -> SubPath {
        rect(Point::ORIGIN, Point::new(10.0, 10.0))
            .expect("rect")
            .subpaths[0]
            .clone()
    }

    #[test]
    fn reverse_flips_the_winding() {
        let path = rect(Point::ORIGIN, Point::new(4.0, 4.0)).expect("rect");
        let back = reverse(&path);
        assert!((path_area(&back, FLAT) + 16.0).abs() < 1e-12);
        assert!(back.subpaths[0].start.approx_eq(Point::ORIGIN, EQ));
    }

    #[test]
    fn reverse_negates_arc_bulges() {
        let path = circle(Point::ORIGIN, 3.0).expect("circle");
        let back = reverse(&path);
        for seg in &back.subpaths[0].segments {
            match seg {
                Segment::Arc { bulge, .. } => assert!((*bulge + 1.0).abs() < 1e-12),
                _ => panic!("expected arcs"),
            }
        }
    }

    #[test]
    fn reverse_twice_is_the_original() {
        let path = circle(Point::ORIGIN, 3.0).expect("circle");
        assert_eq!(reverse(&reverse(&path)), path);
    }

    #[test]
    fn reverse_of_an_empty_chain_is_empty() {
        let (start, segs) = reverse_chain(Point::ORIGIN, &[]);
        assert!(start.approx_eq(Point::ORIGIN, EQ));
        assert!(segs.is_empty());
    }

    #[test]
    fn set_start_node_rotates_a_closed_loop() {
        let sub = square();
        let moved = set_start_node(&sub, Point::new(10.0, 10.0), FLAT).expect("start");
        assert!(moved.start.approx_eq(Point::new(10.0, 10.0), EQ));
        assert_eq!(moved.segments.len(), 4);
        assert!((path_area(&moved.clone().into_path(), FLAT) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn set_start_node_rejects_open_vectors() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert!(set_start_node(&sub, Point::ORIGIN, FLAT).is_err());
    }

    #[test]
    fn split_segment_of_a_line_halves_it() {
        let (a, b) = split_segment(Point::ORIGIN, &Segment::line(Point::new(10.0, 0.0)), 0.5)
            .expect("split");
        assert!(a.end().approx_eq(Point::new(5.0, 0.0), EQ));
        assert!(b.end().approx_eq(Point::new(10.0, 0.0), EQ));
    }

    #[test]
    fn split_segment_of_an_arc_halves_the_sweep() {
        let (a, b) = split_segment(
            Point::new(0.0, 0.0),
            &Segment::arc(Point::new(2.0, 0.0), 1.0),
            0.5,
        )
        .expect("split");
        match (a, b) {
            (Segment::Arc { bulge: ba, .. }, Segment::Arc { bulge: bb, .. }) => {
                let quarter = bulge_from_sweep(std::f64::consts::FRAC_PI_2);
                assert!((ba - quarter).abs() < 1e-12);
                assert!((bb - quarter).abs() < 1e-12);
            }
            _ => panic!("expected arcs"),
        }
    }

    #[test]
    fn split_segment_rejects_a_bad_parameter() {
        assert!(split_segment(Point::ORIGIN, &Segment::line(Point::new(1.0, 0.0)), 1.5).is_err());
    }

    #[test]
    fn insert_node_keeps_the_shape() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        let grown = insert_node(&path.subpaths[0], 0, 0.5).expect("insert");
        assert_eq!(grown.segments.len(), 3);
        let area = path_area(&grown.into_path(), FLAT);
        assert!((area - std::f64::consts::PI * 25.0).abs() < 1e-9);
    }

    #[test]
    fn insert_node_rejects_a_missing_segment() {
        assert!(insert_node(&square(), 9, 0.5).is_err());
    }

    #[test]
    fn delete_node_removes_a_corner() {
        let sub = square();
        let cut = delete_node(&sub, 1).expect("delete");
        assert_eq!(cut.segments.len(), 3);
        assert_eq!(cut.node_count(), 3);
        assert!(cut.closed);
    }

    #[test]
    fn delete_node_zero_keeps_the_loop_closed() {
        let sub = square();
        let cut = delete_node(&sub, 0).expect("delete");
        assert_eq!(cut.segments.len(), 3);
        assert!(cut
            .end()
            .approx_eq(cut.start, Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn delete_node_on_an_open_end_drops_a_segment() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(2.0, 0.0));
        let first = delete_node(&sub, 0).expect("delete");
        assert_eq!(first.segments.len(), 1);
        assert!(first.start.approx_eq(Point::new(1.0, 0.0), EQ));
        let last = delete_node(&sub, 2).expect("delete");
        assert_eq!(last.segments.len(), 1);
    }

    #[test]
    fn delete_node_refuses_to_destroy_a_triangle() {
        let tri = crate::geom::construct::regular_polygon(
            Point::ORIGIN,
            3,
            5.0,
            crate::geom::construct::RadiusKind::Circumscribed,
            0.0,
        )
        .expect("triangle");
        assert!(delete_node(&tri.subpaths[0], 0).is_err());
        assert!(delete_node(&square(), 9).is_err());
    }

    #[test]
    fn convert_segment_to_a_line_drops_the_curve() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        let flat = convert_segment(&path.subpaths[0], 0, SegmentKind::Line).expect("convert");
        assert!(matches!(flat.segments[0], Segment::Line { .. }));
        assert_eq!(flat.segments.len(), 2);
    }

    #[test]
    fn convert_segment_to_an_arc_keeps_the_midpoint() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.cubic_to(
            Point::new(0.0, 4.0),
            Point::new(6.0, 4.0),
            Point::new(6.0, 0.0),
        );
        let arced = convert_segment(&sub, 0, SegmentKind::Arc).expect("convert");
        assert!(matches!(arced.segments[0], Segment::Arc { .. }));
        assert!(arced.segments[0].end().approx_eq(Point::new(6.0, 0.0), EQ));
    }

    #[test]
    fn convert_segment_to_cubics_splits_a_half_circle() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        let cubics = convert_segment(&path.subpaths[0], 0, SegmentKind::Cubic).expect("convert");
        assert_eq!(cubics.segments.len(), 3);
        assert!(matches!(cubics.segments[0], Segment::Cubic { .. }));
    }

    #[test]
    fn convert_segment_rejects_a_missing_index() {
        assert!(convert_segment(&square(), 8, SegmentKind::Line).is_err());
    }

    #[test]
    fn smooth_node_lines_up_the_control_points() {
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(5.0, 5.0))
            .line_to(Point::new(10.0, 0.0));
        let smoothed = smooth_node(&sub, 1).expect("smooth");
        assert_eq!(smoothed.segments.len(), 2);
        let (c2, c1) = match (&smoothed.segments[0], &smoothed.segments[1]) {
            (Segment::Cubic { c2, .. }, Segment::Cubic { c1, .. }) => (*c2, *c1),
            _ => panic!("expected cubics"),
        };
        let node = Point::new(5.0, 5.0);
        let a = c2.to(node).normalised().expect("direction");
        let b = node.to(c1).normalised().expect("direction");
        assert!(a.cross(b).abs() < 1e-9);
    }

    #[test]
    fn smooth_node_needs_two_segments() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(1.0, 0.0));
        assert!(smooth_node(&sub, 1).is_err());
    }

    #[test]
    fn split_at_opens_a_closed_loop() {
        let pieces = split_at(&square(), 0, 0.5).expect("split");
        assert_eq!(pieces.len(), 1);
        assert!(!pieces[0].closed);
        assert!(pieces[0].start.approx_eq(Point::new(5.0, 0.0), EQ));
        assert!((path_length(&pieces[0].clone().into_path(), FLAT) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn split_at_cuts_an_open_path_in_two() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let pieces = split_at(&sub, 0, 0.5).expect("split");
        assert_eq!(pieces.len(), 2);
        assert!((path_length(&pieces[0].clone().into_path(), FLAT) - 5.0).abs() < 1e-12);
        assert!((path_length(&pieces[1].clone().into_path(), FLAT) - 15.0).abs() < 1e-12);
    }

    #[test]
    fn join_paths_links_two_open_runs() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(10.0, 0.0));
        let mut b = SubPath::open(Point::new(10.0, 0.0));
        b.line_to(Point::new(10.0, 10.0));
        let joined = join_paths(&[a.into_path(), b.into_path()], FLAT);
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].subpaths[0].segments.len(), 2);
    }

    #[test]
    fn join_paths_reverses_when_it_has_to() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(10.0, 0.0));
        let mut b = SubPath::open(Point::new(10.0, 10.0));
        b.line_to(Point::new(10.0, 0.0));
        let joined = join_paths(&[a.into_path(), b.into_path()], FLAT);
        assert_eq!(joined.len(), 1);
        assert!(joined[0].subpaths[0]
            .end()
            .approx_eq(Point::new(10.0, 10.0), EQ));
    }

    #[test]
    fn join_paths_closes_a_complete_loop() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(10.0, 0.0));
        let mut b = SubPath::open(Point::new(10.0, 0.0));
        b.line_to(Point::new(10.0, 10.0));
        let mut c = SubPath::open(Point::new(10.0, 10.0));
        c.line_to(Point::ORIGIN);
        let joined = join_paths(&[a.into_path(), b.into_path(), c.into_path()], FLAT);
        assert_eq!(joined.len(), 1);
        assert!(joined[0].subpaths[0].closed);
    }

    #[test]
    fn smooth_join_bridges_a_gap_with_a_curve() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(10.0, 0.0));
        let mut b = SubPath::open(Point::new(10.5, 0.2));
        b.line_to(Point::new(20.0, 0.2));
        let tol = Tolerance::new(1.0).expect("tolerance");
        let paths = [a.into_path(), b.into_path()];
        let smooth = join_paths_with(&paths, tol, true);
        assert_eq!(smooth.len(), 1);
        let segments = &smooth[0].subpaths[0].segments;
        assert_eq!(segments.len(), 3);
        assert!(matches!(segments[1], Segment::Cubic { .. }));
        let plain = join_paths_with(&paths, tol, false);
        assert_eq!(plain[0].subpaths[0].segments.len(), 2);
    }

    #[test]
    fn join_paths_leaves_far_apart_runs_alone() {
        let mut a = SubPath::open(Point::ORIGIN);
        a.line_to(Point::new(1.0, 0.0));
        let mut b = SubPath::open(Point::new(50.0, 0.0));
        b.line_to(Point::new(51.0, 0.0));
        assert_eq!(join_paths(&[a.into_path(), b.into_path()], FLAT).len(), 2);
    }

    #[test]
    fn join_paths_passes_closed_vectors_through() {
        let path = circle(Point::ORIGIN, 3.0).expect("circle");
        let joined = join_paths(&[path], FLAT);
        assert_eq!(joined.len(), 1);
        assert!(joined[0].subpaths[0].closed);
    }

    #[test]
    fn close_straight_adds_one_line() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let closed = close_subpath(&sub, CloseStyle::Straight, FLAT);
        assert!(closed.closed);
        assert_eq!(closed.segments.len(), 3);
        assert!((path_area(&closed.into_path(), FLAT) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn the_shallow_smooth_join_bows_less_than_the_smoother_one() {
        // Two lines heading up, 10 apart: the bridge bows above them.
        let mut a = SubPath::open(Point::new(0.0, 0.0));
        a.line_to(Point::new(0.0, 10.0));
        let mut b = SubPath::open(Point::new(10.0, 10.0));
        b.line_to(Point::new(10.0, 0.0));
        let top = |how: JoinHow| subpath_bbox(&join_two(&a, &b, how)).expect("bbox").max.y;
        let smoother = top(JoinHow::Smooth);
        let shallow = top(JoinHow::SmoothShallow);
        assert!(smoother > shallow + 0.5, "{smoother} {shallow}");
        assert!(shallow > 10.0 + 1e-6);
    }

    #[test]
    fn close_smooth_adds_a_cubic() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let closed = close_subpath(&sub, CloseStyle::Smooth, FLAT);
        assert!(closed.closed);
        assert!(matches!(closed.segments[2], Segment::Cubic { .. }));
    }

    #[test]
    fn close_leaves_an_already_closed_vector_alone() {
        let sub = square();
        assert_eq!(close_subpath(&sub, CloseStyle::Straight, FLAT), sub);
    }

    #[test]
    fn trim_drops_the_picked_piece() {
        let mut target = SubPath::open(Point::new(-10.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        let mut cutter = SubPath::open(Point::new(0.0, -5.0));
        cutter.line_to(Point::new(0.0, 5.0));
        let left = trim_to_intersection(&target, &cutter.into_path(), Point::new(-5.0, 0.0), FLAT)
            .expect("trim");
        assert_eq!(left.len(), 1);
        assert!(left[0]
            .start
            .approx_eq(Point::ORIGIN, Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn trim_needs_a_crossing() {
        let mut target = SubPath::open(Point::new(-10.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        let mut cutter = SubPath::open(Point::new(0.0, 5.0));
        cutter.line_to(Point::new(0.0, 8.0));
        assert!(trim_to_intersection(&target, &cutter.into_path(), Point::ORIGIN, FLAT).is_err());
    }

    #[test]
    fn extend_reaches_the_other_vector() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(5.0, 0.0));
        let mut target = SubPath::open(Point::new(10.0, -5.0));
        target.line_to(Point::new(10.0, 5.0));
        let extended = extend_to_nearest(&sub, &target.into_path(), false, FLAT).expect("extend");
        assert!(extended
            .end()
            .approx_eq(Point::new(10.0, 0.0), Tolerance::new(1e-9).expect("tol")));
    }

    #[test]
    fn extend_fails_when_it_never_meets() {
        let mut sub = SubPath::open(Point::ORIGIN);
        sub.line_to(Point::new(5.0, 0.0));
        let mut target = SubPath::open(Point::new(-10.0, -5.0));
        target.line_to(Point::new(-10.0, 5.0));
        assert!(extend_to_nearest(&sub, &target.into_path(), false, FLAT).is_err());
        assert!(extend_to_nearest(&square(), &Path::new(), false, FLAT).is_err());
    }

    #[test]
    fn fillet_on_a_square_gives_four_arcs() {
        let mut sub = square();
        for index in [1usize, 3, 5, 0] {
            sub = fillet_corner(&sub, index, 2.0, FilletKind::Normal).expect("fillet");
        }
        let arcs = sub
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Arc { .. }))
            .count();
        assert_eq!(arcs, 4);
        let area = path_area(&sub.into_path(), FLAT);
        let expected = 100.0 - 4.0 * 4.0 + std::f64::consts::PI * 4.0;
        assert!((area - expected).abs() < 1e-6, "area {area}");
    }

    #[test]
    fn fillet_at_the_start_node_keeps_the_loop_closed() {
        let sub = fillet_corner(&square(), 0, 2.0, FilletKind::Normal).expect("fillet");
        assert!(sub.closed);
        assert!(sub
            .end()
            .approx_eq(sub.start, Tolerance::new(1e-9).expect("tol")));
        assert_eq!(sub.segments.len(), 5);
    }

    #[test]
    fn dog_bone_circle_passes_through_the_corner() {
        let sub = fillet_corner(&square(), 1, 2.0, FilletKind::DogBone).expect("fillet");
        let bbox = subpath_bbox(&sub).expect("bbox");
        // Centre one radius in along the bisector from (10, 0): the circle
        // reaches 2 - sqrt(2) past each wall and runs through the corner.
        let past = 2.0 - 2f64.sqrt();
        assert!(
            (bbox.max.x - (10.0 + past)).abs() < 1e-6,
            "right edge {}",
            bbox.max.x
        );
        assert!(
            (bbox.min.y + past).abs() < 1e-6,
            "bottom edge {}",
            bbox.min.y
        );
        let path = Path::from_subpath(sub);
        let near =
            crate::geom::nearest_point_on_path(&path, Point::new(10.0, 0.0), FLAT).expect("near");
        assert!(near.distance < 1e-6, "the relief touches the sharp corner");
    }

    #[test]
    fn t_bone_runs_along_the_longer_wall_or_the_one_asked() {
        // A 20 by 10 box: at (20, 0) the incoming wall is the longer one.
        let long = rect(Point::ORIGIN, Point::new(20.0, 10.0))
            .expect("rect")
            .subpaths[0]
            .clone();
        let sub = fillet_corner(&long, 1, 2.0, FilletKind::TBone).expect("fillet");
        let bbox = subpath_bbox(&sub).expect("bbox");
        assert!((bbox.max.x - 20.0).abs() < 1e-9);
        assert!(bbox.min.y < -1.9, "bottom edge {}", bbox.min.y);
        let other = fillet_corner_on(&long, 1, 2.0, FilletKind::TBone, Some(TBoneSide::Outgoing))
            .expect("fillet");
        let bbox = subpath_bbox(&other).expect("bbox");
        assert!(bbox.max.x > 21.9 && bbox.min.y.abs() < 1e-9, "{bbox:?}");
        // At (20, 10) the outgoing wall (the top) is the longer one.
        let top = fillet_corner(&long, 2, 2.0, FilletKind::TBone).expect("fillet");
        assert!(subpath_bbox(&top).expect("bbox").max.y > 11.9);
    }

    #[test]
    fn plasma_loop_runs_past_the_corner_and_back() {
        let sub = fillet_corner(&square(), 1, 2.0, FilletKind::Plasma).expect("fillet");
        // Both walls run on 2 past (10, 0); the loop round the circle at
        // (12, -2) reaches 4 past each wall.
        let bbox = subpath_bbox(&sub).expect("bbox");
        assert!(
            (bbox.max.x - 14.0).abs() < 1e-6,
            "right edge {}",
            bbox.max.x
        );
        assert!(
            (bbox.min.y + 4.0).abs() < 1e-6,
            "bottom edge {}",
            bbox.min.y
        );
        assert!(sub.closed);
        let arc = sub
            .segments
            .iter()
            .find(|s| s.kind() == SegmentKind::Arc)
            .expect("an arc");
        let Segment::Arc { bulge, .. } = arc else {
            unreachable!()
        };
        let sweep = crate::geom::sweep_from_bulge(*bulge);
        assert!(
            (sweep + 1.5 * std::f64::consts::PI).abs() < 1e-9,
            "the loop turns back three quarters round, {sweep}"
        );
        let path = Path::from_subpath(sub);
        let near =
            crate::geom::nearest_point_on_path(&path, Point::new(10.0, 0.0), FLAT).expect("near");
        assert!(near.distance < 1e-6, "the cut still reaches the corner");
        let straight = {
            let mut s = SubPath::open(Point::ORIGIN);
            s.line_to(Point::new(5.0, 0.0));
            s.line_to(Point::new(10.0, 0.0));
            s
        };
        assert!(fillet_corner(&straight, 1, 1.0, FilletKind::Plasma).is_err());
    }

    #[test]
    fn fillet_kinds_read_as_the_form_sends_them() {
        for (text, kind) in [
            ("\"normal\"", FilletKind::Normal),
            ("\"dogbone\"", FilletKind::DogBone),
            ("\"dog_bone\"", FilletKind::DogBone),
            ("\"tbone\"", FilletKind::TBone),
            ("\"t_bone\"", FilletKind::TBone),
            ("\"plasma\"", FilletKind::Plasma),
        ] {
            let read: FilletKind = serde_json::from_str(text).expect(text);
            assert_eq!(read, kind);
        }
    }

    #[test]
    fn every_kind_of_fillet_comes_back_out() {
        for kind in [
            FilletKind::Normal,
            FilletKind::DogBone,
            FilletKind::TBone,
            FilletKind::Plasma,
        ] {
            for node in 0..4 {
                let sub = fillet_corner(&square(), node, 2.0, kind).expect("fillet");
                let arc = sub
                    .segments
                    .iter()
                    .position(|s| s.kind() == SegmentKind::Arc)
                    .expect("an arc");
                let back = remove_fillet(&sub, arc).expect("removed");
                assert_eq!(
                    back.segments.len(),
                    square().segments.len(),
                    "{kind:?} {node}"
                );
                let b = subpath_bbox(&back).expect("bbox");
                assert!(
                    b.min.x.abs() < 1e-9 && b.min.y.abs() < 1e-9,
                    "{kind:?} {node} {b:?}"
                );
                assert!((b.max.x - 10.0).abs() < 1e-9 && (b.max.y - 10.0).abs() < 1e-9);
                assert!(back.segments.iter().all(|s| s.kind() == SegmentKind::Line));
            }
        }
        assert!(remove_fillet(&square(), 1).is_err(), "a line is no fillet");
    }

    #[test]
    fn fillet_rejects_a_radius_that_does_not_fit() {
        assert!(fillet_corner(&square(), 1, 20.0, FilletKind::Normal).is_err());
        assert!(fillet_corner(&square(), 1, 0.0, FilletKind::Normal).is_err());
    }

    #[test]
    fn fillet_rejects_a_curved_corner() {
        let path = circle(Point::ORIGIN, 5.0).expect("circle");
        assert!(fillet_corner(&path.subpaths[0], 0, 1.0, FilletKind::Normal).is_err());
    }

    #[test]
    fn chamfer_cuts_the_corner_straight() {
        let sub = chamfer_corner(&square(), 1, 2.0).expect("chamfer");
        assert_eq!(sub.segments.len(), 5);
        assert!(sub
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
        let area = path_area(&sub.into_path(), FLAT);
        assert!((area - (100.0 - 2.0)).abs() < 1e-9);
    }

    #[test]
    fn chamfer_rejects_a_bad_distance() {
        assert!(chamfer_corner(&square(), 1, -1.0).is_err());
        assert!(chamfer_corner(&square(), 1, 50.0).is_err());
    }

    #[test]
    fn helper_functions_behave() {
        let sub = square();
        assert!(gap_length(&sub) < 1e-12);
        assert!((subpath_span(&sub, FLAT) - 40.0).abs() < 1e-9);
        assert_eq!(nearest_node(&sub, Point::new(9.0, 9.0)), Some(2));
        assert!(nearest_node(&SubPath::open(Point::ORIGIN), Point::ORIGIN).is_some());
        assert!(
            (parameter_near(
                Point::ORIGIN,
                &Segment::line(Point::new(10.0, 0.0)),
                Point::new(2.0, 1.0)
            ) - 0.2)
                .abs()
                < 1e-12
        );
    }
}
