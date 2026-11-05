//! Position Model: where the zero plane cuts the oriented mesh, whether the
//! part below it is kept, whether a double sided job gets the underside as
//! a second component, and which faces are undercuts (hidden from above,
//! so they become vertical walls down from the silhouette).

use serde::{Deserialize, Serialize};

use crate::doc::JobSetup;
use crate::error::CoreError;
use crate::geom::Point;
use crate::io::mesh::Triangle;

use super::super::field::Field;
use super::super::shape::Grid;
use super::raster::{window, Hits};
use super::vec3;

/// The Position Model settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Placement {
    /// Height of the zero plane above the model's lowest point, mm.
    pub zero_plane: f64,
    /// On a double sided job, also make the underside seen from below as a
    /// component on the other side.
    pub both_sides: bool,
    /// Drop whatever lies below the zero plane.
    pub discard_below: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            zero_plane: 0.0,
            both_sides: false,
            discard_below: true,
        }
    }
}

/// The component heights the placement makes, cropped to what they cover.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Seen from above, heights measured from the zero plane.
    pub top: Option<Field>,
    /// Seen from below on the other side of a double sided job, in that
    /// side's job space.
    pub bottom: Option<Field>,
}

/// The cells of `grid` under the bounding box of `triangles`.
fn grid_over(triangles: &[Triangle], grid: &Grid) -> Option<Grid> {
    let (mut lo, mut hi) = (
        Point::new(f64::MAX, f64::MAX),
        Point::new(f64::MIN, f64::MIN),
    );
    for v in triangles.iter().flatten() {
        lo = Point::new(lo.x.min(v[0]), lo.y.min(v[1]));
        hi = Point::new(hi.x.max(v[0]), hi.y.max(v[1]));
    }
    window(grid, lo, hi)
}

/// The highest surface of `triangles` over `grid`, turned into heights by
/// `height` (NaN to leave a cell empty), cropped.
pub fn surface_field(
    triangles: &[Triangle],
    grid: &Grid,
    height: impl Fn(f64) -> f32,
) -> Option<Field> {
    let part = grid_over(triangles, grid)?;
    let hits = Hits::cast(triangles, &part);
    let all = (f64::MIN, f64::MAX);
    let mut field = Field::empty(part.width, part.height, part.cell, part.origin);
    for (cell, slot) in field.z.iter_mut().enumerate() {
        if let Some(h) = hits.highest(cell, all, false) {
            *slot = height(h);
        }
    }
    field.cropped()
}

/// Makes the component heights for `triangles` (oriented, standing on Z
/// zero) placed as `p` says on `grid`.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a zero plane that is not a number
/// or a model that lands outside the material.
pub fn place(
    triangles: &[Triangle],
    grid: &Grid,
    p: &Placement,
    job: &JobSetup,
) -> Result<Placed, CoreError> {
    if !p.zero_plane.is_finite() {
        return Err(CoreError::validation("the zero plane must be a number"));
    }
    let plane = p.zero_plane;
    let both = p.both_sides && job.is_double_sided();
    let discard = p.discard_below || both;
    let top = surface_field(triangles, grid, |h| {
        let v = h - plane;
        if discard && v < 0.0 {
            f32::NAN
        } else {
            v as f32
        }
    });
    let bottom = if both {
        // Seen from below: the material turned over, so X or Y mirrors and
        // depth below the plane becomes height.
        let turned: Vec<Triangle> = triangles
            .iter()
            .map(|t| {
                t.map(|v| {
                    let (x, y) = job.flip_xy(v[0], v[1]);
                    [x, y, plane - v[2]]
                })
            })
            .collect();
        surface_field(&turned, grid, |h| if h > 0.0 { h as f32 } else { f32::NAN })
    } else {
        None
    };
    if top.is_none() && bottom.is_none() {
        return Err(CoreError::validation(
            "the model lands outside the material",
        ));
    }
    Ok(Placed { top, bottom })
}

/// Cells across the grid undercuts are judged on.
const UNDERCUT_CELLS: f64 = 256.0;

/// For each triangle, whether it is an undercut: it faces down, or
/// something above hides it from a tool coming straight down.
pub fn undercuts(triangles: &[Triangle]) -> Vec<bool> {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in triangles.iter().flatten() {
        for a in 0..3 {
            lo[a] = lo[a].min(v[a]);
            hi[a] = hi[a].max(v[a]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if !span.is_finite() || span <= 0.0 {
        return vec![false; triangles.len()];
    }
    let cell = span / UNDERCUT_CELLS;
    let grid = Grid {
        width: ((hi[0] - lo[0]) / cell).ceil() as usize + 1,
        height: ((hi[1] - lo[1]) / cell).ceil() as usize + 1,
        cell,
        origin: Point::new(lo[0], lo[1]),
    };
    let hits = Hits::cast(triangles, &grid);
    let all = (f64::MIN, f64::MAX);
    let slack = 1e-6 * (hi[2] - lo[2]).max(span);
    triangles
        .iter()
        .map(|t| {
            let n = vec3::normal(t);
            let len = vec3::length(n);
            if len < 1e-12 {
                return false;
            }
            let nz = n[2] / len;
            if nz < -0.02 {
                return true;
            }
            let c = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            ];
            let i = ((c[0] - grid.origin.x) / cell).round() as usize;
            let j = ((c[1] - grid.origin.y) / cell).round() as usize;
            let Some(top) = hits.highest(
                j.min(grid.height - 1) * grid.width + i.min(grid.width - 1),
                all,
                false,
            ) else {
                return false;
            };
            // How far a face this steep rises across one cell.
            let tol = 1.5 * cell * (1.0 - nz * nz).max(0.0).sqrt() / nz.max(0.05) + slack;
            c[2] + tol < top
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::raster::tests::{cube, grid};
    use super::*;
    use crate::doc::JobKind;

    #[test]
    fn the_zero_plane_lowers_the_heights_and_discards_below() {
        let mesh = cube([2.0, 2.0, 0.0], [8.0, 8.0, 4.0]);
        let job = JobSetup::default();
        let at = |f: &Field, x: f64, y: f64| f.at(Point::new(x, y));
        let p = Placement {
            zero_plane: 1.0,
            ..Placement::default()
        };
        let placed = place(&mesh, &grid(10, 1.0), &p, &job).expect("places");
        let top = placed.top.expect("a top");
        assert_eq!(at(&top, 5.5, 5.5), 3.0);
        assert!(placed.bottom.is_none());
        // Above the whole model everything is below the plane.
        let p = Placement {
            zero_plane: 5.0,
            ..Placement::default()
        };
        assert!(place(&mesh, &grid(10, 1.0), &p, &job).is_err());
        let p = Placement {
            zero_plane: 5.0,
            discard_below: false,
            ..Placement::default()
        };
        let top = place(&mesh, &grid(10, 1.0), &p, &job)
            .expect("places")
            .top
            .expect("a top");
        assert_eq!(at(&top, 5.5, 5.5), -1.0, "kept below the plane");
    }

    #[test]
    fn both_sides_makes_the_underside_mirrored() {
        let job = JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        };
        let mesh = cube([10.0, 10.0, 0.0], [20.0, 30.0, 6.0]);
        let g = Grid {
            width: 300,
            height: 200,
            cell: 1.0,
            origin: Point::new(0.5, 0.5),
        };
        let p = Placement {
            zero_plane: 2.0,
            both_sides: true,
            discard_below: false,
        };
        let placed = place(&mesh, &g, &p, &job).expect("places");
        let bottom = placed.bottom.expect("an underside");
        assert_eq!(bottom.max(), Some(2.0), "the depth below the plane");
        let (fx, fy) = job.flip_xy(15.0, 20.0);
        assert_eq!(
            bottom.at(Point::new(fx.floor() + 0.5, fy.floor() + 0.5)),
            2.0
        );
        assert_eq!(placed.top.expect("a top").max(), Some(4.0));
        // A single sided job ignores the option.
        let single = place(&mesh, &g, &p, &JobSetup::default()).expect("places");
        assert!(single.bottom.is_none());
    }

    #[test]
    fn a_mushroom_cap_hides_its_underside() {
        // A wide cap on a thin stem: the cap's underside faces down and the
        // stem's top is hidden, but the cap's top is not.
        let mut mesh = cube([4.0, 4.0, 0.0], [6.0, 6.0, 5.0]);
        let cap = cube([0.0, 0.0, 5.0], [10.0, 10.0, 7.0]);
        mesh.extend(&cap);
        let flags = undercuts(&mesh);
        // Cube faces: 0 and 1 bottom, 2 and 3 top.
        assert!(flags[12] && flags[13], "the cap's underside");
        assert!(!flags[14] && !flags[15], "the cap's top");
        assert!(flags[2] && flags[3], "the stem top under the cap");
    }
}
