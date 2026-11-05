//! Exact solids of the Assembly View's pieces, for measuring fits.
//!
//! The toolpath simulation is a grid, so a piece's relief is only as good
//! as its cell size. Here a piece is built from the cuts themselves: each
//! cutting move swept by the tool's radius, the way the tool clears the
//! material. A profile outside, inside or on a vector leaves its wall at
//! the vector offset by the radius on the cut side (and its allowance), a
//! pocket its boundary; nothing is rounded to a grid. Moves that reach
//! through the material part the piece from the waste (tabs, thinner than
//! half the material, count as cut, as on the grid); shallower moves sink
//! recesses at their own depth.
//!
//! A piece is then a stack of slabs: between two recess depths its
//! section is its outline less every recess deeper than the slab.
//! A tool whose walls are not upright (a V bit, a ball nose) sinks its
//! recess as terraces: slabs a little apart in height, each as wide as the
//! tool is at the terrace's middle, so the sloped wall is followed to
//! within [`TERRACE_STEP`]. Cutting through, such a tool parts the piece
//! as wide as it is in the lowest terrace, and each terrace above sinks a
//! wider kerf from the top, so the piece keeps its sloped edge (the swept
//! tool profile). Cuts that cannot be made so (cuts from below, 3D work at
//! many depths) and that cut deep leave no exact edge where they run; the
//! pieces they touch fall back to the grid.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;

use crate::cam::tool::ToolGeometry;
use crate::cam::toolpath::{MoveKind, Toolpath};
use crate::doc::{Document, Side};
use crate::error::CoreError;

use super::pieces::SheetParts;

/// A closed ring of points, millimetres.
pub type Contour = Vec<[f64; 2]>;
/// An area: its outer ring (counter clockwise) then its holes (clockwise).
pub type Shape = Vec<Contour>;
/// Several separate areas.
pub type Shapes = Vec<Shape>;

/// Largest distance a flattened arc strays from the true arc, mm.
const ARC_TOLERANCE: f64 = 0.001;

/// Material a through cut leaves standing thinner than this share of the
/// material counts as a tab, cut through (as on the simulation grid).
const TAB_SHARE: f64 = 0.5;

/// A toolpath that cannot be made exact spoils the pieces it runs over
/// only when it cuts deeper than this share of the material; shallower
/// (decoration) is left out of the exact solid.
const DEEP_SHARE: f64 = 0.25;

/// Most cutting segments one sheet may have for an exact solid.
const MAX_SEGMENTS: usize = 60_000;

/// Most distinct recess depths one toolpath may cut; more is 3D work.
const MAX_DEPTHS: usize = 64;

/// A terrace of a sloped wall is at most this much narrower than the
/// wall's widest place in it, mm.
pub const TERRACE_STEP: f64 = 0.1;

/// Most terraces one depth of a sloped cut is sunk in.
const MAX_TERRACES: usize = 12;

/// Most terraces a through cut with a sloped tool is sunk in.
const MAX_THROUGH_TERRACES: usize = 48;

/// Narrowest kerf a through cut parts the material with, mm (a V bit's
/// point just reaching the underside still parts it).
const MIN_KERF: f64 = 0.005;

/// Depths closer than this are one depth, mm.
const DEPTH_STEP: f64 = 1e-3;

/// A straight piece of tool travel: the tool centre from `a` to `b`, its
/// tip at `z` (the lower end of a sloped move).
#[derive(Debug, Clone, Copy)]
struct Seg {
    a: [f64; 2],
    b: [f64; 2],
    z: f64,
}

/// Signed area of a ring, positive counter clockwise.
pub fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let mut sum = 0.0;
    for i in 0..n {
        let p = ring[i];
        let q = ring[(i + 1) % n];
        sum += p[0] * q[1] - q[0] * p[1];
    }
    sum / 2.0
}

/// Net area of a set of shapes.
pub fn shapes_area(shapes: &Shapes) -> f64 {
    shapes
        .iter()
        .flat_map(|shape| shape.iter())
        .map(|ring| ring_area(ring))
        .sum()
}

/// Puts every outer ring counter clockwise and every hole clockwise, and
/// drops rings too small to matter.
fn tidy(shapes: Shapes) -> Shapes {
    shapes
        .into_iter()
        .filter_map(|shape| {
            let mut rings = shape.into_iter().filter(|r| r.len() >= 3);
            let mut outer = rings.next()?;
            if ring_area(&outer) < 0.0 {
                outer.reverse();
            }
            if ring_area(&outer) < 1e-9 {
                return None;
            }
            let mut out = vec![outer];
            for mut hole in rings {
                if ring_area(&hole) > 0.0 {
                    hole.reverse();
                }
                if ring_area(&hole).abs() > 1e-9 {
                    out.push(hole);
                }
            }
            Some(out)
        })
        .collect()
}

fn contours(shapes: &Shapes) -> Vec<Contour> {
    shapes.iter().flat_map(|s| s.iter().cloned()).collect()
}

/// `a` less `b`.
pub fn difference(a: &Shapes, b: &Shapes) -> Shapes {
    if b.is_empty() {
        return a.clone();
    }
    if a.is_empty() {
        return Vec::new();
    }
    tidy(contours(a).overlay(&contours(b), OverlayRule::Difference, FillRule::NonZero))
}

/// Where `a` and `b` overlap.
pub fn intersection(a: &Shapes, b: &Shapes) -> Shapes {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    tidy(contours(a).overlay(&contours(b), OverlayRule::Intersect, FillRule::NonZero))
}

/// Everything any of the rings covers.
fn union_of(rings: &[Contour]) -> Shapes {
    if rings.is_empty() {
        return Vec::new();
    }
    tidy(rings.to_vec().simplify_shape(FillRule::NonZero))
}

/// The angle step for arcs of `radius`: a whole fraction of a turn (a
/// power of two), so arcs of about the same size round the same centre are
/// cut at the same angles and their facets face each other exactly.
fn angle_step(radius: f64) -> f64 {
    let wanted = 2.0 * (1.0 - ARC_TOLERANCE / radius.max(ARC_TOLERANCE * 2.0)).acos();
    let mut n = 16.0_f64;
    while std::f64::consts::TAU / n > wanted && n < 8192.0 {
        n *= 2.0;
    }
    std::f64::consts::TAU / n
}

/// Points of an arc of `radius` round `c` from angle `from` turning by
/// `sweep` (radians, either way): the ends exactly and the whole steps of
/// the global angle grid between them.
fn arc_points(c: [f64; 2], radius: f64, from: f64, sweep: f64, out: &mut Vec<[f64; 2]>) {
    let step = angle_step(radius);
    let at = |angle: f64| [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()];
    out.push(at(from));
    let to = from + sweep;
    if sweep >= 0.0 {
        let mut k = (from / step).floor() + 1.0;
        while k * step < to - 1e-9 {
            out.push(at(k * step));
            k += 1.0;
        }
    } else {
        let mut k = (from / step).ceil() - 1.0;
        while k * step > to + 1e-9 {
            out.push(at(k * step));
            k -= 1.0;
        }
    }
    out.push(at(to));
}

/// The area a tool of `radius` sweeps along a segment: a round ended
/// strip whose long sides lie exactly `radius` from it.
fn capsule(seg: &Seg, radius: f64) -> Contour {
    let (dx, dy) = (seg.b[0] - seg.a[0], seg.b[1] - seg.a[1]);
    let mut out = Vec::new();
    if dx.hypot(dy) < 1e-9 {
        arc_points(seg.a, radius, 0.0, std::f64::consts::TAU, &mut out);
        out.pop();
        return out;
    }
    let heading = dy.atan2(dx);
    let half = std::f64::consts::FRAC_PI_2;
    arc_points(
        seg.b,
        radius,
        heading - half,
        std::f64::consts::PI,
        &mut out,
    );
    arc_points(
        seg.a,
        radius,
        heading + half,
        std::f64::consts::PI,
        &mut out,
    );
    out
}

/// The cutting moves of a toolpath as straight segments, arcs flattened on
/// the global angle grid.
fn segments(toolpath: &Toolpath) -> Vec<Seg> {
    segments_of(toolpath.home, &toolpath.moves)
}

/// Moves from `home` as straight segments, arcs flattened on the global
/// angle grid.
fn segments_of(home: crate::cam::Point3, moves: &[crate::cam::toolpath::Move]) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut at = home;
    for m in moves {
        let to = m.to;
        match m.kind {
            MoveKind::Rapid => {}
            MoveKind::Dwell { .. } => continue,
            MoveKind::Feed | MoveKind::Plunge => out.push(Seg {
                a: [at.x, at.y],
                b: [to.x, to.y],
                z: at.z.min(to.z),
            }),
            MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                let c = [centre.x, centre.y];
                let radius = (at.x - c[0]).hypot(at.y - c[1]);
                let from = (at.y - c[1]).atan2(at.x - c[0]);
                let end = (to.y - c[1]).atan2(to.x - c[0]);
                let ccw = matches!(m.kind, MoveKind::ArcCcw { .. });
                let mut sweep = end - from;
                if ccw && sweep <= 1e-12 {
                    sweep += std::f64::consts::TAU;
                } else if !ccw && sweep >= -1e-12 {
                    sweep -= std::f64::consts::TAU;
                }
                let mut points = Vec::new();
                arc_points(c, radius, from, sweep, &mut points);
                let n = points.len().saturating_sub(1).max(1) as f64;
                for (i, pair) in points.windows(2).enumerate() {
                    let z0 = at.z + (to.z - at.z) * i as f64 / n;
                    let z1 = at.z + (to.z - at.z) * (i + 1) as f64 / n;
                    out.push(Seg {
                        a: pair[0],
                        b: pair[1],
                        z: z0.min(z1),
                    });
                }
            }
        }
        at = to;
    }
    out
}

/// Whether a tool cuts upright walls its full radius out from its centre,
/// down to its tip.
fn upright(tool: &ToolGeometry) -> bool {
    matches!(
        tool,
        ToolGeometry::EndMill { .. } | ToolGeometry::LaserTool { .. }
    )
}

/// The terraces a tool sinks a recess of depth `depth` in: each one's
/// depth from the top and the radius it cuts at. An upright tool cuts one,
/// its full radius to the bottom; a sloped one a terrace for each
/// [`TERRACE_STEP`] its radius grows up to its shoulder (at most
/// [`MAX_TERRACES`]), each as wide as the tool is at its middle height, and
/// its full radius above the shoulder.
fn terraces(tool: &ToolGeometry, depth: f64) -> Vec<(f64, f64)> {
    if upright(tool) {
        return vec![(depth, tool.radius())];
    }
    let rise = tool.shoulder_height().min(depth).max(0.0);
    let grow = tool.radius_at_height(rise) - tool.radius_at_height(0.0);
    let count = ((grow / TERRACE_STEP).ceil() as usize).clamp(1, MAX_TERRACES);
    let mut out = Vec::with_capacity(count + 1);
    for i in 0..count {
        let (h0, h1) = (
            rise * i as f64 / count as f64,
            rise * (i + 1) as f64 / count as f64,
        );
        let width = tool.radius_at_height((h0 + h1) / 2.0);
        if width > 1e-6 {
            out.push((depth - h0, width));
        }
    }
    if depth > rise + 1e-6 {
        out.push((depth - rise, tool.radius()));
    }
    out
}

/// The heights (job Z, from the underside up to the top) between which a
/// sloped tool whose tip reaches `tip` cuts a through kerf of one width:
/// terraces each [`TERRACE_STEP`] wider than the one below (at most
/// [`MAX_THROUGH_TERRACES`]) up to its shoulder, then one to the top.
fn through_levels(tool: &ToolGeometry, tip: f64, bottom: f64, top: f64) -> Vec<f64> {
    let (from, to) = ((bottom - tip).max(0.0), top - tip);
    let shoulder = tool.shoulder_height().clamp(from, to);
    let grow = tool.radius_at_height(shoulder) - tool.radius_at_height(from);
    let mut levels = vec![bottom];
    if shoulder > from + 1e-6 {
        let count = ((grow / TERRACE_STEP).ceil() as usize).clamp(1, MAX_THROUGH_TERRACES);
        for i in 1..=count {
            levels.push(tip + from + (shoulder - from) * i as f64 / count as f64);
        }
    }
    if top > levels[levels.len() - 1] + 1e-6 {
        levels.push(top);
    }
    levels
}

/// A tapered wall's cut as the exact solids see it: at a depth `d` from
/// `top` down to `bottom` (mm below the material's top) it is `kerf` swept
/// over `(bottom - d) x shift`, so its walls are planes leaning with the
/// taper; below `bottom` it is `kerf` itself.
#[derive(Debug, Clone)]
pub struct SlopeRecess {
    /// The cut at `bottom` and below, down to its foot.
    pub kerf: Shapes,
    /// How far the cut moves in X and Y for each millimetre it rises.
    pub shift: [f64; 2],
    /// Depth the wall starts at.
    pub top: f64,
    /// Depth its sloped part ends at.
    pub bottom: f64,
}

impl SlopeRecess {
    /// The cut at `depth`.
    pub fn at(&self, depth: f64) -> Shapes {
        let rise = self.bottom - depth.clamp(self.top, self.bottom);
        sweep(&self.kerf, [self.shift[0] * rise, self.shift[1] * rise])
    }

    /// The same cut moved by `by`.
    fn moved(&self, by: [f64; 2]) -> SlopeRecess {
        SlopeRecess {
            kerf: self
                .kerf
                .iter()
                .map(|shape| {
                    shape
                        .iter()
                        .map(|ring| ring.iter().map(|p| [p[0] + by[0], p[1] + by[1]]).collect())
                        .collect()
                })
                .collect(),
            ..self.clone()
        }
    }

    /// The box its cut reaches over, min X, min Y, max X, max Y.
    fn reach(&self) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let rise = self.bottom - self.top;
        let far = [self.shift[0] * rise, self.shift[1] * rise];
        for p in self.kerf.iter().flatten().flatten() {
            for q in [*p, [p[0] + far[0], p[1] + far[1]]] {
                b = [
                    b[0].min(q[0]),
                    b[1].min(q[1]),
                    b[2].max(q[0]),
                    b[3].max(q[1]),
                ];
            }
        }
        b
    }
}

/// `shapes` swept along the straight move `by` (their Minkowski sum with
/// that segment): the shapes at both ends and every edge's parallelogram.
pub fn sweep(shapes: &Shapes, by: [f64; 2]) -> Shapes {
    if by[0].hypot(by[1]) < 1e-9 || shapes.is_empty() {
        return shapes.clone();
    }
    let moved: Shapes = shapes
        .iter()
        .map(|shape| {
            shape
                .iter()
                .map(|ring| ring.iter().map(|p| [p[0] + by[0], p[1] + by[1]]).collect())
                .collect()
        })
        .collect();
    let ends =
        tidy(contours(shapes).overlay(&contours(&moved), OverlayRule::Union, FillRule::NonZero));
    let mut quads: Vec<Contour> = Vec::new();
    for ring in shapes.iter().flatten() {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let mut quad = vec![
                a,
                b,
                [b[0] + by[0], b[1] + by[1]],
                [a[0] + by[0], a[1] + by[1]],
            ];
            let area = ring_area(&quad);
            if area.abs() < 1e-12 {
                continue;
            }
            if area < 0.0 {
                quad.reverse();
            }
            quads.push(quad);
        }
    }
    if quads.is_empty() {
        return ends;
    }
    tidy(contours(&ends).overlay(&quads, OverlayRule::Union, FillRule::NonZero))
}

/// The exact cuts of a tapered profile cut with an end mill: the rings it
/// parts the material with (a wall cut right through), the recesses its
/// walls leave below their slope (depth, rings), and the slopes. `None`
/// when it has walls that are not kept exact, or they do not match its
/// moves (the moves were changed after).
#[allow(clippy::type_complexity)]
fn slopes_of(
    toolpath: &Toolpath,
    radius: f64,
    moves_box: [f64; 4],
    top: f64,
    bottom: f64,
) -> Option<(Vec<Contour>, Vec<(f64, Vec<Contour>)>, Vec<SlopeRecess>)> {
    if toolpath.slopes.is_empty() {
        return None;
    }
    let thickness = top - bottom;
    let mut through = Vec::new();
    let mut shallow = Vec::new();
    let mut found = Vec::new();
    for slope in &toolpath.slopes {
        let start = slope.moves.first()?.to;
        let foot: Vec<Seg> = segments_of(start, &slope.moves);
        if foot.is_empty() {
            return None;
        }
        for s in &foot {
            for p in [s.a, s.b] {
                let inside = p[0] - radius >= moves_box[0] - 1e-3
                    && p[1] - radius >= moves_box[1] - 1e-3
                    && p[0] + radius <= moves_box[2] + 1e-3
                    && p[1] + radius <= moves_box[3] + 1e-3;
                if !inside {
                    return None;
                }
            }
        }
        let rings: Vec<Contour> = foot.iter().map(|s| capsule(s, radius)).collect();
        let kerf = union_of(&rings);
        let settle = slope.cut - slope.bottom;
        let last = sweep(&kerf, [slope.shift[0] * settle, slope.shift[1] * settle]);
        if slope.cut >= thickness - 1e-4 {
            through.extend(contours(&last));
        } else {
            shallow.push((slope.cut, contours(&last)));
        }
        if slope.bottom > slope.top + 1e-6 {
            found.push(SlopeRecess {
                kerf: last,
                shift: slope.shift,
                top: slope.top,
                bottom: slope.bottom,
            });
        }
    }
    Some((through, shallow, found))
}

/// A sheet's cuts, exactly: its material as cut through, the recesses sunk
/// into it, and where cuts that cannot be made exact run.
#[derive(Debug, Clone)]
pub struct SheetExact {
    /// The material standing once every through cut is made, job X and Y.
    pub material: Shapes,
    /// Areas sunk from the top and how deep, mm.
    pub recesses: Vec<(Shapes, f64)>,
    /// Tapered walls: cuts that widen as they rise.
    pub slopes: Vec<SlopeRecess>,
    /// Boxes (min X, min Y, max X, max Y) where a deep cut that cannot be
    /// made exact runs.
    pub blocked: Vec<[f64; 4]>,
    /// The material's top, job Z.
    pub top: f64,
    /// The material's underside, job Z.
    pub bottom: f64,
}

/// The exact cuts of `sheet`'s visible toolpaths.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown sheet.
pub fn sheet_exact(document: &Document, sheet: u64) -> Result<SheetExact, CoreError> {
    let material = document.sheet_material(sheet)?;
    let job = material.applied_to(document.job_setup());
    let (datum_x, datum_y) = job.datum();
    let top = job.surface_z();
    let bottom = top - job.thickness;
    let thickness = job.thickness;
    let (x0, y0) = (-datum_x, -datum_y);
    let sheet_ring: Contour = vec![
        [x0, y0],
        [x0 + job.width, y0],
        [x0 + job.width, y0 + job.height],
        [x0, y0 + job.height],
    ];
    let mut kerf: Vec<Contour> = Vec::new();
    let mut sunk: Vec<(f64, Vec<Contour>)> = Vec::new();
    let mut slopes: Vec<SlopeRecess> = Vec::new();
    let mut blocked = Vec::new();
    let mut total = 0usize;
    for id in document.toolpath_order() {
        let Ok(toolpath) = document.toolpath(*id) else {
            continue;
        };
        if !toolpath.visible || (toolpath.sheet != sheet && toolpath.sheet != 0) {
            continue;
        }
        let from_below = toolpath.side != Side::Top;
        if from_below && !job.is_double_sided() {
            continue;
        }
        let segs = segments(toolpath);
        let radius = toolpath.tool.radius();
        let cutting: Vec<Seg> = segs.into_iter().filter(|s| s.z < top - 1e-6).collect();
        let Some(deepest) = cutting.iter().map(|s| s.z).reduce(f64::min) else {
            continue;
        };
        let depth = top - deepest;
        let bounds = || {
            let mut b = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for s in &cutting {
                for p in [s.a, s.b] {
                    b = [
                        b[0].min(p[0] - radius),
                        b[1].min(p[1] - radius),
                        b[2].max(p[0] + radius),
                        b[3].max(p[1] + radius),
                    ];
                }
            }
            b
        };
        total += cutting.len();
        let straight = upright(&toolpath.tool.tool_type);
        let tapered = toolpath.params_json["params"]["wallAngle"]
            .as_f64()
            .is_some_and(|a| a != 0.0);
        if tapered && straight && !from_below {
            // Stepped with an end mill: exact as the slopes it keeps, else
            // not at all.
            let bb = bounds();
            match slopes_of(toolpath, radius, bb, top, bottom) {
                Some((through_rings, shallow, found)) if total <= MAX_SEGMENTS => {
                    kerf.extend(through_rings);
                    for (d, rings) in shallow {
                        match sunk
                            .iter_mut()
                            .find(|(at, _)| (*at - d).abs() < DEPTH_STEP / 2.0)
                        {
                            Some((_, all)) => all.extend(rings),
                            None => sunk.push((d, rings)),
                        }
                    }
                    slopes.extend(found);
                }
                _ => {
                    if depth > thickness * DEEP_SHARE {
                        blocked.push(bb);
                    }
                }
            }
            continue;
        }
        let through = deepest <= bottom + 1e-4;
        let exactable = !from_below && total <= MAX_SEGMENTS;
        if !exactable {
            if depth > thickness * DEEP_SHARE {
                blocked.push(bounds());
            }
            continue;
        }
        if through {
            let tab_line = top - thickness * TAB_SHARE;
            let parting: Vec<&Seg> = cutting.iter().filter(|s| s.z <= tab_line).collect();
            if straight {
                kerf.extend(parting.iter().map(|s| capsule(s, radius)));
                continue;
            }
            let tool = &toolpath.tool.tool_type;
            let levels = through_levels(tool, deepest, bottom, top);
            let width = |s: &Seg, w: &[f64]| {
                tool.radius_at_height((w[0] + w[1]) / 2.0 - s.z)
                    .max(MIN_KERF)
            };
            let lowest = &levels[..2];
            kerf.extend(parting.iter().map(|s| capsule(s, width(s, lowest))));
            for w in levels.windows(2).skip(1) {
                let rings: Vec<Contour> = parting
                    .iter()
                    .filter(|s| width(s, w) > width(s, lowest) + 1e-9)
                    .map(|s| capsule(s, width(s, w)))
                    .collect();
                if rings.is_empty() {
                    continue;
                }
                let d = top - w[0];
                match sunk
                    .iter_mut()
                    .find(|(at, _)| (*at - d).abs() < DEPTH_STEP / 2.0)
                {
                    Some((_, all)) => all.extend(rings),
                    None => sunk.push((d, rings)),
                }
            }
            continue;
        }
        let mut tips: Vec<(f64, Vec<&Seg>)> = Vec::new();
        for s in &cutting {
            let d = ((top - s.z) / DEPTH_STEP).round() * DEPTH_STEP;
            match tips
                .iter_mut()
                .find(|(k, _)| (*k - d).abs() < DEPTH_STEP / 2.0)
            {
                Some((_, segs)) => segs.push(s),
                None => tips.push((d, vec![s])),
            }
        }
        let mut depths: Vec<(f64, Vec<Contour>)> = Vec::new();
        for (d, segs) in &tips {
            for (at, width) in terraces(&toolpath.tool.tool_type, *d) {
                let rings: Vec<Contour> = segs.iter().map(|s| capsule(s, width)).collect();
                match depths
                    .iter_mut()
                    .find(|(k, _)| (*k - at).abs() < DEPTH_STEP / 2.0)
                {
                    Some((_, all)) => all.extend(rings),
                    None => depths.push((at, rings)),
                }
            }
        }
        if depths.len() > MAX_DEPTHS {
            if depth > thickness * DEEP_SHARE {
                blocked.push(bounds());
            }
            continue;
        }
        for (d, rings) in depths {
            match sunk
                .iter_mut()
                .find(|(k, _)| (*k - d).abs() < DEPTH_STEP / 2.0)
            {
                Some((_, all)) => all.extend(rings),
                None => sunk.push((d, rings)),
            }
        }
    }
    let material = if kerf.is_empty() {
        vec![vec![sheet_ring]]
    } else {
        tidy(vec![sheet_ring].overlay(&kerf, OverlayRule::Difference, FillRule::NonZero))
    };
    sunk.sort_by(|a, b| a.0.total_cmp(&b.0));
    let recesses = sunk
        .into_iter()
        .map(|(d, rings)| (union_of(&rings), d))
        .filter(|(shapes, _)| !shapes.is_empty())
        .collect();
    Ok(SheetExact {
        material,
        recesses,
        slopes,
        blocked,
        top,
        bottom,
    })
}

/// Whether a point lies inside a set of shapes (on the boundary counts as
/// either).
pub fn contains(shapes: &Shapes, p: [f64; 2]) -> bool {
    let mut inside = false;
    for ring in shapes.iter().flat_map(|s| s.iter()) {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if x > p[0] {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// Distance from a point to the nearest edge of a set of shapes.
pub fn edge_distance(shapes: &Shapes, p: [f64; 2]) -> f64 {
    let mut best = f64::INFINITY;
    for ring in shapes.iter().flat_map(|s| s.iter()) {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let len2 = ex * ex + ey * ey;
            let t = if len2 > 0.0 {
                (((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = (p[0] - a[0] - t * ex).hypot(p[1] - a[1] - t * ey);
            best = best.min(d);
        }
    }
    best
}

/// A point well inside a shape: the middle of the widest span a level
/// line through it crosses, or `None` for a sliver.
fn inner_point(shape: &Shape) -> Option<[f64; 2]> {
    let outer = shape.first()?;
    let (lo, hi) = outer
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    let mut best: Option<(f64, [f64; 2])> = None;
    for k in 1..8 {
        let y = lo + (hi - lo) * f64::from(k) / 8.0 + (hi - lo) * 1e-7;
        let mut xs: Vec<f64> = Vec::new();
        for ring in shape {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            let width = pair[1] - pair[0];
            if best.is_none_or(|(w, _)| width > w) {
                best = Some((width, [(pair[0] + pair[1]) / 2.0, y]));
            }
        }
    }
    best.filter(|(w, _)| *w > 0.0).map(|(_, p)| p)
}

/// A piece's exact solid in its own frame: X and Y from its part box's
/// lower left corner, Z up from the material's underside, mm.
///
/// Under a tapered wall the slabs are thin terraces, each as its section is
/// at its bottom (where the cut is narrowest, so a terrace never stands
/// short of the material), and the wall itself is kept as flat leaning
/// faces for measuring fits: each edge of the section that the taper moves,
/// from where it is at the bottom of the wall to where it is at the top.
#[derive(Debug, Clone)]
pub struct ExactSolid {
    /// Material thickness, mm.
    pub thickness: f64,
    /// Heights where the section changes, from 0 up to the thickness.
    pub levels: Vec<f64>,
    /// The section of each slab between two levels (`levels.len() - 1`).
    pub sections: Vec<Shapes>,
    /// Which slabs are terraces under a tapered wall.
    pub terraced: Vec<bool>,
    /// The tapered walls' leaning faces.
    pub leaning: Vec<Leaning>,
}

/// A flat leaning face of a tapered wall: an edge of the section at the
/// bottom height and the same edge, moved, at the top one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaning {
    /// The edge at `z[0]`, material on its left.
    pub bottom: [[f64; 2]; 2],
    /// The edge at `z[1]`.
    pub top: [[f64; 2]; 2],
    /// The heights, mm up from the underside.
    pub z: [f64; 2],
}

/// Most a terrace under a tapered wall stands out of the slope, mm.
pub const TERRACE_LEAN: f64 = 0.02;

/// Most terraces one slab under a tapered wall is cut into.
const MAX_LEANING_TERRACES: usize = 400;

/// The edges of `bottom` (at height `z0`) that `moves` carries to edges of
/// `top` (at `z1`): each the same direction, on the line moved by one of
/// the moves, overlapping it once moved.
fn leaning_edges(bottom: &Shapes, top: &Shapes, moves: &[[f64; 2]], z: [f64; 2]) -> Vec<Leaning> {
    let edges = |shapes: &Shapes| -> Vec<[[f64; 2]; 2]> {
        shapes
            .iter()
            .flatten()
            .flat_map(|ring| {
                let n = ring.len();
                (0..n).map(move |i| [ring[i], ring[(i + 1) % n]])
            })
            .filter(|[a, b]| (b[0] - a[0]).hypot(b[1] - a[1]) > 1e-6)
            .collect()
    };
    let above = edges(top);
    let mut out = Vec::new();
    for [a, b] in edges(bottom) {
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let e = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
        let n = [e[1], -e[0]];
        let mut best: Option<(f64, [[f64; 2]; 2])> = None;
        for d in moves {
            let shift = d[0] * n[0] + d[1] * n[1];
            if shift.abs() < 1e-6 {
                continue;
            }
            for [p, q] in &above {
                let l = (q[0] - p[0]).hypot(q[1] - p[1]);
                let f = [(q[0] - p[0]) / l, (q[1] - p[1]) / l];
                if e[0] * f[0] + e[1] * f[1] < 1.0 - 1e-9 {
                    continue;
                }
                let across = (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                if (across - shift).abs() > 1e-5 {
                    continue;
                }
                // Overlap along the edge once the bottom edge is moved.
                let along = |v: [f64; 2]| (v[0] - a[0] - d[0]) * e[0] + (v[1] - a[1] - d[1]) * e[1];
                let (lo, hi) = (along(*p).max(0.0), along(*q).min(len));
                let overlap = hi - lo;
                if overlap > 1e-6 && best.is_none_or(|(o, _)| overlap > o) {
                    best = Some((overlap, [*p, *q]));
                }
            }
        }
        if let Some((_, top)) = best {
            out.push(Leaning {
                bottom: [a, b],
                top,
                z,
            });
        }
    }
    out
}

impl ExactSolid {
    /// Builds the slabs of an outline sunk by `recesses` (area, depth from
    /// the top), all in the piece's frame.
    pub fn new(outline: Shapes, recesses: &[(Shapes, f64)], thickness: f64) -> ExactSolid {
        ExactSolid::with_slopes(outline, recesses, &[], thickness)
    }

    /// Builds the slabs of an outline sunk by `recesses` and by tapered
    /// cuts `slopes`, all in the piece's frame.
    pub fn with_slopes(
        outline: Shapes,
        recesses: &[(Shapes, f64)],
        slopes: &[SlopeRecess],
        thickness: f64,
    ) -> ExactSolid {
        let mut marks = vec![0.0, thickness];
        let depths = recesses
            .iter()
            .map(|(_, d)| *d)
            .chain(slopes.iter().flat_map(|s| [s.top, s.bottom]));
        for depth in depths {
            let z = thickness - depth;
            if z > 1e-6 && z < thickness - 1e-6 && !marks.iter().any(|l| (l - z).abs() < 1e-6) {
                marks.push(z);
            }
        }
        marks.sort_by(f64::total_cmp);
        let mut levels = vec![0.0];
        let mut sections = Vec::new();
        let mut terraced = Vec::new();
        let mut leaning = Vec::new();
        for w in marks.windows(2) {
            let cut: Shapes = recesses
                .iter()
                .filter(|(_, depth)| thickness - depth <= w[0] + 1e-6)
                .flat_map(|(s, _)| s.iter().cloned())
                .collect();
            let active: Vec<&SlopeRecess> = slopes
                .iter()
                .filter(|s| thickness - s.bottom <= w[0] + 1e-6 && thickness - s.top >= w[1] - 1e-6)
                .collect();
            if active.is_empty() {
                levels.push(w[1]);
                sections.push(difference(&outline, &cut));
                terraced.push(false);
                continue;
            }
            let section = |z: f64| -> Shapes {
                let mut all = cut.clone();
                for s in &active {
                    all.extend(s.at(thickness - z));
                }
                difference(&outline, &all)
            };
            let lean = active
                .iter()
                .map(|s| s.shift[0].hypot(s.shift[1]))
                .fold(0.0, f64::max);
            let count = ((lean * (w[1] - w[0]) / TERRACE_LEAN).ceil() as usize)
                .clamp(1, MAX_LEANING_TERRACES);
            let bottom = section(w[0]);
            let top = section(w[1]);
            let rise = w[1] - w[0];
            let moves: Vec<[f64; 2]> = active
                .iter()
                .map(|s| [s.shift[0] * rise, s.shift[1] * rise])
                .collect();
            leaning.extend(leaning_edges(&bottom, &top, &moves, [w[0], w[1]]));
            for i in 0..count {
                let z1 = w[0] + rise * (i + 1) as f64 / count as f64;
                let z0 = w[0] + rise * i as f64 / count as f64;
                levels.push(if i + 1 == count { w[1] } else { z1 });
                sections.push(if i == 0 { bottom.clone() } else { section(z0) });
                terraced.push(true);
            }
        }
        ExactSolid {
            thickness,
            levels,
            sections,
            terraced,
            leaning,
        }
    }

    /// The slab a height lies in, if inside the material's thickness.
    fn slab(&self, z: f64) -> Option<usize> {
        if z <= 0.0 || z >= self.thickness {
            return None;
        }
        self.levels.windows(2).position(|w| z >= w[0] && z < w[1])
    }

    /// Whether a point in the solid's frame lies inside its material.
    pub fn holds(&self, p: [f64; 3]) -> bool {
        self.slab(p[2])
            .is_some_and(|k| contains(&self.sections[k], [p[0], p[1]]))
    }

    /// How deep a point lies inside the material: the distance to the
    /// nearest wall of its slab, the underside or the top above it; `None`
    /// outside.
    pub fn depth_inside(&self, p: [f64; 3]) -> Option<f64> {
        let k = self.slab(p[2])?;
        let section = &self.sections[k];
        let xy = [p[0], p[1]];
        if !contains(section, xy) {
            return None;
        }
        // The top over this point: up through the slabs while it stays inside.
        let mut top = self.levels[k + 1];
        for (j, s) in self.sections.iter().enumerate().skip(k + 1) {
            if contains(s, xy) {
                top = self.levels[j + 1];
            } else {
                break;
            }
        }
        Some(edge_distance(section, xy).min(p[2]).min(top - p[2]))
    }

    /// The box round the solid: min X, min Y, max X, max Y.
    pub fn bounds(&self) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in self
            .sections
            .iter()
            .flat_map(|s| s.iter())
            .flat_map(|shape| shape.first())
            .flat_map(|ring| ring.iter())
        {
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ];
        }
        b
    }
}

/// Part `local` of a sheet as an exact solid in the piece's frame, or
/// `None` when no exact edge exists for it (a cut that cannot be made
/// exact runs over it, or its material does not show up in the exact cut).
pub fn piece_solid(exact: &SheetExact, parts: &SheetParts, local: u32) -> Option<ExactSolid> {
    let jb = parts.job_box(local)?;
    let meets = |b: &[f64; 4]| b[0] < jb[2] && jb[0] < b[2] && b[1] < jb[3] && jb[1] < b[3];
    if exact.blocked.iter().any(meets) {
        return None;
    }
    // The grid counts material a through cut leaves thinner than half the
    // sheet as cut (a tab), so a shape is matched to its part at a point
    // where it stands thicker: clear of the recesses that deep (a sloped
    // through cut's wider kerf above), else anywhere in it.
    let thick_line = (exact.top - exact.bottom) * TAB_SHARE;
    let deep: Shapes = exact
        .recesses
        .iter()
        .filter(|(_, depth)| *depth >= thick_line)
        .flat_map(|(shapes, _)| shapes.iter().cloned())
        .collect();
    let probe = |shape: &Shape| {
        let whole = vec![shape.clone()];
        let standing = if deep.is_empty() {
            Vec::new()
        } else {
            difference(&whole, &deep)
        };
        standing
            .iter()
            .filter_map(inner_point)
            .next()
            .or_else(|| inner_point(shape))
    };
    let mine: Shapes = exact
        .material
        .iter()
        .filter(|shape| probe(shape).is_some_and(|p| parts.part_at(p[0], p[1]) == local))
        .cloned()
        .collect();
    if mine.is_empty() {
        return None;
    }
    let shift = |shapes: &Shapes| -> Shapes {
        shapes
            .iter()
            .map(|shape| {
                shape
                    .iter()
                    .map(|ring| ring.iter().map(|p| [p[0] - jb[0], p[1] - jb[1]]).collect())
                    .collect()
            })
            .collect()
    };
    let recesses: Vec<(Shapes, f64)> = exact
        .recesses
        .iter()
        .map(|(shapes, depth)| (shift(&intersection(shapes, &mine)), *depth))
        .filter(|(shapes, _)| !shapes.is_empty())
        .collect();
    let slopes: Vec<SlopeRecess> = exact
        .slopes
        .iter()
        .filter(|s| meets(&s.reach()))
        .map(|s| s.moved([-jb[0], -jb[1]]))
        .collect();
    // A part set thinner (a key cut from thinner stock) keeps its own.
    Some(ExactSolid::with_slopes(
        shift(&mine),
        &recesses,
        &slopes,
        parts.thickness(local).min(exact.top - exact.bottom),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Shapes {
        vec![vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]]]
    }

    #[test]
    fn a_capsule_keeps_its_long_sides_exactly_the_radius_off() {
        let seg = Seg {
            a: [0.0, 0.0],
            b: [10.0, 0.0],
            z: 0.0,
        };
        let ring = capsule(&seg, 3.175);
        let lo = ring.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let hi = ring.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        assert!((lo + 3.175).abs() < 1e-12 && (hi - 3.175).abs() < 1e-12);
        let area = ring_area(&ring);
        let want = 20.0 * 3.175 + std::f64::consts::PI * 3.175 * 3.175;
        // Flattened arcs lose about two thirds of their stray times their length.
        assert!((area - want).abs() < 0.02, "{area} vs {want}");
    }

    #[test]
    fn a_tapered_cut_is_terraced_never_short_and_keeps_its_leaning_faces() {
        // A 10 x 10 cut through the middle of a 40 x 40 x 12 piece, its
        // left wall leaning back (to -X) by tan 14 degrees per mm it rises.
        let tan = 14f64.to_radians().tan();
        let kerf = square(10.0, 10.0, 20.0, 20.0);
        assert!((shapes_area(&sweep(&kerf, [-2.0, 0.0])) - 120.0).abs() < 1e-6);
        let slope = SlopeRecess {
            kerf,
            shift: [-tan, 0.0],
            top: 0.0,
            bottom: 12.0,
        };
        let solid = ExactSolid::with_slopes(square(0.0, 0.0, 40.0, 40.0), &[], &[slope], 12.0);
        // Terraces no more than TERRACE_LEAN out of the slope.
        let slabs = solid.levels.len() - 1;
        assert!(slabs as f64 >= 12.0 * tan / TERRACE_LEAN, "{slabs}");
        for k in 0..slabs {
            let (z0, z1) = (solid.levels[k], solid.levels[k + 1]);
            let wall = 10.0 - z0 * tan;
            // Never short of the material: the wall at the slab's bottom.
            assert!(solid.holds([wall - 1e-6, 15.0, (z0 + z1) / 2.0]));
            assert!(!solid.holds([wall + 1e-6, 15.0, (z0 + z1) / 2.0]));
            assert!((z1 - z0) * tan <= TERRACE_LEAN + 1e-9);
        }
        // One leaning face for the left wall, from x 10 at the underside to
        // 10 - 12 tan 14 at the top.
        let lean: Vec<&Leaning> = solid
            .leaning
            .iter()
            .filter(|l| {
                (l.bottom[0][0] - 10.0).abs() < 1e-9 && (l.bottom[1][0] - 10.0).abs() < 1e-9
            })
            .collect();
        assert_eq!(lean.len(), 1, "{:?}", solid.leaning);
        assert!((lean[0].top[0][0] - (10.0 - 12.0 * tan)).abs() < 1e-6);
        assert_eq!(lean[0].z, [0.0, 12.0]);
    }

    #[test]
    fn slabs_follow_the_recesses_and_depth_is_to_the_nearest_surface() {
        let solid = ExactSolid::new(
            square(0.0, 0.0, 100.0, 50.0),
            &[(square(10.0, 10.0, 30.0, 30.0), 4.0)],
            18.0,
        );
        assert_eq!(solid.levels, vec![0.0, 14.0, 18.0]);
        assert!((shapes_area(&solid.sections[0]) - 5000.0).abs() < 1e-6);
        assert!((shapes_area(&solid.sections[1]) - 4600.0).abs() < 1e-6);
        // Under the pocket floor, 1 mm down.
        let d = solid.depth_inside([20.0, 20.0, 13.0]).expect("inside");
        assert!((d - 1.0).abs() < 1e-9, "{d}");
        assert!(solid.depth_inside([20.0, 20.0, 15.0]).is_none());
        let wall = solid.depth_inside([0.5, 25.0, 9.0]).expect("inside");
        assert!((wall - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_inner_point_lies_inside_a_ring_with_a_hole() {
        let mut shape = square(0.0, 0.0, 40.0, 40.0).remove(0);
        shape.push(vec![[10.0, 10.0], [10.0, 30.0], [30.0, 30.0], [30.0, 10.0]]);
        let p = inner_point(&shape).expect("a point");
        assert!(contains(&vec![shape], p));
    }

    #[test]
    fn a_v_bit_sinks_terraces_that_follow_its_cone() {
        let tool = ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 90.0,
        };
        let steps = terraces(&tool, 3.0);
        // Grows 3 mm in radius over 3 mm of height: 12 terraces of 0.25 mm.
        assert_eq!(steps.len(), 12, "{steps:?}");
        let (deepest, narrow) = steps[0];
        assert!((deepest - 3.0).abs() < 1e-9 && (narrow - 0.125).abs() < 1e-9);
        let (shallow, wide) = steps[11];
        assert!((shallow - 0.25).abs() < 1e-9 && (wide - 2.875).abs() < 1e-9);
        // Deeper than a ball nose's radius, full width above its shoulder.
        let ball = ToolGeometry::BallNose { diameter: 6.0 };
        let steps = terraces(&ball, 5.0);
        let (top, full) = steps[steps.len() - 1];
        assert!(
            (top - 2.0).abs() < 1e-9 && (full - 3.0).abs() < 1e-9,
            "{steps:?}"
        );
        assert_eq!(
            terraces(&ToolGeometry::EndMill { diameter: 6.0 }, 4.0),
            vec![(4.0, 3.0)]
        );
        // Through 10 mm with the tip 1 mm under: terraces up to the ball's
        // shoulder 2 mm over the underside, then its full width to the top.
        let levels = through_levels(&ball, -11.0, -10.0, 0.0);
        let n = levels.len();
        assert!(n > 4, "{levels:?}");
        assert!((levels[0] + 10.0).abs() < 1e-9 && levels[n - 1].abs() < 1e-9);
        assert!((levels[n - 2] + 8.0).abs() < 1e-9, "{levels:?}");
        assert!(levels.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn a_v_bit_through_cut_keeps_its_sloped_edge() {
        use super::super::pieces::tests::{add, document};
        use super::super::pieces::{self, SheetParts};
        use crate::cam::tool::Tool;
        use crate::cam::toolpath::{Move, StrategyKind};
        use crate::cam::Point3;
        let mut doc = document();
        let sheet = doc.active_sheet();
        let mut tp = Toolpath::new(
            0,
            "v cut out",
            crate::cam::tool::Tool {
                name: "90 V".into(),
                tool_type: ToolGeometry::VBit {
                    diameter: 25.0,
                    included_angle: 90.0,
                },
                ..Tool::default()
            },
            StrategyKind::Profile,
        );
        tp.sheet = sheet;
        tp.home = Point3::new(0.0, 0.0, 20.0);
        // The tip round (20, 10)-(80, 40), 1 mm under the 10 mm material.
        let corners = [
            [20.0, 10.0],
            [80.0, 10.0],
            [80.0, 40.0],
            [20.0, 40.0],
            [20.0, 10.0],
        ];
        tp.moves = vec![
            Move::rapid(Point3::new(20.0, 10.0, 5.0)),
            Move::plunge(Point3::new(20.0, 10.0, -11.0)),
        ];
        tp.moves.extend(
            corners[1..]
                .iter()
                .map(|c| Move::feed(Point3::new(c[0], c[1], -11.0))),
        );
        tp.moves.push(Move::rapid(Point3::new(20.0, 10.0, 5.0)));
        add(&mut doc, tp);
        let exact = sheet_exact(&doc, sheet).expect("exact");
        assert!(exact.blocked.is_empty(), "{:?}", exact.blocked);
        let parts = SheetParts::find(sheet, pieces::simulate(&doc, sheet, 20_000.0).expect("sim"));
        let local = parts.part_at(50.0, 25.0);
        assert!(local > 0);
        let solid = piece_solid(&exact, &parts, local).expect("an exact solid");
        let span = |k: usize| {
            let mut b = [f64::INFINITY, f64::NEG_INFINITY];
            for p in solid.sections[k].iter().flat_map(|s| s.iter()).flatten() {
                b = [b[0].min(p[0]), b[1].max(p[0])];
            }
            b[1] - b[0]
        };
        // The cone is 1 mm wide each side at the underside and 11 mm at the
        // top: the part is 58 mm across at the bottom and 38 mm at the top,
        // the terraces following the cone between.
        let last = solid.sections.len() - 1;
        assert!(last >= 40, "{} slabs", last + 1);
        assert!((span(0) - 58.0).abs() < 0.3, "{}", span(0));
        assert!((span(last) - 38.0).abs() < 0.3, "{}", span(last));
        for k in 0..=last {
            let z = (solid.levels[k] + solid.levels[k + 1]) / 2.0;
            let want = 60.0 - 2.0 * (z + 1.0);
            assert!(
                (span(k) - want).abs() < 0.3,
                "slab {k}: {} vs {want}",
                span(k)
            );
        }
    }
}
