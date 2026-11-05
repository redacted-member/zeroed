//! V-Groove toolpaths: a V shaped cutter follows the centre line (medial
//! axis) of each shape, going deeper where the shape is wider, so the
//! cutter's sides touch both edges at once and sharp corners come out
//! sharp.
//!
//! The centre line is found from a Delaunay triangulation of points spaced
//! evenly round the outlines: the circumcentres of triangles that lie inside
//! the shape are points of the medial axis, the circumradius is the distance
//! to the nearest edge, and neighbouring inside triangles give its edges.
//! Edges whose two defining outline points sit close together are noise
//! from the sampling and are dropped (the lambda medial axis).

use std::collections::{BTreeMap, BTreeSet};

use delaunator::{triangulate, Point as DPoint, EMPTY};
use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Region, SubPath, Tolerance};

use super::common::order_nearest_either_way;
use super::common::{pass_depths, Emitter, Heights};
use super::pocket::{pocket, rest_area, rest_pocket, ClearanceOptions};
use super::profile::Calculated;
use super::tool::{Tool, ToolGeometry};
use super::toolpath::Point3;

/// The V-Groove form's settings. Depths are millimetres below the surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VGrooveParams {
    /// Depth the carving starts at; the V is cut this much deeper.
    pub start_depth: f64,
    /// Deepest the cutter goes; wider parts are left flat at this depth.
    pub flat_depth: Option<f64>,
    /// Most the V may go down in one pass, in millimetres. When the carving
    /// is deeper, the centre line is cut in levels, shallowest first.
    /// Absent means the tool's own pass depth, as the form sends it; older
    /// toolpaths may keep a value here.
    pub max_pass_depth: Option<f64>,
    /// Tool that clears the flat bottomed areas when a flat depth is set.
    /// Older files keep their single tool here; the form sends the list in
    /// `clearance_tools` instead.
    pub clearance_tool: Option<Tool>,
    /// How the clearance tools clear, for a tool without its own options.
    pub clearance_options: ClearanceOptions,
    /// Flat area clearance tools in cutting order: the first clears all it
    /// can, each later one only what the tools before it could not reach.
    pub clearance_tools: Vec<ClearanceTool>,
    /// Carve the shapes in the order their vectors were selected in;
    /// otherwise nearest next.
    pub selection_order: bool,
    /// Start each shape's centre line at the end nearest its vector's
    /// start point; otherwise wherever is nearest.
    pub vector_start: bool,
    /// With a flat depth and no clearance tool, the V tool clears the flat
    /// areas itself, as the V-Groove form does. Callers that want flat
    /// bottomed lines left as they are (sketch carving, inlay waste) leave
    /// it off. Set by the calculation, never read from the form.
    #[serde(skip)]
    pub v_clears_flats: bool,
}

/// One tool of the flat area clearance list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearanceTool {
    /// The tool.
    pub tool: Tool,
    /// For a V-bit or engraving tool after the first: follow the centre
    /// line into the corners the tools before it left, raised so its tip
    /// fits, instead of pocketing.
    #[serde(default)]
    pub corner_sharpen: bool,
    /// This tool's own strategy, direction and ramps; the toolpath's
    /// `clearance_options` when absent. Only the first tool's strategy and
    /// raster angle count: the later tools clear what is left by offsets.
    #[serde(default)]
    pub options: Option<ClearanceOptions>,
}

impl VGrooveParams {
    /// The flat area clearance tools in cutting order, the old single tool
    /// when the list is empty.
    pub fn clearance_list(&self) -> Vec<ClearanceTool> {
        if self.clearance_tools.is_empty() {
            self.clearance_tool
                .iter()
                .map(|tool| ClearanceTool {
                    tool: tool.clone(),
                    corner_sharpen: false,
                    options: None,
                })
                .collect()
        } else {
            self.clearance_tools.clone()
        }
    }
}

impl Default for VGrooveParams {
    fn default() -> Self {
        VGrooveParams {
            start_depth: 0.0,
            flat_depth: None,
            max_pass_depth: None,
            clearance_tool: None,
            clearance_options: ClearanceOptions::default(),
            clearance_tools: Vec::new(),
            selection_order: false,
            vector_start: false,
            v_clears_flats: false,
        }
    }
}

/// A V-Groove and, when a clearance tool is given, the pocket that clears
/// its flat areas. The clearance runs first, then the V-Groove.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VGrooveOutput {
    /// The V shaped tool's moves.
    pub vgroove: Calculated,
    /// The clearance tool's moves, cut before the V-Groove.
    pub clearance: Option<Calculated>,
    /// The later clearance tools that found something left to clear, each
    /// with its place in the list after the first (from 0) and its tool.
    pub later: Vec<(usize, Calculated, Tool)>,
}

/// Half the included angle, in radians, and the flat tip width, in
/// millimetres, of a V shaped tool.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not a V-bit or an
/// engraving tool, or whose included angle is not between 0 and 180 degrees.
pub(crate) fn v_shape(tool: &Tool) -> Result<(f64, f64), CoreError> {
    let (angle, flat) = match tool.tool_type {
        ToolGeometry::VBit { included_angle, .. } => (included_angle, 0.0),
        ToolGeometry::EngravingTool {
            included_angle,
            flat_diameter,
            ..
        } => (included_angle, flat_diameter),
        _ => {
            return Err(CoreError::validation(
                "V-Groove needs a V-bit or an engraving tool",
            ))
        }
    };
    if !(angle > 0.0 && angle < 180.0) {
        return Err(CoreError::validation(
            "the tool's included angle must be between 0 and 180 degrees",
        ));
    }
    Ok(((angle / 2.0).to_radians(), flat.max(0.0)))
}

/// A sample on an outline: where it is, which loop and how far along it.
struct Sample {
    at: Point,
    ring: usize,
    along: f64,
}

/// Evenly spaced points round every loop of the region.
fn sample_outlines(region: &Region, spacing: f64, tol: Tolerance) -> (Vec<Sample>, Vec<f64>) {
    let mut samples = Vec::new();
    let mut perimeters = Vec::new();
    for (ring, sub) in region.loops.iter().enumerate() {
        let line = flatten_subpath(sub, tol);
        let mut points = line.points.clone();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        let n = points.len();
        let mut along = 0.0;
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
            let length = a.distance(b);
            let steps = (length / spacing).ceil().max(1.0) as usize;
            for k in 0..steps {
                let t = k as f64 / steps as f64;
                samples.push(Sample {
                    at: a.lerp(b, t),
                    ring,
                    along: along + length * t,
                });
            }
            along += length;
        }
        perimeters.push(along);
    }
    (samples, perimeters)
}

/// Even-odd point in polygon test against the flattened loops.
/// Whether `p` is inside the polygons by the even-odd rule, testing every
/// edge; [`EdgeBands`] answers the same faster.
#[cfg(test)]
fn inside(polygons: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for poly in polygons {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                odd = !odd;
            }
        }
    }
    odd
}

/// The edges of some polygons sorted into horizontal bands, so the even-odd
/// test of a point looks only at the edges that can cross its row. It
/// answers as [`inside`] does: an edge left out could never count.
struct EdgeBands {
    low: f64,
    height: f64,
    bands: Vec<Vec<(Point, Point)>>,
}

impl EdgeBands {
    fn new(polygons: &[Vec<Point>]) -> EdgeBands {
        let (mut low, mut high, mut count) = (f64::INFINITY, f64::NEG_INFINITY, 0usize);
        for poly in polygons {
            for p in poly {
                low = low.min(p.y);
                high = high.max(p.y);
            }
            count += poly.len();
        }
        let n = (count / 8).clamp(1, 4096);
        let height = (high - low) / n as f64;
        let mut index = EdgeBands {
            low,
            height,
            bands: vec![Vec::new(); n],
        };
        if !(height.is_finite() && height > 0.0) {
            // Flat or empty: one band holds every edge.
            index.bands = vec![Vec::new()];
            index.height = f64::INFINITY;
        }
        for poly in polygons {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                let (from, to) = (index.band(a.y.min(b.y)), index.band(a.y.max(b.y)));
                for band in &mut index.bands[from..=to] {
                    band.push((a, b));
                }
            }
        }
        index
    }

    fn band(&self, y: f64) -> usize {
        let at = ((y - self.low) / self.height).floor();
        if at.is_nan() || at < 0.0 {
            0
        } else {
            (at as usize).min(self.bands.len() - 1)
        }
    }

    fn inside(&self, p: Point) -> bool {
        let mut odd = false;
        for &(a, b) in &self.bands[self.band(p.y)] {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                odd = !odd;
            }
        }
        odd
    }
}

fn circumcentre(a: Point, b: Point, c: Point) -> Option<(Point, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-18 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let x = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let y = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let centre = Point::new(x, y);
    Some((centre, centre.distance(a)))
}

/// A run of the centre line between two graph nodes (junctions or ends).
/// A closed loop starts and ends on the same node.
struct MedialChain {
    /// Node the run starts on.
    from: usize,
    /// Node the run ends on.
    to: usize,
    /// Points along the run, each with its distance to the nearest edge,
    /// in millimetres.
    points: Vec<(Point, f64)>,
}

/// Medial axis chains, joined at shared nodes into a graph.
fn medial_chains(region: &Region, tol: Tolerance) -> Vec<MedialChain> {
    let Some(bbox) = crate::geom::path_bbox(&region.as_path()) else {
        return Vec::new();
    };
    let size = bbox.width().max(bbox.height());
    let spacing = (size / 600.0).clamp(0.03, 0.3);
    let (samples, perimeters) = sample_outlines(region, spacing, tol);
    if samples.len() < 3 {
        return Vec::new();
    }
    let polygons: Vec<Vec<Point>> = region
        .loops
        .iter()
        .map(|sub| flatten_subpath(sub, tol).points)
        .collect();
    let points: Vec<DPoint> = samples
        .iter()
        .map(|s| DPoint {
            x: s.at.x,
            y: s.at.y,
        })
        .collect();
    let mesh = triangulate(&points);
    let triangle_count = mesh.triangles.len() / 3;

    // Inside triangles become medial axis nodes.
    let bands = EdgeBands::new(&polygons);
    let mut node: Vec<Option<(Point, f64)>> = vec![None; triangle_count];
    for (t, slot) in node.iter_mut().enumerate() {
        let (i, j, k) = (
            mesh.triangles[3 * t],
            mesh.triangles[3 * t + 1],
            mesh.triangles[3 * t + 2],
        );
        if let Some((centre, radius)) = circumcentre(samples[i].at, samples[j].at, samples[k].at) {
            if radius < size && bands.inside(centre) {
                *slot = Some((centre, radius));
            }
        }
    }

    // Two outline points that are neighbours along the same loop define no
    // real branch of the centre line, only sampling noise.
    let lambda = 2.5 * spacing;
    let far_apart = |a: usize, b: usize| {
        let (sa, sb) = (&samples[a], &samples[b]);
        if sa.ring != sb.ring {
            return true;
        }
        let perimeter = perimeters[sa.ring];
        let gap = (sa.along - sb.along).abs();
        let around = gap.min(perimeter - gap);
        around > lambda && sa.at.distance(sb.at) > lambda * 0.5
    };
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); triangle_count];
    for e in 0..mesh.halfedges.len() {
        let twin = mesh.halfedges[e];
        if twin == EMPTY || twin < e {
            continue;
        }
        let (t, u) = (e / 3, twin / 3);
        if node[t].is_none() || node[u].is_none() {
            continue;
        }
        let a = mesh.triangles[e];
        let b = mesh.triangles[if e % 3 == 2 { e - 2 } else { e + 1 }];
        if far_apart(a, b) {
            adjacency[t].push(u);
            adjacency[u].push(t);
        }
    }

    // Walk the graph into chains between junctions and ends.
    let mut used = std::collections::BTreeSet::new();
    let mut chains = Vec::new();
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let walk =
        |start: usize, first: usize, used: &mut std::collections::BTreeSet<(usize, usize)>| {
            let mut chain = vec![start];
            let (mut prev, mut at) = (start, first);
            used.insert(key(prev, at));
            loop {
                chain.push(at);
                if adjacency[at].len() != 2 {
                    break;
                }
                let next = if adjacency[at][0] == prev {
                    adjacency[at][1]
                } else {
                    adjacency[at][0]
                };
                if used.contains(&key(at, next)) {
                    break;
                }
                used.insert(key(at, next));
                prev = at;
                at = next;
            }
            chain
        };
    // Chains start at ends and junctions first; whatever is left after
    // that is closed loops of degree two nodes.
    for junctions_only in [true, false] {
        for (start, neighbours) in adjacency.iter().enumerate() {
            if junctions_only && (neighbours.is_empty() || neighbours.len() == 2) {
                continue;
            }
            for &first in neighbours {
                if !used.contains(&key(start, first)) {
                    chains.push(walk(start, first, &mut used));
                }
            }
        }
    }
    let mut out: Vec<MedialChain> = chains
        .into_iter()
        .filter_map(|chain| {
            let (&from, &to) = (chain.first()?, chain.last()?);
            Some(MedialChain {
                from,
                to,
                points: chain.into_iter().filter_map(|t| node[t]).collect(),
            })
        })
        .collect();
    // A shape with no branches (a circle, a dot) still gets its deepest point.
    if out.is_empty() {
        if let Some((t, best)) = node
            .iter()
            .enumerate()
            .filter_map(|(t, n)| n.map(|n| (t, n)))
            .max_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
        {
            out.push(MedialChain {
                from: t,
                to: t,
                points: vec![best],
            });
        }
    }
    out
}

/// Drops points that lie within `tol` of the straight line between their
/// neighbours, in 3D.
fn simplify(points: &[Point3], tol: f64) -> Vec<Point3> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (points[a], points[b]);
        let (dx, dy, dz) = (pb.x - pa.x, pb.y - pa.y, pb.z - pa.z);
        let len2 = dx * dx + dy * dy + dz * dz;
        let mut worst = (0.0, a);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let t = if len2 > 0.0 {
                (((p.x - pa.x) * dx + (p.y - pa.y) * dy + (p.z - pa.z) * dz) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = Point3::new(pa.x + dx * t, pa.y + dy * t, pa.z + dz * t);
            let d = q.distance(*p);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Calculates a V-Groove toolpath for the closed vectors in `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped, bad
/// depths, and when there are no closed vectors.
pub fn vgroove(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    vgroove_with_clearance(paths, job, tool, params, tol).map(|out| out.vgroove)
}

/// Calculates a V-Groove toolpath for the closed vectors in `paths`, and,
/// when a flat depth and a clearance tool are both set, an offset pocket
/// that clears the flat bottomed areas at the flat depth with that tool.
///
/// The flat areas are where the V would go deeper than the flat depth: the
/// region inset by `flat_depth * tan(half angle)` plus half the tool's flat
/// tip, in millimetres.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped, bad
/// depths or pass depth, when there are no closed vectors, and whatever the
/// clearance pocket refuses.
pub fn vgroove_with_clearance(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<VGrooveOutput, CoreError> {
    let plan = plan(paths, job, tool, params, tol)?;
    let mut out = Emitter::new(plan.heights);
    if let Some(flats) = &plan.v_flats {
        let last_cut = flats.moves.iter().rposition(|m| m.kind.is_cutting());
        if let Some(last_cut) = last_cut {
            out.extend(flats.moves[..=last_cut].iter().copied());
        }
    }
    let order = Ordering::of(paths, params, tol);
    for edges in plan.levels {
        emit_linked(&mut out, plan.heights, edges, order.as_ref());
    }
    Ok(VGrooveOutput {
        vgroove: Calculated {
            moves: out.finish(),
            warnings: plan.warnings,
        },
        clearance: plan.clearance,
        later: plan.later,
    })
}

/// Code of the warning given when some areas are left flat at the flat
/// depth with no clearance tool to pocket them.
pub(crate) use crate::warning::FLATS_NEED_A_POCKET;

/// Everything a V-Groove cuts, before it is turned into moves.
struct Plan {
    heights: Heights,
    /// Per level, shallowest first, the centre line runs cut at that level.
    levels: Vec<Vec<Edge>>,
    warnings: Vec<crate::warning::Warning>,
    clearance: Option<Calculated>,
    later: Vec<(usize, Calculated, Tool)>,
    /// The V tool's own clearing of the flat areas, cut before the V-Groove
    /// when there is no clearance tool.
    v_flats: Option<Calculated>,
}

/// One run of the centre line at one level, as the cutter follows it.
struct Edge {
    /// Graph node at the first point.
    from: usize,
    /// Graph node at the last point.
    to: usize,
    /// Tool tip positions, in millimetres.
    points: Vec<Point3>,
    /// Length in XY, in millimetres.
    length: f64,
}

impl Edge {
    fn other(&self, node: usize) -> usize {
        if self.from == node {
            self.to
        } else {
            self.from
        }
    }
}

fn plan(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Plan, CoreError> {
    let (half, flat_tip) = v_shape(tool)?;
    if !params.start_depth.is_finite() || params.start_depth < 0.0 {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if let Some(flat) = params.flat_depth {
        if !flat.is_finite() || flat <= 0.0 {
            return Err(CoreError::validation(
                "the flat depth must be more than zero",
            ));
        }
    }
    if let Some(pass) = params.max_pass_depth {
        if !pass.is_finite() || pass <= 0.0 {
            return Err(CoreError::validation(
                "the pass depth must be more than zero",
            ));
        }
    }
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation(
            "V-Groove needs closed vectors, such as text or shapes",
        ));
    }
    let heights = Heights::from_job(job);
    let slope = half.tan();
    let depth_at = |radius: f64| {
        let carve = ((radius - flat_tip / 2.0).max(0.0)) / slope;
        let limited = match params.flat_depth {
            Some(flat) => carve.min(flat),
            None => carve,
        };
        params.start_depth + limited
    };
    let mut clipped = false;
    // Centre lines as (point, depth below the surface), with their nodes.
    let medial: Vec<MedialChain> = medial_chains(&region, tol)
        .into_iter()
        .map(|chain| MedialChain {
            points: chain
                .points
                .iter()
                .map(|&(p, r)| {
                    if let Some(flat) = params.flat_depth {
                        clipped |= (r - flat_tip / 2.0) / slope > flat + 1e-9;
                    }
                    (p, depth_at(r))
                })
                .collect(),
            ..chain
        })
        .filter(|c| !c.points.is_empty())
        .collect();
    let deepest = medial
        .iter()
        .flat_map(|c| c.points.iter())
        .map(|&(_, d)| d)
        .fold(params.start_depth, f64::max);
    let max_pass = params
        .max_pass_depth
        .or((tool.pass_depth.is_finite() && tool.pass_depth > 0.0).then_some(tool.pass_depth));
    let levels = carve_levels(params.start_depth, deepest, max_pass)?;

    let mut warnings = Vec::new();
    let list = params.clearance_list();
    let mut v_flats = None;
    let (clearance, later) = match (params.flat_depth, list.split_first()) {
        (Some(flat), Some((first, rest))) if clipped => {
            let flats = Flats {
                region: &region,
                depth: flat,
                slope,
                flat_tip,
            };
            let clearance = clear_flats(&flats, job, first, params, tol)?;
            let later = if clearance.is_some() {
                clear_rest(&flats, job, first, rest, params, &mut warnings, tol)?
            } else {
                Vec::new()
            };
            if params.v_clears_flats {
                // What no clearance tool reached is cleared by the V itself,
                // so no floor is left standing in a part too narrow for them.
                let reached = clearance.as_ref().map(|_| {
                    later
                        .iter()
                        .filter(|(index, _, _)| !rest[*index].corner_sharpen)
                        .map(|(_, _, tool)| tool.radius())
                        .fold(first.tool.radius(), f64::min)
                });
                v_flats = clear_unreached_with_v(&flats, reached, job, tool, params, tol)?;
            }
            (clearance, later)
        }
        (Some(flat), None) if clipped && params.v_clears_flats => {
            let flats = Flats {
                region: &region,
                depth: flat,
                slope,
                flat_tip,
            };
            v_flats = clear_flats_with_v(&flats, job, tool, params, tol)?;
            (None, Vec::new())
        }
        _ => (None, Vec::new()),
    };
    if clipped && clearance.is_none() && v_flats.is_none() {
        warnings.push(crate::warning::Warning::flats_need_a_pocket());
    }

    let mut previous: Option<f64> = None;
    let mut per_level = Vec::with_capacity(levels.len());
    for &level in &levels {
        // A chain no deeper than the last level was cut in full already.
        let edges: Vec<Edge> = medial
            .iter()
            .filter(|chain| {
                previous.is_none_or(|p| chain.points.iter().any(|&(_, d)| d > p + 1e-9))
            })
            .map(|chain| {
                let points: Vec<Point3> = chain
                    .points
                    .iter()
                    .map(|&(p, d)| Point3::new(p.x, p.y, heights.z_at(d.min(level))))
                    .collect();
                let points = simplify(&points, 0.005);
                let length = points
                    .windows(2)
                    .map(|w| w[0].xy().distance(w[1].xy()))
                    .sum();
                Edge {
                    from: chain.from,
                    to: chain.to,
                    points,
                    length,
                }
            })
            .collect();
        per_level.push(edges);
        previous = Some(level);
    }
    Ok(Plan {
        heights,
        levels: per_level,
        warnings,
        clearance,
        later,
        v_flats,
    })
}

/// Depths below the surface each level of the V is clamped at, shallowest
/// first, so that no level cuts more than `max_pass` millimetres below the
/// one before. A single level at `deepest` without a pass depth, or when
/// one pass is enough.
fn carve_levels(start: f64, deepest: f64, max_pass: Option<f64>) -> Result<Vec<f64>, CoreError> {
    match max_pass {
        Some(pass) if deepest - start > pass + 1e-9 => pass_depths(start, deepest, pass),
        _ => Ok(vec![deepest]),
    }
}

/// The shape being carved and how the V tool meets its flat depth.
struct Flats<'a> {
    /// The whole carved region.
    region: &'a Region,
    /// Flat depth below the start depth, in millimetres.
    depth: f64,
    /// Tangent of the V tool's half angle.
    slope: f64,
    /// The V tool's flat tip width, in millimetres.
    flat_tip: f64,
}

impl Flats<'_> {
    /// The flat bottomed areas: where the V would go deeper than the flat
    /// depth.
    fn area(&self, tol: Tolerance) -> Region {
        self.region
            .offset(self.depth * self.slope + self.flat_tip / 2.0, tol)
    }
}

/// The pocket settings a clearance tool clears with: its own options, or
/// the toolpath's.
fn clearance_pocket(
    params: &VGrooveParams,
    entry: &ClearanceTool,
    flat: f64,
) -> super::pocket::PocketParams {
    let mut pocket_params = entry
        .options
        .unwrap_or(params.clearance_options)
        .pocket_params(params.start_depth, params.start_depth + flat);
    pocket_params.selection_order = params.selection_order;
    pocket_params
}

/// The offset pocket that clears the flat areas at the flat depth, or
/// `None` when there is no flat area the tool fits in.
fn clear_flats(
    flats: &Flats<'_>,
    job: &JobSetup,
    entry: &ClearanceTool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    let tool = &entry.tool;
    let area = flats.area(tol);
    if area.is_empty() || area.offset(tool.radius(), tol).is_empty() {
        return Ok(None);
    }
    let pocket_params = clearance_pocket(params, entry, flats.depth);
    pocket(&[area.as_path()], job, tool, &pocket_params, tol).map(Some)
}

/// The V tool clearing the flat areas itself at the flat depth, for a
/// V-Groove with a flat depth and no clearance tool. The V is as wide as
/// it is at the flat depth, so its centre covers the flat area exactly;
/// it steps over by its own stepover, or by its stepover percentage of
/// that width when that is less, and goes down by its own pass depth.
/// `None` when there is no flat area.
fn clear_flats_with_v(
    flats: &Flats<'_>,
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    if flats.area(tol).is_empty() {
        return Ok(None);
    }
    pocket(
        &[flats.region.as_path()],
        job,
        &v_floor_tool(flats, tool),
        &v_floor_params(flats, params),
        tol,
    )
    .map(Some)
}

/// The V tool as the end mill it is at the flat depth: as wide as the V is
/// there, stepping over by its own stepover, or by its stepover percentage
/// of that width when that is less.
fn v_floor_tool(flats: &Flats<'_>, tool: &Tool) -> Tool {
    let width = 2.0 * (flats.depth * flats.slope + flats.flat_tip / 2.0);
    let by_percent = width * tool.stepover_percent / 100.0;
    let stepover = if by_percent.is_finite() && by_percent > 0.0 {
        tool.stepover.min(by_percent)
    } else {
        tool.stepover
    };
    Tool {
        tool_type: ToolGeometry::EndMill { diameter: width },
        stepover,
        ..tool.clone()
    }
}

/// The pocket settings the V clears flat areas with, at the flat depth.
fn v_floor_params(flats: &Flats<'_>, params: &VGrooveParams) -> super::pocket::PocketParams {
    let mut pocket_params = params
        .clearance_options
        .pocket_params(params.start_depth, params.start_depth + flats.depth);
    pocket_params.selection_order = params.selection_order;
    pocket_params
}

/// The V tool clearing the flat area the clearance tools left: all of it
/// when `reached` is `None` (no clearance tool fits anywhere), else what a
/// tool of radius `reached` (mm) could not get into. The small cusps any
/// round tool leaves in a corner are left, as the V's own pass at the flat
/// depth runs into those corners. `None` when nothing is left to clear.
fn clear_unreached_with_v(
    flats: &Flats<'_>,
    reached: Option<f64>,
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    let Some(reached) = reached else {
        return clear_flats_with_v(flats, job, tool, params, tol);
    };
    let rest = rest_area(&flats.area(tol), reached, 0.0, 0.0, tol)?;
    // A corner cusp of a round tool in a square corner holds about a fifth
    // of the radius squared; a part the tool could not enter holds more.
    let cusp = 0.25 * reached * reached;
    let kept: Vec<SubPath> = rest
        .outers()
        .filter(|outer| crate::geom::subpath_signed_area(outer) > cusp)
        .cloned()
        .chain(rest.holes().cloned())
        .collect();
    if !kept
        .iter()
        .any(|s| crate::geom::subpath_signed_area(s) > 0.0)
    {
        return Ok(None);
    }
    let rest = Region::from_paths(&[Path { subpaths: kept }], tol);
    // The V's tip runs everywhere in the rest at the flat depth, stepping
    // over as it does when it clears a whole flat area.
    let tip = Tool {
        tool_type: ToolGeometry::EndMill {
            diameter: 2.0 * tol.get(),
        },
        ..v_floor_tool(flats, tool)
    };
    let cut = pocket(
        &[rest.as_path()],
        job,
        &tip,
        &v_floor_params(flats, params),
        tol,
    )?;
    Ok(Some(cut).filter(|c| c.moves.iter().any(|m| m.kind.is_cutting())))
}

/// The later clearance tools' passes: each clears only what the smallest
/// tool before it could not reach, by pocketing, or with corner sharpening
/// by following the centre line into those corners. A tool with nothing
/// left to cut is left out with a warning.
fn clear_rest(
    flats: &Flats<'_>,
    job: &JobSetup,
    first: &ClearanceTool,
    rest: &[ClearanceTool],
    params: &VGrooveParams,
    warnings: &mut Vec<crate::warning::Warning>,
    tol: Tolerance,
) -> Result<Vec<(usize, Calculated, Tool)>, CoreError> {
    let area = flats.area(tol);
    let mut reached = first.tool.radius();
    let mut out = Vec::new();
    for (index, entry) in rest.iter().enumerate() {
        let tool = &entry.tool;
        tool.validate()?;
        let pocket_params = clearance_pocket(params, entry, flats.depth);
        if tool.radius() >= reached - 1e-9 && !entry.corner_sharpen {
            warnings.push(crate::warning::Warning::no_smaller_tool(&tool.name));
            continue;
        }
        let cut = if entry.corner_sharpen {
            sharpen_corners(flats, &area, reached, job, tool, params, tol)?
        } else {
            rest_pocket(&area, reached, job, tool, &pocket_params, tol)?
        };
        match cut {
            Some(cut) if cut.moves.iter().any(|m| m.kind.is_cutting()) => {
                out.push((index, cut, tool.clone()));
            }
            _ => warnings.push(crate::warning::Warning::nothing_left(&tool.name)),
        }
        reached = reached.min(tool.radius());
    }
    Ok(out)
}

/// Corner sharpening: a V-bit or engraving tool follows the shape's centre
/// line where the flat areas' corners were left uncut by a tool of radius
/// `reached` (mm), each point as deep as the tool fits there, down to the
/// flat depth. A tool with a narrower tip than the V tool goes further
/// into the corners; a steeper one is raised to fit.
fn sharpen_corners(
    flats: &Flats<'_>,
    area: &Region,
    reached: f64,
    job: &JobSetup,
    tool: &Tool,
    params: &VGrooveParams,
    tol: Tolerance,
) -> Result<Option<Calculated>, CoreError> {
    let (half, tip) = v_shape(tool).map_err(|_| {
        CoreError::validation("corner sharpening needs a V-bit or an engraving tool")
    })?;
    let slope = half.tan();
    let corners = rest_area(area, reached, 0.0, 0.0, tol)?;
    if corners.is_empty() {
        return Ok(None);
    }
    // The corners reach out a little past the flat area's edge, where the
    // V tool's walls meet the floor.
    let corners = corners.offset(-(tip / 2.0 + tol.get()), tol);
    let heights = Heights::from_job(job);
    let depth_at =
        |r: f64| params.start_depth + ((r - tip / 2.0).max(0.0) / slope).min(flats.depth);
    let mut runs: Vec<Vec<Point3>> = Vec::new();
    for chain in medial_chains(flats.region, tol) {
        let mut run: Vec<Point3> = Vec::new();
        for &(p, r) in &chain.points {
            if corners.contains(p, tol) {
                run.push(Point3::new(p.x, p.y, heights.z_at(depth_at(r))));
            } else if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
    }
    runs.retain(|run| run.len() > 1);
    if runs.is_empty() {
        return Ok(None);
    }
    let runs = order_nearest_either_way(
        runs,
        heights.home.xy(),
        |r| r.first().map_or(heights.home.xy(), |p| p.xy()),
        |r| r.last().map_or(heights.home.xy(), |p| p.xy()),
        |mut r| {
            r.reverse();
            r
        },
    );
    let mut out = Emitter::new(heights);
    for run in runs {
        let Some(first) = run.first() else {
            continue;
        };
        out.travel_to(first.xy());
        out.plunge_to(first.z);
        for p in run.iter().skip(1) {
            out.cut(
                &super::chain::CutSeg {
                    from: out.at().xy(),
                    to: p.xy(),
                    bulge: 0.0,
                },
                p.z,
            );
        }
    }
    Ok(Some(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    }))
}

/// Which shape each centre line piece belongs to, for carving shapes in
/// selection order and starting them near their vector's start point.
struct Ordering {
    /// Each selected vector's area, in selection order.
    shapes: Vec<Region>,
    /// Carve the shapes in that order.
    by_selection: bool,
    /// Each vector's start point, when the carving starts there.
    starts: Option<Vec<Point>>,
    tol: Tolerance,
}

impl Ordering {
    fn of(paths: &[Path], params: &VGrooveParams, tol: Tolerance) -> Option<Ordering> {
        if !params.selection_order && !params.vector_start {
            return None;
        }
        let starts = params.vector_start.then(|| {
            paths
                .iter()
                .map(|p| {
                    p.subpaths
                        .first()
                        .map_or(Point::new(0.0, 0.0), |sub| sub.start)
                })
                .collect()
        });
        Some(Ordering {
            shapes: paths
                .iter()
                .map(|p| Region::from_paths(std::slice::from_ref(p), tol))
                .collect(),
            by_selection: params.selection_order,
            starts,
            tol,
        })
    }

    /// The selected vector whose shape holds `p`, if any.
    fn shape_of(&self, p: Point) -> Option<usize> {
        self.shapes
            .iter()
            .position(|shape| shape.contains(p, self.tol))
    }
}

/// One pass along an edge: which edge, the node it starts from, and
/// whether it goes back over ground already cut.
#[derive(Debug, Clone, Copy)]
struct Step {
    edge: usize,
    from: usize,
    back: bool,
}

/// The walk that cuts every edge of the component holding `root`, starting
/// there: depth first, out along each branch and back along it to the
/// junction, the longest branch last so that the walk can end out at its
/// tip without coming back. Marks the edges it covers in `taken`.
///
/// Going back re-cuts the same points at the same heights, so it only ever
/// touches material the way out already removed.
fn walk_component(
    edges: &[Edge],
    incident: &BTreeMap<usize, Vec<usize>>,
    root: usize,
    taken: &mut [bool],
) -> Vec<Step> {
    // A spanning tree by depth first search; edges that close a loop hang
    // off the node that found them and are walked out and back.
    let mut hangs: BTreeMap<usize, Vec<(usize, Option<usize>)>> = BTreeMap::new();
    let mut seen = BTreeSet::from([root]);
    let mut order = Vec::new();
    let mut stack = vec![root];
    while let Some(u) = stack.pop() {
        order.push(u);
        for &e in incident.get(&u).map_or(&[][..], Vec::as_slice) {
            if taken[e] {
                continue;
            }
            taken[e] = true;
            let v = edges[e].other(u);
            let child = if seen.insert(v) {
                stack.push(v);
                Some(v)
            } else {
                None
            };
            hangs.entry(u).or_default().push((e, child));
        }
    }
    // Length of the longest walk down from each node, in millimetres.
    let mut reach: BTreeMap<usize, f64> = BTreeMap::new();
    for &u in order.iter().rev() {
        let longest = hangs.get(&u).map_or(0.0, |items| {
            items
                .iter()
                .map(|&(e, child)| {
                    edges[e].length + child.map_or(0.0, |c| reach.get(&c).copied().unwrap_or(0.0))
                })
                .fold(0.0, f64::max)
        });
        reach.insert(u, longest);
    }
    let reach_of = |&(e, child): &(usize, Option<usize>)| {
        edges[e].length + child.map_or(0.0, |c| reach.get(&c).copied().unwrap_or(0.0))
    };

    enum Task {
        Visit(usize),
        Cut(Step),
    }
    let mut steps = Vec::new();
    let mut tasks = vec![Task::Visit(root)];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Cut(step) => steps.push(step),
            Task::Visit(u) => {
                let mut items = hangs.remove(&u).unwrap_or_default();
                items.sort_by(|a, b| reach_of(a).total_cmp(&reach_of(b)).then(a.0.cmp(&b.0)));
                // Pushed last first, so they run shortest first.
                for &(e, child) in items.iter().rev() {
                    let far = edges[e].other(u);
                    if far != u {
                        tasks.push(Task::Cut(Step {
                            edge: e,
                            from: far,
                            back: true,
                        }));
                    }
                    if let Some(c) = child {
                        tasks.push(Task::Visit(c));
                    }
                    tasks.push(Task::Cut(Step {
                        edge: e,
                        from: u,
                        back: false,
                    }));
                }
            }
        }
    }
    // Nothing is left to cut after the last new edge.
    while steps.last().is_some_and(|s| s.back) {
        steps.pop();
    }
    steps
}

/// The points of `edge` in the order a pass from node `from` meets them.
fn oriented(edge: &Edge, from: usize) -> impl Iterator<Item = Point3> + '_ {
    let forward = edge.from == from;
    let n = edge.points.len();
    (0..n).map(move |i| edge.points[if forward { i } else { n - 1 - i }])
}

/// Cuts one level's centre line runs as a graph: each connected piece in
/// one continuous walk from one plunge, pieces nearest first, staying down
/// where a piece starts where the last one ended.
fn emit_linked(out: &mut Emitter, heights: Heights, edges: Vec<Edge>, order: Option<&Ordering>) {
    let mut incident: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (e, edge) in edges.iter().enumerate() {
        incident.entry(edge.from).or_default().push(e);
        if edge.to != edge.from {
            incident.entry(edge.to).or_default().push(e);
        }
    }
    // Where each node is, from the first edge that touches it.
    let position = |node: usize| -> Option<Point3> {
        let &e = incident.get(&node)?.first()?;
        let edge = &edges[e];
        if edge.from == node {
            edge.points.first().copied()
        } else {
            edge.points.last().copied()
        }
    };
    let degree = |node: usize| {
        incident.get(&node).map_or(0, |list| {
            list.iter()
                .map(|&e| if edges[e].from == edges[e].to { 2 } else { 1 })
                .sum::<usize>()
        })
    };
    // Where each piece may start: the ends of its branches, or any node
    // of a piece that is all loops. Starting at an end means the walk goes
    // out to the far ends only once.
    let starts: Vec<Vec<(usize, Point3)>> = components(&edges)
        .into_iter()
        .map(|nodes| {
            let ends: Vec<usize> = nodes.iter().copied().filter(|&n| degree(n) == 1).collect();
            let pick = if ends.is_empty() { nodes } else { ends };
            pick.into_iter()
                .filter_map(|n| position(n).map(|p| (n, p)))
                .collect()
        })
        .collect();
    // The selected vector each piece lies in, when the order matters.
    let shape: Vec<Option<usize>> = starts
        .iter()
        .map(|candidates| {
            let first = candidates.first()?;
            order.and_then(|o| o.shape_of(first.1.xy()))
        })
        .collect();
    let mut started: BTreeSet<usize> = BTreeSet::new();
    let mut done = vec![false; starts.len()];
    let mut taken = vec![false; edges.len()];
    loop {
        let here = out.at();
        // In selection order only the earliest vector's pieces are open.
        let rank = |c: usize| match order {
            Some(o) if o.by_selection => shape[c].unwrap_or(usize::MAX),
            _ => 0,
        };
        let open = (0..starts.len()).filter(|&c| !done[c]).map(rank).min();
        let mut best: Option<(f64, usize, usize, Point3)> = None;
        for (c, candidates) in starts.iter().enumerate() {
            if done[c] || Some(rank(c)) != open {
                continue;
            }
            // The first piece of a vector starts nearest its start point.
            let aim = match (order.and_then(|o| o.starts.as_ref()), shape[c]) {
                (Some(points), Some(s)) if !started.contains(&s) => points[s],
                _ => here.xy(),
            };
            for &(n, p) in candidates {
                let d = p.xy().distance(aim);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, c, n, p));
                }
            }
        }
        let Some((_, c, root, first)) = best else {
            break;
        };
        done[c] = true;
        if let Some(s) = shape[c] {
            started.insert(s);
        }
        let joined = (here.x - first.x).abs() < 1e-6
            && (here.y - first.y).abs() < 1e-6
            && here.z <= heights.surface;
        if !joined {
            out.travel_to(first.xy());
        }
        // Straight down in place when a deeper level starts where the last
        // one ended.
        out.plunge_to(first.z);
        for step in walk_component(&edges, &incident, root, &mut taken) {
            for p in oriented(&edges[step.edge], step.from).skip(1) {
                out.cut(
                    &super::chain::CutSeg {
                        from: out.at().xy(),
                        to: p.xy(),
                        bulge: 0.0,
                    },
                    p.z,
                );
            }
        }
    }
}

/// The nodes of each connected piece of the graph, in first seen order.
fn components(edges: &[Edge]) -> Vec<Vec<usize>> {
    let mut parent: BTreeMap<usize, usize> = BTreeMap::new();
    fn find(parent: &mut BTreeMap<usize, usize>, mut n: usize) -> usize {
        let mut root = n;
        while let Some(&p) = parent.get(&root) {
            if p == root {
                break;
            }
            root = p;
        }
        while let Some(&p) = parent.get(&n) {
            if p == root {
                break;
            }
            parent.insert(n, root);
            n = p;
        }
        root
    }
    let mut seen = Vec::new();
    for edge in edges {
        for n in [edge.from, edge.to] {
            if let std::collections::btree_map::Entry::Vacant(slot) = parent.entry(n) {
                slot.insert(n);
                seen.push(n);
            }
        }
        let (a, b) = (find(&mut parent, edge.from), find(&mut parent, edge.to));
        if a != b {
            parent.insert(a.max(b), a.min(b));
        }
    }
    let mut groups: BTreeMap<usize, usize> = BTreeMap::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for n in seen {
        let root = find(&mut parent, n);
        let slot = *groups.entry(root).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[slot].push(n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::common::order_nearest_either_way;
    use crate::cam::toolpath::{Move, MoveKind};
    use crate::geom::{circle, rect, SubPath};

    #[test]
    fn banded_edges_answer_as_every_edge_does() {
        // Stars and rings of many sizes, points everywhere round them,
        // on their vertices and exactly on their rows.
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut polygons: Vec<Vec<Point>> = Vec::new();
        for k in 0..40 {
            let (cx, cy) = (next() * 200.0, next() * 100.0);
            let n = 3 + k % 17;
            polygons.push(
                (0..n)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / n as f64;
                        let r = 2.0 + next() * 20.0;
                        Point::new(cx + r * a.cos(), cy + r * a.sin())
                    })
                    .collect(),
            );
        }
        polygons.push(vec![Point::new(0.0, 5.0), Point::new(10.0, 5.0)]);
        let bands = EdgeBands::new(&polygons);
        let mut probes: Vec<Point> = polygons.iter().flatten().copied().collect();
        for _ in 0..20000 {
            probes.push(Point::new(next() * 240.0 - 20.0, next() * 140.0 - 20.0));
        }
        for p in probes {
            assert_eq!(bands.inside(p), inside(&polygons, p), "{p:?}");
        }
        for flat in [
            Vec::new(),
            vec![vec![Point::new(0.0, 1.0), Point::new(5.0, 1.0)]],
        ] {
            let bands = EdgeBands::new(&flat);
            for p in [
                Point::new(1.0, 1.0),
                Point::new(1.0, 0.0),
                Point::new(9.0, 3.0),
            ] {
                assert_eq!(bands.inside(p), inside(&flat, p));
            }
        }
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn vbit(angle: f64) -> Tool {
        Tool {
            name: "V".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter: 25.0,
                included_angle: angle,
            },
            ..Tool::default()
        }
    }

    fn deepest(moves: &[Move]) -> f64 {
        moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min)
    }

    #[test]
    fn a_square_is_carved_to_half_its_width_with_a_ninety_degree_bit() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let done = vgroove(
            &[square],
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        let z = deepest(&done.moves);
        assert!((z + 5.0).abs() < 0.1, "deepest {z}");
        // The diagonals reach out towards the corners, getting shallower.
        let near_corner = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.to.x < 2.0 && m.to.y < 2.0)
            .count();
        assert!(near_corner > 0);
        assert!(done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .all(|m| m.to.x >= -1e-6
                && m.to.x <= 10.0 + 1e-6
                && m.to.y >= -1e-6
                && m.to.y <= 10.0 + 1e-6));
    }

    #[test]
    fn a_narrower_angle_cuts_deeper() {
        let bar = rect(Point::new(0.0, 0.0), Point::new(40.0, 6.0)).expect("rect");
        let wide = vgroove(
            std::slice::from_ref(&bar),
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        let narrow = vgroove(
            &[bar],
            &JobSetup::default(),
            &vbit(60.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        assert!((deepest(&wide.moves) + 3.0).abs() < 0.1);
        let want = 3.0 / 30f64.to_radians().tan();
        assert!(
            (deepest(&narrow.moves) + want).abs() < 0.15,
            "{}",
            deepest(&narrow.moves)
        );
    }

    #[test]
    fn the_flat_depth_limits_the_cut_and_warns() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            ..VGrooveParams::default()
        };
        let done =
            vgroove(&[square], &JobSetup::default(), &vbit(90.0), &params, tol()).expect("vgroove");
        assert!(deepest(&done.moves) >= -2.0 - 1e-9);
        assert_eq!(done.warnings.len(), 1);
    }

    #[test]
    fn a_circle_is_cut_to_its_radius_at_its_centre() {
        let c = circle(Point::new(20.0, 20.0), 4.0).expect("circle");
        let done = vgroove(
            &[c],
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        let bottom = done
            .moves
            .iter()
            .min_by(|a, b| a.to.z.total_cmp(&b.to.z))
            .expect("moves");
        assert!((bottom.to.z + 4.0).abs() < 0.1, "{bottom:?}");
        assert!(bottom.to.xy().distance(Point::new(20.0, 20.0)) < 0.2);
    }

    #[test]
    fn a_letter_with_a_hole_keeps_its_hole() {
        let outer = rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect");
        let hole = rect(Point::new(6.0, 6.0), Point::new(14.0, 14.0)).expect("rect");
        let done = vgroove(
            &[outer, hole],
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        for m in done.moves.iter().filter(|m| m.kind.is_cutting()) {
            let in_hole = m.to.x > 6.0 + 1e-6
                && m.to.x < 14.0 - 1e-6
                && m.to.y > 6.0 + 1e-6
                && m.to.y < 14.0 - 1e-6;
            assert!(!in_hole, "carved inside the hole: {m:?}");
        }
        // The ring is 6 mm wide, so its straight runs are 3 mm deep; at the
        // corners the widest circle touches two outer edges and the inner
        // corner, radius 6 sqrt 2 / (1 + sqrt 2).
        let corner = 6.0 * 2f64.sqrt() / (1.0 + 2f64.sqrt());
        assert!(
            (deepest(&done.moves) + corner).abs() < 0.1,
            "{}",
            deepest(&done.moves)
        );
        let on_straight_run = done.moves.iter().any(|m| {
            m.kind.is_cutting() && (m.to.y - 3.0).abs() < 0.2 && (m.to.z + 3.0).abs() < 0.05
        });
        assert!(
            on_straight_run,
            "a cut 3 mm deep along the middle of the bottom run"
        );
    }

    #[test]
    fn an_end_mill_is_refused() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let error = vgroove(
            &[square],
            &JobSetup::default(),
            &Tool::default(),
            &VGrooveParams::default(),
            tol(),
        )
        .expect_err("needs a V");
        assert!(matches!(error, CoreError::Validation(_)));
    }

    #[test]
    fn levels_never_step_down_more_than_the_pass_depth() {
        let levels = carve_levels(0.5, 5.5, Some(2.0)).expect("levels");
        assert_eq!(levels.len(), 3);
        let mut last = 0.5;
        for level in &levels {
            assert!(level - last <= 2.0 + 1e-9, "{levels:?}");
            last = *level;
        }
        assert!((last - 5.5).abs() < 1e-12);
        assert_eq!(carve_levels(0.0, 1.5, Some(2.0)).expect("one"), vec![1.5]);
        assert_eq!(carve_levels(0.0, 1.5, None).expect("one"), vec![1.5]);
    }

    #[test]
    fn a_deep_carving_is_cut_in_levels_shallowest_first() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let params = VGrooveParams {
            max_pass_depth: Some(2.0),
            ..VGrooveParams::default()
        };
        let done =
            vgroove(&[square], &JobSetup::default(), &vbit(90.0), &params, tol()).expect("vgroove");
        let deepest_z = deepest(&done.moves);
        assert!((deepest_z + 5.0).abs() < 0.1, "deepest {deepest_z}");
        let levels = carve_levels(0.0, -deepest_z, Some(2.0)).expect("levels");
        assert_eq!(levels.len(), 3);
        // Walk the cutting moves: each level's floor is reached in order, and
        // no move goes below the level being cut.
        let mut level = 0;
        let mut seen = vec![false; levels.len()];
        for m in done.moves.iter().filter(|m| m.kind.is_cutting()) {
            let depth = -m.to.z;
            while depth > levels[level] + 1e-6 {
                assert!(seen[level], "level {level} skipped");
                level += 1;
            }
            if (depth - levels[level]).abs() < 1e-6 {
                seen[level] = true;
            }
        }
        assert!(seen.iter().all(|s| *s), "{seen:?}");
    }

    #[test]
    fn flat_areas_are_cleared_at_the_flat_depth_inside_the_inset() {
        let square = rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect");
        let end_mill = Tool {
            pass_depth: 1.5,
            ..Tool::default()
        };
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            clearance_tool: Some(end_mill),
            ..VGrooveParams::default()
        };
        let done =
            vgroove_with_clearance(&[square], &JobSetup::default(), &vbit(90.0), &params, tol())
                .expect("vgroove");
        let clearance = done.clearance.expect("a clearance toolpath");
        assert!(done.vgroove.warnings.is_empty());
        // Inset by 2 tan 45 = 2 mm, then the 3 mm tool radius: 5 to 15.
        let cuts: Vec<&Move> = clearance
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .collect();
        assert!(!cuts.is_empty());
        for m in &cuts {
            assert!(
                m.to.x >= 5.0 - 1e-6
                    && m.to.x <= 15.0 + 1e-6
                    && m.to.y >= 5.0 - 1e-6
                    && m.to.y <= 15.0 + 1e-6,
                "outside the flat area: {m:?}"
            );
        }
        let min_x = cuts.iter().map(|m| m.to.x).fold(f64::MAX, f64::min);
        assert!((min_x - 5.0).abs() < 1e-3, "{min_x}");
        assert!((deepest(&clearance.moves) + 2.0).abs() < 1e-9);
        assert!(deepest(&done.vgroove.moves) >= -2.0 - 1e-9);
        assert!(cuts.iter().any(|m| (m.to.z + 2.0).abs() < 1e-9));
    }

    /// Distance in XY from `p` to the segment from `a` to `b`.
    fn to_segment(p: Point, a: Point, b: Point) -> f64 {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        p.distance(Point::new(a.x + t * dx, a.y + t * dy))
    }

    #[test]
    fn a_flat_depth_letter_with_no_clearance_tool_leaves_no_uncut_floor() {
        let letter = letter_h();
        let v = Tool {
            stepover: 0.5,
            stepover_percent: 20.0,
            pass_depth: 5.0,
            ..vbit(90.0)
        };
        let params = VGrooveParams {
            flat_depth: Some(1.0),
            v_clears_flats: true,
            ..VGrooveParams::default()
        };
        let done = vgroove_with_clearance(
            std::slice::from_ref(&letter),
            &JobSetup::default(),
            &v,
            &params,
            tol(),
        )
        .expect("vgroove");
        assert!(done.clearance.is_none());
        assert!(
            done.vgroove
                .warnings
                .iter()
                .all(|w| !w.is(FLATS_NEED_A_POCKET)),
            "{:?}",
            done.vgroove.warnings
        );
        // Floor cuts: cutting moves that start and end at the flat depth.
        let mut floor = Vec::new();
        let mut at = Heights::from_job(&JobSetup::default()).home;
        for m in &done.vgroove.moves {
            if m.kind.is_cutting() && (at.z + 1.0).abs() < 1e-9 && (m.to.z + 1.0).abs() < 1e-9 {
                let (a, b) = (at.xy(), m.to.xy());
                match m.kind {
                    MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                        let r = a.distance(centre);
                        let a0 = (a.y - centre.y).atan2(a.x - centre.x);
                        let mut a1 = (b.y - centre.y).atan2(b.x - centre.x);
                        let ccw = matches!(m.kind, MoveKind::ArcCcw { .. });
                        if ccw && a1 <= a0 {
                            a1 += std::f64::consts::TAU;
                        } else if !ccw && a1 >= a0 {
                            a1 -= std::f64::consts::TAU;
                        }
                        let on = |t: f64| {
                            let ang = a0 + (a1 - a0) * t;
                            Point::new(centre.x + r * ang.cos(), centre.y + r * ang.sin())
                        };
                        for k in 0..16 {
                            floor.push((on(k as f64 / 16.0), on((k + 1) as f64 / 16.0)));
                        }
                    }
                    _ => floor.push((a, b)),
                }
            }
            at = m.to;
        }
        assert!(!floor.is_empty());
        // The flat area is the letter inset by 1 mm (1 tan 45). Every point
        // of it is within a stepover over the square root of two of a floor
        // cut (the offset rings round their corners), so no floor is left
        // standing between the passes beyond the V's own ridges.
        let area = Region::from_paths(&[letter], tol()).offset(1.0, tol());
        let mut checked = 0;
        for i in 0..=48 {
            for j in 0..=60 {
                let p = Point::new(i as f64 * 0.5, j as f64 * 0.5);
                if !area.contains(p, tol()) {
                    continue;
                }
                checked += 1;
                let near = floor
                    .iter()
                    .map(|&(a, b)| to_segment(p, a, b))
                    .fold(f64::MAX, f64::min);
                assert!(
                    near <= 0.5 / 2f64.sqrt() + 0.02,
                    "uncut floor at {p:?}: {near}"
                );
            }
        }
        assert!(checked > 50, "{checked}");
        // Without the flag (sketch carving, inlay waste) the flats stay
        // flat bottomed lines with the warning.
        let plain = vgroove(
            std::slice::from_ref(&letter_h()),
            &JobSetup::default(),
            &v,
            &VGrooveParams {
                flat_depth: Some(1.0),
                ..VGrooveParams::default()
            },
            tol(),
        )
        .expect("vgroove");
        assert!(plain.warnings.iter().any(|w| w.is(FLATS_NEED_A_POCKET)));
    }

    #[test]
    fn the_clearance_tool_can_raster_at_an_angle() {
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        let end_mill = Tool {
            pass_depth: 3.0,
            ..Tool::default()
        };
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            clearance_tool: Some(end_mill),
            clearance_options: ClearanceOptions {
                strategy: super::super::pocket::ClearStrategy::Raster,
                raster_angle: 90.0,
                ..ClearanceOptions::default()
            },
            ..VGrooveParams::default()
        };
        let done =
            vgroove_with_clearance(&[square], &JobSetup::default(), &vbit(90.0), &params, tol())
                .expect("vgroove");
        let clearance = done.clearance.expect("a clearance toolpath");
        let along_y = clearance
            .moves
            .windows(2)
            .filter(|w| {
                w[1].kind == MoveKind::Feed
                    && (w[0].to.x - w[1].to.x).abs() < 1e-9
                    && (w[0].to.y - w[1].to.y).abs() > 20.0
            })
            .count();
        assert!(along_y >= 4, "{along_y}");
    }

    #[test]
    fn no_clearance_without_a_flat_depth_or_a_tool() {
        let square = rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect");
        let params = VGrooveParams {
            clearance_tool: Some(Tool::default()),
            ..VGrooveParams::default()
        };
        let done = vgroove_with_clearance(
            std::slice::from_ref(&square),
            &JobSetup::default(),
            &vbit(90.0),
            &params,
            tol(),
        )
        .expect("vgroove");
        assert!(done.clearance.is_none());
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            ..VGrooveParams::default()
        };
        let done =
            vgroove_with_clearance(&[square], &JobSetup::default(), &vbit(90.0), &params, tol())
                .expect("vgroove");
        assert!(done.clearance.is_none());
        assert_eq!(done.vgroove.warnings.len(), 1);
    }

    /// A closed polygon through `points`.
    fn polygon(points: &[(f64, f64)]) -> Path {
        let mut sub = SubPath::open(Point::new(points[0].0, points[0].1));
        for &(x, y) in &points[1..] {
            sub.line_to(Point::new(x, y));
        }
        sub.line_to(Point::new(points[0].0, points[0].1));
        sub.closed = true;
        sub.into_path()
    }

    /// A plus sign 30 mm across with 6 mm wide arms.
    fn plus() -> Path {
        polygon(&[
            (12.0, 0.0),
            (18.0, 0.0),
            (18.0, 12.0),
            (30.0, 12.0),
            (30.0, 18.0),
            (18.0, 18.0),
            (18.0, 30.0),
            (12.0, 30.0),
            (12.0, 18.0),
            (0.0, 18.0),
            (0.0, 12.0),
            (12.0, 12.0),
        ])
    }

    /// A letter H 24 by 30 mm with 6 mm strokes.
    /// XY segments of the cutting moves that run along `z`, arcs split
    /// into short chords.
    fn cuts_at(moves: &[Move], z: f64) -> Vec<(Point, Point)> {
        let mut out = Vec::new();
        let mut at = Heights::from_job(&JobSetup::default()).home;
        for m in moves {
            if m.kind.is_cutting() && (at.z - z).abs() < 1e-9 && (m.to.z - z).abs() < 1e-9 {
                let (a, b) = (at.xy(), m.to.xy());
                match m.kind {
                    MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                        let r = a.distance(centre);
                        let a0 = (a.y - centre.y).atan2(a.x - centre.x);
                        let mut a1 = (b.y - centre.y).atan2(b.x - centre.x);
                        let ccw = matches!(m.kind, MoveKind::ArcCcw { .. });
                        if ccw && a1 <= a0 {
                            a1 += std::f64::consts::TAU;
                        } else if !ccw && a1 >= a0 {
                            a1 -= std::f64::consts::TAU;
                        }
                        let on = |t: f64| {
                            let ang = a0 + (a1 - a0) * t;
                            Point::new(centre.x + r * ang.cos(), centre.y + r * ang.sin())
                        };
                        for k in 0..16 {
                            out.push((on(k as f64 / 16.0), on((k + 1) as f64 / 16.0)));
                        }
                    }
                    _ => out.push((a, b)),
                }
            }
            at = m.to;
        }
        out
    }

    /// Checks every point of the flat area (the shape inset by 1 mm, a
    /// 90 degree V at a flat depth of 1) is floored by the clearance tool
    /// (radius `r`) or by the V's floor passes (stepover 0.5), leaving out
    /// the cusps a round tool leaves in the flat area's corners.
    fn assert_floor_cut(shape: &Path, done: &VGrooveOutput, r: f64, corners: &[(f64, f64)]) {
        let v_floor = cuts_at(&done.vgroove.moves, -1.0);
        let end_floor = done
            .clearance
            .as_ref()
            .map(|c| cuts_at(&c.moves, -1.0))
            .unwrap_or_default();
        let area = Region::from_paths(std::slice::from_ref(shape), tol()).offset(1.0, tol());
        let near = |cuts: &[(Point, Point)], p: Point| {
            cuts.iter()
                .map(|&(a, b)| to_segment(p, a, b))
                .fold(f64::MAX, f64::min)
        };
        let mut checked = 0;
        for i in 0..=100 {
            for j in 0..=100 {
                let p = Point::new(i as f64 * 0.5, j as f64 * 0.5);
                if !area.contains(p, tol())
                    || corners
                        .iter()
                        .any(|&(x, y)| p.distance(Point::new(x, y)) < 1.8 * r)
                {
                    continue;
                }
                checked += 1;
                let by_end_mill = near(&end_floor, p) <= r + 0.02;
                let by_v = near(&v_floor, p) <= 0.5 / 2f64.sqrt() + 0.02;
                assert!(by_end_mill || by_v, "uncut floor at {p:?}");
            }
        }
        assert!(checked > 50, "{checked}");
    }

    #[test]
    fn the_v_clears_flat_areas_a_clearance_tool_cannot_get_into() {
        let v = Tool {
            stepover: 0.5,
            stepover_percent: 20.0,
            pass_depth: 5.0,
            ..vbit(90.0)
        };
        // A stroke 6 mm wide leaves a 4 mm flat: a 5 mm end mill fits nowhere.
        let wide_mill = Tool {
            tool_type: ToolGeometry::EndMill { diameter: 5.0 },
            pass_depth: 5.0,
            ..Tool::default()
        };
        let params = VGrooveParams {
            flat_depth: Some(1.0),
            clearance_tool: Some(wide_mill.clone()),
            v_clears_flats: true,
            ..VGrooveParams::default()
        };
        let letter = polygon(&[(0.0, 0.0), (6.0, 0.0), (6.0, 40.0), (0.0, 40.0)]);
        let done = vgroove_with_clearance(
            std::slice::from_ref(&letter),
            &JobSetup::default(),
            &v,
            &params,
            tol(),
        )
        .expect("vgroove");
        assert!(done.clearance.is_none());
        assert!(
            done.vgroove
                .warnings
                .iter()
                .all(|w| !w.is(FLATS_NEED_A_POCKET)),
            "{:?}",
            done.vgroove.warnings
        );
        assert_floor_cut(&letter, &done, 0.0, &[]);

        // A square head the mill fits in, on a leg it does not: the mill
        // pockets the head, the V the leg.
        let flag = polygon(&[
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 20.0),
            (26.0, 20.0),
            (26.0, 40.0),
            (0.0, 40.0),
        ]);
        let done = vgroove_with_clearance(
            std::slice::from_ref(&flag),
            &JobSetup::default(),
            &v,
            &params,
            tol(),
        )
        .expect("vgroove");
        assert!(done.clearance.is_some());
        // The flat area's corners, where the mill leaves its cusps.
        let corners = [(5.0, 21.0), (25.0, 21.0), (25.0, 39.0), (1.0, 39.0)];
        assert_floor_cut(&flag, &done, 2.5, &corners);
        // The V floors the leg, not the middle of the head the mill cleared.
        let v_floor = cuts_at(&done.vgroove.moves, -1.0);
        assert!(v_floor.iter().any(|(a, _)| a.y < 15.0));
        let inside_head = |p: &Point| p.x > 8.0 && p.x < 22.0 && p.y > 25.0 && p.y < 35.0;
        let in_head = |cuts: &[(Point, Point)]| {
            cuts.iter()
                .filter(|(a, b)| inside_head(a) || inside_head(b))
                .count()
        };
        let centre_lines_only = vgroove_with_clearance(
            std::slice::from_ref(&flag),
            &JobSetup::default(),
            &v,
            &VGrooveParams {
                v_clears_flats: false,
                ..params.clone()
            },
            tol(),
        )
        .expect("vgroove");
        assert_eq!(
            in_head(&v_floor),
            in_head(&cuts_at(&centre_lines_only.vgroove.moves, -1.0))
        );
    }

    fn letter_h() -> Path {
        polygon(&[
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 12.0),
            (18.0, 12.0),
            (18.0, 0.0),
            (24.0, 0.0),
            (24.0, 30.0),
            (18.0, 30.0),
            (18.0, 18.0),
            (6.0, 18.0),
            (6.0, 30.0),
            (0.0, 30.0),
        ])
    }

    /// A letter X made of two crossing bars.
    fn letter_x() -> Path {
        polygon(&[
            (0.0, 0.0),
            (5.0, 0.0),
            (15.0, 11.0),
            (25.0, 0.0),
            (30.0, 0.0),
            (18.0, 15.0),
            (30.0, 30.0),
            (25.0, 30.0),
            (15.0, 19.0),
            (5.0, 30.0),
            (0.0, 30.0),
            (12.0, 15.0),
        ])
    }

    /// A five pointed star.
    fn star() -> Path {
        let points: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let r = if i % 2 == 0 { 20.0 } else { 8.0 };
                let a = std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
                (30.0 + r * a.cos(), 30.0 + r * a.sin())
            })
            .collect();
        polygon(&points)
    }

    /// A ring: a 20 mm radius circle with a 12 mm radius hole.
    fn ring() -> Vec<Path> {
        vec![
            circle(Point::new(40.0, 40.0), 20.0).expect("circle"),
            circle(Point::new(40.0, 40.0), 12.0).expect("circle"),
        ]
    }

    /// The same carving cut the old way: every run on its own, nearest
    /// first, lifting between runs that do not meet end to end.
    fn unlinked(paths: &[Path], params: &VGrooveParams) -> Vec<Move> {
        let plan = plan(paths, &JobSetup::default(), &vbit(90.0), params, tol()).expect("plan");
        let mut out = Emitter::new(plan.heights);
        for edges in plan.levels {
            let home = out.at().xy();
            let chains = order_nearest_either_way(
                edges.into_iter().map(|e| e.points).collect(),
                home,
                |c: &Vec<Point3>| c.first().map_or(home, |p| p.xy()),
                |c: &Vec<Point3>| c.last().map_or(home, |p| p.xy()),
                |mut c| {
                    c.reverse();
                    c
                },
            );
            for chain in chains {
                let Some(first) = chain.first().copied() else {
                    continue;
                };
                let here = out.at();
                let joined = (here.x - first.x).abs() < 1e-6
                    && (here.y - first.y).abs() < 1e-6
                    && here.z <= plan.heights.surface;
                if !joined {
                    out.travel_to(first.xy());
                }
                out.plunge_to(first.z);
                for p in chain.iter().skip(1) {
                    out.cut(
                        &crate::cam::chain::CutSeg {
                            from: out.at().xy(),
                            to: p.xy(),
                            bulge: 0.0,
                        },
                        p.z,
                    );
                }
            }
        }
        out.finish()
    }

    fn linked(paths: &[Path], params: &VGrooveParams) -> Vec<Move> {
        vgroove(paths, &JobSetup::default(), &vbit(90.0), params, tol())
            .expect("vgroove")
            .moves
    }

    /// Times the tool lifts out of the cut, the last lift included.
    fn retracts(moves: &[Move]) -> usize {
        moves
            .windows(2)
            .filter(|w| {
                !matches!(w[0].kind, MoveKind::Rapid)
                    && matches!(w[1].kind, MoveKind::Rapid)
                    && w[1].to.z > w[0].to.z + 1e-9
            })
            .count()
    }

    fn plunges(moves: &[Move]) -> usize {
        moves
            .iter()
            .filter(|m| matches!(m.kind, MoveKind::Plunge))
            .count()
    }

    /// Cutting segments as (from, to) pairs, following the tool.
    fn segments(moves: &[Move]) -> Vec<(Point3, Point3)> {
        let mut at: Option<Point3> = None;
        let mut out = Vec::new();
        for m in moves {
            if m.kind.is_cutting() && !matches!(m.kind, MoveKind::Plunge) {
                if let Some(a) = at {
                    out.push((a, m.to));
                }
            }
            at = Some(m.to);
        }
        out
    }

    fn key(p: Point3) -> (i64, i64) {
        ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64)
    }

    #[test]
    fn a_plus_sign_is_carved_from_a_single_plunge() {
        let moves = linked(&[plus()], &VGrooveParams::default());
        assert_eq!(plunges(&moves), 1);
        assert_eq!(retracts(&moves), 1, "only the final lift");
        assert!(retracts(&unlinked(&[plus()], &VGrooveParams::default())) > 1);
    }

    #[test]
    fn linking_covers_every_run_at_the_same_heights() {
        let shapes: Vec<Vec<Path>> = vec![
            vec![plus()],
            vec![letter_h()],
            vec![letter_x()],
            vec![star()],
            ring(),
        ];
        for (i, paths) in shapes.iter().enumerate() {
            for params in [
                VGrooveParams::default(),
                VGrooveParams {
                    max_pass_depth: Some(1.5),
                    flat_depth: Some(4.0),
                    ..VGrooveParams::default()
                },
            ] {
                let old = unlinked(paths, &params);
                let new = linked(paths, &params);
                assert!(retracts(&new) <= retracts(&old), "shape {i}");
                // Every point the tool reaches is a point the old cut
                // reached, at the same height.
                let mut heights_at = BTreeMap::new();
                for m in old.iter().filter(|m| m.kind.is_cutting()) {
                    heights_at
                        .entry(key(m.to))
                        .or_insert_with(Vec::new)
                        .push(m.to.z);
                }
                for m in new.iter().filter(|m| m.kind.is_cutting()) {
                    let zs = heights_at.get(&key(m.to)).expect("an old point");
                    assert!(
                        zs.iter().any(|z| (z - m.to.z).abs() < 1e-9),
                        "shape {i}: {m:?} not at {zs:?}"
                    );
                }
                // Every old segment is cut, in one direction or the other.
                let cut: BTreeSet<((i64, i64), (i64, i64))> = segments(&new)
                    .into_iter()
                    .flat_map(|(a, b)| [(key(a), key(b)), (key(b), key(a))])
                    .collect();
                for (a, b) in segments(&old) {
                    assert!(
                        cut.contains(&(key(a), key(b))),
                        "shape {i}: missing {a:?} to {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "prints plunge counts; run with --ignored --nocapture"]
    fn report_plunge_counts() {
        let shapes: Vec<(&str, Vec<Path>)> = vec![
            ("plus", vec![plus()]),
            ("H", vec![letter_h()]),
            ("X", vec![letter_x()]),
            ("star", vec![star()]),
            ("ring", ring()),
            ("square with hole", {
                vec![
                    rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect"),
                    rect(Point::new(6.0, 6.0), Point::new(14.0, 14.0)).expect("rect"),
                ]
            }),
            ("H X star side by side", {
                let shift = |p: Path, dx: f64| {
                    crate::geom::transform_path(
                        &p,
                        &crate::geom::Mat3::translate(crate::geom::Vec2::new(dx, 0.0)),
                    )
                };
                vec![letter_h(), shift(letter_x(), 40.0), shift(star(), 60.0)]
            }),
        ];
        for (name, paths) in shapes {
            for params in [
                VGrooveParams::default(),
                VGrooveParams {
                    max_pass_depth: Some(1.5),
                    ..VGrooveParams::default()
                },
            ] {
                let old = unlinked(&paths, &params);
                let new = linked(&paths, &params);
                let length = |moves: &[Move]| {
                    segments(moves)
                        .iter()
                        .map(|(a, b)| a.distance(*b))
                        .sum::<f64>()
                };
                println!(
                    "{name} pass {:?}: retracts {} -> {}, plunges {} -> {}, cut length {:.1} -> {:.1} mm",
                    params.max_pass_depth,
                    retracts(&old),
                    retracts(&new),
                    plunges(&old),
                    plunges(&new),
                    length(&old),
                    length(&new)
                );
            }
        }
    }

    fn mill(diameter: f64) -> Tool {
        let mut tool = Tool {
            name: format!("End Mill {diameter}"),
            tool_type: ToolGeometry::EndMill { diameter },
            pass_depth: 3.0,
            ..Tool::default()
        };
        tool.set_stepover(diameter * 0.4);
        tool
    }

    fn engraver() -> Tool {
        Tool {
            name: "Engraver".to_string(),
            tool_type: ToolGeometry::EngravingTool {
                diameter: 6.0,
                included_angle: 60.0,
                flat_diameter: 0.3,
            },
            ..Tool::default()
        }
    }

    fn flat_square(list: Vec<ClearanceTool>) -> VGrooveOutput {
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        vgroove_with_clearance(
            &[square],
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams {
                flat_depth: Some(2.0),
                clearance_tools: list,
                ..VGrooveParams::default()
            },
            tol(),
        )
        .expect("vgroove")
    }

    fn entry(tool: Tool, corner_sharpen: bool) -> ClearanceTool {
        ClearanceTool {
            tool,
            corner_sharpen,
            options: None,
        }
    }

    #[test]
    fn each_clearance_tool_clears_with_its_own_options() {
        let clearance = |options: Option<ClearanceOptions>| {
            let first = ClearanceTool {
                options,
                ..entry(mill(6.0), false)
            };
            flat_square(vec![first])
                .clearance
                .expect("the flats are cleared")
                .moves
        };
        let raster = ClearanceOptions {
            strategy: crate::cam::pocket::ClearStrategy::Raster,
            raster_angle: 45.0,
            ..ClearanceOptions::default()
        };
        assert_eq!(
            clearance(None),
            clearance(Some(ClearanceOptions::default()))
        );
        assert_ne!(clearance(None), clearance(Some(raster)));
    }

    #[test]
    fn without_a_max_pass_depth_the_tool_pass_depth_sets_the_levels() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let tool = Tool {
            pass_depth: 2.0,
            ..vbit(90.0)
        };
        let job = JobSetup::default();
        let own = vgroove(
            std::slice::from_ref(&square),
            &job,
            &tool,
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        let given = VGrooveParams {
            max_pass_depth: Some(2.0),
            ..VGrooveParams::default()
        };
        let set = vgroove(
            std::slice::from_ref(&square),
            &job,
            &vbit(90.0),
            &given,
            tol(),
        )
        .expect("vgroove");
        assert_eq!(own.moves, set.moves);
        // A pass depth the carving does not reach leaves one level.
        let deep = Tool {
            pass_depth: 50.0,
            ..vbit(90.0)
        };
        let one =
            vgroove(&[square], &job, &deep, &VGrooveParams::default(), tol()).expect("vgroove");
        assert!(one.moves.len() < own.moves.len());
    }

    fn cut_points(moves: &[Move]) -> Vec<Point3> {
        moves
            .iter()
            .filter(|m| m.kind.is_cutting())
            .map(|m| m.to)
            .collect()
    }

    /// Distance from the nearest corner of the 40 mm square.
    fn corner_distance(p: Point3) -> f64 {
        let dx = p.x.min(40.0 - p.x);
        let dy = p.y.min(40.0 - p.y);
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn a_later_clearance_tool_clears_only_the_corners_the_first_left() {
        let out = flat_square(vec![entry(mill(12.0), false), entry(mill(3.0), false)]);
        assert!(out.clearance.is_some(), "the big tool clears the flats");
        assert_eq!(out.later.len(), 1);
        let (index, cut, tool) = &out.later[0];
        assert_eq!(*index, 0);
        assert_eq!(tool.name, "End Mill 3");
        let points = cut_points(&cut.moves);
        assert!(!points.is_empty());
        // The small tool stays in the corners and cuts at the flat depth.
        assert!(
            points.iter().all(|p| corner_distance(*p) < 12.0),
            "{points:?}"
        );
        assert!((deepest(&cut.moves) + 2.0).abs() < 1e-6);
        assert!(!points
            .iter()
            .any(|p| (p.x - 20.0).abs() < 5.0 && (p.y - 20.0).abs() < 5.0));
    }

    #[test]
    fn a_later_tool_no_smaller_than_the_one_before_is_left_out() {
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            clearance_tools: vec![entry(mill(6.0), false), entry(mill(8.0), false)],
            ..VGrooveParams::default()
        };
        let out =
            vgroove_with_clearance(&[square], &JobSetup::default(), &vbit(90.0), &params, tol())
                .expect("vgroove");
        assert!(out.later.is_empty());
        assert!(out
            .vgroove
            .warnings
            .iter()
            .any(|w| w.message.contains("End Mill 8")));
    }

    #[test]
    fn corner_sharpening_follows_the_centre_line_into_the_corners() {
        let out = flat_square(vec![entry(mill(12.0), false), entry(engraver(), true)]);
        assert_eq!(out.later.len(), 1);
        let (_, cut, _) = &out.later[0];
        let points = cut_points(&cut.moves);
        assert!(!points.is_empty());
        // Along the diagonals only, no deeper than the flat depth.
        assert!(points.iter().all(|p| corner_distance(*p) < 12.0));
        assert!(points
            .iter()
            .all(|p| ((p.x - p.y).abs() < 0.3) || ((p.x + p.y - 40.0).abs() < 0.3)));
        assert!(deepest(&cut.moves) >= -2.0 - 1e-6);
        // It reaches further into each corner than the big tool did.
        let tip = points
            .iter()
            .map(|p| corner_distance(*p))
            .fold(f64::MAX, f64::min);
        assert!(tip < 3.5, "reaches {tip} from the corner");
        // A cutter that is not V shaped cannot sharpen corners.
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        let params = VGrooveParams {
            flat_depth: Some(2.0),
            clearance_tools: vec![entry(mill(12.0), false), entry(mill(3.0), true)],
            ..VGrooveParams::default()
        };
        assert!(vgroove_with_clearance(
            &[square],
            &JobSetup::default(),
            &vbit(90.0),
            &params,
            tol()
        )
        .is_err());
    }

    #[test]
    fn the_old_single_clearance_tool_still_reads() {
        let params: VGrooveParams = serde_json::from_value(serde_json::json!({
            "flatDepth": 2.0,
            "clearanceTool": mill(6.0),
        }))
        .expect("parses");
        assert_eq!(params.clearance_list().len(), 1);
        assert!(!params.clearance_list()[0].corner_sharpen);
    }

    fn first_cut(moves: &[Move]) -> Point3 {
        moves
            .iter()
            .find(|m| m.kind.is_cutting() && m.to.z < 0.0)
            .map(|m| m.to)
            .expect("a cut")
    }

    #[test]
    fn shapes_are_carved_in_selection_order_when_asked() {
        let left = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let right = rect(Point::new(60.0, 0.0), Point::new(70.0, 10.0)).expect("rect");
        // Home is at the origin, so nearest first starts on the left.
        let paths = [right, left];
        let nearest = vgroove(
            &paths,
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams::default(),
            tol(),
        )
        .expect("vgroove");
        let ordered = vgroove(
            &paths,
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams {
                selection_order: true,
                ..VGrooveParams::default()
            },
            tol(),
        )
        .expect("vgroove");
        let home_x = Heights::from_job(&JobSetup::default()).home.x;
        let first_nearest = first_cut(&nearest.moves);
        let first_ordered = first_cut(&ordered.moves);
        assert!(
            (first_nearest.x < 35.0) == (home_x < 35.0),
            "nearest first starts on the home side"
        );
        assert!(
            first_ordered.x > 55.0,
            "selection order starts on the right"
        );
    }

    #[test]
    fn a_shape_starts_nearest_its_vector_start_when_asked() {
        // A plus shaped outline whose first node is at the top arm's end.
        let mut plus = rect(Point::new(0.0, 20.0), Point::new(60.0, 40.0)).expect("rect");
        let upright = rect(Point::new(20.0, 0.0), Point::new(40.0, 60.0)).expect("rect");
        plus = crate::geom::boolean::weld(&[plus, upright], tol())
            .expect("weld")
            .into_iter()
            .next()
            .expect("one outline");
        let start = plus.subpaths[0].start;
        let done = vgroove(
            std::slice::from_ref(&plus),
            &JobSetup::default(),
            &vbit(90.0),
            &VGrooveParams {
                vector_start: true,
                ..VGrooveParams::default()
            },
            tol(),
        )
        .expect("vgroove");
        let first = first_cut(&done.moves);
        // The first cut is at the branch end nearest the start node.
        let ends = [(30.0, 60.0), (30.0, 0.0), (0.0, 30.0), (60.0, 30.0)];
        let nearest_end = ends
            .iter()
            .min_by(|a, b| {
                let da = Point::new(a.0, a.1).distance(start);
                let db = Point::new(b.0, b.1).distance(start);
                da.total_cmp(&db)
            })
            .copied()
            .expect("an end");
        let arm = |p: Point3| {
            ends.iter()
                .min_by(|a, b| {
                    Point::new(a.0, a.1)
                        .distance(p.xy())
                        .total_cmp(&Point::new(b.0, b.1).distance(p.xy()))
                })
                .copied()
                .expect("an end")
        };
        assert_eq!(arm(first), nearest_end, "first {first:?} start {start:?}");
    }
}
