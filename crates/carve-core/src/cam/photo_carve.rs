//! Photo Carve: a photograph carved as grooves of varying depth. A V
//! shaped tool runs along parallel lines (or two sets at right angles, or
//! along chosen vectors) across a placed bitmap, going deeper where the
//! picture is darker, so each groove widens in the dark areas and the
//! grooves together shade the picture in.
//!
//! The line spacing is a percentage of the groove width at the full carving
//! depth: at 100 percent neighbouring grooves meet in a sharp ridge where
//! the picture is black; more leaves flat material between them, fewer
//! lines and a lighter picture.

use serde::{Deserialize, Serialize};

use crate::doc::{Document, EntityKind, JobSetup};
use crate::error::CoreError;
use crate::geom::{flatten_subpath, Path, Point, Tolerance};

use super::chain::CutSeg;
use super::common::{Emitter, Heights};
use super::profile::Calculated;
use super::sketch::{placed_bitmap, ToneGrid};
use super::tool::{Tool, ToolGeometry};

/// How the grooves are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PhotoStrategy {
    /// One set of parallel lines at the line angle.
    #[default]
    Raster,
    /// Two sets at right angles.
    Hatch,
    /// Along the selected vectors.
    Vectors,
}

/// The Photo Carve form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhotoCarveParams {
    /// Raster, hatch or the selected vectors.
    pub strategy: PhotoStrategy,
    /// Angle of the lines in degrees from the X axis, 0 to 90.
    pub line_angle: f64,
    /// Distance between lines as a percentage of the groove width at the
    /// full carving depth.
    pub line_spacing: f64,
    /// Depth below the surface the carving starts at.
    pub start_depth: f64,
    /// Depth below the start depth the tool reaches where the picture is
    /// black.
    pub max_depth: f64,
    /// How far above the surface the tool lifts to move from one groove to
    /// the next close by; zero goes up to the safe height.
    pub stepover_retract: f64,
}

impl Default for PhotoCarveParams {
    fn default() -> Self {
        PhotoCarveParams {
            strategy: PhotoStrategy::Raster,
            line_angle: 0.0,
            line_spacing: 100.0,
            start_depth: 0.0,
            max_depth: 1.5,
            stepover_retract: 1.0,
        }
    }
}

/// Darkness below which the tool does not carve: the groove would be a
/// scratch.
const LIGHT: f32 = 0.02;

/// Calculates Photo Carve on the bitmap among `ids`, along the other
/// selected vectors for the vectors strategy.
///
/// # Errors
/// [`CoreError::Validation`] when no bitmap (or, for the vectors strategy,
/// no vector) is selected, for a tool that is not V shaped, and for
/// settings out of range.
pub fn photo_carve(
    doc: &Document,
    ids: &[u64],
    tool: &Tool,
    params: &PhotoCarveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let mut bitmap = None;
    let mut vectors: Vec<Path> = Vec::new();
    for id in ids {
        let entity = doc.entity(*id)?;
        if matches!(entity.kind, EntityKind::Bitmap(_)) {
            bitmap.get_or_insert(*id);
        } else {
            vectors.extend(doc.paths_of(*id).into_iter().map(|(_, path)| path));
        }
    }
    let bitmap = bitmap
        .ok_or_else(|| CoreError::validation("select the bitmap to carve before calculating"))?;
    let (tone, _) = placed_bitmap(doc, bitmap)?;
    photo_carve_tone(&tone, &vectors, doc.job(), tool, params, tol)
}

/// Photo Carve of a tone grid.
///
/// # Errors
/// As [`photo_carve`].
pub fn photo_carve_tone(
    tone: &ToneGrid,
    vectors: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &PhotoCarveParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (flat, half_angle) = match tool.tool_type {
        ToolGeometry::VBit { included_angle, .. } => (0.0, included_angle / 2.0),
        ToolGeometry::EngravingTool {
            included_angle,
            flat_diameter,
            ..
        } => (flat_diameter, included_angle / 2.0),
        _ => {
            return Err(CoreError::validation(
                "Photo Carve needs a V-bit or an engraving tool",
            ))
        }
    };
    if !(params.max_depth.is_finite() && params.max_depth > 0.0) {
        return Err(CoreError::validation(
            "the maximum carving depth must be more than zero",
        ));
    }
    if !(params.start_depth.is_finite() && params.start_depth >= 0.0) {
        return Err(CoreError::validation(
            "the start depth must be zero or more",
        ));
    }
    if !(params.line_spacing.is_finite() && params.line_spacing >= 10.0) {
        return Err(CoreError::validation(
            "the line spacing must be at least 10 percent",
        ));
    }
    if !(params.stepover_retract.is_finite() && params.stepover_retract >= 0.0) {
        return Err(CoreError::validation(
            "the stepover retract must be zero or more",
        ));
    }
    if !params.line_angle.is_finite() {
        return Err(CoreError::validation("the line angle is not a number"));
    }
    let width = flat + 2.0 * params.max_depth * half_angle.to_radians().tan();
    let spacing = width * params.line_spacing / 100.0;
    if !(spacing.is_finite() && spacing > 1e-3) {
        return Err(CoreError::validation(
            "the tool cuts no width at that depth",
        ));
    }
    let heights = Heights::from_job(job);
    let step = (tone.cell / 2.0).max(0.05);
    let lines: Vec<Vec<Point>> = match params.strategy {
        PhotoStrategy::Raster => raster(tone, spacing, params.line_angle, step),
        PhotoStrategy::Hatch => {
            let mut both = raster(tone, spacing, params.line_angle, step);
            both.extend(raster(tone, spacing, params.line_angle + 90.0, step));
            both
        }
        PhotoStrategy::Vectors => {
            if vectors.is_empty() {
                return Err(CoreError::validation(
                    "select the vectors to follow as well as the bitmap",
                ));
            }
            along_vectors(vectors, step, tol)
        }
    };
    let depth_at = |p: Point| -> Option<f64> {
        let dark = sample(tone, p)?;
        (dark >= LIGHT).then(|| params.start_depth + f64::from(dark) * params.max_depth)
    };
    let hop = if params.stepover_retract > 0.0 {
        Some((heights.surface + params.stepover_retract).min(heights.safe))
    } else {
        None
    };
    let mut out = Emitter::new(heights);
    let mut carved = false;
    for line in &lines {
        // Runs of the line over carvable picture.
        let mut run: Vec<(Point, f64)> = Vec::new();
        let mut runs: Vec<Vec<(Point, f64)>> = Vec::new();
        for p in line {
            match depth_at(*p) {
                Some(depth) => run.push((*p, heights.z_at(depth))),
                None if !run.is_empty() => runs.push(std::mem::take(&mut run)),
                None => {}
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        for run in runs {
            let run = thin(&run, 0.01);
            let Some(&(first, z0)) = run.first() else {
                continue;
            };
            let here = out.at();
            let near = here.z < heights.surface && here.xy().distance(first) <= spacing * 3.0;
            match hop {
                Some(lift) if near => {
                    let up = CutSeg {
                        from: here.xy(),
                        to: here.xy(),
                        bulge: 0.0,
                    };
                    out.cut(&up, lift);
                    out.cut(
                        &CutSeg {
                            from: here.xy(),
                            to: first,
                            bulge: 0.0,
                        },
                        lift,
                    );
                    out.plunge_to(z0);
                }
                _ => {
                    out.travel_to(first);
                    out.plunge_to(z0);
                }
            }
            let mut prev = first;
            for &(p, z) in &run[1..] {
                out.cut(
                    &CutSeg {
                        from: prev,
                        to: p,
                        bulge: 0.0,
                    },
                    z,
                );
                prev = p;
            }
            carved = true;
        }
    }
    if !carved {
        return Err(CoreError::validation(
            "the picture is too light to carve anything",
        ));
    }
    Ok(Calculated {
        moves: out.finish(),
        warnings: Vec::new(),
    })
}

/// Darkness at `p`, or `None` outside the picture.
pub(crate) fn sample(tone: &ToneGrid, p: Point) -> Option<f32> {
    let fi = (p.x - tone.origin.x) / tone.cell;
    let fj = (p.y - tone.origin.y) / tone.cell;
    if !(fi >= 0.0 && fj >= 0.0) {
        return None;
    }
    let (i, j) = (fi as usize, fj as usize);
    if i >= tone.cols || j >= tone.rows {
        return None;
    }
    let k = j * tone.cols + i;
    tone.inside[k].then(|| tone.dark[k])
}

/// Parallel lines `spacing` apart at `angle_deg` across the picture's box,
/// sampled every `step`, alternate lines reversed so the tool zigzags.
pub(crate) fn raster(tone: &ToneGrid, spacing: f64, angle_deg: f64, step: f64) -> Vec<Vec<Point>> {
    let lo = tone.origin;
    let hi = Point::new(
        lo.x + tone.cols as f64 * tone.cell,
        lo.y + tone.rows as f64 * tone.cell,
    );
    let a = angle_deg.to_radians();
    let (u, n) = (Point::new(a.cos(), a.sin()), Point::new(-a.sin(), a.cos()));
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    let dot = |p: Point, d: Point| p.x * d.x + p.y * d.y;
    let span = |d: Point| {
        corners.iter().fold((f64::MAX, f64::MIN), |(a0, a1), p| {
            (a0.min(dot(*p, d)), a1.max(dot(*p, d)))
        })
    };
    let (s0, s1) = span(u);
    let (t0, t1) = span(n);
    let count = (((t1 - t0) / spacing).floor() as usize).min(100_000);
    let samples = (((s1 - s0) / step).ceil() as usize).clamp(1, 1_000_000);
    let mut lines = Vec::with_capacity(count + 1);
    for k in 0..=count {
        let t = t0 + spacing / 2.0 + k as f64 * spacing;
        if t > t1 {
            break;
        }
        let mut line: Vec<Point> = (0..=samples)
            .map(|i| {
                let s = s0 + (s1 - s0) * i as f64 / samples as f64;
                Point::new(u.x * s + n.x * t, u.y * s + n.y * t)
            })
            .collect();
        if k % 2 == 1 {
            line.reverse();
        }
        lines.push(line);
    }
    lines
}

/// Points every `step` along each vector.
pub(crate) fn along_vectors(vectors: &[Path], step: f64, tol: Tolerance) -> Vec<Vec<Point>> {
    let mut lines = Vec::new();
    for path in vectors {
        for subpath in &path.subpaths {
            let flat = flatten_subpath(subpath, tol);
            let mut points = flat.points.clone();
            if flat.closed {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            let mut line: Vec<Point> = Vec::new();
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let length = a.distance(b);
                let count = ((length / step).ceil() as usize).max(1);
                if line.is_empty() {
                    line.push(a);
                }
                for i in 1..=count {
                    let t = i as f64 / count as f64;
                    line.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
                }
            }
            if line.len() > 1 {
                lines.push(line);
            }
        }
    }
    lines
}

/// Drops points that lie on the straight line between their neighbours,
/// Z included, within `tol` millimetres.
fn thin(points: &[(Point, f64)], tol: f64) -> Vec<(Point, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for i in 1..points.len() - 1 {
        let (a, az) = *out.last().unwrap_or(&points[0]);
        let (p, pz) = points[i];
        let (b, bz) = points[i + 1];
        let length = a.distance(b);
        let t = if length > 1e-12 {
            a.distance(p) / length
        } else {
            0.0
        };
        let on_line = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        let z = az + (bz - az) * t;
        if on_line.distance(p) > tol || (z - pz).abs() > tol {
            out.push(points[i]);
        }
    }
    if let Some(last) = points.last() {
        out.push(*last);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rect;

    /// A 40 by 20 mm picture: black on the left half, white on the right.
    fn half_black() -> ToneGrid {
        let (cols, rows) = (80, 40);
        let mut dark = vec![0.0f32; cols * rows];
        for j in 0..rows {
            for i in 0..cols / 2 {
                dark[j * cols + i] = 1.0;
            }
        }
        ToneGrid {
            cols,
            rows,
            cell: 0.5,
            origin: Point::new(10.0, 10.0),
            dark,
            inside: vec![true; cols * rows],
        }
    }

    fn vbit() -> Tool {
        Tool {
            name: "V Bit 90".to_string(),
            tool_type: ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
            ..Tool::default()
        }
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    #[test]
    fn dark_areas_are_carved_deep_and_light_areas_are_left() {
        let job = JobSetup::default();
        let params = PhotoCarveParams {
            max_depth: 2.0,
            ..PhotoCarveParams::default()
        };
        let done =
            photo_carve_tone(&half_black(), &[], &job, &vbit(), &params, tol()).expect("carves");
        let heights = Heights::from_job(&job);
        let cuts: Vec<_> = done
            .moves
            .iter()
            .filter(|m| m.kind.is_cutting() && m.to.z < heights.surface - 1e-6)
            .collect();
        assert!(!cuts.is_empty());
        assert!(
            cuts.iter().all(|m| m.to.x <= 30.0 + 0.5),
            "only the black half"
        );
        let deepest = cuts.iter().map(|m| m.to.z).fold(f64::MAX, f64::min);
        assert!((deepest - heights.z_at(2.0)).abs() < 1e-9);
        // A 90 degree V at 2 mm is 4 mm wide: lines 4 mm apart at 100 percent.
        let ys: Vec<i64> = cuts
            .iter()
            .map(|m| (m.to.y * 10.0).round() as i64)
            .collect();
        let mut rows = ys.clone();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows.len(), 5, "{rows:?}");
        assert_eq!(rows[1] - rows[0], 40);
        let hatch = PhotoCarveParams {
            strategy: PhotoStrategy::Hatch,
            ..params
        };
        let both =
            photo_carve_tone(&half_black(), &[], &job, &vbit(), &hatch, tol()).expect("hatch");
        assert!(both.moves.len() > done.moves.len());
    }

    #[test]
    fn vectors_are_followed_and_bad_settings_refused() {
        let job = JobSetup::default();
        let line = rect(Point::new(12.0, 12.0), Point::new(45.0, 25.0)).expect("rect");
        let params = PhotoCarveParams {
            strategy: PhotoStrategy::Vectors,
            ..PhotoCarveParams::default()
        };
        let done = photo_carve_tone(&half_black(), &[line], &job, &vbit(), &params, tol())
            .expect("follows");
        assert!(done.moves.iter().any(|m| m.kind.is_cutting()));
        assert!(photo_carve_tone(&half_black(), &[], &job, &vbit(), &params, tol()).is_err());
        let mill = Tool::default();
        assert!(photo_carve_tone(
            &half_black(),
            &[],
            &job,
            &mill,
            &PhotoCarveParams::default(),
            tol()
        )
        .is_err());
        let white = ToneGrid {
            dark: vec![0.0; 80 * 40],
            ..half_black()
        };
        assert!(photo_carve_tone(
            &white,
            &[],
            &job,
            &vbit(),
            &PhotoCarveParams::default(),
            tol()
        )
        .is_err());
    }
}
