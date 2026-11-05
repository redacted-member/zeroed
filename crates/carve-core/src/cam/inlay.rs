//! V-Groove inlays: one set of vectors and one V shaped tool make a female
//! pocket and a male plug that fits it.
//!
//! The female is a V-Groove of the shapes from the surface down, flat at the
//! pocket depth. The male is cut in a second board as the negative: the
//! area round the shapes, inside a boundary grown out from them, is V-Grooved
//! so that the shapes stand up as a plug. The plug is mirrored left to right
//! (it is turned face down to glue in), so its outline matches the pocket.
//!
//! How the two meet, with `D` the pocket depth, `G` the glue gap and `P` the
//! plug depth, all millimetres:
//!
//! * The female's V walls pass through the outline at the surface (start
//!   depth 0) and its floor is `D` deep.
//! * The male is carved with start depth `D - G`, so its walls pass through
//!   the outline `D - G` below the male board's surface, and with flat
//!   depth `P` below that start, so the waste round the plug is cleared
//!   `D - G + P` deep.
//! * Turned over and pushed in, the plug's walls touch the pocket's walls
//!   where the male start plane lies on the female surface. The plug's top
//!   (the male board's surface) then sits `D - (D - G) = G` above the
//!   pocket floor, which is the room left for glue, and the male board's
//!   cleared floor stands `P` above the female surface, so the boards meet
//!   on the walls, never on the floors. `P` is the Surface
//!   Gap and defaults to `D`.
//!
//! The plug's outer boundary is the shapes grown out by a boundary offset
//! (Vector Offset), or the whole sheet (Sheet Limits), so the plug board is
//! cut down from the design out to the sheet edge.

use serde::{Deserialize, Serialize};

use crate::doc::{Document, Entity, EntityKind, JobSetup};
use crate::error::CoreError;
use crate::geom::{path_bbox, rect, transform_path, BBox, Mat3, Path, Region, Tolerance};

use super::pocket::{pocket, rest_pocket, ClearanceOptions};
use super::tool::Tool;
use super::vgroove::{
    v_shape, vgroove_with_clearance, ClearanceTool, VGrooveOutput, VGrooveParams,
    FLATS_NEED_A_POCKET,
};

/// Extra room, in millimetres, the default boundary leaves past what the
/// plug walls need.
const BOUNDARY_MARGIN: f64 = 2.0;

/// Which halves of the inlay to calculate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InlayPart {
    /// Only the pocket.
    Female,
    /// Only the plug.
    Male,
    /// The pocket and the plug.
    #[default]
    Both,
}

/// Where the plug's waste ends (Plug Outer Boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlugBoundary {
    /// Out to the edges of the sheet.
    SheetLimits,
    /// The shapes grown out by the boundary offset.
    #[default]
    VectorOffset,
}

/// The Inlay form's settings. Depths and lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InlayParams {
    /// Depth of the female pocket's flat floor below the surface.
    pub pocket_depth: f64,
    /// Room left for glue between the plug's top and the pocket's floor.
    /// The male is carved with start depth `pocket_depth - glue_gap`, so it
    /// must not be more than the pocket depth.
    pub glue_gap: f64,
    /// Surface Gap: depth the male's waste is cleared below its
    /// start depth, which is the gap left between the plug board and the
    /// pocket board's surface. Absent means the pocket depth.
    pub plug_depth: Option<f64>,
    /// Plug Outer Boundary: the sheet edges or a vector offset.
    pub plug_boundary: PlugBoundary,
    /// Mirror the plug left to right about the centre of the design's
    /// bounding box, since it is glued in face down.
    pub mirror_plug: bool,
    /// Distance the male's outer boundary is grown out from the shapes.
    /// Absent means wide enough for the plug walls to reach the plug depth
    /// on both sides of the V (`2 * plug_depth * tan(half angle)` plus the
    /// flat tip), at least the tool radius, plus 2 mm.
    pub boundary_offset: Option<f64>,
    /// Boundary Offset of the form: how much further than the plug walls
    /// need the Vector Offset boundary reaches (with `boundary_offset`
    /// absent). Absent means 2 mm.
    pub boundary_extra: Option<f64>,
    /// Most the V may go down in one pass, for both halves.
    pub max_pass_depth: Option<f64>,
    /// Tool that clears the flat areas of both halves. Older files keep
    /// their single tool here; the form sends `clearance_tools`.
    pub clearance_tool: Option<Tool>,
    /// How the clearance tools clear, for a tool without its own options.
    pub clearance_options: ClearanceOptions,
    /// Clearance tools in cutting order for both halves: the first clears
    /// all it can, each later one what the tools before it could not reach.
    pub clearance_tools: Vec<ClearanceTool>,
    /// Which halves to calculate.
    pub part: InlayPart,
}

impl Default for InlayParams {
    fn default() -> Self {
        InlayParams {
            pocket_depth: 5.0,
            glue_gap: 2.5,
            plug_depth: None,
            plug_boundary: PlugBoundary::VectorOffset,
            mirror_plug: true,
            boundary_offset: None,
            boundary_extra: None,
            max_pass_depth: None,
            clearance_tool: None,
            clearance_options: ClearanceOptions::default(),
            clearance_tools: Vec::new(),
            part: InlayPart::Both,
        }
    }
}

impl InlayParams {
    /// The clearance tools in cutting order, the old single tool when the
    /// list is empty.
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

/// The two halves of an inlay. A half not asked for is `None`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlayOutput {
    /// The pocket: its V-Groove and the clearance of its flat floor.
    pub female: Option<VGrooveOutput>,
    /// The plug: the V-Groove round it and the clearance of its waste.
    pub male: Option<VGrooveOutput>,
}

fn positive(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "the {what} must be more than zero"
        )))
    }
}

/// Bounding box of every vector in `paths`.
fn design_bbox(paths: &[Path]) -> Option<BBox> {
    paths.iter().filter_map(path_bbox).reduce(|a, b| a.union(b))
}

/// `paths` mirrored left to right about the centre of their bounding box.
pub fn mirror_design(paths: &[Path]) -> Vec<Path> {
    let Some(bbox) = design_bbox(paths) else {
        return paths.to_vec();
    };
    let m = Mat3::scale_about(-1.0, 1.0, bbox.centre());
    paths.iter().map(|p| transform_path(p, &m)).collect()
}

/// The area the male's V-Groove works in: inside the shapes grown out by
/// `boundary` millimetres, outside the shapes themselves. Only the grown
/// outer loops bound it, so letter counters are carved right across.
fn plug_waste(shapes: &Region, boundary: f64, tol: Tolerance) -> Region {
    let grown = shapes.offset(-boundary, tol);
    let mut loops: Vec<_> = grown.outers().cloned().collect();
    loops.extend(shapes.as_path().subpaths);
    Region::from_paths(&[Path::from_subpaths(loops)], tol)
}

/// Calculates a V-Groove inlay from the closed vectors in `paths` with one V
/// shaped `tool`: the female pocket, the male plug, or both, as
/// `params.part` asks. See the module documentation for how the depths
/// relate.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped, a
/// pocket depth, plug depth, boundary offset or pass depth that is not more
/// than zero, a glue gap that is negative or deeper than the pocket, when
/// there are no closed vectors, and whatever the V-Groove or the clearance
/// pocket refuses.
pub fn inlay(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &InlayParams,
    tol: Tolerance,
) -> Result<InlayOutput, CoreError> {
    let (half, flat_tip) = v_shape(tool)?;
    let slope = half.tan();
    positive(params.pocket_depth, "pocket depth")?;
    // A glue gap of zero is refused: a real inlay always needs room for glue.
    if !params.glue_gap.is_finite() || params.glue_gap <= 0.0 {
        return Err(CoreError::validation("the glue gap must be more than zero"));
    }
    if params.glue_gap > params.pocket_depth {
        return Err(CoreError::validation(
            "the glue gap must not be more than the pocket depth",
        ));
    }
    let plug_depth = params.plug_depth.unwrap_or(params.pocket_depth);
    positive(plug_depth, "plug depth")?;
    if let Some(offset) = params.boundary_offset {
        positive(offset, "boundary offset")?;
    }
    if let Some(pass) = params.max_pass_depth {
        positive(pass, "pass depth")?;
    }
    let shapes = Region::from_paths(paths, tol);
    if shapes.is_empty() {
        return Err(CoreError::validation(
            "an inlay needs closed vectors, such as text or shapes",
        ));
    }

    let mut out = InlayOutput::default();
    if params.part != InlayPart::Male {
        let female = VGrooveParams {
            start_depth: 0.0,
            flat_depth: Some(params.pocket_depth),
            max_pass_depth: params.max_pass_depth,
            clearance_tool: None,
            clearance_options: params.clearance_options,
            clearance_tools: params.clearance_list(),
            ..VGrooveParams::default()
        };
        out.female = Some(vgroove_with_clearance(paths, job, tool, &female, tol)?);
    }
    if params.part != InlayPart::Female {
        out.male = Some(male(
            paths, job, tool, params, plug_depth, slope, flat_tip, tol,
        )?);
    }
    Ok(out)
}

/// The plug: a V-Groove of the waste round the (mirrored) shapes, and the
/// clearance of the waste's flat areas from the surface down.
#[allow(clippy::too_many_arguments)]
fn male(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &InlayParams,
    plug_depth: f64,
    slope: f64,
    flat_tip: f64,
    tol: Tolerance,
) -> Result<VGrooveOutput, CoreError> {
    let design = if params.mirror_plug {
        mirror_design(paths)
    } else {
        paths.to_vec()
    };
    let shapes = Region::from_paths(&design, tol);
    let wall = plug_depth * slope + flat_tip / 2.0;
    let needed = 2.0 * wall;
    let extra = params.boundary_extra.unwrap_or(BOUNDARY_MARGIN).max(0.0);
    let boundary = params
        .boundary_offset
        .unwrap_or_else(|| needed.max(tool.radius()) + extra);
    let waste = match params.plug_boundary {
        PlugBoundary::VectorOffset => plug_waste(&shapes, boundary, tol),
        PlugBoundary::SheetLimits => {
            let sheet = job.material_bounds();
            let mut loops = rect(sheet.min, sheet.max)?.subpaths;
            loops.extend(shapes.as_path().subpaths);
            Region::from_paths(&[Path::from_subpaths(loops)], tol)
        }
    };
    if waste.is_empty() {
        return Err(CoreError::validation(
            "there is no room round the shapes to carve the plug",
        ));
    }
    let start = params.pocket_depth - params.glue_gap;
    let carve = VGrooveParams {
        start_depth: start,
        flat_depth: Some(plug_depth),
        max_pass_depth: params.max_pass_depth,
        clearance_tool: None,
        ..VGrooveParams::default()
    };
    let waste_path = waste.as_path();
    let mut done =
        vgroove_with_clearance(std::slice::from_ref(&waste_path), job, tool, &carve, tol)?;

    // The waste is cleared from the surface, not from the start depth, so
    // that nothing is left standing above the start plane; each later tool
    // clears only what the smallest tool before it could not reach.
    let list = params.clearance_list();
    if let Some((first, rest)) = list.split_first() {
        let flats = waste.offset(wall, tol);
        let options = |entry: &ClearanceTool| {
            entry
                .options
                .unwrap_or(params.clearance_options)
                .pocket_params(0.0, start + plug_depth)
        };
        if !flats.is_empty() && !flats.offset(first.tool.radius(), tol).is_empty() {
            let cleared = pocket(&[flats.as_path()], job, &first.tool, &options(first), tol)?;
            done.vgroove.warnings.retain(|w| !w.is(FLATS_NEED_A_POCKET));
            done.clearance = Some(cleared);
            let mut reached = first.tool.radius();
            for (index, entry) in rest.iter().enumerate() {
                let tool = &entry.tool;
                tool.validate()?;
                if tool.radius() >= reached - 1e-9 {
                    done.vgroove
                        .warnings
                        .push(crate::warning::Warning::no_smaller_tool(&tool.name));
                    continue;
                }
                match rest_pocket(&flats, reached, job, tool, &options(entry), tol)? {
                    Some(cut) if cut.moves.iter().any(|m| m.kind.is_cutting()) => {
                        done.later.push((index, cut, tool.clone()));
                    }
                    _ => done
                        .vgroove
                        .warnings
                        .push(crate::warning::Warning::nothing_left(&tool.name)),
                }
                reached = reached.min(tool.radius());
            }
        }
    }
    if params.plug_boundary == PlugBoundary::VectorOffset && boundary + 1e-9 < needed {
        done.vgroove
            .warnings
            .push(crate::warning::Warning::inlay_boundary_small());
    }
    Ok(done)
}

/// Plug Destination Sheet: copies of the design vectors, mirrored left to
/// right about the design's middle, on `sheet` (the active sheet when
/// `None`). On the design's own sheet they stay where they are, mirrored;
/// on another sheet they are moved to the middle of that sheet's material.
/// Answers the new vectors' ids, which the plug toolpath then machines.
///
/// # Errors
/// [`CoreError::Validation`] for an unknown vector or sheet, or when the
/// ids hold no vectors.
pub fn plug_vectors(
    doc: &mut Document,
    ids: &[u64],
    sheet: Option<u64>,
) -> Result<Vec<u64>, CoreError> {
    let mut sources: Vec<(u64, u64, Path)> = Vec::new();
    for id in ids {
        let entity = doc.entity(*id)?;
        let (layer, from) = (entity.layer, entity.sheet);
        for (_, path) in doc.paths_of(*id) {
            sources.push((layer, from, path));
        }
    }
    let paths: Vec<Path> = sources.iter().map(|(_, _, p)| p.clone()).collect();
    let bbox = design_bbox(&paths)
        .ok_or_else(|| CoreError::validation("select the inlay vectors first"))?;
    let target = sheet.unwrap_or_else(|| doc.active_sheet());
    let material = doc.sheet_material(target)?.applied_to(doc.job());
    let same = sources.iter().all(|(_, from, _)| *from == target);
    let mirror = Mat3::scale_about(-1.0, 1.0, bbox.centre());
    let place = if same {
        mirror
    } else {
        let centre = material.material_bounds().centre();
        mirror.then(&Mat3::translate(bbox.centre().to(centre)))
    };
    let mut made = Vec::with_capacity(sources.len());
    for (layer, _, path) in sources {
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet: target,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: transform_path(&path, &place),
            },
        });
        made.push(id);
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::ToolGeometry;
    use crate::cam::toolpath::Move;
    use crate::cam::vgroove::vgroove_with_clearance;
    use crate::geom::{circle, rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn vbit() -> Tool {
        Tool {
            name: "V".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
            ..Tool::default()
        }
    }

    fn cutting(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind.is_cutting()).collect()
    }

    fn shallowest_cut(moves: &[Move]) -> f64 {
        cutting(moves)
            .iter()
            .map(|m| m.to.z)
            .fold(f64::MIN, f64::max)
    }

    fn deepest(moves: &[Move]) -> f64 {
        moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min)
    }

    /// An L shape, not symmetric left to right, from x 10 to 30.
    fn ell() -> Path {
        let mut sub = crate::geom::SubPath::open(Point::new(10.0, 0.0));
        sub.line_to(Point::new(30.0, 0.0))
            .line_to(Point::new(30.0, 8.0))
            .line_to(Point::new(18.0, 8.0))
            .line_to(Point::new(18.0, 30.0))
            .line_to(Point::new(10.0, 30.0))
            .line_to(Point::new(10.0, 0.0));
        sub.closed = true;
        sub.into_path()
    }

    #[test]
    fn the_female_is_a_vgroove_flat_at_the_pocket_depth() {
        let square = rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect");
        let params = InlayParams {
            part: InlayPart::Female,
            pocket_depth: 3.0,
            clearance_tool: Some(Tool::default()),
            ..InlayParams::default()
        };
        let done = inlay(
            std::slice::from_ref(&square),
            &JobSetup::default(),
            &vbit(),
            &params,
            tol(),
        )
        .expect("inlay");
        assert!(done.male.is_none());
        let want = vgroove_with_clearance(
            &[square],
            &JobSetup::default(),
            &vbit(),
            &VGrooveParams {
                flat_depth: Some(3.0),
                clearance_tool: Some(Tool::default()),
                ..VGrooveParams::default()
            },
            tol(),
        )
        .expect("vgroove");
        assert_eq!(done.female, Some(want));
    }

    #[test]
    fn the_plug_is_mirrored_about_the_design_centre() {
        let params = InlayParams {
            part: InlayPart::Male,
            boundary_offset: Some(10.0),
            ..InlayParams::default()
        };
        let plain = InlayParams {
            mirror_plug: false,
            ..params.clone()
        };
        let job = JobSetup::default();
        let mirrored = inlay(&[ell()], &job, &vbit(), &params, tol())
            .expect("inlay")
            .male
            .expect("male");
        let straight = inlay(&[ell()], &job, &vbit(), &plain, tol())
            .expect("inlay")
            .male
            .expect("male");
        let a = cutting(&mirrored.vgroove.moves);
        let b = cutting(&straight.vgroove.moves);
        assert!(!a.is_empty());
        // The x extents of the cuts flip about x = 20.
        let extent = |cuts: &[&Move]| {
            cuts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), m| {
                (lo.min(m.to.x), hi.max(m.to.x))
            })
        };
        let (a_lo, a_hi) = extent(&a);
        let (b_lo, b_hi) = extent(&b);
        assert!((a_lo - (40.0 - b_hi)).abs() < 0.05, "{a_lo} {b_hi}");
        assert!((a_hi - (40.0 - b_lo)).abs() < 0.05, "{a_hi} {b_lo}");
        // The L's long leg is on the left unmirrored and on the right
        // mirrored: no cut inside the mirrored leg (x 22 to 30, y 8 to 30).
        for m in &a {
            let in_leg = m.to.x > 22.5 && m.to.x < 29.5 && m.to.y > 8.5 && m.to.y < 29.5;
            assert!(!in_leg, "cut inside the plug: {m:?}");
        }
        assert!(b
            .iter()
            .any(|m| m.to.x > 22.5 && m.to.x < 29.5 && m.to.y > 8.5 && m.to.y < 29.5));
    }

    #[test]
    fn the_plug_carve_stays_out_of_the_shape() {
        let c = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let params = InlayParams {
            part: InlayPart::Male,
            boundary_offset: Some(8.0),
            ..InlayParams::default()
        };
        let done = inlay(&[c], &JobSetup::default(), &vbit(), &params, tol())
            .expect("inlay")
            .male
            .expect("male");
        let cuts = cutting(&done.vgroove.moves);
        assert!(!cuts.is_empty());
        for m in cuts {
            let r = m.to.xy().distance(Point::new(0.0, 0.0));
            assert!((9.95..=18.05).contains(&r), "{m:?} at radius {r}");
        }
    }

    #[test]
    fn the_glue_gap_starts_the_plug_shallower() {
        let c = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let job = JobSetup::default();
        // The ring round the plug is 12 mm wide, so a 90 degree V goes 6 mm
        // below the start depth along its middle, short of the plug depth.
        let bottom = |gap: f64| {
            let params = InlayParams {
                part: InlayPart::Male,
                pocket_depth: 5.0,
                glue_gap: gap,
                plug_depth: Some(10.0),
                boundary_offset: Some(12.0),
                ..InlayParams::default()
            };
            let done = inlay(std::slice::from_ref(&c), &job, &vbit(), &params, tol())
                .expect("inlay")
                .male
                .expect("male");
            deepest(&done.vgroove.moves)
        };
        let small = bottom(1.0);
        let big = bottom(2.0);
        assert!((small + 4.0 + 6.0).abs() < 0.15, "{small}");
        assert!((big + 3.0 + 6.0).abs() < 0.15, "{big}");
    }

    #[test]
    fn the_plug_is_flat_at_the_plug_depth_below_the_start() {
        let c = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let params = InlayParams {
            part: InlayPart::Male,
            pocket_depth: 5.0,
            glue_gap: 1.0,
            boundary_offset: Some(12.0),
            ..InlayParams::default()
        };
        let done = inlay(&[c], &JobSetup::default(), &vbit(), &params, tol())
            .expect("inlay")
            .male
            .expect("male");
        assert!((deepest(&done.vgroove.moves) + 4.0 + 5.0).abs() < 1e-6);
        assert!(shallowest_cut(&done.vgroove.moves) >= -9.0 - 1e-6);
    }

    #[test]
    fn the_male_clearance_starts_at_the_surface() {
        let c = circle(Point::new(0.0, 0.0), 10.0).expect("circle");
        let params = InlayParams {
            part: InlayPart::Male,
            pocket_depth: 4.0,
            glue_gap: 1.0,
            plug_depth: Some(2.0),
            boundary_offset: Some(20.0),
            clearance_tool: Some(Tool::default()),
            ..InlayParams::default()
        };
        let done = inlay(&[c], &JobSetup::default(), &vbit(), &params, tol())
            .expect("inlay")
            .male
            .expect("male");
        let clearance = done.clearance.expect("clearance");
        assert!((deepest(&clearance.moves) + 5.0).abs() < 1e-9);
        // Default tool pass depth 3 mm: the first level is above the start.
        assert!(cutting(&clearance.moves)
            .iter()
            .any(|m| (m.to.z + 2.5).abs() < 1e-9));
        assert!(
            done.vgroove.warnings.is_empty(),
            "{:?}",
            done.vgroove.warnings
        );
    }

    #[test]
    fn both_halves_by_default() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let done = inlay(
            &[square],
            &JobSetup::default(),
            &vbit(),
            &InlayParams::default(),
            tol(),
        )
        .expect("inlay");
        assert!(done.female.is_some() && done.male.is_some());
    }

    #[test]
    fn bad_tools_and_depths_are_refused() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let job = JobSetup::default();
        let refused = |tool: &Tool, params: InlayParams| {
            matches!(
                inlay(std::slice::from_ref(&square), &job, tool, &params, tol()),
                Err(CoreError::Validation(_))
            )
        };
        assert!(refused(&Tool::default(), InlayParams::default()));
        let base = InlayParams::default();
        assert!(refused(
            &vbit(),
            InlayParams {
                pocket_depth: -1.0,
                ..base.clone()
            }
        ));
        assert!(refused(
            &vbit(),
            InlayParams {
                glue_gap: -0.5,
                ..base.clone()
            }
        ));
        assert!(refused(
            &vbit(),
            InlayParams {
                glue_gap: 0.0,
                ..base.clone()
            }
        ));
        assert!(refused(
            &vbit(),
            InlayParams {
                glue_gap: 6.0,
                ..base.clone()
            }
        ));
        assert!(refused(
            &vbit(),
            InlayParams {
                plug_depth: Some(0.0),
                ..base.clone()
            }
        ));
        assert!(refused(
            &vbit(),
            InlayParams {
                boundary_offset: Some(-2.0),
                ..base
            }
        ));
    }

    #[test]
    fn params_use_camel_case_names() {
        let json = serde_json::to_value(InlayParams::default()).expect("json");
        for key in [
            "pocketDepth",
            "glueGap",
            "plugDepth",
            "mirrorPlug",
            "boundaryOffset",
            "maxPassDepth",
            "clearanceTool",
            "part",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["part"], "both");
        let back: InlayParams =
            serde_json::from_str(r#"{"pocketDepth":3,"part":"male"}"#).expect("parse");
        assert_eq!(back.part, InlayPart::Male);
        assert!(back.mirror_plug);
    }

    #[test]
    fn a_plug_to_the_sheet_limits_is_cut_out_to_the_sheet_edge() {
        let square = rect(Point::new(40.0, 40.0), Point::new(60.0, 60.0)).expect("rect");
        let job = JobSetup {
            width: 100.0,
            height: 100.0,
            ..JobSetup::default()
        };
        let reach = |boundary: PlugBoundary, extra: Option<f64>| {
            let params = InlayParams {
                boundary_extra: extra,
                part: InlayPart::Male,
                pocket_depth: 3.0,
                glue_gap: 0.2,
                plug_depth: Some(2.0),
                plug_boundary: boundary,
                ..InlayParams::default()
            };
            let out =
                inlay(std::slice::from_ref(&square), &job, &vbit(), &params, tol()).expect("inlay");
            let male = out.male.expect("male");
            cutting(&male.vgroove.moves)
                .iter()
                .map(|m| (m.to.x - 50.0).abs().max((m.to.y - 50.0).abs()))
                .fold(0.0, f64::max)
        };
        let offset = reach(PlugBoundary::VectorOffset, Some(0.0));
        let sheet = reach(PlugBoundary::SheetLimits, None);
        assert!(offset < 30.0, "vector offset reaches {offset}");
        assert!(sheet > 45.0, "sheet limits reach {sheet}");
    }

    fn mill(diameter: f64) -> Tool {
        Tool {
            name: format!("End Mill {diameter}"),
            tool_type: ToolGeometry::EndMill { diameter },
            pass_depth: 3.0,
            stepover: diameter * 0.4,
            ..Tool::default()
        }
    }

    fn listed(tool: Tool) -> ClearanceTool {
        ClearanceTool {
            tool,
            corner_sharpen: false,
            options: None,
        }
    }

    #[test]
    fn both_halves_clear_with_the_tool_list_in_order() {
        // A square with a sharp inside corner region the big tool cannot reach.
        let square = rect(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).expect("rect");
        let base = InlayParams {
            pocket_depth: 3.0,
            glue_gap: 0.5,
            plug_depth: Some(2.0),
            boundary_offset: Some(20.0),
            clearance_tools: vec![listed(mill(12.0)), listed(mill(3.0))],
            ..InlayParams::default()
        };
        let female = inlay(
            std::slice::from_ref(&square),
            &JobSetup::default(),
            &vbit(),
            &InlayParams {
                part: InlayPart::Female,
                ..base.clone()
            },
            tol(),
        )
        .expect("inlay")
        .female
        .expect("female");
        assert!(female.clearance.is_some());
        assert_eq!(female.later.len(), 1, "{:?}", female.vgroove.warnings);
        // Round the plug the waste fills the L's inside corner.
        let male = inlay(
            &[ell()],
            &JobSetup::default(),
            &vbit(),
            &InlayParams {
                part: InlayPart::Male,
                ..base
            },
            tol(),
        )
        .expect("inlay")
        .male
        .expect("male");
        assert!(male.clearance.is_some());
        assert_eq!(male.later.len(), 1, "{:?}", male.vgroove.warnings);
        let (index, cut, tool) = &male.later[0];
        assert_eq!((*index, tool.name.as_str()), (0, "End Mill 3"));
        // The small tool cuts no deeper than the waste floor.
        assert!(
            deepest(&cut.moves) >= -4.5 - 1e-9,
            "{}",
            deepest(&cut.moves)
        );
    }

    #[test]
    fn plug_vectors_are_mirrored_in_place_or_centered_on_another_sheet() {
        let mut doc = Document::new();
        doc.begin("Create");
        let ell_id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path: ell() },
        });
        doc.commit();
        let here = plug_vectors(&mut doc, &[ell_id], None).expect("plug");
        assert_eq!(here.len(), 1);
        let path = doc.paths_of(here[0]).remove(0).1;
        let bbox = path_bbox(&path).expect("bbox");
        assert!((bbox.min.x - 10.0).abs() < 1e-9 && (bbox.max.x - 30.0).abs() < 1e-9);
        // The foot of the L now points left: its corner at x 30 moved to 10.
        let foot = path.subpaths[0].start;
        assert!((foot.x - 30.0).abs() < 1e-9, "{foot:?}");
        let other = doc.add_sheet(None);
        let there = plug_vectors(&mut doc, &[ell_id], Some(other)).expect("plug");
        assert_eq!(doc.entity(there[0]).expect("made").sheet, other);
        let moved = path_bbox(&doc.paths_of(there[0]).remove(0).1).expect("bbox");
        let centre = doc
            .sheet_material(other)
            .expect("sheet")
            .applied_to(doc.job())
            .material_bounds()
            .centre();
        assert!(moved.centre().distance(centre) < 1e-9);
        assert!(plug_vectors(&mut doc, &[999], None).is_err());
    }
}
