//! The release of a DWG file and the logical sections it is made of.

use super::{r2004, r2007};

/// A DWG release this reader knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Version {
    R13,
    R14,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl Version {
    /// The release a file's first six bytes name.
    pub(super) fn of(bytes: &[u8]) -> Result<Version, Bad> {
        let tag = bytes.get(..6).ok_or(Bad("the file is too short"))?;
        Ok(match tag {
            b"AC1012" => Version::R13,
            b"AC1014" => Version::R14,
            b"AC1015" => Version::R2000,
            b"AC1018" => Version::R2004,
            b"AC1021" => Version::R2007,
            b"AC1024" => Version::R2010,
            b"AC1027" => Version::R2013,
            b"AC1032" => Version::R2018,
            _ => {
                // Later releases so far keep the 2018 layout.
                let number = std::str::from_utf8(&tag[2..])
                    .ok()
                    .and_then(|n| n.parse::<u32>().ok());
                match number {
                    Some(n) if n > 1032 => Version::R2018,
                    _ => return Err(Bad("this DWG release is not supported")),
                }
            }
        })
    }
}

/// Why a file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Bad(pub(super) &'static str);

/// The parts of a file the importer reads.
pub(super) struct Sections {
    pub(super) version: Version,
    /// The maintenance release byte at 0x0B.
    pub(super) maintenance: u8,
    /// The header variables, from their sentinel on.
    pub(super) header: Vec<u8>,
    /// The class list, from its sentinel on.
    pub(super) classes: Vec<u8>,
    /// The object map.
    pub(super) handles: Vec<u8>,
    /// The bytes object map offsets point into.
    pub(super) objects: Vec<u8>,
    /// Added to every object map offset.
    pub(super) objects_base: usize,
}

fn u32_at(data: &[u8], at: usize) -> Result<usize, Bad> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or(Bad("the file header is damaged"))
}

/// Splits a file into its sections.
pub(super) fn sections(bytes: &[u8]) -> Result<Sections, Bad> {
    let version = Version::of(bytes)?;
    match version {
        Version::R13 | Version::R14 | Version::R2000 => flat(bytes, version),
        Version::R2007 => r2007::sections(bytes),
        _ => r2004::sections(bytes, version),
    }
}

/// R13 to R2000: section locator records at 0x15 give each part's place.
fn flat(bytes: &[u8], version: Version) -> Result<Sections, Bad> {
    let count = u32_at(bytes, 0x15)?;
    if !(3..=16).contains(&count) {
        return Err(Bad("the file header is damaged"));
    }
    let mut parts: [Option<(usize, usize)>; 3] = [None; 3];
    for i in 0..count {
        let at = 0x19 + i * 9;
        let number = *bytes.get(at).ok_or(Bad("the file header is damaged"))?;
        let seeker = u32_at(bytes, at + 1)?;
        let size = u32_at(bytes, at + 5)?;
        if let Some(slot) = parts.get_mut(usize::from(number)) {
            *slot = Some((seeker, size));
        }
    }
    let part = |n: usize| -> Result<Vec<u8>, Bad> {
        let (seeker, size) = parts[n].ok_or(Bad("a section the drawing needs is missing"))?;
        bytes
            .get(seeker..seeker.saturating_add(size))
            .map(<[u8]>::to_vec)
            .ok_or(Bad("a section runs past the end of the file"))
    };
    Ok(Sections {
        version,
        maintenance: bytes.get(0x0B).copied().unwrap_or(0),
        header: part(0)?,
        classes: part(1).unwrap_or_default(),
        handles: part(2)?,
        objects: bytes.to_vec(),
        objects_base: 0,
    })
}
