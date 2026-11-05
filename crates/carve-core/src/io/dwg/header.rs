//! The header variables, the class list and the object map.

use std::collections::HashMap;

use super::bits::{Bits, HandleRef};
use super::{Bad, Sections, Version};

/// Where an object's strings are read from.
pub(super) enum Strings<'a> {
    /// Among the other data (R2004 and before).
    Inline,
    /// In a separate string stream (R2007 on).
    Stream(Bits<'a>),
    /// Not read: R2007 on, where only the plain data is wanted.
    Skip,
}

/// Where an object's handles are read from.
pub(super) enum Handles<'a> {
    Inline,
    Stream(Bits<'a>),
    Skip,
}

/// Data read from up to three streams: plain data, strings and handles.
pub(super) struct Streams<'a> {
    pub(super) dat: Bits<'a>,
    pub(super) strs: Strings<'a>,
    pub(super) hdl: Handles<'a>,
}

impl<'a> Streams<'a> {
    pub(super) fn tv(&mut self) -> Result<String, Bad> {
        match &mut self.strs {
            Strings::Inline => self.dat.tv(),
            Strings::Stream(s) => s.tu(),
            Strings::Skip => Ok(String::new()),
        }
    }

    pub(super) fn h(&mut self) -> Result<HandleRef, Bad> {
        match &mut self.hdl {
            Handles::Inline => self.dat.h(),
            Handles::Stream(h) => h.h(),
            Handles::Skip => Ok(HandleRef { code: 0, value: 0 }),
        }
    }

    /// A colour with its optional names (header and table colours).
    pub(super) fn cmc(&mut self) -> Result<(), Bad> {
        self.dat.bs()?;
        if self.dat.version >= Version::R2004 {
            self.dat.bl()?;
            let flags = self.dat.rc()?;
            if flags & 1 != 0 {
                self.tv()?;
            }
            if flags & 2 != 0 {
                self.tv()?;
            }
        }
        Ok(())
    }
}

/// Finds the string stream of R2007 and later data whose plain data ends
/// at bit `end` of `bits`: the last bit says whether there is one, and the
/// size of the stream is stored just before that bit.
pub(super) fn string_stream<'a>(bits: &Bits<'a>, end: usize) -> Result<Option<Bits<'a>>, Bad> {
    if end == 0 || end > bits.end() {
        return Err(Bad("a string stream outside its object"));
    }
    let mut probe = bits.clone();
    probe.seek(end - 1)?;
    if !probe.b()? {
        return Ok(None);
    }
    let mut at = end
        .checked_sub(1 + 16)
        .ok_or(Bad("a string stream outside its object"))?;
    probe.seek(at)?;
    let mut size = usize::from(probe.rs()?);
    if size & 0x8000 != 0 {
        at = at
            .checked_sub(16)
            .ok_or(Bad("a string stream outside its object"))?;
        probe.seek(at)?;
        let high = usize::from(probe.rs()?);
        size = (size & 0x7FFF) | (high << 15);
    }
    let start = at
        .checked_sub(size)
        .ok_or(Bad("a string stream outside its object"))?;
    let mut strings = bits.clone();
    strings.seek(start)?;
    strings.limit(at);
    Ok(Some(strings))
}

/// The byte where a section's bit data starts: after the 16 byte
/// sentinel, the size and, in some releases, a second long.
fn data_start(sections: &Sections) -> usize {
    let extra = match sections.version {
        Version::R2010 | Version::R2013 => sections.maintenance > 3,
        v => v >= Version::R2018,
    };
    16 + 4 + if extra { 4 } else { 0 }
}

/// `$INSUNITS`, or 0 (unitless) when the header does not hold it or cannot
/// be read.
pub(super) fn insunits(sections: &Sections) -> i16 {
    read_insunits(sections).unwrap_or(0)
}

fn read_insunits(sections: &Sections) -> Result<i16, Bad> {
    let v = sections.version;
    if v < Version::R2000 {
        return Ok(0);
    }
    let start = data_start(sections);
    let data = sections
        .header
        .get(start..)
        .ok_or(Bad("the header is too short"))?;
    let split = v >= Version::R2007;
    let mut s = Streams {
        dat: Bits::new(data, v),
        strs: if split {
            Strings::Skip
        } else {
            Strings::Inline
        },
        hdl: if split {
            Handles::Skip
        } else {
            Handles::Inline
        },
    };
    if v >= Version::R2007 {
        s.dat.rl()?;
    }
    if v >= Version::R2013 {
        s.dat.bll()?;
    }
    for _ in 0..4 {
        s.dat.bd()?;
    }
    for _ in 0..4 {
        s.tv()?;
    }
    s.dat.bl()?;
    s.dat.bl()?;
    if v < Version::R2004 {
        s.h()?;
    }
    // DIMASO, DIMSHO, then PLINEGEN to LIMCHECK.
    for _ in 0..9 {
        s.dat.b()?;
    }
    if v >= Version::R2004 {
        s.dat.b()?;
    }
    // USRTIMER to SPLFRAME, MIRRTEXT, WORLDVIEW, TILEMODE to VISRETAIN,
    // DISPSILH, PELLIPSE.
    for _ in 0..11 {
        s.dat.b()?;
    }
    // PROXYGRAPHICS, TREEDEPTH, LUNITS, LUPREC, AUNITS, AUPREC, ATTMODE,
    // PDMODE.
    for _ in 0..8 {
        s.dat.bs()?;
    }
    if v >= Version::R2004 {
        for _ in 0..3 {
            s.dat.bl()?;
        }
    }
    // USERI1 to TEXTQLTY.
    for _ in 0..19 {
        s.dat.bs()?;
    }
    // LTSCALE to CELTSCALE.
    for _ in 0..21 {
        s.dat.bd()?;
    }
    if v <= Version::R2004 {
        s.tv()?;
    }
    for _ in 0..4 {
        s.dat.bl()?;
    }
    if v >= Version::R2004 {
        for _ in 0..3 {
            s.dat.bl()?;
        }
    }
    for _ in 0..4 {
        s.dat.bl()?;
    }
    s.cmc()?;
    // HANDSEED sits in the plain data even when handles are apart.
    s.dat.h()?;
    for _ in 0..3 {
        s.h()?;
    }
    if v >= Version::R2007 {
        s.h()?;
    }
    s.h()?;
    s.h()?;
    s.dat.bd()?;
    // Paper space, then model space settings.
    for _ in 0..2 {
        for _ in 0..3 {
            s.dat.bd3()?;
        }
        s.dat.rd2()?;
        s.dat.rd2()?;
        s.dat.bd()?;
        for _ in 0..3 {
            s.dat.bd3()?;
        }
        s.h()?;
        s.h()?;
        s.dat.bs()?;
        s.h()?;
        for _ in 0..6 {
            s.dat.bd3()?;
        }
    }
    s.tv()?;
    s.tv()?;
    for _ in 0..9 {
        s.dat.bd()?;
    }
    if v >= Version::R2007 {
        s.dat.bd()?;
        s.dat.bd()?;
        s.dat.bs()?;
        s.cmc()?;
    }
    for _ in 0..6 {
        s.dat.b()?;
    }
    for _ in 0..3 {
        s.dat.bs()?;
    }
    if v >= Version::R2007 {
        s.dat.bs()?;
    }
    for _ in 0..8 {
        s.dat.bd()?;
    }
    s.dat.bd()?;
    s.dat.b()?;
    s.dat.bs()?;
    for _ in 0..4 {
        s.dat.b()?;
    }
    for _ in 0..3 {
        s.cmc()?;
    }
    for _ in 0..11 {
        s.dat.bs()?;
    }
    s.dat.b()?;
    s.dat.b()?;
    for _ in 0..4 {
        s.dat.bs()?;
    }
    s.dat.b()?;
    s.dat.bs()?;
    if v >= Version::R2007 {
        s.dat.b()?;
    }
    if v >= Version::R2010 {
        s.dat.b()?;
        s.dat.bd()?;
        s.tv()?;
        s.dat.bd()?;
        s.tv()?;
    }
    for _ in 0..5 {
        s.h()?;
    }
    if v >= Version::R2007 {
        for _ in 0..3 {
            s.h()?;
        }
    }
    s.dat.bs()?;
    s.dat.bs()?;
    for _ in 0..9 {
        s.h()?;
    }
    if v <= Version::R2000 {
        s.h()?;
    }
    for _ in 0..3 {
        s.h()?;
    }
    s.dat.bs()?;
    s.dat.bs()?;
    s.tv()?;
    s.tv()?;
    for _ in 0..3 {
        s.h()?;
    }
    if v >= Version::R2004 {
        s.h()?;
        s.h()?;
    }
    if v >= Version::R2007 {
        s.h()?;
    }
    if v >= Version::R2013 {
        s.h()?;
    }
    s.dat.bl()?;
    s.dat.bs()
}

/// Object type numbers of the classes the drawing defines, with their DXF
/// names. A class list that cannot be read is left empty: the types it
/// names are then skipped.
pub(super) fn classes(sections: &Sections) -> HashMap<u16, String> {
    read_classes(sections).unwrap_or_default()
}

fn read_classes(sections: &Sections) -> Result<HashMap<u16, String>, Bad> {
    let v = sections.version;
    let data = &sections.classes;
    let size = data
        .get(16..20)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or(Bad("the class list is too short"))?;
    let start = data_start(sections);
    let mut bits = Bits::slice(data, start, data.len(), v);
    let mut out = HashMap::new();
    let mut strings = Strings::Inline;
    let end;
    if v >= Version::R2007 {
        let total = bits.rl()? as usize;
        let s = string_stream(&bits, total)?.ok_or(Bad("the class names are missing"))?;
        // The class data stops where the strings start.
        end = s.position();
        strings = Strings::Stream(s);
        bits.limit(end);
    } else {
        end = size.saturating_mul(8);
        bits.limit(end);
    }
    if v >= Version::R2004 {
        bits.bs()?;
        bits.rc()?;
        bits.rc()?;
        bits.b()?;
    }
    let mut s = Streams {
        dat: bits,
        strs: strings,
        hdl: Handles::Skip,
    };
    // Each class takes at least a few dozen bits; stop when too few are left.
    while s.dat.position() + 40 < end {
        let number = s.dat.bs()? as u16;
        s.dat.bs()?;
        s.tv()?;
        s.tv()?;
        let name = s.tv()?;
        s.dat.b()?;
        s.dat.bs()?;
        if v >= Version::R2004 {
            s.dat.bl()?;
            s.dat.bs()?;
            s.dat.bs()?;
            s.dat.bl()?;
            s.dat.bl()?;
        }
        out.insert(number, name);
    }
    Ok(out)
}

/// The object map: every object's handle and where its data starts, in
/// handle order.
pub(super) fn object_map(sections: &Sections) -> Result<Vec<(u64, usize)>, Bad> {
    let data = &sections.handles;
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 2 <= data.len() {
        // Each section starts counting from zero again.
        let mut handle = 0u64;
        let mut place = 0i64;
        let size = usize::from(u16::from_be_bytes([data[at], data[at + 1]]));
        if size <= 2 {
            break;
        }
        let body = data
            .get(at + 2..at + size)
            .ok_or(Bad("the object map is damaged"))?;
        let mut bits = Bits::new(body, sections.version);
        while bits.remaining() >= 16 {
            handle = handle.wrapping_add(bits.umc()?);
            place = place.wrapping_add(bits.mc()?);
            if let Ok(offset) = usize::try_from(place) {
                out.push((handle, offset));
            }
        }
        at += size + 2;
    }
    if out.is_empty() {
        return Err(Bad("the drawing has no objects"));
    }
    out.sort_unstable_by_key(|(h, _)| *h);
    out.dedup_by_key(|(h, _)| *h);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_map_sections_add_up_deltas() {
        // One section: size 2 + 4 bytes, two entries, CRC, then the
        // empty closing section.
        let handles = vec![
            0x00, 0x06, 0x01, 0x10, 0x02, 0x05, 0xAA, 0xBB, 0x00, 0x02, 0, 0,
        ];
        let sections = Sections {
            version: Version::R2000,
            maintenance: 0,
            header: Vec::new(),
            classes: Vec::new(),
            handles,
            objects: Vec::new(),
            objects_base: 0,
        };
        assert_eq!(object_map(&sections).expect("map"), vec![(1, 16), (3, 21)]);
    }
}
