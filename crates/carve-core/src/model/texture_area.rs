//! Create Texture Area: one component (the tile) repeated across a region,
//! with spacing between the tiles, alternate rows and columns shifted, and
//! each tile of every group of four reflected its own way.
//!
//! The tile is the box round the seed component, or a box the user has
//! moved and sized; the seed's heights are stretched into it. Tiles sit on
//! a lattice from the tile's lower left corner, `width * (1 + spacing_x)`
//! apart across and `height * (1 + spacing_y)` up. Odd rows move across by
//! `shift_x` of the width and odd columns up by `shift_y` of the height.
//! Where tiles overlap (negative spacing) the higher wins; inside the
//! region where no tile reaches the texture is flat at zero.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{Path, Point, Region, Tolerance};

use super::field::Field;
use super::shape::{inside_cells, Grid};

/// How one tile of a group of four is reflected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reflect {
    /// As the seed is.
    #[default]
    None,
    /// Mirrored left to right.
    Horizontal,
    /// Mirrored top to bottom.
    Vertical,
    /// Mirrored both ways.
    Both,
}

/// The tile's box in job space, millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileBox {
    /// Lower left corner.
    pub x: f64,
    pub y: f64,
    /// Size across and up, more than zero.
    pub width: f64,
    pub height: f64,
}

/// The Create Texture Area form's settings. Spacing and shifts are percent
/// of the tile's width or height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextureParams {
    /// Gap between tiles across, percent of the width; below zero overlaps.
    pub spacing_x: f64,
    /// Gap between tiles up, percent of the height.
    pub spacing_y: f64,
    /// Odd rows move across by this percent of the width.
    pub shift_x: f64,
    /// Odd columns move up by this percent of the height.
    pub shift_y: f64,
    /// Reflection of the lower left, lower right, upper left and upper
    /// right tile of every group of four.
    pub reflect: [Reflect; 4],
    /// The tile's box; the seed's own box when absent.
    pub tile: Option<TileBox>,
    /// Turn of the tile lattice, degrees counter clockwise about the tile
    /// box's lower left corner, which a turned texture area keeps.
    pub angle: f64,
}

impl Default for TextureParams {
    fn default() -> Self {
        TextureParams {
            spacing_x: 0.0,
            spacing_y: 0.0,
            shift_x: 0.0,
            shift_y: 0.0,
            reflect: [Reflect::None; 4],
            tile: None,
            angle: 0.0,
        }
    }
}

/// What a texture area was made from, kept on its component so the tool
/// can edit it again and resizing changes the area, not the tiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSource {
    /// Id of the seed component.
    pub seed: u64,
    /// The area's outline, fixed when it was applied (the job's rectangle
    /// when no vectors were chosen).
    pub boundary: Vec<Path>,
    /// Settings, with the tile box filled in.
    pub params: TextureParams,
}

impl TextureSource {
    /// The source after the texture area component is moved, sized or
    /// turned by `m`: the outline follows, the tiles keep their size and
    /// their lattice moves (and turns) with the outline. Answers none for a
    /// change the tiles cannot follow (a mirror, a slant, or a turn with a
    /// scale), which leaves a plain component.
    pub fn transformed(&self, m: &crate::geom::Mat3) -> Option<TextureSource> {
        const EPS: f64 = 1e-9;
        // A turn (a half turn included, whose sine is only rounding) keeps
        // the tiles; so does a move or a size without one.
        let rigid = (m.a - m.d).abs() <= EPS
            && (m.b + m.c).abs() <= EPS
            && (m.a * m.a + m.b * m.b - 1.0).abs() <= 1e-6;
        let turn = if rigid {
            m.b.atan2(m.a).to_degrees()
        } else if m.b.abs() <= EPS && m.c.abs() <= EPS && m.a > 0.0 && m.d > 0.0 {
            0.0
        } else {
            return None;
        };
        // A lattice already turned only follows moves and turns.
        if self.params.angle.abs() > EPS && !rigid {
            return None;
        }
        let mut params = self.params;
        if let Some(tile) = params.tile {
            let corner = m.apply(Point::new(tile.x, tile.y));
            params.tile = Some(TileBox {
                x: corner.x,
                y: corner.y,
                ..tile
            });
        }
        params.angle = normalised_degrees(params.angle + turn);
        Some(TextureSource {
            seed: self.seed,
            boundary: self
                .boundary
                .iter()
                .map(|p| crate::geom::transform_path(p, m))
                .collect(),
            params,
        })
    }
}

/// An angle in degrees brought into -180 to 180.
fn normalised_degrees(angle: f64) -> f64 {
    let a = angle.rem_euclid(360.0);
    if a > 180.0 {
        a - 360.0
    } else {
        a
    }
}

/// The box round a field's non-empty cells (outer cell edges), or none.
pub fn seed_box(seed: &Field) -> Option<TileBox> {
    let (mut lo, mut hi) = (
        Point::new(f64::INFINITY, f64::INFINITY),
        Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for j in 0..seed.height {
        for i in 0..seed.width {
            if !seed.z[j * seed.width + i].is_nan() {
                let c = seed.centre(i, j);
                lo = Point::new(lo.x.min(c.x), lo.y.min(c.y));
                hi = Point::new(hi.x.max(c.x), hi.y.max(c.y));
            }
        }
    }
    if !lo.x.is_finite() {
        return None;
    }
    let half = seed.cell / 2.0;
    Some(TileBox {
        x: lo.x - half,
        y: lo.y - half,
        width: hi.x - lo.x + seed.cell,
        height: hi.y - lo.y + seed.cell,
    })
}

fn check(params: &TextureParams, tile: &TileBox) -> Result<(), CoreError> {
    let all = [
        params.spacing_x,
        params.spacing_y,
        params.shift_x,
        params.shift_y,
        tile.x,
        tile.y,
        tile.width,
        tile.height,
    ];
    if all.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::validation(
            "the texture settings must be numbers",
        ));
    }
    if tile.width <= 0.0 || tile.height <= 0.0 {
        return Err(CoreError::validation("the tile must have a size"));
    }
    if params.spacing_x <= -90.0 || params.spacing_y <= -90.0 {
        return Err(CoreError::validation(
            "the spacing must be more than -90 percent",
        ));
    }
    Ok(())
}

/// Builds the texture over the whole grid: `seed` tiled across the cells
/// inside `boundary`, NaN outside it.
///
/// # Errors
/// [`CoreError::Validation`] for settings that are not numbers, a tile
/// with no size, spacing of -90 percent or less, an empty seed, or a
/// boundary that covers no cell.
pub fn texture_area(
    seed: &Field,
    boundary: &[Path],
    grid: &Grid,
    params: &TextureParams,
    tol: Tolerance,
) -> Result<Field, CoreError> {
    let own = seed_box(seed).ok_or_else(|| CoreError::validation("the tile component is empty"))?;
    let tile = params.tile.unwrap_or(own);
    check(params, &tile)?;
    let region = Region::from_paths(boundary, tol);
    let inside = inside_cells(&region, grid, tol);
    if !inside.iter().any(|&c| c) {
        return Err(CoreError::validation(
            "the boundary is too small for the model resolution, or not closed",
        ));
    }
    let px = tile.width * (1.0 + params.spacing_x / 100.0);
    let py = tile.height * (1.0 + params.spacing_y / 100.0);
    let sx = tile.width * params.shift_x / 100.0;
    let sy = tile.height * params.shift_y / 100.0;
    // Tiles that can reach a point, either side of the one it falls in.
    let reach_x = (tile.width / px).ceil() as i64 + 1;
    let reach_y = (tile.height / py).ceil() as i64 + 1;
    let odd = |k: i64| k.rem_euclid(2) == 1;
    // A turned lattice: each cell centre is taken back into the lattice's
    // own frame about the tile's corner.
    let (sin, cos) = (-params.angle).to_radians().sin_cos();
    let unturn = |p: Point| -> Point {
        if params.angle == 0.0 {
            return p;
        }
        let (dx, dy) = (p.x - tile.x, p.y - tile.y);
        Point::new(tile.x + dx * cos - dy * sin, tile.y + dx * sin + dy * cos)
    };
    let sample = |u: f64, v: f64| -> f32 {
        // (u, v) from 0 to 1 across the tile, onto the seed's own box.
        seed.at(Point::new(own.x + u * own.width, own.y + v * own.height))
    };

    let mut field = Field::empty(grid.width, grid.height, grid.cell, grid.origin);
    for j in 0..grid.height {
        for i in 0..grid.width {
            let index = j * grid.width + i;
            if !inside[index] {
                continue;
            }
            let p = unturn(field.centre(i, j));
            let row = ((p.y - tile.y) / py).floor() as i64;
            let col = ((p.x - tile.x) / px).floor() as i64;
            let mut best = f32::NAN;
            for r in row - reach_y..=row + reach_y {
                for c in col - reach_x..=col + reach_x {
                    let ox = tile.x + c as f64 * px + if odd(r) { sx } else { 0.0 };
                    let oy = tile.y + r as f64 * py + if odd(c) { sy } else { 0.0 };
                    let (mut u, mut v) = ((p.x - ox) / tile.width, (p.y - oy) / tile.height);
                    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                        continue;
                    }
                    let quarter = usize::from(odd(c)) + 2 * usize::from(odd(r));
                    match params.reflect[quarter] {
                        Reflect::None => {}
                        Reflect::Horizontal => u = 1.0 - u,
                        Reflect::Vertical => v = 1.0 - v,
                        Reflect::Both => {
                            u = 1.0 - u;
                            v = 1.0 - v;
                        }
                    }
                    let z = sample(u, v);
                    if !z.is_nan() && (best.is_nan() || z > best) {
                        best = z;
                    }
                }
            }
            field.z[index] = if best.is_nan() { 0.0 } else { best };
        }
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rect;

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn grid() -> Grid {
        Grid {
            width: 400,
            height: 400,
            cell: 0.25,
            origin: Point::new(0.125, 0.125),
        }
    }

    /// A 10 by 10 mm seed at (0, 0) rising from 0 at its left to 1 at its
    /// right, with a 1 mm step along its bottom row.
    fn seed() -> Field {
        let g = grid();
        let mut f = Field::empty(40, 40, g.cell, g.origin);
        for j in 0..40 {
            for i in 0..40 {
                let x = f64::from(i as u32) / 39.0;
                f.z[j * 40 + i] = (x + if j < 4 { 1.0 } else { 0.0 }) as f32;
            }
        }
        f
    }

    fn area() -> Vec<Path> {
        vec![rect(Point::new(0.0, 0.0), Point::new(100.0, 100.0)).expect("rect")]
    }

    fn at(field: &Field, x: f64, y: f64) -> f64 {
        f64::from(field.at(Point::new(x, y)))
    }

    #[test]
    fn a_turned_lattice_turns_the_tiles_about_the_tile_corner() {
        let params = TextureParams {
            tile: Some(TileBox {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }),
            angle: 90.0,
            ..TextureParams::default()
        };
        let field = texture_area(&seed(), &area(), &grid(), &params, tol()).expect("texture");
        // Turned a quarter, the seed's left to right rise runs bottom to top
        // and its step along the bottom row lies along the right of a tile
        // (lattice x 0 to 1 mm is job x -1 to 0, the tile to the left).
        assert!((at(&field, 5.0, 2.0) - 0.2).abs() < 0.06);
        assert!((at(&field, 5.0, 8.0) - 0.8).abs() < 0.06);
        assert!(at(&field, 9.5, 5.0) > 1.0);
        assert!(at(&field, 5.0, 5.0) < 1.0);
    }

    #[test]
    fn moves_and_turns_carry_the_lattice_and_mirrors_do_not() {
        use crate::geom::{Mat3, Vec2};
        let source = TextureSource {
            seed: 1,
            boundary: area(),
            params: TextureParams {
                tile: Some(TileBox {
                    x: 10.0,
                    y: 0.0,
                    width: 5.0,
                    height: 5.0,
                }),
                ..TextureParams::default()
            },
        };
        let turned = source
            .transformed(&Mat3::rotate_about(90f64.to_radians(), Point::ORIGIN))
            .expect("a turn");
        assert!((turned.params.angle - 90.0).abs() < 1e-9);
        let corner = turned.params.tile.expect("tile");
        assert!(corner.x.abs() < 1e-9 && (corner.y - 10.0).abs() < 1e-9);
        assert_eq!(corner.width, 5.0);
        let moved = turned
            .transformed(&Mat3::translate(Vec2::new(3.0, 0.0)))
            .expect("a move");
        assert!((moved.params.angle - 90.0).abs() < 1e-9);
        assert!(source.transformed(&Mat3::scale(-1.0, 1.0)).is_none());
        assert!(turned.transformed(&Mat3::scale(2.0, 1.0)).is_none());
        assert!(source.transformed(&Mat3::scale(2.0, 1.0)).is_some());
    }

    #[test]
    fn the_seed_repeats_edge_to_edge() {
        let field = texture_area(&seed(), &area(), &grid(), &TextureParams::default(), tol())
            .expect("texture");
        for (x, y) in [(2.0, 5.0), (32.0, 45.0), (92.0, 85.0)] {
            let want = 0.2;
            assert!((at(&field, x, y) - want).abs() < 0.05, "{x} {y}");
        }
        assert!(
            at(&field, 52.0, 50.5) > 1.0,
            "the step on every tile's bottom"
        );
        assert!(field.at(Point::new(99.9, 99.9)).is_finite());
        let tile = seed_box(&seed()).expect("box");
        assert!((tile.width - 10.0).abs() < 1e-9 && tile.x.abs() < 1e-9);
    }

    #[test]
    fn spacing_leaves_flat_gaps_and_shift_moves_odd_rows() {
        let spaced = TextureParams {
            spacing_x: 50.0,
            ..TextureParams::default()
        };
        let field = texture_area(&seed(), &area(), &grid(), &spaced, tol()).expect("texture");
        assert!(
            at(&field, 12.0, 5.0).abs() < 1e-6,
            "a gap after the first tile"
        );
        assert!(
            (at(&field, 17.0, 5.0) - 0.2).abs() < 0.05,
            "the next tile at 15"
        );
        let shifted = TextureParams {
            shift_x: 50.0,
            ..TextureParams::default()
        };
        let field = texture_area(&seed(), &area(), &grid(), &shifted, tol()).expect("texture");
        // Row 1 (y 10 to 20) starts its tiles at x 5, 15, ...
        assert!((at(&field, 7.0, 15.0) - 0.2).abs() < 0.05);
        assert!((at(&field, 2.0, 5.0) - 0.2).abs() < 0.05, "row 0 as it was");
        let bad = TextureParams {
            spacing_x: -95.0,
            ..TextureParams::default()
        };
        assert!(texture_area(&seed(), &area(), &grid(), &bad, tol()).is_err());
    }

    #[test]
    fn each_tile_of_a_group_of_four_reflects_its_own_way() {
        let params = TextureParams {
            reflect: [
                Reflect::None,
                Reflect::Horizontal,
                Reflect::Vertical,
                Reflect::Both,
            ],
            ..TextureParams::default()
        };
        let field = texture_area(&seed(), &area(), &grid(), &params, tol()).expect("texture");
        assert!(
            (at(&field, 2.0, 5.0) - 0.2).abs() < 0.05,
            "lower left as the seed"
        );
        assert!(
            (at(&field, 12.0, 5.0) - 0.8).abs() < 0.05,
            "lower right mirrored"
        );
        assert!(
            at(&field, 2.0, 19.5) > 1.0,
            "upper left: the step at its top"
        );
        assert!(at(&field, 12.0, 19.5) > 1.7, "upper right: both");
    }

    #[test]
    fn a_moved_and_sized_tile_stretches_the_seed() {
        let params = TextureParams {
            tile: Some(TileBox {
                x: 5.0,
                y: 0.0,
                width: 20.0,
                height: 10.0,
            }),
            ..TextureParams::default()
        };
        let field = texture_area(&seed(), &area(), &grid(), &params, tol()).expect("texture");
        assert!(
            (at(&field, 15.0, 5.0) - 0.5).abs() < 0.05,
            "middle of the first tile"
        );
        assert!(
            (at(&field, 3.0, 5.0) - 0.9).abs() < 0.05,
            "the tile before it"
        );
        let empty = Field::empty(4, 4, 1.0, Point::new(0.5, 0.5));
        assert!(texture_area(&empty, &area(), &grid(), &params, tol()).is_err());
    }

    #[test]
    fn only_the_boundary_is_filled() {
        let small = vec![rect(Point::new(20.0, 20.0), Point::new(40.0, 40.0)).expect("rect")];
        let field = texture_area(&seed(), &small, &grid(), &TextureParams::default(), tol())
            .expect("texture");
        assert!(field.at(Point::new(10.0, 10.0)).is_nan());
        assert!(
            (at(&field, 22.0, 25.0) - 0.2).abs() < 0.05,
            "tiles keep their lattice"
        );
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::geom::{rect, Mat3};

    fn source() -> TextureSource {
        TextureSource {
            seed: 1,
            boundary: vec![rect(Point::new(0.0, 0.0), Point::new(40.0, 20.0)).expect("rect")],
            params: TextureParams {
                tile: Some(TileBox {
                    x: 0.0,
                    y: 0.0,
                    width: 5.0,
                    height: 5.0,
                }),
                ..TextureParams::default()
            },
        }
    }

    #[test]
    fn a_half_turn_keeps_the_tiles_like_any_other_turn() {
        let centre = Point::new(20.0, 10.0);
        let half = source()
            .transformed(&Mat3::rotate_about(std::f64::consts::PI, centre))
            .expect("a half turn is a turn");
        assert!((half.params.angle.abs() - 180.0).abs() < 1e-9);
        let corner = half.params.tile.expect("tile");
        assert!((corner.x - 40.0).abs() < 1e-9 && (corner.y - 20.0).abs() < 1e-9);
        // Mirrored both ways is the same half turn.
        assert!(source().transformed(&Mat3::scale(-1.0, -1.0)).is_some());
        // A turned lattice still follows a plain move after a half turn.
        assert!(half
            .transformed(&Mat3::translate(crate::geom::Vec2::new(1.0, 0.0)))
            .is_some());
    }
}
