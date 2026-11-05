//! The R2007 file layout.
//!
//! R2007 keeps the idea of pages and sections but stores every page with
//! Reed-Solomon error correction bytes (interleaved over the page's blocks)
//! and compresses with its own LZ77 variant. This reader does not correct
//! errors: it takes the data bytes of each block and leaves the parity.

use std::collections::HashMap;

use super::r2004::most;
use super::{Bad, Sections, Version};

/// The largest page: real files use at most 0xF800 bytes.
const MAX_PAGE: usize = 1 << 24;

/// Where the page map offsets count from.
const PAGE_BASE: usize = 0x480;

fn u64_at(data: &[u8], at: usize) -> Result<u64, Bad> {
    data.get(at..at + 8)
        .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        .ok_or(Bad("an R2007 table ended early"))
}

fn usize_at(data: &[u8], at: usize) -> Result<usize, Bad> {
    usize::try_from(u64_at(data, at)?).map_err(|_| Bad("an R2007 size out of range"))
}

/// The data bytes of `blocks` Reed-Solomon code words of which each holds
/// `k` data bytes, stored interleaved (byte `j` of block `b` at
/// `j * blocks + b`) or one block after another.
fn reed_solomon_data(
    raw: &[u8],
    blocks: usize,
    k: usize,
    interleaved: bool,
) -> Result<Vec<u8>, Bad> {
    if raw.len() < blocks.saturating_mul(255) {
        return Err(Bad("an R2007 page runs past the end of the file"));
    }
    let mut out = Vec::with_capacity(blocks * k);
    for b in 0..blocks {
        for j in 0..k {
            out.push(if interleaved {
                raw[j * blocks + b]
            } else {
                raw[b * k + j]
            });
        }
    }
    Ok(out)
}

/// How the literal copies of the R2007 compression lay out 1 to 32 bytes:
/// (size, source offset) pieces written in this order.
const LITERAL_ORDER: [&[(u8, u8)]; 33] = [
    &[],
    &[(1, 0)],
    &[(1, 1), (1, 0)],
    &[(1, 2), (1, 1), (1, 0)],
    &[(1, 0), (1, 1), (1, 2), (1, 3)],
    &[(1, 4), (4, 0)],
    &[(1, 5), (4, 1), (1, 0)],
    &[(2, 5), (4, 1), (1, 0)],
    &[(4, 0), (4, 4)],
    &[(1, 8), (8, 0)],
    &[(1, 9), (8, 1), (1, 0)],
    &[(2, 9), (8, 1), (1, 0)],
    &[(4, 8), (8, 0)],
    &[(1, 12), (4, 8), (8, 0)],
    &[(1, 13), (4, 9), (8, 1), (1, 0)],
    &[(2, 13), (4, 9), (8, 1), (1, 0)],
    &[(8, 8), (8, 0)],
    &[(8, 9), (1, 8), (8, 0)],
    &[(1, 17), (16, 1), (1, 0)],
    &[(3, 16), (16, 0)],
    &[(4, 16), (16, 0)],
    &[(1, 20), (4, 16), (16, 0)],
    &[(2, 20), (4, 16), (16, 0)],
    &[(3, 20), (4, 16), (16, 0)],
    &[(8, 16), (16, 0)],
    &[(8, 17), (1, 16), (16, 0)],
    &[(1, 25), (8, 17), (1, 16), (16, 0)],
    &[(2, 25), (8, 17), (1, 16), (16, 0)],
    &[(4, 24), (8, 16), (16, 0)],
    &[(1, 28), (4, 24), (8, 16), (16, 0)],
    &[(2, 28), (4, 24), (8, 16), (16, 0)],
    &[(1, 30), (4, 26), (8, 18), (16, 2), (2, 0)],
    &[(16, 16), (16, 0)],
];

/// Appends `n` (1 to 32) literal bytes from `src` in the stored order.
fn put_literal(out: &mut Vec<u8>, src: &[u8], n: usize) {
    if n == 1 {
        out.push(src[0]);
        return;
    }
    for &(size, at) in LITERAL_ORDER[n] {
        let at = usize::from(at);
        let size = usize::from(size);
        put_literal(out, &src[at..at + size], size);
    }
}

/// Undoes the R2007 compression of `input` into at most `size` bytes.
pub(super) fn decompress(input: &[u8], size: usize) -> Result<Vec<u8>, Bad> {
    let short = || Bad("compressed data ended early");
    let byte = |at: usize| input.get(at).copied().ok_or_else(short);
    let mut out: Vec<u8> = Vec::with_capacity(size.min(input.len().saturating_mul(8)));
    let end = input.len();
    let mut at = 0usize;
    let mut op = byte(0)?;
    at += 1;
    let mut length = 0usize;
    if op >> 4 == 2 {
        at = 3;
        length = usize::from(byte(2)? & 7);
    }
    while at < end {
        if length == 0 {
            length = usize::from(op) + 8;
            if length == 0x17 {
                let n = byte(at)?;
                at += 1;
                length += usize::from(n);
                if n == 0xFF {
                    loop {
                        let n = usize::from(byte(at)?) | (usize::from(byte(at + 1)?) << 8);
                        at += 2;
                        length += n;
                        if n != 0xFFFF {
                            break;
                        }
                    }
                }
            }
        }
        let literal = input.get(at..at + length).ok_or_else(short)?;
        if out.len() + length > size {
            return Err(Bad("compressed data larger than its section"));
        }
        let mut chunks = literal.chunks(32);
        for chunk in &mut chunks {
            put_literal(&mut out, chunk, chunk.len());
        }
        at += length;
        if at >= end {
            break;
        }
        op = byte(at)?;
        at += 1;
        let (mut offset, mut count) = instructions(input, &mut at, &mut op)?;
        loop {
            // An offset of 0 would copy the byte being written.
            if offset == 0 {
                return Err(Bad("compressed data that points at itself"));
            }
            let from = out
                .len()
                .checked_sub(offset)
                .ok_or(Bad("compressed data that points before its start"))?;
            if out.len() + count > size {
                return Err(Bad("compressed data larger than its section"));
            }
            for i in 0..count {
                let b = out[from + i];
                out.push(b);
            }
            length = usize::from(op & 7);
            if length != 0 || at >= end {
                break;
            }
            op = byte(at)?;
            at += 1;
            if op >> 4 == 0 {
                break;
            }
            if op >> 4 == 15 {
                op &= 15;
            }
            (offset, count) = instructions(input, &mut at, &mut op)?;
        }
    }
    Ok(out)
}

/// Reads the offset and length of a back reference that starts with `op`,
/// leaving the byte that follows it in `op`.
fn instructions(input: &[u8], at: &mut usize, op: &mut u8) -> Result<(usize, usize), Bad> {
    let mut next = || -> Result<usize, Bad> {
        let b = input
            .get(*at)
            .copied()
            .ok_or(Bad("compressed data ended early"))?;
        *at += 1;
        Ok(usize::from(b))
    };
    let code = usize::from(*op);
    let (offset, length);
    match code >> 4 {
        0 => {
            let mut len = (code & 0xF) + 0x13;
            let lo = next()?;
            let o = next()?;
            *op = o as u8;
            len += (o >> 3) & 0x10;
            offset = ((o & 0x78) << 5) + 1 + lo;
            length = len;
        }
        1 => {
            let len = (code & 0xF) + 3;
            let lo = next()?;
            let o = next()?;
            *op = o as u8;
            offset = ((o & 0xF8) << 5) + 1 + lo;
            length = len;
        }
        2 => {
            let mut off = next()?;
            off |= (next()? << 8) & 0xFF00;
            let mut len = code & 7;
            if code & 8 == 0 {
                let o = next()?;
                *op = o as u8;
                len += o & 0xF8;
            } else {
                off += 1;
                len += next()? << 3;
                let o = next()?;
                *op = o as u8;
                len += ((o & 0xF8) << 8) + 0x100;
            }
            offset = off;
            length = len;
        }
        _ => {
            let len = code >> 4;
            let lo = code & 15;
            let o = next()?;
            *op = o as u8;
            offset = ((o & 0xF8) << 1) + lo + 1;
            length = len;
        }
    }
    Ok((offset, length))
}

/// A system page: `compressed` bytes repeated `factor` times, Reed-Solomon
/// coded (239 data bytes a block, interleaved) at `address`.
fn system_page(
    file: &[u8],
    address: usize,
    compressed: usize,
    size: usize,
    factor: usize,
) -> Result<Vec<u8>, Bad> {
    if size > most(file.len()) || compressed > file.len() {
        return Err(Bad("an R2007 system page larger than the file could hold"));
    }
    let aligned = compressed.div_ceil(8) * 8;
    let blocks = (aligned.saturating_mul(factor.max(1))).div_ceil(239);
    let raw = file
        .get(address..)
        .ok_or(Bad("an R2007 page runs past the end of the file"))?;
    let data = reed_solomon_data(raw, blocks, 239, true)?;
    let data = data
        .get(..compressed)
        .ok_or(Bad("an R2007 system page is damaged"))?;
    if compressed < size {
        decompress(data, size)
    } else {
        Ok(data[..size].to_vec())
    }
}

struct Page {
    offset: usize,
    id: u64,
    size: usize,
    compressed: usize,
}

struct Section {
    size: usize,
    interleaved: bool,
    pages: Vec<Page>,
}

/// Splits an R2007 file into its logical sections.
pub(super) fn sections(file: &[u8]) -> Result<Sections, Bad> {
    let raw = file
        .get(0x80..0x80 + 0x3D8)
        .ok_or(Bad("the file is too short for its header"))?;
    let head = reed_solomon_data(raw, 3, 239, true)?;
    let length = i32::from_le_bytes([head[0x18], head[0x19], head[0x1A], head[0x1B]]);
    let header = if length < 0 {
        let n = length.unsigned_abs() as usize;
        head.get(0x20..0x20 + n)
            .ok_or(Bad("the file header is damaged"))?
            .to_vec()
    } else {
        let n = length as usize;
        let data = head
            .get(0x20..0x20 + n)
            .ok_or(Bad("the file header is damaged"))?;
        decompress(data, 0x110)?
    };
    if header.len() < 0x110 {
        return Err(Bad("the file header is damaged"));
    }

    let map = system_page(
        file,
        PAGE_BASE.saturating_add(usize_at(&header, 0x38)?),
        usize_at(&header, 0x50)?,
        usize_at(&header, 0x58)?,
        usize_at(&header, 0x18)?,
    )?;
    let mut pages: HashMap<u64, usize> = HashMap::new();
    let mut offset = 0usize;
    let mut at = 0;
    while at + 16 <= map.len() {
        let size = usize_at(&map, at)?;
        let id = u64_at(&map, at + 8)? as i64;
        pages.insert(id.unsigned_abs(), offset);
        offset = offset.saturating_add(size);
        at += 16;
    }

    let section_map_id = u64_at(&header, 0xC0)?;
    let section_map_at = *pages
        .get(&section_map_id)
        .ok_or(Bad("the section map is missing"))?;
    let info = system_page(
        file,
        PAGE_BASE.saturating_add(section_map_at),
        usize_at(&header, 0xB0)?,
        usize_at(&header, 0xC8)?,
        usize_at(&header, 0xD8)?,
    )?;

    let mut named: HashMap<String, Section> = HashMap::new();
    let mut at = 0;
    while at + 64 <= info.len() {
        let size = usize_at(&info, at)?;
        let name_len = usize_at(&info, at + 0x20)?;
        let encoding = u64_at(&info, at + 0x30)?;
        let page_count = usize_at(&info, at + 0x38)?;
        at += 0x40;
        let name_bytes = info
            .get(at..at.saturating_add(name_len))
            .ok_or(Bad("the section map is damaged"))?;
        let units: Vec<u16> = name_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        let name = String::from_utf16_lossy(&units);
        at += name_len;
        if page_count > info.len() / 56 {
            return Err(Bad("the section map lists more pages than it holds"));
        }
        let mut list = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            list.push(Page {
                offset: usize_at(&info, at)?,
                id: u64_at(&info, at + 16)?,
                size: usize_at(&info, at + 24)?,
                compressed: usize_at(&info, at + 32)?,
            });
            at += 56;
        }
        named.insert(
            name,
            Section {
                size,
                interleaved: encoding == 4,
                pages: list,
            },
        );
    }

    let read = |name: &str| -> Result<Vec<u8>, Bad> {
        let section = named
            .get(name)
            .ok_or(Bad("a section the drawing needs is missing"))?;
        if section.size > most(file.len()) {
            return Err(Bad("a section larger than the file could hold"));
        }
        let mut out = vec![0u8; section.size];
        for page in &section.pages {
            let address = pages
                .get(&page.id)
                .ok_or(Bad("a page the section map lists is missing"))?;
            let raw = file
                .get(PAGE_BASE.saturating_add(*address)..)
                .ok_or(Bad("an R2007 page runs past the end of the file"))?;
            if page.compressed > raw.len() || page.size > MAX_PAGE {
                return Err(Bad("an R2007 page larger than the file could hold"));
            }
            let aligned = page.compressed.div_ceil(8) * 8;
            let blocks = aligned.div_ceil(251);
            let data = reed_solomon_data(raw, blocks, 251, section.interleaved)?;
            let data = data
                .get(..page.compressed)
                .ok_or(Bad("an R2007 page is damaged"))?;
            let bytes = if page.compressed < page.size {
                decompress(data, page.size)?
            } else {
                data.to_vec()
            };
            if page.offset > out.len() {
                return Err(Bad("a page placed past the end of its section"));
            }
            let n = bytes.len().min(out.len() - page.offset);
            out[page.offset..page.offset + n].copy_from_slice(&bytes[..n]);
        }
        Ok(out)
    };

    Ok(Sections {
        version: Version::R2007,
        maintenance: file.get(0x0B).copied().unwrap_or(0),
        header: read("AcDb:Header")?,
        classes: read("AcDb:Classes").unwrap_or_default(),
        handles: read("AcDb:Handles")?,
        objects: read("AcDb:AcDbObjects")?,
        objects_base: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_come_in_their_stored_order() {
        let src: Vec<u8> = (0..32).collect();
        let mut out = Vec::new();
        put_literal(&mut out, &src[..3], 3);
        assert_eq!(out, vec![2, 1, 0]);
        out.clear();
        put_literal(&mut out, &src[..8], 8);
        assert_eq!(out, (0..8).collect::<Vec<u8>>());
        out.clear();
        put_literal(&mut out, &src, 32);
        let mut expect: Vec<u8> = (24..32).collect();
        expect.extend(16..24);
        expect.extend(8..16);
        expect.extend(0..8);
        assert_eq!(out, expect);
        for n in 1..=32 {
            let mut out = Vec::new();
            put_literal(&mut out, &src[..n], n);
            let mut sorted = out.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..n as u8).collect::<Vec<u8>>(), "{n}");
        }
    }

    #[test]
    fn interleaved_code_words_give_their_data_back() {
        // Two blocks of 255 bytes: block 0 holds 0s and block 1 holds 1s.
        let raw: Vec<u8> = (0..510).map(|i| (i % 2) as u8).collect();
        let data = reed_solomon_data(&raw, 2, 239, true).expect("data");
        assert_eq!(data.len(), 478);
        assert!(data[..239].iter().all(|b| *b == 0));
        assert!(data[239..].iter().all(|b| *b == 1));
        assert!(reed_solomon_data(&raw[..300], 2, 239, true).is_err());
    }

    #[test]
    fn damaged_streams_are_errors() {
        assert!(decompress(&[], 16).is_err());
        assert!(decompress(&[0x0F, 0xFF], 16).is_err());
        assert!(decompress(&[0x00, 1, 2, 3, 4, 5, 6, 7, 8, 0x30, 0x40], 4).is_err());
    }
}
