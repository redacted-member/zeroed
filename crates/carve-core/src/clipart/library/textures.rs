//! Tileable textures: relief tiles whose heights repeat exactly every tile,
//! so the Texture Area tool (or copies laid side by side) lays them with no
//! seam. Every formula here is periodic in the tile's width and height;
//! the noise and cell patterns wrap their lattices to the tile.

use std::f64::consts::{PI, TAU};

use crate::model::field::Field;

use super::art::{self, smoothstep};
use super::{Adder, Entry};

const SOURCE: &str = "textures.rs";

/// Tile size, millimetres.
const L: f64 = 25.0;

/// A repeatable pseudo random value in 0..1 for lattice point `(i, j)`.
fn hash(i: i64, j: i64, seed: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth value noise in 0..1 on an `n` by `n` lattice across the tile,
/// wrapping at the tile's edges.
fn noise(x: f64, y: f64, n: i64, seed: u64) -> f64 {
    noise_xy(x, y, n, n, seed)
}

/// [`noise`] with `nx` lattice steps across and `ny` down, for grain that
/// runs one way.
fn noise_xy(x: f64, y: f64, nx: i64, ny: i64, seed: u64) -> f64 {
    let (u, v) = (x / L * nx as f64, y / L * ny as f64);
    let (i, j) = (u.floor() as i64, v.floor() as i64);
    let (fu, fv) = (u - i as f64, v - j as f64);
    let s = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let at = |a: i64, b: i64| hash(a.rem_euclid(nx), b.rem_euclid(ny), seed);
    let (su, sv) = (s(fu), s(fv));
    let top = at(i, j) + (at(i + 1, j) - at(i, j)) * su;
    let bottom = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * su;
    top + (bottom - top) * sv
}

/// Several octaves of [`noise`], halving the amplitude as the lattice
/// doubles, in 0..1.
fn fbm(x: f64, y: f64, n: i64, octaves: u32, seed: u64) -> f64 {
    let (mut sum, mut amp, mut total) = (0.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * noise(x, y, n << o, seed + u64::from(o));
        total += amp;
        amp *= 0.5;
    }
    sum / total
}

/// Distances to the nearest and second nearest of `n * n` jittered points
/// spread over the tile, the pattern wrapping at its edges.
fn cells(x: f64, y: f64, n: i64, seed: u64) -> (f64, f64) {
    let size = L / n as f64;
    let (ci, cj) = ((x / size).floor() as i64, (y / size).floor() as i64);
    let (mut f1, mut f2) = (f64::MAX, f64::MAX);
    for dj in -1..=1 {
        for di in -1..=1 {
            let (i, j) = (ci + di, cj + dj);
            let (wi, wj) = (i.rem_euclid(n), j.rem_euclid(n));
            let px = (i as f64 + 0.15 + 0.7 * hash(wi, wj, seed)) * size;
            let py = (j as f64 + 0.15 + 0.7 * hash(wi, wj, seed + 7)) * size;
            let d = (x - px).hypot(y - py);
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2)
}

/// `x` folded into `0..period`.
fn wrap(x: f64, period: f64) -> f64 {
    x.rem_euclid(period)
}

/// A cosine wave along `x` repeating `k` times across the tile.
fn wave(x: f64, k: f64) -> f64 {
    (TAU * k * x / L).cos()
}

/// A rounded cushion over a rectangle, from the distance `d` to its edge.
fn cushion(d: f64, soft: f64) -> f64 {
    art::round_cap((d / soft).clamp(0.0, 1.0))
}

type Texture = (&'static str, &'static [&'static str], fn(f64, f64) -> f64);

fn geometric() -> Vec<Texture> {
    vec![
        ("Horizontal Waves", &["waves", "lines"], |_x, y| {
            1.0 + 0.6 * wave(y, 5.0)
        }),
        ("Diagonal Waves", &["waves", "diagonal"], |x, y| {
            1.0 + 0.6 * wave(x + y, 4.0)
        }),
        ("Egg Crate", &["egg crate", "bumps"], |x, y| {
            1.2 + 0.5 * wave(x, 4.0) * wave(y, 4.0)
        }),
        ("Cross Waves", &["waves", "cross"], |x, y| {
            1.2 + 0.35 * wave(x, 5.0) + 0.35 * wave(y, 5.0)
        }),
        ("Pyramids", &["pyramids", "diamond", "faceted"], |x, y| {
            let (u, v) = (
                wrap(x, L / 5.0) / (L / 5.0) - 0.5,
                wrap(y, L / 5.0) / (L / 5.0) - 0.5,
            );
            0.3 + 2.0 * (0.5 - u.abs().max(v.abs()))
        }),
        ("Diamond Pyramids", &["pyramids", "diamond"], |x, y| {
            let (u, v) = (
                wrap(x, L / 5.0) / (L / 5.0) - 0.5,
                wrap(y, L / 5.0) / (L / 5.0) - 0.5,
            );
            0.3 + 1.6 * (0.5 - (u.abs() + v.abs()).min(0.5))
        }),
        ("Dimples", &["dimples", "dots"], |x, y| {
            let s = L / 6.0;
            let (u, v) = (wrap(x, s) - s / 2.0, wrap(y, s) - s / 2.0);
            let r = u.hypot(v) / (s * 0.42);
            2.0 - 1.2 * (1.0 - r * r).max(0.0).sqrt()
        }),
        ("Studs", &["studs", "dots", "domes"], |x, y| {
            let s = L / 6.0;
            let (u, v) = (wrap(x, s) - s / 2.0, wrap(y, s) - s / 2.0);
            let r = u.hypot(v) / (s * 0.4);
            0.5 + 1.4 * (1.0 - r * r).max(0.0).sqrt()
        }),
        ("Offset Dots", &["dots", "offset"], |x, y| {
            let s = L / 6.0;
            let row = (y / s).floor() as i64;
            let shift = if row.rem_euclid(2) == 1 { s / 2.0 } else { 0.0 };
            let (u, v) = (wrap(x + shift, s) - s / 2.0, wrap(y, s) - s / 2.0);
            let r = u.hypot(v) / (s * 0.3);
            0.5 + 1.0 * (1.0 - r * r).max(0.0).sqrt()
        }),
        ("Checker", &["checker", "squares"], |x, y| {
            let s = L / 6.0;
            let (i, j) = ((x / s).floor() as i64, (y / s).floor() as i64);
            let (u, v) = (wrap(x, s), wrap(y, s));
            let d = u.min(s - u).min(v).min(s - v);
            if (i + j).rem_euclid(2) == 0 {
                0.5 + 1.0 * cushion(d, 1.0)
            } else {
                0.5
            }
        }),
        (
            "Chevron Ridges",
            &["chevron", "zigzag", "ridges"],
            |x, y| {
                let s = L / 5.0;
                let u = wrap(x, s) / s;
                let zig = (u - 0.5).abs() * s;
                1.0 + 0.6 * wave(y + zig, 5.0)
            },
        ),
        ("Crosshatch", &["crosshatch", "grooves"], |x, y| {
            let a = 0.5 + 0.5 * wave(x + y, 6.0);
            let b = 0.5 + 0.5 * wave(x - y, 6.0);
            0.5 + 0.8 * a.min(b)
        }),
        ("Knurled", &["knurled", "grip", "diamond"], |x, y| {
            let tri = |t: f64| {
                let f = wrap(t / (L / 8.0), 1.0);
                2.0 * f.min(1.0 - f)
            };
            0.3 + 1.2 * tri(x + y).min(tri(x - y))
        }),
        ("Flutes", &["flutes", "grooves", "vertical"], |x, _y| {
            let s = L / 6.0;
            let u = wrap(x, s) / s - 0.5;
            1.8 - 1.0
                * (1.0 - (2.0 * u).powi(2)).max(0.0).sqrt()
                * smoothstep(0.0, 0.05, 0.5 - u.abs())
        }),
        ("Reeds", &["reeds", "beads", "vertical"], |x, _y| {
            let s = L / 6.0;
            let u = wrap(x, s) / s - 0.5;
            0.4 + 1.2 * (1.0 - (2.0 * u).powi(2)).max(0.0).sqrt()
        }),
        ("Corduroy", &["corduroy", "ribs", "fabric"], |x, _y| {
            let s = L / 12.0;
            let u = wrap(x, s) / s - 0.5;
            0.3 + 0.6 * (1.0 - (2.0 * u).powi(2)).max(0.0).powf(0.35)
        }),
        ("Quilted", &["quilted", "diamond", "pillows"], |x, y| {
            let s = L / 4.0;
            let (u, v) = (wrap(x + y, s) / s - 0.5, wrap(x - y, s) / s - 0.5);
            let d = 0.5 - u.abs().max(v.abs());
            0.4 + 1.6 * cushion(d, 0.35)
        }),
        ("Tufted", &["tufted", "cushion", "buttons"], |x, y| {
            let s = L / 4.0;
            let (u, v) = (wrap(x, s), wrap(y, s));
            let d = u.min(s - u).min(v).min(s - v);
            let corner = u.min(s - u).hypot(v.min(s - v));
            let button = 0.9 * (1.0 - (corner / 1.3).min(1.0)).powi(2);
            0.5 + 1.5 * cushion(d, s / 2.0) - button
        }),
        ("Tile Grid", &["tiles", "grout", "squares"], |x, y| {
            let s = L / 4.0;
            let (u, v) = (wrap(x, s), wrap(y, s));
            let d = u.min(s - u).min(v).min(s - v);
            0.4 + 1.0 * cushion((d - 0.3).max(0.0), 0.8)
        }),
        ("Lattice", &["lattice", "grid", "bars"], |x, y| {
            let s = L / 5.0;
            let (u, v) = (wrap(x, s) - s / 2.0, wrap(y, s) - s / 2.0);
            let bar = |w: f64| (1.0 - (w / 0.8).powi(2)).max(0.0).sqrt();
            0.3 + 1.2 * bar(u).max(bar(v))
        }),
        ("Honeycomb", &["honeycomb", "hexagon", "cells"], |x, y| {
            // Regular hexagons, squashed a little down the tile so whole
            // rows fit it exactly: two cells across, four rows down.
            let s = L / 6.0;
            let h = L / 4.0;
            let ky = s * 3f64.sqrt() / h;
            let mut best = f64::MAX;
            for (ox, oy) in [(0.0, 0.0), (1.5 * s, h / 2.0)] {
                let cx0 = ox + ((x - ox) / (3.0 * s)).round() * 3.0 * s;
                let cy0 = oy + ((y - oy) / h).round() * h;
                for (ex, ey) in [
                    (0.0, 0.0),
                    (3.0 * s, 0.0),
                    (-3.0 * s, 0.0),
                    (0.0, h),
                    (0.0, -h),
                ] {
                    let (dx, dy) = ((x - cx0 - ex).abs(), (y - cy0 - ey).abs() * ky);
                    best = best.min((dx * 0.866 + dy * 0.5).max(dy));
                }
            }
            let edge = (s * 0.866 - best).max(0.0);
            0.4 + 1.2 * cushion(edge - 0.25, 0.9)
        }),
    ]
}

fn stone() -> Vec<Texture> {
    vec![
        ("Brick", &["brick", "wall", "running bond"], |x, y| {
            let (bw, bh) = (L / 2.0, L / 6.0);
            let row = (y / bh).floor() as i64;
            let shift = if row.rem_euclid(2) == 1 {
                bw / 2.0
            } else {
                0.0
            };
            let (u, v) = (wrap(x + shift, bw), wrap(y, bh));
            let d = u.min(bw - u).min(v).min(bh - v);
            0.4 + 1.3 * cushion((d - 0.35).max(0.0), 0.7)
        }),
        (
            "Stack Bond Brick",
            &["brick", "wall", "stack bond"],
            |x, y| {
                let (bw, bh) = (L / 2.0, L / 5.0);
                let (u, v) = (wrap(x, bw), wrap(y, bh));
                let d = u.min(bw - u).min(v).min(bh - v);
                0.4 + 1.3 * cushion((d - 0.35).max(0.0), 0.7)
            },
        ),
        ("Parquet", &["parquet", "floor", "wood blocks"], |x, y| {
            let s = L / 4.0;
            let (i, j) = ((x / s).floor() as i64, (y / s).floor() as i64);
            let (u, v) = (wrap(x, s), wrap(y, s));
            let along = if (i + j).rem_euclid(2) == 0 { u } else { v };
            let strip = wrap(along, s / 3.0);
            let d = strip
                .min(s / 3.0 - strip)
                .min(u)
                .min(s - u)
                .min(v)
                .min(s - v);
            0.6 + 0.6 * cushion((d - 0.15).max(0.0), 0.35)
        }),
        (
            "Cobblestone",
            &["cobblestone", "stones", "paving"],
            |x, y| {
                let (f1, f2) = cells(x, y, 5, 3);
                0.3 + 1.6
                    * cushion((f2 - f1) / 2.0 - 0.25, 1.4)
                    * (1.0 - 0.15 * (f1 / 3.0).min(1.0))
            },
        ),
        ("Stone Wall", &["stone", "wall", "rubble"], |x, y| {
            let (f1, f2) = cells(x, y, 4, 11);
            let rough = 0.25 * fbm(x, y, 8, 3, 21);
            0.3 + 1.6 * cushion((f2 - f1) / 2.0 - 0.35, 1.6) + rough
        }),
        ("Pebbles", &["pebbles", "stones", "gravel"], |x, y| {
            let (f1, f2) = cells(x, y, 7, 5);
            let r = f1 / ((f1 + f2) / 2.0);
            0.2 + 1.2 * (1.0 - r * r).max(0.0).sqrt()
        }),
        ("Hammered", &["hammered", "planished", "metal"], |x, y| {
            let (f1, _) = cells(x, y, 7, 9);
            1.5 - 0.35 * (1.0 - (f1 / 2.2).min(1.0).powi(2))
        }),
        ("Rough Rock", &["rock", "stone", "rough"], |x, y| {
            let n = fbm(x, y, 4, 4, 31);
            0.3 + 2.0 * (1.0 - (2.0 * n - 1.0).abs()).powf(1.5)
        }),
        ("Crackle", &["crackle", "cracks", "craquelure"], |x, y| {
            let (f1, f2) = cells(x, y, 6, 13);
            1.2 - 0.6 * (1.0 - smoothstep(0.0, 0.45, f2 - f1))
        }),
        ("Slate", &["slate", "stone", "layers"], |x, y| {
            let n = fbm(x, y, 3, 4, 41);
            0.5 + 1.2 * n + 0.15 * (TAU * 4.0 * n).sin()
        }),
    ]
}

fn natural() -> Vec<Texture> {
    vec![
        ("Stucco", &["stucco", "plaster", "rough"], |x, y| {
            0.4 + 1.2 * fbm(x, y, 5, 4, 1)
        }),
        ("Sandblasted", &["sandblasted", "grain", "fine"], |x, y| {
            0.6 + 0.5 * fbm(x, y, 12, 3, 2)
        }),
        ("Bark", &["bark", "tree", "wood"], |x, y| {
            let n = (noise_xy(x, y, 6, 2, 51) + 0.5 * noise_xy(x, y, 12, 4, 52)) / 1.5;
            let ridges = (TAU * (x / L * 8.0 + 1.5 * n)).cos();
            0.5 + 0.8 * (0.5 + 0.5 * ridges).powf(0.6) + 0.4 * n
        }),
        ("Wood Grain", &["wood", "grain", "rings"], |x, y| {
            let n = fbm(x, y, 3, 3, 61);
            1.0 + 0.25 * (TAU * (x / L * 10.0 + 2.0 * n)).cos()
        }),
        ("Water", &["water", "waves", "ripples"], |x, y| {
            1.2 + 0.25 * wave(x + 2.0 * y, 1.0)
                + 0.2 * wave(3.0 * x - y, 1.0)
                + 0.15 * wave(x - 4.0 * y, 1.0)
        }),
        ("Sand Ripples", &["sand", "ripples", "dunes"], |x, y| {
            let n = fbm(x, y, 3, 2, 71);
            let t = wrap(y / (L / 6.0) + 1.2 * n, 1.0);
            // Gentle windward slope, steeper lee side.
            0.4 + 0.9 * if t < 0.75 { t / 0.75 } else { (1.0 - t) / 0.25 }.powf(1.2)
        }),
        ("Leather", &["leather", "hide", "grain"], |x, y| {
            let (f1, f2) = cells(x, y, 9, 17);
            1.0 - 0.3 * (1.0 - smoothstep(0.0, 0.5, f2 - f1)) + 0.15 * fbm(x, y, 16, 2, 18)
        }),
        ("Fish Scales", &["scales", "fish", "mermaid"], |x, y| {
            let (sw, sh) = (L / 5.0, L / 8.0);
            let row = (y / sh).floor() as i64;
            let shift = if row.rem_euclid(2) == 1 {
                sw / 2.0
            } else {
                0.0
            };
            let u = wrap(x + shift, sw) - sw / 2.0;
            let v = wrap(y, sh);
            // Each scale is a disc hanging from the row above.
            let r = u.hypot(v - sh) / (sw * 0.62);
            0.4 + 1.1 * (1.0 - r.min(1.0)).powf(0.4)
        }),
        ("Snakeskin", &["snake", "scales", "reptile"], |x, y| {
            let (f1, f2) = cells(x, y, 8, 23);
            let lift = (f2 - f1) / (f2 + f1);
            0.3 + 1.3 * lift.powf(0.5)
        }),
        ("Bubbles", &["bubbles", "foam"], |x, y| {
            let (f1, f2) = cells(x, y, 6, 29);
            let r = f1 / (0.45 * (f1 + f2));
            0.3 + 1.2 * (1.0 - r * r).max(0.0).sqrt()
        }),
        ("Stipple", &["stipple", "dots", "fine"], |x, y| {
            let (f1, _) = cells(x, y, 14, 37);
            0.4 + 0.45 * (1.0 - (f1 / 0.45).powi(2)).max(0.0).sqrt()
        }),
    ]
}

fn woven() -> Vec<Texture> {
    vec![
        ("Basket Weave", &["basket", "weave", "woven"], |x, y| {
            let s = L / 4.0;
            let (i, j) = ((x / s).floor() as i64, (y / s).floor() as i64);
            let (u, v) = (wrap(x, s), wrap(y, s));
            let across = if (i + j).rem_euclid(2) == 0 { v } else { u };
            let strip = wrap(across, s / 3.0) / (s / 3.0) - 0.5;
            let border = u.min(s - u).min(v).min(s - v);
            0.4 + 1.0
                * (1.0 - (2.0 * strip).powi(2)).max(0.0).powf(0.4)
                * smoothstep(0.0, 0.6, border)
        }),
        ("Linen", &["linen", "fabric", "weave"], |x, y| {
            let n = 10.0;
            let (u, v) = (x / L * n, y / L * n);
            let warp = (TAU * u).cos() * (PI * v).sin();
            let weft = (TAU * v).cos() * (PI * u).cos();
            0.8 + 0.25 * warp.max(weft)
        }),
        ("Wicker", &["wicker", "cane", "woven"], |x, y| {
            let s = L / 5.0;
            let (a, b) = (x + y, x - y);
            let (ia, ib) = ((a / s).floor() as i64, (b / s).floor() as i64);
            let (ua, ub) = (wrap(a, s) / s - 0.5, wrap(b, s) / s - 0.5);
            let over_a = (ia + ib).rem_euclid(2) == 0;
            let band_a =
                (1.0 - (2.0 * ub).powi(2)).max(0.0).sqrt() * if over_a { 1.0 } else { 0.6 };
            let band_b =
                (1.0 - (2.0 * ua).powi(2)).max(0.0).sqrt() * if over_a { 0.6 } else { 1.0 };
            0.3 + 1.1 * band_a.max(band_b)
        }),
        ("Rope Weave", &["rope", "twist", "weave"], |x, y| {
            let s = L / 5.0;
            let row = wrap(y, s) / s - 0.5;
            let strand = wrap(x / (L / 8.0) + y / (L / 10.0), 1.0);
            let env = (1.0 - (2.0 * row).powi(2)).max(0.0).sqrt();
            0.3 + env * (0.8 + 0.4 * (PI * strand).sin().powf(0.7))
        }),
        ("Twill", &["twill", "fabric", "diagonal"], |x, y| {
            1.0 + 0.35 * wave(x + 2.0 * y, 6.0) + 0.08 * wave(x - y, 12.0)
        }),
    ]
}

pub(super) fn entries(out: &mut Vec<Entry>) {
    for (category, list) in [
        ("Textures/Geometric", geometric()),
        ("Textures/Stone and Brick", stone()),
        ("Textures/Natural", natural()),
        ("Textures/Woven", woven()),
    ] {
        let mut add = Adder {
            out: &mut *out,
            category,
            source: SOURCE,
        };
        for (name, tags, height) in list {
            let mut all = vec!["texture", "tileable"];
            all.extend_from_slice(tags);
            add.tile(name, &all, move || Ok(tile(height)));
        }
    }
}

/// The tile of a texture.
fn tile(height: fn(f64, f64) -> f64) -> Field {
    art::tile_fn(L, L, height)
}
