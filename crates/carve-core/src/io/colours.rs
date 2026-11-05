//! Colour images for colour tracing: the decoded RGB pixels, the reduced
//! palette Trace Bitmap shows and the mask of the colours picked to trace.
//!
//! A colour image is first reduced to at most [`BASE_COLOURS`] colours by
//! median cut. Asking for fewer colours merges the two closest palette
//! colours (weighted by how many pixels each covers) until the count fits,
//! so every reduced colour is a union of base colours and a pixel always
//! belongs to the reduced colour its nearest base colour went into.
//! Everything is deterministic: pixels are sampled on a fixed stride and
//! ties are broken by index.

use image::DynamicImage;
use serde::Serialize;

use crate::error::CoreError;
use crate::io::bitmap::Bitmap;

/// How many colours an image is reduced to before any merging.
pub const BASE_COLOURS: usize = 16;

/// Alpha below this counts as transparent, which is never traced.
const OPAQUE_FROM: u8 = 128;

/// At most this many pixels feed the median cut; the rest are only mapped.
const SAMPLE_PIXELS: usize = 65_536;

/// A channel spread above this marks a pixel as coloured rather than grey.
const GREY_SPREAD: u8 = 12;

/// An RGB image with optional alpha, row by row from the top left.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourImage {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Colour per pixel.
    pub rgb: Vec<[u8; 3]>,
    /// Opacity per pixel, `None` when every pixel is opaque.
    pub alpha: Option<Vec<u8>>,
}

/// One colour of a reduced palette.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteColour {
    /// The colour, 8 bit sRGB.
    pub rgb: [u8; 3],
    /// How many opaque pixels of the image it covers.
    pub pixels: u64,
}

/// A reduced palette: the colours shown, and which of them each base
/// colour went into.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    /// The reduced colours, most pixels first.
    pub colours: Vec<PaletteColour>,
    base: Vec<[f64; 3]>,
    group: Vec<usize>,
}

/// Decodes a PNG, JPEG, BMP or WebP image, keeping its colours.
///
/// # Errors
/// [`CoreError::Format`] when the bytes cannot be decoded.
pub fn decode(bytes: &[u8]) -> Result<ColourImage, CoreError> {
    let image = image::load_from_memory(bytes).map_err(|e| CoreError::format("image", e))?;
    Ok(from_image(&image))
}

fn from_image(image: &DynamicImage) -> ColourImage {
    let has_alpha = image.color().has_alpha();
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity(width as usize * height as usize);
    let mut alpha = has_alpha.then(|| Vec::with_capacity(rgb.capacity()));
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        rgb.push([r, g, b]);
        if let Some(out) = alpha.as_mut() {
            out.push(a);
        }
    }
    ColourImage {
        width,
        height,
        rgb,
        alpha,
    }
}

impl ColourImage {
    fn opaque(&self, at: usize) -> bool {
        self.alpha.as_ref().is_none_or(|a| a[at] >= OPAQUE_FROM)
    }

    /// True when a noticeable share of the opaque pixels (one in a hundred)
    /// is coloured rather than grey, which is when Trace Bitmap works by
    /// colour instead of by threshold.
    pub fn is_colour(&self) -> bool {
        let mut coloured = 0usize;
        let mut opaque = 0usize;
        for (at, [r, g, b]) in self.rgb.iter().enumerate() {
            if !self.opaque(at) {
                continue;
            }
            opaque += 1;
            let high = (*r).max(*g).max(*b);
            let low = (*r).min(*g).min(*b);
            if high - low > GREY_SPREAD {
                coloured += 1;
            }
        }
        opaque > 0 && coloured * 100 >= opaque
    }

    /// The image reduced to at most `count` colours (clamped to 2 to
    /// [`BASE_COLOURS`]).
    pub fn palette(&self, count: usize) -> Palette {
        let count = count.clamp(2, BASE_COLOURS);
        let opaque: Vec<usize> = (0..self.rgb.len()).filter(|&at| self.opaque(at)).collect();
        let stride = opaque.len().div_ceil(SAMPLE_PIXELS).max(1);
        let sample: Vec<[u8; 3]> = opaque
            .iter()
            .step_by(stride)
            .map(|&at| self.rgb[at])
            .collect();
        let base = median_cut(sample, BASE_COLOURS);
        let mut counts = vec![0u64; base.len()];
        for &at in &opaque {
            counts[nearest(&base, self.rgb[at])] += 1;
        }
        // Start with one group per base colour that covers any pixel.
        let mut groups: Vec<(Vec<usize>, [f64; 3], u64)> = base
            .iter()
            .enumerate()
            .filter(|(i, _)| counts[*i] > 0)
            .map(|(i, c)| (vec![i], *c, counts[i]))
            .collect();
        while groups.len() > count {
            let mut best = (0, 1, f64::INFINITY);
            for i in 0..groups.len() {
                for j in i + 1..groups.len() {
                    let d = distance(groups[i].1, groups[j].1);
                    if d < best.2 {
                        best = (i, j, d);
                    }
                }
            }
            let (i, j, _) = best;
            let (members, colour, pixels) = groups.remove(j);
            let target = &mut groups[i];
            let total = (target.2 + pixels).max(1) as f64;
            let weight = target.2 as f64;
            for (mean, add) in target.1.iter_mut().zip(colour) {
                *mean = (*mean * weight + add * pixels as f64) / total;
            }
            target.2 += pixels;
            target.0.extend(members);
        }
        groups.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.0.iter().min().cmp(&b.0.iter().min()))
        });
        let mut group = vec![0usize; base.len()];
        for (index, (members, _, _)) in groups.iter().enumerate() {
            for &m in members {
                group[m] = index;
            }
        }
        // Base colours without pixels join the reduced colour closest to them.
        for (i, colour) in base.iter().enumerate() {
            if counts[i] == 0 {
                let closest = groups
                    .iter()
                    .enumerate()
                    .min_by(|a, b| distance(a.1 .1, *colour).total_cmp(&distance(b.1 .1, *colour)))
                    .map_or(0, |(index, _)| index);
                group[i] = closest;
            }
        }
        let colours = groups
            .iter()
            .map(|(_, colour, pixels)| PaletteColour {
                rgb: colour.map(|v| v.round().clamp(0.0, 255.0) as u8),
                pixels: *pixels,
            })
            .collect();
        Palette {
            colours,
            base,
            group,
        }
    }

    /// The palette index of the pixel at column `col` and row `row` (from
    /// the top), or `None` outside the image or on a transparent pixel.
    pub fn index_at(&self, palette: &Palette, col: i64, row: i64) -> Option<usize> {
        if col < 0 || row < 0 || col >= i64::from(self.width) || row >= i64::from(self.height) {
            return None;
        }
        let at = row as usize * self.width as usize + col as usize;
        self.opaque(at).then(|| palette.index_of(self.rgb[at]))
    }

    /// The palette index of every pixel, row by row from the top left, with
    /// 255 for a transparent pixel.
    pub fn indices(&self, palette: &Palette) -> Vec<u8> {
        (0..self.rgb.len())
            .map(|at| {
                if self.opaque(at) {
                    u8::try_from(palette.index_of(self.rgb[at])).unwrap_or(u8::MAX)
                } else {
                    u8::MAX
                }
            })
            .collect()
    }

    /// A black and white bitmap for the tracer: black where a pixel belongs
    /// to one of the `trace` palette colours, white elsewhere. Transparent
    /// pixels stay transparent.
    pub fn mask(&self, palette: &Palette, trace: &[usize]) -> Bitmap {
        let mut chosen = vec![false; palette.colours.len()];
        for &index in trace {
            if let Some(slot) = chosen.get_mut(index) {
                *slot = true;
            }
        }
        let luma = self
            .rgb
            .iter()
            .map(|rgb| {
                if chosen[palette.index_of(*rgb)] {
                    0
                } else {
                    255
                }
            })
            .collect();
        Bitmap {
            width: self.width,
            height: self.height,
            luma,
            alpha: self.alpha.clone(),
        }
    }
}

impl Palette {
    /// The reduced colour a pixel colour belongs to.
    pub fn index_of(&self, rgb: [u8; 3]) -> usize {
        self.group
            .get(nearest(&self.base, rgb))
            .copied()
            .unwrap_or(0)
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn nearest(base: &[[f64; 3]], rgb: [u8; 3]) -> usize {
    let p = rgb.map(f64::from);
    let mut best = (0, f64::INFINITY);
    for (i, c) in base.iter().enumerate() {
        let d = distance(*c, p);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Median cut: splits the box with the widest channel range at its median
/// until there are `count` boxes or none can split, and answers each box's
/// mean colour.
fn median_cut(sample: Vec<[u8; 3]>, count: usize) -> Vec<[f64; 3]> {
    if sample.is_empty() {
        return vec![[0.0; 3]];
    }
    let mut boxes = vec![sample];
    while boxes.len() < count {
        let mut pick: Option<(usize, usize, u8)> = None;
        for (i, b) in boxes.iter().enumerate() {
            for channel in 0..3 {
                let (low, high) = b.iter().fold((255u8, 0u8), |(lo, hi), p| {
                    (lo.min(p[channel]), hi.max(p[channel]))
                });
                let range = high.saturating_sub(low);
                if range > 0 && pick.is_none_or(|(_, _, best)| range > best) {
                    pick = Some((i, channel, range));
                }
            }
        }
        let Some((i, channel, _)) = pick else {
            break;
        };
        let mut b = boxes.swap_remove(i);
        b.sort_unstable_by_key(|p| (p[channel], p[0], p[1], p[2]));
        let mut mid = b.len() / 2;
        // Never split between two equal values, so both halves differ.
        while mid > 0 && b[mid - 1][channel] == b[mid][channel] {
            mid -= 1;
        }
        if mid == 0 {
            mid = b.partition_point(|p| p[channel] == b[0][channel]);
        }
        let upper = b.split_off(mid);
        boxes.push(b);
        boxes.push(upper);
    }
    boxes.sort_by_key(|b| b.iter().min().copied());
    boxes
        .iter()
        .map(|b| {
            let mut sum = [0.0f64; 3];
            for p in b {
                for k in 0..3 {
                    sum[k] += f64::from(p[k]);
                }
            }
            sum.map(|s| s / b.len() as f64)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 3] = [220, 30, 30];
    const DARK_RED: [u8; 3] = [200, 20, 40];
    const BLUE: [u8; 3] = [20, 40, 220];
    const WHITE: [u8; 3] = [255, 255, 255];

    /// A 30 x 10 image: red, dark red and blue thirds on a white border row.
    fn stripes() -> ColourImage {
        let (width, height) = (30u32, 10u32);
        let mut rgb = Vec::new();
        for row in 0..height {
            for col in 0..width {
                rgb.push(if row == 0 {
                    WHITE
                } else if col < 10 {
                    RED
                } else if col < 20 {
                    DARK_RED
                } else {
                    BLUE
                });
            }
        }
        ColourImage {
            width,
            height,
            rgb,
            alpha: None,
        }
    }

    #[test]
    fn a_colour_image_keeps_its_distinct_colours() {
        let image = stripes();
        assert!(image.is_colour());
        let palette = image.palette(16);
        assert_eq!(palette.colours.len(), 4);
        let total: u64 = palette.colours.iter().map(|c| c.pixels).sum();
        assert_eq!(total, 300);
        assert_eq!(palette.colours[0].pixels, 90);
    }

    #[test]
    fn fewer_colours_merge_the_closest_ones() {
        let image = stripes();
        let palette = image.palette(3);
        assert_eq!(palette.colours.len(), 3);
        let red = palette.index_of(RED);
        assert_eq!(red, palette.index_of(DARK_RED));
        assert_ne!(red, palette.index_of(BLUE));
        assert_eq!(palette.colours[red].pixels, 180);
    }

    #[test]
    fn the_mask_is_black_on_the_traced_colours_only() {
        let image = stripes();
        let palette = image.palette(16);
        let blue = palette.index_of(BLUE);
        let mask = image.mask(&palette, &[blue]);
        assert_eq!(mask.luma.iter().filter(|v| **v == 0).count(), 90);
        assert_eq!(mask.luma[10 * 30 - 1], 0);
        assert_eq!(mask.luma[0], 255);
        assert_eq!(image.index_at(&palette, 25, 5), Some(blue));
        assert_eq!(image.index_at(&palette, 40, 5), None);
        let indices = image.indices(&palette);
        assert_eq!(indices.len(), 300);
        assert_eq!(usize::from(indices[35]), palette.index_of(RED));
    }

    #[test]
    fn a_grey_image_is_not_colour() {
        let image = ColourImage {
            width: 2,
            height: 1,
            rgb: vec![[10, 10, 12], [200, 200, 200]],
            alpha: None,
        };
        assert!(!image.is_colour());
        assert_eq!(image.palette(16).colours.len(), 2);
    }

    #[test]
    fn transparent_pixels_are_left_out() {
        let mut image = stripes();
        image.alpha = Some(
            (0..300)
                .map(|i| if i % 30 < 10 { 0 } else { 255 })
                .collect(),
        );
        let palette = image.palette(16);
        let total: u64 = palette.colours.iter().map(|c| c.pixels).sum();
        assert_eq!(total, 300 - 100);
        assert_eq!(image.index_at(&palette, 5, 5), None);
    }
}
