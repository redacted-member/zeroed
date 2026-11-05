//! Turns a toolpath form's request into a calculated [`Toolpath`]: resolves
//! the selected vectors, runs the strategy and fills in heights, feeds,
//! time estimate and the record needed to recalculate it later.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::{Path, Region, Tolerance};
use crate::warning::Warning;

use super::chamfer::{chamfer, ChamferParams};
use super::common::Heights;
use super::drill::{drill, DrillParams};
use super::engrave::{engrave, EngraveParams};
use super::estimate::TimeEstimate;
use super::filter::filter_moves;
use super::fluting::{fluting, FlutingParams};
use super::inlay::{inlay, InlayParams, InlayPart};
use super::keepout::{keep_outs, zones_entered};
use super::keyhole::{keyhole, KeyholeParams};
use super::laser::{
    laser_cut_fill, laser_picture, laser_picture_tone, speed_of, LaserCutFillParams,
    LaserPictureParams,
};
use super::moulding::{moulding_with_clearance, MouldingParams};
use super::photo_carve::{photo_carve, PhotoCarveParams};
use super::pocket::{pocket_tool_list, pocket_with_clearance, TwoToolPocketParams};
use super::prism::{prism, PrismParams};
use super::profile::{profile, Calculated, ProfileParams};
use super::project::{project_moves_with, PROJECT_STEP};
use super::rounding::{rounding, RoundingParams};
use super::sim::Footprint;
use super::sketch::{laser_sketch, sketch_carve, uses_model, LaserSketchParams, SketchCarveParams};
use super::straight_inlay::{straight_inlay, StraightInlayParams};
use super::surface::{
    finish, finish_within, rough, FinishParams, RestParams, RoughParams, Surface,
};
use super::texture::{texture, TextureParams};
use super::thread::{thread_mill, ThreadParams};
use super::tool::Tool;
use super::toolpath::{StrategyKind, Toolpath};
use super::vgroove::{vgroove_with_clearance, VGrooveParams};

/// Tolerance bezier spans are flattened to for cutting, in millimetres.
pub const CUT_TOLERANCE: f64 = 0.01;

/// The strategies this build calculates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Strategy {
    /// Along vectors.
    Profile,
    /// Clearing inside vectors.
    Pocket,
    /// Holes at vector centres.
    Drill,
    /// A V cutter along the centre line of shapes.
    #[serde(rename = "vgroove")]
    VGroove,
    /// An engraving tool along the vectors, optionally filling them.
    QuickEngrave,
    /// Level by level clearing over the relief model.
    #[serde(rename = "rough3d")]
    Rough3d,
    /// Surface following raster over the relief model.
    #[serde(rename = "finish3d")]
    Finish3d,
    /// A V shaped bevel along the edges of vectors.
    Chamfer,
    /// Flutes along vectors whose depth ramps in and out.
    Fluting,
    /// A V cutter carving shapes as sharp ridged prisms, sunk or raised.
    Prism,
    /// Short random strokes filling shapes for a tooled look.
    Texture,
    /// A V-Groove pocket and the matching plug, one toolpath per part.
    Inlay,
    /// A profile swept along a rail: the first vector is the rail, the
    /// second the profile.
    Moulding,
    /// Laser cutting along vectors and hatch filling inside them.
    LaserCutFill,
    /// A laser raster burning a bitmap.
    LaserPicture,
    /// A V cutter carving the sketch lines of a bitmap or the model.
    SketchCarve,
    /// A laser hatching the sketch lines of a bitmap or the model.
    LaserSketch,
    /// A thread mill cutting a helix round the centre of each circle.
    ThreadMilling,
    /// A straight walled insert or the pocket or hole it fits.
    StraightInlay,
    /// A rotary blank turned down to a finished diameter.
    Rounding,
    /// A photograph carved as grooves deeper where it is darker.
    #[serde(rename = "photoCarve")]
    PhotoCarve,
    /// Keyhole hanging slots from the centre of each vector.
    Keyhole,
    /// A bank of drills lowered together over matching circles.
    DrillBank,
}

impl Strategy {
    /// True for strategies that machine the relief model, which need no
    /// selected vectors unless they are limited to them.
    fn is_3d(self) -> bool {
        matches!(self, Strategy::Rough3d | Strategy::Finish3d)
    }

    /// True for strategies that need selected vectors.
    pub fn needs_vectors(self) -> bool {
        !self.is_3d()
            && !matches!(
                self,
                Strategy::LaserPicture
                    | Strategy::SketchCarve
                    | Strategy::LaserSketch
                    | Strategy::Rounding
            )
    }

    /// True for strategies that machine the area inside closed vectors,
    /// whose vectors are repaired first when they cross themselves.
    fn machines_areas(self) -> bool {
        matches!(
            self,
            Strategy::Profile
                | Strategy::Pocket
                | Strategy::VGroove
                | Strategy::Chamfer
                | Strategy::Prism
                | Strategy::Texture
                | Strategy::Inlay
                | Strategy::LaserCutFill
                | Strategy::StraightInlay
        )
    }

    fn is_laser(self) -> bool {
        matches!(
            self,
            Strategy::LaserCutFill | Strategy::LaserPicture | Strategy::LaserSketch
        )
    }
}

impl Strategy {
    /// The toolpath kind this strategy makes.
    pub fn kind(self) -> StrategyKind {
        match self {
            Strategy::Profile => StrategyKind::Profile,
            Strategy::Pocket => StrategyKind::Pocket,
            Strategy::Drill => StrategyKind::Drill,
            Strategy::VGroove => StrategyKind::VGroove,
            Strategy::QuickEngrave => StrategyKind::QuickEngrave,
            Strategy::Rough3d => StrategyKind::Rough3d,
            Strategy::Finish3d => StrategyKind::Finish3d,
            Strategy::Chamfer => StrategyKind::Chamfer,
            Strategy::Fluting => StrategyKind::Fluting,
            Strategy::Prism => StrategyKind::PrismCarve,
            Strategy::Texture => StrategyKind::Texture,
            Strategy::Inlay => StrategyKind::Inlay,
            Strategy::Moulding => StrategyKind::Moulding,
            Strategy::LaserCutFill | Strategy::LaserPicture | Strategy::LaserSketch => {
                StrategyKind::Laser
            }
            Strategy::SketchCarve => StrategyKind::VGroove,
            Strategy::ThreadMilling => StrategyKind::ThreadMilling,
            Strategy::StraightInlay => StrategyKind::StraightInlay,
            Strategy::Rounding => StrategyKind::Rotary,
            Strategy::PhotoCarve => StrategyKind::PhotoCarve,
            Strategy::Keyhole => StrategyKind::Keyhole,
            Strategy::DrillBank => StrategyKind::Drill,
        }
    }

    fn default_name(self) -> &'static str {
        match self {
            Strategy::Profile => "Profile",
            Strategy::Pocket => "Pocket",
            Strategy::Drill => "Drill",
            Strategy::VGroove => "V-Groove",
            Strategy::QuickEngrave => "Quick Engrave",
            Strategy::Rough3d => "3D Roughing",
            Strategy::Finish3d => "3D Finish",
            Strategy::Chamfer => "Chamfer",
            Strategy::Fluting => "Fluting",
            Strategy::Prism => "Prism Carve",
            Strategy::Texture => "Texture",
            Strategy::Inlay => "VGroove Inlay",
            Strategy::Moulding => "Swept Profile",
            Strategy::LaserCutFill => "Laser Cut",
            Strategy::LaserPicture => "Laser Picture",
            Strategy::SketchCarve => "Sketch Carve",
            Strategy::LaserSketch => "Laser Sketch",
            Strategy::ThreadMilling => "Thread",
            Strategy::StraightInlay => "Inlay",
            Strategy::Rounding => "Rounding",
            Strategy::PhotoCarve => "Photo Carve",
            Strategy::Keyhole => "Keyhole Toolpath",
            Strategy::DrillBank => "Drill Bank",
        }
    }
}

/// What a toolpath form sends to calculate or recalculate a toolpath.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Which strategy.
    pub strategy: Strategy,
    /// Name for the toolpath list; a numbered default when absent.
    #[serde(default)]
    pub name: Option<String>,
    /// The tool, as picked from the database.
    pub tool: Tool,
    /// The strategy's parameters, in its own shape.
    #[serde(default)]
    pub params: Value,
    /// Top level vectors to machine.
    pub ids: Vec<u64>,
}

impl Request {
    /// The request a calculated toolpath was made from, so it can be edited
    /// and recalculated.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a toolpath that was not made by
    /// a strategy this build knows.
    pub fn of(toolpath: &Toolpath) -> Result<Request, CoreError> {
        let record = &toolpath.params_json;
        let strategy: Strategy = serde_json::from_value(record["strategy"].clone())
            .map_err(|_| CoreError::validation("this toolpath cannot be recalculated"))?;
        let ids = record["ids"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        // A clearance toolpath keeps the request of the toolpath it clears
        // for, whose tool is recorded beside it.
        let tool = match record.get("mainTool") {
            Some(main) => serde_json::from_value(main.clone())
                .map_err(|_| CoreError::validation("this toolpath cannot be recalculated"))?,
            None => toolpath.tool.clone(),
        };
        // A toolpath of a two tool strategy carries its tool's name in its
        // own; the request keeps the name without it, unless the user has
        // renamed the toolpath since.
        let name = match record.get("baseName").and_then(Value::as_str) {
            Some(base) if toolpath.name == tool_inclusive_name(base, &toolpath.tool) => {
                base.to_string()
            }
            _ => toolpath.name.clone(),
        };
        Ok(Request {
            strategy,
            name: Some(name),
            tool,
            params: record["params"].clone(),
            ids,
        })
    }
}

fn parse<T: for<'de> Deserialize<'de> + Default>(value: &Value) -> Result<T, CoreError> {
    if value.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(value.clone()).map_err(|error| {
        CoreError::validation(format!("the toolpath settings do not read: {error}"))
    })
}

/// Calculates the toolpath a request asks for. The toolpath id is left at
/// zero for the document to assign.
///
/// # Errors
/// Returns [`CoreError::Validation`] when nothing was selected, the tool or
/// settings are not valid, and whatever the strategy reports.
pub fn calculate(doc: &Document, request: &Request) -> Result<(Toolpath, Vec<Warning>), CoreError> {
    let done = calculate_parts(doc, request)?;
    Ok((done.main, done.warnings))
}

/// The name a toolpath of a two tool strategy gets: its own name with the
/// name of the tool it cuts with, so that the two are told apart in the
/// toolpath list.
/// `base N`, with N one more than the highest number a name `base N`
/// (optionally followed by more text, such as a tool name) uses.
pub fn next_name<'a>(base: &str, names: impl IntoIterator<Item = &'a str>) -> String {
    let prefix = format!("{base} ");
    let highest = names
        .into_iter()
        .filter_map(|name| name.strip_prefix(prefix.as_str()))
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            let after = rest[digits.len()..].chars().next();
            if digits.is_empty() || after.is_some_and(|c| !c.is_whitespace()) {
                None
            } else {
                digits.parse::<u64>().ok()
            }
        })
        .max()
        .unwrap_or(0);
    format!("{base} {}", highest + 1)
}

pub fn tool_inclusive_name(base: &str, tool: &Tool) -> String {
    format!("{base} [{}]", tool.name)
}

/// Whether a toolpath is the clearance half of a two tool strategy.
pub fn is_clearance(toolpath: &Toolpath) -> bool {
    toolpath.params_json.get("part").and_then(Value::as_str) == Some("clearance")
}

/// For a later tool of a multi tool toolpath, its place among the rest
/// tools (from 0); `None` for any other toolpath.
pub fn rest_index(toolpath: &Toolpath) -> Option<usize> {
    if toolpath.params_json.get("part").and_then(Value::as_str) != Some("rest") {
        return None;
    }
    toolpath.params_json["restIndex"]
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
}

/// The id of the first toolpath of the multi tool family a toolpath
/// belongs to, or `None` when it stands alone.
pub fn family(toolpath: &Toolpath) -> Option<u64> {
    toolpath.params_json.get("family").and_then(Value::as_u64)
}

/// A multi tool strategy's later tools and how they share the work: each
/// machines only what the tools before it could not reach.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MultiTool {
    /// The tools after the request's own, in cutting order.
    rest_tools: Vec<Tool>,
    #[serde(flatten)]
    rest: RestParams,
}

/// The later tools of a pocket's tool list, after the request's own tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PocketToolList {
    /// The tools after the request's own, in cutting order.
    rest_tools: Vec<Tool>,
}

/// A calculated request: the toolpath asked for, and for strategies given
/// a second, larger tool, the toolpath that tool cuts first.
#[derive(Debug)]
pub struct Calculation {
    /// The toolpath with the request's own tool.
    pub main: Toolpath,
    /// The clearance tool's toolpath, when one was asked for and fits.
    pub clearance: Option<Toolpath>,
    /// For a multi tool strategy, a toolpath for each later tool that has
    /// something left to cut, in cutting order after `main`, with its
    /// place in the tool list (from 0, the first tool after the main one).
    pub rest: Vec<(usize, Toolpath)>,
    /// Messages for the user about all of them.
    pub warnings: Vec<Warning>,
}

/// Calculates a request with its clearance toolpath, if it has one.
///
/// # Errors
/// As [`calculate`].
pub fn calculate_parts(doc: &Document, request: &Request) -> Result<Calculation, CoreError> {
    let mut resolved = super::selector::resolve(doc, request)?;
    if resolved.strategy == Strategy::Profile && resolved.params.is_object() {
        super::tabs::anchor_positions(doc, &resolved.ids, &mut resolved.params)?;
    }
    // An inlay insert or hole keeps its placed tabs in its Profile options.
    if resolved.strategy == Strategy::StraightInlay && resolved.params["profileOptions"].is_object()
    {
        let ids = resolved.ids.clone();
        super::tabs::anchor_positions(doc, &ids, &mut resolved.params["profileOptions"])?;
    }
    let request = &resolved;
    if request.ids.is_empty() && request.strategy.needs_vectors() {
        return Err(CoreError::validation(
            "select the vectors to machine before calculating",
        ));
    }
    request.tool.validate()?;
    let mut leaves = Vec::new();
    let mut paths: Vec<Path> = Vec::new();
    for id in &request.ids {
        doc.entity(*id)?;
        for (leaf, path) in doc.paths_of(*id) {
            leaves.push(leaf);
            paths.push(path);
        }
    }
    let tol = Tolerance::new(CUT_TOLERANCE)?;
    let job = doc.job();
    let settings = *doc.cam_settings();
    let mut fix_notes = Vec::new();
    if request.strategy.machines_areas() {
        let limit = std::time::Duration::from_secs(u64::from(settings.geometry_fix_seconds));
        let fixed = super::fix::fix_crossings(paths, limit, tol);
        fix_notes = super::fix::fix_warnings(&fixed, limit);
        paths = fixed.paths;
    }
    let mut clearance: Option<(Calculated, Tool)> = None;
    let mut rest: Vec<(usize, Calculated, Tool)> = Vec::new();
    let mut rest_warnings: Vec<Warning> = Vec::new();
    let mut firings: Vec<super::toolpath::BankFiring> = Vec::new();
    let mut slopes: Vec<super::taper::SlopeCut> = Vec::new();
    let done = match request.strategy {
        Strategy::Profile => {
            let params = parse::<ProfileParams>(&request.params)?;
            if params.wall_angle == 0.0 {
                profile(&paths, job, &request.tool, &params, tol)?
            } else {
                let (done, found) =
                    super::taper::tapered(&paths, job, &request.tool, &params, tol)?;
                slopes = found;
                done
            }
        }
        Strategy::Pocket => {
            let mut params = parse::<TwoToolPocketParams>(&request.params)?;
            params.pocket.strips = settings.new_raster;
            let list = parse::<PocketToolList>(&request.params)?;
            if params.clearance_tool.is_none() && !list.rest_tools.is_empty() {
                for tool in &list.rest_tools {
                    tool.validate()?;
                }
                // The tool list: the request's tool first, then the rest.
                let out = pocket_tool_list(
                    &paths,
                    job,
                    &request.tool,
                    &list.rest_tools,
                    &params.pocket,
                    tol,
                )?;
                for (index, (cut, tool)) in out.later.into_iter().zip(&list.rest_tools).enumerate()
                {
                    match cut {
                        Some(cut) => rest.push((index, cut, tool.clone())),
                        None => rest_warnings.push(Warning::nothing_left(&tool.name)),
                    }
                }
                out.first
            } else {
                let out = pocket_with_clearance(&paths, job, &request.tool, &params, tol)?;
                if let (Some(cleared), Some(tool)) = (out.clearance, params.clearance_tool) {
                    clearance = Some((cleared, tool));
                }
                out.pocket
            }
        }
        Strategy::Drill => drill(&paths, job, &parse::<DrillParams>(&request.params)?)?,
        Strategy::Rough3d | Strategy::Finish3d => {
            if doc.components().is_empty() {
                return Err(CoreError::validation(
                    "there is no model to machine: create a shape in the Modeling tab first",
                ));
            }
            let surface = Surface::of(doc);
            let selection = Region::from_paths(&paths, tol);
            let selection = (!selection.is_empty()).then_some(&selection);
            if request.strategy == Strategy::Rough3d {
                let params = parse::<RoughParams>(&request.params)?;
                rough(&surface, job, &request.tool, &params, selection, tol)?
            } else {
                let params = parse::<FinishParams>(&request.params)?;
                let multi = parse::<MultiTool>(&request.params)?;
                let first = finish(&surface, job, &request.tool, &params, selection, tol)?;
                // Rest machining: each later tool finishes only where it
                // reaches deeper than every tool before it.
                for (index, tool) in multi.rest_tools.iter().enumerate() {
                    tool.validate()?;
                    let earlier: Vec<&Tool> = std::iter::once(&request.tool)
                        .chain(multi.rest_tools[..index].iter())
                        .collect();
                    let cells = surface.rest_cells(&earlier, tool, &multi.rest)?;
                    if !cells.iter().any(|c| *c) {
                        rest_warnings.push(Warning::nothing_left(&tool.name));
                        continue;
                    }
                    match finish_within(&surface, job, tool, &params, selection, Some(&cells), tol)
                    {
                        Ok(cut) if cut.moves.iter().any(|m| m.kind.is_cutting()) => {
                            rest.push((index, cut, tool.clone()));
                        }
                        Ok(_) => rest_warnings.push(Warning::nothing_left(&tool.name)),
                        Err(error) => return Err(error),
                    }
                }
                first
            }
        }
        Strategy::Chamfer => chamfer(
            &paths,
            job,
            &request.tool,
            &parse::<ChamferParams>(&request.params)?,
            tol,
        )?,
        Strategy::Fluting => fluting(
            &paths,
            job,
            &request.tool,
            &parse::<FlutingParams>(&request.params)?,
            tol,
        )?,
        Strategy::Prism => prism(
            &paths,
            job,
            &request.tool,
            &parse::<PrismParams>(&request.params)?,
            tol,
        )?,
        Strategy::Texture => texture(
            &paths,
            job,
            &request.tool,
            &parse::<TextureParams>(&request.params)?,
            tol,
        )?,
        Strategy::Moulding => {
            let params = parse::<MouldingParams>(&request.params)?;
            let (finish, rough) =
                moulding_with_clearance(&paths, job, &request.tool, &params, tol)?;
            if let (Some(rough), Some(tool)) = (rough, params.clearance_tool) {
                clearance = Some((rough, tool));
            }
            finish
        }
        Strategy::Inlay => {
            let params = parse::<InlayParams>(&request.params)?;
            if params.part == InlayPart::Both {
                return Err(CoreError::validation(
                    "an inlay toolpath is either the pocket or the plug; calculate them one at a time",
                ));
            }
            let out = inlay(&paths, job, &request.tool, &params, tol)?;
            let half = out.female.or(out.male).ok_or_else(|| {
                CoreError::validation("the inlay gave nothing to cut for that part")
            })?;
            if let (Some(cleared), Some(first)) = (half.clearance, params.clearance_list().first())
            {
                clearance = Some((cleared, first.tool.clone()));
            }
            // Later clearance tools follow as the rest of the family.
            rest.extend(half.later);
            half.vgroove
        }
        Strategy::LaserCutFill => laser_cut_fill(
            &paths,
            job,
            &request.tool,
            &parse::<LaserCutFillParams>(&request.params)?,
            tol,
        )?,
        Strategy::LaserPicture => {
            let params = parse::<LaserPictureParams>(&request.params)?;
            let bitmap = request.ids.iter().copied().find(|id| {
                doc.entity(*id)
                    .is_ok_and(|e| matches!(e.kind, crate::doc::EntityKind::Bitmap(_)))
            });
            match bitmap {
                // The bitmap selected in the drawing, as placed, with the
                // other selected vectors to burn along.
                Some(id) => {
                    let (tone, _) = crate::cam::sketch::placed_bitmap(doc, id)?;
                    let vectors: Vec<Path> = request
                        .ids
                        .iter()
                        .filter(|other| **other != id)
                        .flat_map(|other| doc.paths_of(*other).into_iter().map(|(_, p)| p))
                        .collect();
                    laser_picture_tone(&tone, &vectors, job, &request.tool, &params, tol)?
                }
                None if !params.path.as_os_str().is_empty() => {
                    let image = crate::io::bitmap::read(&params.path)?;
                    laser_picture(&image, job, &request.tool, &params)?
                }
                None => {
                    return Err(CoreError::validation(
                        "select the bitmap to burn before calculating",
                    ))
                }
            }
        }
        Strategy::SketchCarve => sketch_carve(
            doc,
            &paths,
            &request.tool,
            &parse::<SketchCarveParams>(&request.params)?,
            tol,
        )?,
        Strategy::LaserSketch => laser_sketch(
            doc,
            &paths,
            &request.tool,
            &parse::<LaserSketchParams>(&request.params)?,
            tol,
        )?,
        Strategy::StraightInlay => {
            let params = parse::<StraightInlayParams>(&request.params)?;
            let out = straight_inlay(&paths, job, &request.tool, &params, tol)?;
            if let (Some(cleared), Some(tool)) = (out.clearance, params.clearance_tool) {
                clearance = Some((cleared, tool));
            }
            // A pocket's later tools follow as the rest of the family.
            for (index, (cut, tool)) in out.later.into_iter().zip(&params.rest_tools).enumerate() {
                match cut {
                    Some(cut) => rest.push((index, cut, tool.clone())),
                    None => rest_warnings.push(Warning::nothing_left(&tool.name)),
                }
            }
            out.main
        }
        Strategy::ThreadMilling => thread_mill(
            &paths,
            job,
            &request.tool,
            &parse::<ThreadParams>(&request.params)?,
        )?,
        Strategy::Rounding => rounding(
            job,
            &request.tool,
            &parse::<RoundingParams>(&request.params)?,
        )?,
        Strategy::Keyhole => keyhole(&paths, job, &parse::<KeyholeParams>(&request.params)?)?,
        Strategy::DrillBank => {
            let cut = super::drill_bank::drill_bank(
                &paths,
                job,
                &parse::<super::drill_bank::DrillBankParams>(&request.params)?,
            )?;
            firings = cut.firings;
            cut.calculated
        }
        Strategy::PhotoCarve => photo_carve(
            doc,
            &request.ids,
            &request.tool,
            &parse::<PhotoCarveParams>(&request.params)?,
            tol,
        )?,
        Strategy::QuickEngrave => {
            engrave(&paths, job, &parse::<EngraveParams>(&request.params)?, tol)?
        }
        Strategy::VGroove => {
            let mut params = parse::<VGrooveParams>(&request.params)?;
            params.v_clears_flats = true;
            let out = vgroove_with_clearance(&paths, job, &request.tool, &params, tol)?;
            if let (Some(cleared), Some(first)) = (out.clearance, params.clearance_list().first()) {
                clearance = Some((cleared, first.tool.clone()));
            }
            // Later clearance tools follow as the rest of the family.
            rest.extend(out.later);
            out.vgroove
        }
    };
    // Any flat strategy can be dropped onto the relief model.
    let project = !request.strategy.is_3d()
        && request
            .params
            .get("projectToModel")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let surface = if project {
        if doc.components().is_empty() {
            return Err(CoreError::validation(
                "there is no model to project onto: create a shape in the Modeling tab first",
            ));
        }
        Some(Surface::of(doc))
    } else {
        None
    };
    let laser_speed = if request.strategy.is_laser() {
        let speed = request.params.get("speed").and_then(Value::as_f64);
        Some(speed_of(&request.tool, speed)?)
    } else {
        None
    };
    let heights = Heights::from_job(job);
    let drop_footprint = match (&surface, settings.drop_tool) {
        (Some(surface), true) => Some(Footprint::of(&request.tool, surface.grid().cell)?),
        _ => None,
    };
    let filter_tolerance = if request.strategy.is_3d() || surface.is_some() {
        settings.tolerance_3d
    } else if matches!(
        request.strategy,
        Strategy::VGroove | Strategy::Inlay | Strategy::PhotoCarve | Strategy::Prism
    ) {
        settings.tolerance_vgroove
    } else {
        settings.tolerance_2d
    };
    let name = request
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            next_name(
                request.strategy.default_name(),
                doc.toolpaths()
                    .iter()
                    .filter(|t| !is_clearance(t) && rest_index(t).is_none())
                    .map(|t| t.name.as_str()),
            )
        });
    let build = |cut: Calculated, tool: &Tool, name: String| {
        let mut toolpath = Toolpath::new(0, name, tool.clone(), request.strategy.kind());
        let moves = match &surface {
            Some(surface) => project_moves_with(
                &cut.moves,
                heights.home,
                surface,
                drop_footprint.as_ref(),
                heights.surface,
                heights.safe,
                PROJECT_STEP.min(surface.grid().cell),
            ),
            None => cut.moves,
        };
        toolpath.moves = if request.strategy == Strategy::LaserPicture {
            // Every pixel keeps its own power.
            moves
        } else {
            filter_moves(&moves, heights.home, filter_tolerance)
        };
        toolpath.safe_z = heights.safe;
        toolpath.home = heights.home;
        toolpath.rapid_clearance_gap = job.machine.plunge_gap;
        if let Some(speed) = laser_speed {
            toolpath.feed = speed;
            toolpath.plunge = speed;
        }
        toolpath.params_json = json!({
            "strategy": request.strategy,
            "params": request.params,
            "ids": request.ids,
        });
        let sketch = matches!(
            request.strategy,
            Strategy::SketchCarve | Strategy::LaserSketch
        );
        if request.strategy.is_3d() || surface.is_some() || (sketch && uses_model(&request.params))
        {
            toolpath.params_json["model"] = json!(doc.model_fingerprint());
        }
        toolpath.sources = doc.source_refs(&leaves);
        toolpath.recompute_bounds();
        TimeEstimate::default().apply(&mut toolpath);
        toolpath
    };
    let mut warnings = fix_notes;
    warnings.extend(done.warnings.iter().cloned());
    warnings.extend(rest_warnings);
    let two_tools = clearance.is_some() || !rest.is_empty();
    let clearance = clearance.map(|(cut, tool)| {
        warnings.extend(cut.warnings.iter().cloned());
        let mut toolpath = build(cut, &tool, tool_inclusive_name(&name, &tool));
        toolpath.params_json["part"] = json!("clearance");
        toolpath.params_json["mainTool"] = json!(request.tool);
        toolpath.params_json["baseName"] = json!(name);
        toolpath
    });
    let rest: Vec<(usize, Toolpath)> = rest
        .into_iter()
        .map(|(index, cut, tool)| {
            warnings.extend(cut.warnings.iter().cloned());
            let mut toolpath = build(cut, &tool, tool_inclusive_name(&name, &tool));
            toolpath.params_json["part"] = json!("rest");
            toolpath.params_json["restIndex"] = json!(index);
            toolpath.params_json["mainTool"] = json!(request.tool);
            toolpath.params_json["baseName"] = json!(name);
            (index, toolpath)
        })
        .collect();
    let main = if two_tools {
        let mut main = build(
            done,
            &request.tool,
            tool_inclusive_name(&name, &request.tool),
        );
        main.params_json["baseName"] = json!(name);
        main
    } else {
        build(done, &request.tool, name)
    };
    let mut main = main;
    main.firings = firings;
    if surface.is_none() {
        main.slopes = slopes;
    }
    let zones = keep_outs(doc, tol);
    if !zones.is_empty() {
        let mut entered: Vec<String> = Vec::new();
        for (toolpath, tool) in std::iter::once((&main, &request.tool))
            .chain(clearance.as_ref().map(|c| (c, &c.tool)))
            .chain(rest.iter().map(|(_, r)| (r, &r.tool)))
        {
            let found = zones_entered(
                &toolpath.moves,
                toolpath.home,
                toolpath.safe_z,
                tool.radius(),
                &zones,
                tol,
            );
            for zone in found {
                if !entered.contains(&zone) {
                    entered.push(zone);
                }
            }
        }
        for zone in entered {
            warnings.push(Warning::keep_out(&zone.to_string()));
        }
    }
    // Carving never means to cut through; going below the material's
    // underside is a mistake worth saying.
    let carves = matches!(
        request.strategy,
        Strategy::VGroove
            | Strategy::Moulding
            | Strategy::Prism
            | Strategy::PhotoCarve
            | Strategy::SketchCarve
            | Strategy::Chamfer
            | Strategy::Fluting
            | Strategy::QuickEngrave
            | Strategy::Texture
    );
    if carves {
        let top = heights.surface;
        let deepest = std::iter::once(&main)
            .chain(clearance.as_ref())
            .chain(rest.iter().map(|(_, r)| r))
            .filter_map(|t| t.bounds.map(|b| b.min.z))
            .fold(f64::INFINITY, f64::min);
        let depth = top - deepest;
        if depth.is_finite() && depth > job.thickness + THROUGH_SLACK {
            warnings.push(Warning::cuts_through(depth, job.thickness));
        }
    }
    Ok(Calculation {
        main,
        clearance,
        rest,
        warnings,
    })
}

/// How far past the material's underside a carving may reach before it
/// counts as cutting through, mm.
const THROUGH_SLACK: f64 = 1e-3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{rect, Point};

    fn doc_with_square() -> (Document, u64) {
        let mut doc = Document::new();
        let path = rect(Point::new(0.0, 0.0), Point::new(40.0, 30.0)).expect("rect");
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        (doc, id)
    }

    fn request(strategy: Strategy, ids: Vec<u64>) -> Request {
        Request {
            strategy,
            name: None,
            tool: Tool {
                name: "End Mill 6 mm".to_string(),
                ..Tool::default()
            },
            params: Value::Null,
            ids,
        }
    }

    #[test]
    fn new_names_follow_the_highest_number_in_use() {
        assert_eq!(next_name("Profile", []), "Profile 1");
        assert_eq!(
            next_name(
                "Pocket",
                [
                    "Pocket 1",
                    "Pocket 4 [End Mill]",
                    "Pocket side",
                    "Pockets 9",
                    "Profile 7"
                ]
            ),
            "Pocket 5"
        );
        assert_eq!(Strategy::Finish3d.default_name(), "3D Finish");
        assert_eq!(Strategy::Moulding.default_name(), "Swept Profile");
    }

    #[test]
    fn a_request_becomes_a_named_toolpath_that_records_how_it_was_made() {
        let (doc, id) = doc_with_square();
        let (toolpath, warnings) =
            calculate(&doc, &request(Strategy::Profile, vec![id])).expect("calculates");
        assert!(warnings.is_empty());
        assert_eq!(toolpath.name, "Profile 1");
        assert!(!toolpath.moves.is_empty());
        assert!(toolpath.estimated_seconds > 0.0);
        assert!(toolpath.bounds.is_some());
        assert_eq!(toolpath.sources.len(), 1);
        let again = Request::of(&toolpath).expect("reads back");
        assert_eq!(again.strategy, Strategy::Profile);
        assert_eq!(again.ids, vec![id]);
    }

    #[test]
    fn a_v_carve_deeper_than_the_material_says_it_cuts_through() {
        // A 40 x 30 square in the 18 mm default material: a 60 degree bit
        // reaches 15 / tan 30 = 26 mm in its middle, a 90 degree one 15.
        let (doc, id) = doc_with_square();
        let carve = |angle: f64, params: Value| {
            let mut v = request(Strategy::VGroove, vec![id]);
            v.tool.tool_type = crate::cam::tool::ToolGeometry::VBit {
                diameter: 40.0,
                included_angle: angle,
            };
            v.params = params;
            let (_, warnings) = calculate(&doc, &v).expect("v-grooves");
            warnings.into_iter().find(|w| w.is("toolpath.cutsThrough"))
        };
        let through = carve(60.0, Value::Null).expect("cuts through");
        let depth = through.params["depth"].as_f64().unwrap_or(0.0);
        assert!(
            (depth - 15.0 / 30f64.to_radians().tan()).abs() < 0.1,
            "{depth}"
        );
        assert_eq!(through.params["thickness"], 18.0);
        assert!(
            carve(90.0, Value::Null).is_none(),
            "15 mm stays in the material"
        );
        assert!(
            carve(60.0, json!({ "flatDepth": 5.0 })).is_none(),
            "a flat depth keeps it in"
        );
        // A profile cut right through is meant, not warned about.
        let mut cut = request(Strategy::Profile, vec![id]);
        cut.params = json!({ "side": "outside", "cutDepth": 18.5 });
        let (_, warnings) = calculate(&doc, &cut).expect("profiles");
        assert!(!warnings.iter().any(|w| w.is("toolpath.cutsThrough")));
    }

    #[test]
    fn every_strategy_runs_on_a_closed_vector() {
        let (doc, id) = doc_with_square();
        for strategy in [
            Strategy::Profile,
            Strategy::Pocket,
            Strategy::Drill,
            Strategy::QuickEngrave,
        ] {
            let (toolpath, _) = calculate(&doc, &request(strategy, vec![id])).expect("calculates");
            assert!(!toolpath.moves.is_empty(), "{strategy:?}");
        }
        let mut v = request(Strategy::VGroove, vec![id]);
        v.tool.tool_type = crate::cam::tool::ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 90.0,
        };
        let (toolpath, _) = calculate(&doc, &v).expect("v-grooves");
        for strategy in [
            Strategy::Chamfer,
            Strategy::Fluting,
            Strategy::Prism,
            Strategy::Texture,
        ] {
            let mut r = request(strategy, vec![id]);
            r.tool.tool_type = v.tool.tool_type.clone();
            let (made, _) = calculate(&doc, &r).expect("calculates");
            assert_eq!(made.kind, strategy.kind());
            assert!(!made.moves.is_empty(), "{strategy:?}");
        }
        assert!(
            calculate(&doc, &request(Strategy::Finish3d, vec![])).is_err(),
            "no model to finish"
        );
        assert_eq!(toolpath.kind, StrategyKind::VGroove);
        assert_eq!(
            serde_json::to_value(Strategy::VGroove).ok(),
            Some(json!("vgroove"))
        );
    }

    #[test]
    fn inlay_parts_are_calculated_one_at_a_time() {
        let (doc, id) = doc_with_square();
        let mut r = request(Strategy::Inlay, vec![id]);
        r.tool.tool_type = crate::cam::tool::ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 60.0,
        };
        r.params = json!({ "pocketDepth": 3.0, "glueGap": 1.0 });
        assert!(
            calculate(&doc, &r).is_err(),
            "both parts at once is refused"
        );
        for part in ["female", "male"] {
            r.params["part"] = json!(part);
            let (made, _) = calculate(&doc, &r).expect("calculates");
            assert_eq!(made.kind, StrategyKind::Inlay);
            assert!(!made.moves.is_empty(), "{part}");
        }
    }

    #[test]
    fn a_moulding_follows_its_rail_with_the_profile() {
        let mut doc = Document::new();
        let rail =
            crate::geom::polyline_path(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], false)
                .expect("rail");
        let profile = crate::geom::polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 5.0),
                Point::new(20.0, 0.0),
            ],
            false,
        )
        .expect("profile");
        doc.begin("Create");
        let mut ids = Vec::new();
        for path in [rail, profile] {
            ids.push(doc.insert_entity(Entity {
                rotation: 0.0,
                id: 0,
                layer: doc.active_layer(),
                sheet: doc.active_sheet(),
                revision: 0,
                name: None,
                parent: None,
                kind: EntityKind::Path { path },
            }));
        }
        doc.commit();
        let mut r = request(Strategy::Moulding, ids);
        r.tool.tool_type = crate::cam::tool::ToolGeometry::BallNose { diameter: 6.0 };
        let (made, _) = calculate(&doc, &r).expect("calculates");
        assert_eq!(made.kind, StrategyKind::Moulding);
        assert!(!made.moves.is_empty());
    }

    #[test]
    fn a_pocket_with_a_larger_clearance_tool_makes_two_toolpaths() {
        let (doc, id) = doc_with_square();
        let mut p = request(Strategy::Pocket, vec![id]);
        p.tool.tool_type = crate::cam::tool::ToolGeometry::EndMill { diameter: 3.0 };
        let big = Tool {
            name: "End Mill 12 mm".to_string(),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter: 12.0 },
            ..Tool::default()
        };
        p.params = json!({ "cutDepth": 3.0, "clearanceTool": big });
        let done = calculate_parts(&doc, &p).expect("calculates");
        let clearance = done.clearance.expect("a 12 mm tool fits a 40 by 30 pocket");
        // Each half carries the name of the tool it cuts with.
        assert_eq!(clearance.name, "Pocket 1 [End Mill 12 mm]");
        assert_eq!(done.main.name, "Pocket 1 [End Mill 6 mm]");
        assert_eq!(clearance.tool.name, "End Mill 12 mm");
        // Reading the request back drops the tool name again.
        let back = Request::of(&done.main).expect("reads back");
        assert_eq!(back.name.as_deref(), Some("Pocket 1"));
        let mut renamed = done.main.clone();
        renamed.name = "Big pocket".to_string();
        let back = Request::of(&renamed).expect("reads back");
        assert_eq!(back.name.as_deref(), Some("Big pocket"));
        assert!(!done.main.moves.is_empty());
    }

    #[test]
    fn a_pocket_tool_list_makes_a_toolpath_for_each_tool_with_work_left() {
        let (doc, id) = doc_with_square();
        let mut p = request(Strategy::Pocket, vec![id]);
        p.tool.name = "End Mill 12 mm".to_string();
        p.tool.tool_type = crate::cam::tool::ToolGeometry::EndMill { diameter: 12.0 };
        let small = |name: &str, diameter: f64| Tool {
            name: name.to_string(),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter },
            ..Tool::default()
        };
        p.params = json!({
            "cutDepth": 3.0,
            "restTools": [small("End Mill 3 mm", 3.0), small("End Mill 6 mm", 6.0)],
        });
        let done = calculate_parts(&doc, &p).expect("calculates");
        assert!(done.clearance.is_none());
        assert_eq!(done.main.name, "Pocket 1 [End Mill 12 mm]");
        let names: Vec<(usize, &str)> = done
            .rest
            .iter()
            .map(|(i, t)| (*i, t.name.as_str()))
            .collect();
        // The 6 mm tool after the 3 mm one reaches no corner the 3 mm one
        // left, but as the last tool it removes the allowance the others
        // left on the walls.
        assert_eq!(
            names,
            vec![
                (0, "Pocket 1 [End Mill 3 mm]"),
                (1, "Pocket 1 [End Mill 6 mm]")
            ]
        );
        assert_eq!(rest_index(&done.rest[0].1), Some(0));
        assert_eq!(rest_index(&done.rest[1].1), Some(1));
    }

    #[test]
    fn a_flat_v_carve_with_a_clearance_tool_makes_two_toolpaths() {
        let (doc, id) = doc_with_square();
        let mut v = request(Strategy::VGroove, vec![id]);
        v.tool.tool_type = crate::cam::tool::ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 90.0,
        };
        let end_mill = request(Strategy::Pocket, vec![]).tool;
        v.params = json!({ "flatDepth": 2.0, "clearanceTool": end_mill });
        let done = calculate_parts(&doc, &v).expect("calculates");
        let clearance = done.clearance.expect("the end mill fits the square");
        assert_eq!(done.main.name, "V-Groove 1 [End Mill 6 mm]");
        assert_eq!(clearance.name, "V-Groove 1 [End Mill 6 mm]");
        assert_eq!(
            Request::of(&clearance).expect("reads").name.as_deref(),
            Some("V-Groove 1")
        );
        assert!(is_clearance(&clearance) && !is_clearance(&done.main));
        // The clearance toolpath recalculates from the V-Groove's request.
        let again = Request::of(&clearance).expect("reads back");
        assert_eq!(again.tool, v.tool);
        assert_eq!(again.strategy, Strategy::VGroove);
    }

    #[test]
    fn a_drill_bank_toolpath_keeps_its_firings() {
        let mut doc = Document::new();
        doc.begin("Create");
        let mut ids = Vec::new();
        for x in [20.0, 52.0, 84.0] {
            let path = crate::geom::circle(Point::new(x, 20.0), 2.5).expect("circle");
            ids.push(doc.insert_entity(Entity {
                rotation: 0.0,
                id: 0,
                layer: doc.active_layer(),
                sheet: doc.active_sheet(),
                revision: 0,
                name: None,
                parent: None,
                kind: EntityKind::Path { path },
            }));
        }
        doc.commit();
        let mut r = request(Strategy::DrillBank, ids);
        r.params = json!({ "drill": { "cutDepth": 12.0 } });
        let (made, _) = calculate(&doc, &r).expect("calculates");
        assert_eq!(made.name, "Drill Bank 1");
        assert_eq!(made.firings.len(), 1);
        assert_eq!(made.firings[0].drills, vec![1, 2, 3]);
    }

    #[test]
    fn laser_toolpaths_run_at_the_laser_speed_with_per_move_power() {
        let (doc, id) = doc_with_square();
        let mut r = request(Strategy::LaserCutFill, vec![id]);
        r.tool.tool_type = crate::cam::tool::ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1500.0,
            kerf: 0.1,
            max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
        };
        r.params = json!({ "mode": "cutAndFill", "lineInterval": 2.0, "fillPower": 40.0 });
        let (made, _) = calculate(&doc, &r).expect("calculates");
        assert_eq!(made.kind, StrategyKind::Laser);
        assert_eq!(made.name, "Laser Cut 1");
        assert!((made.feed - 1500.0).abs() < 1e-9);
        let powers: std::collections::BTreeSet<i64> = made
            .moves
            .iter()
            .filter_map(|m| m.power)
            .map(|p| p as i64)
            .collect();
        assert_eq!(powers.into_iter().collect::<Vec<_>>(), vec![40, 80]);
        r.params = json!({ "speed": 600.0 });
        let (slow, _) = calculate(&doc, &r).expect("calculates");
        assert!((slow.feed - 600.0).abs() < 1e-9);

        let mut picture = request(Strategy::LaserPicture, vec![]);
        picture.tool = r.tool.clone();
        picture.params = json!({ "path": "/nowhere/missing.png" });
        assert!(calculate(&doc, &picture).is_err(), "a missing image");
        let mut mill = request(Strategy::LaserCutFill, vec![id]);
        mill.params = Value::Null;
        assert!(
            calculate(&doc, &mill).is_err(),
            "an end mill is not a laser"
        );
    }

    #[test]
    fn a_projected_toolpath_follows_the_model_and_needs_one() {
        let (mut doc, id) = doc_with_square();
        let mut r = request(Strategy::QuickEngrave, vec![id]);
        r.params = json!({ "depth": 0.5, "projectToModel": true });
        assert!(calculate(&doc, &r).is_err(), "no model yet");
        let job = *doc.job();
        // A 2 mm slab over the right half of the job: the square sits on
        // the zero plane beside it, 2 mm below the material top.
        let grid = crate::model::grid_for(&job);
        let mut field =
            crate::model::field::Field::empty(grid.width, grid.height, grid.cell, grid.origin);
        for j in 0..grid.height {
            for i in grid.width / 2..grid.width {
                field.z[j * grid.width + i] = 2.0;
            }
        }
        doc.begin("Model");
        doc.add_component(crate::model::Component {
            id: 0,
            name: "Slab".to_string(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        });
        doc.commit();
        let (flat, _) = calculate(&doc, &request(Strategy::QuickEngrave, vec![id])).expect("flat");
        let (projected, _) = calculate(&doc, &r).expect("projects");
        let lowest = |t: &Toolpath| t.moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        assert!(projected.params_json.get("model").is_some());
        // Cuts are split to follow the model; the filter merges the pieces
        // again where the model is flat.
        assert!(projected.moves.len() >= flat.moves.len());
        let heights = Heights::from_job(&job);
        assert!((lowest(&flat) - (heights.surface - 0.5)).abs() < 1e-6);
        assert!(
            (lowest(&projected) - (heights.surface - 2.5)).abs() < 1e-3,
            "{}",
            lowest(&projected)
        );
    }

    #[test]
    fn a_toolpath_into_a_keep_out_zone_is_warned_about() {
        let (mut doc, id) = doc_with_square();
        let (_, clean) =
            calculate(&doc, &request(Strategy::Profile, vec![id])).expect("calculates");
        assert!(clean.iter().all(|w| !w.is("toolpath.keepOut")));
        doc.begin("Clamp");
        let layer = doc.add_layer(Some("Clamps".to_string()), None);
        doc.set_layer_keep_out(layer, true).expect("keep out");
        let clamp = rect(Point::new(35.0, 10.0), Point::new(50.0, 20.0)).expect("rect");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path: clamp },
        });
        doc.commit();
        let (_, warned) =
            calculate(&doc, &request(Strategy::Profile, vec![id])).expect("calculates");
        assert!(
            warned
                .iter()
                .any(|w| w.is("toolpath.keepOut") && w.params["zone"] == "Clamps"),
            "{warned:?}"
        );
    }

    #[test]
    fn nothing_selected_and_bad_settings_are_refused() {
        let (doc, id) = doc_with_square();
        assert!(calculate(&doc, &request(Strategy::Profile, vec![])).is_err());
        let mut bad = request(Strategy::Profile, vec![id]);
        bad.params = json!({ "cutDepth": "deep" });
        assert!(calculate(&doc, &bad).is_err());
        assert!(calculate(&doc, &request(Strategy::Profile, vec![999])).is_err());
    }

    #[test]
    fn a_vgroove_with_two_clearance_tools_makes_a_family_of_three() {
        let (doc, id) = doc_with_square();
        let mut r = request(Strategy::VGroove, vec![id]);
        r.tool = Tool {
            name: "V-Bit 90".to_string(),
            tool_type: crate::cam::tool::ToolGeometry::VBit {
                diameter: 25.0,
                included_angle: 90.0,
            },
            ..Tool::default()
        };
        let mill = |d: f64| Tool {
            name: format!("End Mill {d} mm"),
            tool_type: crate::cam::tool::ToolGeometry::EndMill { diameter: d },
            ..Tool::default()
        };
        r.name = Some("Sign".to_string());
        r.params = json!({
            "flatDepth": 2.0,
            "clearanceTools": [
                { "tool": mill(12.0) },
                { "tool": mill(3.0), "cornerSharpen": false },
            ],
        });
        let done = calculate_parts(&doc, &r).expect("calculates");
        let clearance = done.clearance.expect("the first clearance tool");
        assert_eq!(clearance.name, "Sign [End Mill 12 mm]");
        assert_eq!(done.rest.len(), 1);
        let (index, later) = &done.rest[0];
        assert_eq!(*index, 0);
        assert_eq!(later.name, "Sign [End Mill 3 mm]");
        assert_eq!(rest_index(later), Some(0));
        assert_eq!(done.main.name, "Sign [V-Bit 90]");
        // The later part recalculates from its own record.
        let again = Request::of(later).expect("request");
        assert_eq!(again.tool.name, "V-Bit 90");
        assert_eq!(again.name.as_deref(), Some("Sign"));
    }
}
