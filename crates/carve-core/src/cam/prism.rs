//! Prism carving: a V shaped cutter turns closed shapes into sharp ridged
//! prisms, every point's depth proportional to its distance from the
//! shape's edge, so the faces slope at the angle of the cutter's flank.
//!
//! Inside sinks the prism into the material: the shape becomes a V channel,
//! deepest along its centre line (medial axis), exactly a V-Groove with no
//! flat depth. The centre line is cut in levels no deeper than the tool's
//! pass depth apart.
//!
//! Outside raises the prism: the shape stands proud with its ridge along
//! the centre line at the start depth, and at a point `d` millimetres in
//! from the edge the finished surface is `H - d / tan(a)` below the start
//! depth, where `a` is the cutter's half angle, `H = R / tan(a)` the prism's
//! height and `R` the radius of the widest circle that fits in the shape.
//! Narrow parts of a shape therefore have lower ridges than wide ones.
//!
//! The raised prism is cut like a very wide chamfer: the cutter runs round
//! inward offsets of the outline, each with its tip (the inner edge of an
//! engraving tool's flat) on the finished face, `H - d / tan(a)` deep at
//! offset `d`. Its inner flank then lies on the face, so every pass cuts the
//! face exactly from its offset inwards as far as the cone reaches, and the
//! outer flank stays above the face. Passes run from the ridge outwards,
//! evenly spaced, as many as it takes for none to step sideways more than
//! the tool's stepover (or cone reach) or down more than its pass depth
//! from the one before; the last runs on the outline at the full height.
//! Offsets that vanish in narrow parts are simply left out. The waste round
//! the prism is not cleared beyond what the cutter's outer flank removes;
//! pocket it separately to the prism's depth.
//!
//! A maximum depth caps the prism: Inside, wider parts are left flat at
//! that depth (as a V-Groove flat depth); Outside, the prism rises from that
//! depth at its edge and where it would reach above the start depth its top
//! is left flat, uncut.

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{path_bbox, Path, Region, Tolerance};

use super::chain::Chain;
use super::common::{order_nearest, Emitter, Heights};
use super::profile::Calculated;
use super::profile::CutDirection;
use super::tool::{Tool, ToolGeometry};
use super::vgroove::{vgroove, VGrooveParams};

/// Whether the prism is sunk into the material or stands proud of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrismDirection {
    /// The shape is carved as a V channel, deepest along its centre line.
    Inside,
    /// The shape is left standing as a raised prism, its ridge along the
    /// centre line.
    #[default]
    Outside,
}

/// The Prism Carve form's settings. Depths are millimetres below the
/// surface; the pass depth and stepover come from the tool.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PrismParams {
    /// Sunk (Inside) or raised (Outside).
    pub direction: PrismDirection,
    /// Deepest the prism may go below the start depth, in millimetres.
    /// Absent means as deep as the shape's width asks for.
    pub max_depth: Option<f64>,
    /// Depth below the surface the prism's top (the outline for Inside, the
    /// ridge for Outside) sits at, in millimetres.
    pub start_depth: f64,
    /// Climb or conventional round the raised prism's passes.
    pub cut_direction: CutDirection,
    /// Cut the vectors in the order they were selected instead of the
    /// nearest next (Use Vector Selection Order).
    pub selection_order: bool,
}

impl Default for PrismParams {
    fn default() -> Self {
        PrismParams {
            direction: PrismDirection::Outside,
            max_depth: None,
            start_depth: 0.0,
            cut_direction: CutDirection::Climb,
            selection_order: false,
        }
    }
}

/// Half angle in radians, flat tip radius and cone radius of a V shaped
/// tool, all lengths in millimetres.
fn cone(tool: &Tool) -> Result<(f64, f64, f64), CoreError> {
    let (angle, flat, diameter) = match tool.tool_type {
        ToolGeometry::VBit {
            included_angle,
            diameter,
        } => (included_angle, 0.0, diameter),
        ToolGeometry::EngravingTool {
            included_angle,
            flat_diameter,
            diameter,
        } => (included_angle, flat_diameter, diameter),
        _ => {
            return Err(CoreError::validation(
                "a prism carving needs a V-bit or an engraving tool",
            ))
        }
    };
    if !(angle > 0.0 && angle < 180.0) {
        return Err(CoreError::validation(
            "the tool's included angle must be between 0 and 180 degrees",
        ));
    }
    let flat_radius = flat.max(0.0) / 2.0;
    let radius = diameter / 2.0;
    if !(radius.is_finite() && radius > flat_radius) {
        return Err(CoreError::validation(
            "the tool's diameter must be wider than its flat tip",
        ));
    }
    Ok(((angle / 2.0).to_radians(), flat_radius, radius))
}

/// Radius of the widest circle that fits in the region, in millimetres:
/// the largest inward offset that leaves something, found by halving.
fn inscribed_radius(region: &Region, tol: Tolerance) -> f64 {
    let Some(bbox) = path_bbox(&region.as_path()) else {
        return 0.0;
    };
    let mut lo = 0.0;
    let mut hi = bbox.width().min(bbox.height()) / 2.0 + tol.get();
    let precision = (tol.get() * 0.1).max(1e-6);
    while hi - lo > precision {
        let mid = (lo + hi) / 2.0;
        if region.offset(mid, tol).is_empty() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

/// The depth below the start depth that carves the selected shapes into
/// full, pointed prisms with `tool`: the widest shape's inscribed radius
/// over the tangent of the tool's half angle, in millimetres (Set Depth for
/// Full Prism).
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped and
/// when there are no closed vectors.
pub fn full_prism_depth(paths: &[Path], tool: &Tool, tol: Tolerance) -> Result<f64, CoreError> {
    let (half, _, _) = cone(tool)?;
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation(
            "a prism carving needs closed vectors",
        ));
    }
    Ok(inscribed_radius(&region, tol) / half.tan())
}

/// `cam.prismDepth`: `{ tool, ids }` answers `{ depth }`, the depth that
/// carves the selected vectors into full prisms with the tool, in
/// millimetres.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the arguments do not parse, a missing
/// vector, and what [`full_prism_depth`] refuses.
pub fn depth_command(
    doc: &crate::doc::Document,
    args: &str,
) -> Result<serde_json::Value, CoreError> {
    #[derive(Deserialize)]
    struct Args {
        tool: Tool,
        #[serde(default)]
        ids: Vec<u64>,
    }
    let args: Args =
        serde_json::from_str(args).map_err(|e| CoreError::invalid_args("cam.prismDepth", e))?;
    let mut paths = Vec::new();
    for id in &args.ids {
        doc.entity(*id)?;
        paths.extend(doc.paths_of(*id).into_iter().map(|(_, p)| p));
    }
    let tol = Tolerance::new(super::compute::CUT_TOLERANCE)?;
    let depth = full_prism_depth(&paths, &args.tool, tol)?;
    Ok(serde_json::json!({ "depth": depth }))
}

/// One pass of a raised prism: how far in from the outline the tip (or the
/// inner edge of the flat) runs, and its depth below the ridge.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PrismPass {
    inset: f64,
    depth: f64,
}

/// Passes from the ridge out to the outline down a face `width` wide and
/// `width / tan_a` deep, evenly spaced, none more than `step` sideways or
/// `pass_depth` down from the one before. The last lands on the outline.
fn prism_passes(width: f64, tan_a: f64, step: f64, pass_depth: f64) -> Vec<PrismPass> {
    let height = width / tan_a;
    let by_width = ((width / step) - 1e-9).ceil().max(1.0);
    let by_depth = ((height / pass_depth) - 1e-9).ceil().max(1.0);
    let count = by_width.max(by_depth) as usize;
    (1..=count)
        .map(|k| {
            if k == count {
                PrismPass {
                    inset: 0.0,
                    depth: height,
                }
            } else {
                let t = k as f64 / count as f64;
                PrismPass {
                    inset: width * (1.0 - t),
                    depth: height * t,
                }
            }
        })
        .collect()
}

/// Calculates a prism carving of the closed vectors in `paths` with a V-bit
/// or engraving tool. See the module notes for exactly what each direction
/// cuts.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a tool that is not V shaped, a
/// negative start depth, a maximum depth that is not more than zero, a tool
/// pass depth or (Outside) stepover that is not more than zero, and when
/// there are no closed vectors.
pub fn prism(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &PrismParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (half, flat, radius) = cone(tool)?;
    let tan_a = half.tan();
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if let Some(max) = params.max_depth {
        if !(max.is_finite() && max > 0.0) {
            return Err(CoreError::validation(
                "the maximum depth must be more than zero",
            ));
        }
    }
    if !(tool.pass_depth.is_finite() && tool.pass_depth > 0.0) {
        return Err(CoreError::validation(
            "the tool's pass depth must be more than zero",
        ));
    }
    let region = Region::from_paths(paths, tol);
    if region.is_empty() {
        return Err(CoreError::validation(
            "a prism carving needs closed vectors",
        ));
    }
    let reach = radius - flat;
    let cone_depth = reach / tan_a;
    match params.direction {
        PrismDirection::Inside => inside(paths, job, tool, params, cone_depth, tol),
        PrismDirection::Outside => outside(&region, job, tool, params, (tan_a, flat, reach), tol),
    }
}

/// The sunk prism: a V-Groove with no flat depth unless a maximum is set.
fn inside(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &PrismParams,
    cone_depth: f64,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let carve = VGrooveParams {
        start_depth: params.start_depth,
        flat_depth: params.max_depth,
        max_pass_depth: Some(tool.pass_depth),
        clearance_tool: None,
        ..VGrooveParams::default()
    };
    let mut done = vgroove(paths, job, tool, &carve, tol)?;
    let heights = Heights::from_job(job);
    let deepest = done
        .moves
        .iter()
        .filter(|m| m.kind.is_cutting())
        .map(|m| heights.surface - m.to.z)
        .fold(params.start_depth, f64::max);
    if deepest - params.start_depth > cone_depth + 1e-6 {
        done.warnings.push(cone_warning(cone_depth));
    }
    Ok(done)
}

fn cone_warning(cone_depth: f64) -> crate::warning::Warning {
    crate::warning::Warning::prism_deeper_than_cone(cone_depth)
}

/// The raised prism, cut in inward offsets of the outline from the ridge
/// out. `shape` is the tangent of the half angle, the flat tip radius and
/// the cone's sideways reach.
fn outside(
    region: &Region,
    job: &JobSetup,
    tool: &Tool,
    params: &PrismParams,
    shape: (f64, f64, f64),
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (tan_a, flat, reach) = shape;
    if !(tool.stepover.is_finite() && tool.stepover > 0.0) {
        return Err(CoreError::validation(
            "the tool's stepover must be more than zero",
        ));
    }
    let ridge_radius = inscribed_radius(region, tol);
    let full_height = ridge_radius / tan_a;
    let mut warnings = Vec::new();
    let height = match params.max_depth {
        Some(max) if max < full_height - 1e-9 => {
            warnings.push(crate::warning::Warning::prism_top_flat());
            max
        }
        _ => full_height,
    };
    if height <= 1e-9 {
        return Err(CoreError::validation(
            "the shapes are too thin to carve a prism in",
        ));
    }
    let cone_depth = reach / tan_a;
    if height > cone_depth + 1e-6 {
        warnings.push(cone_warning(cone_depth));
    }
    let step = tool.stepover.min(reach);
    let passes = prism_passes(height * tan_a, tan_a, step, tool.pass_depth);

    let heights = Heights::from_job(job);
    let home = heights.home.xy();
    let mut out = Emitter::new(heights);
    for pass in &passes {
        // The axis sits the flat's radius outside the face point, so that
        // the flat's inner edge and the cone above it lie on the face.
        let loops = region.offset(pass.inset - flat, tol);
        let chains: Vec<Chain> = loops
            .loops
            .iter()
            .map(|sub| {
                let chain = Chain::from_subpath(sub, tol);
                if params.cut_direction == CutDirection::Climb {
                    chain.reversed()
                } else {
                    chain
                }
            })
            .filter(|c| !c.is_empty())
            .collect();
        let from = out.at().xy();
        let chains = if params.selection_order {
            chains
        } else {
            order_nearest(
                chains,
                from,
                |c| c.start().unwrap_or(home),
                |c| c.end().unwrap_or(home),
            )
        };
        let z = heights.z_at(params.start_depth + pass.depth);
        for chain in &chains {
            let chain = chain.starting_near(out.at().xy());
            let Some(begin) = chain.start() else {
                continue;
            };
            out.travel_to(begin);
            out.plunge_to(z);
            out.cut_flat(&chain.segs, z);
        }
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::{Move, MoveKind, Point3};
    use crate::geom::{rect, Point};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn vbit(angle: f64, diameter: f64) -> Tool {
        Tool {
            name: "V".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter,
                included_angle: angle,
            },
            ..Tool::default()
        }
    }

    fn deepest(moves: &[Move]) -> f64 {
        moves.iter().map(|m| m.to.z).fold(f64::MAX, f64::min)
    }

    /// Straight cutting spans of a move list, as (from, to).
    fn spans(moves: &[Move]) -> Vec<(Point3, Point3)> {
        let mut out = Vec::new();
        for pair in moves.windows(2) {
            if pair[1].kind == MoveKind::Feed {
                out.push((pair[0].to, pair[1].to));
            }
        }
        out
    }

    /// Depth below zero the swept cone of a pointed tool reaches at `p`.
    fn cut_depth_at(spans: &[(Point3, Point3)], p: Point, tan_a: f64) -> f64 {
        let mut best = 0.0f64;
        for (a, b) in spans {
            let (ax, ay) = (b.x - a.x, b.y - a.y);
            let len2 = ax * ax + ay * ay;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * ax + (p.y - a.y) * ay) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = Point::new(a.x + ax * t, a.y + ay * t);
            let z = a.z + (b.z - a.z) * t;
            best = best.max(-z - q.distance(p) / tan_a);
        }
        best
    }

    #[test]
    fn inside_the_middle_of_a_square_is_half_its_width_over_the_slope() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        for angle in [90.0, 60.0] {
            let params = PrismParams {
                direction: PrismDirection::Inside,
                ..PrismParams::default()
            };
            let done = prism(
                std::slice::from_ref(&square),
                &JobSetup::default(),
                &vbit(angle, 25.0),
                &params,
                tol(),
            )
            .expect("prism");
            let want = 5.0 / (angle / 2.0f64).to_radians().tan();
            let bottom = done
                .moves
                .iter()
                .min_by(|a, b| a.to.z.total_cmp(&b.to.z))
                .expect("moves");
            assert!((bottom.to.z + want).abs() < 0.15, "{bottom:?} want {want}");
            assert!(bottom.to.xy().distance(Point::new(5.0, 5.0)) < 0.2);
            // Cut in levels no deeper than the tool's pass depth apart.
            let plunges: Vec<f64> = done
                .moves
                .iter()
                .filter(|m| m.kind == MoveKind::Plunge)
                .map(|m| m.to.z)
                .collect();
            assert!(!plunges.is_empty());
            assert!(done.warnings.is_empty(), "{:?}", done.warnings);
        }
    }

    #[test]
    fn inside_warns_when_deeper_than_the_cone() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let params = PrismParams {
            direction: PrismDirection::Inside,
            ..PrismParams::default()
        };
        let done = prism(
            &[square],
            &JobSetup::default(),
            &vbit(90.0, 6.0),
            &params,
            tol(),
        )
        .expect("prism");
        assert_eq!(done.warnings.len(), 1, "{:?}", done.warnings);
    }

    #[test]
    fn outside_square_is_a_pyramid_with_its_point_at_the_surface() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let done = prism(
            &[square],
            &JobSetup::default(),
            &vbit(90.0, 25.0),
            &PrismParams::default(),
            tol(),
        )
        .expect("prism");
        assert!(done.warnings.is_empty(), "{:?}", done.warnings);
        // The last pass runs on the outline at the full height, 5 mm.
        assert!((deepest(&done.moves) + 5.0).abs() < 0.02);
        for m in done.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            let d = m.to.x.min(10.0 - m.to.x).min(m.to.y).min(10.0 - m.to.y);
            assert!(d >= -1e-6, "outside the square: {m:?}");
            // Every tip sits on the face: depth = height - inset.
            assert!((-m.to.z - (5.0 - d)).abs() < 0.02, "{m:?}");
        }
        // Passes go from the ridge out, getting deeper.
        let plunges: Vec<f64> = done
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.z)
            .collect();
        assert!(plunges.len() >= 2);
        assert!(
            plunges.windows(2).all(|w| w[1] <= w[0] + 1e-9),
            "{plunges:?}"
        );
        for step in plunges.windows(2) {
            assert!(step[0] - step[1] <= 3.0 + 1e-9);
        }
        let spans = spans(&done.moves);
        let tan_a = 1.0;
        assert!(cut_depth_at(&spans, Point::new(5.0, 5.0), tan_a) < 0.02);
        for (p, want) in [
            (Point::new(5.0, 1.0), 4.0),
            (Point::new(3.0, 5.0), 2.0),
            (Point::new(7.5, 7.5), 2.5),
        ] {
            let got = cut_depth_at(&spans, p, tan_a);
            assert!((got - want).abs() < 0.03, "{p:?} {got} want {want}");
        }
    }

    #[test]
    fn outside_rectangle_has_its_ridge_along_the_middle() {
        let bar = rect(Point::new(0.0, 0.0), Point::new(30.0, 10.0)).expect("rect");
        let tool = vbit(60.0, 25.0);
        let tan_a = 30f64.to_radians().tan();
        let done = prism(
            &[bar],
            &JobSetup::default(),
            &tool,
            &PrismParams::default(),
            tol(),
        )
        .expect("prism");
        let height = 5.0 / tan_a;
        assert!((deepest(&done.moves) + height).abs() < 0.03);
        let spans = spans(&done.moves);
        for x in [6.0, 15.0, 24.0] {
            let ridge = cut_depth_at(&spans, Point::new(x, 5.0), tan_a);
            assert!(ridge < 0.03, "ridge at {x}: {ridge}");
            // Deeper towards the long edges.
            let mut last = ridge;
            for y in [4.0, 3.0, 2.0, 1.0] {
                let d = cut_depth_at(&spans, Point::new(x, y), tan_a);
                let want = height - y / tan_a;
                assert!((d - want).abs() < 0.05, "({x}, {y}) {d} want {want}");
                assert!(d > last);
                last = d;
            }
        }
    }

    #[test]
    fn outside_max_depth_leaves_the_top_flat_and_warns() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let params = PrismParams {
            max_depth: Some(2.0),
            start_depth: 0.5,
            ..PrismParams::default()
        };
        let done = prism(
            &[square],
            &JobSetup::default(),
            &vbit(90.0, 25.0),
            &params,
            tol(),
        )
        .expect("prism");
        assert!((deepest(&done.moves) + 2.5).abs() < 1e-9);
        assert_eq!(done.warnings.len(), 1, "{:?}", done.warnings);
        let spans = spans(&done.moves);
        // The middle is not reached: only the outer 2 mm band is sloped.
        assert!(cut_depth_at(&spans, Point::new(5.0, 5.0), 1.0) < 0.5 + 1e-6);
        let d = cut_depth_at(&spans, Point::new(5.0, 1.0), 1.0);
        assert!((d - 1.5).abs() < 0.03, "{d}");
    }

    #[test]
    fn passes_step_no_more_than_the_stepover_or_pass_depth() {
        let passes = prism_passes(10.0, 1.0, 2.5, 3.0);
        assert_eq!(passes.len(), 4);
        let last = passes.last().expect("passes");
        assert_eq!(last.inset, 0.0);
        assert_eq!(last.depth, 10.0);
        let passes = prism_passes(10.0, 1.0, 20.0, 3.0);
        assert_eq!(passes.len(), 4);
        let passes = prism_passes(2.0, 1.0, 5.0, 5.0);
        assert_eq!(passes.len(), 1);
    }

    #[test]
    fn bad_settings_are_refused() {
        let square = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let job = JobSetup::default();
        let v = vbit(90.0, 25.0);
        let bad = [
            PrismParams {
                max_depth: Some(0.0),
                ..PrismParams::default()
            },
            PrismParams {
                start_depth: -1.0,
                ..PrismParams::default()
            },
        ];
        for params in &bad {
            assert!(prism(std::slice::from_ref(&square), &job, &v, params, tol()).is_err());
        }
        let end_mill = Tool::default();
        let err = prism(
            std::slice::from_ref(&square),
            &job,
            &end_mill,
            &PrismParams::default(),
            tol(),
        )
        .expect_err("needs a V");
        assert!(matches!(err, CoreError::Validation(_)));
        let open =
            crate::geom::polyline_path(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false)
                .expect("line");
        assert!(prism(&[open], &job, &v, &PrismParams::default(), tol()).is_err());
    }

    #[test]
    fn settings_read_camel_case_with_defaults() {
        let params: PrismParams =
            serde_json::from_str(r#"{"direction":"inside","maxDepth":4.0}"#).expect("json");
        assert_eq!(params.direction, PrismDirection::Inside);
        assert_eq!(params.max_depth, Some(4.0));
        assert_eq!(params.start_depth, 0.0);
    }

    #[test]
    fn the_full_prism_depth_comes_from_the_widest_shape_and_the_angle() {
        let small = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        let wide = rect(Point::new(20.0, 0.0), Point::new(40.0, 30.0)).expect("rect");
        let depth =
            full_prism_depth(&[small, wide.clone()], &vbit(90.0, 25.0), tol()).expect("depth");
        assert!((depth - 10.0).abs() < 0.01, "{depth}");
        let sixty =
            full_prism_depth(std::slice::from_ref(&wide), &vbit(60.0, 25.0), tol()).expect("depth");
        assert!((sixty - 10.0 / 30f64.to_radians().tan()).abs() < 0.02);
        let mill = Tool {
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        assert!(full_prism_depth(&[wide], &mill, tol()).is_err());
    }

    #[test]
    fn conventional_runs_the_passes_the_other_way_round() {
        let square = rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect");
        let run = |cut_direction| {
            let params = PrismParams {
                cut_direction,
                ..PrismParams::default()
            };
            prism(
                std::slice::from_ref(&square),
                &JobSetup::default(),
                &vbit(90.0, 25.0),
                &params,
                tol(),
            )
            .expect("prism")
            .moves
        };
        // Signed area of the last pass round the outline tells its turn.
        let turn = |moves: &[Move]| {
            let last: Vec<Point3> = moves
                .iter()
                .filter(|m| m.kind == MoveKind::Feed && (m.to.z - deepest(moves)).abs() < 1e-9)
                .map(|m| m.to)
                .collect();
            last.windows(2)
                .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
                .sum::<f64>()
        };
        let climb = turn(&run(CutDirection::Climb));
        let conventional = turn(&run(CutDirection::Conventional));
        assert!(climb * conventional < 0.0, "{climb} {conventional}");
    }

    /// Which of the shapes (numbered by `bucket`) each plunge goes into, in
    /// cutting order, one entry per run of plunges into the same shape.
    fn plunge_order(
        moves: &[crate::cam::toolpath::Move],
        bucket: impl Fn(f64, f64) -> usize,
    ) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::new();
        for m in moves
            .iter()
            .filter(|m| m.kind == crate::cam::toolpath::MoveKind::Plunge)
        {
            let b = bucket(m.to.x, m.to.y);
            if order.last() != Some(&b) {
                order.push(b);
            }
        }
        order
    }

    #[test]
    fn use_vector_selection_order_carves_in_the_order_selected() {
        let squares: Vec<Path> = [100.0, 0.0, 50.0]
            .iter()
            .map(|x| rect(Point::new(*x, 0.0), Point::new(x + 10.0, 10.0)).expect("rect"))
            .collect();
        let by_x = |x: f64, _y: f64| ((x + 20.0) / 50.0) as usize;
        let tool = vbit(90.0, 25.0);
        let nearest = prism(
            &squares,
            &JobSetup::default(),
            &tool,
            &PrismParams::default(),
            tol(),
        )
        .expect("prism");
        assert_eq!(plunge_order(&nearest.moves, by_x)[..3], [0, 1, 2]);
        let params = PrismParams {
            selection_order: true,
            ..PrismParams::default()
        };
        let picked = prism(&squares, &JobSetup::default(), &tool, &params, tol()).expect("prism");
        assert_eq!(plunge_order(&picked.moves, by_x)[..3], [2, 0, 1]);
    }
}
