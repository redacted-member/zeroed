//! Binary kernel replies (`BLOB` in KERNEL_API.md): a little endian `u32`
//! header length, the JSON header listing its `sections`, then the payload
//! the sections index into, each starting on a four byte boundary.
//!
//! Also the compact forms of the large per cell sections the 3D view reads
//! after every model change: heights as `u16` steps above a base, and
//! colours as a palette with one entry number per cell.

use std::collections::HashMap;

use serde_json::{json, Value};

/// One payload section: its name, element type and little endian bytes.
#[derive(Debug, Clone)]
pub struct Section {
    name: &'static str,
    dtype: &'static str,
    count: usize,
    bytes: Vec<u8>,
}

impl Section {
    /// A section of `f32` values.
    pub fn f32(name: &'static str, values: &[f32]) -> Section {
        Section {
            name,
            dtype: "f32",
            count: values.len(),
            bytes: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
        }
    }

    /// A section of `u16` values.
    pub fn u16(name: &'static str, values: &[u16]) -> Section {
        Section {
            name,
            dtype: "u16",
            count: values.len(),
            bytes: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
        }
    }

    /// A section of bytes.
    pub fn u8(name: &'static str, values: Vec<u8>) -> Section {
        Section {
            name,
            dtype: "u8",
            count: values.len(),
            bytes: values,
        }
    }

    /// Element type as the header names it (`f32`, `u16` or `u8`).
    pub fn dtype(&self) -> &'static str {
        self.dtype
    }
}

/// Frames `header` (a JSON object; its `sections` entry is written here)
/// and the sections into the bytes of a `BLOB` reply after its tag.
pub fn frame(mut header: Value, sections: &[Section]) -> Vec<u8> {
    let mut entries = Vec::with_capacity(sections.len());
    let mut offset = 0usize;
    for section in sections {
        entries.push(json!({
            "name": section.name,
            "dtype": section.dtype,
            "offset": offset,
            "count": section.count,
        }));
        offset = (offset + section.bytes.len() + 3) & !3;
    }
    if let Some(object) = header.as_object_mut() {
        object.insert("sections".to_string(), Value::Array(entries));
    }
    let header = header.to_string().into_bytes();
    let mut out = Vec::with_capacity(4 + header.len() + offset);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    let payload = out.len();
    for section in sections {
        out.extend_from_slice(&section.bytes);
        let padded = (out.len() - payload + 3) & !3;
        out.resize(payload + padded, 0);
    }
    out
}

/// The `u16` code a height section uses for a NaN height (no material, or
/// a cell cut through).
pub const HEIGHT_NAN: u16 = u16::MAX;

/// The most rounding a height sent as `u16` may take, as a fraction of the
/// grid's cell size: a hundredth of a cell, far below what the grid shows.
pub const HEIGHT_TOLERANCE: f64 = 0.01;

/// Steps between the lowest and the highest height a `u16` section can
/// tell apart; the code above them is [`HEIGHT_NAN`].
const HEIGHT_STEPS: f64 = (HEIGHT_NAN - 1) as f64;

/// Heights sent as `u16` steps: code `q` is `base + q * step` mm, and
/// [`HEIGHT_NAN`] is NaN. Rounding moves a height by at most half a step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCode {
    /// Height of code 0, mm.
    pub base: f64,
    /// Height between neighbouring codes, mm (0 when every height is the same).
    pub step: f64,
}

impl HeightCode {
    /// The code for heights from `min` to `max` mm when rounding to it moves
    /// no height by more than `tolerance` mm; `None` when it would, so the
    /// heights go as `f32`.
    pub fn fit(min: f32, max: f32, tolerance: f64) -> Option<HeightCode> {
        let (min, max) = (f64::from(min), f64::from(max));
        if !min.is_finite() || !max.is_finite() || max < min {
            return None;
        }
        let step = (max - min) / HEIGHT_STEPS;
        (step * 0.5 <= tolerance).then_some(HeightCode { base: min, step })
    }

    /// The code for `value` mm.
    pub fn encode(&self, value: f32) -> u16 {
        if value.is_nan() {
            return HEIGHT_NAN;
        }
        if self.step <= 0.0 {
            return 0;
        }
        let q = ((f64::from(value) - self.base) / self.step).round();
        q.clamp(0.0, HEIGHT_STEPS) as u16
    }

    /// The height a code stands for, as the front end reads it back.
    pub fn decode(&self, code: u16) -> f32 {
        if code == HEIGHT_NAN {
            f32::NAN
        } else {
            (self.base + f64::from(code) * self.step) as f32
        }
    }
}

/// The lowest and highest heights among `values`, NaN ignored.
pub fn height_range<'a>(values: impl IntoIterator<Item = &'a f32>) -> Option<(f32, f32)> {
    values
        .into_iter()
        .filter(|v| !v.is_nan())
        .fold(None, |range, &v| match range {
            None => Some((v, v)),
            Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        })
}

/// A height section: `u16` codes under `code`, or plain `f32` without one.
pub fn heights(name: &'static str, values: &[f32], code: Option<&HeightCode>) -> Section {
    match code {
        Some(code) => {
            let codes: Vec<u16> = values.iter().map(|v| code.encode(*v)).collect();
            Section::u16(name, &codes)
        }
        None => Section::f32(name, values),
    }
}

/// Adds `heightBase` and `heightStep` to a header whose height sections
/// use `code`.
pub fn describe_heights(header: &mut Value, code: Option<&HeightCode>) {
    if let (Some(code), Some(object)) = (code, header.as_object_mut()) {
        object.insert("heightBase".to_string(), json!(code.base));
        object.insert("heightStep".to_string(), json!(code.step));
    }
}

/// Colours per cell (RGBA bytes, alpha 0 for none) as a `palette` section
/// of RGBA entries, entry 0 always clear, and an `index` section with the
/// entry of each cell: `u8` while the palette has at most 256 entries,
/// `u16` beyond. Entries are numbered in the order the cells first use them.
pub fn palette(rgba: &[u8]) -> (Section, Section) {
    let (palette, index, _) = tagged_palette(rgba, &[]);
    (palette, index)
}

/// [`palette`] with a tag per cell (missing tags count as 0): cells of one
/// colour but different tags get separate entries. Also answers the tag of
/// each entry.
pub fn tagged_palette(rgba: &[u8], tags: &[u16]) -> (Section, Section, Vec<u16>) {
    let mut entries: Vec<[u8; 4]> = vec![[0, 0, 0, 0]];
    let mut entry_tags: Vec<u16> = vec![0];
    let mut lookup: HashMap<([u8; 4], u16), u16> = HashMap::new();
    let mut index: Vec<u16> = Vec::with_capacity(rgba.len() / 4);
    let mut last: Option<(([u8; 4], u16), u16)> = None;
    for (cell, px) in rgba.chunks_exact(4).enumerate() {
        let colour = [px[0], px[1], px[2], px[3]];
        if colour[3] == 0 {
            index.push(0);
            continue;
        }
        let key = (colour, tags.get(cell).copied().unwrap_or(0));
        if let Some((seen, entry)) = last {
            if seen == key {
                index.push(entry);
                continue;
            }
        }
        let entry = match lookup.get(&key) {
            Some(entry) => *entry,
            None => match u16::try_from(entries.len()) {
                Ok(next) => {
                    entries.push(colour);
                    entry_tags.push(key.1);
                    lookup.insert(key, next);
                    next
                }
                // More colours than a u16 can number cannot come from the
                // components; any beyond share the last entry.
                Err(_) => u16::MAX,
            },
        };
        last = Some((key, entry));
        index.push(entry);
    }
    let palette = Section::u8("palette", entries.concat());
    let index = if entries.len() <= 256 {
        Section::u8("index", index.iter().map(|e| *e as u8).collect())
    } else {
        Section::u16("index", &index)
    };
    (palette, index, entry_tags)
}

/// Reads a `BLOB` reply body back: its header and the heights of the
/// section `name` as the front end decodes them (`f32`, or `u16` codes
/// under the header's `heightBase` and `heightStep`).
#[cfg(test)]
pub(crate) fn read_heights(bytes: &[u8], name: &str) -> (Value, Vec<f32>) {
    let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
    let payload = &bytes[4 + len..];
    let section = header["sections"]
        .as_array()
        .and_then(|all| all.iter().find(|s| s["name"] == json!(name)))
        .expect("the section");
    let offset = section["offset"].as_u64().expect("offset") as usize;
    let count = section["count"].as_u64().expect("count") as usize;
    let heights = if section["dtype"] == json!("u16") {
        let code = HeightCode {
            base: header["heightBase"].as_f64().expect("base"),
            step: header["heightStep"].as_f64().expect("step"),
        };
        payload[offset..offset + count * 2]
            .chunks_exact(2)
            .map(|b| code.decode(u16::from_le_bytes([b[0], b[1]])))
            .collect()
    } else {
        payload[offset..offset + count * 4]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    };
    (header, heights)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections_of(bytes: &[u8]) -> (Value, &[u8]) {
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + len]).expect("header");
        (header, &bytes[4 + len..])
    }

    #[test]
    fn sections_start_on_four_byte_boundaries() {
        let bytes = frame(
            json!({ "width": 3 }),
            &[
                Section::u8("a", vec![1, 2, 3]),
                Section::u16("b", &[7]),
                Section::f32("c", &[1.5]),
            ],
        );
        let (header, payload) = sections_of(&bytes);
        assert_eq!(header["width"], json!(3));
        let offsets: Vec<u64> = header["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .map(|s| s["offset"].as_u64().expect("offset"))
            .collect();
        assert_eq!(offsets, vec![0, 4, 8]);
        assert_eq!(&payload[0..3], &[1, 2, 3]);
        assert_eq!(u16::from_le_bytes([payload[4], payload[5]]), 7);
        assert_eq!(
            f32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]),
            1.5
        );
        assert_eq!(payload.len(), 12);
    }

    #[test]
    fn height_codes_round_trip_within_half_a_step() {
        let code = HeightCode::fit(-25.0, 3.0, 1.0).expect("fits");
        assert!((code.step - 28.0 / 65534.0).abs() < 1e-12);
        for i in 0..=10_000 {
            let v = -25.0 + 28.0 * (i as f32) / 10_000.0;
            let back = code.decode(code.encode(v));
            assert!(
                f64::from((back - v).abs()) <= code.step * 0.5 + 1e-6,
                "{v} came back as {back}"
            );
        }
        assert_eq!(code.encode(f32::NAN), HEIGHT_NAN);
        assert!(code.decode(HEIGHT_NAN).is_nan());
        assert_eq!(code.decode(code.encode(-25.0)), -25.0);
        assert_eq!(code.decode(code.encode(3.0)), 3.0);
    }

    #[test]
    fn a_flat_field_codes_exactly_and_a_coarse_one_stays_f32() {
        let flat = HeightCode::fit(2.5, 2.5, 1e-6).expect("fits");
        assert_eq!(flat.decode(flat.encode(2.5)), 2.5);
        // 1000 mm in 65534 steps is 7.6 um between codes.
        assert!(HeightCode::fit(0.0, 1000.0, 0.001).is_none());
        assert!(HeightCode::fit(0.0, 1000.0, 0.01).is_some());
        assert_eq!(
            heights("z", &[1.0, 2.0], None).dtype(),
            "f32",
            "no code, plain floats"
        );
    }

    #[test]
    fn height_codes_at_the_edges_of_f32() {
        // Every height missing: no range, the section stays f32.
        assert_eq!(height_range(&[f32::NAN, f32::NAN]), None);
        // An infinite height cannot be coded.
        let range = height_range(&[1.0, f32::INFINITY]).expect("a range");
        assert!(HeightCode::fit(range.0, range.1, 1.0).is_none());
        // The widest f32 range is far too coarse; a flat field at the
        // largest f32 codes exactly.
        assert!(HeightCode::fit(f32::MIN, f32::MAX, 1e30).is_none());
        let top = HeightCode::fit(f32::MAX, f32::MAX, 1e-6).expect("flat");
        assert_eq!(top.decode(top.encode(f32::MAX)), f32::MAX);
        assert_eq!(top.encode(f32::NAN), HEIGHT_NAN);
        // Far from zero the f32 grid is coarser than the code: heights come
        // back as the same f32.
        let far = HeightCode::fit(1.0e7, 1.0e7 + 64.0, 1.0).expect("fits");
        for v in [1.0e7_f32, 1.0e7 + 1.0, 1.0e7 + 63.0, 1.0e7 + 64.0] {
            assert_eq!(far.decode(far.encode(v)), v);
        }
        // Out of range values clamp to the ends rather than wrap into NaN.
        let code = HeightCode::fit(0.0, 10.0, 1.0).expect("fits");
        assert_eq!(code.encode(11.0), HEIGHT_NAN - 1);
        assert_eq!(code.encode(-1.0), 0);
    }

    #[test]
    fn colours_become_a_palette_and_one_entry_per_cell() {
        let rgba = [
            [0, 0, 0, 0],
            [255, 128, 0, 255],
            [255, 128, 0, 255],
            [9, 9, 9, 0],
            [10, 20, 30, 255],
            [255, 128, 0, 255],
        ]
        .concat();
        let (palette, index) = palette(&rgba);
        assert_eq!(
            palette.bytes,
            [[0, 0, 0, 0], [255, 128, 0, 255], [10, 20, 30, 255]].concat()
        );
        assert_eq!(index.dtype(), "u8");
        assert_eq!(index.bytes, vec![0, 1, 1, 0, 2, 1]);
    }

    #[test]
    fn many_colours_take_a_wide_index() {
        let rgba: Vec<u8> = (0..300u32)
            .flat_map(|i| [(i % 256) as u8, (i / 256) as u8, 0, 255])
            .collect();
        let (palette, index) = palette(&rgba);
        assert_eq!(palette.count, 301 * 4);
        assert_eq!(index.dtype(), "u16");
        assert_eq!(
            u16::from_le_bytes([index.bytes[598], index.bytes[599]]),
            300
        );
    }
}
