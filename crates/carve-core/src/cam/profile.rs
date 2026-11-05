//! Profile toolpaths: cut along vectors, outside, inside or on the line, in
//! passes down to the cut depth, with optional tabs, ramps, leads and a
//! separate last pass.
//!
//! Closed vectors are treated as shapes: a loop inside another is a hole, so
//! an outside profile of a letter O cuts round the outside of the O and the
//! inside of its counter, leaving the letter. Direction follows the milling
//! rule for a clockwise spindle: climb keeps the finished wall on the right
//! of travel.
//!
//! Parts are cut one at a time, holes before the outline that frees the
//! part, and parts lying inside the holes of other parts before those.

use serde::{Deserialize, Serialize};

use crate::doc::ops::Anchor;
use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{
    parallel_offset_open, subpath_signed_area, BBox, Path, Point, Region, Tolerance,
};

use super::chain::{Chain, CutSeg};
use super::common::{
    end_tangent, order_nearest, planned_depths, restart_nearest, sharpen_corners, start_tangent,
    Emitter, Heights, LeadSpec, PassPlan, RampSpec, TabSpec,
};
use super::tool::Tool;
use super::toolpath::Move;

/// Which side of the vector the tool cuts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileSide {
    /// Keeps what is inside the vectors.
    #[default]
    Outside,
    /// Keeps what is outside the vectors.
    Inside,
    /// Tool centre on the vector.
    On,
}

/// Which side of an open vector the tool runs on, looking along the
/// vector's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenSide {
    /// Tool centre on the vector.
    #[default]
    On,
    /// Tool edge on the vector, the tool to its left.
    Left,
    /// Tool edge on the vector, the tool to its right.
    Right,
}

/// Climb or conventional milling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutDirection {
    /// Finished wall on the right of travel.
    #[default]
    Climb,
    /// Finished wall on the left of travel.
    Conventional,
}

/// The order parts are machined in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutOrder {
    /// Nearest next, to keep travel short.
    #[default]
    Optimized,
    /// The order the vectors were selected in.
    SelectionOrder,
    /// Left to right by each part's left edge.
    LeftToRight,
    /// Bottom to top by each part's bottom edge.
    BottomToTop,
    /// Rows the height of the tallest part, bottom to top, each left to
    /// right.
    Grid,
}

/// Where each closed cut starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartAt {
    /// Wherever suits: the loop's own start, or with leads the middle of
    /// its longest span.
    #[default]
    Optimized,
    /// Next to the start point of the vector the cut follows.
    VectorStart,
    /// The span end nearest the chosen point of the cut's bounding box
    /// (Closest on Bounding Box, `start_anchor`).
    BoundingBox,
}

/// A final thin pass cut separately, at full depth, after the others.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LastPass {
    /// Whether the last pass is cut separately.
    pub enabled: bool,
    /// Thickness of the skin the last pass removes, in millimetres. The
    /// other passes leave it on the wall.
    pub allowance: f64,
    /// Cut the last pass the other way round.
    pub reverse: bool,
}

impl Default for LastPass {
    fn default() -> Self {
        LastPass {
            enabled: false,
            allowance: 0.25,
            reverse: false,
        }
    }
}

/// The Profile form's settings. Depths are millimetres below the surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileParams {
    /// Depth the cut starts at.
    pub start_depth: f64,
    /// Depth the cut finishes at.
    pub cut_depth: f64,
    /// Depth of each pass; the tool's own pass depth when absent.
    pub pass_depth: Option<f64>,
    /// Evenly spaced passes, this many, instead of by the pass depth.
    pub pass_count: Option<usize>,
    /// Thickness of the last depth pass, in millimetres, when it is set
    /// apart from the others.
    pub last_pass_thickness: Option<f64>,
    /// Keep every pass exactly the pass depth instead of letting it grow by
    /// up to 15% to save a pass.
    pub exact_pass_depth: bool,
    /// Custom pass depths below the surface, in millimetres, from the Edit
    /// Passes dialog; they win over the other pass settings.
    pub pass_list: Option<Vec<f64>>,
    /// Outside, inside or on the vector.
    pub side: ProfileSide,
    /// Left, right or on, for open vectors.
    pub open_side: OpenSide,
    /// Climb or conventional.
    pub direction: CutDirection,
    /// Material left on the wall, in millimetres; negative cuts deeper in.
    pub allowance: f64,
    /// A separate thin finishing pass.
    pub last_pass: LastPass,
    /// Tabs on closed vectors.
    pub tabs: TabSpec,
    /// Tabs placed or edited in the Edit Tabs dialog, as points on the
    /// vectors; the spec's number or spacing places them when absent.
    pub tab_positions: Option<Vec<Point>>,
    /// Ramping into the cut.
    pub ramp: RampSpec,
    /// Leads into and out of closed vectors. When a lead is cut, each pass
    /// plunges at the lead's start instead of ramping.
    pub lead: LeadSpec,
    /// Order the parts are cut in, when `orders` is empty.
    pub order: CutOrder,
    /// The order options ticked on the Order page. Each is tried and the
    /// one with the least travel between parts is kept; empty uses `order`.
    pub orders: Vec<CutOrder>,
    /// Where closed cuts start.
    pub start_at: StartAt,
    /// The point of each cut's bounding box Closest on Bounding Box starts
    /// near.
    pub start_anchor: Anchor,
    /// Run out into sharp external corners instead of rolling round them.
    pub sharp_corners: bool,
    /// Run a V-bit's tip up into each internal corner so the corner comes
    /// out sharp (Sharp Internal Corner (3D)). Other tools ignore it.
    pub sharp_internal_corners: bool,
    /// How far the wall leans back into the material as it rises, degrees
    /// from upright; zero for an upright wall (see [`super::taper`]).
    pub wall_angle: f64,
    /// The most a step of a tapered wall cut with an end mill may stand out
    /// of the slope, millimetres.
    pub scallop: f64,
    /// An open vector's tapered wall moves as a whole, square to its
    /// longest straight span, so a wall it turns onto stays upright (the
    /// Joint Creator's socket walls).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rigid: bool,
}

impl Default for ProfileParams {
    fn default() -> Self {
        ProfileParams {
            start_depth: 0.0,
            cut_depth: 6.0,
            pass_depth: None,
            pass_count: None,
            last_pass_thickness: None,
            exact_pass_depth: false,
            pass_list: None,
            side: ProfileSide::Outside,
            open_side: OpenSide::On,
            direction: CutDirection::Climb,
            allowance: 0.0,
            last_pass: LastPass::default(),
            tabs: TabSpec::default(),
            tab_positions: None,
            ramp: RampSpec::default(),
            lead: LeadSpec::default(),
            order: CutOrder::Optimized,
            orders: Vec::new(),
            start_at: StartAt::Optimized,
            start_anchor: Anchor::BottomLeft,
            sharp_corners: false,
            sharp_internal_corners: false,
            wall_angle: 0.0,
            scallop: 0.02,
            rigid: false,
        }
    }
}

/// What a strategy produced: the moves and anything the user should know.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calculated {
    /// Machine moves from home back to home.
    pub moves: Vec<Move>,
    /// What the user should know, for the warnings list.
    pub warnings: Vec<crate::warning::Warning>,
}

/// One part's cuts: its holes, then the outline that frees it. Open
/// vectors make parts of their own with only holes.
#[derive(Debug, Clone, Default)]
struct Part {
    holes: Vec<Chain>,
    outline: Option<Chain>,
    /// The separate last pass round the same holes and outline.
    finish_holes: Vec<Chain>,
    finish_outline: Option<Chain>,
    /// Open vector cuts, cut back and forth.
    open: bool,
    /// How many other outlines surround this one.
    nesting: usize,
    /// Index of the selected vector the part comes from.
    source: usize,
    bbox: Option<BBox>,
}

impl Part {
    fn first(&self) -> Option<&Chain> {
        self.holes.first().or(self.outline.as_ref())
    }

    fn last(&self) -> Option<&Chain> {
        self.finish_outline
            .as_ref()
            .or(self.outline.as_ref())
            .or(self.finish_holes.last())
            .or(self.holes.last())
    }
}

/// Calculates a profile toolpath round `paths`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for depths that make no sense or when
/// no vector is left to cut.
pub fn profile(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &ProfileParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    if params.wall_angle != 0.0 {
        return super::taper::tapered(paths, job, tool, params, tol).map(|(done, _)| done);
    }
    let depths = planned_depths(
        params.start_depth,
        params.cut_depth,
        params.pass_depth.unwrap_or(tool.pass_depth),
        &PassPlan {
            count: params.pass_count,
            last_thickness: params.last_pass_thickness,
            exact: params.exact_pass_depth,
            list: params.pass_list.as_deref(),
        },
    )?;
    params.lead.validate()?;
    let skin = if params.last_pass.enabled {
        let skin = params.last_pass.allowance;
        if !skin.is_finite() || skin < 0.0 {
            return Err(CoreError::validation(
                "the last pass allowance must be zero or more",
            ));
        }
        skin
    } else {
        0.0
    };
    let heights = Heights::from_job(job);
    let mut warnings = Vec::new();

    let radius = tool.effective_radius_at_depth(-params.cut_depth);
    let region = Region::from_paths(paths, tol);
    let sign = match params.side {
        ProfileSide::Outside => -1.0,
        ProfileSide::Inside => 1.0,
        ProfileSide::On => 0.0,
    };
    let main_offset = sign * (radius + params.allowance + skin);
    let main_region = region.offset(main_offset, tol);
    if !region.is_empty() && main_region.is_empty() {
        warnings.push(crate::warning::Warning::tool_too_big());
    }
    let reverse_loops = match params.side {
        ProfileSide::Inside => params.direction == CutDirection::Conventional,
        ProfileSide::Outside | ProfileSide::On => params.direction == CutDirection::Climb,
    };
    let sharpen = params.sharp_corners && params.side != ProfileSide::On;
    let loops = |r: &Region, offset: f64, reverse: bool| -> Vec<(Chain, bool)> {
        r.loops
            .iter()
            .map(|sub| {
                let mut chain = Chain::from_subpath(sub, tol);
                if sharpen {
                    chain = sharpen_corners(&chain, offset.abs());
                }
                if reverse {
                    chain = chain.reversed();
                }
                (chain, subpath_signed_area(sub) < 0.0)
            })
            .collect()
    };
    let mut parts = parts_of(loops(&main_region, main_offset, reverse_loops));
    if params.last_pass.enabled {
        let finish_offset = sign * (radius + params.allowance);
        let finish_region = region.offset(finish_offset, tol);
        let reverse = reverse_loops != params.last_pass.reverse;
        let finish = parts_of(loops(&finish_region, finish_offset, reverse));
        attach_finish(&mut parts, finish);
    }

    // Open vectors, on the line or beside it.
    for (index, path) in paths.iter().enumerate() {
        for sub in path.subpaths.iter().filter(|s| !s.closed) {
            let tracks: Vec<Chain> = match params.open_side {
                OpenSide::On => vec![Chain::from_subpath(sub, tol)],
                side => {
                    let reach = radius + params.allowance;
                    if reach <= 1e-9 {
                        vec![Chain::from_subpath(sub, tol)]
                    } else {
                        parallel_offset_open(
                            &sub.clone().into_path(),
                            reach,
                            side == OpenSide::Left,
                            tol,
                        )?
                        .iter()
                        .flat_map(|p| p.subpaths.iter())
                        .map(|s| Chain::from_subpath(s, tol))
                        .collect()
                    }
                }
            };
            for chain in tracks.into_iter().filter(|c| !c.is_empty()) {
                parts.push(Part {
                    bbox: chain_bbox(&chain),
                    holes: vec![chain],
                    open: true,
                    nesting: usize::MAX,
                    source: index,
                    ..Part::default()
                });
            }
        }
    }
    if parts.iter().all(|p| p.first().is_none()) {
        return Err(CoreError::validation(
            "there is nothing to profile: select at least one vector the tool fits",
        ));
    }

    // Start points.
    let vector_starts: Vec<Point> = paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .filter(|s| s.closed)
        .map(|s| s.start)
        .collect();
    let place = |chain: Chain| -> Chain {
        if !chain.closed {
            return chain;
        }
        match params.start_at {
            StartAt::Optimized if params.lead.is_active() => start_mid_longest(&chain),
            StartAt::Optimized => chain,
            StartAt::VectorStart => {
                let at = chain.start().unwrap_or(Point::ORIGIN);
                match vector_starts
                    .iter()
                    .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
                {
                    Some(target) => restart_nearest(&chain, *target),
                    None => chain,
                }
            }
            StartAt::BoundingBox => match chain_bbox(&chain) {
                Some(b) => restart_at_node(&chain, params.start_anchor.on(b)),
                None => chain,
            },
        }
    };
    for part in &mut parts {
        part.holes = std::mem::take(&mut part.holes)
            .into_iter()
            .map(place)
            .collect();
        part.outline = part.outline.take().map(place);
        part.finish_holes = std::mem::take(&mut part.finish_holes)
            .into_iter()
            .map(place)
            .collect();
        part.finish_outline = part.finish_outline.take().map(place);
        let by_selection = if params.orders.is_empty() {
            params.order == CutOrder::SelectionOrder
        } else {
            params.orders.contains(&CutOrder::SelectionOrder)
        };
        if by_selection && !part.open {
            if let Some(first) = part.outline.as_ref().or(part.holes.first()) {
                part.source = nearest_source(paths, first);
            }
        }
    }
    let ordered = if params.orders.is_empty() {
        order_parts(parts, params.order, heights.home.xy())
    } else {
        fastest_order(parts, &params.orders, heights.home.xy())
    };

    // Climb keeps the finished wall on the right, so the waste is on the
    // left of travel; conventional the other way round.
    let waste_on_left = params.direction == CutDirection::Climb;
    let finish_waste_on_left = waste_on_left != params.last_pass.reverse;
    let tab_top = heights.z_at((params.cut_depth - params.tabs.thickness).max(params.start_depth));
    let last_depth = [params.cut_depth];
    let mut out = Emitter::new(heights);
    // How far a tab position on a vector may be from the cut beside it.
    let tab_reach = radius.abs() + params.allowance.abs() + params.last_pass.allowance.abs() + 1.0;
    let internal = if params.sharp_internal_corners && params.side != ProfileSide::On {
        internal_corner_rise(tool, radius)
    } else {
        None
    };
    let cutter = Cutter {
        params,
        heights,
        tab_top,
        tab_reach,
        internal: internal.map(|rise| (radius, rise)),
    };
    for part in &ordered {
        for chain in &part.holes {
            cutter.cut(&mut out, chain, &depths, waste_on_left);
        }
        for chain in &part.finish_holes {
            cutter.cut(&mut out, chain, &last_depth, finish_waste_on_left);
        }
        if let Some(chain) = &part.outline {
            cutter.cut(&mut out, chain, &depths, waste_on_left);
        }
        if let Some(chain) = &part.finish_outline {
            cutter.cut(&mut out, chain, &last_depth, finish_waste_on_left);
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

/// Writes the moves for one chain at each of its depths.
struct Cutter<'a> {
    params: &'a ProfileParams,
    heights: Heights,
    tab_top: f64,
    tab_reach: f64,
    /// For Sharp Internal Corner: how far the chain runs from the wall and
    /// how far the tip rises to reach the corner, in millimetres.
    internal: Option<(f64, f64)>,
}

impl Cutter<'_> {
    fn cut(&self, out: &mut Emitter, chain: &Chain, depths: &[f64], waste_on_left: bool) {
        let from = out.move_count();
        self.cut_levels(out, chain, depths, waste_on_left);
        if let Some((reach, rise)) = self.internal {
            let corners = internal_corners(chain, waste_on_left, reach);
            out.add_corner_runs(from, &corners, rise);
        }
    }

    fn cut_levels(&self, out: &mut Emitter, chain: &Chain, depths: &[f64], waste_on_left: bool) {
        let params = self.params;
        let heights = self.heights;
        let tab_top = self.tab_top;
        let triangular = params.tabs.triangular;
        let Some(begin) = chain.start() else {
            return;
        };
        let tabs = if chain.closed {
            match &params.tab_positions {
                Some(positions) => {
                    super::tabs::intervals_at(&params.tabs, chain, positions, self.tab_reach)
                }
                None => params.tabs.intervals(chain.length()),
            }
        } else {
            Vec::new()
        };
        let start_z = heights.z_at(params.start_depth);
        if chain.closed && params.lead.is_active() {
            if let Some(leads) = Leads::new(chain, &params.lead, &tabs, waste_on_left) {
                let mut above = start_z;
                for depth in depths {
                    let z = heights.z_at(*depth);
                    out.travel_to(leads.plunge_at());
                    if params.ramp.enabled && params.ramp.on_lead_in && !leads.lead_in.is_empty() {
                        out.plunge_to(start_z);
                        out.plunge_to(above);
                        out.cut_sloped(&leads.lead_in, z);
                    } else {
                        out.plunge_to(start_z);
                        out.plunge_to(z);
                        out.cut_flat(&leads.lead_in, z);
                    }
                    out.cut_chain_tabbed(chain, z, &tabs, tab_top, triangular);
                    out.cut_chain_tabbed(
                        &leads.overcut,
                        z,
                        &leads.overcut_tabs,
                        tab_top,
                        triangular,
                    );
                    if params.lead.lead_out {
                        out.cut_flat(&leads.lead_out, z);
                    }
                    out.retract();
                    above = z;
                }
                return;
            }
        }
        out.travel_to(begin);
        out.plunge_to(start_z);
        let mut forward = true;
        for depth in depths {
            let z = heights.z_at(*depth);
            if chain.closed {
                out.back_to_start(chain);
                let under_tabs = !tabs.is_empty() && z < tab_top - 1e-9;
                if params.ramp.enabled && !under_tabs {
                    out.cut_chain_ramp(chain, z, &params.ramp);
                } else {
                    out.plunge_to(z);
                    out.cut_chain_tabbed(chain, z, &tabs, tab_top, triangular);
                }
            } else {
                // Open vectors are cut back and forth, staying down.
                let chain = if forward {
                    chain.clone()
                } else {
                    chain.reversed()
                };
                if params.ramp.enabled {
                    out.cut_chain_ramp(&chain, z, &params.ramp);
                } else {
                    out.plunge_to(z);
                    out.cut_flat(&chain.segs, z);
                }
                forward = !forward;
            }
        }
        out.retract();
    }
}

/// How far a V-bit's tip rises from the tool position of a cut with
/// radius `radius` to reach the wall's corner: the height at which the cone
/// is that wide. `None` for other tools, and for a V-bit cutting deeper than
/// its cone, whose shank would cut into the walls.
fn internal_corner_rise(tool: &Tool, radius: f64) -> Option<f64> {
    let crate::cam::tool::ToolGeometry::VBit {
        diameter,
        included_angle,
    } = tool.tool_type
    else {
        return None;
    };
    let tan = (included_angle / 2.0).to_radians().tan();
    if !(tan.is_finite() && tan > 1e-9) || radius <= 1e-9 || radius >= diameter / 2.0 - 1e-9 {
        return None;
    }
    Some(radius / tan)
}

/// The internal corners of a chain that runs `reach` millimetres from its
/// wall: where it turns towards the waste. Each comes with the point on the
/// bisector where the two walls meet, which the tip runs up to.
fn internal_corners(chain: &Chain, waste_on_left: bool, reach: f64) -> Vec<(Point, Point)> {
    let n = chain.segs.len();
    let joins = if chain.closed { n } else { n.saturating_sub(1) };
    let part_side = |t: crate::geom::Vec2| {
        if waste_on_left {
            t.perp().scaled(-1.0)
        } else {
            t.perp()
        }
    };
    let mut corners = Vec::new();
    for i in 0..joins {
        let (a, b) = (&chain.segs[i], &chain.segs[(i + 1) % n]);
        let (Some(t_in), Some(t_out)) = (end_tangent(a), start_tangent(b)) else {
            continue;
        };
        let turn = t_in.cross(t_out).atan2(t_in.dot(t_out));
        let towards_waste = if waste_on_left {
            turn > 0.0
        } else {
            turn < 0.0
        };
        if !towards_waste || turn.abs() < 1f64.to_radians() || turn.abs() > 179f64.to_radians() {
            continue;
        }
        let Some(bisector) = part_side(t_in).plus(part_side(t_out)).normalised() else {
            continue;
        };
        let along = reach / (turn / 2.0).cos();
        corners.push((a.to, a.to.offset(bisector.scaled(along))));
    }
    corners
}

/// Groups closed loops into parts: each outline with the holes directly
/// inside it. Loops are `(chain, is_hole)`.
fn parts_of(loops: Vec<(Chain, bool)>) -> Vec<Part> {
    let mut outlines: Vec<Part> = Vec::new();
    let mut holes: Vec<Chain> = Vec::new();
    for (chain, hole) in loops {
        if chain.is_empty() {
            continue;
        }
        if hole {
            holes.push(chain);
        } else {
            outlines.push(Part {
                bbox: chain_bbox(&chain),
                outline: Some(chain),
                ..Part::default()
            });
        }
    }
    let polygons: Vec<Vec<Point>> = outlines
        .iter()
        .map(|p| p.outline.as_ref().map(chain_polygon).unwrap_or_default())
        .collect();
    let areas: Vec<f64> = polygons.iter().map(|p| polygon_area(p).abs()).collect();
    for (i, part) in outlines.iter_mut().enumerate() {
        let probe = polygons[i].first().copied().unwrap_or(Point::ORIGIN);
        part.nesting = polygons
            .iter()
            .enumerate()
            .filter(|(j, poly)| *j != i && areas[*j] > areas[i] && inside(poly, probe))
            .count();
    }
    let mut loose = Vec::new();
    for hole in holes {
        let probe = hole.start().unwrap_or(Point::ORIGIN);
        // The smallest outline round the hole owns it.
        let owner = polygons
            .iter()
            .enumerate()
            .filter(|(_, poly)| inside(poly, probe))
            .min_by(|a, b| areas[a.0].total_cmp(&areas[b.0]))
            .map(|(i, _)| i);
        match owner {
            Some(i) => outlines[i].holes.push(hole),
            None => loose.push(hole),
        }
    }
    for hole in loose {
        outlines.push(Part {
            bbox: chain_bbox(&hole),
            holes: vec![hole],
            ..Part::default()
        });
    }
    outlines
}

/// Hangs each part of the last pass on the part of the main passes it
/// belongs to: the nearest by bounding box centre.
fn attach_finish(parts: &mut Vec<Part>, finish: Vec<Part>) {
    let centre = |b: &Option<BBox>| {
        b.map_or(Point::ORIGIN, |b| {
            Point::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0)
        })
    };
    for f in finish {
        let c = centre(&f.bbox);
        let best = parts
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.open && p.outline.is_some() == f.outline.is_some())
            .min_by(|a, b| {
                centre(&a.1.bbox)
                    .distance_squared(c)
                    .total_cmp(&centre(&b.1.bbox).distance_squared(c))
            })
            .map(|(i, _)| i);
        match best {
            Some(i) if parts[i].finish_outline.is_none() || f.outline.is_none() => {
                let part = &mut parts[i];
                part.finish_holes.extend(f.holes);
                if f.outline.is_some() {
                    part.finish_outline = f.outline;
                }
            }
            _ => parts.push(Part {
                finish_holes: f.holes,
                finish_outline: f.outline,
                nesting: f.nesting,
                bbox: f.bbox,
                ..Part::default()
            }),
        }
    }
}

/// Orders parts: deepest nested first (open vectors before everything),
/// then by `order` within each nesting level.
fn order_parts(parts: Vec<Part>, order: CutOrder, home: Point) -> Vec<Part> {
    let mut levels: Vec<usize> = parts.iter().map(|p| p.nesting).collect();
    levels.sort_unstable();
    levels.dedup();
    let mut buckets: Vec<Vec<Part>> = levels.iter().map(|_| Vec::new()).collect();
    for part in parts {
        if part.first().is_none() && part.last().is_none() {
            continue;
        }
        let at = levels.binary_search(&part.nesting).unwrap_or(0);
        buckets[at].push(part);
    }
    let grid_row = {
        let tallest = buckets
            .iter()
            .flatten()
            .filter_map(|p| p.bbox)
            .map(|b| b.max.y - b.min.y)
            .fold(0.0_f64, f64::max)
            .max(1e-6);
        let bottom = buckets
            .iter()
            .flatten()
            .filter_map(|p| p.bbox)
            .map(|b| b.min.y)
            .fold(f64::MAX, f64::min);
        move |b: &Option<BBox>| b.map_or(0, |b| ((b.min.y - bottom) / tallest).floor() as i64)
    };
    let min_x = |p: &Part| p.bbox.map_or(0.0, |b| b.min.x);
    let min_y = |p: &Part| p.bbox.map_or(0.0, |b| b.min.y);
    let mut out = Vec::new();
    let mut at = home;
    for mut bucket in buckets.into_iter().rev() {
        match order {
            CutOrder::Optimized => {
                bucket = order_nearest(
                    bucket,
                    at,
                    |p| p.first().and_then(Chain::start).unwrap_or(home),
                    |p| p.last().and_then(Chain::end).unwrap_or(home),
                );
            }
            CutOrder::SelectionOrder => bucket.sort_by_key(|p| p.source),
            CutOrder::LeftToRight => {
                bucket.sort_by(|a, b| {
                    min_x(a)
                        .total_cmp(&min_x(b))
                        .then(min_y(a).total_cmp(&min_y(b)))
                });
            }
            CutOrder::BottomToTop => {
                bucket.sort_by(|a, b| {
                    min_y(a)
                        .total_cmp(&min_y(b))
                        .then(min_x(a).total_cmp(&min_x(b)))
                });
            }
            CutOrder::Grid => bucket.sort_by(|a, b| {
                grid_row(&a.bbox)
                    .cmp(&grid_row(&b.bbox))
                    .then(min_x(a).total_cmp(&min_x(b)))
            }),
        }
        if let Some(end) = bucket.last().and_then(Part::last).and_then(Chain::end) {
            at = end;
        }
        out.extend(bucket);
    }
    out
}

/// Tries each order and keeps the one with the least travel between parts
/// (the cuts themselves are the same whatever the order); the first listed
/// wins a tie.
fn fastest_order(parts: Vec<Part>, orders: &[CutOrder], home: Point) -> Vec<Part> {
    let mut best: Option<(f64, Vec<Part>)> = None;
    for order in orders {
        let ordered = order_parts(parts.clone(), *order, home);
        let travel = travel_between(&ordered, home);
        if best.as_ref().is_none_or(|(b, _)| travel < *b - 1e-9) {
            best = Some((travel, ordered));
        }
    }
    best.map_or_else(|| order_parts(parts, CutOrder::Optimized, home), |b| b.1)
}

/// How far the tool travels from home, between parts and back home.
fn travel_between(parts: &[Part], home: Point) -> f64 {
    let mut at = home;
    let mut total = 0.0;
    for part in parts {
        if let Some(start) = part.first().and_then(Chain::start) {
            total += at.distance(start);
        }
        if let Some(end) = part.last().and_then(Chain::end) {
            at = end;
        }
    }
    total + at.distance(home)
}

/// A closed chain restarted at the end of one of its spans, the one nearest
/// `p`.
fn restart_at_node(chain: &Chain, p: Point) -> Chain {
    let Some(best) = chain
        .segs
        .iter()
        .enumerate()
        .min_by(|a, b| {
            a.1.from
                .distance_squared(p)
                .total_cmp(&b.1.from.distance_squared(p))
        })
        .map(|(i, _)| i)
    else {
        return chain.clone();
    };
    let mut segs = chain.segs[best..].to_vec();
    segs.extend_from_slice(&chain.segs[..best]);
    Chain { segs, closed: true }
}

/// Index of the path in `paths` that comes nearest the chain's start.
fn nearest_source(paths: &[Path], chain: &Chain) -> usize {
    let probe = chain.start().unwrap_or(Point::ORIGIN);
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let d = path
                .subpaths
                .iter()
                .flat_map(|s| std::iter::once(s.start).chain(s.segments.iter().map(|g| g.end())))
                .map(|p| p.distance_squared(probe))
                .fold(f64::MAX, f64::min);
            (i, d)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map_or(0, |(i, _)| i)
}

/// Points along a chain, a few per arc, for bounds and containment.
fn chain_polygon(chain: &Chain) -> Vec<Point> {
    let mut points = Vec::with_capacity(chain.segs.len() * 2);
    for seg in &chain.segs {
        points.push(seg.from);
        if seg.arc().is_some() {
            let length = seg.length();
            for i in 1..8 {
                points.push(seg.point_at(length * i as f64 / 8.0));
            }
        }
    }
    points
}

fn chain_bbox(chain: &Chain) -> Option<BBox> {
    let mut points = chain_polygon(chain);
    points.extend(chain.end());
    BBox::of_points(points)
}

fn polygon_area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

fn inside(polygon: &[Point], p: Point) -> bool {
    let n = polygon.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            odd = !odd;
        }
    }
    odd
}

fn start_mid_longest(chain: &Chain) -> Chain {
    let mut at = 0.0;
    let mut best = (0.0, 0.0);
    for seg in &chain.segs {
        let length = seg.length();
        if length > best.0 {
            best = (length, at + length / 2.0);
        }
        at += length;
    }
    if !chain.closed || best.0 <= 1e-9 {
        return chain.clone();
    }
    let mut pieces = chain.split_at(&[best.1]).into_iter();
    let head = pieces.next().unwrap_or_default();
    let mut segs = pieces.next().unwrap_or_default();
    segs.extend(head);
    Chain { segs, closed: true }
}

/// The extra cuts of a closed chain with leads: in, overcut and out.
struct Leads {
    start: crate::geom::Point,
    lead_in: Vec<CutSeg>,
    overcut: Chain,
    overcut_tabs: Vec<(f64, f64)>,
    lead_out: Vec<CutSeg>,
}

impl Leads {
    /// Builds the leads for `chain`, or `None` when the chain has no
    /// direction to lead along.
    fn new(
        chain: &Chain,
        spec: &LeadSpec,
        tabs: &[(f64, f64)],
        waste_on_left: bool,
    ) -> Option<Leads> {
        let waste = |tangent: crate::geom::Vec2| {
            if waste_on_left {
                tangent.perp()
            } else {
                tangent.perp().scaled(-1.0)
            }
        };
        let first = chain.segs.first()?;
        let t_in = start_tangent(first)?;
        let lead_in = spec.lead_in(first.from, t_in, waste(t_in));

        // The overcut stops short of a tab rather than ending inside one.
        let mut overcut = spec.overcut.min(chain.length());
        for &(a, b) in tabs {
            if a < overcut && overcut < b {
                overcut = a;
            }
        }
        let overcut_chain = if overcut > 1e-9 {
            let pieces = chain.split_at(&[overcut]);
            Chain {
                segs: pieces.into_iter().next().unwrap_or_default(),
                closed: false,
            }
        } else {
            Chain {
                segs: Vec::new(),
                closed: false,
            }
        };
        let overcut_tabs: Vec<(f64, f64)> = tabs
            .iter()
            .copied()
            .filter(|&(_, b)| b <= overcut)
            .collect();
        let last = overcut_chain.segs.last().or(chain.segs.last())?;
        let t_out = end_tangent(last)?;
        let lead_out = spec.lead_out(last.to, t_out, waste(t_out));
        let start = lead_in.first().map_or(first.from, |s| s.from);
        Some(Leads {
            start,
            lead_in,
            overcut: overcut_chain,
            overcut_tabs,
            lead_out,
        })
    }

    /// Where the tool goes down: the start of the lead in.
    fn plunge_at(&self) -> crate::geom::Point {
        self.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::common::RampKind;
    use crate::cam::toolpath::{MoveKind, Point3};
    use crate::geom::{circle, path_bbox, rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(1e-3).expect("tolerance")
    }

    fn tool() -> Tool {
        Tool {
            pass_depth: 3.0,
            ..Tool::default()
        }
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
        rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect")
    }

    fn cutting_xy_box(moves: &[Move], z: f64) -> (f64, f64, f64, f64) {
        let pts: Vec<Point3> = moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z - z).abs() < 1e-9)
            .map(|m| m.to)
            .collect();
        let min_x = pts.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        let max_x = pts.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        let min_y = pts.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        let max_y = pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn an_outside_profile_runs_a_tool_radius_outside_in_two_passes() {
        let params = ProfileParams::default();
        let done = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("profile");
        let (x0, y0, x1, y1) = cutting_xy_box(&done.moves, -6.0);
        assert!(
            (x0 + 3.0).abs() < 1e-6 && (x1 - 53.0).abs() < 1e-6,
            "{x0} {x1}"
        );
        assert!(
            (y0 + 3.0).abs() < 1e-6 && (y1 - 33.0).abs() < 1e-6,
            "{y0} {y1}"
        );
        let levels: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.z)
            .collect();
        assert!(
            levels.contains(&-3.0) && levels.contains(&-6.0),
            "{levels:?}"
        );
        assert!(done.moves.iter().all(|m| m.to.z >= -6.0 - 1e-9));
        assert_eq!(
            done.moves.last().map(|m| m.to),
            Some(Heights::from_job(&JobSetup::default()).home)
        );
    }

    #[test]
    fn an_inside_profile_runs_inside() {
        let params = ProfileParams {
            side: ProfileSide::Inside,
            ..ProfileParams::default()
        };
        let done = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("profile");
        let (x0, _, x1, _) = cutting_xy_box(&done.moves, -6.0);
        assert!((x0 - 3.0).abs() < 1e-6 && (x1 - 47.0).abs() < 1e-6);
    }

    #[test]
    fn climb_goes_clockwise_round_the_outside_and_conventional_the_other_way() {
        let loop_area = |moves: &[Move]| {
            let pts: Vec<Point3> = moves
                .iter()
                .filter(|m| m.kind.is_cutting() && (m.to.z + 6.0).abs() < 1e-9)
                .map(|m| m.to)
                .collect();
            let mut twice = 0.0;
            for w in pts.windows(2) {
                twice += w[0].x * w[1].y - w[1].x * w[0].y;
            }
            twice
        };
        let climb = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &ProfileParams::default(),
            tol(),
        )
        .expect("profile");
        assert!(loop_area(&climb.moves) < 0.0, "clockwise");
        let conventional = ProfileParams {
            direction: CutDirection::Conventional,
            ..ProfileParams::default()
        };
        let conv = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &conventional,
            tol(),
        )
        .expect("profile");
        assert!(loop_area(&conv.moves) > 0.0, "counter clockwise");
    }

    #[test]
    fn a_letter_with_a_counter_cuts_the_counter_first_and_the_outline_last() {
        let outer = square(0.0, 0.0, 60.0, 60.0);
        let counter = square(20.0, 20.0, 40.0, 40.0);
        let done = profile(
            &[outer, counter],
            &JobSetup::default(),
            &tool(),
            &ProfileParams::default(),
            tol(),
        )
        .expect("profile");
        let deep: Vec<Point3> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 6.0).abs() < 1e-9)
            .map(|m| m.to)
            .collect();
        let first = deep.first().copied().expect("cuts");
        let last = deep.last().copied().expect("cuts");
        assert!(first.x > 15.0 && first.x < 45.0, "counter first: {first:?}");
        assert!(
            last.x < 0.0 || last.x > 60.0 || last.y < 0.0 || last.y > 60.0,
            "outline last: {last:?}"
        );
        // The counter's cut sits inside it, a radius in from its edges.
        let inside: Vec<&Point3> = deep
            .iter()
            .filter(|p| p.x > 10.0 && p.x < 50.0 && p.y > 10.0 && p.y < 50.0)
            .collect();
        assert!(inside
            .iter()
            .all(|p| p.x >= 23.0 - 1e-6 && p.x <= 37.0 + 1e-6));
    }

    #[test]
    fn tabs_leave_material_under_the_last_pass() {
        let params = ProfileParams {
            tabs: TabSpec {
                enabled: true,
                count: 4,
                length: 8.0,
                thickness: 2.0,
                ..TabSpec::default()
            },
            ..ProfileParams::default()
        };
        let done = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("profile");
        let on_tabs = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 4.0).abs() < 1e-9)
            .count();
        assert!(on_tabs >= 8, "moves at the tab top: {on_tabs}");
    }

    #[test]
    fn edited_tab_positions_put_tabs_where_they_were_placed() {
        let tabs = TabSpec {
            enabled: true,
            count: 4,
            length: 8.0,
            thickness: 2.0,
            ..TabSpec::default()
        };
        let at_tabs = |positions: Vec<Point>| {
            let params = ProfileParams {
                tabs,
                tab_positions: Some(positions),
                ..ProfileParams::default()
            };
            run(&params)
                .into_iter()
                .filter(|m| m.kind.is_cutting() && (m.to.z + 4.0).abs() < 1e-9)
                .map(|m| m.to.xy())
                .collect::<Vec<_>>()
        };
        // One tab on the bottom edge, at x = 20: every move on the tab top
        // is near it, under the cut that runs beside the edge.
        let one = at_tabs(vec![Point::new(20.0, 0.0)]);
        assert!(!one.is_empty());
        assert!(
            one.iter()
                .all(|p| (p.x - 20.0).abs() <= 4.0 + 1e-6 && p.y < 0.0),
            "{one:?}"
        );
        // A position far from the vector adds no tab.
        assert!(at_tabs(vec![Point::new(25.0, 15.0)]).is_empty());
    }

    #[test]
    fn a_ramp_replaces_the_plunges_below_the_start() {
        let params = ProfileParams {
            ramp: RampSpec {
                enabled: true,
                distance: 20.0,
                ..RampSpec::default()
            },
            ..ProfileParams::default()
        };
        let done = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("profile");
        assert!(done
            .moves
            .iter()
            .all(|m| m.kind != MoveKind::Plunge || m.to.z >= -1e-9));
    }

    #[test]
    fn a_smooth_ramp_keeps_every_pass_on_the_vector_with_tabs_below() {
        // Several passes with a smooth ramp, then passes under tabs: each
        // pass starts from the vector's start, so every arc keeps its
        // radius and no move leaves the offset line.
        let params = ProfileParams {
            cut_depth: 12.0,
            ramp: RampSpec {
                enabled: true,
                distance: 20.0,
                ..RampSpec::default()
            },
            tabs: TabSpec {
                enabled: true,
                count: 4,
                length: 10.0,
                thickness: 3.0,
                ..TabSpec::default()
            },
            ..ProfileParams::default()
        };
        let done = profile(
            &[square(20.0, 20.0, 120.0, 80.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect("profile");
        let r = tool().diameter() / 2.0;
        let off = |p: Point3| {
            let dx = (20.0 - p.x).max(p.x - 120.0).max(0.0);
            let dy = (20.0 - p.y).max(p.y - 80.0).max(0.0);
            (dx * dx + dy * dy).sqrt()
        };
        let mut at = done.moves[0].to;
        for m in &done.moves {
            if m.kind.is_cutting() && m.to.z < -1e-9 {
                assert!((off(m.to) - r).abs() < 1e-6, "{m:?} leaves the line");
            }
            if let MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } = m.kind {
                let a = at.xy().distance(centre);
                let b = m.to.xy().distance(centre);
                assert!((a - b).abs() < 1e-6, "arc {a} vs {b} from {at:?} {m:?}");
            }
            at = m.to;
        }
        assert!(done.moves.iter().any(|m| (m.to.z + 9.0).abs() < 1e-9));
    }

    #[test]
    fn an_open_vector_is_cut_on_the_line_back_and_forth() {
        let mut sub = crate::geom::SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(40.0, 0.0));
        let done = profile(
            &[sub.into_path()],
            &JobSetup::default(),
            &tool(),
            &ProfileParams::default(),
            tol(),
        )
        .expect("profile");
        let deep: Vec<Point3> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| m.to)
            .collect();
        assert_eq!(deep.first().map(|p| p.x), Some(40.0));
        assert!(
            deep.iter().any(|p| p.x == 0.0 && p.z == -6.0),
            "comes back on the second pass"
        );
        assert!(deep.iter().all(|p| p.y == 0.0));
    }

    #[test]
    fn a_circle_profile_is_cut_with_arcs() {
        let c = circle(Point::new(50.0, 50.0), 20.0).expect("circle");
        let done = profile(
            std::slice::from_ref(&c),
            &JobSetup::default(),
            &tool(),
            &ProfileParams::default(),
            tol(),
        )
        .expect("profile");
        assert!(done.moves.iter().any(|m| m.kind.is_arc()));
        let bbox = path_bbox(&c).expect("bbox");
        assert!(bbox.width() > 0.0);
    }

    #[test]
    fn a_tool_too_big_for_an_inside_profile_is_reported() {
        let params = ProfileParams {
            side: ProfileSide::Inside,
            ..ProfileParams::default()
        };
        let error = profile(
            &[square(0.0, 0.0, 4.0, 4.0)],
            &JobSetup::default(),
            &tool(),
            &params,
            tol(),
        )
        .expect_err("nothing fits");
        assert!(matches!(error, CoreError::Validation(_)));
    }

    fn lead_params(side: ProfileSide, lead: LeadSpec) -> ProfileParams {
        ProfileParams {
            side,
            lead,
            ..ProfileParams::default()
        }
    }

    fn straight(length: f64, angle_deg: f64, overcut: f64) -> LeadSpec {
        LeadSpec {
            kind: super::super::common::LeadKind::Straight,
            length,
            angle_deg,
            radius: 0.0,
            arc_angle_deg: 90.0,
            overcut,
            lead_out: true,
        }
    }

    /// Distance from `p` to the edge of the rectangle 0..50 by 0..30,
    /// negative inside.
    fn from_rect(p: Point) -> f64 {
        let dx = (-p.x).max(p.x - 50.0);
        let dy = (-p.y).max(p.y - 30.0);
        if dx > 0.0 || dy > 0.0 {
            (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
        } else {
            dx.max(dy)
        }
    }

    fn run(params: &ProfileParams) -> Vec<Move> {
        profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            params,
            tol(),
        )
        .expect("profile")
        .moves
    }

    fn first_plunge(moves: &[Move]) -> usize {
        moves
            .iter()
            .position(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .expect("a plunge")
    }

    fn xy_length(from: Point3, m: &Move) -> f64 {
        let (a, b) = (from.xy(), m.to.xy());
        match m.kind {
            MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                let (u, v) = (centre.to(a), centre.to(b));
                u.length() * u.cross(v).atan2(u.dot(v)).abs()
            }
            _ => a.distance(b),
        }
    }

    #[test]
    fn a_straight_lead_plunges_off_the_path_on_the_waste_side() {
        let lead = straight(4.0, 90.0, 0.0);
        let outside = run(&lead_params(ProfileSide::Outside, lead));
        let plunge = outside[first_plunge(&outside)].to.xy();
        // The path runs 3 mm outside the rectangle; the lead adds 4 more.
        assert!((from_rect(plunge) - 7.0).abs() < 1e-6, "{plunge:?}");
        let lead_end = outside[first_plunge(&outside) + 1].to.xy();
        assert!((plunge.distance(lead_end) - 4.0).abs() < 1e-6);
        assert!((from_rect(lead_end) - 3.0).abs() < 1e-6);

        let inside = run(&lead_params(ProfileSide::Inside, lead));
        let plunge = inside[first_plunge(&inside)].to.xy();
        assert!((from_rect(plunge) + 7.0).abs() < 1e-6, "{plunge:?}");

        // A circle: the path is at radius 17 inside, 23 outside.
        let c = circle(Point::new(50.0, 50.0), 20.0).expect("circle");
        for (side, want) in [(ProfileSide::Inside, 13.0), (ProfileSide::Outside, 27.0)] {
            let moves = profile(
                std::slice::from_ref(&c),
                &JobSetup::default(),
                &tool(),
                &lead_params(side, lead),
                tol(),
            )
            .expect("profile")
            .moves;
            let plunge = moves[first_plunge(&moves)].to.xy();
            let r = plunge.distance(Point::new(50.0, 50.0));
            assert!((r - want).abs() < 1e-6, "{side:?} {r}");
        }
    }

    #[test]
    fn an_arc_lead_meets_the_path_tangentially() {
        let lead = LeadSpec {
            kind: super::super::common::LeadKind::Arc,
            radius: 5.0,
            ..LeadSpec::default()
        };
        for side in [ProfileSide::Outside, ProfileSide::Inside] {
            let moves = run(&lead_params(side, lead));
            let at = first_plunge(&moves);
            let plunge = moves[at].to.xy();
            let arc = &moves[at + 1];
            let (MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre }) = arc.kind else {
                panic!("an arc lead: {arc:?}");
            };
            let meet = arc.to.xy();
            assert!((centre.distance(meet) - 5.0).abs() < 1e-6);
            assert!((centre.distance(plunge) - 5.0).abs() < 1e-6);
            let next = &moves[at + 2];
            let radial = centre.to(meet);
            let off_tangent = match next.kind {
                MoveKind::ArcCw { centre: c } | MoveKind::ArcCcw { centre: c } => {
                    radial.cross(c.to(meet)) / radial.length() / c.distance(meet)
                }
                _ => radial.dot(meet.to(next.to.xy())) / radial.length(),
            };
            assert!(off_tangent.abs() < 1e-6, "{side:?} {off_tangent}");
            // The lead starts on the waste side of the path.
            let from_path = from_rect(plunge);
            match side {
                ProfileSide::Outside => assert!(from_path > 3.0 + 1.0, "{from_path}"),
                _ => assert!(from_path < -3.0 - 1.0, "{from_path}"),
            }
        }
    }

    #[test]
    fn a_v_bit_runs_up_into_internal_corners_when_asked() {
        let l_shape = crate::geom::polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(30.0, 0.0),
                Point::new(30.0, 10.0),
                Point::new(10.0, 10.0),
                Point::new(10.0, 30.0),
                Point::new(0.0, 30.0),
            ],
            true,
        )
        .expect("L");
        let v_bit = Tool {
            pass_depth: 5.0,
            tool_type: crate::cam::tool::ToolGeometry::VBit {
                diameter: 20.0,
                included_angle: 90.0,
            },
            ..Tool::default()
        };
        let cut = |tool: &Tool, sharp: bool| {
            let params = ProfileParams {
                cut_depth: 3.0,
                sharp_internal_corners: sharp,
                ..ProfileParams::default()
            };
            profile(
                std::slice::from_ref(&l_shape),
                &JobSetup::default(),
                tool,
                &params,
                tol(),
            )
            .expect("profile")
            .moves
        };
        let at_corner = |moves: &[Move]| {
            moves
                .iter()
                .filter(|m| m.to.xy().distance(Point::new(10.0, 10.0)) < 1e-6)
                .map(|m| m.to.z)
                .collect::<Vec<f64>>()
        };
        let sharp = cut(&v_bit, true);
        // The cone is 3 mm wide at the surface: the tip reaches the corner
        // there, and nothing goes below the cut depth.
        assert_eq!(at_corner(&sharp).len(), 1, "{:?}", at_corner(&sharp));
        assert!(at_corner(&sharp)[0].abs() < 1e-6);
        assert!(sharp.iter().all(|m| m.to.z >= -3.0 - 1e-9));
        assert!(at_corner(&cut(&v_bit, false)).is_empty());
        // An end mill cannot make the corner sharp.
        assert!(at_corner(&cut(&tool(), true)).is_empty());
    }

    #[test]
    fn a_circular_lead_turns_through_its_angle() {
        let lead = LeadSpec {
            kind: super::super::common::LeadKind::Arc,
            radius: 5.0,
            arc_angle_deg: 30.0,
            lead_out: false,
            ..LeadSpec::default()
        };
        let moves = run(&lead_params(ProfileSide::Outside, lead));
        let at = first_plunge(&moves);
        let arc = &moves[at + 1];
        let length = xy_length(moves[at].to, arc);
        assert!((length - 5.0 * 30f64.to_radians()).abs() < 1e-6, "{length}");
        let bad = LeadSpec {
            arc_angle_deg: 95.0,
            ..lead
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn a_zig_zag_by_angle_goes_back_and_forth_over_its_distance() {
        let params = ProfileParams {
            cut_depth: 3.0,
            ramp: RampSpec {
                enabled: true,
                kind: RampKind::ZigZag,
                distance: 10.0,
                angle: Some(5.0),
                on_lead_in: false,
            },
            ..ProfileParams::default()
        };
        let moves = run(&params);
        // 10 mm legs at 5 degrees drop under 0.875 mm each: four legs for 3 mm.
        let start = moves
            .iter()
            .position(|m| m.kind.is_cutting() && m.to.z < 0.0)
            .expect("a cut");
        let from = moves[start - 1].to;
        let mut along = 0.0;
        let mut at = from;
        let mut turns: Vec<(f64, f64)> = Vec::new();
        for m in &moves[start..] {
            along += xy_length(at, m);
            at = m.to;
            for leg in 1..=4 {
                let want = -3.0 * f64::from(leg) / 4.0;
                if (m.to.z - want).abs() < 1e-6 && turns.len() == leg as usize - 1 {
                    turns.push((along, m.to.z));
                }
            }
            if turns.len() == 4 {
                break;
            }
        }
        assert_eq!(turns.len(), 4, "{turns:?}");
        for (i, (distance, _)) in turns.iter().enumerate() {
            assert!((distance - 10.0 * (i + 1) as f64).abs() < 1e-6, "{turns:?}");
        }
        // Even legs end back where the ramp started.
        assert!(at.xy().distance(from.xy()) < 1e-6, "{at:?} {from:?}");
        assert!(moves.iter().all(|m| m.to.z >= -3.0 - 1e-9));
    }

    #[test]
    fn the_overcut_recuts_the_start_of_the_loop() {
        let moves = run(&lead_params(ProfileSide::Outside, straight(4.0, 90.0, 7.0)));
        let last = moves
            .iter()
            .rposition(|m| m.kind == MoveKind::Plunge && (m.to.z + 6.0).abs() < 1e-9)
            .expect("the last pass");
        let mut total = 0.0;
        let mut from = moves[last].to;
        for m in moves[last + 1..]
            .iter()
            .take_while(|m| m.kind.is_cutting() && (m.to.z + 6.0).abs() < 1e-9)
        {
            total += xy_length(from, m);
            from = m.to;
        }
        let perimeter = 2.0 * (50.0 + 30.0) + std::f64::consts::TAU * 3.0;
        assert!(
            (total - (4.0 + perimeter + 7.0 + 4.0)).abs() < 1e-6,
            "{total}"
        );
        // Two passes, each with its own lead.
        let plunges = moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .count();
        assert_eq!(plunges, 2);
        assert!(moves.iter().all(|m| m.to.z >= -6.0 - 1e-9));
    }

    fn deep_points(moves: &[Move], z: f64) -> Vec<Point3> {
        moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z - z).abs() < 1e-9)
            .map(|m| m.to)
            .collect()
    }

    #[test]
    fn a_separate_last_pass_leaves_a_skin_then_cuts_it_at_full_depth() {
        let params = ProfileParams {
            last_pass: LastPass {
                enabled: true,
                allowance: 0.5,
                reverse: true,
            },
            ..ProfileParams::default()
        };
        let moves = run(&params);
        // Rough passes run 3.5 out; the last one 3 out, only at full depth.
        let (x0, _, _, _) = cutting_xy_box(&moves, -3.0);
        assert!((x0 + 3.5).abs() < 1e-6, "{x0}");
        let (x0, _, _, _) = cutting_xy_box(&moves, -6.0);
        assert!((x0 + 3.5).abs() < 1e-6 || (x0 + 3.0).abs() < 1e-6);
        let finish: Vec<Point3> = deep_points(&moves, -6.0)
            .into_iter()
            .filter(|p| (from_rect(p.xy()) - 3.0).abs() < 1e-6)
            .collect();
        assert!(finish.len() >= 4, "a last pass at 3 mm");
        let plunges = moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .count();
        assert_eq!(plunges, 3, "two rough passes and the last pass");
    }

    #[test]
    fn pass_count_and_last_pass_thickness_set_the_levels() {
        let params = ProfileParams {
            pass_count: Some(3),
            ..ProfileParams::default()
        };
        let levels: Vec<f64> = run(&params)
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| m.to.z)
            .collect();
        assert_eq!(levels, vec![-2.0, -4.0, -6.0]);
        let params = ProfileParams {
            last_pass_thickness: Some(0.5),
            ..ProfileParams::default()
        };
        let levels: Vec<f64> = run(&params)
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            .map(|m| m.to.z)
            .collect();
        assert_eq!(levels, vec![-2.75, -5.5, -6.0]);
    }

    #[test]
    fn triangular_tabs_climb_to_the_top_and_back() {
        let params = ProfileParams {
            tabs: TabSpec {
                enabled: true,
                count: 2,
                length: 8.0,
                thickness: 2.0,
                triangular: true,
                spacing: None,
                ..TabSpec::default()
            },
            ..ProfileParams::default()
        };
        let moves = run(&params);
        let peaks = moves
            .iter()
            .filter(|m| m.kind.is_cutting() && (m.to.z + 4.0).abs() < 1e-9)
            .count();
        assert!(peaks >= 2, "{peaks}");
        // No square lift: every rise is a sloped cut, never straight up.
        assert!(!moves.windows(2).any(|w| w[1].kind.is_cutting()
            && w[1].to.z > w[0].to.z + 1e-9
            && w[1].to.xy().distance(w[0].to.xy()) < 1e-9
            && w[0].to.z < -3.0));
    }

    #[test]
    fn tabs_by_distance_scale_with_the_vector() {
        let tabs = TabSpec {
            enabled: true,
            spacing: Some(50.0),
            ..TabSpec::default()
        };
        assert_eq!(tabs.intervals(200.0).len(), 4);
        assert_eq!(tabs.intervals(20.0).len(), 1);
    }

    #[test]
    fn zig_zag_and_spiral_ramps_never_plunge_into_the_cut() {
        for kind in [RampKind::ZigZag, RampKind::Spiral] {
            let params = ProfileParams {
                ramp: RampSpec {
                    enabled: true,
                    kind,
                    angle: Some(10.0),
                    ..RampSpec::default()
                },
                ..ProfileParams::default()
            };
            let moves = run(&params);
            assert!(
                moves
                    .iter()
                    .all(|m| m.kind != MoveKind::Plunge || m.to.z >= -1e-9),
                "{kind:?}"
            );
            assert!(moves.iter().all(|m| m.to.z >= -6.0 - 1e-9));
        }
    }

    #[test]
    fn a_lead_in_without_a_lead_out_lifts_off_the_loop() {
        let mut lead = straight(4.0, 90.0, 0.0);
        lead.lead_out = false;
        let moves = run(&lead_params(ProfileSide::Outside, lead));
        let last_cut = moves
            .iter()
            .rposition(|m| m.kind.is_cutting())
            .expect("cuts");
        assert!((from_rect(moves[last_cut].to.xy()) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_ramp_on_the_lead_in_slopes_down_the_lead() {
        let mut params = lead_params(ProfileSide::Outside, straight(4.0, 90.0, 0.0));
        params.ramp = RampSpec {
            enabled: true,
            on_lead_in: true,
            ..RampSpec::default()
        };
        let moves = run(&params);
        assert!(moves
            .iter()
            .all(|m| m.kind != MoveKind::Plunge || m.to.z >= -3.0 - 1e-9));
        assert!(moves.iter().any(|m| m.kind == MoveKind::Feed
            && m.to.z < 0.0
            && (from_rect(m.to.xy()) - 3.0).abs() < 1e-6));
    }

    #[test]
    fn an_open_vector_can_be_cut_to_its_left_or_right() {
        let mut sub = crate::geom::SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(40.0, 0.0));
        let path = sub.into_path();
        for (side, y) in [(OpenSide::Left, 3.0), (OpenSide::Right, -3.0)] {
            let params = ProfileParams {
                open_side: side,
                ..ProfileParams::default()
            };
            let done = profile(
                std::slice::from_ref(&path),
                &JobSetup::default(),
                &tool(),
                &params,
                tol(),
            )
            .expect("profile");
            let deep = deep_points(&done.moves, -6.0);
            assert!(!deep.is_empty());
            assert!(deep.iter().all(|p| (p.y - y).abs() < 1e-6), "{side:?}");
        }
    }

    #[test]
    fn parts_follow_the_chosen_order() {
        let shapes = [
            square(100.0, 0.0, 120.0, 20.0),
            square(0.0, 50.0, 20.0, 70.0),
            square(50.0, 0.0, 70.0, 20.0),
        ];
        let first_x = |order: CutOrder| {
            let params = ProfileParams {
                order,
                pass_count: Some(1),
                ..ProfileParams::default()
            };
            let moves = profile(&shapes, &JobSetup::default(), &tool(), &params, tol())
                .expect("profile")
                .moves;
            let mut xs: Vec<f64> = Vec::new();
            for m in moves
                .iter()
                .filter(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0)
            {
                xs.push(m.to.x);
            }
            xs
        };
        let ltr = first_x(CutOrder::LeftToRight);
        assert!(ltr.windows(2).all(|w| w[0] < w[1]), "{ltr:?}");
        let sel = first_x(CutOrder::SelectionOrder);
        assert!(
            sel[0] > 90.0 && sel[1] < 30.0 && sel[2] > 40.0 && sel[2] < 80.0,
            "{sel:?}"
        );
        let btt = first_x(CutOrder::BottomToTop);
        assert!(btt[2] < 30.0, "{btt:?}");
    }

    #[test]
    fn a_part_inside_a_hole_is_cut_before_the_hole() {
        let frame = square(0.0, 0.0, 100.0, 100.0);
        let window = square(10.0, 10.0, 90.0, 90.0);
        let island = square(40.0, 40.0, 60.0, 60.0);
        let moves = profile(
            &[frame, window, island],
            &JobSetup::default(),
            &tool(),
            &ProfileParams::default(),
            tol(),
        )
        .expect("profile")
        .moves;
        let first = deep_points(&moves, -6.0)[0];
        assert!(
            first.x > 30.0 && first.x < 70.0 && first.y > 30.0 && first.y < 70.0,
            "{first:?}"
        );
    }

    #[test]
    fn starts_can_sit_at_the_bounding_box_corner() {
        let params = ProfileParams {
            start_at: StartAt::BoundingBox,
            ..ProfileParams::default()
        };
        let moves = run(&params);
        let plunge = moves[first_plunge(&moves)].to.xy();
        // A span end of the outside loop's corner arc nearest (-3, -3).
        let near = |p: Point, x: f64, y: f64| (p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6;
        assert!(
            near(plunge, 0.0, -3.0) || near(plunge, -3.0, 0.0),
            "{plunge:?}"
        );
        let top_right = ProfileParams {
            start_anchor: Anchor::TopRight,
            ..params
        };
        let moves = run(&top_right);
        let plunge = moves[first_plunge(&moves)].to.xy();
        assert!(
            near(plunge, 50.0, 33.0) || near(plunge, 53.0, 30.0),
            "{plunge:?}"
        );
        let middle_left = ProfileParams {
            start_anchor: Anchor::MiddleLeft,
            ..top_right
        };
        let moves = run(&middle_left);
        let plunge = moves[first_plunge(&moves)].to.xy();
        assert!((plunge.x + 3.0).abs() < 1e-6, "{plunge:?}");
    }

    #[test]
    fn ticked_orders_are_each_tried_and_the_least_travel_kept() {
        // A column of squares: bottom to top travels least from home at the
        // origin; left to right would sort them by x, all equal, then by y.
        let parts: Vec<Path> = [60.0, 0.0, 30.0]
            .iter()
            .map(|y| square(0.0, *y, 10.0, y + 10.0))
            .chain(std::iter::once(square(100.0, 200.0, 110.0, 210.0)))
            .collect();
        let starts = |orders: Vec<CutOrder>| {
            let params = ProfileParams {
                orders,
                ..ProfileParams::default()
            };
            let moves = profile(&parts, &JobSetup::default(), &tool(), &params, tol())
                .expect("profile")
                .moves;
            let mut seen: Vec<Point> = Vec::new();
            for m in moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
                let p = m.to.xy();
                if seen.last().is_none_or(|q| q.distance(p) > 20.0) {
                    seen.push(p);
                }
            }
            let total: f64 = std::iter::once(Point::ORIGIN)
                .chain(seen.iter().copied())
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .sum();
            (seen, total)
        };
        let (selection, _) = starts(vec![CutOrder::SelectionOrder]);
        assert!(selection[0].y > 50.0, "{selection:?}");
        let (both, travel) = starts(vec![CutOrder::SelectionOrder, CutOrder::BottomToTop]);
        let (bottom, bottom_travel) = starts(vec![CutOrder::BottomToTop]);
        assert_eq!(both, bottom);
        assert!(both[0].y < 10.0, "{both:?}");
        assert!((travel - bottom_travel).abs() < 1e-9);
    }

    #[test]
    fn sharp_corners_run_out_to_the_corner_point() {
        let params = ProfileParams {
            sharp_corners: true,
            ..ProfileParams::default()
        };
        let moves = run(&params);
        assert!(!moves.iter().any(|m| m.kind.is_arc()));
        assert!(deep_points(&moves, -6.0)
            .iter()
            .any(|p| (p.x + 3.0).abs() < 1e-6 && (p.y + 3.0).abs() < 1e-6));
    }

    #[test]
    fn old_requests_without_a_lead_still_parse() {
        let params: ProfileParams =
            serde_json::from_value(serde_json::json!({ "cutDepth": 4.0 })).expect("parse");
        assert_eq!(params.lead, LeadSpec::default());
        assert!(!params.lead.is_active());
        assert!(params.lead.lead_out);
        assert_eq!(params.order, CutOrder::Optimized);
        assert!(!params.last_pass.enabled);
    }
    #[test]
    fn the_allowance_moves_the_cut_away_from_the_line_on_the_part_side() {
        // Outside: the tool radius and the allowance further out, so the part
        // is left 0.5 too big all round; inside: the same amount further in.
        let out = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &ProfileParams {
                allowance: 0.5,
                ..ProfileParams::default()
            },
            tol(),
        )
        .expect("profile");
        let (x0, y0, x1, y1) = cutting_xy_box(&out.moves, -6.0);
        assert!(
            (x0 + 3.5).abs() < 1e-6
                && (x1 - 53.5).abs() < 1e-6
                && (y0 + 3.5).abs() < 1e-6
                && (y1 - 33.5).abs() < 1e-6,
            "{x0} {y0} {x1} {y1}"
        );
        let inside = profile(
            &[square(0.0, 0.0, 50.0, 30.0)],
            &JobSetup::default(),
            &tool(),
            &ProfileParams {
                side: ProfileSide::Inside,
                allowance: 0.5,
                ..ProfileParams::default()
            },
            tol(),
        )
        .expect("profile");
        let (x0, y0, x1, y1) = cutting_xy_box(&inside.moves, -6.0);
        assert!(
            (x0 - 3.5).abs() < 1e-6
                && (x1 - 46.5).abs() < 1e-6
                && (y0 - 3.5).abs() < 1e-6
                && (y1 - 26.5).abs() < 1e-6,
            "{x0} {y0} {x1} {y1}"
        );
        // With a separate last pass the rough passes add its skin to the
        // allowance and the last pass cuts at the allowance alone.
        let skinned = run(&ProfileParams {
            allowance: 0.5,
            last_pass: LastPass {
                enabled: true,
                allowance: 0.2,
                reverse: false,
            },
            ..ProfileParams::default()
        });
        let (x0, _, _, _) = cutting_xy_box(&skinned, -3.0);
        assert!((x0 + 3.7).abs() < 1e-6, "{x0}");
        let finish = deep_points(&skinned, -6.0)
            .into_iter()
            .filter(|p| (from_rect(p.xy()) - 3.5).abs() < 1e-6)
            .count();
        assert!(finish >= 4, "a last pass at 3.5 out");
    }

    #[test]
    fn the_last_pass_runs_the_other_way_round_only_when_asked() {
        // The signed area of the loop a pass traces, from its points in
        // cutting order: negative clockwise.
        let loop_area = |moves: &[Move], distance: f64| {
            let pts: Vec<Point3> = moves
                .iter()
                .filter(|m| {
                    m.kind.is_cutting()
                        && (m.to.z + 6.0).abs() < 1e-9
                        && (from_rect(m.to.xy()) - distance).abs() < 1e-6
                })
                .map(|m| m.to)
                .collect();
            assert!(pts.len() >= 4, "a pass {distance} out");
            let mut twice = 0.0;
            for w in pts.windows(2) {
                twice += w[0].x * w[1].y - w[1].x * w[0].y;
            }
            twice
        };
        for reverse in [false, true] {
            let moves = run(&ProfileParams {
                last_pass: LastPass {
                    enabled: true,
                    allowance: 0.5,
                    reverse,
                },
                ..ProfileParams::default()
            });
            let rough = loop_area(&moves, 3.5);
            let last = loop_area(&moves, 3.0);
            assert!(rough < 0.0, "climb runs clockwise outside: {rough}");
            assert_eq!(
                last < 0.0,
                !reverse,
                "reverse {reverse}: rough {rough}, last {last}"
            );
        }
    }
}
