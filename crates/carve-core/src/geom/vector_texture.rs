//! Vector Texture: rows of parallel, optionally wavy and noisy lines laid
//! across a boundary, for machining as a texture.
//!
//! The rows run along `angle`, `max_spacing` apart; `spacing_variation`
//! shortens some gaps at random, `amplitude` and `wavelength` bend every row
//! into a sine wave and `noise` roughens the rows. The result is clipped to
//! the closed boundary rings (even-odd, so holes stay clear). Randomness
//! comes from a fixed seed, so the same settings give the same vectors.

use serde::{Deserialize, Serialize};

use super::flatten::fit_arcs;
use super::path::{Path, Polyline, SubPath};
use super::point::{Point, Tolerance};
use crate::error::CoreError;

/// Most rows one texture makes.
const MAX_ROWS: usize = 5_000;
/// Most samples along one row.
const MAX_SAMPLES: usize = 20_000;

/// Vector Texture settings; lengths in millimetres, angle in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextureParams {
    /// Direction of the rows, -90 to 90 degrees from the X axis.
    pub angle: f64,
    /// Largest distance between two rows.
    pub max_spacing: f64,
    /// 0 (even rows) to 100 (gaps anywhere from 5 % to all of the spacing).
    pub spacing_variation: f64,
    /// Length of one wave along a row.
    pub wavelength: f64,
    /// Height of the waves either side of the row; 0 keeps rows straight.
    pub amplitude: f64,
    /// 0 (smooth) to 100: random wobble added to the rows.
    pub noise: f64,
    /// Seed of the random choices.
    pub seed: u64,
}

impl Default for TextureParams {
    fn default() -> Self {
        TextureParams {
            angle: 0.0,
            max_spacing: 5.0,
            spacing_variation: 0.0,
            wavelength: 50.0,
            amplitude: 0.0,
            noise: 0.0,
            seed: 1,
        }
    }
}

impl TextureParams {
    /// Checks the ranges.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the first setting out of range.
    pub fn check(&self) -> Result<(), CoreError> {
        let finite = [
            self.angle,
            self.max_spacing,
            self.spacing_variation,
            self.wavelength,
            self.amplitude,
            self.noise,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !finite {
            return Err(CoreError::validation("texture settings must be numbers"));
        }
        if !(-90.0..=90.0).contains(&self.angle) {
            return Err(CoreError::validation(
                "the angle must be from -90 to 90 degrees",
            ));
        }
        if self.max_spacing <= 0.0 {
            return Err(CoreError::validation("the spacing must be more than zero"));
        }
        if !(0.0..=100.0).contains(&self.spacing_variation) || !(0.0..=100.0).contains(&self.noise)
        {
            return Err(CoreError::validation(
                "spacing variation and noise must be from 0 to 100",
            ));
        }
        if self.wavelength <= 0.0 {
            return Err(CoreError::validation(
                "the wavelength must be more than zero",
            ));
        }
        if self.amplitude < 0.0 {
            return Err(CoreError::validation("the amplitude cannot be negative"));
        }
        Ok(())
    }
}

/// Small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// Uniform in [0, 1).
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn rotate(p: Point, cos: f64, sin: f64) -> Point {
    Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos)
}

fn inside(rings: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x > p.x {
                    odd = !odd;
                }
            }
        }
    }
    odd
}

/// Where segment `p`-`q` first crosses a ring edge, as a fraction along it.
fn crossing(rings: &[Vec<Point>], p: Point, q: Point) -> Option<f64> {
    let (dx, dy) = (q.x - p.x, q.y - p.y);
    let mut best: Option<f64> = None;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let denom = dx * ey - dy * ex;
            if denom.abs() < 1e-15 {
                continue;
            }
            let t = ((a.x - p.x) * ey - (a.y - p.y) * ex) / denom;
            let u = ((a.x - p.x) * dy - (a.y - p.y) * dx) / denom;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                best = Some(best.map_or(t, |b: f64| b.min(t)));
            }
        }
    }
    best
}

/// Keeps the parts of `row` inside `rings`, cutting exactly at the edges.
fn clip_row(rings: &[Vec<Point>], row: &[Point]) -> Vec<Vec<Point>> {
    let mut runs = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let Some(&first) = row.first() else {
        return runs;
    };
    let mut was_inside = inside(rings, first);
    if was_inside {
        current.push(first);
    }
    for pair in row.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        let now_inside = inside(rings, q);
        if now_inside != was_inside {
            let t = crossing(rings, p, q).unwrap_or(0.5);
            let cut = Point::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
            current.push(cut);
            if was_inside {
                runs.push(std::mem::take(&mut current));
            }
        }
        if now_inside {
            current.push(q);
        }
        was_inside = now_inside;
    }
    if current.len() >= 2 {
        runs.push(current);
    }
    runs.retain(|run| run.len() >= 2);
    runs
}

/// Smooth random wobble: values at knots `spacing` apart, eased between.
fn wobble(knots: &[f64], spacing: f64, x: f64) -> f64 {
    if knots.is_empty() {
        return 0.0;
    }
    let f = (x / spacing).max(0.0);
    let i = (f.floor() as usize).min(knots.len() - 1);
    let j = (i + 1).min(knots.len() - 1);
    let t = f - f.floor();
    let t = t * t * (3.0 - 2.0 * t);
    knots[i] * (1.0 - t) + knots[j] * t
}

/// Makes the texture inside `rings` (closed polylines in millimetres).
///
/// # Errors
/// [`CoreError::Validation`] for settings out of range or when the rings
/// enclose nothing, and [`CoreError::Validation`] when the spacing would
/// need more rows than the tool makes.
pub fn vector_texture(
    rings: &[Vec<Point>],
    params: &TextureParams,
    tol: Tolerance,
) -> Result<Vec<Path>, CoreError> {
    params.check()?;
    let radians = params.angle.to_radians();
    let (cos, sin) = (radians.cos(), radians.sin());
    // Into the pattern frame, where rows run along X.
    let local: Vec<Vec<Point>> = rings
        .iter()
        .filter(|ring| ring.len() >= 3)
        .map(|ring| ring.iter().map(|p| rotate(*p, cos, -sin)).collect())
        .collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in local.iter().flatten() {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    if local.is_empty() || x1 <= x0 || y1 <= y0 {
        return Err(CoreError::validation(
            "the texture needs a closed boundary with an area",
        ));
    }
    let wobble_height = params.noise / 100.0 * params.max_spacing;
    let reach = params.amplitude + wobble_height;
    let rows_needed = ((y1 - y0 + 2.0 * reach) / (params.max_spacing * 0.05)).ceil();
    let even_rows = (y1 - y0 + 2.0 * reach) / params.max_spacing;
    if even_rows > MAX_ROWS as f64
        || (params.spacing_variation > 0.0 && rows_needed > MAX_ROWS as f64)
    {
        return Err(CoreError::validation(
            "the spacing is too small for this boundary; make it larger",
        ));
    }
    let step = (params.wavelength / 24.0)
        .min(params.max_spacing)
        .max((x1 - x0) / MAX_SAMPLES as f64);
    let samples = ((x1 - x0) / step).ceil() as usize + 1;
    let knot_spacing = (params.wavelength / 2.0).max(step);
    let knot_count = ((x1 - x0) / knot_spacing).ceil() as usize + 2;
    let mut rng = Rng::new(params.seed);
    let mut paths = Vec::new();
    let mut y = y0 - reach + params.max_spacing / 2.0;
    let mut row_points = Vec::with_capacity(samples);
    while y <= y1 + reach {
        let knots: Vec<f64> = if wobble_height > 0.0 {
            (0..knot_count).map(|_| rng.next() * 2.0 - 1.0).collect()
        } else {
            Vec::new()
        };
        row_points.clear();
        for i in 0..samples {
            let x = (x0 + i as f64 * step).min(x1);
            let wave = if params.amplitude > 0.0 {
                params.amplitude * (std::f64::consts::TAU * (x - x0) / params.wavelength).sin()
            } else {
                0.0
            };
            let rough = wobble_height * wobble(&knots, knot_spacing, x - x0);
            row_points.push(Point::new(x, y + wave + rough));
        }
        for run in clip_row(&local, &row_points) {
            // A cut lands anywhere between two samples; drop a sample that
            // sits right next to it so the curve fit gets even spacing.
            let min_gap = step * 0.25;
            let mut world: Vec<Point> = Vec::with_capacity(run.len());
            for (i, p) in run.iter().enumerate() {
                let q = rotate(*p, cos, sin);
                let last = i + 1 == run.len();
                if let Some(prev) = world.last().copied() {
                    if q.distance(prev) < min_gap {
                        if last {
                            world.pop();
                        } else {
                            continue;
                        }
                    }
                }
                world.push(q);
            }
            if world.len() < 2 {
                continue;
            }
            let straight = params.amplitude <= 0.0 && wobble_height <= 0.0;
            let fitted = if straight {
                None
            } else {
                fit_arcs(&Polyline::new(world.clone(), false), tol)
            };
            let sub = fitted.unwrap_or_else(|| {
                let mut sub = SubPath::open(world[0]);
                let rest = if straight {
                    &world[world.len() - 1..]
                } else {
                    &world[1..]
                };
                for p in rest {
                    sub.line_to(*p);
                }
                sub
            });
            paths.push(Path::from_subpath(sub));
        }
        let gap = params.max_spacing * (1.0 - params.spacing_variation / 100.0 * 0.95 * rng.next());
        y += gap;
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::metrics::path_bbox;

    fn square(size: f64) -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0),
            Point::new(size, 0.0),
            Point::new(size, size),
            Point::new(0.0, size),
        ]
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tol")
    }

    #[test]
    fn straight_rows_fill_the_boundary_evenly() {
        let params = TextureParams {
            max_spacing: 10.0,
            ..TextureParams::default()
        };
        let paths = vector_texture(&[square(100.0)], &params, tol()).expect("texture");
        assert_eq!(paths.len(), 10);
        for path in &paths {
            let bbox = path_bbox(path).expect("bbox");
            assert!((bbox.min.x - 0.0).abs() < 1e-6 && (bbox.max.x - 100.0).abs() < 1e-6);
            assert!((bbox.min.y - bbox.max.y).abs() < 1e-9, "rows are straight");
        }
    }

    #[test]
    fn rows_follow_the_angle() {
        let params = TextureParams {
            angle: 90.0,
            max_spacing: 20.0,
            ..TextureParams::default()
        };
        let paths = vector_texture(&[square(100.0)], &params, tol()).expect("texture");
        let bbox = path_bbox(&paths[0]).expect("bbox");
        assert!((bbox.max.x - bbox.min.x).abs() < 1e-6, "vertical rows");
        assert!((bbox.max.y - bbox.min.y - 100.0).abs() < 1e-6);
    }

    #[test]
    fn waves_stay_inside_the_boundary() {
        let params = TextureParams {
            max_spacing: 8.0,
            amplitude: 3.0,
            wavelength: 30.0,
            noise: 40.0,
            spacing_variation: 60.0,
            ..TextureParams::default()
        };
        let paths = vector_texture(&[square(100.0)], &params, tol()).expect("texture");
        assert!(!paths.is_empty());
        for path in &paths {
            for line in crate::geom::flatten_path(path, tol()) {
                for p in line.points {
                    assert!(p.x >= -0.05 && p.x <= 100.05, "{p:?}");
                    assert!(p.y >= -0.05 && p.y <= 100.05, "{p:?}");
                }
            }
        }
    }

    #[test]
    fn the_same_settings_give_the_same_vectors() {
        let params = TextureParams {
            amplitude: 2.0,
            noise: 50.0,
            spacing_variation: 50.0,
            ..TextureParams::default()
        };
        let a = vector_texture(&[square(50.0)], &params, tol()).expect("a");
        let b = vector_texture(&[square(50.0)], &params, tol()).expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn a_hole_is_left_clear() {
        let hole = vec![
            Point::new(40.0, 20.0),
            Point::new(60.0, 20.0),
            Point::new(60.0, 80.0),
            Point::new(40.0, 80.0),
        ];
        let params = TextureParams {
            max_spacing: 25.0,
            ..TextureParams::default()
        };
        let paths = vector_texture(&[square(100.0), hole], &params, tol()).expect("texture");
        // Rows at 12.5, 37.5, 62.5 and 87.5 mm; the middle two are cut in two.
        assert_eq!(paths.len(), 6);
    }

    #[test]
    fn bad_settings_are_refused() {
        for params in [
            TextureParams {
                max_spacing: 0.0,
                ..TextureParams::default()
            },
            TextureParams {
                angle: 120.0,
                ..TextureParams::default()
            },
            TextureParams {
                max_spacing: 0.001,
                ..TextureParams::default()
            },
        ] {
            assert!(vector_texture(&[square(100.0)], &params, tol()).is_err());
        }
        assert!(vector_texture(&[], &TextureParams::default(), tol()).is_err());
    }
}
