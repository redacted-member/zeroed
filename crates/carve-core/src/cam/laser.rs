//! Laser module toolpaths: Laser Cut and Fill along and inside vectors, and
//! Laser Picture, a raster that burns a bitmap by varying the power of every
//! move.
//!
//! Lasers cut at the material surface: every move stays at the surface
//! height (a Cut Depth steps later passes down to keep the beam in focus),
//! travel moves are rapids (the post turns the beam off for them)
//! and burning moves are feeds that carry their power, a percentage of the
//! machine maximum, in [`Move::power`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::{Path, Point, Region, Tolerance};
use crate::io::bitmap::Bitmap;

use super::chain::{Chain, CutSeg};
use super::common::{order_nearest, Heights};
use super::pocket::raster_lines;
use super::profile::Calculated;
use super::tool::{Tool, ToolGeometry};
use super::toolpath::{Move, Point3};

/// Most raster cells a Laser Picture may burn, to keep toolpaths a size a
/// controller can stream.
pub const MAX_PICTURE_CELLS: usize = 16_000_000;

/// What Laser Cut and Fill burns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaserMode {
    /// Along the vectors.
    #[default]
    Cut,
    /// Hatch lines filling the closed vectors.
    Fill,
    /// The fill first, then the outline.
    CutAndFill,
}

/// Where the cut runs relative to the vector, allowing for the kerf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaserSide {
    /// Centred on the vector.
    #[default]
    On,
    /// Half a kerf outside, so the part keeps its size.
    Outside,
    /// Half a kerf inside, so the hole keeps its size.
    Inside,
}

/// Fill hatching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hatch {
    /// Parallel lines at the fill angle.
    #[default]
    Lines,
    /// Lines at the fill angle, then again at right angles to it.
    Cross,
}

/// Settings of Laser Cut and Fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaserCutFillParams {
    /// Cut, fill or both.
    pub mode: LaserMode,
    /// Kerf compensation for the cut.
    pub side: LaserSide,
    /// No. Repeats: times each pass is burnt at the same height, for a
    /// laser that does not burn deep enough at once. Older files call it
    /// `passes`.
    #[serde(alias = "passes")]
    pub repeats: u32,
    /// Cut Depth in millimetres: zero burns at the surface; deeper, the
    /// toolpath steps down in passes of the laser's pass depth to keep it
    /// in focus (No. Passes, see [`laser_passes`]).
    pub cut_depth: f64,
    /// Cut power in percent; the tool's power when absent.
    pub power: Option<f64>,
    /// Cut speed in millimetres per minute; the tool's speed when absent.
    pub speed: Option<f64>,
    /// Fill power in percent; the cut power when absent.
    pub fill_power: Option<f64>,
    /// Distance between fill lines, in millimetres.
    pub line_interval: f64,
    /// Fill line angle in degrees from the X axis.
    pub angle: f64,
    /// Single or cross hatching.
    pub hatch: Hatch,
    /// Burn fill lines in both directions rather than always the same way.
    pub bidirectional: bool,
    /// Distance each fill line runs on unpowered past its ends, so the head
    /// is at speed when it burns, in millimetres (older files).
    pub overscan: f64,
    /// Overscan as a percentage of each fill line's burnt length; used in
    /// place of `overscan` when above zero.
    pub overscan_percent: f64,
    /// Fill Regionally: each separate area (a letter of a word) is filled
    /// on its own, one after the other; off fills them together.
    pub fill_regions: bool,
    /// Extra offset of an outside or inside cut past the kerf, in
    /// millimetres, to ease or tighten a fit (Allowance).
    pub allowance: f64,
}

impl Default for LaserCutFillParams {
    fn default() -> Self {
        LaserCutFillParams {
            mode: LaserMode::Cut,
            side: LaserSide::On,
            repeats: 1,
            cut_depth: 0.0,
            power: None,
            speed: None,
            fill_power: None,
            line_interval: 0.1,
            angle: 0.0,
            hatch: Hatch::Lines,
            bidirectional: true,
            overscan: 0.0,
            overscan_percent: 0.0,
            fill_regions: false,
            allowance: 0.0,
        }
    }
}

/// No. Passes of a laser toolpath: the Cut Depth over the laser's pass
/// depth, rounded up; one at the surface.
pub fn laser_passes(cut_depth: f64, pass_depth: f64) -> u32 {
    if !(cut_depth.is_finite() && cut_depth > 1e-9) || !(pass_depth.is_finite() && pass_depth > 0.0)
    {
        return 1;
    }
    ((cut_depth / pass_depth - 1e-9).ceil() as u32).clamp(1, 100)
}

/// The overscan of a line `length` long: the percentage of it, or the old
/// fixed distance.
fn overscan_for(length: f64, percent: f64, fixed: f64) -> f64 {
    if percent > 0.0 {
        length * percent / 100.0
    } else {
        fixed
    }
}

/// How Laser Picture turns grey levels into burning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PictureMode {
    /// The power follows the darkness of each dot.
    #[default]
    PowerModulation,
    /// Dots are fully on or off, spread by a dither pattern.
    Dither,
}

/// Dither patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dither {
    /// Error diffusion.
    #[default]
    FloydSteinberg,
    /// A 4 by 4 Bayer threshold matrix.
    Ordered,
}

/// The lines Laser Picture burns the picture along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PictureStrategy {
    /// One set of parallel lines at the line angle.
    #[default]
    Raster,
    /// Two sets at right angles.
    Hatch,
    /// Along the selected vectors.
    Vectors,
}

/// Settings of Laser Picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaserPictureParams {
    /// Raster, Hatch or the selected vectors (a bitmap selected in the
    /// drawing; older toolpaths read `path` instead).
    pub strategy: PictureStrategy,
    /// Line Angle in degrees, -90 to 90: 0 burns lines along X.
    pub line_angle: f64,
    /// No. Passes: times the picture is burnt.
    pub repeats: u32,
    /// Overscan as a percentage of each line's burnt length; used in place
    /// of `overscan` when above zero.
    pub overscan_percent: f64,
    /// The image file.
    pub path: PathBuf,
    /// Lower left corner of the picture on the job, in millimetres.
    pub origin: Point,
    /// Width of the picture in millimetres.
    pub width: f64,
    /// Height in millimetres; zero keeps the image's proportions.
    pub height: f64,
    /// Distance between raster lines and between dots along a line.
    pub line_interval: f64,
    /// Power modulation or dithering.
    pub mode: PictureMode,
    /// The dither pattern when dithering.
    pub dither: Dither,
    /// Power for white, in percent.
    pub min_power: f64,
    /// Power for black in percent; the tool's power when absent.
    pub max_power: Option<f64>,
    /// Speed in millimetres per minute; the tool's speed when absent.
    pub speed: Option<f64>,
    /// Burn the light parts instead of the dark ones.
    pub invert: bool,
    /// Burn lines in both directions.
    pub bidirectional: bool,
    /// Unpowered run past each end of a line, in millimetres.
    pub overscan: f64,
}

impl Default for LaserPictureParams {
    fn default() -> Self {
        LaserPictureParams {
            path: PathBuf::new(),
            origin: Point::new(0.0, 0.0),
            width: 100.0,
            height: 0.0,
            line_interval: 0.1,
            mode: PictureMode::PowerModulation,
            dither: Dither::FloydSteinberg,
            min_power: 0.0,
            max_power: None,
            speed: None,
            invert: false,
            bidirectional: true,
            overscan: 0.0,
            strategy: PictureStrategy::Raster,
            line_angle: 0.0,
            repeats: 1,
            overscan_percent: 0.0,
        }
    }
}

/// The power and speed a laser tool burns at.
///
/// # Errors
/// [`CoreError::Validation`] when the tool is not a laser.
pub fn tool_power_speed(tool: &Tool) -> Result<(f64, f64), CoreError> {
    match tool.tool_type {
        ToolGeometry::LaserTool { power, speed, .. } => Ok((power, speed)),
        _ => Err(CoreError::validation(
            "laser toolpaths need a laser tool: pick one from the tool database",
        )),
    }
}

fn check_power(name: &str, power: f64) -> Result<f64, CoreError> {
    if power.is_finite() && (0.0..=100.0).contains(&power) {
        Ok(power)
    } else {
        Err(CoreError::validation(format!(
            "{name} must be 0 to 100 percent"
        )))
    }
}

fn check_speed(speed: f64) -> Result<f64, CoreError> {
    if speed.is_finite() && speed > 0.0 {
        Ok(speed)
    } else {
        Err(CoreError::validation("the speed must be greater than zero"))
    }
}

fn check_interval(interval: f64) -> Result<f64, CoreError> {
    if interval.is_finite() && interval >= 0.01 {
        Ok(interval)
    } else {
        Err(CoreError::validation(
            "the line interval must be at least 0.01 mm",
        ))
    }
}

fn check_overscan(overscan: f64) -> Result<f64, CoreError> {
    if overscan.is_finite() && overscan >= 0.0 {
        Ok(overscan)
    } else {
        Err(CoreError::validation("the overscan cannot be negative"))
    }
}

/// The speed a laser toolpath runs at: the setting when given, else the
/// tool's.
///
/// # Errors
/// When the tool is not a laser or the speed is not positive.
pub fn speed_of(tool: &Tool, speed: Option<f64>) -> Result<f64, CoreError> {
    let (_, tool_speed) = tool_power_speed(tool)?;
    check_speed(speed.unwrap_or(tool_speed))
}

/// Builds laser moves, all at the surface height.
pub(crate) struct Beam {
    z: f64,
    home: Point3,
    at: Point3,
    moves: Vec<Move>,
}

impl Beam {
    pub(crate) fn new(heights: Heights) -> Beam {
        Beam {
            z: heights.surface,
            home: heights.home,
            at: heights.home,
            moves: Vec::new(),
        }
    }

    /// Burns the next moves `depth` millimetres below the surface.
    pub(crate) fn set_depth(&mut self, surface: f64, depth: f64) {
        self.z = surface - depth.max(0.0);
    }

    fn push(&mut self, m: Move) {
        if m.to != self.at {
            self.at = m.to;
            self.moves.push(m);
        }
    }

    fn travel(&mut self, p: Point) {
        self.push(Move::rapid(Point3::new(p.x, p.y, self.z)));
    }

    pub(crate) fn burn_to(&mut self, p: Point, power: f64) {
        self.push(Move::feed(Point3::new(p.x, p.y, self.z)).with_power(power));
    }

    fn burn(&mut self, seg: &CutSeg, power: f64) {
        let to = Point3::new(seg.to.x, seg.to.y, self.z);
        let m = match seg.arc() {
            Some(arc) if arc.sweep > 0.0 => Move::arc_ccw(arc.centre, to),
            Some(arc) => Move::arc_cw(arc.centre, to),
            None => Move::feed(to),
        };
        self.push(m.with_power(power));
    }

    fn burn_chain(&mut self, chain: &Chain, power: f64) {
        let Some(start) = chain.start() else {
            return;
        };
        self.travel(start);
        for seg in &chain.segs {
            self.burn(seg, power);
        }
    }

    /// A straight line burnt at `power`, run on unpowered for `overscan`
    /// before and after.
    pub(crate) fn burn_line(&mut self, from: Point, to: Point, power: f64, overscan: f64) {
        let dir = from.to(to);
        let length = dir.length();
        if length < 1e-9 {
            return;
        }
        let unit = dir.scaled(1.0 / length);
        if overscan > 0.0 {
            self.travel(from.offset(unit.scaled(-overscan)));
            self.burn_to(from, 0.0);
        } else {
            self.travel(from);
        }
        self.burn_to(to, power);
        if overscan > 0.0 {
            self.burn_to(to.offset(unit.scaled(overscan)), 0.0);
        }
    }

    pub(crate) fn finish(mut self) -> Vec<Move> {
        let home = self.home;
        self.push(Move::rapid(Point3::new(
            home.x,
            home.y,
            self.at.z.max(home.z),
        )));
        self.push(Move::rapid(home));
        self.moves
    }
}

/// The chains a cut follows: closed vectors offset by half the kerf for the
/// side, and open vectors as they are.
fn cut_chains(
    paths: &[Path],
    side: LaserSide,
    kerf: f64,
    allowance: f64,
    tol: Tolerance,
) -> Vec<Chain> {
    let region = Region::from_paths(paths, tol);
    let offset = match side {
        LaserSide::Outside => -(kerf / 2.0 + allowance),
        LaserSide::Inside => kerf / 2.0 + allowance,
        LaserSide::On => 0.0,
    };
    let region = if offset == 0.0 {
        region
    } else {
        region.offset(offset, tol)
    };
    let mut chains: Vec<Chain> = region
        .loops
        .iter()
        .map(|sub| Chain::from_subpath(sub, tol))
        .collect();
    for path in paths {
        for sub in path.subpaths.iter().filter(|s| !s.closed) {
            chains.push(Chain::from_subpath(sub, tol));
        }
    }
    chains.retain(|c| !c.segs.is_empty());
    chains
}

/// Calculates Laser Cut and Fill.
///
/// # Errors
/// [`CoreError::Validation`] for a tool that is not a laser, powers outside
/// 0 to 100 percent, no passes, a line interval under 0.01 mm, or nothing to
/// burn.
pub fn laser_cut_fill(
    paths: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &LaserCutFillParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    let (tool_power, _) = tool_power_speed(tool)?;
    let power = check_power("the power", params.power.unwrap_or(tool_power))?;
    let fill_power = check_power("the fill power", params.fill_power.unwrap_or(power))?;
    speed_of(tool, params.speed)?;
    if params.repeats == 0 || params.repeats > 100 {
        return Err(CoreError::validation("repeats must be 1 to 100"));
    }
    if !(params.cut_depth.is_finite() && params.cut_depth >= 0.0) {
        return Err(CoreError::validation("the cut depth cannot be negative"));
    }
    let overscan = check_overscan(params.overscan)?;
    let overscan_percent = check_overscan(params.overscan_percent)?;
    let heights = Heights::from_job(job);
    let kerf = tool.diameter();

    let fill = matches!(params.mode, LaserMode::Fill | LaserMode::CutAndFill);
    let cut = matches!(params.mode, LaserMode::Cut | LaserMode::CutAndFill);
    let mut lines: Vec<Chain> = Vec::new();
    if fill {
        let interval = check_interval(params.line_interval)?;
        let whole = Region::from_paths(paths, tol);
        let regions = if params.fill_regions {
            let mut parts = crate::cam::surface::components(&whole, tol);
            // Nearest area next, starting from home.
            let from = heights.home.xy();
            parts = order_nearest(
                parts,
                from,
                |r| r.loops.first().map_or(from, |l| l.start),
                |r| r.loops.first().map_or(from, |l| l.start),
            );
            parts
        } else {
            vec![whole]
        };
        for region in &regions {
            let mut angles = vec![params.angle];
            if params.hatch == Hatch::Cross {
                angles.push(params.angle + 90.0);
            }
            for angle in angles {
                let mut set = raster_lines(region, interval, angle, tol);
                if !params.bidirectional {
                    // raster_lines alternates rows; turn the odd ones back.
                    let first = set.first().and_then(|c| {
                        let (a, b) = (c.start()?, c.end()?);
                        Some(a.to(b))
                    });
                    if let Some(first) = first {
                        for chain in &mut set {
                            let same = match (chain.start(), chain.end()) {
                                (Some(a), Some(b)) => a.to(b).dot(first) > 0.0,
                                _ => true,
                            };
                            if !same {
                                *chain = chain.reversed();
                            }
                        }
                    }
                }
                lines.extend(set);
            }
        }
    }
    let chains = if cut {
        let chains = cut_chains(paths, params.side, kerf, params.allowance, tol);
        let from = heights.home.xy();
        order_nearest(
            chains,
            from,
            |c| c.start().unwrap_or(from),
            |c| c.end().unwrap_or(from),
        )
    } else {
        Vec::new()
    };
    if lines.is_empty() && chains.is_empty() {
        return Err(CoreError::validation(if fill && !cut {
            "there is nothing to fill: fills need closed vectors"
        } else {
            "there is nothing to burn in the selected vectors"
        }));
    }

    let passes = laser_passes(params.cut_depth, tool.pass_depth);
    let step = if passes > 1 {
        params.cut_depth / f64::from(passes)
    } else {
        0.0
    };
    let surface = heights.surface;
    let mut beam = Beam::new(heights);
    for pass in 0..passes {
        // Each pass a step deeper, the first at the surface.
        beam.set_depth(surface, step * f64::from(pass));
        for _ in 0..params.repeats {
            for line in &lines {
                if let (Some(a), Some(b)) = (line.start(), line.end()) {
                    let run = overscan_for(a.distance(b), overscan_percent, overscan);
                    beam.burn_line(a, b, fill_power, run);
                }
            }
            for chain in &chains {
                beam.burn_chain(chain, power);
            }
        }
    }
    Ok(Calculated {
        moves: beam.finish(),
        warnings: Vec::new(),
    })
}

/// Grey level of the image at a normalised position (0,0 lower left, 1,1
/// upper right), as darkness from 0 (white or transparent) to 1 (black),
/// averaged over the pixels the dot covers.
fn darkness(image: &Bitmap, u0: f64, v0: f64, u1: f64, v1: f64) -> f64 {
    let (w, h) = (image.width as usize, image.height as usize);
    let px = |u: f64| ((u * w as f64).floor() as usize).min(w - 1);
    // Image rows run top to bottom, the job's Y bottom to top.
    let py = |v: f64| (((1.0 - v) * h as f64).floor() as usize).min(h - 1);
    let (x0, x1) = (px(u0), px((u1 - 1e-9).max(u0)));
    let (y0, y1) = (py((v1 - 1e-9).max(v0)), py(v0));
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = y * w + x;
            let alpha = image
                .alpha
                .as_ref()
                .map_or(1.0, |a| f64::from(a[i]) / 255.0);
            sum += (1.0 - f64::from(image.luma[i]) / 255.0) * alpha;
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        0.0
    }
}

/// Darkness of every dot, row 0 at the bottom.
fn dots(image: &Bitmap, cols: usize, rows: usize, invert: bool) -> Vec<Vec<f64>> {
    (0..rows)
        .map(|r| {
            let (v0, v1) = (r as f64 / rows as f64, (r + 1) as f64 / rows as f64);
            (0..cols)
                .map(|c| {
                    let (u0, u1) = (c as f64 / cols as f64, (c + 1) as f64 / cols as f64);
                    let d = darkness(image, u0, v0, u1, v1);
                    if invert {
                        1.0 - d
                    } else {
                        d
                    }
                })
                .collect()
        })
        .collect()
}

/// Turns darkness into on (1) or off (0) dots.
fn dither(grid: &mut [Vec<f64>], kind: Dither) {
    const BAYER: [[f64; 4]; 4] = [
        [0.0, 8.0, 2.0, 10.0],
        [12.0, 4.0, 14.0, 6.0],
        [3.0, 11.0, 1.0, 9.0],
        [15.0, 7.0, 13.0, 5.0],
    ];
    let rows = grid.len();
    match kind {
        Dither::Ordered => {
            for (r, row) in grid.iter_mut().enumerate() {
                for (c, d) in row.iter_mut().enumerate() {
                    let threshold = (BAYER[r % 4][c % 4] + 0.5) / 16.0;
                    *d = if *d > threshold { 1.0 } else { 0.0 };
                }
            }
        }
        Dither::FloydSteinberg => {
            // Top row first, as the image is read.
            for r in (0..rows).rev() {
                let cols = grid[r].len();
                for c in 0..cols {
                    let old = grid[r][c];
                    let new = if old >= 0.5 { 1.0 } else { 0.0 };
                    grid[r][c] = new;
                    let err = old - new;
                    if c + 1 < cols {
                        grid[r][c + 1] += err * 7.0 / 16.0;
                    }
                    if r > 0 {
                        if c > 0 {
                            grid[r - 1][c - 1] += err * 3.0 / 16.0;
                        }
                        grid[r - 1][c] += err * 5.0 / 16.0;
                        if c + 1 < cols {
                            grid[r - 1][c + 1] += err * 1.0 / 16.0;
                        }
                    }
                }
            }
        }
    }
}

/// Calculates Laser Picture from an image already read.
///
/// # Errors
/// [`CoreError::Validation`] for a tool that is not a laser, powers outside
/// 0 to 100 percent or a minimum above the maximum, a size or interval that
/// is not positive, too many dots, or an image with nothing to burn.
pub fn laser_picture(
    image: &Bitmap,
    job: &JobSetup,
    tool: &Tool,
    params: &LaserPictureParams,
) -> Result<Calculated, CoreError> {
    let (tool_power, _) = tool_power_speed(tool)?;
    let max_power = check_power("the maximum power", params.max_power.unwrap_or(tool_power))?;
    let min_power = check_power("the minimum power", params.min_power)?;
    if min_power > max_power {
        return Err(CoreError::validation(
            "the minimum power cannot be above the maximum",
        ));
    }
    speed_of(tool, params.speed)?;
    let interval = check_interval(params.line_interval)?;
    let overscan = check_overscan(params.overscan)?;
    if image.width == 0 || image.height == 0 {
        return Err(CoreError::validation("the image has no pixels"));
    }
    let width = params.width;
    let height = if params.height > 0.0 {
        params.height
    } else {
        width * f64::from(image.height) / f64::from(image.width)
    };
    if !(width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0) {
        return Err(CoreError::validation(
            "the picture size must be greater than zero",
        ));
    }
    let cols = (width / interval).round().max(1.0) as usize;
    let rows = (height / interval).round().max(1.0) as usize;
    if cols.saturating_mul(rows) > MAX_PICTURE_CELLS {
        return Err(CoreError::validation(format!(
            "the picture would have {} dots; use a larger line interval or a smaller size",
            cols * rows
        )));
    }
    let mut grid = dots(image, cols, rows, params.invert);
    if params.mode == PictureMode::Dither {
        dither(&mut grid, params.dither);
    }
    let dot_w = width / cols as f64;
    let dot_h = height / rows as f64;
    let power_of = |d: f64| -> f64 {
        let d = d.clamp(0.0, 1.0);
        if d <= 1e-6 {
            0.0
        } else {
            // Whole percent steps let neighbouring dots merge into one move.
            (min_power + d * (max_power - min_power)).round()
        }
    };

    let heights = Heights::from_job(job);
    let mut beam = Beam::new(heights);
    let mut forward = true;
    let mut burnt = false;
    for (r, row) in grid.iter().enumerate() {
        let powers: Vec<f64> = row.iter().map(|d| power_of(*d)).collect();
        let (Some(first), Some(last)) = (
            powers.iter().position(|p| *p > 0.0),
            powers.iter().rposition(|p| *p > 0.0),
        ) else {
            continue;
        };
        burnt = true;
        let y = params.origin.y + (r as f64 + 0.5) * dot_h;
        let x_at = |c: usize| params.origin.x + c as f64 * dot_w;
        // Runs of equal power from `first` to `last`, as (start col, end col, power).
        let mut runs: Vec<(usize, usize, f64)> = Vec::new();
        for (c, p) in powers.iter().enumerate().take(last + 1).skip(first) {
            match runs.last_mut() {
                Some(run) if (run.2 - p).abs() < 1e-9 => run.1 = c + 1,
                _ => runs.push((c, c + 1, *p)),
            }
        }
        let (start, end) = (x_at(first), x_at(last + 1));
        let sign = if forward { 1.0 } else { -1.0 };
        let (from, to) = if forward { (start, end) } else { (end, start) };
        beam.travel(Point::new(from - sign * overscan, y));
        if overscan > 0.0 {
            beam.burn_to(Point::new(from, y), 0.0);
        }
        if !forward {
            runs.reverse();
        }
        for (c0, c1, p) in runs {
            let x = if forward { x_at(c1) } else { x_at(c0) };
            beam.burn_to(Point::new(x, y), p);
        }
        if overscan > 0.0 {
            beam.burn_to(Point::new(to + sign * overscan, y), 0.0);
        }
        if params.bidirectional {
            forward = !forward;
        }
    }
    if !burnt {
        return Err(CoreError::validation(
            "the picture has nothing dark enough to burn",
        ));
    }
    Ok(Calculated {
        moves: beam.finish(),
        warnings: Vec::new(),
    })
}

/// Laser Picture of the bitmap selected in the drawing (its tone grid, as
/// placed), along raster or hatch lines at the line angle or along
/// `vectors`: the power follows the darkness, or with Dither Image each dot
/// is fully on or off.
///
/// # Errors
/// [`CoreError::Validation`] for a tool that is not a laser, a power
/// outside 0 to 100 percent, a line spacing under 0.01 mm, an angle outside
/// -90 to 90 degrees, no repeats, no vectors for that strategy, or a
/// picture with nothing to burn.
pub fn laser_picture_tone(
    tone: &super::sketch::ToneGrid,
    vectors: &[Path],
    job: &JobSetup,
    tool: &Tool,
    params: &LaserPictureParams,
    tol: Tolerance,
) -> Result<Calculated, CoreError> {
    use super::photo_carve::{along_vectors, raster, sample};
    let (tool_power, _) = tool_power_speed(tool)?;
    let max_power = check_power("the power", params.max_power.unwrap_or(tool_power))?;
    let min_power = check_power("the minimum power", params.min_power)?.min(max_power);
    speed_of(tool, params.speed)?;
    let interval = check_interval(params.line_interval)?;
    let overscan = check_overscan(params.overscan)?;
    let overscan_percent = check_overscan(params.overscan_percent)?;
    if !(params.line_angle.is_finite() && (-90.0..=90.0).contains(&params.line_angle)) {
        return Err(CoreError::validation(
            "the line angle must be -90 to 90 degrees",
        ));
    }
    if params.repeats == 0 || params.repeats > 100 {
        return Err(CoreError::validation("passes must be 1 to 100"));
    }
    let sets: Vec<(Vec<Vec<Point>>, bool)> = match params.strategy {
        PictureStrategy::Raster => {
            vec![(raster(tone, interval, params.line_angle, interval), true)]
        }
        PictureStrategy::Hatch => vec![
            (raster(tone, interval, params.line_angle, interval), true),
            (
                raster(tone, interval, params.line_angle + 90.0, interval),
                true,
            ),
        ],
        PictureStrategy::Vectors => {
            if vectors.is_empty() {
                return Err(CoreError::validation(
                    "select the vectors to burn along as well as the bitmap",
                ));
            }
            vec![(along_vectors(vectors, interval, tol), false)]
        }
    };
    let cells: usize = sets
        .iter()
        .flat_map(|(lines, _)| lines.iter().map(Vec::len))
        .sum();
    if cells > MAX_PICTURE_CELLS {
        return Err(CoreError::validation(format!(
            "the picture would have {cells} dots; use a wider line spacing"
        )));
    }
    let power_of = |d: f64| -> f64 {
        let d = d.clamp(0.0, 1.0);
        if d <= 1e-6 {
            0.0
        } else {
            // Whole percent steps let neighbouring dots merge into one move.
            (min_power + d * (max_power - min_power)).round()
        }
    };
    let heights = Heights::from_job(job);
    let mut beam = Beam::new(heights);
    let mut burnt = false;
    for _ in 0..params.repeats {
        for (lines, straight) in &sets {
            // Darkness of each dot: the stretch from one point to the next.
            let mut grid: Vec<Vec<f64>> = lines
                .iter()
                .map(|line| {
                    line.windows(2)
                        .map(|w| {
                            let mid = Point::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
                            let d = sample(tone, mid).map_or(0.0, f64::from);
                            if params.invert && sample(tone, mid).is_some() {
                                1.0 - d
                            } else {
                                d
                            }
                        })
                        .collect()
                })
                .collect();
            if params.mode == PictureMode::Dither {
                if *straight {
                    // The raster zigzags; dither the rows the same way round.
                    for (k, row) in grid.iter_mut().enumerate() {
                        if k % 2 == 1 {
                            row.reverse();
                        }
                    }
                    dither(&mut grid, params.dither);
                    for (k, row) in grid.iter_mut().enumerate() {
                        if k % 2 == 1 {
                            row.reverse();
                        }
                    }
                } else {
                    for row in &mut grid {
                        let mut carry = 0.0;
                        for d in row.iter_mut() {
                            let want = *d + carry;
                            *d = if want >= 0.5 { 1.0 } else { 0.0 };
                            carry = want - *d;
                        }
                    }
                }
            }
            for (line, row) in lines.iter().zip(&grid) {
                let powers: Vec<f64> = row.iter().map(|d| power_of(*d)).collect();
                let (Some(first), Some(last)) = (
                    powers.iter().position(|p| *p > 0.0),
                    powers.iter().rposition(|p| *p > 0.0),
                ) else {
                    continue;
                };
                burnt = true;
                let (start, end) = (line[first], line[last + 1]);
                let run = if *straight {
                    overscan_for(start.distance(end), overscan_percent, overscan)
                } else {
                    0.0
                };
                let dir = start.to(end);
                let length = dir.length();
                let unit = if length > 1e-9 {
                    dir.scaled(1.0 / length)
                } else {
                    dir
                };
                if run > 0.0 {
                    beam.travel(start.offset(unit.scaled(-run)));
                    beam.burn_to(start, 0.0);
                } else {
                    beam.travel(start);
                }
                for i in first..=last {
                    let next_differs = i == last || (powers[i + 1] - powers[i]).abs() > 1e-9;
                    if !*straight || next_differs {
                        beam.burn_to(line[i + 1], powers[i]);
                    }
                }
                if run > 0.0 {
                    beam.burn_to(end.offset(unit.scaled(run)), 0.0);
                }
            }
        }
    }
    if !burnt {
        return Err(CoreError::validation(
            "the picture has nothing dark enough to burn",
        ));
    }
    Ok(Calculated {
        moves: beam.finish(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;
    use crate::geom::rect;

    fn laser(power: f64) -> Tool {
        Tool {
            name: "Laser".to_string(),
            tool_type: ToolGeometry::LaserTool {
                power,
                speed: 1200.0,
                kerf: 0.2,
                max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
            },
            ..Tool::default()
        }
    }

    fn square() -> Vec<Path> {
        vec![rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")]
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tol")
    }

    fn burns(moves: &[Move]) -> Vec<&Move> {
        moves.iter().filter(|m| m.kind.is_cutting()).collect()
    }

    #[test]
    fn a_cut_burns_every_pass_at_the_surface_with_its_power() {
        let job = JobSetup::default();
        let surface = Heights::from_job(&job).surface;
        let params = LaserCutFillParams {
            repeats: 3,
            power: Some(60.0),
            ..LaserCutFillParams::default()
        };
        let out = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("cuts");
        let cuts = burns(&out.moves);
        assert_eq!(cuts.len(), 12, "four sides three times");
        assert!(cuts.iter().all(|m| m.power == Some(60.0)));
        assert!(out
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .all(|m| (m.to.z - surface).abs() < 1e-9));
        assert!(out.moves.iter().all(|m| m.kind != MoveKind::Plunge));
    }

    #[test]
    fn a_cut_depth_steps_down_in_passes_of_the_laser_pass_depth() {
        assert_eq!(laser_passes(0.0, 1.0), 1);
        assert_eq!(laser_passes(3.0, 1.0), 3);
        assert_eq!(laser_passes(2.5, 1.0), 3);
        assert_eq!(laser_passes(2.0, 0.0), 1);
        let job = JobSetup::default();
        let surface = Heights::from_job(&job).surface;
        let mut tool = laser(80.0);
        tool.pass_depth = 1.0;
        let params = LaserCutFillParams {
            cut_depth: 2.0,
            repeats: 2,
            ..LaserCutFillParams::default()
        };
        let out = laser_cut_fill(&square(), &job, &tool, &params, tol()).expect("cuts");
        let cuts = burns(&out.moves);
        assert_eq!(cuts.len(), 16, "two passes, each burnt twice");
        let mut depths: Vec<f64> = cuts.iter().map(|m| surface - m.to.z).collect();
        depths.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert_eq!(depths, vec![0.0, 1.0]);
        // Older files named the repeats passes.
        let old: LaserCutFillParams = serde_json::from_str(r#"{"passes":4}"#).expect("json");
        assert_eq!(old.repeats, 4);
    }

    #[test]
    fn overscan_is_a_share_of_each_fill_line_and_regions_fill_one_by_one() {
        let job = JobSetup::default();
        let two = vec![
            rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect"),
            rect(Point::new(20.0, 0.0), Point::new(30.0, 10.0)).expect("rect"),
        ];
        let params = LaserCutFillParams {
            mode: LaserMode::Fill,
            line_interval: 1.0,
            overscan_percent: 10.0,
            ..LaserCutFillParams::default()
        };
        let together = laser_cut_fill(&two, &job, &laser(50.0), &params, tol()).expect("fills");
        // An unpowered run of a tenth of the line before each burn.
        let m = &together.moves;
        let burn = (2..m.len())
            .find(|&i| m[i].power.is_some_and(|p| p > 0.0) && m[i - 1].power == Some(0.0))
            .expect("a burn after an overscan");
        let run = m[burn - 2].to.xy().distance(m[burn - 1].to.xy());
        let line = m[burn - 1].to.xy().distance(m[burn].to.xy());
        assert!((run - line * 0.1).abs() < 1e-6, "{run} for a {line} line");
        let regional = laser_cut_fill(
            &two,
            &job,
            &laser(50.0),
            &LaserCutFillParams {
                fill_regions: true,
                ..params.clone()
            },
            tol(),
        )
        .expect("fills");
        // Filled one square after the other: the burns switch squares once.
        let sides: Vec<bool> = burns(&regional.moves)
            .iter()
            .filter(|m| m.power.is_some_and(|p| p > 0.0))
            .map(|m| m.to.x > 15.0)
            .collect();
        let switches = sides.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(switches, 1, "{sides:?}");
        let joint: Vec<bool> = burns(&together.moves)
            .iter()
            .filter(|m| m.power.is_some_and(|p| p > 0.0))
            .map(|m| m.to.x > 15.0)
            .collect();
        assert!(joint.windows(2).filter(|w| w[0] != w[1]).count() > 1);
    }

    #[test]
    fn the_kerf_moves_an_outside_cut_out_by_half_its_width() {
        let job = JobSetup::default();
        let params = LaserCutFillParams {
            side: LaserSide::Outside,
            ..LaserCutFillParams::default()
        };
        let out = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("cuts");
        let max_x = burns(&out.moves)
            .iter()
            .map(|m| m.to.x)
            .fold(f64::MIN, f64::max);
        assert!((max_x - 10.1).abs() < 1e-6, "{max_x}");
    }

    #[test]
    fn the_allowance_moves_an_outside_cut_further_out() {
        let job = JobSetup::default();
        let params = LaserCutFillParams {
            side: LaserSide::Outside,
            allowance: 0.4,
            ..LaserCutFillParams::default()
        };
        let out = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("cuts");
        let max_x = burns(&out.moves)
            .iter()
            .map(|m| m.to.x)
            .fold(f64::MIN, f64::max);
        assert!((max_x - 10.5).abs() < 1e-6, "{max_x}");
    }

    #[test]
    fn a_fill_hatches_the_inside_and_cross_hatching_doubles_it() {
        let job = JobSetup::default();
        let mut params = LaserCutFillParams {
            mode: LaserMode::Fill,
            line_interval: 1.0,
            fill_power: Some(30.0),
            ..LaserCutFillParams::default()
        };
        let single = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("fills");
        assert_eq!(burns(&single.moves).len(), 10);
        assert!(burns(&single.moves).iter().all(|m| m.power == Some(30.0)));
        params.hatch = Hatch::Cross;
        let cross = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("fills");
        assert_eq!(burns(&cross.moves).len(), 20);
        params.hatch = Hatch::Lines;
        params.bidirectional = false;
        let one_way = laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).expect("fills");
        let mut from = Heights::from_job(&job).home;
        for m in &one_way.moves {
            if m.kind == MoveKind::Feed {
                assert!(m.to.x > from.x, "every line burns left to right");
            }
            from = m.to;
        }
    }

    #[test]
    fn power_and_settings_out_of_range_are_refused() {
        let job = JobSetup::default();
        let bad = |params: LaserCutFillParams| {
            laser_cut_fill(&square(), &job, &laser(80.0), &params, tol()).is_err()
        };
        assert!(bad(LaserCutFillParams {
            power: Some(120.0),
            ..LaserCutFillParams::default()
        }));
        assert!(bad(LaserCutFillParams {
            fill_power: Some(-1.0),
            ..LaserCutFillParams::default()
        }));
        assert!(bad(LaserCutFillParams {
            repeats: 0,
            ..LaserCutFillParams::default()
        }));
        assert!(bad(LaserCutFillParams {
            cut_depth: -1.0,
            ..LaserCutFillParams::default()
        }));
        let mut mill = laser(80.0);
        mill.tool_type = ToolGeometry::EndMill { diameter: 6.0 };
        assert!(laser_cut_fill(
            &square(),
            &job,
            &mill,
            &LaserCutFillParams::default(),
            tol()
        )
        .is_err());
    }

    fn gradient(width: u32, height: u32) -> Bitmap {
        let mut luma = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                luma.push((255 * x / (width - 1)) as u8);
            }
        }
        Bitmap {
            width,
            height,
            luma,
            alpha: None,
        }
    }

    #[test]
    fn a_picture_burns_one_line_per_interval_with_power_following_the_grey() {
        let job = JobSetup::default();
        let image = gradient(11, 5);
        let params = LaserPictureParams {
            width: 11.0,
            height: 5.0,
            line_interval: 0.5,
            max_power: Some(90.0),
            min_power: 10.0,
            ..LaserPictureParams::default()
        };
        let out = laser_picture(&image, &job, &laser(80.0), &params).expect("burns");
        let ys: std::collections::BTreeSet<i64> = burns(&out.moves)
            .iter()
            .map(|m| (m.to.y * 1000.0).round() as i64)
            .collect();
        assert_eq!(ys.len(), 10, "5 mm at 0.5 mm is ten lines");
        let powers: Vec<f64> = burns(&out.moves).iter().filter_map(|m| m.power).collect();
        let (lo, hi) = powers
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        assert!((85.0..=90.0).contains(&hi), "{hi}");
        assert!(lo >= 10.0, "{lo}");
    }

    #[test]
    fn dithered_pictures_are_fully_on_or_off() {
        let job = JobSetup::default();
        let image = gradient(40, 10);
        for dither in [Dither::FloydSteinberg, Dither::Ordered] {
            let params = LaserPictureParams {
                width: 20.0,
                line_interval: 0.5,
                mode: PictureMode::Dither,
                dither,
                overscan: 1.0,
                ..LaserPictureParams::default()
            };
            let out = laser_picture(&image, &job, &laser(70.0), &params).expect("burns");
            assert!(burns(&out.moves)
                .iter()
                .all(|m| m.power == Some(0.0) || m.power == Some(70.0)));
            let min_x = out.moves.iter().map(|m| m.to.x).fold(f64::MAX, f64::min);
            assert!(min_x < 0.0, "overscan runs past the left edge");
        }
    }

    #[test]
    fn a_blank_or_oversized_picture_is_refused() {
        let job = JobSetup::default();
        let white = Bitmap {
            width: 4,
            height: 4,
            luma: vec![255; 16],
            alpha: None,
        };
        let params = LaserPictureParams {
            width: 4.0,
            ..LaserPictureParams::default()
        };
        assert!(laser_picture(&white, &job, &laser(50.0), &params).is_err());
        let inverted = LaserPictureParams {
            invert: true,
            ..params.clone()
        };
        assert!(laser_picture(&white, &job, &laser(50.0), &inverted).is_ok());
        let huge = LaserPictureParams {
            width: 5000.0,
            line_interval: 0.01,
            ..params.clone()
        };
        assert!(laser_picture(&white, &job, &laser(50.0), &huge).is_err());
        let backwards = LaserPictureParams {
            min_power: 60.0,
            ..params
        };
        assert!(laser_picture(&white, &job, &laser(50.0), &backwards).is_err());
    }

    /// A 20 by 10 mm picture at the origin, white on the left to black on
    /// the right, in 0.5 mm cells.
    fn tone() -> super::super::sketch::ToneGrid {
        let (cols, rows) = (40, 20);
        let dark = (0..rows)
            .flat_map(|_| (0..cols).map(|i| i as f32 / (cols - 1) as f32))
            .collect();
        super::super::sketch::ToneGrid {
            cols,
            rows,
            cell: 0.5,
            origin: Point::new(0.0, 0.0),
            dark,
            inside: vec![true; cols * rows],
        }
    }

    fn picture(params: LaserPictureParams) -> Result<Calculated, CoreError> {
        laser_picture_tone(
            &tone(),
            &[],
            &JobSetup::default(),
            &laser(80.0),
            &params,
            tol(),
        )
    }

    fn burn_rows(moves: &[Move], along_x: bool) -> usize {
        let mut rows: Vec<i64> = moves
            .iter()
            .filter(|m| m.power.is_some_and(|p| p > 0.0))
            .map(|m| ((if along_x { m.to.y } else { m.to.x }) * 1000.0).round() as i64)
            .collect();
        rows.sort_unstable();
        rows.dedup();
        rows.len()
    }

    #[test]
    fn a_selected_bitmap_burns_rasters_at_the_line_angle_with_power_following_the_grey() {
        let params = LaserPictureParams {
            line_interval: 1.0,
            ..LaserPictureParams::default()
        };
        let out = picture(params.clone()).expect("burns");
        assert_eq!(
            burn_rows(&out.moves, true),
            10,
            "a line a millimetre up the picture"
        );
        // Darker to the right: the power rises with X along a line.
        let row: Vec<&Move> = out
            .moves
            .iter()
            .filter(|m| m.power.is_some_and(|p| p > 0.0) && (m.to.y - 0.5).abs() < 1e-6)
            .collect();
        let right = row
            .iter()
            .max_by(|a, b| a.to.x.total_cmp(&b.to.x))
            .expect("right");
        let left = row
            .iter()
            .min_by(|a, b| a.to.x.total_cmp(&b.to.x))
            .expect("left");
        assert!(right.power > left.power);
        // At 90 degrees the lines run up the picture.
        let up = picture(LaserPictureParams {
            line_angle: 90.0,
            ..params.clone()
        })
        .expect("burns");
        assert_eq!(burn_rows(&up.moves, false), 20);
        // Hatch burns both sets.
        let hatch = picture(LaserPictureParams {
            strategy: PictureStrategy::Hatch,
            ..params.clone()
        })
        .expect("burns");
        assert!(burns(&hatch.moves).len() > burns(&out.moves).len());
        // Dither Image: every dot fully on or off.
        let dots = picture(LaserPictureParams {
            mode: PictureMode::Dither,
            ..params.clone()
        })
        .expect("burns");
        assert!(burns(&dots.moves)
            .iter()
            .all(|m| m.power == Some(0.0) || m.power == Some(80.0)));
        // Two passes burn it twice.
        let twice = picture(LaserPictureParams {
            repeats: 2,
            ..params.clone()
        })
        .expect("burns");
        assert_eq!(burns(&twice.moves).len(), 2 * burns(&out.moves).len());
        assert!(picture(LaserPictureParams {
            line_angle: 95.0,
            ..params.clone()
        })
        .is_err());
    }

    #[test]
    fn selected_vectors_burn_the_picture_along_them() {
        let params = LaserPictureParams {
            strategy: PictureStrategy::Vectors,
            line_interval: 0.5,
            ..LaserPictureParams::default()
        };
        assert!(picture(params.clone()).is_err(), "no vectors");
        let line =
            vec![
                crate::geom::polyline_path(&[Point::new(0.0, 5.0), Point::new(20.0, 5.0)], false)
                    .expect("line"),
            ];
        let out = laser_picture_tone(
            &tone(),
            &line,
            &JobSetup::default(),
            &laser(80.0),
            &params,
            tol(),
        )
        .expect("burns");
        assert!(burns(&out.moves)
            .iter()
            .all(|m| (m.to.y - 5.0).abs() < 1e-6));
    }
}
