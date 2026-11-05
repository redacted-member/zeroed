//! The paged file layout of R2004, also used by R2010, R2013 and R2018.
//!
//! After a masked header at 0x80 the file is a run of pages. Two system
//! pages, the page map and the section map, say where every page is and
//! which logical section (header, classes, object map, objects) each one
//! belongs to. Data pages carry a masked 32 byte header and are compressed
//! with the release's LZ77 variant, undone by [`decompress`].

use std::collections::HashMap;

use super::{Bad, Sections, Version};

/// The largest logical section this reader rebuilds.
const MAX_SECTION: usize = 1 << 30;
/// The largest page: real files use 0x7400 bytes.
const MAX_PAGE: usize = 1 << 24;

/// The most a section of a file of `len` bytes can hold: the compression
/// can shrink data a great deal, but not without end.
pub(super) fn most(len: usize) -> usize {
    len.saturating_mul(64)
        .saturating_add(1 << 20)
        .min(MAX_SECTION)
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, Bad> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Bad("the file ended inside a page header"))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64, Bad> {
    Ok(u64::from(u32_at(data, at)?) | (u64::from(u32_at(data, at + 4)?) << 32))
}

/// The byte sequence the header at 0x80 is masked with.
fn header_mask() -> [u8; 0x6C] {
    let mut out = [0u8; 0x6C];
    let mut seed: u32 = 1;
    for byte in &mut out {
        seed = seed.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        *byte = (seed >> 16) as u8;
    }
    out
}

/// Undoes the R2004 compression of `input` into at most `size` bytes.
pub(super) fn decompress(input: &[u8], size: usize) -> Result<Vec<u8>, Bad> {
    let mut out: Vec<u8> = Vec::with_capacity(size.min(input.len().saturating_mul(8)));
    let mut src = Source { data: input, at: 0 };
    let mut literal = src.literal_length()?;
    src.copy_literal(&mut out, literal, size)?;
    loop {
        let op = src.byte()?;
        let (length, offset);
        match op {
            0x10 => {
                length = src.long_length()? + 9;
                let (o, lit) = src.two_byte_offset()?;
                offset = o + 0x3FFF;
                literal = lit;
            }
            0x11 => break,
            0x12..=0x1F => {
                length = usize::from(op & 0x0F) + 2;
                let (o, lit) = src.two_byte_offset()?;
                offset = o + 0x3FFF;
                literal = lit;
            }
            0x20 => {
                length = src.long_length()? + 0x21;
                let (o, lit) = src.two_byte_offset()?;
                offset = o;
                literal = lit;
            }
            0x21..=0x3F => {
                length = usize::from(op) - 0x1E;
                let (o, lit) = src.two_byte_offset()?;
                offset = o;
                literal = lit;
            }
            0x40..=0xFF => {
                length = usize::from(op >> 4) - 1;
                let op2 = src.byte()?;
                offset = (usize::from(op2) << 2) | usize::from((op & 0x0C) >> 2);
                literal = usize::from(op & 0x03);
            }
            _ => return Err(Bad("a compression code that is never written")),
        }
        // Offsets count back from the byte before the current one.
        let from = out
            .len()
            .checked_sub(offset + 1)
            .ok_or(Bad("compressed data that points before its start"))?;
        if out.len() + length > size {
            return Err(Bad("compressed data larger than its section"));
        }
        for i in 0..length {
            let byte = out[from + i];
            out.push(byte);
        }
        if literal == 0 {
            literal = src.literal_length()?;
        }
        src.copy_literal(&mut out, literal, size)?;
    }
    Ok(out)
}

struct Source<'a> {
    data: &'a [u8],
    at: usize,
}

impl Source<'_> {
    fn byte(&mut self) -> Result<u8, Bad> {
        let byte = *self
            .data
            .get(self.at)
            .ok_or(Bad("compressed data ended early"))?;
        self.at += 1;
        Ok(byte)
    }

    fn peek(&self) -> Result<u8, Bad> {
        self.data
            .get(self.at)
            .copied()
            .ok_or(Bad("compressed data ended early"))
    }

    /// A literal run length; a byte with its high nibble set is the next
    /// opcode instead, meaning no literal bytes.
    fn literal_length(&mut self) -> Result<usize, Bad> {
        let first = self.peek()?;
        if first & 0xF0 != 0 {
            return Ok(0);
        }
        self.at += 1;
        if first != 0 {
            return Ok(usize::from(first) + 3);
        }
        let mut total = 0x0Fusize;
        loop {
            let byte = self.byte()?;
            if byte == 0 {
                total += 0xFF;
            } else {
                return Ok(total + usize::from(byte) + 3);
            }
        }
    }

    fn long_length(&mut self) -> Result<usize, Bad> {
        let first = self.byte()?;
        if first != 0 {
            return Ok(usize::from(first));
        }
        let mut total = 0xFFusize;
        loop {
            let byte = self.byte()?;
            if byte == 0 {
                total += 0xFF;
            } else {
                return Ok(total + usize::from(byte));
            }
        }
    }

    fn two_byte_offset(&mut self) -> Result<(usize, usize), Bad> {
        let first = self.byte()?;
        let second = self.byte()?;
        let offset = usize::from(first >> 2) | (usize::from(second) << 6);
        Ok((offset, usize::from(first & 0x03)))
    }

    fn copy_literal(&mut self, out: &mut Vec<u8>, n: usize, size: usize) -> Result<(), Bad> {
        let bytes = self
            .data
            .get(self.at..self.at + n)
            .ok_or(Bad("compressed data ended early"))?;
        if out.len() + n > size {
            return Err(Bad("compressed data larger than its section"));
        }
        out.extend_from_slice(bytes);
        self.at += n;
        Ok(())
    }
}

/// A page as the page map places it.
#[derive(Clone, Copy)]
struct Page {
    address: usize,
}

/// Reads a system page (page map or section map) at `address`.
fn system_page(file: &[u8], address: usize, kind: u32) -> Result<Vec<u8>, Bad> {
    if u32_at(file, address)? != kind {
        return Err(Bad("a system page of the wrong kind"));
    }
    let size = u32_at(file, address + 4)? as usize;
    let compressed = u32_at(file, address + 8)? as usize;
    let method = u32_at(file, address + 12)?;
    let start = address + 0x14;
    let data = file
        .get(start..start.saturating_add(compressed))
        .ok_or(Bad("a system page runs past the end of the file"))?;
    if size > most(file.len()) {
        return Err(Bad("a system page larger than the file could hold"));
    }
    if method == 2 {
        decompress(data, size)
    } else {
        Ok(data.to_vec())
    }
}

/// One logical section and its pages.
struct Section {
    size: usize,
    page_size: usize,
    compressed: bool,
    pages: Vec<(u32, usize)>,
}

/// Splits an R2004 style file into its logical sections.
pub(super) fn sections(file: &[u8], version: Version) -> Result<Sections, Bad> {
    let raw = file
        .get(0x80..0x80 + 0x6C)
        .ok_or(Bad("the file is too short for its header"))?;
    let header: Vec<u8> = raw.iter().zip(header_mask()).map(|(b, m)| b ^ m).collect();
    if &header[..11] != b"AcFssFcAJMB" {
        return Err(Bad("the file header is damaged"));
    }
    let page_map_address = (u64_at(&header, 0x54)? as usize).saturating_add(0x100);
    let section_map_id = u32_at(&header, 0x5C)?;

    let map = system_page(file, page_map_address, 0x4163_0E3B)?;
    let mut pages: HashMap<u32, Page> = HashMap::new();
    let mut address = 0x100usize;
    let mut at = 0;
    while at + 8 <= map.len() {
        let number = u32_at(&map, at)? as i32;
        let size = u32_at(&map, at + 4)? as usize;
        at += 8;
        if number < 0 {
            at += 16;
        } else {
            pages.insert(number as u32, Page { address });
        }
        address = address.saturating_add(size);
    }

    let section_map_page = pages
        .get(&section_map_id)
        .ok_or(Bad("the section map is missing"))?;
    let info = system_page(file, section_map_page.address, 0x4163_003B)?;
    let count = u32_at(&info, 0)? as usize;
    let mut named: HashMap<String, Section> = HashMap::new();
    let mut at = 0x14;
    for _ in 0..count {
        let size = u64_at(&info, at)? as usize;
        let page_count = u32_at(&info, at + 8)? as usize;
        let page_size = u32_at(&info, at + 12)? as usize;
        let compressed = u32_at(&info, at + 20)? == 2;
        let name_bytes = info
            .get(at + 32..at + 96)
            .ok_or(Bad("the section map ended early"))?;
        let name: String = name_bytes
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| char::from(*b))
            .collect();
        at += 96;
        if page_count > info.len() / 16 {
            return Err(Bad("the section map lists more pages than it holds"));
        }
        let mut list = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            let number = u32_at(&info, at)?;
            let offset = u64_at(&info, at + 8)? as usize;
            list.push((number, offset));
            at += 16;
        }
        named.insert(
            name,
            Section {
                size,
                page_size,
                compressed,
                pages: list,
            },
        );
    }

    let read = |name: &str| -> Result<Vec<u8>, Bad> {
        let section = named
            .get(name)
            .ok_or(Bad("a section the drawing needs is missing"))?;
        assemble(file, &pages, section)
    };

    let maintenance = file.get(0x0B).copied().unwrap_or(0);
    Ok(Sections {
        version,
        maintenance,
        header: read("AcDb:Header")?,
        classes: read("AcDb:Classes").unwrap_or_default(),
        handles: read("AcDb:Handles")?,
        objects: read("AcDb:AcDbObjects")?,
        objects_base: 0,
    })
}

/// The most work (bytes decoded plus bytes placed) rebuilding a section
/// may take. A real file decodes each page once and places each byte once;
/// a damaged section map can list one page many times, and without this
/// bound each listing would cost up to a whole page again.
fn work_bound(section: &Section) -> usize {
    section
        .size
        .saturating_mul(2)
        .saturating_add(section.page_size.saturating_mul(2))
        .saturating_add(1 << 16)
}

/// Rebuilds a logical section from its pages. Each page is decoded once
/// however often the section map lists it, and the whole rebuild stops
/// with an error past [`work_bound`].
fn assemble(file: &[u8], pages: &HashMap<u32, Page>, section: &Section) -> Result<Vec<u8>, Bad> {
    Ok(assemble_counted(file, pages, section)?.0)
}

/// [`assemble`], also answering how many pages it decoded and the work it
/// took.
fn assemble_counted(
    file: &[u8],
    pages: &HashMap<u32, Page>,
    section: &Section,
) -> Result<(Vec<u8>, usize, usize), Bad> {
    if section.size > most(file.len()) || section.page_size > MAX_PAGE {
        return Err(Bad("a section larger than the file could hold"));
    }
    let bound = work_bound(section);
    let mut work = 0usize;
    let mut decoded: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut out = vec![0u8; section.size];
    for &(number, offset) in &section.pages {
        let page = pages
            .get(&number)
            .ok_or(Bad("a page the section map lists is missing"))?;
        if offset > out.len() {
            return Err(Bad("a page placed past the end of its section"));
        }
        let bytes = match decoded.entry(page.address) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let bytes = data_page(file, page.address, section)?;
                work = work.saturating_add(bytes.len());
                entry.insert(bytes)
            }
        };
        let room = (out.len() - offset).min(section.page_size.max(1));
        let n = bytes.len().min(room);
        work = work.saturating_add(n);
        if work > bound {
            return Err(Bad("the section map places its pages over and over"));
        }
        out[offset..offset + n].copy_from_slice(&bytes[..n]);
    }
    Ok((out, decoded.len(), work))
}

/// Unmasks the data page at `address` and answers its data.
fn data_page(file: &[u8], address: usize, section: &Section) -> Result<Vec<u8>, Bad> {
    let raw = file
        .get(address..address.saturating_add(32))
        .ok_or(Bad("a page runs past the end of the file"))?;
    let mask = 0x4164_536B_u32 ^ (address as u32);
    let mut head = [0u8; 32];
    for (i, chunk) in raw.chunks(4).enumerate() {
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ mask;
        head[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    if u32_at(&head, 0)? != 0x4163_043B {
        return Err(Bad("a data page header is damaged"));
    }
    let stored = u32_at(&head, 8)? as usize;
    let start = address + 32;
    let data = file
        .get(start..start.saturating_add(stored))
        .ok_or(Bad("a page runs past the end of the file"))?;
    if section.compressed {
        decompress(data, section.page_size.max(1))
    } else {
        Ok(data[..data.len().min(section.page_size.max(1))].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_mask_starts_as_published() {
        let mask = header_mask();
        assert_eq!(&mask[..4], &[0x29, 0x23, 0xBE, 0x84]);
        assert_eq!(&mask[0x0C..0x10], &[0xF1, 0xBB, 0xE9, 0xEB]);
    }

    #[test]
    fn literal_runs_and_back_references() {
        // A literal length of 2 means 5 bytes, then the end code.
        let data = [0x02, b'a', b'b', b'c', b'd', b'e', 0x11];
        assert_eq!(decompress(&data, 64).expect("decodes"), b"abcde");
        // Four literals, then 0x21 copies 3 bytes from 4 back.
        let data = [0x01, b'a', b'b', b'c', b'd', 0x21, 0x0C, 0x00, 0x11];
        assert_eq!(decompress(&data, 64).expect("decodes"), b"abcdabc");
    }

    #[test]
    fn damaged_streams_are_errors() {
        // A back reference before the start.
        let data = [0x21, 0x40, 0x00, 0x11];
        assert!(decompress(&data, 64).is_err());
        // Ends without the end code.
        assert!(decompress(&[0x02, b'a'], 64).is_err());
        // Larger than declared.
        assert!(decompress(&[0x05, 1, 2, 3, 4, 5, 6, 7, 8, 0x11], 4).is_err());
    }

    /// A file of `count` compressed data pages from 0x100 on, page `k`
    /// holding five bytes of value `k`; answers the file and its page map.
    fn paged_file(count: u32) -> (Vec<u8>, HashMap<u32, Page>) {
        let mut file = vec![0u8; 0x100];
        let mut pages = HashMap::new();
        for k in 0..count {
            let address = file.len();
            let b = k as u8;
            let data = [0x02, b, b, b, b, b, 0x11];
            let mask = 0x4164_536B_u32 ^ (address as u32);
            let mut words = [0u32; 8];
            words[0] = 0x4163_043B;
            words[2] = data.len() as u32;
            for word in words {
                file.extend_from_slice(&(word ^ mask).to_le_bytes());
            }
            file.extend_from_slice(&data);
            pages.insert(k, Page { address });
        }
        (file, pages)
    }

    fn section(size: usize, pages: Vec<(u32, usize)>) -> Section {
        Section {
            size,
            page_size: 5,
            compressed: true,
            pages,
        }
    }

    #[test]
    fn pages_tile_their_section() {
        let (file, pages) = paged_file(3);
        let tiled = section(15, vec![(0, 0), (1, 5), (2, 10)]);
        let (out, decoded, work) = assemble_counted(&file, &pages, &tiled).expect("reads");
        assert_eq!(out, [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2]);
        assert_eq!(decoded, 3);
        assert!(work <= work_bound(&tiled));
    }

    #[test]
    fn a_page_listed_over_and_over_is_decoded_once_and_bounded() {
        let (file, pages) = paged_file(2);
        // Listed twice: decoded once, placed twice.
        let twice = section(10, vec![(1, 0), (1, 5)]);
        let (out, decoded, _) = assemble_counted(&file, &pages, &twice).expect("reads");
        assert_eq!(out, [1; 10]);
        assert_eq!(decoded, 1);
        // Listed a million times over the same place: stopped at the bound.
        let many = section(10, vec![(1, 0); 1_000_000]);
        let started = std::time::Instant::now();
        assert!(assemble_counted(&file, &pages, &many).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn damaged_page_lists_never_panic_and_stay_bounded() {
        let (file, pages) = paged_file(6);
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..2_000 {
            let size = (next() % 200) as usize;
            let count = (next() % 5_000) as usize;
            let list: Vec<(u32, usize)> = (0..count)
                .map(|_| ((next() % 8) as u32, (next() % 220) as usize))
                .collect();
            let damaged = Section {
                size,
                page_size: (next() % 12) as usize,
                compressed: next() % 4 != 0,
                pages: list,
            };
            let result = std::panic::catch_unwind(|| assemble_counted(&file, &pages, &damaged));
            let result = result.unwrap_or_else(|_| panic!("round {round} panicked"));
            if let Ok((out, decoded, work)) = result {
                assert_eq!(out.len(), size, "round {round}");
                assert!(decoded <= 6, "round {round}");
                assert!(work <= work_bound(&damaged), "round {round}");
            }
        }
    }
}
